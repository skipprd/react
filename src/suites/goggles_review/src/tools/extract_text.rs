use async_trait::async_trait;
use serde_json::Value;

use react_core::agent::AgentCtx;
use react_core::tools::Tool;

pub struct ExtractTextTool;

#[async_trait]
impl Tool for ExtractTextTool {
    fn name(&self) -> &'static str {
        "extract_text"
    }

    async fn call(&self, args: Value, ctx: &AgentCtx) -> Result<Value, String> {
        let s3_key = args
            .get("s3_key")
            .and_then(|v| v.as_str())
            .ok_or("s3_key is required")?
            .to_string();
        let content_type = args
            .get("content_type")
            .and_then(|v| v.as_str())
            .unwrap_or("application/pdf");

        let storage = ctx.storage();
        let bytes = storage
            .get_bytes(&s3_key)
            .await
            .map_err(|e| format!("failed to read document from storage: {}", e))?;

        let extracted = if content_type.contains("pdf") {
            extract_pdf_text(&bytes)?
        } else if content_type.starts_with("image/") {
            // For images, we return a placeholder; the assess_requirement tool
            // should use the LLM's vision capability to read the image directly.
            serde_json::json!({
                "method": "image_passthrough",
                "note": "Image content requires LLM vision analysis. Pass the s3_key to the LLM with vision enabled.",
                "s3_key": s3_key,
                "content_type": content_type,
                "size_bytes": bytes.len(),
            })
        } else {
            let text = String::from_utf8_lossy(&bytes).to_string();
            serde_json::json!({
                "method": "plaintext",
                "text": text,
                "pages": [],
            })
        };

        Ok(serde_json::json!({
            "ok": true,
            "s3_key": s3_key,
            "content_type": content_type,
            "extraction": extracted,
        }))
    }
}

fn extract_pdf_text(bytes: &[u8]) -> Result<Value, String> {
    // Stub: in production, use pdf-extract or lopdf crate.
    // For now, attempt UTF-8 text extraction as a best-effort fallback.
    let text = String::from_utf8_lossy(bytes);

    // Filter to printable content (PDF binary often contains garbled bytes)
    let cleaned: String = text
        .chars()
        .filter(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
        .collect();

    if cleaned.trim().len() < 50 {
        return Ok(serde_json::json!({
            "method": "pdf_fallback",
            "note": "Could not extract meaningful text from PDF. May be a scanned document requiring OCR or LLM vision.",
            "raw_length": bytes.len(),
            "text": "",
            "pages": [],
        }));
    }

    Ok(serde_json::json!({
        "method": "pdf_text_layer",
        "text": cleaned.trim(),
        "pages": [{"page": 1, "text": cleaned.trim()}],
    }))
}
