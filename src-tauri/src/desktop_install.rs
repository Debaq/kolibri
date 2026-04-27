//! Auto-instala entrada `.desktop` + iconos hicolor en `~/.local/share/`
//! para que Kolibri aparezca en el menu/launcher de cualquier WM Linux sin
//! pasos manuales del usuario. Idempotente: re-instala solo si cambia version.
//!
//! Tambien fija `StartupWMClass=kolibri` y prgname GTK matching para que el
//! WM asocie la ventana corriendo al icono instalado.

#![cfg(target_os = "linux")]

use std::fs;
use std::path::PathBuf;

const APP_ID: &str = "kolibri";
const APP_NAME: &str = "Kolibri";
const APP_GENERIC: &str = "Multi-service messenger";
const APP_COMMENT: &str = "Cliente multi-servicio (WhatsApp, Gmail, Outlook)";
const APP_CATEGORIES: &str = "Network;InstantMessaging;Email;";

const ICON_32: &[u8] = include_bytes!("../icons/32x32.png");
const ICON_64: &[u8] = include_bytes!("../icons/64x64.png");
const ICON_128: &[u8] = include_bytes!("../icons/128x128.png");
const ICON_256: &[u8] = include_bytes!("../icons/128x128@2x.png");
const ICON_512: &[u8] = include_bytes!("../icons/512x512.png");

const ICONS: &[(&str, &[u8])] = &[
    ("32x32", ICON_32),
    ("64x64", ICON_64),
    ("128x128", ICON_128),
    ("256x256", ICON_256),
    ("512x512", ICON_512),
];

fn data_home() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        let p = PathBuf::from(xdg);
        if p.is_absolute() {
            return Some(p);
        }
    }
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share"))
}

fn marker_path(data: &PathBuf) -> PathBuf {
    data.join("kolibri").join("install.version")
}

fn desktop_path(data: &PathBuf) -> PathBuf {
    data.join("applications").join("kolibri.desktop")
}

fn icon_path(data: &PathBuf, size: &str) -> PathBuf {
    data.join("icons/hicolor")
        .join(size)
        .join("apps/kolibri.png")
}

fn build_desktop_entry(exec: &str) -> String {
    // Quoting: si el path tiene espacios, envolver en comillas dobles segun
    // spec freedesktop. Reservados: " ' \ ` $ se escapan con \\.
    let exec_quoted = if exec
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '\\' | '`' | '$'))
    {
        let escaped: String = exec
            .chars()
            .map(|c| match c {
                '"' | '\\' | '`' | '$' => format!("\\{}", c),
                _ => c.to_string(),
            })
            .collect();
        format!("\"{}\"", escaped)
    } else {
        exec.to_string()
    };

    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.0\n\
         Name={name}\n\
         GenericName={generic}\n\
         Comment={comment}\n\
         Exec={exec}\n\
         Icon={id}\n\
         Terminal=false\n\
         Categories={cats}\n\
         StartupNotify=true\n\
         StartupWMClass={id}\n\
         SingleMainWindow=true\n",
        name = APP_NAME,
        generic = APP_GENERIC,
        comment = APP_COMMENT,
        exec = exec_quoted,
        id = APP_ID,
        cats = APP_CATEGORIES,
    )
}

fn current_marker() -> String {
    format!("{}\n{}\n", env!("CARGO_PKG_VERSION"), exec_path_string())
}

fn exec_path_string() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.to_str().map(|s| s.to_string()))
        .unwrap_or_else(|| APP_ID.to_string())
}

/// Devuelve true si re-instalo (algo cambio o era primera vez).
pub fn install(force: bool) -> std::io::Result<bool> {
    let Some(data) = data_home() else {
        return Ok(false);
    };
    let marker = marker_path(&data);
    let want = current_marker();
    if !force {
        if let Ok(prev) = fs::read_to_string(&marker) {
            if prev == want {
                return Ok(false);
            }
        }
    }

    // Iconos
    for (size, bytes) in ICONS {
        let p = icon_path(&data, size);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&p, bytes)?;
    }

    // .desktop
    let dp = desktop_path(&data);
    if let Some(parent) = dp.parent() {
        fs::create_dir_all(parent)?;
    }
    let entry = build_desktop_entry(&exec_path_string());
    fs::write(&dp, entry)?;

    // Marker
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&marker, want)?;

    // Best-effort: refrescar caches (no falla si no estan instalados).
    let apps_dir = data.join("applications");
    let icons_dir = data.join("icons/hicolor");
    let _ = std::process::Command::new("update-desktop-database")
        .arg(&apps_dir)
        .status();
    let _ = std::process::Command::new("gtk-update-icon-cache")
        .arg("-q")
        .arg("-t")
        .arg("-f")
        .arg(&icons_dir)
        .status();

    Ok(true)
}

