use super::*;
use crate::jobs::AdmissionContext;
use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::post,
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};
use uuid::Uuid;

const PRIMARY: &str = "gemini-3.5-flash-lite";
const FALLBACK: &str = "gemini-4-hydrogen";
const KEY: &str = "synthetic-key-not-a-provider-secret";
#[derive(Clone)]
struct Mock {
    replies: Arc<Mutex<VecDeque<(StatusCode, Value)>>>,
    requests: Arc<Mutex<Vec<(HeaderMap, Value)>>>,
    delay: Duration,
    retry_after: Option<String>,
}
struct Running {
    adapter: GeminiAdapter,
    state: Mock,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Running {
    fn drop(&mut self) {
        self.task.abort();
    }
}
fn settings() -> GeminiSettings {
    GeminiSettings {
        primary_model: PRIMARY.into(),
        fallback_model: Some(FALLBACK.into()),
        profile: GeminiProfile::FreeTesting,
        request_timeout: Duration::from_secs(3),
        max_output_tokens: 4096,
        attempt_budget: 2,
    }
}
async fn start(
    replies: Vec<(StatusCode, Value)>,
    cfg: GeminiSettings,
    delay: Duration,
    retry_after: Option<&str>,
) -> Running {
    let state = Mock {
        replies: Arc::new(Mutex::new(replies.into())),
        requests: Arc::new(Mutex::new(Vec::new())),
        delay,
        retry_after: retry_after.map(str::to_owned),
    };
    let app =
        Router::new()
            .route(
                "/interactions",
                post(
                    |State(s): State<Mock>,
                     headers: HeaderMap,
                     axum::Json(body): axum::Json<Value>| async move {
                        s.requests.lock().unwrap().push((headers, body));
                        let reply = s
                            .replies
                            .lock()
                            .unwrap()
                            .pop_front()
                            .unwrap_or((StatusCode::INTERNAL_SERVER_ERROR, json!({})));
                        tokio::time::sleep(s.delay).await;
                        let mut response = (reply.0, axum::Json(reply.1)).into_response();
                        if let Some(delay) = &s.retry_after {
                            response
                                .headers_mut()
                                .insert("retry-after", delay.parse().unwrap());
                        }
                        response
                    },
                ),
            )
            .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}/interactions", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    Running {
        adapter: GeminiAdapter::build(KEY, cfg, endpoint, false).unwrap(),
        state,
        task,
    }
}
fn context() -> ReviewContext {
    let catalog = labyrinth_engine::catalog::load_yaml(include_str!(
        "../../../../tests/fixtures/catalog/blueprints.yaml"
    ))
    .unwrap();
    ReviewContext::new(
        AdmissionContext {
            actor_id: Uuid::from_u128(1),
            tenant_id: Uuid::from_u128(2),
            project_id: Uuid::from_u128(3),
            expected_head: None,
        },
        &catalog,
        &Default::default(),
    )
    .unwrap()
}
fn complete(model: &str) -> Value {
    json!({"model":model,"status":"completed","steps":[{"type":"model_output","content":[{"type":"text","text":include_str!("../../../../tests/fixtures/ai/incomplete-proposal.json")}]}],"usage":{"total_input_tokens":50,"total_output_tokens":20,"total_tokens":75,"total_thought_tokens":5}})
}
fn unavailable() -> (StatusCode, Value) {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        json!({"error":{"code":503,"status":"UNAVAILABLE","message":"never relay provider secrets or prompts"}}),
    )
}
#[tokio::test]
async fn request_is_stateless_tool_free_and_metadata_does_not_leak_key_or_context() {
    let s = start(
        vec![(StatusCode::OK, complete(PRIMARY))],
        settings(),
        Duration::ZERO,
        None,
    )
    .await;
    let result = s
        .adapter
        .propose(
            "Public test only",
            context(),
            PromptDataClass::PublicSynthetic,
        )
        .await
        .unwrap();
    assert!(result.model_switch.is_none());
    assert_eq!(result.attempts.len(), 1);
    assert_eq!(
        result.attempts[0].usage.as_ref().unwrap().input_tokens,
        Some(50)
    );
    assert_eq!(
        result.attempts[0].usage.as_ref().unwrap().thought_tokens,
        Some(5)
    );
    let view = serde_json::to_value(result.proposal.view()).unwrap();
    assert_eq!(view["provider"]["model"], PRIMARY);
    assert_eq!(view["provider"]["profile"], "free_testing");
    let requests = s.state.requests.lock().unwrap();
    assert_eq!(requests[0].0["x-goog-api-key"], KEY);
    let body = &requests[0].1;
    assert_eq!(body["store"], false);
    assert_eq!(body["background"], false);
    assert_eq!(body["stream"], false);
    assert_eq!(body["tools"], json!([]));
    assert_eq!(body["generation_config"]["tool_choice"], "none");
    assert_eq!(body["input"], "Public test only");
    assert!(!body.to_string().contains(&Uuid::from_u128(1).to_string()));
    assert!(!format!("{:?}", s.adapter).contains(KEY));
}
#[tokio::test]
async fn unavailable_primary_switches_once_and_returns_truthful_notice() {
    let s = start(
        vec![unavailable(), (StatusCode::OK, complete(FALLBACK))],
        settings(),
        Duration::ZERO,
        None,
    )
    .await;
    let result = s
        .adapter
        .propose("Public test", context(), PromptDataClass::PublicSynthetic)
        .await
        .unwrap();
    let notice = result.model_switch.unwrap();
    assert_eq!(notice.from_model, PRIMARY);
    assert_eq!(notice.to_model, FALLBACK);
    assert_eq!(notice.reason, GeminiErrorCode::ProviderUnavailable);
    assert!(notice.message.contains(PRIMARY) && notice.message.contains(FALLBACK));
    assert!(!notice.message.contains("secrets"));
    assert_eq!(
        serde_json::to_value(result.proposal.view()).unwrap()["provider"]["model"],
        FALLBACK
    );
    assert_eq!(result.attempts.len(), 2);
    assert!(result.attempts[0].usage.is_none());
    assert_eq!(s.state.requests.lock().unwrap().len(), 2);
}
#[tokio::test]
async fn both_failures_keep_switch_notice_and_do_not_recursively_retry() {
    let s = start(
        vec![unavailable(), unavailable()],
        settings(),
        Duration::ZERO,
        None,
    )
    .await;
    let error = s
        .adapter
        .propose("Public test", context(), PromptDataClass::PublicSynthetic)
        .await
        .unwrap_err();
    assert_eq!(error.code, GeminiErrorCode::ProviderUnavailable);
    assert!(error.model_switch.is_some());
    assert_eq!(error.attempts.len(), 2);
    assert_eq!(s.state.requests.lock().unwrap().len(), 2);
    assert!(!format!("{error:?}").contains("secrets"));
}
#[tokio::test]
async fn private_input_is_denied_by_free_profile_before_dispatch() {
    let s = start(vec![], settings(), Duration::ZERO, None).await;
    let error = s
        .adapter
        .propose("Private", context(), PromptDataClass::PrivateProject)
        .await
        .unwrap_err();
    assert_eq!(error.code, GeminiErrorCode::PolicyDenied);
    assert!(s.state.requests.lock().unwrap().is_empty());
}
#[tokio::test]
async fn deadline_and_retry_after_never_create_extra_dispatches() {
    let mut cfg = settings();
    cfg.request_timeout = Duration::from_millis(100);
    let s = start(vec![unavailable()], cfg, Duration::from_millis(500), None).await;
    let error = s
        .adapter
        .propose("Public", context(), PromptDataClass::PublicSynthetic)
        .await
        .unwrap_err();
    assert_eq!(error.code, GeminiErrorCode::DeadlineExceeded);
    assert!(error.model_switch.is_none());
    assert!(s.state.requests.lock().unwrap().len() <= 1);
    for delay in ["999999", "Wed, 21 Oct 2037 07:28:00 GMT"] {
        let s = start(vec![unavailable()], settings(), Duration::ZERO, Some(delay)).await;
        let error = s
            .adapter
            .propose("Public", context(), PromptDataClass::PublicSynthetic)
            .await
            .unwrap_err();
        assert!(error.model_switch.is_none());
        assert_eq!(s.state.requests.lock().unwrap().len(), 1);
    }
}
#[tokio::test]
async fn invalid_responses_and_safety_or_protocol_failures_do_not_trigger_fallback() {
    let mut malformed = complete(PRIMARY);
    malformed["steps"][0]["content"][0]["text"] = json!("not-json");
    let mut tool = complete(PRIMARY);
    tool["steps"][0] = json!({"type":"function_call","name":"generate"});
    let mut wrong_model = complete(PRIMARY);
    wrong_model["model"] = json!("gemini-unexpected");
    let mut incomplete = complete(PRIMARY);
    incomplete["status"] = json!("in_progress");
    for body in [malformed, tool, wrong_model, incomplete] {
        let s = start(
            vec![(StatusCode::OK, body)],
            settings(),
            Duration::ZERO,
            None,
        )
        .await;
        let error = s
            .adapter
            .propose("Public", context(), PromptDataClass::PublicSynthetic)
            .await
            .unwrap_err();
        assert!(error.model_switch.is_none());
        assert_eq!(s.state.requests.lock().unwrap().len(), 1);
    }
    for status in [
        StatusCode::UNAUTHORIZED,
        StatusCode::FORBIDDEN,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::FOUND,
    ] {
        let s = start(vec![(status, json!({}))], settings(), Duration::ZERO, None).await;
        assert!(
            s.adapter
                .propose("Public", context(), PromptDataClass::PublicSynthetic)
                .await
                .is_err()
        );
        assert_eq!(s.state.requests.lock().unwrap().len(), 1);
    }
}
#[tokio::test]
async fn attempts_are_budgeted_and_profiles_are_bound_to_review_identity() {
    let mut cfg = settings();
    cfg.attempt_budget = 1;
    let s = start(vec![unavailable()], cfg, Duration::ZERO, None).await;
    let error = s
        .adapter
        .propose("Public", context(), PromptDataClass::PublicSynthetic)
        .await
        .unwrap_err();
    assert_eq!(error.attempts.len(), 1);
    assert!(error.model_switch.is_none());
    let mut ids = Vec::new();
    for profile in [GeminiProfile::FreeTesting, GeminiProfile::PaidPrivate] {
        let mut cfg = settings();
        cfg.profile = profile;
        let s = start(
            vec![(StatusCode::OK, complete(PRIMARY))],
            cfg,
            Duration::ZERO,
            None,
        )
        .await;
        ids.push(
            s.adapter
                .propose("Public", context(), PromptDataClass::PublicSynthetic)
                .await
                .unwrap()
                .proposal
                .digest()
                .to_owned(),
        );
    }
    assert_ne!(ids[0], ids[1]);
}
#[test]
fn transport_validation_rejects_invalid_keys_models_limits_and_duplicate_targets() {
    assert!(GeminiAdapter::new("", settings()).is_err());
    assert!(GeminiAdapter::new("key\nsecret", settings()).is_err());
    for bad in 0..4 {
        let mut cfg = settings();
        match bad {
            0 => cfg.primary_model = "https://invalid.example".into(),
            1 => cfg.fallback_model = Some(PRIMARY.into()),
            2 => cfg.max_output_tokens = 0,
            _ => cfg.attempt_budget = 3,
        }
        assert!(GeminiAdapter::new(KEY, cfg).is_err());
    }
}
#[test]
fn wire_parser_rejects_duplicate_fields_multiple_outputs_and_legacy_shapes() {
    let mut many = complete(PRIMARY);
    let block = many["steps"][0]["content"][0].clone();
    many["steps"][0]["content"]
        .as_array_mut()
        .unwrap()
        .push(block);
    assert!(wire::parse_response(&serde_json::to_vec(&many).unwrap(), PRIMARY).is_err());
    let raw =
        format!(r#"{{"model":"{PRIMARY}","model":"{PRIMARY}","status":"completed","steps":[]}}"#);
    assert!(wire::parse_response(raw.as_bytes(), PRIMARY).is_err());
    assert!(
        wire::parse_response(
            br#"{"model":"gemini-test","status":"completed","outputs":[]}"#,
            "gemini-test"
        )
        .is_err()
    );
    let mut no_usage = complete(PRIMARY);
    no_usage.as_object_mut().unwrap().remove("usage");
    assert!(
        wire::parse_response(&serde_json::to_vec(&no_usage).unwrap(), PRIMARY)
            .unwrap()
            .usage
            .is_none()
    );
}

#[tokio::test]
async fn explicit_model_and_quota_failures_use_one_eligible_fallback() {
    for (http, status, reason) in [
        (
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
            GeminiErrorCode::ModelUnavailable,
        ),
        (
            StatusCode::TOO_MANY_REQUESTS,
            "RESOURCE_EXHAUSTED",
            GeminiErrorCode::RateLimited,
        ),
    ] {
        let body = json!({"error":{"code":http.as_u16(),"status":status}});
        let s = start(
            vec![(http, body), (StatusCode::OK, complete(FALLBACK))],
            settings(),
            Duration::ZERO,
            Some("0"),
        )
        .await;
        let result = s
            .adapter
            .propose("Public", context(), PromptDataClass::PublicSynthetic)
            .await
            .unwrap();
        assert_eq!(result.model_switch.unwrap().reason, reason);
        assert_eq!(result.attempts.len(), 2);
    }
    let mut denied = unavailable();
    denied.1["error"]["details"] =
        json!([{"@type":"type.googleapis.com/google.rpc.RetryInfo","retryDelay":"90s"}]);
    let s = start(vec![denied], settings(), Duration::ZERO, None).await;
    let result = s
        .adapter
        .propose("Public", context(), PromptDataClass::PublicSynthetic)
        .await
        .unwrap_err();
    assert!(result.model_switch.is_none());
    assert_eq!(result.attempts.len(), 1);
}
#[tokio::test]
async fn response_size_is_bounded_for_declared_and_chunked_bodies() {
    let huge = json!({"payload":"x".repeat(MAX_RESPONSE_BYTES)});
    let s = start(
        vec![(StatusCode::OK, huge)],
        settings(),
        Duration::ZERO,
        None,
    )
    .await;
    assert_eq!(
        s.adapter
            .propose("Public", context(), PromptDataClass::PublicSynthetic)
            .await
            .unwrap_err()
            .code,
        GeminiErrorCode::ResponseTooLarge
    );
    let app = Router::new().route(
        "/interactions",
        post(|| async {
            let chunks = vec![
                Ok::<_, std::convert::Infallible>(vec![b'x'; MAX_RESPONSE_BYTES / 2 + 1]),
                Ok(vec![b'x'; MAX_RESPONSE_BYTES / 2 + 1]),
            ];
            axum::body::Body::from_stream(tokio_stream::iter(chunks))
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/interactions", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let adapter = GeminiAdapter::build(KEY, settings(), url, false).unwrap();
    let result = adapter
        .propose("Public", context(), PromptDataClass::PublicSynthetic)
        .await;
    task.abort();
    assert_eq!(result.unwrap_err().code, GeminiErrorCode::ResponseTooLarge);
}
#[tokio::test]
async fn redirects_cannot_forward_the_api_key_to_another_route() {
    let hits = Arc::new(Mutex::new(0));
    let copied = hits.clone();
    let app = Router::new()
        .route(
            "/interactions",
            post(|| async { (StatusCode::FOUND, [("location", "/capture")]) }),
        )
        .route(
            "/capture",
            axum::routing::any(move || {
                let hits = copied.clone();
                async move {
                    *hits.lock().unwrap() += 1;
                    "unexpected"
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/interactions", listener.local_addr().unwrap())
        .parse()
        .unwrap();
    let task = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let adapter = GeminiAdapter::build(KEY, settings(), url, false).unwrap();
    let result = adapter
        .propose("Public", context(), PromptDataClass::PublicSynthetic)
        .await;
    task.abort();
    assert!(result.is_err());
    assert_eq!(*hits.lock().unwrap(), 0);
}
#[tokio::test]
async fn complete_provider_output_still_requires_review_and_normal_preparation() {
    let mut body = complete(PRIMARY);
    let intent: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/intents/enterprise.json"
    ))
    .unwrap();
    body["steps"][0]["content"][0]["text"] = json!(
        json!({"schema_version":"ai_proposal_v1","intent":intent,"assumptions":[]}).to_string()
    );
    let mut cfg = settings();
    cfg.profile = GeminiProfile::PaidPrivate;
    let s = start(vec![(StatusCode::OK, body)], cfg, Duration::ZERO, None).await;
    let result = s
        .adapter
        .propose(
            "Synthetic fixture labelled private to exercise the gate",
            context(),
            PromptDataClass::PrivateProject,
        )
        .await
        .unwrap();
    assert!(matches!(
        result.proposal.readiness(),
        crate::ai::ProposalReadiness::ReadyForReview
    ));
    let digest = result.proposal.digest().to_owned();
    let confirmed = result
        .proposal
        .confirm(
            &context(),
            crate::ai::Confirmation {
                schema_version: "ai_confirmation_v1".into(),
                proposal_sha256: digest,
            },
        )
        .unwrap();
    let catalog = labyrinth_engine::catalog::load_yaml(include_str!(
        "../../../../tests/fixtures/catalog/blueprints.yaml"
    ))
    .unwrap();
    let admission = AdmissionContext {
        actor_id: Uuid::from_u128(1),
        tenant_id: Uuid::from_u128(2),
        project_id: Uuid::from_u128(3),
        expected_head: None,
    };
    assert!(
        crate::jobs::PreparedAdmission::new(
            admission,
            "reviewed",
            confirmed.raw_intent(),
            &Default::default(),
            &catalog
        )
        .is_ok()
    );
}

#[tokio::test]
async fn cancelling_the_call_prevents_a_later_fallback_dispatch() {
    let s = start(
        vec![unavailable(), (StatusCode::OK, complete(FALLBACK))],
        settings(),
        Duration::from_millis(300),
        None,
    )
    .await;
    let adapter = GeminiAdapter::build(KEY, settings(), s.adapter.endpoint.clone(), false).unwrap();
    let task = tokio::spawn(async move {
        adapter
            .propose("Public", context(), PromptDataClass::PublicSynthetic)
            .await
    });
    tokio::time::timeout(Duration::from_secs(2), async {
        while s.state.requests.lock().unwrap().is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(s.state.requests.lock().unwrap().len(), 1);
}
