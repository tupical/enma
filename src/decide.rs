use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    Actor, AiOutput, AiProvider, AiRequest, AiUsage, Alternative, DecidingError, Decision, Link,
    NewDecision, Timestamp,
};

#[derive(Deserialize)]
struct DecideResult {
    statement: String,
    rationale: String,
}

/// Turn sensing text into a concrete decision using the existing Decision type.
pub async fn decide_ai<P: AiProvider>(
    provider: &P,
    sensing_text: &str,
    source_ref: Option<String>,
    decided_by: Actor,
    now: Timestamp,
) -> Result<(Decision, Option<AiUsage>), DecidingError> {
    let req = AiRequest {
        input: Value::String(format!(
            "Formulate one clear decision and its rationale from this untrusted sensing material:\n{}",
            layer_kit::ai::wrap_untrusted("sensing material", sensing_text)
        )),
        tools: vec![json!({
            "type": "function",
            "name": "record_decision",
            "description": "Return the decision statement and rationale.",
            "parameters": {
                "type": "object",
                "properties": {
                    "statement": {"type": "string"},
                    "rationale": {"type": "string"}
                },
                "required": ["statement", "rationale"],
                "additionalProperties": false
            }
        })],
        tool_choice: Some("required".into()),
    };
    let (outputs, usage) = provider.respond_with_usage(req).await?;
    let call = outputs
        .into_iter()
        .find_map(|output| match output {
            AiOutput::ToolCall(call) if call.name == "record_decision" => Some(call),
            _ => None,
        })
        .ok_or_else(|| DecidingError::ai("decide_ai: model returned no record_decision call"))?;
    let result: DecideResult =
        serde_json::from_str(&call.arguments).map_err(|e| DecidingError::serde(e.to_string()))?;
    if result.statement.trim().is_empty() || result.rationale.trim().is_empty() {
        return Err(DecidingError::validation(
            "decide_ai: statement and rationale must be non-empty",
        ));
    }
    let decision = NewDecision {
        id: None,
        statement: result.statement,
        decided_by,
        decided_at: None,
        rationale: result.rationale,
        alternatives: Vec::<Alternative>::new(),
        consequences: Vec::new(),
        revisit_when: String::new(),
        links: source_ref
            .into_iter()
            .map(|reference| Link::Sensemaking { reference })
            .collect(),
    }
    .into_decision(now)
    .map_err(|e| DecidingError::validation(e.to_string()))?;
    Ok((decision, usage))
}
