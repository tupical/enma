use crate::ai::AiError;

layer_kit::layer_error!(DecidingError {
    Ai(ai, "AI provider failed or returned an unusable response."),
    Serde(serde, "(De)serialization failure."),
    Validation(validation, "Output failed validation."),
});

impl From<AiError> for DecidingError {
    fn from(value: AiError) -> Self {
        Self::ai(value.to_string())
    }
}
