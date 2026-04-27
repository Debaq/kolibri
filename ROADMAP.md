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
- [x] **Atajo global** `Ctrl+Alt+K` show/hide via `tauri-plugin-global-shortcut`. UI en Settings (capture + save) con `get/set_toggle_shortcut`. Atajos desde child webview siguen pendientes (requeriría init_script + IPC)
- [x] **Settings panel** real: tema dark/light/auto (sigue prefers-color-scheme), rename, cambiar URL, color, reordenar (botones up/down), referencia atajos
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

- [x] Iconos de la app (32x32, 64x64, 128x128, 128x128@2x, .icns, .ico, Square*, Android, iOS) generados desde `logo.png` con `pnpm tauri icon`
- [ ] Auto-update via Tauri updater (firma + endpoint)
- [x] Empaquetado `.AppImage` (Linux) y `.msi` (Windows). `.deb`/`.rpm` descartados.
- [ ] CI GitHub Actions: build cross-platform + release on tag
- [ ] README usuario final
- [x] **Auto-instalar `.desktop` + iconos en Linux**: `desktop_install.rs` escribe `~/.local/share/applications/kolibri.desktop` (o `$XDG_DATA_HOME/...`) y copia iconos hicolor 32/64/128/256/512. Idempotente via marker `install.version` (re-instala si cambia version o exec path). `.desktop` incluye `StartupWMClass=kolibri`; `gtk::glib::set_prgname("kolibri")` hace que GTK setee WM_CLASS instance matching → el WM asocia ventana al icono. Refresca caches via `update-desktop-database` y `gtk-update-icon-cache` (best-effort). Boton "Reinstalar entrada de menu" en Settings (cmd `reinstall_desktop_entry`).

## Datos / privacidad

- [ ] **Backup/restore**: export ZIP con `services.json` + `sessions/`
- [ ] **Modo invitado**: sesión efímera por servicio (no persiste cookies)
- [x] **Limpiar cookies/cache** desde Settings, por servicio (cmd `clear_service_session`, botón en edit modal)
- [x] **Logout** un servicio sin perderlo de la lista (mismo cmd, botón "Cerrar sesión" en edit modal)

## Release 0.1.4 (2026-04-26) — fix login Google/Microsoft

Bug introducido en `feat/ram-optimization` (0.1.3): al unificar todos los servicios en un solo `WebContext` compartido (sin `data_directory`) + patch FFI `UsesSingleWebProcess`/`SiteIsolation off` aplicado globalmente, Google/Microsoft detectaban entorno embebido y bloqueaban login redirigiendo a `workspace.google.com` "browser may not be secure". Outlook tampoco persistía cookies.

### Cambios

- [x] **`Service.isolated_session: bool`** (`services.rs`): flag por servicio. Hosts que lo requieren auto-detectados via `needs_isolated_session(host)` para `*.google.com`, `*.gmail.com`, `*.live.com`, `*.outlook.com`, `*.office.com`, `*.microsoft.com`, `*.microsoftonline.com`, `*.googleusercontent.com`. Migración en `load_from_disk` upgradea servicios persistidos.
- [x] **`mount_child` (`layout_linux.rs`)**: si `isolated_session=true` → `data_directory` propio (WebContext + NetworkProcess + WebProcess aislados, modelo v0.1.0). Si false → engine unificado (opt RAM 0.1.3).
- [x] **Patch FFI condicional** (`vendor/wry/src/webkitgtk/`): `WebContextImpl.is_unified` (true cuando `data_directory=None`). `set_webview_settings` solo aplica `UsesSingleWebProcess`/`SiteIsolation off` si el context es unificado. Servicios aislados conservan comportamiento WebKit estándar.
- [x] **Persistencia robusta de cookies** (`vendor/wry/src/webkitgtk/web_context.rs`) para context aislado:
  - `CookiePersistentStorage::Sqlite` (no `Text`) — robusto con cookies grandes de Microsoft.
  - `CookieAcceptPolicy::Always` — acepta third-party (necesario para flow `login.live.com → outlook.live.com`).
  - `set_itp_enabled(false)` — ITP rompía "Stay signed in" cross-domain.
- [x] **Toggle UI** "Sesión aislada" en panel edit del servicio (re-mount al cambiar).
- [x] **Tests nuevos**: `needs_isolated_session_*`, `host_of_handles_subdomains_and_ports`. Total: 23 tests verde.
- [x] **CI workflow** (`.github/workflows/ci.yml`): `cargo test --lib` + clippy + fmt + svelte-check + frontend build en push/PR a `main`.
- [x] **Warnings vendor wry silenciados** (deprecations `run_javascript` 2.40+, unused import del patch FFI).
- [x] **Bump** `package.json`, `Cargo.toml`, `tauri.conf.json` → 0.1.4.

### Trade-offs aceptados

- **+1 WebProcess** por servicio aislado (Gmail + Outlook → ~3 procesos vs 2 unificado puro). Sigue muy por debajo de Rambox (~3 GB vs ~1.5 GB Kolibri esperado).
- **`CookieAcceptPolicy::Always`** en aislados: superficie de tracking third-party. Aceptable: usuario eligió usar Gmail/Outlook que ya son ecosistemas Google/MS.

## Release 0.1.3 (2026-04-26)

- [x] **WhatsApp graba audio**: en `mount_child` (Linux), via `with_webview`, habilitar `enable_media_stream` / `enable_mediasource` / `enable_encrypted_media`, y conectar `permission-request` para auto-grant `UserMediaPermissionRequest` y `NotificationPermissionRequest`. Resto deny.
- [x] **Drag-and-drop de archivos** a WhatsApp/Slack: bridge GTK→JS en `scripts.rs::install_filedrop_bridge`. Intercepta signal `drag-data-received` (target text/uri-list), lee bytes (cap 64 MiB), base64-encodea, sintetiza `dragenter/dragover/drop` con `DataTransfer` poblado en el elemento bajo el cursor.
- [x] **Catalogo reducido**: solo WhatsApp, Gmail, Outlook hasta validar el resto con engine unificado.
- [x] **Validado 2026-04-26**: WhatsApp mic OK, DnD archivos OK.
- [ ] **Bug Gmail login**: no permite iniciar sesión (probable bloqueo Google "navegador no seguro" por UA/embedded webview). Investigar UA override o flujo OAuth externo.

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
- [x] **Tests unitarios** backend: `services.rs` (`sanitize_host`, `host_of`, `allocate_slot`, `apply_reorder`) y `favicons.rs` (`detect_mime` PNG/GIF/JPEG/WEBP/SVG/ico, `looks_valid`, `safe_name`, `host_from`). 20 tests total, `cargo test --lib` verde. `apply_reorder` extraído como helper puro desde `reorder_services`.

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

- [x] **Bug migración**: heurística `migrate_old_sessions` ahora borra cualquier dir top-level distinto de `by-host` (UUIDs viejos, `svc_*`, timestamps). Borrado quirúrgico (no nukea `by-host`).
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
- **Scratchpad**: tab especial con notas locales markdown
