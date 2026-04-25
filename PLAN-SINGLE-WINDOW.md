# Plan: Single-window con child webviews

Branch: `feat/single-window-gtk-hack`

## Objetivo

Una sola ventana en el WM. Bar arriba (44px) + área de servicio activo abajo. Switch entre servicios = instantáneo, sin animación, sin nuevas ventanas. Cookies aisladas por servicio. Cross-platform Linux + Windows.

## Causa raíz del problema actual

- En **Linux GTK**, child webviews se meten todos en un `gtk::Box` vertical (creado por TAO en `tao-0.34.8/src/platform_impl/linux/window.rs:160`). El `GtkBox` ignora `position`/`size` y solo pack-startea con `expand=true` (ver `wry-0.54.4/src/webkitgtk/mod.rs:602`). Por eso `set_position`/`set_size` en child webviews no hace nada.
- En **Windows**, child webviews usan `build_as_child` que SÍ respeta `position`/`size` (HWND con SetWindowPos). No hay limitación.
- En **KDE Wayland**, top-level windows no pueden ser posicionadas por la app (xdg-shell rule). Por eso multi-WebviewWindow se ve roto.

→ Conclusión: Linux **necesita** child webviews + hack GTK. Windows **puede** elegir cualquiera de los dos enfoques.

## Arquitectura

### Common (ambos OS)

- 1 `WebviewWindow` "main" borderless
- Webview principal de main = SvelteKit con bar superior fijo (44px)
- Por servicio: 1 child webview agregado al main vía `Window::add_child(builder, pos, size)`
- Cada child webview tiene `data_directory` propio para aislar cookies
- Cada child webview tiene `initialization_script` con `bar.js` (no, **OJO**: ahora el bar lo hace el SVELTE main, no el bar.js inyectado. El inyectado solo lo necesitábamos cuando el servicio tenía su propia ventana sin acceso al Svelte. Volvemos al modelo Svelte-only para el bar)

### Linux path

- Child webview se agrega al `default_vbox()` con `expand=true` (default Wry)
- En setup, después de crear main, hack GTK:
  - Tomar `default_vbox()`
  - El primer child del vbox = webview Svelte. Cambiar packing a `expand=false, fill=false` con `set_size_request(-1, BAR_H)`
- Al agregar service webview: queda con `expand=true` (default). Inmediatamente `gtk_widget_hide()` si no es el activo
- Switch service:
  - `gtk_widget_hide()` en el actual
  - `gtk_widget_show()` en el nuevo
  - Solo uno visible a la vez → toma todo el espacio remanente
- Acceder al GTK widget de cada child webview:
  - Vía `Webview::with_webview(closure)` que da `PlatformWebview`
  - `PlatformWebview::inner()` → `webkit2gtk::WebView` (impl `IsA<gtk::Widget>`)
  - Llamar `widget.show()` / `widget.hide()`
- Crate dependency:
  - `[target.'cfg(target_os = "linux")'.dependencies] gtk = "0.18"`
  - Match exact con la versión que usa Tauri 2.10.x (verificar con `cargo tree`)

### Windows path

Dos opciones:

**A) Mismo enfoque que Linux** (child webviews dentro de main window)
- Wry en Windows usa `build_as_child` con HWND child
- `set_position(BAR_H)` y `set_size(window_w, window_h - BAR_H)` funcionan
- Switch via show/hide del HWND del webview
- Ventaja: código común con Linux, único `webview.rs`
- Desventaja: hay que mantener dos branches de hide/show (GTK widget vs WebView2 controller visibility)

**B) Multi-WebviewWindow** (lo que ya teníamos)
- En Windows funciona perfecto
- Pero requiere mantener dos arquitecturas paralelas

→ **Decisión**: opción A. Código unificado: child webviews + hide/show, encapsulado en módulo con cfg gates por OS para los detalles del hide/show.

### macOS

Postergado. Fuera de scope inicial. Cuando se ataque, opción A es portable (NSView visibility).

## Estructura de archivos

```
src-tauri/src/
  lib.rs                  # Entry, sin cambios estructurales
  services.rs             # Modelo + persistencia, sin tocar mucho
  webview/
    mod.rs                # API pública: ensure_mounted, switch, etc
    layout_linux.rs       # GTK packing hack + hide/show
    layout_windows.rs     # set_position/set_size + show/hide
    layout_other.rs       # fallback (panic o noop)
  tray.rs                 # Sin cambios
  bar.js                  # ELIMINAR (bar vuelve a ser puro Svelte en main)
```

## API interna unificada

```rust
pub fn ensure_mounted(app: &AppHandle, svc: &Service) -> Result<()>;
pub fn unmount(app: &AppHandle, svc_id: &str) -> Result<()>;
pub fn set_active(app: &AppHandle, svc_id: Option<&str>) -> Result<()>;
pub fn relayout(app: &AppHandle) -> Result<()>;  // recalcula bounds (Windows) o es noop (Linux)

mod platform {
    #[cfg(target_os = "linux")] pub use super::layout_linux::*;
    #[cfg(target_os = "windows")] pub use super::layout_windows::*;
    #[cfg(not(any(target_os = "linux", target_os = "windows")))] pub use super::layout_other::*;
}
```

