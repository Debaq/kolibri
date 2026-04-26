# Kolibri — Roadmap

Estado actual (branch `feat/single-window-gtk-hack`):

- Single window borderless en KDE Wayland, sin bordes visibles
- Bar superior 56px con tabs + picker inline + controles
- Child webviews aislados (cookies por servicio)
- Linux: GTK packing hack en `webview/layout_linux.rs`
- Windows: path con `set_position`/`set_size` (sin probar todavía)
- Tray icon, drag, min/max/close, hide-to-tray
- WhatsApp / Gmail / Outlook funcionando

Falta desde acá:

## Limpieza inmediata (antes de merge a main)

- [x] Borrar `src/lib/AddServiceModal.svelte` (ya no se usa, picker es inline)
- [x] Borrar `set_sidebar_collapsed` y `sidebar_collapsed` field en `PersistedState` (ya estaba limpio)
- [x] Quitar `eprintln!` debug (ya no quedaban)
- [ ] Probar resize de ventana (ahora que está maximizada por defecto, salir de maximizado y verificar)
- [x] Mergear branch a `main`

## UX corto plazo

- [x] **Iconos reales por servicio**: favicon vía `google.com/s2/favicons` con fallback a inicial sobre color
- [x] **Reordenar tabs** vía drag-and-drop HTML5 + comando `reorder_services`
- [x] **Atajos** (a nivel bar Svelte — solo cuando bar tiene foco):
  - `Ctrl+1..9` → switch a tab N
  - `Ctrl+T` → abrir picker
  - `Ctrl+W` → eliminar servicio actual
  - `Ctrl+0` / `Ctrl+H` → inicio
- [ ] **Atajos globales / desde child webview** (pendiente): requiere `tauri-plugin-global-shortcut` o init_script + IPC. Atajo global `Ctrl+Alt+K` show/hide
- [x] **Settings panel** real: tema dark/light, rename, cambiar URL, color, reordenar (botones up/down), referencia atajos
  - Pendiente: editar icono custom, autostart al iniciar sesión, configurar atajos
- [x] **Reload button** siempre visible cuando hay activo + comando `reload_service`. Detección formal de `load_failed` postergada (Wry no expone hook directo en Tauri 2)
- [x] **Spinner en tab activa** durante carga (`on_page_load` Started/Finished). Skeleton overlay sobre área del child webview no implementado: child webview es widget GTK nativo, tapa overlays del Svelte
- [x] **Botón Home (⌂)** ya está; verificar que esté claro cómo volver

## Personalización por servicio

Extender `Service` con campos opcionales y aplicarlos en `mount_child`:

| Campo | Para qué |
|-------|----------|
| `user_agent` | Override por sitio (ya existe a nivel default Chrome UA) |
| `init_script` | JS extra inyectado por servicio (ej: ocultar header duplicado del sitio, tweaks) |
| `custom_css` | CSS extra inyectado en el sitio |
| `zoom` | Zoom inicial del servicio |
| `auto_grant_notifications` | Conceder permiso Notification API automáticamente |
| `block_popups` | Bloquear popups |
| `mute` | Silenciar audio del servicio |

## Notificaciones / badge no-leídos

- [ ] Inyectar JS por servicio que escuche cambios en `document.title` (la mayoría usa `(N) Inbox` para no-leídos)
- [ ] Emitir evento Tauri `kolibri:unread` con `{service_id, count}`
- [ ] Sumar contador y mostrar:
  - Badge sobre el icono de la tab
  - Tooltip en el tray icon
  - Total combinado en algún lado del bar
- [ ] Notif nativa via `tauri-plugin-notification` cuando hay nuevo no-leído

## Multi-OS

- [ ] **Windows**: probar el path `layout_windows.rs`. Posibles ajustes: scale_factor en HiDPI, ajuste de `set_position` con padding del Windows chrome
- [ ] **macOS**: implementar `layout_macos.rs` cuando se necesite (postergado)

## Sistema / packaging

- [ ] Iconos de la app (32x32, 128x128, 128x128@2x, .icns, .ico) — los placeholders están en `src-tauri/icons/`
- [ ] Auto-update via Tauri updater (firma + endpoint)
- [ ] Empaquetado `.deb`, `.rpm`, `.AppImage`, `.msi`
- [ ] CI GitHub Actions: build cross-platform + release on tag
- [ ] README usuario final

## Datos / privacidad

- [ ] **Backup/restore**: export ZIP con `services.json` + `sessions/`
- [ ] **Modo invitado**: sesión efímera por servicio (no persiste cookies)
- [ ] **Limpiar cookies/cache** desde Settings, por servicio
- [ ] **Logout** un servicio sin perderlo de la lista

## Deuda técnica detectada (review 2026-04-25)

Backend Rust:

- [x] **`unwrap()` en todos los `Mutex::lock()`**: reemplazado por `expect()` informativo en `lib.rs`, `services.rs`, `webview/mod.rs`
- [x] **`tray.rs` `default_window_icon().unwrap()`**: fallback a icono embebido (`icons/32x32.png` vía `include_bytes!` + `Image::from_bytes`, feature `image-png`)
- [x] **ID por `SystemTime` en ms**: migrado a `uuid::Uuid::new_v4()` en `services::add_service`
- [x] **`emit_unread` spam al iniciar**: primer tick por servicio se marca como seed (HashSet en `UnreadState.seeded`) y no dispara notif nativa
- [x] **Favicon cache sin TTL**: TTL 7 días vía mtime; si fetch falla y existe cache stale, se reusa
- [x] **`looks_valid` rechaza <64 bytes**: umbral bajado a 16 bytes; sigue descartando HTML
- [x] **`data_dir_for` ignora fallo `create_dir_all`**: ahora propaga error con `?`
- [x] **`update_service` con `icon: Some("")` borra icono**: ahora `Some("")` se ignora; comandos `clear_service_icon` y `clear_service_color` separados para borrar
- [x] **HiDPI**: bar multiplica `BAR_HEIGHT` por `scale_factor()` en `layout_linux`

Frontend:

- [ ] **`+page.svelte` 992 líneas**: monolito. Romper en componentes (`Bar.svelte`, `Tabs.svelte`, `Settings.svelte`, `Picker.svelte`)
- [ ] **Cero tests**: agregar al menos tests unitarios de `services.rs` (add/remove/reorder) y `favicons.rs` (detect_mime, looks_valid)

## Riesgos / deudas técnicas

- **`gtk::Box::set_child_packing` API frágil**: si Tauri/gtk-rs hace breaking change, hay que adaptar `layout_linux.rs`
- **`default_vbox()`**: pin de la API; Tauri 3 puede cambiar arquitectura interna
- **Trusted Types**: si volvemos a inyectar bar JS en sites externos, usar APIs DOM seguras (sin `innerHTML`)
- **CSP estricta**: algunos sites pueden bloquear `init_script` de Wry. Hasta ahora no pasó pero mantenerlo en mente
- **WebKit2GTK 2.0.x es la versión usada por Wry 0.54**: cuando upgrade a WebKit 6 (GTK4), revalidar todo

## Ideas largas

- **Workspaces**: agrupar servicios (ej "Trabajo" = Slack+Gmail+Notion; "Personal" = WhatsApp+IG). Switch entre workspaces como switch entre layouts
- **Plugin API**: que terceros agreguen integraciones específicas por servicio (badge custom, atajos, scripts)
- **Sync metadata cross-device**: lista de servicios via WebDAV/Nextcloud (sin cookies)
- **Dark/light auto**: respetar tema del SO
- **Scratchpad**: tab especial con notas locales markdown
