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

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("cab-config-{name}-{}.toml", uuid::Uuid::new_v4()))
    }

    #[test]
    fn default_config_uses_local_gateway_defaults() {
        let cfg = CabConfig::default();
        assert_eq!(cfg.gateway.host, "127.0.0.1");
        assert_eq!(cfg.gateway.port, 3125);
    }

    #[test]
    fn load_from_reads_valid_toml_and_applies_field_defaults() {
        let full = temp_path("full");
        std::fs::write(
            &full,
            r#"[gateway]
host = "0.0.0.0"
port = 4567
"#,
        )
        .unwrap();
        let cfg = CabConfig::load_from(full.to_str().unwrap());
        assert_eq!(cfg.gateway.host, "0.0.0.0");
        assert_eq!(cfg.gateway.port, 4567);
        let _ = std::fs::remove_file(full);

        let partial = temp_path("partial");
        std::fs::write(&partial, "[gateway]\n").unwrap();
        let cfg = CabConfig::load_from(partial.to_str().unwrap());
        assert_eq!(cfg.gateway.host, "127.0.0.1");
        assert_eq!(cfg.gateway.port, 3125);
        let _ = std::fs::remove_file(partial);
    }

    #[test]
    fn load_from_falls_back_for_missing_or_invalid_files() {
        let missing = temp_path("missing");
        assert_eq!(
            CabConfig::load_from(missing.to_str().unwrap()).gateway.port,
            3125
        );

        let invalid = temp_path("invalid");
        std::fs::write(&invalid, "not = [toml").unwrap();
        assert_eq!(
            CabConfig::load_from(invalid.to_str().unwrap()).gateway.port,
            3125
        );
        let _ = std::fs::remove_file(invalid);
    }
}
