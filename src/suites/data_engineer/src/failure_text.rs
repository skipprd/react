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

pub fn classify_dbt_failure(errors: &[String]) -> FailureKind {
    if errors.is_empty() {
        return FailureKind::Unknown;
    }
    let s = normalize_errors(errors);
    if is_infra_transient(&s) {
        FailureKind::InfraTransient
    } else {
        FailureKind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_dbt_failure_non_infra_is_unknown() {
        let errors = vec!["Runtime Error: syntax error at or near FROM".to_string()];
        assert_eq!(classify_dbt_failure(&errors), FailureKind::Unknown);
    }

    #[test]
    fn classify_dbt_failure_missing_source_is_unknown() {
        let errors = vec![
            "Compilation Error: depends on a source named 'x.y' which was not found".to_string(),
        ];
        assert_eq!(classify_dbt_failure(&errors), FailureKind::Unknown);
    }

    #[test]
    fn classify_dbt_failure_schema_error_is_unknown() {
        let errors = vec!["Compilation Error: schema.yml parse failure".to_string()];
        assert_eq!(classify_dbt_failure(&errors), FailureKind::Unknown);
    }

    #[test]
    fn classify_dbt_failure_warehouse_config_is_unknown() {
        let errors = vec!["accessdenied: user is not authorized".to_string()];
        assert_eq!(classify_dbt_failure(&errors), FailureKind::Unknown);
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

    #[test]
    fn classify_dbt_failure_empty_is_unknown() {
        assert_eq!(classify_dbt_failure(&[]), FailureKind::Unknown);
    }

    #[test]
    fn old_variants_deserialize_as_unknown() {
        let old = r#""missing_source""#;
        let kind: FailureKind = serde_json::from_str(old).unwrap();
        assert_eq!(kind, FailureKind::Unknown);

        let old = r#""no_failure""#;
        let kind: FailureKind = serde_json::from_str(old).unwrap();
        assert_eq!(kind, FailureKind::Unknown);
    }
}
