# Kolibri

Cliente liviano multi-servicio (WhatsApp Web, Gmail, Outlook, etc.) construido con Tauri + Rust + Svelte.

Alternativa minimalista a Rambox. Una sola WebView del SO en lugar de un Chromium por servicio = menos RAM, menos CPU, arranque rápido.

## Stack

- **Tauri 2** — shell nativo, ~600KB
- **Rust** — gestión de sesiones, tray, notificaciones, hotkeys, persistencia
- **Svelte + TypeScript** — UI sidebar
- **WebView del SO** — WebKitGTK (Linux), WebView2 (Windows)

## Objetivo RAM

3 servicios activos: ~200-400 MB (vs Rambox/Electron ~1 GB+).

## Servicios planeados

- WhatsApp Web
- Gmail
- Outlook
- Genéricos (URL custom)

## Desarrollo

```bash
pnpm install
pnpm tauri dev
```

## Build

```bash
pnpm tauri build
```

## Estado

WIP. Scaffold inicial.
