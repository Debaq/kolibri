# Kolibri — Roadmap

## Próximos pasos cortos

- [ ] Limpieza: quitar `eprintln!` debug + `setInterval` poll del bar (eventos suficientes)
- [ ] Verificar que el borde transparente raro del WM ya no aparece en service windows
- [ ] Probar con Gmail, Outlook, Telegram que la barra inyectada se mantiene en SPAs (MutationObserver hace su trabajo)

## Personalización por servicio

Extender `ServiceTemplate` (y `Service` en Rust) con campos opcionales que se apliquen en `ensure_service_window`:

| Campo | Para qué |
|-------|----------|
| `user_agent` | Override por sitio (WSP necesita UA Chrome; otros podrían romperse con UA default) |
| `custom_js` | Script inyectado además del bar — ocultar headers duplicados del sitio, tweaks visuales |
| `custom_css` | CSS extra inyectado |
| `zoom` | Zoom inicial (Discord/Notion se ven chicos) |
| `auto_grant_notifications` | Conceder permiso de Notification API automáticamente |
| `block_popups` | Bloquear popups de login/spam |

## Settings panel

Botón ⚙ del bar abre una vista en main window con:
- Lista de servicios → renombrar, cambiar URL, cambiar color/icono, eliminar
- Reordenar tabs (drag)
- Atajos globales
- Tema (dark/light, accent)

## UX

- [ ] Iconos reales (fetch del favicon del servicio en vez de letra inicial)
- [ ] Reordenar tabs vía drag-and-drop
- [ ] Badge no-leídos por servicio: inyectar JS que escuche cambios en `document.title` (la mayoría usa `(3) Inbox`) → emit a Rust → mostrar contador en tab + tray
- [ ] Atajo global `Ctrl+Alt+K` para mostrar/ocultar la app
- [ ] Atajo `Ctrl+1..9` para switchear entre tabs
- [ ] Reload botón visible cuando un servicio falla a cargar
- [ ] Splash/skeleton mientras la primera carga del servicio
- [ ] Modal "agregar servicio" con búsqueda más potente + sugerencias en base a uso

## Multi-OS

- [ ] Probar en Windows (alguien del equipo). El path Windows ya no necesita hack — debería funcionar out-of-the-box porque la arquitectura es WebviewWindow por servicio (no child webview)
- [ ] Macos: opcional, validar comportamiento de drag y controles ventana

## Sistema

- [ ] Auto-update via Tauri updater (firma + endpoint)
- [ ] Empaquetado: `.deb`, `.rpm`, `.AppImage`, `.msi`
- [ ] CI GitHub Actions: build cross-platform + release on tag
- [ ] Tray icon mejor: counter de no-leídos total

## Datos

- [ ] Backup/restore de `services.json` + `sessions/` (export ZIP)
- [ ] Modo invitado por servicio (sesión efímera, sin persistir cookies)
- [ ] Limpieza programada de cookies expiradas

## Riesgos / deudas técnicas

- **CSP variantes**: usar siempre APIs DOM seguras (`createElement`, `removeChild`) en `bar.js`. Nunca `innerHTML`, `document.write`, `eval`, ni inline event handlers como string.
- **Trusted Types**: Gmail, Outlook y otros Google sites lo usan. El bar ya está adaptado.
- **Service workers**: algunos sitios registran SW que puede interferir. Si pasa, considerar deshabilitar SW por servicio.
- **Polling como fallback**: hoy `setInterval(refresh, 1500)` en `bar.js`. Quitar cuando los eventos `kolibri:services_changed` / `kolibri:active_changed` estén verificados estables.

## Ideas largas

- Plugin/extension API: que terceros agreguen servicios con su propio bar customization
- Sync cross-device de la lista de servicios (sin cookies — solo metadata) via WebDAV/Nextcloud
- Modo workspace: agrupar servicios (ej: "Trabajo" = Slack+Gmail+Notion; "Personal" = WhatsApp+IG)
