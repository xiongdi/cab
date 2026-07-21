//! CAB data directory resolution.
//!
//! Priority:
//! 1. `CAB_HOME` environment variable (absolute path to the data root)
//! 2. Default user home: `$HOME/.cab` / `%USERPROFILE%\.cab`

use std::path::PathBuf;

/// Root directory for CAB runtime data (`cab.db`, catalog cache, logos, logs, …).
pub fn cab_home() -> PathBuf {
    if let Ok(dir) = std::env::var("CAB_HOME") {
        let trimmed = dir.trim();
        if !trimmed.is_empty() {
            return PathBuf::from(trimmed);
        }
    }
    default_user_cab_home()
}

/// Default per-user data directory (`~/.cab`), ignoring `CAB_HOME`.
pub fn default_user_cab_home() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| {
            let tmp = std::env::temp_dir().join(".cab-fallback");
            tmp.to_string_lossy().into_owned()
        });
    PathBuf::from(home).join(".cab")
}

/// Default system-wide data directory for `--scope system` installs.
pub fn default_system_cab_home() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let program_data =
            std::env::var("ProgramData").unwrap_or_else(|_| r"C:\ProgramData".into());
        return PathBuf::from(program_data).join("cab");
    }
    #[cfg(target_os = "macos")]
    {
        return PathBuf::from("/Library/Application Support/cab");
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        PathBuf::from("/var/lib/cab")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cab_home_respects_env() {
        let tmp = tempfile::tempdir().unwrap();
        let custom = tmp.path().join("custom-cab");
        // SAFETY: test-only; serialised by tempfile uniqueness
        unsafe {
            std::env::set_var("CAB_HOME", &custom);
        }
        assert_eq!(cab_home(), custom);
        unsafe {
            std::env::remove_var("CAB_HOME");
        }
    }
}
