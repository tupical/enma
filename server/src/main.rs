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
use enma::{Actor, Alternative, Link, NewDecision, Timestamp};
use layer_kit::ai::extract_ai_config;
use layer_kit::auth::Claims;
use layer_kit::openai::{AiConfig, OpenAiProvider};
use layer_kit::serve::{serve, McpHandler, ServeConfig};
use serde_json::json;

const TOOL: &str = "enma";

/// Dispatches enma's MCP methods and owns the optional env fallback provider.
struct Handler {
    ai: Option<OpenAiProvider>,
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
            dispatch(Some(&provider), true, method, params).await
        } else {
            dispatch(self.ai.as_ref(), false, method, params).await
        }
    }

    fn tools(&self) -> Vec<serde_json::Value> {
        tools()
    }
}

/// Tool descriptors for `tools/list` — one per method actually handled by
/// [`dispatch`] (`enma.list`/`enma.get`/`enma.list_decisions`/
/// `enma.get_decision` are NOT_IMPLEMENTED, so they are omitted).
fn tools() -> Vec<serde_json::Value> {
    vec![json!({
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
    })]
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().json().init();

    serve(
        ServeConfig {
            tool: TOOL,
            default_port: 8092,
            default_version: env!("CARGO_PKG_VERSION"),
            git_sha: option_env!("GIT_SHA").unwrap_or("dev"),
        },
        Handler {
            ai: AiConfig::from_env().map(OpenAiProvider::new),
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
    decided_by: Option<Actor>,
}

/// Pure MCP dispatch over the enma decisions lib — no auth, no HTTP, so it is
/// unit-testable directly. `enma` is a stateless OSS skeleton: it builds typed
/// objects but stores nothing, so read methods are unsupported.
async fn dispatch<P: enma::AiProvider>(
    ai: Option<&P>,
    request_ai: bool,
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
            if request_ai {
                let provider = ai.ok_or_else(|| {
                    (
                        StatusCode::SERVICE_UNAVAILABLE,
                        json!({"error": "ai_not_configured", "detail": "AI provider is not configured"}),
                    )
                })?;
                let decision = enma::decide_ai(
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
                return Ok(json!({ "method": "enma.decide", "decision": decision }));
            }
            let source_ref = p.source_ref;
            let links = source_ref
                .iter()
                .map(|reference| Link::Sensemaking {
                    reference: reference.clone(),
                })
                .collect();
            let decision = NewDecision {
                id: None,
                statement: p.statement,
                decided_by: p.decided_by.unwrap_or_else(Actor::user),
                decided_at: None,
                rationale: source_ref
                    .as_ref()
                    .map(|reference| format!("Promoted from sensing item {reference}"))
                    .unwrap_or_default(),
                alternatives: Vec::<Alternative>::new(),
                consequences: Vec::new(),
                revisit_when: String::new(),
                links,
            }
            .into_decision(now_timestamp())
            .map_err(|e| {
                (
                    StatusCode::BAD_REQUEST,
                    json!({"error": "invalid_params", "detail": e.to_string()}),
                )
            })?;
            Ok(json!({ "method": "enma.decide", "decision": decision }))
        }
        "enma.list" | "enma.get" | "enma.list_decisions" | "enma.get_decision" => Err((
            StatusCode::NOT_IMPLEMENTED,
            json!({"error": "unsupported", "detail": "enma-server is stateless (OSS skeleton has no store); list/get need a storage adapter"}),
        )),
        other => Err((
            StatusCode::BAD_REQUEST,
            json!({"error": "unknown_method", "detail": other}),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enma::{AiError, AiOutput, AiRequest, ToolCall};

    struct Fake(Result<Vec<AiOutput>, AiError>);

    impl enma::AiProvider for Fake {
        async fn respond(&self, _req: AiRequest) -> Result<Vec<AiOutput>, AiError> {
            self.0.clone()
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
    }

    #[tokio::test]
    async fn read_methods_unsupported_and_unknown_method_rejected() {
        let (code, _) = dispatch(None::<&OpenAiProvider>, false, "enma.list", json!({}))
            .await
            .unwrap_err();
        assert_eq!(code, StatusCode::NOT_IMPLEMENTED);
        let (code, _) = dispatch(None::<&OpenAiProvider>, false, "enma.nope", json!({}))
            .await
            .unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn tools_list_names_are_all_dispatchable() {
        for tool in tools() {
            let name = tool["name"].as_str().unwrap();
            let method = name.replacen('_', ".", 1);
            let (_, body) = dispatch(None::<&OpenAiProvider>, false, &method, json!({}))
                .await
                .expect_err("empty params must not satisfy any real method");
            assert_ne!(
                body["error"], "unknown_method",
                "{method} must be a real dispatch method"
            );
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
            "ai": {"api_key": "sk-secret", "base_url": "https://ai.test/v1", "model": "test"}
        });
        assert!(extract_ai_config(&mut params).is_some());
        let out = dispatch(Some(&fake), true, "enma.decide", params)
            .await
            .unwrap();
        assert_eq!(out["decision"]["statement"], "Use Postgres");
        assert_eq!(out["decision"]["rationale"], "Integrity matters");
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
