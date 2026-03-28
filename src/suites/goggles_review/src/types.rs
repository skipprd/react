use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRequest {
    pub instance_id: Uuid,
    pub evidence_id: Uuid,
    pub s3_key: String,
    pub content_type: Option<String>,
    pub requirement: RequirementContext,
    pub question_context: QuestionContext,
    pub callback_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementContext {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionContext {
    pub question_id: String,
    pub prompt_summary: String,
    pub module_title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    Satisfied,
    NotSatisfied,
    Inconclusive,
    NeedsHumanReview,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentCitation {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph: Option<u32>,
    pub relevance_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceReviewResult {
    pub requirement_id: String,
    pub evidence_id: Uuid,
    pub verdict: ReviewVerdict,
    pub confidence: f64,
    pub reasoning: String,
    pub citations: Vec<DocumentCitation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extracted_text: Option<String>,
    pub reviewed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewCallbackPayload {
    pub instance_id: Uuid,
    pub evidence_id: Uuid,
    pub result: EvidenceReviewResult,
}
