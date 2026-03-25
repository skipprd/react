mod auth;
mod api_client;
mod public_config;
mod translate;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use public_config::{DbtConfig, S3Transform, SkipprDbtConfig, SourceConfig, WarehouseConfig};

#[derive(Parser, Debug)]
#[command(name = "skippr-dbt", about = "Data pipeline CLI — extract, load, and model with dbt")]
struct Cli {
    /// Log level (info, debug, trace). When omitted the live terminal UI is shown.
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "info")]
    log: Option<String>,

    /// Path to config file. Defaults to ./skippr-dbt.yaml in the working directory.
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Initialise a new project.
    Init {
        /// Project name (used as the pipeline identifier and default dbt schema).
        name: String,
    },

    /// Configure a warehouse or source connection.
    Connect {
        #[command(subcommand)]
        target: ConnectTarget,
    },

    /// Check that all prerequisites are in place.
    Doctor,

    /// Execute the full pipeline (extract, load, model).
    Run,

    /// User account management (signup, login, balance, etc.).
    User {
        #[command(subcommand)]
        action: UserAction,
    },
}

#[derive(Subcommand, Debug)]
enum UserAction {
    /// Sign up or log in with your phone number.
    Login,
    /// Log out and remove local credentials.
    Logout,
    /// Show account balance and recent usage.
    Account,
    /// Purchase a credit pack.
    BuyCredits {
        /// Credit pack: starter (500), growth (2000), scale (10000).
        #[arg(long)]
        pack: String,
    },
    /// Show detailed usage log.
    Usage,
    /// Create a new API key for CI/CD or automation.
    CreateApiKey {
        /// Human-readable label (e.g. "github-actions").
        #[arg(long)]
        name: String,
    },
    /// Revoke an existing API key.
    RevokeApiKey {
        /// The key_id to revoke (from list-api-keys output).
        #[arg(long)]
        key_id: String,
    },
    /// List all API keys for your account.
    ListApiKeys,
}

#[derive(Subcommand, Debug)]
enum ConnectTarget {
    /// Configure the destination warehouse.
    Warehouse {
        #[command(subcommand)]
        kind: WarehouseKind,
    },
    /// Configure the data source for extraction.
    Source {
        #[command(subcommand)]
        kind: SourceKind,
    },
}

