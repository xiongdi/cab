use crate::InMemoryStore;
use cab_core::types::{CreateModel, Model, UpdateModel};

fn normalize_model(model: &mut Model) {
    cab_core::normalize_legacy_missing_indices(model);
}

pub async fn list(store: &InMemoryStore) -> Result<Vec<Model>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let mut list: Vec<Model> = inner.models.values().cloned().collect();
    drop(inner);
    for model in &mut list {
        normalize_model(model);
    }
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

pub async fn get_by_id(store: &InMemoryStore, id: &str) -> Result<Option<Model>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let mut found = inner.models.get(id).cloned();
    drop(inner);
    if let Some(model) = &mut found {
        normalize_model(model);
    }
    Ok(found)
}

pub async fn get_by_name(store: &InMemoryStore, name: &str) -> Result<Option<Model>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let mut found = inner.models.values().find(|m| m.name == name).cloned();
    drop(inner);
    if let Some(model) = &mut found {
        normalize_model(model);
    }
    Ok(found)
}

pub async fn create(store: &InMemoryStore, input: &CreateModel) -> Result<Model, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    let id = input.name.to_lowercase().replace([' ', '/'], "-");
    let now = chrono::Utc::now().to_rfc3339();
    let model = Model {
        id: id.clone(),
        name: input.name.clone(),
        display_name: input.display_name.clone(),
        provider_id: input.provider_id.clone(),
        protocol: input.protocol.clone(),
        context_length: input.context_length,
        input_cost: input.input_cost,
        output_cost: input.output_cost,
        enabled: input.enabled.unwrap_or(true),
        overall_intelligence: input.overall_intelligence,
        coding_index: input.coding_index,
        agentic_index: input.agentic_index,
        math_index: input.math_index,
        output_speed_tps: input.output_speed_tps,
        time_to_first_token_secs: input.time_to_first_token_secs,
        created_at: now.clone(),
        updated_at: now,
        canonical_slug: input.canonical_slug.clone(),
        hugging_face_id: input.hugging_face_id.clone(),
        created: input.created,
        description: input.description.clone(),
        architecture: input.architecture.clone(),
        pricing: input.pricing.clone(),
        top_provider: input.top_provider.clone(),
        per_request_limits: input.per_request_limits.clone(),
        supported_parameters: input.supported_parameters.clone(),
        default_parameters: input.default_parameters.clone(),
        supported_voices: input.supported_voices.clone(),
        knowledge_cutoff: input.knowledge_cutoff.clone(),
        expiration_date: input.expiration_date.clone(),
        links: input.links.clone(),
    };
    inner.models.insert(id, model.clone());
    drop(inner);
    if let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::sqlite::upsert_catalog_model(&conn, &model)?;
    }
    Ok(model)
}

