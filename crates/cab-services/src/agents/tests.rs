use super::*;
use crate::agents::shared::opencode_model_config;
use cab_core::types::{Agent, ApiKeyConfig, Model, Provider, ProviderEndpoint};
use std::fs;
use std::path::PathBuf;

static TEST_HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct TestHome {
    dir: tempfile::TempDir,
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
            std::env::remove_var("GROK_HOME");
        }
        Self { dir, _lock: lock }
    }

    fn path(&self, path: &str) -> PathBuf {
        self.dir.path().join(path)
    }
}

fn agent(id: &str, mode: &str) -> Agent {
    Agent {
        id: id.to_string(),
        name: id.to_string(),
        mode: mode.to_string(),
        model_id: Some("balanced".to_string()),
        api_key: String::new(),
        endpoint: String::new(),
        updated_at: String::new(),
    }
}

fn pool_with_models() -> cab_db::InMemoryStore {
    let pool = cab_db::InMemoryStore::new();
    {
        let mut data = pool.inner.write().unwrap();
        data.providers.insert(
            "provider-1".into(),
            Provider {
                id: "provider-1".into(),
                name: "Provider One".into(),
                endpoints: vec![ProviderEndpoint {
                    id: "chat".into(),
                    protocol: "openai-chat".into(),
                    url: "https://provider.test/v1".into(),
                    label: None,
                    priority: 50,
                    enabled: true,
                }],
                api_key: "key".into(),
                enabled: true,
                created_at: "now".into(),
                updated_at: "now".into(),
                privacy_policy_url: None,
                terms_of_service_url: None,
                status_page_url: None,
                headquarters: None,
                datacenters: None,
                api_keys: vec![ApiKeyConfig {
                    key: "key".into(),
                    enabled: true,
                    quota_reset_at: None,
                }],
                api: None,
                doc: None,
                env: None,
                npm: None,
                model_count: 0,
                logo: None,
                catalog_models: vec![],
            },
        );
        data.models.insert(
            "model-1".into(),
            Model {
                id: "model-1".into(),
                name: "provider/model-1".into(),
                display_name: "Model One".into(),
                provider_id: "provider-1".into(),
                protocol: "openai-chat".into(),
                context_length: 64000,
                input_cost: Some(1.0),
                output_cost: Some(2.0),
                enabled: true,
                overall_intelligence: Some(80.0),
                coding_index: Some(80.0),
                agentic_index: Some(80.0),
                math_index: Some(80.0),
                output_speed_tps: None,
                time_to_first_token_secs: None,
                created_at: "now".into(),
                updated_at: "now".into(),
                canonical_slug: None,
                hugging_face_id: None,
                created: None,
                description: None,
                architecture: None,
                pricing: None,
                top_provider: None,
                per_request_limits: Some(serde_json::json!({"output_tokens": 1234})),
                supported_parameters: None,
                default_parameters: None,
                supported_voices: None,
                knowledge_cutoff: None,
                expiration_date: None,
                links: None,
            },
        );
    }
    pool
}

#[test]
fn opencode_model_config_includes_identifying_headers() {
    let model = opencode_model_config("CAB auto", "opencode");
    let headers = model
        .get("headers")
        .and_then(|v| v.as_object())
        .expect("headers");
    assert_eq!(
        headers.get("X-CAB-Agent").and_then(|v| v.as_str()),
        Some("opencode")
    );
}

#[tokio::test]
async fn codex_auto_manual_and_native_modes_update_toml_config() {
    let home = TestHome::new();
    let pool = pool_with_models();
    let config_path = home.path(".codex/config.toml");
    fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    fs::write(
        &config_path,
        "model_provider = \"old\"\n[model_providers.old]\nname = \"Old\"\n",
    )
    .unwrap();

    apply_agent_config(&pool, &agent("codex", "auto"), 4567, "gw-key")
        .await
        .unwrap();
    let auto = fs::read_to_string(&config_path).unwrap();
    assert!(auto.contains("model = \"gpt-5.5\""));
    assert!(auto.contains("model_provider = \"cab\""));
    assert!(auto.contains("requires_openai_auth = true"));
    assert!(auto.contains("env_key = \"OPENAI_API_KEY\""));
    assert!(auto.contains("wire_api = \"responses\""));
    assert!(auto.contains("supports_websockets = true"));
    assert!(auto.contains("OPENAI_API_KEY = \"gw-key\""));
    assert!(!auto.contains("ANTHROPIC_BASE_URL"));
    assert!(!auto.contains("ANTHROPIC_AUTH_TOKEN"));

    // Check that auth.json contains the gateway key and Codex-required id_token
    let auth_path = home.path(".codex/auth.json");
    let auth_content = fs::read_to_string(&auth_path).unwrap();
    assert!(auth_content.contains("gw-key"));
    assert!(auth_content.contains("id_token"));

    // Write a dummy key/token to auth.json to verify clean-up/restore logic
    fs::write(
        &auth_path,
        "{\"OPENAI_API_KEY\": \"gw-key\", \"tokens\": {\"access_token\": \"gw-key\"}}",
    )
    .unwrap();

    apply_agent_config(&pool, &agent("codex", "native"), 4567, "gw-key")
        .await
        .unwrap();
    let native = fs::read_to_string(&config_path).unwrap();
    assert!(!native.contains("model_provider = \"cab\""));
    assert!(!auth_path.exists() || !fs::read_to_string(&auth_path).unwrap().contains("gw-key"));
}

#[tokio::test]
async fn grok_build_auto_and_native_modes_update_toml_config() {
    let home = TestHome::new();
    let pool = pool_with_models();
    let config_path = home.path(".grok/config.toml");
    fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    fs::write(
        &config_path,
        "[models]\ndefault = \"grok-build\"\n\n[model.grok-build]\nmodel = \"grok-4.5\"\nname = \"Grok 4.5\"\n",
    )
    .unwrap();

    apply_agent_config(&pool, &agent("grok-build", "auto"), 3125, "gw-key")
        .await
        .unwrap();
    let auto = fs::read_to_string(&config_path).unwrap();
    assert!(auto.contains("default = \"cab-balanced\""));
    assert!(auto.contains("api_backend = \"chat_completions\""));
    assert!(auto.contains("X-CAB-Agent"));
    assert!(auto.contains("gw-key"));
    assert!(auto.contains("cab_backup_default_model = \"grok-build\""));
    assert!(auto.contains("[model.grok-build]"));

    apply_agent_config(&pool, &agent("grok-build", "native"), 3125, "gw-key")
        .await
        .unwrap();
    let native = fs::read_to_string(&config_path).unwrap();
    assert!(native.contains("default = \"grok-build\""));
    assert!(!native.contains("cab-auto"));
    assert!(!native.contains("cab_backup_default_model"));
    assert!(native.contains("[model.grok-build]"));
}
