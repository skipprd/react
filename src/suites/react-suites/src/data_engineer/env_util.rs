pub fn getenv_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(|v| {
        let t = v.trim();
        if t.is_empty() { None } else { Some(t.to_string()) }
    })
}

pub fn env_u32(key: &str) -> Option<u32> {
    std::env::var(key)
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
}

pub fn env_usize(key: &str) -> Option<usize> {
    std::env::var(key)
        .ok()
        .and_then(|s| s.trim().parse::<usize>().ok())
}

pub fn env_bool_truthy(key: &str) -> Option<bool> {
    std::env::var(key).ok().map(|v| {
        let t = v.trim().to_ascii_lowercase();
        !(t.is_empty() || t == "0" || t == "false" || t == "no")
    })
}

// ---------------------------------------------------------------------------
// Centralized env-var key names
// ---------------------------------------------------------------------------

pub mod env_keys {
    // Agent orchestration
    pub const AGENT_MAX_PHASE_STEPS: &str = "AGENT_MAX_PHASE_STEPS";
    pub const AGENT_MAX_REPLAN_BACKTRACKS: &str = "AGENT_MAX_REPLAN_BACKTRACKS";
    pub const AGENT_MAX_PUBLISH_RETRIES: &str = "AGENT_MAX_PUBLISH_RETRIES";
    pub const AGENT_MAX_SUBJECTIVE_RETRIES: &str = "AGENT_MAX_SUBJECTIVE_RETRIES";

    // Suite runtime
    pub const REACT_HEADLESS: &str = "REACT_HEADLESS";
    pub const DE_CATALOG_BOOTSTRAP_TIMEOUT_SECS: &str = "DE_CATALOG_BOOTSTRAP_TIMEOUT_SECS";

    // Planning LLM tokens
    pub const LLM_PLAN_MAX_TOKENS_CLEANSE: &str = "LLM_PLAN_MAX_TOKENS_CLEANSE";
    pub const LLM_PLAN_MAX_TOKENS_MODEL: &str = "LLM_PLAN_MAX_TOKENS_MODEL";
    pub const LLM_PLAN_MEMO_MAX_TOKENS: &str = "LLM_PLAN_MEMO_MAX_TOKENS";
    pub const LLM_PLAN_CRITIQUE_MAX_TOKENS: &str = "LLM_PLAN_CRITIQUE_MAX_TOKENS";
    pub const LLM_PLAN_SKELETON_MAX_TOKENS: &str = "LLM_PLAN_SKELETON_MAX_TOKENS";
    pub const LLM_PLAN_ENRICH_MAX_TOKENS: &str = "LLM_PLAN_ENRICH_MAX_TOKENS";
    pub const LLM_PLAN_ENRICH_REASON_MAX_TOKENS: &str = "LLM_PLAN_ENRICH_REASON_MAX_TOKENS";
    pub const LLM_PLAN_ENRICH_CHUNK_SIZE: &str = "LLM_PLAN_ENRICH_CHUNK_SIZE";
    pub const LLM_MODEL_PLAN_MIN_SCORE: &str = "LLM_MODEL_PLAN_MIN_SCORE";

    // Planning reasoning effort
    pub const LLM_PLAN_REASONING_EFFORT: &str = "LLM_PLAN_REASONING_EFFORT";
    pub const LLM_PLAN_REASONING_EFFORT_CLEANSE: &str = "LLM_PLAN_REASONING_EFFORT_CLEANSE";
    pub const LLM_PLAN_REASONING_EFFORT_MODEL: &str = "LLM_PLAN_REASONING_EFFORT_MODEL";

    // Authoring LLM tokens
    pub const LLM_AUTHOR_MAX_TOKENS_CLEANSE: &str = "LLM_AUTHOR_MAX_TOKENS_CLEANSE";
    pub const LLM_AUTHOR_MAX_TOKENS_MODEL: &str = "LLM_AUTHOR_MAX_TOKENS_MODEL";

