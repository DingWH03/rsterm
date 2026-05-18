//! Process / host labels for terminal tab titles (local PTY foreground on Linux).

use std::fs;

/// `user@hostname` for the local machine.
pub fn local_user_at_host() -> String {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "user".into());
    format!("{user}@{}", local_hostname())
}

pub fn ssh_user_at_host(user: &str, host: &str) -> String {
    format!("{user}@{host}")
}

/// Name of the foreground job in a local shell PTY, if any.
pub fn foreground_command(shell_pid: Option<u32>) -> Option<String> {
    let pid = shell_pid?;
    foreground_command_for_pid(pid)
}

#[cfg(target_os = "linux")]
fn foreground_command_for_pid(shell_pid: u32) -> Option<String> {
    let children = read_proc_children(shell_pid)?;
    for &child in children.iter().rev() {
        if child == shell_pid {
            continue;
        }
        let args = read_cmdline(child)?;
        let short = format_cmdline(&args);
        if !short.is_empty() && !is_interactive_shell(&short) {
            return Some(short);
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
fn foreground_command_for_pid(_shell_pid: u32) -> Option<String> {
    None
}

#[cfg(target_os = "linux")]
fn read_proc_children(pid: u32) -> Option<Vec<u32>> {
    let path = format!("/proc/{pid}/task/{pid}/children");
    let s = fs::read_to_string(path).ok()?;
    let pids: Vec<u32> = s.split_whitespace().filter_map(|x| x.parse().ok()).collect();
    if pids.is_empty() {
        None
    } else {
        Some(pids)
    }
}

#[cfg(target_os = "linux")]
fn read_cmdline(pid: u32) -> Option<Vec<String>> {
    let data = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
    let args: Vec<String> = data
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    if args.is_empty() {
        None
    } else {
        Some(args)
    }
}

fn format_cmdline(args: &[String]) -> String {
    let base = args[0].rsplit('/').next().unwrap_or(&args[0]);
    if args.len() <= 2 {
        args.iter()
            .map(|a| a.rsplit('/').next().unwrap_or(a.as_str()))
            .collect::<Vec<_>>()
            .join(" ")
    } else {
        format!("{base} …")
    }
}

fn is_interactive_shell(name: &str) -> bool {
    matches!(
        name,
        "sh" | "bash" | "zsh" | "fish" | "dash" | "ksh" | "tcsh" | "nu"
    ) || name.ends_with("/sh")
        || name.ends_with("/bash")
        || name.ends_with("/zsh")
        || name.ends_with("/fish")
}

#[cfg(unix)]
fn local_hostname() -> String {
    let mut buf = [0u8; 256];
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut _, buf.len()) };
    if rc != 0 {
        return "localhost".into();
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let name = String::from_utf8_lossy(&buf[..end]).trim().to_string();
    if name.is_empty() {
        "localhost".into()
    } else {
        name
    }
}

#[cfg(not(unix))]
fn local_hostname() -> String {
    std::env::var("HOSTNAME").unwrap_or_else(|_| "localhost".into())
}

pub fn truncate_label(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_chars.saturating_sub(1)).collect::<String>())
    }
}

/// True when OSC title is just host info (idle shell), not a running program name.
pub fn title_is_idle_host(title: &str, user_at_host: &str) -> bool {
    let title = title.trim();
    title.is_empty()
        || title == user_at_host
        || title.starts_with(&format!("{user_at_host}:"))
        || title.starts_with(&format!("{user_at_host} "))
}
