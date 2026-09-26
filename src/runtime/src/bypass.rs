/// Temporary Skippr Cloud platform bypasses. Hard-on so a local OpenAI key
/// and local/R2 storage work without login, credits, or the auth API.
/// Remove when those services are wired again.

/// Skippr Cloud metering/credits. When true, LLM pre-call guards and usage
/// reporting are no-ops.
pub const BYPASS_BILLING: bool = true;

/// Skippr Cloud auth API (login, API-key exchange, EULA, GetCredentials,
/// run locks). When true, hosts MUST NOT require those calls.
pub const BYPASS_AUTH: bool = true;

#[cfg(test)]
mod tests {
    #[test]
    fn billing_and_auth_are_hard_bypassed() {
        assert!(super::BYPASS_BILLING);
        assert!(super::BYPASS_AUTH);
    }
}
