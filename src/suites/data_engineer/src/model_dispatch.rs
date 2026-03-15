use react_core::llm::{LlmCallOptions, LlmExpectedFormat};
use react_core::resolved_config::LlmResolved;

/// Routes LLM calls to the appropriate model with role-appropriate sampling parameters.
///
/// `reason_model` is for planning/diagnosis (higher capability, higher temp with retries).
/// `task_model` is for mechanical tool-calling (cheaper, low temperature).
/// If no `task_model` is configured, `reason_model` is used for both roles at low temperature.
#[derive(Clone, Debug)]
pub struct ModelDispatch {
    pub reason_model: String,
    pub task_model: String,
}

impl ModelDispatch {
    pub fn from_resolved(cfg: &LlmResolved) -> Self {
        let reason = cfg
            .reason_model
            .clone()
            .unwrap_or_else(|| "gpt-4o-mini".to_string());
        let task = cfg
            .task_model
            .clone()
            .unwrap_or_else(|| reason.clone());
        Self {
            reason_model: reason,
            task_model: task,
        }
    }

    /// Build call options for the task (gather/tool-calling) model.
    pub fn task_call_options(&self, prompt_id: &'static str) -> LlmCallOptions {
        LlmCallOptions {
            prompt_id,
            model: Some(self.task_model.clone()),
            temperature: Some(0.1),
            expected_format: LlmExpectedFormat::Text,
            ..Default::default()
        }
    }

    /// Build call options for the reasoning model.
    /// Temperature rises with iteration to avoid deterministic loops.
    pub fn reason_call_options(&self, prompt_id: &'static str, iteration: usize) -> LlmCallOptions {
        let temp = (0.15 + (iteration as f32 * 0.05)).min(0.35);
        LlmCallOptions {
            prompt_id,
            model: Some(self.reason_model.clone()),
            temperature: Some(temp),
            expected_format: LlmExpectedFormat::JsonObject,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_model_falls_back_to_reason() {
        let cfg = LlmResolved {
            reason_model: Some("gpt-5.4".into()),
            task_model: None,
            ..Default::default()
        };
        let d = ModelDispatch::from_resolved(&cfg);
        assert_eq!(d.task_model, "gpt-5.4");
    }

    #[test]
    fn explicit_task_model_used() {
        let cfg = LlmResolved {
            reason_model: Some("gpt-5.4".into()),
            task_model: Some("gpt-4.1".into()),
            ..Default::default()
        };
        let d = ModelDispatch::from_resolved(&cfg);
        assert_eq!(d.task_model, "gpt-4.1");
        assert_eq!(d.reason_model, "gpt-5.4");
    }

    #[test]
    fn reason_temp_rises_with_iteration() {
        let cfg = LlmResolved {
            reason_model: Some("gpt-5.4".into()),
            ..Default::default()
        };
        let d = ModelDispatch::from_resolved(&cfg);
        let o0 = d.reason_call_options("test", 0);
        let o3 = d.reason_call_options("test", 3);
        let o10 = d.reason_call_options("test", 10);
        assert_eq!(o0.temperature, Some(0.15));
        assert!((o3.temperature.unwrap() - 0.30).abs() < 0.001);
        assert_eq!(o10.temperature, Some(0.35)); // capped
    }
}
