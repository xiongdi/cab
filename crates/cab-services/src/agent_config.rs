//! Agent configuration switcher — delegates to per-agent integrations.

pub use crate::agents::{
    apply_agent_config, backup_agent_config, build_hermes_model_block, cab_identifying_headers,
    opencode_model_config, replace_top_level_yaml_block, run_openclaw_config, yaml_quote,
};