    // Review LLM tokens
    pub const LLM_REVIEW_REASONING_EFFORT: &str = "LLM_REVIEW_REASONING_EFFORT";
    pub const LLM_REVIEW_MAX_TOKENS: &str = "LLM_REVIEW_MAX_TOKENS";
    pub const LLM_REVIEW_MAX_TOKENS_UNIFY: &str = "LLM_REVIEW_MAX_TOKENS_UNIFY";
    pub const LLM_REVIEW_MAX_TOKENS_CLEANSE: &str = "LLM_REVIEW_MAX_TOKENS_CLEANSE";
    pub const LLM_REVIEW_MAX_TOKENS_CLEANSE_UNIFY: &str = "LLM_REVIEW_MAX_TOKENS_CLEANSE_UNIFY";
    pub const LLM_REVIEW_MAX_TOKENS_MODEL: &str = "LLM_REVIEW_MAX_TOKENS_MODEL";
    pub const LLM_REVIEW_MAX_TOKENS_MODEL_UNIFY: &str = "LLM_REVIEW_MAX_TOKENS_MODEL_UNIFY";
    pub const LLM_REVIEW_MAX_TOKENS_POSTPUBLISH: &str = "LLM_REVIEW_MAX_TOKENS_POSTPUBLISH";
    pub const LLM_REVIEW_MAX_TOKENS_POSTPUBLISH_UNIFY: &str = "LLM_REVIEW_MAX_TOKENS_POSTPUBLISH_UNIFY";

    // SQL-first
    pub const REACT_SQL_FIRST_MAX_OUTPUT_TOKENS: &str = "REACT_SQL_FIRST_MAX_OUTPUT_TOKENS";
    pub const REACT_SQL_FIRST_MAX_REPAIR_ATTEMPTS: &str = "REACT_SQL_FIRST_MAX_REPAIR_ATTEMPTS";

    // Patch protocol
    pub const REACT_PATCH_LOOP_MAX_OUTPUT_TOKENS: &str = "REACT_PATCH_LOOP_MAX_OUTPUT_TOKENS";

    // dbt policy
    pub const DBT_ALLOW_COMPILE_ONLY_COMPLETE: &str = "DBT_ALLOW_COMPILE_ONLY_COMPLETE";

    // dbt config (also read from YAML; env takes priority)
    pub const DBT_TARGET_SCHEMA: &str = "DBT_TARGET_SCHEMA";
    pub const DBT_SILVER_SUFFIX: &str = "DBT_SILVER_SUFFIX";
    pub const DBT_GOLD_SUFFIX: &str = "DBT_GOLD_SUFFIX";
    pub const DBT_PROFILES_DIR: &str = "DBT_PROFILES_DIR";
    pub const DBT_TARGET: &str = "DBT_TARGET";
    pub const DBT_RUNNER: &str = "DBT_RUNNER";
    pub const DBT_DOCKER_IMAGE: &str = "DBT_DOCKER_IMAGE";
    pub const DBT_DOCKER_PLATFORM: &str = "DBT_DOCKER_PLATFORM";
    pub const DBT_DOCKER_NETWORK: &str = "DBT_DOCKER_NETWORK";
    pub const DBT_DOCKER_MOUNT_AWS_DIR: &str = "DBT_DOCKER_MOUNT_AWS_DIR";
    pub const DBT_REPAIR_MAX_ITERS: &str = "DBT_REPAIR_MAX_ITERS";
}

// ---------------------------------------------------------------------------
// Centralized reader functions (with defaults, clamping, etc.)
// ---------------------------------------------------------------------------

pub fn max_phase_steps() -> usize {
    env_usize(env_keys::AGENT_MAX_PHASE_STEPS)
        .unwrap_or(DEFAULT_MAX_PHASE_STEPS)
        .max(MIN_PHASE_STEPS)
        .min(MAX_PHASE_STEPS)
}

pub fn max_replan_backtracks() -> usize {
    env_usize(env_keys::AGENT_MAX_REPLAN_BACKTRACKS)
        .unwrap_or(DEFAULT_MAX_REPLAN_BACKTRACKS)
        .max(MIN_REPLAN_BACKTRACKS)
        .min(MAX_REPLAN_BACKTRACKS)
}

pub fn max_publish_retries() -> usize {
    env_usize(env_keys::AGENT_MAX_PUBLISH_RETRIES)
        .unwrap_or(DEFAULT_MAX_PUBLISH_RETRIES)
        .max(1)
        .min(12)
}

pub fn plan_enrich_chunk_size() -> usize {
    env_usize(env_keys::LLM_PLAN_ENRICH_CHUNK_SIZE)
        .unwrap_or(DEFAULT_PLAN_ENRICH_CHUNK_SIZE)
        .clamp(MIN_PLAN_ENRICH_CHUNK_SIZE, MAX_PLAN_ENRICH_CHUNK_SIZE)
}