#[derive(Subcommand, Debug)]
enum WarehouseKind {
    /// Snowflake warehouse.
    Snowflake {
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        schema: Option<String>,
        #[arg(long)]
        warehouse: Option<String>,
        #[arg(long)]
        role: Option<String>,
    },
    /// Google BigQuery warehouse.
    Bigquery {
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        dataset: Option<String>,
        #[arg(long)]
        location: Option<String>,
    },
    /// PostgreSQL warehouse.
    Postgres {
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        schema: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum SourceKind {
    /// Microsoft SQL Server source.
    Mssql {
        /// ADO.NET connection string, or use ${ENV_VAR} notation.
        #[arg(long)]
        connection_string: Option<String>,
    },
    /// S3 bucket source.
    S3 {
        #[arg(long)]
        bucket: Option<String>,
        #[arg(long)]
        prefix: Option<String>,
        /// Field(s) used to namespace incoming events (e.g. event_type).
        #[arg(long)]
        namespace_fields: Option<String>,
    },
}

fn working_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn config_path(explicit: &Option<PathBuf>) -> PathBuf {
    explicit
        .clone()
        .unwrap_or_else(|| working_dir().join("skippr-dbt.yaml"))
}

fn load_config(explicit: &Option<PathBuf>) -> Result<SkipprDbtConfig, String> {
    SkipprDbtConfig::load_from(&config_path(explicit))
}

fn save_config(cfg: &SkipprDbtConfig, explicit: &Option<PathBuf>) -> Result<(), String> {
    cfg.save_to(&config_path(explicit))
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

fn cmd_init(name: &str, explicit_config: &Option<PathBuf>) {
    let path = config_path(explicit_config);
    if path.exists() {
        eprintln!(
            "{} already exists. Delete it first to re-initialise.",
            path.display()
        );
        std::process::exit(1);
    }

    let cfg = SkipprDbtConfig {
        project: name.to_string(),
        warehouse: None,
        source: None,
        dbt: None,
    };
    if let Err(e) = cfg.save_to(&path) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }

    let env_example = path
        .parent()
        .map(|p| p.join(".env.example"))
        .unwrap_or_else(|| PathBuf::from(".env.example"));
    if !env_example.exists() {
        let _ = std::fs::write(
            &env_example,
            "\
# Authentication (required — choose one)
# Interactive: skippr-dbt user login
# CI/CD: set SKIPPR_API_KEY
SKIPPR_API_KEY=sk_live_...

# Optional: override the server-provided LLM key with your own
# LLM_API_KEY=sk-...

# Snowflake (when warehouse is snowflake)
SNOWFLAKE_ACCOUNT=
SNOWFLAKE_USER=
SNOWFLAKE_PRIVATE_KEY_PATH=

# PostgreSQL (when warehouse is postgres)
PGHOST=localhost
PGPORT=5432
PGUSER=postgres
PGPASSWORD=
PGDATABASE=

# MSSQL (when source is mssql)
MSSQL_CONNECTION_STRING=
",
        );
    }

    println!("Initialised project '{}' — {}", name, path.display());
    println!();
    println!("Next steps:");
    println!("  skippr-dbt connect warehouse snowflake");
    println!("  skippr-dbt connect source mssql");
    println!("  skippr-dbt doctor");
    println!("  skippr-dbt run");
}

// ---------------------------------------------------------------------------
// connect warehouse
// ---------------------------------------------------------------------------

fn cmd_connect_warehouse(kind: WarehouseKind, explicit_config: &Option<PathBuf>) {
    let mut cfg = match load_config(explicit_config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("Run 'skippr-dbt init <project>' first.");
            std::process::exit(1);
        }
    };

    let wh = match kind {
        WarehouseKind::Snowflake {
            database,
            schema,
            warehouse,
            role,
        } => {
            let database = database.or_else(|| prompt("Snowflake database"));
            let schema = schema.or_else(|| prompt("Snowflake schema (bronze/raw)"));
            let warehouse = warehouse.or_else(|| prompt("Snowflake compute warehouse"));
            let role = role.or_else(|| prompt("Snowflake role"));
            WarehouseConfig::Snowflake {
                database,
                schema,
                warehouse,
                role,
            }
        }
        WarehouseKind::Bigquery {
            project,
            dataset,
            location,
        } => {
            let project = project.or_else(|| prompt("BigQuery GCP project"));
            let dataset = dataset.or_else(|| prompt("BigQuery dataset"));
            let location = location.or_else(|| prompt("BigQuery location (e.g. US)"));
            WarehouseConfig::Bigquery {
                project,
                dataset,
                location,
            }
        }
        WarehouseKind::Postgres { database, schema } => {
            let database = database.or_else(|| prompt("PostgreSQL database"));
            let schema = schema.or_else(|| prompt("PostgreSQL schema (default: public)"));
            WarehouseConfig::Postgres { database, schema }
        }
    };

    let kind_label = wh.kind_str();
    cfg.warehouse = Some(wh);

    if cfg.dbt.is_none() {
        cfg.dbt = Some(DbtConfig::default());
    }

    if let Err(e) = save_config(&cfg, explicit_config) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }

    println!("Warehouse ({}) configured.", kind_label);
}

// ---------------------------------------------------------------------------
// connect source
// ---------------------------------------------------------------------------

fn cmd_connect_source(kind: SourceKind, explicit_config: &Option<PathBuf>) {
    let mut cfg = match load_config(explicit_config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("Run 'skippr-dbt init <project>' first.");
            std::process::exit(1);
        }
    };

    let src = match kind {
        SourceKind::Mssql { connection_string } => {
            let connection_string = connection_string.or_else(|| {
                prompt("MSSQL connection string (or ${MSSQL_CONNECTION_STRING} to read from env)")
            });
            SourceConfig::Mssql { connection_string }
        }
        SourceKind::S3 {
            bucket,
            prefix,
            namespace_fields,
        } => {
            let bucket = bucket.or_else(|| prompt("S3 bucket"));
            let prefix = prefix.or_else(|| prompt("S3 prefix"));
            let namespace_fields = namespace_fields.or_else(|| prompt("Namespace fields (optional, e.g. event_type)"));
            let transform = namespace_fields.map(|nf| S3Transform {
                namespace_fields: Some(nf),
            });
            SourceConfig::S3 {
                s3_bucket: bucket,
                s3_prefix: prefix,
                transform,
            }
        }
    };

    let kind_label = cfg
        .source_kind_str()
        .unwrap_or_else(|| match &src {
            SourceConfig::Mssql { .. } => "mssql",
            SourceConfig::S3 { .. } => "s3",
        });
    cfg.source = Some(src);

    if let Err(e) = save_config(&cfg, explicit_config) {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }

    println!("Source ({}) configured.", kind_label);
}

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

