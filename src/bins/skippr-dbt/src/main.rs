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
# Required
LLM_API_KEY=sk-...

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

    if std::env::var("LLM_API_KEY")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_some()
    {
        check_pass("LLM_API_KEY is set");
    } else {
        check_fail("LLM_API_KEY is not set");
        ok = false;
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

    let internal_file = match translate::to_internal(&cfg) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    };

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
    }
}
