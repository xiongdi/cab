use cab_core::routing::{
    RouteCandidate, RoutingStrategy, build_request_profile, rank_models, rank_route_candidates,
};
use cab_core::types::Model;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use serde_json::json;

fn sample_model(name: &str, scores: (f64, f64, f64, f64)) -> Model {
    Model {
        id: name.into(),
        name: name.into(),
        display_name: name.into(),
        provider_id: "p1".into(),
        protocol: "openai-chat".into(),
        context_length: 128_000,
        input_cost: Some(1.0),
        output_cost: Some(2.0),
        enabled: true,
        overall_intelligence: Some(scores.0),
        coding_index: Some(scores.1),
        agentic_index: Some(scores.2),
        math_index: Some(scores.3),
        output_speed_tps: Some(100.0),
        time_to_first_token_secs: Some(0.5),
        created_at: String::new(),
        updated_at: String::new(),
        canonical_slug: None,
        hugging_face_id: None,
        created: None,
        description: None,
        architecture: None,
        pricing: None,
        top_provider: None,
        per_request_limits: None,
        supported_parameters: None,
        default_parameters: None,
        supported_voices: None,
        knowledge_cutoff: None,
        expiration_date: None,
        links: None,
    }
}

fn make_models(n: usize) -> Vec<Model> {
    let mut models = Vec::with_capacity(n);
    for i in 0..n {
        let name = format!("model-{i}");
        let intelligence = 40.0 + (i as f64 % 50.0);
        let coding = 35.0 + (i as f64 % 55.0);
        let agentic = 30.0 + (i as f64 % 45.0);
        let math = 30.0 + (i as f64 % 50.0);
        models.push(sample_model(&name, (intelligence, coding, agentic, math)));
    }
    models
}

fn make_candidates(n: usize) -> Vec<RouteCandidate<'static>> {
    let models = make_models(n);
    let leaked: &'static [Model] = Box::leak(models.into_boxed_slice());
    leaked
        .iter()
        .map(|m| RouteCandidate {
            model: m,
            service_provider_id: "sp1",
            input_cost: 1.0,
            output_cost: 2.0,
            cache_read_cost: None,
        })
        .collect()
}

fn small_body() -> serde_json::Value {
    json!({
        "model": "auto",
        "messages": [{"role": "user", "content": "hello"}]
    })
}

fn large_body() -> serde_json::Value {
    let code = "```rust\nfn main() {\n".repeat(200);
    let tools: Vec<_> = (0..20)
        .map(|i| json!({"name": format!("tool_{i}"), "description": "d", "input_schema": {"type": "object"}}))
        .collect();
    let messages: Vec<_> = (0..50)
        .map(|i| json!({"role": if i % 2 == 0 { "user" } else { "assistant" }, "content": format!("Message {i}: {code}")}))
        .collect();
    json!({
        "model": "auto",
        "messages": messages,
        "tools": tools,
        "max_tokens": 4096,
    })
}

fn bench_build_profile(c: &mut Criterion) {
    let small = small_body();
    let large = large_body();

    c.bench_function("build_request_profile/small", |b| {
        b.iter(|| build_request_profile(&small, "claude-code"));
    });

    c.bench_function("build_request_profile/large", |b| {
        b.iter(|| build_request_profile(&large, "claude-code"));
    });
}

fn bench_rank_models(c: &mut Criterion) {
    let profile = build_request_profile(&small_body(), "claude-code");

    for n in [10, 50, 100, 200] {
        let models = make_models(n);
        c.bench_with_input(
            BenchmarkId::new("rank_models/balanced", n),
            &models,
            |b, models| {
                b.iter(|| rank_models(models, RoutingStrategy::Balanced, &profile));
            },
        );
        c.bench_with_input(
            BenchmarkId::new("rank_models/auto", n),
            &models,
            |b, models| {
                b.iter(|| rank_models(models, RoutingStrategy::Auto, &profile));
            },
        );
    }
}

fn bench_rank_candidates(c: &mut Criterion) {
    let profile = build_request_profile(&small_body(), "claude-code");

    for n in [10, 50, 100, 200] {
        let candidates = make_candidates(n);
        c.bench_with_input(
            BenchmarkId::new("rank_candidates/balanced", n),
            &candidates,
            |b, cands| {
                b.iter(|| rank_route_candidates(cands, RoutingStrategy::Balanced, &profile));
            },
        );
    }
}

fn bench_full_routing_decision(c: &mut Criterion) {
    let small = small_body();
    let large = large_body();
    let models = make_models(100);

    c.bench_function("full_routing/small_body_100_models", |b| {
        b.iter(|| {
            let profile = build_request_profile(&small, "claude-code");
            rank_models(&models, RoutingStrategy::Auto, &profile);
        });
    });

    c.bench_function("full_routing/large_body_100_models", |b| {
        b.iter(|| {
            let profile = build_request_profile(&large, "claude-code");
            rank_models(&models, RoutingStrategy::Auto, &profile);
        });
    });
}

criterion_group!(
    benches,
    bench_build_profile,
    bench_rank_models,
    bench_rank_candidates,
    bench_full_routing_decision,
);
criterion_main!(benches);