fn cmd_doctor(explicit_config: &Option<PathBuf>) {
    let mut ok = true;

    let cfg = match load_config(explicit_config) {
        Ok(c) => {
            check_pass("skippr-dbt.yaml found");
            c
        }
        Err(_) => {
            check_fail("skippr-dbt.yaml not found — run 'skippr-dbt init <project>'");
            std::process::exit(1);
        }
    };

    if cfg.warehouse.is_some() {
        check_pass(&format!(
            "warehouse configured ({})",
            cfg.warehouse_kind_str().unwrap_or("unknown")
        ));
    } else {
        check_fail("warehouse not configured — run 'skippr-dbt connect warehouse <kind>'");
        ok = false;
    }

    if cfg.source.is_some() {
        check_pass(&format!(
            "source configured ({})",
            cfg.source_kind_str().unwrap_or("unknown")
        ));
    } else {
        check_fail("source not configured — run 'skippr-dbt connect source <kind>'");
        ok = false;
    }

    if which("skippr") {
        check_pass("skippr binary found on PATH");
    } else {
        check_fail("skippr binary not found on PATH");
        ok = false;
    }

    if which("dbt") {
        check_pass("dbt binary found on PATH");
    } else {
        check_fail("dbt not found on PATH — install dbt-core and the warehouse adapter in a venv");
        ok = false;
    }

    if which("python3") || which("python") {
        check_pass("python found on PATH");
    } else {
        check_fail("python not found on PATH — Python 3.10+ is required for dbt");
        ok = false;
    }

    if auth::load_credentials().is_some() || env_set("SKIPPR_API_KEY") {
        check_pass("authenticated (credentials or SKIPPR_API_KEY)");
    } else {
        check_fail("not authenticated — run 'skippr-dbt user login' or set SKIPPR_API_KEY");
        ok = false;
    }

    if std::env::var("LLM_API_KEY")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_some()
    {
        check_pass("LLM_API_KEY is set (custom key — overrides server-provided key)");
    } else {
        check_pass("LLM_API_KEY not set (will use server-provided key)");
    }

    if let Some(WarehouseConfig::Snowflake { .. }) = &cfg.warehouse {
        check_snowflake_env(&mut ok);
    }

    if let Some(WarehouseConfig::Postgres { .. }) = &cfg.warehouse {
        check_postgres_env(&mut ok);
    }

    println!();
    if ok {
        println!("All checks passed. Run 'skippr-dbt run' to start.");
    } else {
        println!("Some checks failed. Fix the issues above and re-run 'skippr-dbt doctor'.");
        std::process::exit(1);
    }
}

fn check_snowflake_env(ok: &mut bool) {
    let has_account = env_set("SNOWFLAKE_ACCOUNT");
    let has_user = env_set("SNOWFLAKE_USER");
    let has_key = env_set("SNOWFLAKE_PRIVATE_KEY_PATH");
    let has_pw = env_set("SNOWFLAKE_PASSWORD");

    if has_account {
        check_pass("SNOWFLAKE_ACCOUNT is set");
    } else {
        check_fail("SNOWFLAKE_ACCOUNT is not set");
        *ok = false;
    }
    if has_user {
        check_pass("SNOWFLAKE_USER is set");
    } else {
        check_fail("SNOWFLAKE_USER is not set");
        *ok = false;
    }
    if has_key {
        check_pass("SNOWFLAKE_PRIVATE_KEY_PATH is set (key-pair auth)");
    } else if has_pw {
        check_pass("SNOWFLAKE_PASSWORD is set (password auth)");
    } else {
        check_fail("Snowflake auth not configured — set SNOWFLAKE_PRIVATE_KEY_PATH or SNOWFLAKE_PASSWORD");
        *ok = false;
    }
}

