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

- [x] Inyectar JS por servicio que escuche cambios en `document.title` (`webview/scripts.rs::unread_watcher_script`). Regex: `(N)`, `N `, `[N]`, `• N`, prefijo `*•!●` → 1 (Slack-like)
- [x] Emitir evento Tauri `kolibri:unread` con `{service_id, count, total}` (`lib.rs::emit_unread`)
- [x] Sumar contador y mostrar:
  - [x] Badge sobre el icono de la tab
  - [x] Tooltip en el tray icon
  - [x] Total combinado en bar (badge al lado del `+`)
- [x] Notif nativa via `tauri-plugin-notification` cuando hay nuevo no-leído (con seed anti-spam al cargar)
- [x] Inyección compartida Linux + Windows (`webview/scripts.rs::apply_common`)

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

## Optimización de RAM (branch `feat/ram-optimization`, PR #2 — 2026-04-26)

Objetivo: superar a Rambox en consumo de RAM en Linux/WebKitGTK.

### Resultado medido (3 servicios cargados: WhatsApp, Gmail, Outlook)

| Estado                                      | RAM total | WebProcess |
|---------------------------------------------|-----------|------------|
| Original                                    | ~3 GB     | 5+         |
| Lazy mount + suspend                        | 1.2 GB    | 4          |
| + `with_related_view` + PSON off            | 1.1 GB    | 3          |
| + vendor wry + FFI feature flags            | **1.3 GB** | **2**     |

El segundo WebProcess remanente es un sandbox iframe forzado por `Cross-Origin-Embedder-Policy` de Gmail, no controlable por la app.

### Cambios entregados

- [x] **Lazy mount**: solo el servicio activo se monta al iniciar; el resto on-demand al hacer switch (`services.rs::mount_active_service` invocado dentro del callback `with_webview` que captura el bar — evita race en la captura).
- [x] **Auto-suspend** configurable (default 5 min) por inactividad. Timers en frontend; backend expone `suspend_service` que valida invariantes.
- [x] **`keep_alive`** por servicio: flag para excluir del suspend (apps con notif background, ej. WhatsApp). UI: checkbox en panel edit.
- [x] **Settings UI**: input minutos de suspend (`get/set_inactive_suspend_minutes`).
- [x] **Sesiones por host+slot** (`session_slot: u32` en `Service`): infra para casos futuros donde se requiera aislar 2 cuentas del mismo host. Hoy todos los servicios comparten WebContext default y cookies se aíslan por dominio nativamente.
- [x] **Sin `data_directory`** en `mount_child` (Linux): todos los webviews comparten el WebContext default → mismo NetworkProcess + mismo pool de WebProcess.
- [x] **`with_related_view(bar)`**: el bar webview se cachea (`BarWebViewHandle`) y se pasa como related a cada servicio. API nativa WebKitGTK para indicar relación de proceso (aunque WebKit moderno la trata como hint).
- [x] **Vendor de wry-0.54.4** en `vendor/wry/`, `[patch.crates-io]` en `src-tauri/Cargo.toml`. Dos patches:
  - `WebContext::builder().process_swap_on_cross_site_navigation_enabled(false)` en `webkitgtk/web_context.rs::WebContextImpl::new`.
  - FFI directa a `webkit_settings_set_feature_enabled` en `set_webview_settings`: ENABLE `UsesSingleWebProcess`, DISABLE `SiteIsolation`, `SiteIsolationSharedProcess`, `ProcessSwapOnCrossSiteNavigation`. La API de feature flags no está expuesta en el crate `webkit2gtk` 2.0.x.
- [x] **Migración destructiva**: `migrate_old_sessions` borra `sessions/<uuid>/` viejos al detectar IDs no compatibles; modal Svelte avisa al usuario que debe reloguear.
- [x] **Cleanup `unsafe_html` warnings backend**: ninguno introducido por este branch.

### Trade-offs aceptados

- **Sin SiteIsolation**: si una pestaña ejecuta exploit, puede leer otros sites. Aceptable para chat aggregator (sitios confiables: Slack/Gmail/WhatsApp).
- **Sin process isolation entre tabs**: si una tab crashea, caen todas.
- **Cookies compartidas por dominio**: 2 cuentas del mismo host comparten login. El campo `session_slot` queda como infra para futura UX "agregar 2da cuenta" si se decide soportar.
- **Vendor wry**: actualizar Tauri (cada 3-6 meses) requiere re-vendorear y re-aplicar patches. Documentado en commit history del branch.
- **macOS** (`layout_other.rs`): sin tocar; las opt aplican solo a Linux.

### Pendientes detectados durante el trabajo

- [ ] **Bug migración**: la heurística `migrate_old_sessions` busca formato UUID v4 (`len==36, 4 guiones`) pero los IDs reales de Kolibri son timestamps (`svc_177...`, `1777...`). Nunca se gatilla y los dirs viejos coexisten con `by-host/`. Cambiar heurística a "cualquier dir top-level distinto de `by-host`".
- [ ] **Validar suspend con engine unificado**: ahora con 1 sólo WebProcess compartido, `suspend_service` mata el WebView pero el proceso sigue vivo (sirve a otros). Medir si realmente libera RAM o solo libera DOM/JS de esa tab. Posiblemente revisar UX del feature.
- [ ] **`new_ephemeral` en wry sin patch**: `WebContext::new_ephemeral()` no usa builder, no se puede inyectar PSON ahí. Kolibri no usa modo incógnito → inocuo, pero dejar nota si se agrega.
- [ ] **Rebase strategy ante upgrade de Tauri**: documentar el procedimiento de re-vendor de wry (actualmente sólo está en commits del branch).

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