pub async fn update(
    store: &InMemoryStore,
    id: &str,
    input: &UpdateModel,
) -> Result<Option<Model>, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    if let Some(m) = inner.models.get_mut(id) {
        if let Some(ref name) = input.name {
            m.name = name.clone();
        }
        if let Some(ref display_name) = input.display_name {
            m.display_name = display_name.clone();
        }
        if let Some(ref provider_id) = input.provider_id {
            m.provider_id = provider_id.clone();
        }
        if let Some(ref protocol) = input.protocol {
            m.protocol = protocol.clone();
        }
        if let Some(ref context_length) = input.context_length {
            m.context_length = *context_length;
        }
        if let Some(input_cost) = input.input_cost {
            m.input_cost = input_cost;
        }
        if let Some(output_cost) = input.output_cost {
            m.output_cost = output_cost;
        }
        if let Some(ref enabled) = input.enabled {
            m.enabled = *enabled;
        }
        if let Some(overall_intelligence) = input.overall_intelligence {
            m.overall_intelligence = overall_intelligence;
        }
        if let Some(coding_index) = input.coding_index {
            m.coding_index = coding_index;
        }
        if let Some(agentic_index) = input.agentic_index {
            m.agentic_index = agentic_index;
        }
        if let Some(math_index) = input.math_index {
            m.math_index = math_index;
        }
        if let Some(output_speed_tps) = input.output_speed_tps {
            m.output_speed_tps = output_speed_tps;
        }
        if let Some(time_to_first_token_secs) = input.time_to_first_token_secs {
            m.time_to_first_token_secs = time_to_first_token_secs;
        }
        if let Some(ref canonical_slug) = input.canonical_slug {
            m.canonical_slug = Some(canonical_slug.clone());
        }
        if let Some(ref hugging_face_id) = input.hugging_face_id {
            m.hugging_face_id = Some(hugging_face_id.clone());
        }
        if let Some(ref created) = input.created {
            m.created = Some(*created);
        }
        if let Some(ref description) = input.description {
            m.description = Some(description.clone());
        }
        if let Some(ref architecture) = input.architecture {
            m.architecture = Some(architecture.clone());
        }
        if let Some(ref pricing) = input.pricing {
            m.pricing = Some(pricing.clone());
        }
        if let Some(ref top_provider) = input.top_provider {
            m.top_provider = Some(top_provider.clone());
        }
        if let Some(ref per_request_limits) = input.per_request_limits {
            m.per_request_limits = Some(per_request_limits.clone());
        }
        if let Some(ref supported_parameters) = input.supported_parameters {
            m.supported_parameters = Some(supported_parameters.clone());
        }
        if let Some(ref default_parameters) = input.default_parameters {
            m.default_parameters = Some(default_parameters.clone());
        }
        if let Some(ref supported_voices) = input.supported_voices {
            m.supported_voices = Some(supported_voices.clone());
        }
        if let Some(ref knowledge_cutoff) = input.knowledge_cutoff {
            m.knowledge_cutoff = Some(knowledge_cutoff.clone());
        }
        if let Some(ref expiration_date) = input.expiration_date {
            m.expiration_date = Some(expiration_date.clone());
        }
        if let Some(ref links) = input.links {
            m.links = Some(links.clone());
        }
        m.updated_at = chrono::Utc::now().to_rfc3339();
        let updated = m.clone();
        drop(inner);
        if let Some(pool) = &store.pool {
            let conn = pool.get().map_err(|e| e.to_string())?;
            crate::sqlite::upsert_catalog_model(&conn, &updated)?;
        }
        Ok(Some(updated))
    } else {
        Ok(None)
    }
}