Cada `layout_*.rs` exporta `setup_window`, `mount_child`, `show_child`, `hide_child`, `unmount_child`, `apply_bounds`.

## Flujo en runtime

1. App start → main window se crea (Svelte UI con bar). En setup:
   - Linux: `setup_window(window)` aplica packing del bar webview
   - Windows: `setup_window(window)` posiciona main webview a `(0,0,w,BAR_H)` y guarda referencia
2. Load services from disk → for each service: `ensure_mounted` (crea child webview)
   - Linux: vbox auto-pack con `expand=true`. `hide_child` inmediato si no es activo
   - Windows: `set_position(0, BAR_H)`, `set_size(w, h-BAR_H)`, `set_visible(false)` si no activo
3. User clic tab → `set_active(id)`:
   - `hide_child(prev_id)`
   - `show_child(id)`
4. Window resize:
   - Linux: noop (GtkBox layout maneja)
   - Windows: `relayout` → recalcula bounds y aplica al webview activo y al bar
5. Add service → ensure_mounted + opcionalmente set_active al recién agregado
6. Remove service → hide + unmount + si era activo, set_active(None)

## Bar UI (Svelte main)

Igual que ahora pero con cambios:
- Quitar `bar.js` de `src-tauri/src/`
- Quitar `initialization_script` de WebviewBuilder (no aplica más, el bar es Svelte main)
- Borrar el modelo de `WebviewWindow` por servicio en `webview.rs`
- Quitar `withGlobalTauri` de tauri.conf.json (no se inyecta nada en sites)
- Quitar `remote.urls` de capabilities (no hay invokes desde sites)

Beneficio extra: dejamos de tener problema con Trusted Types, CSP de sites, navigator spoof (sigue útil, lo movemos a script de inyección por servicio para WhatsApp), porque nuestro código JS solo corre en main webview que es local (tauri://localhost).

## Inyección por servicio (futuro, no MVP)

Para cosas como navigator spoof en WhatsApp, customJS, customCSS:
- Campo opcional `init_script` en `Service` struct
- Aplicarlo via `WebviewBuilder::initialization_script` por servicio en `mount_child`

## Pasos de implementación

1. **Limpieza**: borrar `bar.js`, refactor `webview.rs` para volver a child webviews
2. **layout_linux.rs**: setup_window con packing + mount_child + show/hide vía gtk
3. **layout_windows.rs**: setup_window posiciona main webview, mount_child con set_position/size, show/hide vía set_visible
4. **layout_other.rs**: stubs panicean (avisan no soportado)
5. **services.rs**: usa nueva API. set_active llama platform::set_active
6. **lib.rs**: setup llama platform::setup_window. Listener WindowEvent::Resized llama platform::relayout
7. **+page.svelte**: igual al estado actual menos imports inútiles
8. **capabilities/default.json**: ya no necesita "svc-*" ni remote
9. **tauri.conf.json**: quitar withGlobalTauri, mantener decorations:false, shadow:false. Quitar maximized:true (ya no necesario)
10. **Cargo.toml**: agregar gtk dep para Linux

## Verificación end-to-end

- [ ] Single window en KDE Wayland (sin ventanas extra)
- [ ] Bar superior fijo 44px
- [ ] Welcome cuando no hay activo
- [ ] Agregar WhatsApp → child webview se monta + show
- [ ] Switch a otra app → instantáneo, sin animación
- [ ] Resize ventana → bar y servicio activo se ajustan
- [ ] Agregar Gmail → no rompe el bar
- [ ] Cookies aisladas (login WSP no afecta Gmail)
- [ ] Remover servicio → desmonta correctamente
- [ ] Cerrar (X de bar) → main se oculta a tray
- [ ] Bar drag funciona

## Riesgos

| Riesgo | Mitigación |
|--------|------------|
| `gtk::Box::set_child_packing` API rompe en update Tauri/gtk-rs | Aislar en `layout_linux.rs`. Pinear gtk = "0.18" (igual que Tauri) |
| `default_vbox()` API quitada por Tauri | Capturar fallback con `app.get_window().gtk_window()` y navegar children manualmente |
| `Webview::with_webview` no encuentra widget | Mantener mapping `Service.id → gtk::Widget` en estado Rust |
| Windows: `set_position` falla en HiDPI | Multiplicar por scale_factor explícito |
| Polución de logs por iteración rápida | Quitar todos los `eprintln!` antes de merge |

## Criterio de merge a main

- Todo el checklist de "Verificación" pasa en KDE Wayland local
- `cargo check` y `pnpm check` limpios
- Sin warnings nuevos críticos
- Si Windows path no se prueba, dejar dummy con TODO claro y panic con mensaje útil; merge igual
