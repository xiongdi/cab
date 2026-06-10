//! Agent integration plugins for CAB-managed coding agents.

mod claude_code;
mod codex;
mod hermes;
mod kilocode;
mod openclaw;
mod opencode;
mod pi;
pub mod shared;

#[cfg(test)]
mod tests;

use cab_core::types::{Agent, Model};
use cab_db::InMemoryStore;

pub use shared::{
    backup_agent_config, build_hermes_model_block, cab_identifying_headers, opencode_model_config,
    replace_top_level_yaml_block, run_openclaw_config, yaml_quote,
};

/// Shared inputs for writing an agent's on-disk configuration.
pub struct AgentConfigContext<'a> {
    pub pool: &'a InMemoryStore,
    pub agent: &'a Agent,
    pub gateway_port: i64,
    pub gateway_key: &'a str,
    pub home: String,
    pub mode: &'a str,
    pub api_key: &'a str,
    pub endpoint: &'a str,
    pub strategy: Option<&'a str>,
    pub cab_managed: bool,
    pub enabled_models: &'a [Model],
}

/// Plugin contract for a supported coding agent integration.
pub trait AgentIntegration: Sync {
    fn id(&self) -> &'static str;
    fn apply(&self, ctx: &AgentConfigContext<'_>) -> Result<(), std::io::Error>;
}

static INTEGRATIONS: &[&dyn AgentIntegration] = &[
    &claude_code::Integration,
    &codex::Integration,
    &opencode::Integration,
    &kilocode::Integration,
    &hermes::Integration,
    &openclaw::Integration,
    &pi::Integration,
];

/// Canonical list of CAB-managed agent IDs (single source of truth).
pub const SUPPORTED_AGENT_IDS: &[&str] = &[
    "claude-code",
    "codex",
    "opencode",
    "hermes",
    "kilocode",
    "openclaw",
    "pi",
];

pub fn supported_agent_ids() -> &'static [&'static str] {
    SUPPORTED_AGENT_IDS
}

fn find_integration(agent_id: &str) -> Option<&dyn AgentIntegration> {
    INTEGRATIONS
        .iter()
        .find(|integration| integration.id() == agent_id)
        .copied()
}

/// Apply on-disk configuration for a known agent integration.
pub async fn apply_agent_config(
    pool: &InMemoryStore,
    agent: &Agent,
    gateway_port: i64,
    gateway_key: &str,
) -> Result<(), std::io::Error> {
    let mode = agent.mode.as_str();
    let cab_managed = mode == "auto" || mode == "manual";
    let enabled_models = if mode == "manual" {
        shared::collect_enabled_models(pool).await
    } else {
        Vec::new()
    };

    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map_err(|_| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "User home directory could not be resolved (neither HOME nor USERPROFILE env var is set)",
            )
        })?;

    let strategy = agent.model_id.as_deref().filter(|s| !s.is_empty());
    let ctx = AgentConfigContext {
        pool,
        agent,
        gateway_port,
        gateway_key,
        home,
        mode,
        api_key: &agent.api_key,
        endpoint: &agent.endpoint,
        strategy,
        cab_managed,
        enabled_models: &enabled_models,
    };

    if let Some(integration) = find_integration(&agent.id) {
        integration.apply(&ctx)?;
    }

    Ok(())
}
