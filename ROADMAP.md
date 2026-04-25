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

- [ ] Borrar `src/lib/AddServiceModal.svelte` (ya no se usa, picker es inline)
- [ ] Borrar `set_sidebar_collapsed` y `sidebar_collapsed` field en `PersistedState` (dead code)
- [ ] Quitar `eprintln!` debug si quedó alguno
- [ ] Probar resize de ventana (ahora que está maximizada por defecto, salir de maximizado y verificar)
- [ ] Mergear branch a `main`

## UX corto plazo

- [ ] **Iconos reales por servicio**: fetch del favicon o usar un set local. Hoy es la primer letra sobre un color del catálogo
- [ ] **Reordenar tabs** vía drag-and-drop (HTML5 drag o pointer events)
- [ ] **Atajos**:
  - `Ctrl+1..9` → switch a tab N
  - `Ctrl+T` → abrir picker
  - `Ctrl+W` → cerrar tab actual
  - Atajo global `Ctrl+Alt+K` → mostrar/ocultar app
- [ ] **Settings panel**: hoy `⚙` hace `alert`. Hacer panel real:
  - Renombrar servicios
  - Cambiar URL / icono / color
  - Reordenar
  - Tema
  - Atajos
  - Empezar al iniciar sesión
- [ ] **Reload button visible** cuando un servicio falla a cargar (detectar via `load_failed` event Wry)
- [ ] **Skeleton/spinner** durante la primera carga del servicio
- [ ] **Botón Home (⌂)** ya está; verificar que esté claro cómo volver

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
