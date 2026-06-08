use crate::InMemoryStore;
use cab_core::types::{CreateModel, Model, UpdateModel};

pub async fn list(store: &InMemoryStore) -> Result<Vec<Model>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let mut list: Vec<Model> = inner.models.values().cloned().collect();
    list.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(list)
}

pub async fn get_by_id(store: &InMemoryStore, id: &str) -> Result<Option<Model>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    Ok(inner.models.get(id).cloned())
}

pub async fn get_by_name(store: &InMemoryStore, name: &str) -> Result<Option<Model>, String> {
    let inner = store.inner.read().map_err(|e| e.to_string())?;
    let found = inner.models.values().find(|m| m.name == name).cloned();
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
        overall_intelligence: input.overall_intelligence.unwrap_or(30.0),
        coding_index: input.coding_index.unwrap_or(24.0),
        agentic_index: input.agentic_index.unwrap_or(36.0),
        math_index: input.math_index.unwrap_or(30.0),
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
        if let Some(ref input_cost) = input.input_cost {
            m.input_cost = Some(*input_cost);
        }
        if let Some(ref output_cost) = input.output_cost {
            m.output_cost = Some(*output_cost);
        }
        if let Some(ref enabled) = input.enabled {
            m.enabled = *enabled;
        }
        if let Some(ref overall_intelligence) = input.overall_intelligence {
            m.overall_intelligence = *overall_intelligence;
        }
        if let Some(ref coding_index) = input.coding_index {
            m.coding_index = *coding_index;
        }
        if let Some(ref agentic_index) = input.agentic_index {
            m.agentic_index = *agentic_index;
        }
        if let Some(ref math_index) = input.math_index {
            m.math_index = *math_index;
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
        Ok(Some(m.clone()))
    } else {
        Ok(None)
    }
}

pub async fn delete(store: &InMemoryStore, id: &str) -> Result<bool, String> {
    let mut inner = store.inner.write().map_err(|e| e.to_string())?;
    Ok(inner.models.remove(id).is_some())
}
