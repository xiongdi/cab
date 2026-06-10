//! Collect UAT case results and write a Markdown report after the run.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CaseStatus {
    Pass,
    Fail,
    Manual,
    Skipped,
}

impl CaseStatus {
    fn label(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Manual => "MANUAL",
            Self::Skipped => "SKIP",
        }
    }
}

#[derive(Clone)]
struct CaseEntry {
    id: String,
    title: String,
    status: CaseStatus,
    duration: Duration,
    details: String,
}

struct ReportState {
    run_started: String,
    report_path: PathBuf,
    entries: Vec<CaseEntry>,
}

static REPORT: OnceLock<Mutex<ReportState>> = OnceLock::new();

fn state() -> &'static Mutex<ReportState> {
    REPORT.get_or_init(|| {
        Mutex::new(ReportState {
            run_started: utc_now_rfc3339(),
            report_path: resolve_report_path(),
            entries: Vec::new(),
        })
    })
}

fn resolve_report_path() -> PathBuf {
    if let Ok(path) = std::env::var("CAB_UAT_REPORT") {
        return PathBuf::from(path);
    }
    let stamp = utc_now_compact();
    PathBuf::from(format!("reports/uat/uat-{stamp}.md"))
}

fn utc_now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}Z")
}

fn utc_now_compact() -> String {
    utc_now_rfc3339().replace(':', "")
}

pub fn init_once() {
    let state = state().lock().expect("uat report lock");
    if !state.entries.is_empty() {
        return;
    }
    if let Some(parent) = state.report_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
}

pub struct CaseGuard {
    id: String,
    title: String,
    started: Instant,
    pub details: String,
    recorded: bool,
}

impl CaseGuard {
    pub fn new(id: &str, title: &str) -> Self {
        init_once();
        println!("== {id}: {title} ==");
        Self {
            id: id.to_string(),
            title: title.to_string(),
            started: Instant::now(),
            details: String::new(),
            recorded: false,
        }
    }

    pub fn note(&mut self, detail: &str) {
        if !self.details.is_empty() {
            self.details.push_str("; ");
        }
        self.details.push_str(detail);
    }

    pub fn pass(mut self) {
        record_case(
            &self.id,
            &self.title,
            CaseStatus::Pass,
            self.started.elapsed(),
            &self.details,
        );
        self.recorded = true;
    }
}

impl Drop for CaseGuard {
    fn drop(&mut self) {
        if self.recorded {
            return;
        }
        let status = if std::thread::panicking() {
            CaseStatus::Fail
        } else {
            CaseStatus::Pass
        };
        record_case(
            &self.id,
            &self.title,
            status,
            self.started.elapsed(),
            &self.details,
        );
    }
}

pub fn record_case(id: &str, title: &str, status: CaseStatus, duration: Duration, details: &str) {
    let mut state = state().lock().expect("uat report lock");
    if let Some(existing) = state.entries.iter_mut().find(|e| e.id == id) {
        existing.status = status;
        existing.duration = duration;
        existing.details = details.to_string();
        return;
    }
    state.entries.push(CaseEntry {
        id: id.to_string(),
        title: title.to_string(),
        status,
        duration,
        details: details.to_string(),
    });
    println!(
        "[UAT] {} {} ({:.1}s){}",
        status.label(),
        id,
        duration.as_secs_f64(),
        if details.is_empty() {
            String::new()
        } else {
            format!(" — {details}")
        }
    );
}

pub fn record_manual(id: &str, title: &str, details: &str) {
    record_case(id, title, CaseStatus::Manual, Duration::ZERO, details);
}

fn read_settings_summary() -> String {
    let path = cab_db::settings::settings_file_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return format!("settings: missing ({})", path.display());
    };
    let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) else {
        return "settings: invalid JSON".to_string();
    };

    let mut lines = vec![format!("- settings: `{}`", path.display())];
    if let Some(port) = json.get("gateway_port").and_then(|v| v.as_i64()) {
        lines.push(format!("- gateway_port: {port}"));
    }

    if let Some(providers) = json.get("providers").and_then(|v| v.as_object()) {
        let enabled: Vec<String> = providers
            .iter()
            .filter(|(_, v)| v.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false))
            .map(|(k, v)| {
                let anthropic = v
                    .get("endpoints")
                    .and_then(|e| e.as_array())
                    .map(|eps| {
                        eps.iter().any(|ep| {
                            ep.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false)
                                && ep.get("protocol").and_then(|p| p.as_str()) == Some("anthropic")
                        })
                    })
                    .unwrap_or(false);
                if anthropic {
                    format!("{k} (+anthropic)")
                } else {
                    k.clone()
                }
            })
            .collect();
        lines.push(format!(
            "- enabled providers ({}): {}",
            enabled.len(),
            if enabled.is_empty() {
                "(none)".into()
            } else {
                enabled.join(", ")
            }
        ));
    }

    if let Some(models) = json.get("models").and_then(|v| v.as_object()) {
        let enabled: Vec<&str> = models
            .iter()
            .filter(|(_, v)| v.get("enabled").and_then(|e| e.as_bool()).unwrap_or(false))
            .map(|(k, _)| k.as_str())
            .collect();
        lines.push(format!(
            "- enabled models ({}): {}",
            enabled.len(),
            if enabled.is_empty() {
                "(none)".into()
            } else {
                enabled.join(", ")
            }
        ));
    }

    lines.join("\n")
}

