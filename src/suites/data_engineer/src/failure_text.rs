use crate::failure_kind::FailureKind;

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

pub fn normalize_text(s: &str) -> String {
    s.to_ascii_lowercase()
}

pub fn normalize_errors(errors: &[String]) -> String {
    normalize_text(&errors.join("\n"))
}

pub fn is_infra_transient(s: &str) -> bool {
    contains_any(
        s,
        &[
            // Network / HTTP
            "timeout",
            "timed out",
            "temporar",
            "temporarily",
            "http 502",
            "http 503",
            "http 504",
            "bad gateway",
            "gateway timeout",
            "service unavailable",
            "connection reset",
            "connection aborted",
            "connection refused",
            "network error",
            "econnrefused",
            "econnreset",
            "etimedout",
            // AWS / cloud provider
            "service error",
            "internal error",
            "internal server error",
            "internalserverexception",
            "serviceexception",
            "throttlingexception",
            "toomanyrequestsexception",
            "rate exceeded",
            "slow down",
            "request limit",
            "provisioned throughput",
        ],
    )
}

pub fn is_warehouse_config(s: &str) -> bool {
    contains_any(
        s,
        &[
            "workgroup is not found",
            "datacatalog",
            "accessdenied",
            "expiredtoken",
            "signaturedoesnotmatch",
        ],
    ) && (!s.contains("sql validation"))
}

pub fn is_missing_source(s: &str) -> bool {
    (s.contains("depends on a source named") && s.contains("which was not found"))
        || (s.contains("source named") && s.contains("was not found"))
}

pub fn is_schema_or_contract(s: &str) -> bool {
    contains_any(
        s,
        &[
            "schema",
            "schema.yml",
            "yaml",
            "profiles.yml",
            "dbt_project.yml",
            "additional properties are not allowed",
            "contract",
            "parse",
            "invalid model folder",
        ],
    )
}

pub fn is_sql_or_runtime_strict(s: &str) -> bool {
    contains_any(
        s,
        &[
            "sql validation",
            "compilation error",
            "runtime error",
            "database error",
            "failed to execute query",
            "invalidrequestexception",
            "athena/trino",
            "trino",
            "materialized sql",
            "source() call",
            "dbt source()",
            "syntax error",
        ],
    )
}

pub fn is_sql_or_runtime(s: &str) -> bool {
    is_sql_or_runtime_strict(s) || s.contains("sql")
}

pub fn classify_dbt_failure(errors: &[String]) -> FailureKind {
    if errors.is_empty() {
        return FailureKind::NoFailure;
    }
    let s = normalize_errors(errors);
    if is_infra_transient(&s) {
        return FailureKind::InfraTransient;
    }
    if is_warehouse_config(&s) {
        return FailureKind::WarehouseConfig;
    }
    if is_missing_source(&s) {
        return FailureKind::MissingSource;
    }
    if is_schema_or_contract(&s) {
        return FailureKind::Schema;
    }
    if is_sql_or_runtime(&s) {
        return FailureKind::SqlRuntime;
    }
    FailureKind::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_dbt_failure_missing_source() {
        let errors = vec![
            "Compilation Error: depends on a source named 'x.y' which was not found".to_string(),
        ];
        assert_eq!(classify_dbt_failure(&errors), FailureKind::MissingSource);
    }

    #[test]
    fn classify_dbt_failure_sql_runtime() {
        let errors = vec!["Runtime Error: syntax error at or near FROM".to_string()];
        assert_eq!(classify_dbt_failure(&errors), FailureKind::SqlRuntime);
    }

    #[test]
    fn classify_dbt_failure_service_error_is_transient() {
        let errors = vec!["sql validation failed: service error".to_string()];
        assert_eq!(classify_dbt_failure(&errors), FailureKind::InfraTransient);
    }

    #[test]
    fn classify_dbt_failure_throttling_is_transient() {
        let errors = vec!["ThrottlingException: rate exceeded".to_string()];
        assert_eq!(classify_dbt_failure(&errors), FailureKind::InfraTransient);
    }

    #[test]
    fn classify_dbt_failure_internal_server_is_transient() {
        let errors = vec!["InternalServerException: An internal error occurred".to_string()];
        assert_eq!(classify_dbt_failure(&errors), FailureKind::InfraTransient);
    }

    #[test]
    fn is_infra_transient_covers_aws_patterns() {
        assert!(is_infra_transient("service error"));
        assert!(is_infra_transient("internal server error"));
        assert!(is_infra_transient("internalserverexception"));
        assert!(is_infra_transient("serviceexception: something went wrong"));
        assert!(is_infra_transient("throttlingexception: rate exceeded"));
        assert!(is_infra_transient("toomanyrequestsexception"));
        assert!(is_infra_transient("slow down"));
        assert!(is_infra_transient("request limit exceeded"));
        assert!(is_infra_transient("timed out waiting for response"));
        assert!(is_infra_transient("bad gateway"));
        assert!(is_infra_transient("gateway timeout"));
        assert!(is_infra_transient("service unavailable"));
    }

    #[test]
    fn is_infra_transient_does_not_match_sql_errors() {
        assert!(!is_infra_transient("syntax error at or near select"));
        assert!(!is_infra_transient("compilation error in model"));
    }
}
