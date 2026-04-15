use serde::de::DeserializeOwned;
use serde_json::Value;

/// Extract the first complete JSON value from a string that may contain
/// trailing data (e.g. multiple concatenated JSON objects from an LLM).
pub fn try_first_json_value(s: &str) -> Option<Value> {
    let mut stream = serde_json::Deserializer::from_str(s).into_iter::<Value>();
    stream.next().and_then(|r| r.ok())
}

/// Repair raw control characters inside JSON string literals.
///
/// Some model backends emit "JSON-like" text with literal control characters
/// (e.g. raw newlines) inside string values. This function escapes them so
/// `serde_json` can parse the result.
pub fn escape_control_chars_in_json_strings(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut in_str = false;
    let mut esc = false;
    for ch in s.chars() {
        if in_str {
            if esc {
                out.push(ch);
                esc = false;
                continue;
            }
            if ch == '\\' {
                out.push(ch);
                esc = true;
                continue;
            }
            match ch {
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\u{08}' => out.push_str("\\b"),
                '\u{0C}' => out.push_str("\\f"),
                '"' => {
                    out.push(ch);
                    in_str = false;
                }
                c if (c as u32) < 0x20 => {
                    out.push_str(&format!("\\u{:04x}", c as u32));
                }
                _ => out.push(ch),
            }
            continue;
        }

        if esc {
            out.push(ch);
            esc = false;
            continue;
        }
        match ch {
            '"' => {
                out.push(ch);
                in_str = true;
            }
            '\\' => {
                out.push(ch);
                esc = true;
            }
            _ => out.push(ch),
        }
    }
    out
}

/// Repair invalid JSON escape sequences inside string literals.
///
/// JSON only allows `\"`, `\\`, `\/`, `\b`, `\f`, `\n`, `\r`, `\t`, and `\uXXXX`.
/// LLMs producing regex code often emit `\s`, `\d`, `\w` etc. which are invalid
/// in JSON. This function converts unrecognized `\X` → `\\X` so the JSON parses.
pub fn repair_invalid_json_escapes(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 16);
    let mut in_str = false;
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if !in_str {
            out.push(ch);
            if ch == '"' {
                in_str = true;
            }
            continue;
        }
        if ch == '"' {
            out.push(ch);
            in_str = false;
            continue;
        }
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
                match next {
                    '"' | '\\' | '/' | 'b' | 'f' | 'n' | 'r' | 't' | 'u' => {
                        out.push(ch);
                        out.push(chars.next().unwrap());
                    }
                    _ => {
                        out.push('\\');
                        out.push('\\');
                    }
                }
            } else {
                out.push(ch);
            }
            continue;
        }
        out.push(ch);
    }
    out
}