fn cab_version() -> String {
    std::env::var("CAB_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

pub fn write_report() -> PathBuf {
    init_once();
    record_manual(
        "UAT-07",
        "Desktop shell (manual)",
        "Run `npm run tauri:dev` — seven pages + i18n",
    );

    let state = state().lock().expect("uat report lock");
    let path = state.report_path.clone();
    let entries = state.entries.clone();
    let run_started = state.run_started.clone();
    drop(state);

    let mut pass = 0usize;
    let mut fail = 0usize;
    let mut manual = 0usize;
    let mut skipped = 0usize;

    let mut sorted = entries;
    sorted.sort_by(|a, b| a.id.cmp(&b.id));

    let mut body = String::new();
    body.push_str("# CAB UAT Report\n\n");
    body.push_str(&format!("- Generated (UTC): {}\n", utc_now_rfc3339()));
    body.push_str(&format!("- Run started (UTC): {}\n", run_started));
    body.push_str(&format!("- CAB version: {}\n", cab_version()));
    body.push_str(&format!(
        "- Host: {}\n",
        std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("HOST"))
            .unwrap_or_else(|_| "unknown".into())
    ));
    body.push_str("\n## Environment\n\n");
    body.push_str(&read_settings_summary());
    if let Ok(url) = std::env::var("CAB_UAT_BASE_URL") {
        body.push_str(&format!("\n- UAT server: {url}"));
    }
    if let Ok(bin) = std::env::var("CAB_UAT_SERVER_BIN") {
        body.push_str(&format!("\n- packaged binary: `{bin}`"));
    }
    body.push_str("\n\n## Results\n\n");
    body.push_str("| ID | Case | Status | Duration | Details |\n");
    body.push_str("| --- | --- | --- | --- | --- |\n");

    for entry in &sorted {
        match entry.status {
            CaseStatus::Pass => pass += 1,
            CaseStatus::Fail => fail += 1,
            CaseStatus::Manual => manual += 1,
            CaseStatus::Skipped => skipped += 1,
        }
        let duration = if entry.duration.is_zero() {
            "—".to_string()
        } else {
            format!("{:.1}s", entry.duration.as_secs_f64())
        };
        let details = if entry.details.is_empty() {
            "—".to_string()
        } else {
            entry.details.replace('|', "\\|")
        };
        body.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            entry.id,
            entry.title.replace('|', "\\|"),
            entry.status.label(),
            duration,
            details
        ));
    }

    body.push_str("\n## Summary\n\n");
    body.push_str(&format!("- **Pass**: {pass}\n"));
    body.push_str(&format!("- **Fail**: {fail}\n"));
    body.push_str(&format!("- **Manual**: {manual}\n"));
    if skipped > 0 {
        body.push_str(&format!("- **Skipped**: {skipped}\n"));
    }
    body.push_str(&format!(
        "- **Overall**: {}\n",
        if fail == 0 { "PASS" } else { "FAIL" }
    ));

    write_atomic(&path, &body);
    link_latest(&path);
    println!("\n== UAT report written: {} ==", path.display());
    path
}

fn write_atomic(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp = path.with_extension("md.tmp");
    let mut file = std::fs::File::create(&tmp).expect("create uat report temp");
    file.write_all(content.as_bytes())
        .expect("write uat report temp");
    std::fs::rename(&tmp, path).expect("rename uat report");
}

fn link_latest(path: &Path) {
    let Some(parent) = path.parent() else {
        return;
    };
    let latest = parent.join("latest.md");
    let _ = std::fs::remove_file(&latest);
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        let _ = symlink(path.file_name().unwrap_or_default(), &latest);
    }
    #[cfg(not(unix))]
    {
        let _ = std::fs::copy(path, &latest);
    }
}