pub async fn delete(store: &InMemoryStore, id: &str) -> Result<bool, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    let removed = inner.models.remove(id).is_some();
    drop(inner);
    if removed && let Some(pool) = &store.pool {
        let conn = pool.get().map_err(|e| e.to_string())?;
        crate::sqlite::delete_catalog_model(&conn, id)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_model(name: &str) -> CreateModel {
        CreateModel {
            name: name.into(),
            display_name: format!("Display {name}"),
            provider_id: "provider-1".into(),
            protocol: "openai-chat".into(),
            context_length: 128000,
            input_cost: Some(1.0),
            output_cost: Some(2.0),
            enabled: Some(true),
            overall_intelligence: Some(80.0),
            coding_index: Some(70.0),
            agentic_index: Some(60.0),
            math_index: Some(50.0),
            canonical_slug: Some("canonical".into()),
            hugging_face_id: Some("hf/model".into()),
            created: Some(123),
            description: Some("description".into()),
            architecture: Some(serde_json::json!({"type": "dense"})),
            pricing: Some(serde_json::json!({"prompt": 1})),
            top_provider: Some(serde_json::json!({"name": "provider"})),
            per_request_limits: Some(serde_json::json!({"max_tokens": 100})),
            supported_parameters: Some(serde_json::json!(["temperature"])),
            default_parameters: Some(serde_json::json!({"temperature": 0.7})),
            supported_voices: Some(serde_json::json!(["alloy"])),
            knowledge_cutoff: Some("2025-01".into()),
            expiration_date: Some("2026-01".into()),
            links: Some(serde_json::json!({"native_model_id": "native"})),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn model_crud_covers_defaults_updates_sorting_and_delete() {
        let store = InMemoryStore::new();

        let full = create(&store, &create_model("Provider/Alpha Model"))
            .await
            .unwrap();
        assert_eq!(full.id, "provider-alpha-model");
        assert_eq!(full.display_name, "Display Provider/Alpha Model");
        assert_eq!(full.overall_intelligence, Some(80.0));
        assert_eq!(full.links.as_ref().unwrap()["native_model_id"], "native");

        let defaults = create(
            &store,
            &CreateModel {
                name: "Beta Model".into(),
                display_name: "Beta".into(),
                provider_id: "provider-1".into(),
                protocol: "anthropic".into(),
                context_length: 4096,
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(defaults.id, "beta-model");
        assert!(defaults.enabled);
        assert!(defaults.overall_intelligence.is_none());
        assert!(defaults.coding_index.is_none());
        assert!(defaults.agentic_index.is_none());
        assert!(defaults.math_index.is_none());

        assert_eq!(
            get_by_id(&store, "provider-alpha-model")
                .await
                .unwrap()
                .unwrap()
                .name,
            "Provider/Alpha Model"
        );
        assert_eq!(
            get_by_name(&store, "Beta Model").await.unwrap().unwrap().id,
            "beta-model"
        );

        let names = list(&store)
            .await
            .unwrap()
            .into_iter()
            .map(|model| model.name)
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["Beta Model", "Provider/Alpha Model"]);

        let updated = update(
            &store,
            "provider-alpha-model",
            &UpdateModel {
                name: Some("Updated".into()),
                display_name: Some("Updated Display".into()),
                provider_id: Some("provider-2".into()),
                protocol: Some("openai-responses".into()),
                context_length: Some(64000),
                input_cost: Some(Some(3.0)),
                output_cost: Some(Some(4.0)),
                enabled: Some(false),
                overall_intelligence: Some(Some(1.0)),
                coding_index: Some(Some(2.0)),
                agentic_index: Some(Some(3.0)),
                math_index: Some(Some(4.0)),
                canonical_slug: Some("new-canonical".into()),
                hugging_face_id: Some("new/hf".into()),
                created: Some(456),
                description: Some("new description".into()),
                architecture: Some(serde_json::json!({"type": "moe"})),
                pricing: Some(serde_json::json!({"completion": 4})),
                top_provider: Some(serde_json::json!({"name": "new"})),
                per_request_limits: Some(serde_json::json!({"max_tokens": 200})),
                supported_parameters: Some(serde_json::json!(["top_p"])),
                default_parameters: Some(serde_json::json!({"top_p": 0.9})),
                supported_voices: Some(serde_json::json!(["echo"])),
                knowledge_cutoff: Some("2026-01".into()),
                expiration_date: Some("2027-01".into()),
                links: Some(serde_json::json!({"native_model_id": "new-native"})),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(updated.name, "Updated");
        assert_eq!(updated.display_name, "Updated Display");
        assert_eq!(updated.provider_id, "provider-2");
        assert_eq!(updated.protocol, "openai-responses");
        assert_eq!(updated.context_length, 64000);
        assert_eq!(updated.input_cost, Some(3.0));
        assert_eq!(updated.output_cost, Some(4.0));
        assert!(!updated.enabled);
        assert_eq!(updated.overall_intelligence, Some(1.0));
        assert_eq!(
            updated.links.as_ref().unwrap()["native_model_id"],
            "new-native"
        );

        assert!(
            update(&store, "missing", &UpdateModel::default())
                .await
                .unwrap()
                .is_none()
        );
        assert!(delete(&store, "provider-alpha-model").await.unwrap());
        assert!(!delete(&store, "provider-alpha-model").await.unwrap());
        assert!(
            get_by_id(&store, "provider-alpha-model")
                .await
                .unwrap()
                .is_none()
        );
    }
}
