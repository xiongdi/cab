//! Invoke real coding-agent CLIs against the packaged CAB gateway.

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use super::TestServer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RealCaStatus {
    Pass,
    Skip,
    Fail,
}

#[derive(Debug, Clone)]
pub struct RealCaResult {
    pub agent_id: String,
    pub status: RealCaStatus,
    pub details: String,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
}

fn run_real_ca_script(agent_id: &str, prompt: &str, model: &str) -> RealCaResult {
    let script = repo_root().join("scripts/uat/run-real-ca.sh");
    let output = Command::new("bash")
        .arg(&script)
        .arg(agent_id)
        .arg(prompt)
        .arg(model)
        .env(
            "CAB_UAT_BASE_URL",
            std::env::var("CAB_UAT_BASE_URL").unwrap_or_default(),
        )
        .env(
            "CAB_UAT_GATEWAY_KEY",
            std::env::var("CAB_UAT_GATEWAY_KEY").unwrap_or_default(),
        )
        .output();

    match output {
        Ok(out) if out.status.code() == Some(127) => RealCaResult {
            agent_id: agent_id.to_string(),
            status: RealCaStatus::Skip,
            details: format!(
                "{}: CLI not installed — {}",
                agent_id,
                String::from_utf8_lossy(&out.stdout)
            ),
        },
        Ok(out) if out.status.success() => RealCaResult {
            agent_id: agent_id.to_string(),
            status: RealCaStatus::Pass,
            details: String::from_utf8_lossy(&out.stdout).trim().to_string(),
        },
        Ok(out) => RealCaResult {
            agent_id: agent_id.to_string(),
            status: RealCaStatus::Fail,
            details: format!(
                "{}: exit {:?}\nstdout: {}\nstderr: {}",
                agent_id,
                out.status.code(),
                String::from_utf8_lossy(&out.stdout).trim(),
                String::from_utf8_lossy(&out.stderr).trim()
            ),
        },
        Err(err) => RealCaResult {
            agent_id: agent_id.to_string(),
            status: RealCaStatus::Fail,
            details: format!("{agent_id}: failed to run script: {err}"),
        },
    }
}

/// Run a real CA CLI after agent mode has been applied on the packaged server.
pub async fn run_real_ca(
    _server: &TestServer,
    agent_id: &str,
    model: &str,
    prompt: &str,
) -> RealCaResult {
    // Allow CAB API to flush agent config to disk before the CLI reads it.
    tokio::time::sleep(Duration::from_millis(500)).await;
    run_real_ca_script(agent_id, prompt, model)
}

pub fn assert_all_real_ca_passed(results: &[RealCaResult]) {
    let mut failures = Vec::new();
    let mut skips = Vec::new();
    for result in results {
        match result.status {
            RealCaStatus::Pass => {}
            RealCaStatus::Skip => skips.push(result.clone()),
            RealCaStatus::Fail => failures.push(result.clone()),
        }
    }
    if !skips.is_empty() {
        let msg: Vec<String> = skips.iter().map(|r| r.details.clone()).collect();
        panic!(
            "real CA CLI missing (install all seven agents): {}",
            msg.join("; ")
        );
    }
    if !failures.is_empty() {
        let msg: Vec<String> = failures.iter().map(|r| r.details.clone()).collect();
        panic!("real CA failures: {}", msg.join("; "));
    }
}

pub fn summarize_results(results: &[RealCaResult]) -> String {
    results
        .iter()
        .map(|r| {
            let tag = match r.status {
                RealCaStatus::Pass => "ok",
                RealCaStatus::Skip => "skip",
                RealCaStatus::Fail => "fail",
            };
            format!("{}={}", r.agent_id, tag)
        })
        .collect::<Vec<_>>()
        .join(", ")
}
