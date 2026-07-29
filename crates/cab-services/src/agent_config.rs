//! Agent configuration switcher — delegates to per-agent integrations.

pub use crate::agents::{
    apply_agent_config, backup_agent_config, cab_identifying_headers, opencode_model_config,
    yaml_quote,
};
