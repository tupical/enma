use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    Actor, AiOutput, AiProvider, AiRequest, AiUsage, Alternative, DecidingError, Decision, Link,
    NewDecision, Timestamp,
};

#[derive(Deserialize)]
struct DecideResult {
    /// `false` = the material does not support a decision; `open_questions` says what is missing.
    #[serde(default = "decidable_default")]
    decidable: bool,
    #[serde(default)]
    open_questions: Vec<String>,
    #[serde(default)]
    statement: String,
    #[serde(default)]
    rationale: String,
    #[serde(default)]
    alternatives: Vec<Alternative>,
    #[serde(default)]
    consequences: Vec<String>,
    #[serde(default)]
    revisit_when: String,
}

fn decidable_default() -> bool {
    true
}

/// Outcome of [`decide_ai`]: a decision, or the questions a human must answer first.
#[derive(Debug)]
pub enum Decided {
    Decision(Decision, Option<AiUsage>),
    NeedsInput(Vec<String>),
}

/// Turn sensing text into a concrete decision using the existing Decision type.
pub async fn decide_ai<P: AiProvider>(
    provider: &P,
    sensing_text: &str,
    source_ref: Option<String>,
    decided_by: Actor,
    now: Timestamp,
) -> Result<Decided, DecidingError> {
    let req = AiRequest {
        input: Value::String(format!(
            "Formulate one clear decision and its rationale from this untrusted sensing material. \
If the material does not support a decision (it is a question, or key facts are missing), \
set decidable=false and list open_questions instead of inventing one. \
Only list alternatives that the material actually mentions.\n{}",
            layer_kit::ai::wrap_untrusted("sensing material", sensing_text)
        )),
        tools: vec![json!({
            "type": "function",
            "name": "record_decision",
            "description": "Return the decision, or decidable=false with open_questions.",
            "parameters": {
                "type": "object",
                "properties": {
                    "decidable": {"type": "boolean"},
                    "open_questions": {"type": "array", "items": {"type": "string"}},
                    "statement": {"type": "string"},
                    "rationale": {"type": "string"},
                    "consequences": {"type": "array", "items": {"type": "string"}},
                    "revisit_when": {"type": "string"},
                    "alternatives": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "option": {"type": "string"},
                                "rejected_because": {"type": "string"}
                            },
                            "required": ["option", "rejected_because"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["decidable"],
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
    if !result.decidable {
        let questions: Vec<String> = result
            .open_questions
            .into_iter()
            .filter(|q| !q.trim().is_empty())
            .collect();
        if questions.is_empty() {
            return Err(DecidingError::validation(
                "decide_ai: decidable=false requires open_questions",
            ));
        }
        return Ok(Decided::NeedsInput(questions));
    }
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
        alternatives: result.alternatives,
        consequences: result.consequences,
        revisit_when: result.revisit_when,
        links: source_ref
            .into_iter()
            .map(|reference| Link::Sensemaking { reference })
            .collect(),
    }
    .into_decision(now)
    .map_err(|e| DecidingError::validation(e.to_string()))?;
    Ok(Decided::Decision(decision, usage))
}
