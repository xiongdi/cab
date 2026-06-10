use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use cab_core::types::RequestLog;
use chrono::{Duration, NaiveDate, Utc};

use crate::InMemoryStore;

pub const LOG_FILE_PREFIX: &str = "requests-";
pub const LOG_FILE_SUFFIX: &str = ".jsonl";
pub const MAX_MEMORY_LOGS: usize = 500;

pub fn logs_dir() -> PathBuf {
    crate::settings::settings_file_path()
        .parent()
        .map(|p| p.join("logs"))
        .unwrap_or_else(|| PathBuf::from("logs"))
}

pub fn log_file_path_for_date(date: NaiveDate) -> PathBuf {
    logs_dir().join(format!(
        "{LOG_FILE_PREFIX}{}{LOG_FILE_SUFFIX}",
        date.format("%Y-%m-%d")
    ))
}

fn parse_date_from_filename(path: &Path) -> Option<NaiveDate> {
    let name = path.file_name()?.to_str()?;
    let stem = name
        .strip_prefix(LOG_FILE_PREFIX)?
        .strip_suffix(LOG_FILE_SUFFIX)?;
    NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()
}

pub fn append(log: &RequestLog) -> Result<(), String> {
    let dir = logs_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    let path = log_file_path_for_date(Utc::now().date_naive());
    let line = serde_json::to_string(log).map_err(|e| e.to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    file.write_all(line.as_bytes())
        .and_then(|_| file.write_all(b"\n"))
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn enforce_retention(retention_days: i64) -> Result<usize, String> {
    if retention_days <= 0 {
        return Ok(0);
    }

    let dir = logs_dir();
    if !dir.exists() {
        return Ok(0);
    }

    let cutoff = Utc::now().date_naive() - Duration::days(retention_days);
    let mut removed = 0usize;
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(file_date) = parse_date_from_filename(&path) else {
            continue;
        };
        if file_date < cutoff {
            fs::remove_file(&path).map_err(|e| e.to_string())?;
            removed += 1;
            tracing::info!("Removed expired log file {}", path.display());
        }
    }
    Ok(removed)
}

pub fn load_into_store(store: &InMemoryStore) -> Result<usize, String> {
    let dir = logs_dir();
    if !dir.exists() {
        return Ok(0);
    }

    let mut logs: Vec<RequestLog> = Vec::new();
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .map_err(|e| e.to_string())?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.is_file() && parse_date_from_filename(path).is_some())
        .collect();
    files.sort();

    for path in files {
        let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        for line in content.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<RequestLog>(line) {
                Ok(log) => logs.push(log),
                Err(e) => tracing::warn!("Skipping invalid log line in {}: {e}", path.display()),
            }
        }
    }

    logs.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    let mut deduped: Vec<RequestLog> = Vec::new();
    for log in logs {
        if let Some(pos) = deduped.iter().position(|existing| existing.id == log.id) {
            deduped[pos] = log;
        } else {
            deduped.push(log);
        }
    }

    if deduped.len() > MAX_MEMORY_LOGS {
        deduped = deduped.split_off(deduped.len() - MAX_MEMORY_LOGS);
    }

    let count = deduped.len();
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    inner.request_logs = deduped;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TEST_HOME_LOCK;

    struct TestHome {
        _dir: tempfile::TempDir,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl TestHome {
        fn new() -> Self {
            let lock = TEST_HOME_LOCK
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let dir = tempfile::tempdir().unwrap();
            unsafe {
                std::env::set_var("HOME", dir.path());
                std::env::remove_var("USERPROFILE");
            }
            Self {
                _dir: dir,
                _lock: lock,
            }
        }
    }

    fn sample_log(id: &str) -> RequestLog {
        RequestLog {
            id: id.into(),
            timestamp: format!("2026-06-10T12:00:{id}Z"),
            agent: "codex".into(),
            provider: "provider-1".into(),
            model: "model-1".into(),
            input_tokens: 1,
            output_tokens: 2,
            total_tokens: 3,
            latency_ms: 10,
            status: 200,
            error: None,
            path: "/v1/chat/completions".into(),
            stream: false,
        }
    }

    #[test]
    fn append_and_load_round_trip() {
        let _home = TestHome::new();
        append(&sample_log("a")).unwrap();
        append(&sample_log("b")).unwrap();

        let store = InMemoryStore::new();
        let loaded = load_into_store(&store).unwrap();
        assert_eq!(loaded, 2);
        assert_eq!(store.inner.read().unwrap().request_logs.len(), 2);
    }

    #[test]
    fn enforce_retention_removes_old_files() {
        let _home = TestHome::new();
        let old = log_file_path_for_date(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap());
        fs::create_dir_all(old.parent().unwrap()).unwrap();
        fs::write(&old, "{}\n").unwrap();

        let recent = log_file_path_for_date(Utc::now().date_naive());
        fs::write(&recent, "{}\n").unwrap();

        let removed = enforce_retention(30).unwrap();
        assert_eq!(removed, 1);
        assert!(!old.exists());
        assert!(recent.exists());
    }
}
