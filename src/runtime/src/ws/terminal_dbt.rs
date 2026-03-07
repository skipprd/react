/// DBT-specific rendering helpers for the terminal UI.
///
/// All dbt-validate label parsing, normalisation and sort-ranking lives here
/// so that the main terminal module has no hardcoded dbt knowledge.

pub(super) const DBT_VALIDATE_TOOL_NAME: &str = "dbt_validate";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DbtValidateKind {
    Compile,
    Build,
    Full,
}

pub(super) fn dbt_validate_kind_from_label(label: &str) -> Option<DbtValidateKind> {
    let s = label.trim().to_ascii_lowercase();
    if !s.contains("validate dbt") {
        return None;
    }
    if s.contains("compile") {
        return Some(DbtValidateKind::Compile);
    }
    if s.contains("build") {
        return Some(DbtValidateKind::Build);
    }
    Some(DbtValidateKind::Full)
}

pub(super) fn is_dbt_validate_label(label: &str) -> bool {
    label.trim().to_ascii_lowercase().contains("validate dbt")
}

pub(super) fn normalize_validate_dbt_label(label: &str) -> String {
    let s = label.trim();
    s.replace(" (build)", " Build")
        .replace(" (compile)", " Compile")
}

pub(super) fn validate_dbt_sort_rank(label: &str) -> Option<u8> {
    let s = normalize_validate_dbt_label(label)
        .trim()
        .to_ascii_lowercase();
    if !s.starts_with("validate dbt") {
        return None;
    }
    if s.starts_with("validate dbt compile") {
        return Some(0);
    }
    if s.starts_with("validate dbt build") {
        return Some(1);
    }
    Some(2)
}
