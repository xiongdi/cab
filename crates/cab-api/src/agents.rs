use axum::Json;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use cab_core::CabError;
use cab_core::types::UpdateAgent;

use crate::ApiState;

pub async fn list_agents(State(state): State<ApiState>) -> Result<impl IntoResponse, CabError> {
    let agents = cab_db::agent::list(&state.pool)
        .await
        .map_err(CabError::Database)?
        .into_iter()
        .map(normalize_agent_mode)
        .collect::<Vec<_>>();
    Ok(Json(agents))
}

pub async fn get_agent(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, CabError> {
    let agent = cab_db::agent::get_by_id(&state.pool, &id)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| CabError::NotFound(format!("Agent {id} not found")))?;
    Ok(Json(normalize_agent_mode(agent)))
}

pub async fn update_agent(
    State(state): State<ApiState>,
    Path(id): Path<String>,
    Json(input): Json<UpdateAgent>,
) -> Result<impl IntoResponse, CabError> {
    let agent = cab_db::agent::update(&state.pool, &id, &input)
        .await
        .map_err(CabError::Database)?
        .ok_or_else(|| CabError::NotFound(format!("Agent {id} not found")))?;

    let agent = normalize_agent_mode(agent);

    let settings = cab_db::settings::get(&state.pool)
        .await
        .unwrap_or_else(|_| cab_db::settings::default_settings());

    if let Err(e) = cab_services::agent_config::apply_agent_config(
        &state.pool,
        &agent,
        settings.gateway_port,
        &settings.gateway_key,
    )
    .await
    {
        tracing::error!("Failed to write config file for agent {}: {}", agent.id, e);
    }

    Ok(Json(agent))
}

pub(crate) fn normalize_agent_mode(mut agent: cab_core::types::Agent) -> cab_core::types::Agent {
    agent.mode = match agent.mode.as_str() {
        "config" => "auto".to_string(),
        "proxy" => "native".to_string(),
        other => other.to_string(),
    };
    agent
}

pub async fn sync_all_agent_configs(pool: &cab_db::InMemoryStore) -> Result<(), CabError> {
    let settings = cab_db::settings::get(pool)
        .await
        .map_err(CabError::Database)?;
    let agents = cab_db::agent::list(pool)
        .await
        .map_err(CabError::Database)?;
    for agent in agents {
        let agent = normalize_agent_mode(agent);
        if let Err(e) = cab_services::agent_config::apply_agent_config(
            pool,
            &agent,
            settings.gateway_port,
            &settings.gateway_key,
        )
        .await
        {
            tracing::error!("Failed to sync config file for agent {}: {}", agent.id, e);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_agent(mode: &str) -> cab_core::types::Agent {
        cab_core::types::Agent {
            id: "codex".to_string(),
            name: "Codex".to_string(),
            mode: mode.to_string(),
            model_id: None,
            api_key: String::new(),
            endpoint: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn normalize_maps_config_to_auto() {
        let agent = normalize_agent_mode(sample_agent("config"));
        assert_eq!(agent.mode, "auto");
    }

    #[test]
    fn normalize_maps_proxy_to_native() {
        let agent = normalize_agent_mode(sample_agent("proxy"));
        assert_eq!(agent.mode, "native");
    }

    #[test]
    fn normalize_preserves_supported_modes() {
        for mode in ["native", "auto", "manual"] {
            let agent = normalize_agent_mode(sample_agent(mode));
            assert_eq!(agent.mode, mode);
        }
    }
}
