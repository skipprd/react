use crate::llm::types::{
    Capabilities, ChatRequest, ChatResponse, EmbedRequest, EmbedResponse, ProviderHttpRequest,
    ProviderHttpResponse,
};
use react_core::llm::LlmError;

pub trait Adapter: Send + Sync {
    fn capabilities(&self, model: &str) -> Capabilities;

    fn build_chat_http(&self, req: &ChatRequest) -> Result<ProviderHttpRequest, LlmError>;
    fn parse_chat_http(&self, resp: &ProviderHttpResponse) -> Result<ChatResponse, LlmError>;

    fn build_embed_http(&self, req: &EmbedRequest) -> Result<ProviderHttpRequest, LlmError>;
    fn parse_embed_http(&self, resp: &ProviderHttpResponse) -> Result<EmbedResponse, LlmError>;
}
