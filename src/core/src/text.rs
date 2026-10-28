//! UTF-8-safe bounded text helpers. Slice on char boundaries only.

/// Truncate `s` to at most `max_bytes` UTF-8 bytes, never mid-codepoint.
pub fn truncate_utf8_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

/// Truncate `s` to at most `max_chars` Unicode scalar values.
///
/// Never panics on a multibyte boundary. `max_chars == 0` yields empty.
pub fn truncate_chars(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let mut out = String::new();
    out.reserve(max_chars.min(s.len()));
    for (i, ch) in s.chars().enumerate() {
        if i >= max_chars {
            break;
        }
        out.push(ch);
    }
    out
}

/// Truncate to `max_chars` scalar values. If anything was cut, append `"..."`.
pub fn truncate_chars_ellipsis(s: &str, max_chars: usize) -> String {
    let t = truncate_chars(s, max_chars);
    if s.chars().count() > max_chars {
        format!("{t}...")
    } else {
        t
    }
}

/// Fit `s` into `max_chars` including a trailing `…` when truncated.
pub fn truncate_chars_fit(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    format!("{}…", truncate_chars(s, max_chars.saturating_sub(1)))
}

/// Last `n` Unicode scalar values of `s`.
pub fn last_chars(s: &str, n: usize) -> String {
    if n == 0 {
        return String::new();
    }
    let total = s.chars().count();
    if total <= n {
        return s.to_string();
    }
    s.chars().skip(total - n).collect()
}

/// Split `s` after `n` Unicode scalar values. Both sides are on a char boundary.
pub fn split_at_chars(s: &str, n: usize) -> (&str, &str) {
    match s.char_indices().nth(n) {
        Some((i, _)) => s.split_at(i),
        None => (s, ""),
    }
}

/// Visible-column cap for TUI lines. ANSI CSI SGR sequences are copied but do not
/// consume width. Truncation never indexes a UTF-8 mid-codepoint.
pub fn ansi_fit_line(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if ansi_display_width(s) <= width {
        return s.to_string();
    }

    let mut out = String::new();
    let mut w = 0usize;
    let mut i = 0usize;
    let bytes = s.as_bytes();

    while i < bytes.len() && w + 1 < width {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            let start = i;
            i += 2;
            while i < bytes.len() && bytes[i] != b'm' {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'm' {
                i += 1;
                out.push_str(&String::from_utf8_lossy(&bytes[start..i]));
                continue;
            } else {
                break;
            }
        }

        let ch = match s[i..].chars().next() {
            None => break,
            Some(c) => c,
        };
        let len = ch.len_utf8();
        out.push(ch);
        i += len;
        w += 1;
    }

    out.push('…');
    if out.contains('\u{1b}') && !out.ends_with("\u{1b}[0m") {
        out.push_str("\u{1b}[0m");
    }
    out
}

fn ansi_display_width(s: &str) -> usize {
    let mut w = 0usize;
    let bytes = s.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == 0x1b && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            i += 2;
            while i < bytes.len() && bytes[i] != b'm' {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'm' {
                i += 1;
            }
            continue;
        }
        let ch = match s[i..].chars().next() {
            None => break,
            Some(c) => c,
        };
        i += ch.len_utf8();
        w += 1;
    }
    w
}

#[cfg(test)]
mod tests {
    use super::{
        ansi_fit_line, last_chars, split_at_chars, truncate_chars, truncate_chars_ellipsis,
        truncate_chars_fit, truncate_utf8_bytes,
    };

    #[test]
    fn truncate_chars_does_not_panic_on_utf8_boundary() {
        let s = "éééé";
        assert_eq!(s.len(), 8);
        let out = truncate_chars(s, 1);
        assert_eq!(out, "é");
        assert_eq!(truncate_chars(s, 0), "");
        assert_eq!(truncate_chars(s, 4), s);
        assert_eq!(truncate_chars(s, 99), s);
        assert_eq!(truncate_chars_ellipsis("éééé", 1), "é...");
        assert_eq!(truncate_chars_ellipsis("ab", 10), "ab");
        assert_eq!(truncate_chars_fit("éééé", 2), "é…");
        assert_eq!(truncate_chars_fit("ab", 10), "ab");
        assert_eq!(last_chars("éééé", 1), "é");
        assert_eq!(last_chars("abcd", 2), "cd");
        assert_eq!(split_at_chars("éééé", 1), ("é", "ééé"));
        assert_eq!(split_at_chars("ab", 10), ("ab", ""));
        assert_eq!(truncate_utf8_bytes("éé", 2), "é");
        assert_eq!(truncate_utf8_bytes("ab", 10), "ab");
        let ansi = "\u{1b}[31méééé\u{1b}[0m";
        let fitted = ansi_fit_line(ansi, 3);
        assert!(fitted.starts_with('\u{1b}'));
        assert!(fitted.contains('é'));
        assert!(fitted.contains('…'));
    }
}
