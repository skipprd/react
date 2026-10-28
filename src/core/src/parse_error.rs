//! Typed LLM step parse failures. Never classified by Display prefix.

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum StepParseError {
    #[error("not JSON: {0}")]
    NotJson(String),
    #[error("schema: {0}")]
    Schema(String),
    #[error("shape: {0}")]
    ShapeMismatch(String),
    #[error("args not JSON: {0}")]
    Args(String),
}

impl StepParseError {
    pub const fn retriable(&self) -> bool {
        true
    }

    pub const fn label(&self) -> &'static str {
        match self {
            Self::Schema(_) => "schema_validation_failed",
            Self::NotJson(_) | Self::ShapeMismatch(_) | Self::Args(_) => "invalid_json_from_model",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::StepParseError;

    #[test]
    fn parse_error_labels_are_typed() {
        assert_eq!(
            StepParseError::Schema("x".into()).label(),
            "schema_validation_failed"
        );
        assert_eq!(
            StepParseError::NotJson("x".into()).label(),
            "invalid_json_from_model"
        );
        assert_eq!(
            StepParseError::ShapeMismatch("x".into()).label(),
            "invalid_json_from_model"
        );
    }
}