fn check_postgres_env(ok: &mut bool) {
    let has_host = env_set("PGHOST");
    let has_user = env_set("PGUSER");
    let has_password = env_set("PGPASSWORD");

    if has_host {
        check_pass("PGHOST is set");
    } else {
        check_fail("PGHOST is not set (defaults to localhost)");
    }
    if has_user {
        check_pass("PGUSER is set");
    } else {
        check_fail("PGUSER is not set (defaults to postgres)");
    }
    if has_password {
        check_pass("PGPASSWORD is set");
    } else {
        check_fail("PGPASSWORD is not set");
        *ok = false;
    }
}

fn env_set(key: &str) -> bool {
    std::env::var(key)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_some()
}

fn which(bin: &str) -> bool {
    std::process::Command::new("which")
        .arg(bin)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn check_pass(msg: &str) {
    println!("  [ok]   {}", msg);
}

fn check_fail(msg: &str) {
    println!("  [FAIL] {}", msg);
}

// ---------------------------------------------------------------------------
// run
// ---------------------------------------------------------------------------

async fn cmd_run(log: Option<String>, explicit_config: &Option<PathBuf>) {
    let cfg = match load_config(explicit_config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("Run 'skippr-dbt init <project>' first.");
            std::process::exit(1);
        }
    };

    let mut internal_file = match translate::to_internal(&cfg) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    // Authentication is mandatory. SKIPPR_API_KEY env var takes priority, then credentials.json.
    let creds = if let Ok(api_key) = std::env::var("SKIPPR_API_KEY") {
        if api_key.trim().is_empty() {
            eprintln!("[skippr-dbt] ERROR: SKIPPR_API_KEY is set but empty.");
            std::process::exit(1);
        }
        let base_url = auth::auth_base_url();
        let client = api_client::ApiClient::new(&base_url);
        match client.exchange_api_key(api_key.trim()).await {
            Ok(tokens) => {
                eprintln!("[skippr-dbt] authenticated via API key");
                tokens
            }
            Err(e) => {
                eprintln!("[skippr-dbt] ERROR: API key authentication failed: {}", e);
                std::process::exit(1);
            }
        }
    } else if let Some(creds) = auth::load_credentials() {
        eprintln!("[skippr-dbt] authenticated via stored credentials");
        creds
    } else {
        eprintln!("[skippr-dbt] ERROR: Authentication required.");
        eprintln!("[skippr-dbt]   Run 'skippr-dbt user login' to authenticate interactively,");
        eprintln!("[skippr-dbt]   or set SKIPPR_API_KEY for CI/CD.");
        std::process::exit(1);
    };

    let base_url = auth::auth_base_url();
    let client = api_client::ApiClient::new(&base_url);
    match client.get_credentials(&creds.access_token).await {
        Ok(srv_creds) => {
            translate::apply_authenticated_overlay(&mut internal_file, &srv_creds, &creds.access_token);
            eprintln!("[skippr-dbt] cloud storage + metering active");
        }
        Err(e) => {
            eprintln!("[skippr-dbt] ERROR: Failed to fetch server credentials: {}", e);
            eprintln!("[skippr-dbt]   Check your connection and login status.");
            std::process::exit(1);
        }
    }

    match client.get_account(&creds.access_token).await {
        Ok(account) => {
            let remaining = account.balance.credits_remaining;
            if remaining <= 0.0 {
                eprintln!("[skippr-dbt] ERROR: No credits remaining. Purchase credits to continue.");
                eprintln!("[skippr-dbt]   skippr-dbt user buy-credits --pack starter");
                std::process::exit(1);
            } else if remaining < LOW_BALANCE_THRESHOLD {
                eprintln!("[skippr-dbt] WARNING: Low balance ({:.1} credits). The run may exhaust your credits.", remaining);
            } else {
                eprintln!("[skippr-dbt] credits: {:.1} remaining", remaining);
            }
        }
        Err(e) => {
            eprintln!("[skippr-dbt] ERROR: Could not verify account balance ({}). Refusing to run.", e);
            eprintln!("[skippr-dbt]   Check your connection and login status (skippr-dbt user login).");
            std::process::exit(1);
        }
    }

    let resolved = match react::config::resolve_config(
        internal_file,
        react::config::ServeOverrides::default(),
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

    let mut reg = react_core::suite::SuiteRegistry::new();
    reg.register(react_suite_data_engineer::DataEngineerSuite);

    let thread_id = find_latest_thread(cfg.project.trim());
    if let Some(ref tid) = thread_id {
        eprintln!("[skippr-dbt] resuming thread {tid}");
    }

    let exit_code = react::run_engine::run_headless_from_config(
        resolved,
        reg,
        react::run_engine::HeadlessRunOpts {
            log_level: log,
            verbose_debug: false,
            terminal: false,
            thread_id,
            suite_id: None,
            agent: "agent".to_string(),
            skip_logging_init: false,
        },
    )
    .await;
    std::process::exit(exit_code);
}

/// Find the most recently modified primary thread for a project.
///
/// Scans `.skippr-dbt/local/dev/<project>/threads/` for `<uuid>.json` files
/// (skipping companion files like `<uuid>__gather_0.json`) and returns the
/// thread ID with the newest modification time, if any.
fn find_latest_thread(project: &str) -> Option<String> {
    let threads_dir = PathBuf::from(format!(
        ".skippr-dbt/local/dev/{}/threads",
        project.trim()
    ));
    let entries = std::fs::read_dir(&threads_dir).ok()?;

    let mut latest: Option<(String, std::time::SystemTime)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.ends_with(".json") || name.contains("__") {
            continue;
        }
        let stem = name.strip_suffix(".json")?;
        if stem.len() != 36 || stem.chars().filter(|c| *c == '-').count() != 4 {
            continue;
        }
        let modified = entry.metadata().ok()?.modified().ok()?;
        if latest.as_ref().map_or(true, |(_, t)| modified > *t) {
            latest = Some((stem.to_string(), modified));
        }
    }

    latest.map(|(tid, _)| tid)
}

