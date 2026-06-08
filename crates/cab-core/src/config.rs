use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CabConfig {
    #[serde(default = "default_gateway")]
    pub gateway: GatewayConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_gateway() -> GatewayConfig {
    GatewayConfig {
        host: default_host(),
        port: default_port(),
    }
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    3125
}

impl Default for CabConfig {
    fn default() -> Self {
        Self {
            gateway: default_gateway(),
        }
    }
}

impl CabConfig {
    /// Load config from `cab.toml`, falling back to defaults if file is missing.
    pub fn load() -> Self {
        Self::load_from("cab.toml")
    }

    pub fn load_from(path: &str) -> Self {
        let p = Path::new(path);
        if p.exists() {
            match std::fs::read_to_string(p) {
                Ok(content) => match toml::from_str::<CabConfig>(&content) {
                    Ok(cfg) => return cfg,
                    Err(e) => {
                        tracing::warn!("Failed to parse {path}: {e}, using defaults");
                    }
                },
                Err(e) => {
                    tracing::warn!("Failed to read {path}: {e}, using defaults");
                }
            }
        }
        CabConfig::default()
    }
}
