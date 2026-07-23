//! enma-server — thin, independently-deployed HTTP/MCP wrapper around the
//! `enma` decisions lib. Its own deploy unit (own systemd service, own port).
//! Boundary-clean: no mcpbox dependency; the platform→tool auth contract and
//! the axum/tokio scaffold live in `layer_kit::{auth,serve}`.
//!
//! Routes:
//!   GET  /healthz   — open; liveness + version for the platform registry.
//!   POST /v1/mcp    — requires a valid platform token; decisions surface
//!                     (`enma.decide` builds a typed Decision via the lib).
//!
//! Env: ENMA_PORT (default 8092), ENMA_PLATFORM_SECRET (HMAC key; if unset,
//! /v1/mcp is closed), ENMA_VERSION, and the optional OPENAI_* fallback.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::http::StatusCode;
use enma::{Actor, Decision, Timestamp};
use layer_kit::ai::extract_ai_config;
use layer_kit::auth::Claims;
use layer_kit::openai::{AiConfig, OpenAiProvider};
use layer_kit::serve::{serve, McpHandler, ServeConfig};
use layer_kit::store::Store;
use serde_json::json;

const TOOL: &str = "enma";

/// Dispatches enma's MCP methods and owns the optional env fallback provider.
struct Handler {
    ai: Option<OpenAiProvider>,
    store: Store,
}

impl McpHandler for Handler {
    async fn dispatch(
        &self,
        _claims: &Claims,
        method: &str,
        mut params: serde_json::Value,
    ) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
        if let Some(cfg) = extract_ai_config(&mut params) {
            let provider = OpenAiProvider::new(cfg);
            dispatch(
                &self.store,
                Some(&provider),
                Some(provider.model()),
                method,
                params,
            )
            .await
        } else {
            dispatch(&self.store, self.ai.as_ref(), None, method, params).await
        }
    }

    fn tools(&self) -> Vec<serde_json::Value> {
        tools()
    }
}

/// Tool descriptors for `tools/list` — one per method actually handled by
/// [`dispatch`].
fn tools() -> Vec<serde_json::Value> {
    let mut tools = vec![json!({
        "name": "enma_decide",
        "description": "Build a typed Decision from a decision statement, optionally linked to an upstream sensing item.",
        "inputSchema": {
            "type": "object",
            "properties": {
                "statement": {"type": "string"},
                "source_ref": {"type": "string"},
                "decided_by": {"type": "object"}
            },
            "required": ["statement"]
        }
    })];
    for (name, description) in [
        ("enma_list", "List persisted Decisions."),
        ("enma_list_decisions", "List persisted Decisions."),
        ("enma_get", "Get a persisted Decision by id."),
        ("enma_get_decision", "Get a persisted Decision by id."),
    ] {
        let get = name.contains("get");
        tools.push(json!({
            "name": name,
            "description": description,
            "inputSchema": if get {
                json!({"type": "object", "properties": {"id": {"type": "string"}}, "required": ["id"]})
            } else {
                json!({"type": "object", "properties": {"limit": {"type": "integer", "minimum": 1}}})
            }
        }));
    }
    tools
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().json().init();
    let store = Store::from_env(TOOL).await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "failed to open enma store");
        std::process::exit(1);
    });

    serve(
        ServeConfig {
            tool: TOOL,
            default_port: 8092,
            default_version: env!("CARGO_PKG_VERSION"),
            git_sha: option_env!("GIT_SHA").unwrap_or("dev"),
        },
        Handler {
            ai: AiConfig::from_env().map(OpenAiProvider::new),
            store,
        },
    )
    .await;
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn now_timestamp() -> Timestamp {
    (UNIX_EPOCH + Duration::from_secs(now_secs() as u64)).into()
}

/// Params for `enma.decide`.
#[derive(serde::Deserialize)]
struct DecideParams {
    statement: String,
    /// Optional provenance: upstream SensingItem id, recorded as a typed
    /// Sensemaking link so lineage survives the network hop.
    #[serde(default)]
    source_ref: Option<String>,
    #[serde(default)]
    sensing_item: Option<serde_json::Value>,
    #[serde(default)]
    decided_by: Option<Actor>,
}

#[derive(serde::Deserialize)]
struct ListParams {
    #[serde(default = "default_limit")]
    limit: i64,
}

#[derive(serde::Deserialize)]
struct GetParams {
    id: String,
}

fn default_limit() -> i64 {
    100
}

fn storage_error(e: impl std::fmt::Display) -> (StatusCode, serde_json::Value) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        json!({"error": "storage_error", "detail": e.to_string()}),
    )
}

