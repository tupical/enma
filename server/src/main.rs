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
//! /v1/mcp is closed), ENMA_VERSION (defaults to the crate version).

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::http::StatusCode;
use enma::{Actor, Alternative, Link, NewDecision, Timestamp};
use layer_kit::auth::Claims;
use layer_kit::serve::{serve, McpHandler, ServeConfig};
use serde_json::json;

const TOOL: &str = "enma";

/// Dispatches enma's MCP methods. Stateless — enma has no AI provider.
struct Handler;

impl McpHandler for Handler {
    async fn dispatch(
        &self,
        _claims: &Claims,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, (StatusCode, serde_json::Value)> {
        dispatch(method, params)
    }
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
        Handler,
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
fn dispatch(
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

    #[test]
    fn decide_builds_decision_with_sensemaking_provenance() {
        let out = dispatch(
            "enma.decide",
            json!({
                "statement": "Use Postgres for the primary store",
                "source_ref": "sense_abc"
            }),
        )
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

    #[test]
    fn read_methods_unsupported_and_unknown_method_rejected() {
        let (code, _) = dispatch("enma.list", json!({})).unwrap_err();
        assert_eq!(code, StatusCode::NOT_IMPLEMENTED);
        let (code, _) = dispatch("enma.nope", json!({})).unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn decide_rejects_bad_params() {
        let (code, _) = dispatch("enma.decide", json!({"source_ref": "sense_abc"})).unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }
}
