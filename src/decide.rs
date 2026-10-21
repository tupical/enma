use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    Actor, AiOutput, AiProvider, AiRequest, AiUsage, Alternative, DecidingError, Decision,
    Directive, DirectiveKind, Link, NewDecision, NewDirective, Timestamp,
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
    /// Hard boundaries the material imposes; recorded as `constraint` directives.
    #[serde(default)]
    constraints: Vec<String>,
}

fn decidable_default() -> bool {
    true
}

/// Outcome of [`decide_ai`]: a decision with the `constraint` directives recorded
/// next to it, or the questions a human must answer first.
#[derive(Debug)]
pub enum Decided {
    Decision(Decision, Vec<Directive>, Option<AiUsage>),
    NeedsInput(Vec<String>),
}

/// Role each directive kind plays for the model (ADR-0012 §1).
fn directive_role(kind: DirectiveKind) -> &'static str {
    match kind {
        DirectiveKind::Constraint | DirectiveKind::NonGoal | DirectiveKind::Principle => {
            "boundary: the decision must not violate it"
        }
        DirectiveKind::SuccessCriteria => "threshold: the decision must be good enough to meet it",
        DirectiveKind::Assumption => "source of revisit conditions: revisit if it breaks",
        DirectiveKind::Goal => "context",
    }
}

/// Trusted host context listing the project's active directives; empty when
/// the host passed none.
fn directives_prompt(directives: &[Directive]) -> String {
    if directives.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "\nActive project directives (trusted host context). \
Every rejected_because must name the directive the option violates (its id or statement).\n",
    );
    for d in directives {
        out.push_str(&format!(
            "- [{} {}] {} ({})\n",
            d.kind.as_str(),
            d.id,
            d.statement.trim(),
            directive_role(d.kind)
        ));
    }
    out
}

/// Turn sensing text into a concrete decision using the existing Decision type.
///
/// `directives` are the project's active directives, supplied by the host as
/// trusted context (ADR-0012); an empty slice is allowed and never excuses
/// empty `consequences` / `revisit_when`.
pub async fn decide_ai<P: AiProvider>(
    provider: &P,
    sensing_text: &str,
    directives: &[Directive],
    source_ref: Option<String>,
    decided_by: Actor,
    now: Timestamp,
) -> Result<Decided, DecidingError> {
    let req = AiRequest {
        input: Value::String(format!(
            "Formulate one clear decision and its rationale from this untrusted sensing material. \
If the material does not support a decision (it is a question, or key facts are missing), \
set decidable=false and list open_questions instead of inventing one. \
Only list alternatives that the material actually mentions. \
List as constraints only hard boundaries the material states. \
A decision must list at least one consequence and say when it should be revisited \
(a checkable event, metric or assumption); every alternative needs its rejected_because.{}\n{}",
            directives_prompt(directives),
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
                    "consequences": {
                        "type": "array",
                        "items": {"type": "string"},
                        "description": "Required when decidable: at least one non-empty consequence."
                    },
                    "revisit_when": {
                        "type": "string",
                        "description": "Required when decidable: the checkable condition to revisit the decision."
                    },
                    "constraints": {"type": "array", "items": {"type": "string"}},
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
    // ADR-0012 completeness contract for model-born decisions.
    let consequences: Vec<String> = result
        .consequences
        .into_iter()
        .map(|c| c.trim().to_string())
        .filter(|c| !c.is_empty())
        .collect();
    if consequences.is_empty() {
        return Err(DecidingError::validation(
            "decide_ai: consequences must list at least one non-empty item",
        ));
    }
    if result.revisit_when.trim().is_empty() {
        return Err(DecidingError::validation(
            "decide_ai: revisit_when must be non-empty",
        ));
    }
    if result
        .alternatives
        .iter()
        .any(|a| a.rejected_because.trim().is_empty())
    {
        return Err(DecidingError::validation(
            "decide_ai: every alternative needs a non-empty rejected_because",
        ));
    }
    let links: Vec<Link> = source_ref
        .into_iter()
        .map(|reference| Link::Sensemaking { reference })
        .collect();
    let decision = NewDecision {
        id: None,
        statement: result.statement,
        decided_by: decided_by.clone(),
        decided_at: None,
        rationale: result.rationale,
        alternatives: result.alternatives,
        consequences,
        revisit_when: result.revisit_when.trim().to_string(),
        links: links.clone(),
    }
    .into_decision(now)
    .map_err(|e| DecidingError::validation(e.to_string()))?;
    // Directives have no Decision link variant, so the decision id rides in
    // the rationale; blank and repeated constraints fix nothing and are dropped.
    let mut directives: Vec<Directive> = Vec::new();
    for statement in result.constraints {
        let statement = statement.trim().to_string();
        if statement.is_empty() || directives.iter().any(|d| d.statement == statement) {
            continue;
        }
        directives.push(
            NewDirective {
                id: None,
                kind: DirectiveKind::Constraint,
                statement,
                set_by: decided_by.clone(),
                rationale: format!("Recorded with decision {}", decision.id),
                links: links.clone(),
            }
            .into_directive(now)
            .map_err(|e| DecidingError::validation(e.to_string()))?,
        );
    }
    Ok(Decided::Decision(decision, directives, usage))
}
