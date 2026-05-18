//! Default interactive shell per platform.

/// Shell executable for local PTY sessions.
pub fn default_shell() -> String {
    #[cfg(windows)]
    {
        return std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
    }

    #[cfg(target_os = "android")]
    {
        for candidate in ["/system/bin/sh", "/system/bin/bash", "sh"] {
            if std::path::Path::new(candidate).exists() || candidate == "sh" {
                return candidate.to_string();
            }
        }
        return "/system/bin/sh".to_string();
    }

    #[cfg(all(unix, not(target_os = "android")))]
    {
        return std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    }

    #[cfg(not(any(windows, unix)))]
    {
        "sh".to_string()
    }
}
