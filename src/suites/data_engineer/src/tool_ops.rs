use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileOpKind {
    Get,
    List,
    Patch,
    Write,
    Rm,
    Mv,
    Other,
}

impl FileOpKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Get => "get",
            Self::List => "list",
            Self::Patch => "patch",
            Self::Write => "write",
            Self::Rm => "rm",
            Self::Mv => "mv",
            Self::Other => "other",
        }
    }

    pub const fn is_mutation(self) -> bool {
        matches!(self, Self::Patch | Self::Write | Self::Rm | Self::Mv)
    }

    pub const fn is_read(self) -> bool {
        matches!(self, Self::Get | Self::List)
    }

    pub const fn args_signature(self) -> &'static str {
        match self {
            Self::Get => "path:string, max_chars?:int",
            Self::List => "prefix?:string, limit?:int",
            Self::Patch => "path:string, patch_text:string",
            Self::Write => "path:string, content:string",
            Self::Rm => "path:string, expected_sha256?:string",
            Self::Mv => "from:string, to:string, expected_sha256?:string",
            Self::Other => "",
        }
    }

    pub const fn card_hint(self) -> &'static str {
        match self {
            Self::Patch => "Cursor/Aider hunks-only; patch_text starts with '@@' and MUST NOT include ---/+++ or diff --git",
            Self::Write => "full file overwrite with complete correct content",
            _ => "",
        }
    }

    pub fn tool_card_detail_line(self) -> String {
        let sig = self.args_signature();
        let hint = self.card_hint();
        if hint.is_empty() {
            format!("  - op={} args: {{{}}}", self.as_str(), sig)
        } else {
            format!("  - op={} args: {{{}}} ({})", self.as_str(), sig, hint)
        }
    }
}

pub const GENERAL_MUTATION_OPS: &[FileOpKind] = &[FileOpKind::Patch, FileOpKind::Rm, FileOpKind::Mv];
pub fn ops_label(ops: &[FileOpKind]) -> String {
    ops.iter()
        .map(|op| format!("op={}", op.as_str()))
        .collect::<Vec<_>>()
        .join("|")
}

pub fn ops_quoted_label(ops: &[FileOpKind]) -> String {
    ops.iter()
        .map(|op| format!("\"{}\"", op.as_str()))
        .collect::<Vec<_>>()
        .join("|")
}

pub fn general_mutation_ops_label() -> String {
    ops_label(GENERAL_MUTATION_OPS)
}

pub fn tool_card_header_for_ops(ops: &[FileOpKind]) -> String {
    let quoted = ops_quoted_label(ops);
    format!("- file(args:{{op:{}, ...}})", quoted)
}

pub fn tool_card_lines_for_ops(ops: &[FileOpKind]) -> Vec<String> {
    let mut out = Vec::with_capacity(ops.len() + 1);
    out.push(tool_card_header_for_ops(ops));
    for op in ops {
        out.push(op.tool_card_detail_line());
    }
    out
}

pub fn classify_file_op(args: &Value) -> FileOpKind {
    match args.get("op").and_then(|v| v.as_str()) {
        Some("get") => FileOpKind::Get,
        Some("list") => FileOpKind::List,
        Some("patch") => FileOpKind::Patch,
        Some("write") => FileOpKind::Write,
        Some("rm") => FileOpKind::Rm,
        Some("mv") => FileOpKind::Mv,
        _ => FileOpKind::Other,
    }
}

pub fn is_file_read_op(args: &Value) -> bool {
    matches!(classify_file_op(args), FileOpKind::Get | FileOpKind::List)
}

pub fn is_file_mutation_op(args: &Value) -> bool {
    matches!(
        classify_file_op(args),
        FileOpKind::Patch | FileOpKind::Write | FileOpKind::Rm | FileOpKind::Mv
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ops_label_renders_correctly() {
        assert_eq!(ops_label(&[FileOpKind::Patch]), "op=patch");
        assert_eq!(ops_label(&[FileOpKind::Rm, FileOpKind::Mv]), "op=rm|op=mv");
        assert_eq!(
            ops_label(GENERAL_MUTATION_OPS),
            "op=patch|op=rm|op=mv"
        );
    }

    #[test]
    fn tool_card_lines_contain_header_and_detail() {
        let lines = tool_card_lines_for_ops(&[FileOpKind::Patch]);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("\"patch\""));
        assert!(lines[1].contains("patch_text"));
    }

    #[test]
    fn general_mutation_excludes_write() {
        assert!(!GENERAL_MUTATION_OPS.contains(&FileOpKind::Write));
        assert!(GENERAL_MUTATION_OPS.contains(&FileOpKind::Patch));
    }
}