pub fn model_plan_min_score() -> i32 {
    std::env::var(env_keys::LLM_MODEL_PLAN_MIN_SCORE)
        .ok()
        .and_then(|s| s.parse::<i32>().ok())
        .map(|p| p.clamp(MIN_MODEL_PLAN_SCORE, MAX_MODEL_PLAN_SCORE))
        .unwrap_or(DEFAULT_MODEL_PLAN_MIN_SCORE)
}

pub fn headless_mode_enabled() -> bool {
    env_bool_truthy(env_keys::REACT_HEADLESS).unwrap_or(false)
}

pub fn catalog_bootstrap_timeout_secs() -> u64 {
    env_u32(env_keys::DE_CATALOG_BOOTSTRAP_TIMEOUT_SECS)
        .unwrap_or(DEFAULT_CATALOG_BOOTSTRAP_TIMEOUT_SECS as u32)
        .max(30)
        .min(1800) as u64
}

pub fn dbt_allow_compile_only_complete() -> bool {
    std::env::var(env_keys::DBT_ALLOW_COMPILE_ONLY_COMPLETE)
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn sql_first_max_output_tokens(default: u32) -> u32 {
    env_u32(env_keys::REACT_SQL_FIRST_MAX_OUTPUT_TOKENS)
        .unwrap_or(default)
        .max(800)
        .min(16_000)
}

pub fn sql_first_max_repair_attempts(default: usize) -> usize {
    env_usize(env_keys::REACT_SQL_FIRST_MAX_REPAIR_ATTEMPTS)
        .unwrap_or(default)
        .max(1)
        .min(8)
}

// ---------------------------------------------------------------------------
// Named defaults and bounds for agent-context / orchestration constants
// ---------------------------------------------------------------------------

pub const DEFAULT_TOP_K: usize = 30;

pub const ASK_MAX_STEPS: usize = 50;
pub const REVIEW_MAX_STEPS: usize = 40;
pub const PLAN_MAX_STEPS: usize = 40;
pub const APPROVAL_PARSE_MAX_STEPS: usize = 6;

pub const DEFAULT_MAX_PHASE_STEPS: usize = 40;
pub const MIN_PHASE_STEPS: usize = 8;
pub const MAX_PHASE_STEPS: usize = 400;

pub const DEFAULT_MAX_REPLAN_BACKTRACKS: usize = 3;
pub const MIN_REPLAN_BACKTRACKS: usize = 2;
pub const MAX_REPLAN_BACKTRACKS: usize = 20;

pub const DEFAULT_MAX_PUBLISH_RETRIES: usize = 3;

pub const DEFAULT_PLAN_ENRICH_CHUNK_SIZE: usize = 3;
pub const MIN_PLAN_ENRICH_CHUNK_SIZE: usize = 1;
pub const MAX_PLAN_ENRICH_CHUNK_SIZE: usize = 3;

pub const DEFAULT_MODEL_PLAN_MIN_SCORE: i32 = 70;
pub const MIN_MODEL_PLAN_SCORE: i32 = 0;
pub const MAX_MODEL_PLAN_SCORE: i32 = 100;

pub const DEFAULT_CATALOG_BOOTSTRAP_TIMEOUT_SECS: u64 = 300;

// ---------------------------------------------------------------------------
// Tool timeout tiers (seconds)
// ---------------------------------------------------------------------------

pub const TOOL_TIMEOUT_FAST_SECS: u64 = 120;
pub const TOOL_TIMEOUT_MEDIUM_SECS: u64 = 300;
pub const TOOL_TIMEOUT_SLOW_SECS: u64 = 600;
pub const TOOL_TIMEOUT_EXTRA_SLOW_SECS: u64 = 900;

// ---------------------------------------------------------------------------
// Project identity constants
// ---------------------------------------------------------------------------

pub const SUITE_PROJECT_NAME: &str = "data_engineer";
pub const DEFAULT_AGENT_NAME: &str = "agent";
pub const UNKNOWN_AGENT: &str = "unknown";

// ---------------------------------------------------------------------------
// File-operation limit defaults used in deterministic invariant checks
// ---------------------------------------------------------------------------

pub const FILE_LIST_LIMIT: u64 = 500;
pub const FILE_GET_MAX_CHARS: u64 = 2000;
