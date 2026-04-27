// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // FIX EGL_BAD_PARAMETER en WebKitGTK 2.44+ con DMA-BUF renderer:
    // crashea en sistemas sin EGL/DRM correctamente configurado (NVIDIA propietario,
    // algunas AMD, Wayland sin compositor compatible). Forzar fallback al renderer
    // de software/X11 evita el abort. Solo Linux.
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        if std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none() {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }
    }
    kolibri_lib::run()
}