// ---------------------------------------------------------------------------
// interactive prompt helper
// ---------------------------------------------------------------------------

fn prompt(label: &str) -> Option<String> {
    let result = dialoguer::Input::<String>::new()
        .with_prompt(label)
        .allow_empty(true)
        .interact_text();
    match result {
        Ok(v) if !v.trim().is_empty() => Some(v.trim().to_string()),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.cmd {
        Cmd::Init { name } => cmd_init(&name, &cli.config),
        Cmd::Connect { target } => match target {
            ConnectTarget::Warehouse { kind } => cmd_connect_warehouse(kind, &cli.config),
            ConnectTarget::Source { kind } => cmd_connect_source(kind, &cli.config),
        },
        Cmd::Doctor => cmd_doctor(&cli.config),
        Cmd::Run => cmd_run(cli.log, &cli.config).await,
        Cmd::User { action } => match action {
            UserAction::Login => cmd_user_login().await,
            UserAction::Logout => cmd_user_logout(),
            UserAction::Account => cmd_user_account().await,
            UserAction::BuyCredits { pack } => cmd_user_buy_credits(&pack).await,
            UserAction::Usage => cmd_user_usage().await,
            UserAction::CreateApiKey { name } => cmd_user_create_api_key(&name).await,
            UserAction::RevokeApiKey { key_id } => cmd_user_revoke_api_key(&key_id).await,
            UserAction::ListApiKeys => cmd_user_list_api_keys().await,
        },
    }
}

async fn cmd_user_login() {
    if auth::load_credentials().is_some() {
        eprintln!("Already logged in. Run 'skippr-dbt user logout' first to switch accounts.");
        std::process::exit(1);
    }

    println!("Enter your phone number (e.g. +1234567890):");
    let mut phone = String::new();
    std::io::stdin().read_line(&mut phone).unwrap();
    let phone = phone.trim();

    if phone.is_empty() || !phone.starts_with('+') {
        eprintln!("Invalid phone number. Must start with + and country code (e.g. +1234567890).");
        std::process::exit(1);
    }

    let base_url = auth::auth_base_url();
    let client = api_client::ApiClient::new(&base_url);

    match client.sign_in(phone).await {
        Ok(_) => {
            println!("Verification code sent to {}.", phone);
            println!("Enter the 6-digit code:");
            let mut code = String::new();
            std::io::stdin().read_line(&mut code).unwrap();
            let code = code.trim();

            match client.confirm(phone, code).await {
                Ok(tokens) => {
                    auth::save_credentials(&tokens);
                    println!();
                    println!("  Logged in successfully.");
                    println!();
                    println!("  Next steps:");
                    println!("    skippr-dbt user account       — view balance");
                    println!("    skippr-dbt user buy-credits   — purchase credits");
                    println!("    skippr-dbt run                — start a pipeline");
                    println!();
                }
                Err(e) => {
                    eprintln!("Confirmation failed: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Err(e) => {
            eprintln!("Sign-in failed: {}", e);
            std::process::exit(1);
        }
    }
}

fn cmd_user_logout() {
    auth::clear_credentials();
    println!("Logged out. Local credentials removed.");
}

async fn cmd_user_account() {
    let creds = auth::load_credentials();
    let Some(creds) = creds else {
        eprintln!("Not logged in. Run: skippr-dbt user login");
        std::process::exit(1);
    };
    let base_url = auth::auth_base_url();
    let client = api_client::ApiClient::new(&base_url);
    match client.get_account(&creds.access_token).await {
        Ok(account) => {
            println!();
            println!("  Account");
            println!("  {}", "-".repeat(50));
            println!("  Plan:              {}", account.profile.plan);
            println!("  Credits remaining: {:.1}", account.balance.credits_remaining);
            println!("  Credits purchased: {:.1}", account.balance.credits_purchased);
            println!("  Credits used:      {:.1}", account.balance.credits_used);
            println!("  Period:            {}", account.balance.period);
            if let Some(ref sub) = account.subscription {
                println!("  Subscription:      {} ({})", sub.status, sub.price_id);
            }
            println!();

            print_low_balance_warning(account.balance.credits_remaining);

            if !account.recent_usage.is_empty() {
                println!("  Recent usage (last 10):");
                println!("  {:<22} {:<22} {:>8}  {}", "Timestamp", "Event", "Credits", "Project");
                println!("  {}", "-".repeat(70));
                for u in account.recent_usage.iter().take(10) {
                    println!("  {:<22} {:<22} {:>8.1}  {}",
                        &u.timestamp[..std::cmp::min(22, u.timestamp.len())],
                        u.event_type,
                        u.credits_charged,
                        u.project_id.as_deref().unwrap_or("-"),
                    );
                }
                println!();
            }
        }
        Err(e) => {
            eprintln!("Failed to fetch account: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_user_buy_credits(pack: &str) {
    let valid_packs = ["starter", "growth", "scale"];
    if !valid_packs.contains(&pack) {
        eprintln!("Invalid credit pack '{}'. Choose one of:", pack);
        eprintln!("  starter  — 500 credits   ($50)");
        eprintln!("  growth   — 2,000 credits ($180)");
        eprintln!("  scale    — 10,000 credits ($800)");
        std::process::exit(1);
    }

    let creds = auth::load_credentials();
    let Some(creds) = creds else {
        eprintln!("Not logged in. Run: skippr-dbt user login");
        std::process::exit(1);
    };
    let base_url = auth::auth_base_url();
    let client = api_client::ApiClient::new(&base_url);
    match client.buy_credits(&creds.access_token, pack).await {
        Ok(url) => {
            println!();
            println!("  Open this URL to complete your purchase:");
            println!("  {}", url);
            println!();
            println!("  Credits will be added to your account once payment completes.");
            println!();
        }
        Err(e) => {
            eprintln!("Failed to start checkout: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_user_usage() {
    let creds = auth::load_credentials();
    let Some(creds) = creds else {
        eprintln!("Not logged in. Run: skippr-dbt user login");
        std::process::exit(1);
    };
    let base_url = auth::auth_base_url();
    let client = api_client::ApiClient::new(&base_url);
    match client.get_account(&creds.access_token).await {
        Ok(account) => {
            println!();
            println!("  Usage log ({:.1} credits remaining)", account.balance.credits_remaining);
            println!();
            if account.recent_usage.is_empty() {
                println!("  No usage recorded yet.");
            } else {
                println!("  {:<22} {:<22} {:>8}  {:<10}  {}",
                    "Timestamp", "Event", "Credits", "Qty", "Project");
                println!("  {}", "-".repeat(78));
                let mut total = 0.0_f64;
                for u in &account.recent_usage {
                    total += u.credits_charged;
                    println!("  {:<22} {:<22} {:>8.1}  {:<10.0}  {}",
                        &u.timestamp[..std::cmp::min(22, u.timestamp.len())],
                        u.event_type,
                        u.credits_charged,
                        u.quantity,
                        u.project_id.as_deref().unwrap_or("-"),
                    );
                }
                println!("  {}", "-".repeat(78));
                println!("  {:>52.1}  total shown", total);
            }
            println!();

            print_low_balance_warning(account.balance.credits_remaining);
        }
        Err(e) => {
            eprintln!("Failed to fetch usage: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_user_create_api_key(name: &str) {
    let creds = auth::load_credentials();
    let Some(creds) = creds else {
        eprintln!("Not logged in. Run: skippr-dbt user login");
        std::process::exit(1);
    };
    let base_url = auth::auth_base_url();
    let client = api_client::ApiClient::new(&base_url);
    match client.create_api_key(&creds.access_token, name).await {
        Ok(key) => {
            println!();
            println!("  API key created: {}", key.name);
            println!();
            println!("    {}", key.raw_key);
            println!();
            println!("  Save this key — it will not be shown again.");
            println!("  Set it as SKIPPR_API_KEY in your CI environment.");
            println!();
        }
        Err(e) => {
            eprintln!("Failed to create API key: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_user_revoke_api_key(key_id: &str) {
    let creds = auth::load_credentials();
    let Some(creds) = creds else {
        eprintln!("Not logged in. Run: skippr-dbt user login");
        std::process::exit(1);
    };
    let base_url = auth::auth_base_url();
    let client = api_client::ApiClient::new(&base_url);
    match client.revoke_api_key(&creds.access_token, key_id).await {
        Ok(()) => {
            println!("API key {} revoked.", key_id);
        }
        Err(e) => {
            eprintln!("Failed to revoke API key: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_user_list_api_keys() {
    let creds = auth::load_credentials();
    let Some(creds) = creds else {
        eprintln!("Not logged in. Run: skippr-dbt user login");
        std::process::exit(1);
    };
    let base_url = auth::auth_base_url();
    let client = api_client::ApiClient::new(&base_url);
    match client.list_api_keys(&creds.access_token).await {
        Ok(keys) => {
            println!();
            if keys.is_empty() {
                println!("  No API keys found.");
                println!("  Create one with: skippr-dbt user create-api-key --name \"my-key\"");
            } else {
                println!("  {:<38} {:<20} {:<10} {}", "Key ID", "Name", "Status", "Created");
                println!("  {}", "-".repeat(80));
                for k in &keys {
                    println!("  {:<38} {:<20} {:<10} {}",
                        k.key_id,
                        k.name,
                        k.status,
                        &k.created_at[..std::cmp::min(22, k.created_at.len())],
                    );
                }
            }
            println!();
        }
        Err(e) => {
            eprintln!("Failed to list API keys: {}", e);
            std::process::exit(1);
        }
    }
}

const LOW_BALANCE_THRESHOLD: f64 = 50.0;

fn print_low_balance_warning(credits_remaining: f64) {
    if credits_remaining <= 0.0 {
        eprintln!("  WARNING: You have no credits remaining. Billable operations will fail.");
        eprintln!("  Run: skippr-dbt user buy-credits --pack starter");
        eprintln!();
    } else if credits_remaining < LOW_BALANCE_THRESHOLD {
        eprintln!("  WARNING: Low balance ({:.1} credits). Consider purchasing more credits.", credits_remaining);
        eprintln!("  Run: skippr-dbt user buy-credits --pack starter");
        eprintln!();
    }
}
