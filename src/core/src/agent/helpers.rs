use super::Agent;

pub fn title_case_words(s: &str) -> String {
    let mut out = String::new();
    for (i, w) in s.split_whitespace().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let mut chars = w.chars();
        if let Some(c) = chars.next() {
            out.extend(c.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

impl Agent {
    pub(crate) fn gen_uuid() -> String {
        uuid::Uuid::new_v4().to_string()
    }
}
