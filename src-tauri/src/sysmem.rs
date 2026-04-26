use std::sync::Mutex;
#[cfg(not(target_os = "linux"))]
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
#[cfg(target_os = "linux")]
use sysinfo::System;

pub struct SysState {
    #[allow(dead_code)]
    pub sys: Mutex<System>,
}

impl Default for SysState {
    fn default() -> Self {
        Self {
            sys: Mutex::new(System::new()),
        }
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use std::collections::{HashMap, HashSet};
    use std::fs;

    fn read_status(pid: u32) -> Option<(u32, u64)> {
        let s = fs::read_to_string(format!("/proc/{}/status", pid)).ok()?;
        let mut ppid: Option<u32> = None;
        let mut rss_bytes: Option<u64> = None;
        for line in s.lines() {
            if let Some(rest) = line.strip_prefix("PPid:") {
                ppid = rest.trim().parse().ok();
            } else if let Some(rest) = line.strip_prefix("VmRSS:") {
                let kb: Option<u64> = rest.trim().split_whitespace().next().and_then(|t| t.parse().ok());
                rss_bytes = kb.map(|k| k * 1024);
            }
            if ppid.is_some() && rss_bytes.is_some() { break; }
        }
        Some((ppid.unwrap_or(0), rss_bytes.unwrap_or(0)))
    }

    pub fn tree_rss_bytes(self_pid: u32) -> u64 {
        // Lee todos los procesos una vez, construye mapa parent -> children.
        let mut info: HashMap<u32, (u32, u64)> = HashMap::new();
        let Ok(entries) = fs::read_dir("/proc") else { return 0 };
        for e in entries.flatten() {
            let name = e.file_name();
            let Some(s) = name.to_str() else { continue };
            let Ok(pid) = s.parse::<u32>() else { continue };
            if let Some((ppid, rss)) = read_status(pid) {
                info.insert(pid, (ppid, rss));
            }
        }
        let mut children_of: HashMap<u32, Vec<u32>> = HashMap::new();
        for (&pid, &(ppid, _)) in &info {
            children_of.entry(ppid).or_default().push(pid);
        }

        let mut total: u64 = 0;
        let mut visited: HashSet<u32> = HashSet::new();
        let mut stack: Vec<u32> = vec![self_pid];
        while let Some(pid) = stack.pop() {
            if !visited.insert(pid) { continue; }
            if let Some(&(_, rss)) = info.get(&pid) {
                total = total.saturating_add(rss);
            }
            if let Some(cs) = children_of.get(&pid) {
                for &c in cs { stack.push(c); }
            }
        }
        total
    }
}

#[cfg(not(target_os = "linux"))]
fn tree_rss_bytes_fallback(state: &SysState) -> u64 {
    let mut sys = state.sys.lock().expect("SysState mutex poisoned");
    let self_pid = Pid::from_u32(std::process::id());
    sys.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::new().with_memory(),
    );

    use std::collections::HashSet;
    let mut total: u64 = 0;
    let mut visited: HashSet<Pid> = HashSet::new();
    let mut stack: Vec<Pid> = vec![self_pid];
    while let Some(pid) = stack.pop() {
        if !visited.insert(pid) { continue; }
        if let Some(proc) = sys.process(pid) {
            total = total.saturating_add(proc.memory());
        }
        for (cpid, cproc) in sys.processes() {
            if cproc.parent() == Some(pid) && !visited.contains(cpid) {
                stack.push(*cpid);
            }
        }
    }
    total
}

#[tauri::command]
pub fn get_memory_usage(state: tauri::State<'_, SysState>) -> u64 {
    #[cfg(target_os = "linux")]
    {
        let _ = state; // sysinfo no se usa en Linux
        return linux::tree_rss_bytes(std::process::id());
    }
    #[cfg(not(target_os = "linux"))]
    {
        return tree_rss_bytes_fallback(&state);
    }
}