/// Extract all top-level JSON values (objects/arrays) from a string that may
/// contain non-JSON text around them.
pub fn extract_all_json_values(s: &str, max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if max == 0 {
        return out;
    }
    let mut i = 0usize;
    while i < s.len() && out.len() < max {
        let mut start: Option<usize> = None;
        for (off, ch) in s[i..].char_indices() {
            if ch == '{' || ch == '[' {
                start = Some(i + off);
                break;
            }
        }
        let Some(st) = start else { break };

        let mut stack: Vec<char> = Vec::new();
        let mut in_str = false;
        let mut esc = false;
        let mut end: Option<usize> = None;
        for (pos, ch) in s[st..].char_indices() {
            let abs = st + pos;

            if stack.is_empty() {
                stack.push(ch);
                continue;
            }
            if in_str {
                if esc {
                    esc = false;
                    continue;
                }
                if ch == '\\' {
                    esc = true;
                    continue;
                }
                if ch == '"' {
                    in_str = false;
                }
                continue;
            }

            match ch {
                '"' => in_str = true,
                '{' | '[' => stack.push(ch),
                '}' => {
                    if matches!(stack.pop(), Some('{')) && stack.is_empty() {
                        end = Some(abs);
                        break;
                    }
                }
                ']' => {
                    if matches!(stack.pop(), Some('[')) && stack.is_empty() {
                        end = Some(abs);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(en) = end else { break };
        out.push(s[st..=en].to_string());
        i = en + 1;
    }
    out
}

/// Find the byte offset just past the end of the first complete JSON object in `s`.
/// Returns `None` if `s` does not start with `{`.
pub fn find_first_json_object_end(s: &str) -> Option<usize> {
    let mut stream = serde_json::Deserializer::from_str(s).into_iter::<Value>();
    if stream.next()?.is_ok() {
        Some(stream.byte_offset())
    } else {
        None
    }
}

/// Resilient JSON parse: tries raw parse, then each repair strategy.
///
/// Chains: raw → first-object extraction → control-char repair →
/// invalid-escape repair → extract-from-repaired.
pub fn resilient_parse<T: DeserializeOwned>(text: &str) -> Result<T, String> {
    let s = text.trim();

    if let Ok(v) = serde_json::from_str::<T>(s) {
        return Ok(v);
    }

    if let Some(first) = try_first_json_value(s) {
        if let Ok(v) = serde_json::from_value::<T>(first) {
            return Ok(v);
        }
    }

    let repaired_ctrl = escape_control_chars_in_json_strings(s);
    if let Ok(v) = serde_json::from_str::<T>(&repaired_ctrl) {
        return Ok(v);
    }

    let repaired_esc = repair_invalid_json_escapes(s);
    if let Ok(v) = serde_json::from_str::<T>(&repaired_esc) {
        return Ok(v);
    }

    let vals = extract_all_json_values(s, 8);
    for vtxt in vals.iter().rev() {
        if let Ok(v) = serde_json::from_str::<T>(vtxt) {
            return Ok(v);
        }
    }

    Err("LLM response did not contain valid JSON".to_string())
}

/// Like [`resilient_parse`] but for a JSON string field that was already extracted
/// from an outer JSON object (e.g. the `args` field of an agent step).
pub fn resilient_parse_string_field(raw_json: &str) -> Result<Value, String> {
    if let Ok(v) = serde_json::from_str::<Value>(raw_json) {
        return Ok(v);
    }

    let repaired_ctrl = escape_control_chars_in_json_strings(raw_json);
    if let Ok(v) = serde_json::from_str::<Value>(&repaired_ctrl) {
        return Ok(v);
    }

    let repaired_esc = repair_invalid_json_escapes(raw_json);
    if let Ok(v) = serde_json::from_str::<Value>(&repaired_esc) {
        return Ok(v);
    }

    let both = escape_control_chars_in_json_strings(&repaired_esc);
    serde_json::from_str::<Value>(&both).map_err(|e| format!("not valid JSON string: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn try_first_extracts_from_concatenated() {
        let input = r#"{"a":1}{"b":2}"#;
        let v = try_first_json_value(input).unwrap();
        assert_eq!(v, serde_json::json!({"a": 1}));
    }

    #[test]
    fn escape_control_chars_fixes_raw_newlines() {
        let input = "{\"text\":\"line1\nline2\"}";
        let repaired = escape_control_chars_in_json_strings(input);
        let v: Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(v["text"].as_str().unwrap(), "line1\nline2");
    }

    #[test]
    fn repair_escapes_fixes_backslash_s() {
        let input = r#"{"text":"hello \s world"}"#;
        let repaired = repair_invalid_json_escapes(input);
        assert_eq!(repaired, r#"{"text":"hello \\s world"}"#);
        let v: Value = serde_json::from_str(&repaired).unwrap();
        assert_eq!(v["text"].as_str().unwrap(), r"hello \s world");
    }

    #[test]
    fn repair_escapes_preserves_valid_escapes() {
        let input = r#"{"text":"line1\nline2\ttab\\backslash"}"#;
        let repaired = repair_invalid_json_escapes(input);
        assert_eq!(repaired, input);
    }

    #[test]
    fn extract_all_finds_objects_in_prose() {
        let input = r#"Here is the JSON: {"a":1} and also {"b":2} done."#;
        let vals = extract_all_json_values(input, 5);
        assert_eq!(vals.len(), 2);
        assert_eq!(vals[0], r#"{"a":1}"#);
    }

    #[test]
    fn resilient_parse_handles_concatenated() {
        let input = r#"{"a":1}{"b":2}"#;
        let v: Value = resilient_parse(input).unwrap();
        assert_eq!(v, serde_json::json!({"a": 1}));
    }

    #[test]
    fn resilient_parse_handles_wrapped_json() {
        let input = r#"Sure! Here is the result: {"a":1}"#;
        let v: Value = resilient_parse(input).unwrap();
        assert_eq!(v, serde_json::json!({"a": 1}));
    }

    #[test]
    fn resilient_parse_string_field_handles_invalid_escapes() {
        let input = r#"{"regex":"^[^@\s]+$"}"#;
        let v = resilient_parse_string_field(input).unwrap();
        assert_eq!(v["regex"].as_str().unwrap(), r"^[^@\s]+$");
    }
}
