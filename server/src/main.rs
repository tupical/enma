//! enma-server — thin, independently-deployed HTTP/MCP wrapper around the
//! `enma` decisions lib. Its own deploy unit (own systemd service, own port).
//! Boundary-clean: no mcpbox dependency; the platform→tool auth contract is a
//! configured shared key (see `auth`).
//!
//! Routes:
//!   GET  /healthz   — open; liveness + version for the platform registry.
//!   POST /v1/mcp    — requires a valid platform token; decisions surface
//!                     (`enma.decide` builds a typed Decision via the lib).
//!
//! Env: ENMA_PORT (default 8092), ENMA_PLATFORM_SECRET (HMAC key; if unset,
//! /v1/mcp is closed), ENMA_VERSION (defaults to the crate version).

mod auth;

use std::{
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use enma::{Actor, Alternative, Link, NewDecision, Timestamp};
use serde_json::json;

const TOOL: &str = "enma";

struct AppState {
    version: String,
    platform_secret: Option<Vec<u8>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().json().init();

    let version =
        std::env::var("ENMA_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    let platform_secret = std::env::var("ENMA_PLATFORM_SECRET")
        .ok()
        .filter(|s| !s.is_empty())
        .map(String::into_bytes);
    if platform_secret.is_none() {
        tracing::warn!("ENMA_PLATFORM_SECRET unset - /v1/mcp will reject all requests");
    }
    let state = Arc::new(AppState {
        version,
        platform_secret,
    });

    let app = Router::new()
        .route("/healthz", get(healthz))
        .route("/v1/mcp", post(mcp))
        .with_state(state);

    let port = std::env::var("ENMA_PORT").unwrap_or_else(|_| "8092".to_string());
    // localhost-bound: only the co-located platform reaches it (C3 hardening).
    let addr = format!("127.0.0.1:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("bind {addr}: {e}"));
    tracing::info!(%addr, tool = TOOL, "enma-server listening");
    axum::serve(listener, app).await.expect("server error");
}

async fn healthz(State(s): State<Arc<AppState>>) -> impl IntoResponse {
    Json(json!({ "service": TOOL, "status": "ok", "version": s.version }))
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

async fn mcp(State(s): State<Arc<AppState>>, headers: HeaderMap, body: Bytes) -> impl IntoResponse {
    let Some(secret) = &s.platform_secret else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"auth_disabled"})),
        )
            .into_response();
    };
    let token = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    let Some(claims) = token.and_then(|t| auth::verify(secret, TOOL, now_secs(), t)) else {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"invalid_platform_token"})),
        )
            .into_response();
    };

    // Auth passed — dispatch the MCP method against the enma decisions lib.
    let req: McpRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": "bad_request", "detail": e.to_string()})),
            )
                .into_response();
        }
    };
    match dispatch(&req.method, req.params) {
        Ok(mut result) => {
            result["tool"] = json!(TOOL);
            result["version"] = json!(s.version);
            result["workspace"] = json!(claims.workspace);
            result["project"] = json!(claims.project);
            Json(result).into_response()
        }
        Err((code, payload)) => (code, Json(payload)).into_response(),
    }
}

/// One MCP call: `{ "method": "enma.decide", "params": { ... } }`.
#[derive(serde::Deserialize)]
struct McpRequest {
    method: String,
    #[serde(default)]
    params: serde_json::Value,
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
        "enma.list" | "enma.get" | "enma.list_decisions" | "enma.get_decision" => {
            Err((
                StatusCode::NOT_IMPLEMENTED,
                json!({"error": "unsupported", "detail": "enma-server is stateless (OSS skeleton has no store); list/get need a storage adapter"}),
            ))
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
