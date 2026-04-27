// Logger de RAM activable con `--log-ram` en CLI.
//
// Escribe a {app_data}/ram.log:
// - tick cada 10s con RSS total + procesos hijo (WebKitWebProcess en Linux,
//   msedgewebview2 en Windows) ordenados por consumo.
// - eventos puntuales: switch/mount/unmount/suspend, con delta vs último tick.
//
// La idea es tener histórico para diagnosticar qué proceso crece y cuándo.
// No usar en release a usuarios — append-only, no rota archivo.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, Runtime};

use crate::services::AppState;

static LOGGER: OnceLock<Mutex<Option<LoggerState>>> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();

struct LoggerState {
    file: File,
    last_total_rss: u64,
}

pub fn init(path: PathBuf) -> std::io::Result<()> {
    START.get_or_init(Instant::now);
    let mut f = OpenOptions::new().create(true).append(true).open(&path)?;
    writeln!(
        f,
        "=== kolibri ram log start pid={} version={} ===",
        std::process::id(),
        env!("CARGO_PKG_VERSION")
    )?;
    let lock = LOGGER.get_or_init(|| Mutex::new(None));
    *lock.lock().expect("ram_log mutex poisoned") = Some(LoggerState {
        file: f,
        last_total_rss: 0,
    });
    Ok(())
}

pub fn is_enabled() -> bool {
    LOGGER
        .get()
        .and_then(|m| m.lock().ok().map(|g| g.is_some()))
        .unwrap_or(false)
}

#[cfg(target_os = "linux")]
fn collect_children(self_pid: u32) -> (u64, Vec<(u32, String, u64)>) {
    use std::collections::{HashMap, HashSet};
    use std::fs;

    fn read_proc(pid: u32) -> Option<(u32, String, u64)> {
        let s = fs::read_to_string(format!("/proc/{}/status", pid)).ok()?;
        let mut ppid = 0u32;
        let mut rss = 0u64;
        let mut comm = String::new();
        for line in s.lines() {
            if let Some(r) = line.strip_prefix("Name:") {
                comm = r.trim().to_string();
            } else if let Some(r) = line.strip_prefix("PPid:") {
                ppid = r.trim().parse().unwrap_or(0);
            } else if let Some(r) = line.strip_prefix("VmRSS:") {
                let kb: u64 = r
                    .trim()
                    .split_whitespace()
                    .next()
                    .and_then(|t| t.parse().ok())
                    .unwrap_or(0);
                rss = kb * 1024;
            }
        }
        Some((ppid, comm, rss))
    }

    let mut info: HashMap<u32, (u32, String, u64)> = HashMap::new();
    if let Ok(entries) = fs::read_dir("/proc") {
        for e in entries.flatten() {
            let name = e.file_name();
            let Some(s) = name.to_str() else { continue };
            let Ok(pid) = s.parse::<u32>() else { continue };
            if let Some(t) = read_proc(pid) {
                info.insert(pid, t);
            }
        }
    }
    let mut children_of: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&pid, (ppid, _, _)) in &info {
        children_of.entry(*ppid).or_default().push(pid);
    }
    let mut total = 0u64;
    let mut out: Vec<(u32, String, u64)> = Vec::new();
    let mut visited: HashSet<u32> = HashSet::new();
    let mut stack: Vec<u32> = vec![self_pid];
    while let Some(pid) = stack.pop() {
        if !visited.insert(pid) {
            continue;
        }
        if let Some((_, comm, rss)) = info.get(&pid) {
            total = total.saturating_add(*rss);
            if pid != self_pid {
                out.push((pid, comm.clone(), *rss));
            }
        }
        if let Some(cs) = children_of.get(&pid) {
            for &c in cs {
                stack.push(c);
            }
        }
    }
    out.sort_by(|a, b| b.2.cmp(&a.2));
    (total, out)
}

#[cfg(not(target_os = "linux"))]
fn collect_children(self_pid: u32) -> (u64, Vec<(u32, String, u64)>) {
    use std::collections::HashSet;
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::new().with_memory(),
    );
    let self_pid = Pid::from_u32(self_pid);
    let mut total = 0u64;
    let mut out: Vec<(u32, String, u64)> = Vec::new();
    let mut visited: HashSet<Pid> = HashSet::new();
    let mut stack: Vec<Pid> = vec![self_pid];
    while let Some(pid) = stack.pop() {
        if !visited.insert(pid) {
            continue;
        }
        if let Some(p) = sys.process(pid) {
            total = total.saturating_add(p.memory());
            if pid != self_pid {
                out.push((pid.as_u32(), p.name().to_string_lossy().to_string(), p.memory()));
            }
        }
        for (cpid, cproc) in sys.processes() {
            if cproc.parent() == Some(pid) && !visited.contains(cpid) {
                stack.push(*cpid);
            }
        }
    }
    out.sort_by(|a, b| b.2.cmp(&a.2));
    (total, out)
}

fn fmt_mb(b: u64) -> String {
    format!("{:.1}MB", b as f64 / 1024.0 / 1024.0)
}

fn fmt_delta(b: i64) -> String {
    if b == 0 {
        "0".into()
    } else if b > 0 {
        format!("+{}", fmt_mb(b as u64))
    } else {
        format!("-{}", fmt_mb((-b) as u64))
    }
}

fn ts() -> String {
    let elapsed = START
        .get()
        .map(|s| s.elapsed())
        .unwrap_or_else(|| Duration::ZERO);
    let total_ms = elapsed.as_millis();
    format!("t+{:>6}.{:03}s", total_ms / 1000, total_ms % 1000)
}

pub fn snapshot<R: Runtime>(app: &AppHandle<R>, reason: &str) {
    if !is_enabled() {
        return;
    }
    let self_pid = std::process::id();
    let (total_rss, children) = collect_children(self_pid);

    let (active, services_count) = {
        let st = app.state::<AppState>();
        let g = st.inner.lock().expect("AppState mutex poisoned");
        (
            g.active_id.clone().unwrap_or_else(|| "-".into()),
            g.services.len(),
        )
    };
    let mounted_count = app
        .state::<crate::webview::MountedRegistry>()
        .inner
        .lock()
        .map(|g| g.len())
        .unwrap_or(0);

    let Some(lock) = LOGGER.get() else { return };
    let Ok(mut guard) = lock.lock() else { return };
    let Some(state) = guard.as_mut() else { return };

    let last = state.last_total_rss;
    let delta = total_rss as i64 - last as i64;
    state.last_total_rss = total_rss;

    let mut line = format!(
        "{} | {:24} | rss={} delta={} | active={} mounted={}/{}",
        ts(),
        reason,
        fmt_mb(total_rss),
        fmt_delta(delta),
        active,
        mounted_count,
        services_count
    );
    if !children.is_empty() {
        line.push_str(" | procs=[");
        let take = children.iter().take(10).collect::<Vec<_>>();
        for (i, (pid, comm, rss)) in take.iter().enumerate() {
            if i > 0 {
                line.push_str(", ");
            }
            line.push_str(&format!("{}({}):{}", comm, pid, fmt_mb(*rss)));
        }
        if children.len() > take.len() {
            line.push_str(&format!(" +{}", children.len() - take.len()));
        }
        line.push(']');
    }
    line.push('\n');
    let _ = state.file.write_all(line.as_bytes());
    let _ = state.file.flush();
}

pub fn start_ticker<R: Runtime>(app: AppHandle<R>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(10));
        if !is_enabled() {
            return;
        }
        snapshot(&app, "tick");
    });
}
