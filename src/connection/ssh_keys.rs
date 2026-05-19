//! OpenSSH private key paths (Linux, macOS, Windows).

use std::path::PathBuf;

/// User home directory (`$HOME` / `%USERPROFILE%`).
pub fn ssh_home_dir() -> Option<PathBuf> {
    if let Some(u) = directories::UserDirs::new() {
        return Some(u.home_dir().to_path_buf());
    }
    #[cfg(windows)]
    {
        return std::env::var_os("USERPROFILE").map(PathBuf::from);
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Default private keys under `~/.ssh` (same layout as OpenSSH on Windows).
pub fn default_key_paths() -> Vec<PathBuf> {
    let Some(home) = ssh_home_dir() else {
        return Vec::new();
    };
    let ssh_dir = home.join(".ssh");
    ["id_ed25519", "id_rsa", "id_ecdsa"]
        .into_iter()
        .map(|name| ssh_dir.join(name))
        .collect()
}