/// Pure MCP dispatch over the enma decisions lib — no auth, no HTTP, so it is
/// unit-testable directly. Decisions are persisted before success is returned.
async fn dispatch<P: enma::AiProvider>(
    store: &Store,
    ai: Option<&P>,
    model: Option<&str>,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
    match method {
        "enma.decide" => {
            let p: DecideParams = serde_json::from_value(params).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    json!({"error": "invalid_params", "detail": e.to_string()}),
                )
            })?;
            if let Some(model) = model {
                let provider = ai.ok_or_else(|| {
                    (
                        StatusCode::SERVICE_UNAVAILABLE,
                        json!({"error": "ai_not_configured", "detail": "AI provider is not configured"}),
                    )
                })?;
                let (decision, usage) = enma::decide_ai(
                    provider,
                    &p.statement,
                    p.source_ref,
                    p.decided_by.unwrap_or_else(Actor::user),
                    now_timestamp(),
                )
                .await
                .map_err(|e| {
                    (
                        StatusCode::BAD_GATEWAY,
                        json!({"error": "ai_error", "detail": e.to_string()}),
                    )
                })?;
                store
                    .put("decision", &decision.id.as_uuid().to_string(), &decision)
                    .await
                    .map_err(storage_error)?;
                let mut meta = json!({"model": model});
                if let Some(usage) = usage {
                    meta["usage"] = json!(usage);
                }
                let mut out =
                    json!({ "method": "enma.decide", "decision": decision, "_meta": meta });
                if let Some(sensing_item) = p.sensing_item {
                    out["sensing_item"] = sensing_item;
                }
                return Ok(out);
            }
            let decision = enma::decision_from_sensing(
                p.statement,
                p.source_ref,
                p.decided_by.unwrap_or_else(Actor::user),
                now_timestamp(),
            )
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    json!({"error": "invalid_params", "detail": e.to_string()}),
                )
            })?;
            store
                .put("decision", &decision.id.as_uuid().to_string(), &decision)
                .await
                .map_err(storage_error)?;
            let mut out = json!({ "method": "enma.decide", "decision": decision });
            if let Some(sensing_item) = p.sensing_item {
                out["sensing_item"] = sensing_item;
            }
            Ok(out)
        }
        "enma.list" | "enma.list_decisions" => {
            let p: ListParams = serde_json::from_value(params).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    json!({"error": "invalid_params", "detail": e.to_string()}),
                )
            })?;
            let decisions: Vec<Decision> = store
                .list("decision", p.limit)
                .await
                .map_err(storage_error)?;
            Ok(json!({"method": method, "decisions": decisions}))
        }
        "enma.get" | "enma.get_decision" => {
            let p: GetParams = serde_json::from_value(params).map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    json!({"error": "invalid_params", "detail": e.to_string()}),
                )
            })?;
            let decision: Option<Decision> =
                store.get("decision", &p.id).await.map_err(storage_error)?;
            decision
                .map(|decision| json!({"method": method, "decision": decision}))
                .ok_or_else(|| {
                    (
                        StatusCode::NOT_FOUND,
                        json!({"error": "not_found", "detail": p.id}),
                    )
                })
        }
        other => Err((
            StatusCode::BAD_REQUEST,
            json!({"error": "unknown_method", "detail": other}),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enma::{AiError, AiOutput, AiRequest, AiUsage, ToolCall};
    use std::sync::atomic::{AtomicU64, Ordering};

    static DB_SEQ: AtomicU64 = AtomicU64::new(1);

    fn db_path() -> String {
        std::env::temp_dir()
            .join(format!(
                "enma-server-{}-{}.db",
                std::process::id(),
                DB_SEQ.fetch_add(1, Ordering::Relaxed)
            ))
            .to_string_lossy()
            .into_owned()
    }

    async fn test_store() -> Store {
        Store::open(&db_path()).await.unwrap()
    }

    async fn dispatch<P: enma::AiProvider>(
        ai: Option<&P>,
        request_ai: bool,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
        super::dispatch(
            &test_store().await,
            ai,
            request_ai.then_some("test"),
            method,
            params,
        )
        .await
    }

    struct Fake(Result<Vec<AiOutput>, AiError>);

    impl enma::AiProvider for Fake {
        async fn respond(&self, _req: AiRequest) -> Result<Vec<AiOutput>, AiError> {
            self.0.clone()
        }

        async fn respond_with_usage(
            &self,
            _req: AiRequest,
        ) -> Result<(Vec<AiOutput>, Option<AiUsage>), AiError> {
            Ok((self.0.clone()?, Some(AiUsage {
                input_tokens: Some(123),
                output_tokens: Some(45),
                total_tokens: Some(168),
            })))
        }
    }

    #[tokio::test]
    async fn decide_builds_decision_with_sensemaking_provenance() {
        let out = dispatch(
            None::<&OpenAiProvider>,
            false,
            "enma.decide",
            json!({
                "statement": "Use Postgres for the primary store",
                "source_ref": "sense_abc"
            }),
        )
        .await
        .expect("decide must succeed");
        let decision = &out["decision"];
        assert_eq!(decision["statement"], "Use Postgres for the primary store");
        assert!(
            decision["id"].as_str().is_some(),
            "Decision must carry an id"
        );
        assert_eq!(decision["links"][0]["kind"], "sensemaking");
        assert_eq!(decision["links"][0]["reference"], "sense_abc");
        assert!(out.get("_meta").is_none());
    }

    #[tokio::test]
    async fn read_methods_and_unknown_method_rejected() {
        let out = dispatch(None::<&OpenAiProvider>, false, "enma.list", json!({}))
            .await
            .unwrap();
        assert_eq!(out["decisions"], json!([]));
        let (code, _) = dispatch(None::<&OpenAiProvider>, false, "enma.nope", json!({}))
            .await
            .unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn decision_persists_across_restart_and_write_errors_surface() {
        let path = db_path();
        let store = Store::open(&path).await.unwrap();
        let created = super::dispatch(
            &store,
            None::<&OpenAiProvider>,
            None,
            "enma.decide",
            json!({"statement": "Persist", "source_ref": "sense_1"}),
        )
        .await
        .unwrap();
        let id = created["decision"]["id"].as_str().unwrap().to_owned();
        drop(store);

        let reopened = Store::open(&path).await.unwrap();
        let got = super::dispatch(
            &reopened,
            None::<&OpenAiProvider>,
            None,
            "enma.get_decision",
            json!({"id": id}),
        )
        .await
        .unwrap();
        assert_eq!(got["decision"]["statement"], "Persist");

        reopened.pool().close().await;
        let (code, body) = super::dispatch(
            &reopened,
            None::<&OpenAiProvider>,
            None,
            "enma.decide",
            json!({"statement": "Fail"}),
        )
        .await
        .unwrap_err();
        assert_eq!(code, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(body["error"], "storage_error");
    }

    #[tokio::test]
    async fn tools_list_names_are_all_dispatchable() {
        for tool in tools() {
            let name = tool["name"].as_str().unwrap();
            let method = name.replacen('_', ".", 1);
            if let Err((_, body)) =
                dispatch(None::<&OpenAiProvider>, false, &method, json!({})).await
            {
                assert_ne!(body["error"], "unknown_method", "{method} must be real");
            }
        }
    }

    #[tokio::test]
    async fn decide_rejects_bad_params() {
        let (code, _) = dispatch(
            None::<&OpenAiProvider>,
            false,
            "enma.decide",
            json!({"source_ref": "sense_abc"}),
        )
        .await
        .unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn request_ai_builds_decision_without_leaking_secret() {
        let fake = Fake(Ok(vec![AiOutput::ToolCall(ToolCall {
            name: "record_decision".into(),
            arguments: r#"{"statement":"Use Postgres","rationale":"Integrity matters"}"#.into(),
        })]));
        let mut params = json!({
            "statement": "database sensing",
            "source_ref": "sense_abc",
            "sensing_item": {"id": "sense_abc", "body": "database sensing", "kind": "knowledge"},
            "ai": {"api_key": "sk-secret", "base_url": "https://ai.test/v1", "model": "test"}
        });
        assert!(extract_ai_config(&mut params).is_some());
        let store = test_store().await;
        let out = super::dispatch(&store, Some(&fake), Some("test"), "enma.decide", params)
            .await
            .unwrap();
        assert_eq!(out["decision"]["statement"], "Use Postgres");
        assert_eq!(out["decision"]["rationale"], "Integrity matters");
        let decision: enma::Decision =
            serde_json::from_value(out["decision"].clone()).unwrap();
        assert!(decision.alternatives.is_empty());
        assert_eq!(out["sensing_item"]["id"], "sense_abc");
        assert_eq!(out["_meta"]["model"], "test");
        assert_eq!(out["_meta"]["usage"]["total_tokens"], 168);
        let id = out["decision"]["id"].as_str().unwrap();
        let stored: serde_json::Value = store.get("decision", id).await.unwrap().unwrap();
        assert!(stored.get("_meta").is_none());
        assert!(!out.to_string().contains("sk-secret"));
    }

    #[tokio::test]
    async fn request_ai_failure_is_ai_error() {
        let (code, body) = dispatch(
            Some(&Fake(Err(AiError::new("boom")))),
            true,
            "enma.decide",
            json!({"statement": "sensing"}),
        )
        .await
        .unwrap_err();
        assert_eq!(code, StatusCode::BAD_GATEWAY);
        assert_eq!(body["error"], "ai_error");
    }
}
