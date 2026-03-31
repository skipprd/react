mod auth;
mod api_client;
mod public_config;
mod skippr_bin;
mod translate;

use std::{path::PathBuf, process::Command};

use clap::{Parser, Subcommand};

use public_config::{DbtConfig, S3Transform, SchemaSinkConfig, SkipprDbtConfig, SourceConfig, WarehouseConfig};

#[derive(Parser, Debug)]
#[command(name = "skippr", about = "Data pipeline CLI — extract, load, and model with dbt")]
struct Cli {
    /// Log level (info, debug, trace). When omitted the live terminal UI is shown.
    #[arg(long, global = true, num_args = 0..=1, default_missing_value = "info")]
    log: Option<String>,

    /// Path to config file. Defaults to ./skippr.yaml in the working directory.
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
    /// Add funds to your account.
    BuyCredits {
        /// Dollar amount to add (e.g. 25 for $25). Minimum $5.
        #[arg(long)]
        amount: Option<f64>,
    },
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
    /// AWS Athena (S3 + Glue) warehouse.
    Athena {
        #[arg(long)]
        workgroup: Option<String>,
        #[arg(long)]
        region: Option<String>,
        #[arg(long)]
        result_s3: Option<String>,
        #[arg(long)]
        catalog: Option<String>,
        #[arg(long)]
        schema: Option<String>,
    },
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
    /// Databricks (Unity Catalog) warehouse.
    Databricks {
        #[arg(long)]
        workspace_url: Option<String>,
        #[arg(long)]
        token: Option<String>,
        #[arg(long)]
        warehouse_id: Option<String>,
        #[arg(long)]
        catalog: Option<String>,
        #[arg(long)]
        schema: Option<String>,
    },
    /// Azure Synapse Analytics warehouse.
    Synapse {
        #[arg(long)]
        connection_string: Option<String>,
        #[arg(long)]
        schema: Option<String>,
    },
    /// Amazon Redshift warehouse.
    Redshift {
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        cluster_identifier: Option<String>,
        #[arg(long)]
        workgroup_name: Option<String>,
        #[arg(long)]
        db_user: Option<String>,
        #[arg(long)]
        schema: Option<String>,
        #[arg(long)]
        region: Option<String>,
    },
    /// ClickHouse warehouse.
    Clickhouse {
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: Option<String>,
        #[arg(long)]
        password: Option<String>,
    },
    /// DuckDB / MotherDuck warehouse.
    Duckdb {
        #[arg(long)]
        connection_string: Option<String>,
        #[arg(long)]
        motherduck_token: Option<String>,
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
    /// MySQL source.
    Mysql {
        #[arg(long)]
        connection_string: Option<String>,
        #[arg(long, value_delimiter = ',')]
        tables: Option<Vec<String>>,
    },
    /// PostgreSQL source (distinct from postgres warehouse).
    PostgresSource {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        user: Option<String>,
        #[arg(long)]
        password: Option<String>,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        connection_string: Option<String>,
        #[arg(long, value_delimiter = ',')]
        tables: Option<Vec<String>>,
        #[arg(long)]
        query: Option<String>,
    },
    /// Amazon Redshift source.
    RedshiftSource {
        #[arg(long)]
        cluster_identifier: Option<String>,
        #[arg(long)]
        workgroup_name: Option<String>,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        db_user: Option<String>,
        #[arg(long, value_delimiter = ',')]
        tables: Option<Vec<String>>,
        #[arg(long)]
        region: Option<String>,
    },
    /// MongoDB source.
    Mongodb {
        #[arg(long)]
        connection_string: Option<String>,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        collection: Option<String>,
        #[arg(long)]
        filter: Option<String>,
    },
    /// DynamoDB source.
    Dynamodb {
        #[arg(long)]
        table_name: Option<String>,
        #[arg(long)]
        region: Option<String>,
        #[arg(long)]
        endpoint_url: Option<String>,
    },
    /// ClickHouse source.
    ClickhouseSource {
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        database: Option<String>,
        #[arg(long)]
        user: Option<String>,
        #[arg(long)]
        password: Option<String>,
        #[arg(long, value_delimiter = ',')]
        tables: Option<Vec<String>>,
        #[arg(long)]
        query: Option<String>,
    },
    /// DuckDB / MotherDuck source.
    DuckdbSource {
        #[arg(long)]
        connection_string: Option<String>,
        #[arg(long)]
        motherduck_token: Option<String>,
        #[arg(long)]
        database: Option<String>,
        #[arg(long, value_delimiter = ',')]
        tables: Option<Vec<String>>,
        #[arg(long)]
        query: Option<String>,
    },
    /// SFTP source.
    Sftp {
        #[arg(long)]
        host: Option<String>,
        #[arg(long)]
        port: Option<u16>,
        #[arg(long)]
        username: Option<String>,
        #[arg(long)]
        password: Option<String>,
        #[arg(long)]
        private_key_path: Option<String>,
        #[arg(long)]
        remote_path: Option<String>,
    },
    /// Local file source.
    File {
        #[arg(long)]
        path: Option<String>,
    },
    /// Delta Lake source.
    DeltaLake {
        #[arg(long)]
        table_uri: Option<String>,
        #[arg(long)]
        filter: Option<String>,
    },
    /// Kafka source.
    Kafka {
        #[arg(long)]
        brokers: Option<String>,
        #[arg(long)]
        topic: Option<String>,
        #[arg(long)]
        group_id: Option<String>,
        #[arg(long)]
        mode: Option<String>,
    },
    /// SQS source.
    Sqs {
        #[arg(long)]
        queue_url: Option<String>,
        #[arg(long)]
        region: Option<String>,
        #[arg(long)]
        mode: Option<String>,
    },
    /// Kinesis source.
    Kinesis {
        #[arg(long)]
        stream_name: Option<String>,
        #[arg(long)]
        region: Option<String>,
        #[arg(long)]
        mode: Option<String>,
    },
    /// AMQP (RabbitMQ) source.
    Amqp {
        #[arg(long)]
        connection_string: Option<String>,
        #[arg(long)]
        queue: Option<String>,
        #[arg(long)]
        mode: Option<String>,
    },
    /// SNS source (via SQS).
    Sns {
        #[arg(long)]
        topic_arn: Option<String>,
        #[arg(long)]
        sqs_queue_url: Option<String>,
        #[arg(long)]
        region: Option<String>,
    },
    /// EventBridge source (via SQS).
    Eventbridge {
        #[arg(long)]
        event_bus_name: Option<String>,
        #[arg(long)]
        sqs_queue_url: Option<String>,
        #[arg(long)]
        region: Option<String>,
    },
    /// MQTT source.
    Mqtt {
        #[arg(long)]
        broker_url: Option<String>,
        #[arg(long)]
        topic: Option<String>,
        #[arg(long)]
        mode: Option<String>,
    },
    /// WebSocket source.
    Websocket {
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        mode: Option<String>,
    },
    /// HTTP client source (polling).
    HttpClient {
        #[arg(long)]
        url: Option<String>,
        #[arg(long)]
        method: Option<String>,
        #[arg(long)]
        scrape_interval_seconds: Option<u64>,
    },
    /// HTTP server source (push receiver).
    HttpServer {
        #[arg(long)]
        listen_address: Option<String>,
        #[arg(long)]
        path: Option<String>,
    },
    /// Socket source (TCP/UDP/Unix).
    Socket {
        #[arg(long)]
        mode: Option<String>,
        #[arg(long)]
        address: Option<String>,
    },
    /// StatsD source.
    Statsd {
        #[arg(long)]
        listen_address: Option<String>,
    },
    /// Stdin source.
    Stdin {
        #[arg(long)]
        mode: Option<String>,
    },
}

fn working_dir() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

fn config_path(explicit: &Option<PathBuf>) -> PathBuf {
    explicit
        .clone()
        .unwrap_or_else(|| working_dir().join("skippr.yaml"))
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
        schema_sink: None,
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
# Interactive: skippr user login
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
    println!("  skippr connect warehouse snowflake");
    println!("  skippr connect source mssql");
    println!("  skippr doctor");
    println!("  skippr run");
}

// ---------------------------------------------------------------------------
// connect warehouse
// ---------------------------------------------------------------------------

fn cmd_connect_warehouse(kind: WarehouseKind, explicit_config: &Option<PathBuf>) {
    let mut cfg = match load_config(explicit_config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("error: {}", e);
            eprintln!("Run 'skippr init <project>' first.");
            std::process::exit(1);
        }
    };

    let wh = match kind {
        WarehouseKind::Athena { workgroup, region, result_s3, catalog, schema } => {
            let workgroup = workgroup.or_else(|| prompt("Athena workgroup (optional)"));
            let region = region.or_else(|| prompt("AWS region"));
            let result_s3 = result_s3.or_else(|| prompt("S3 result location (optional, e.g. s3://bucket/path)"));
            let catalog = catalog.or_else(|| prompt("Glue catalog (default: AwsDataCatalog)"));
            let schema = schema.or_else(|| prompt("Default database/schema (optional)"));
            WarehouseConfig::Athena { workgroup, region, result_s3, catalog, schema }
        }
        WarehouseKind::Snowflake {
            database,
            schema,
            warehouse,
            role,
        } => {
            let database = database.or_else(|| prompt("Snowflake database"));
            let schema = schema.or_else(|| prompt("Snowflake schema"));
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
        WarehouseKind::Databricks { workspace_url, token, warehouse_id, catalog, schema } => {
            WarehouseConfig::Databricks { workspace_url, token, warehouse_id, catalog, schema }
        }
        WarehouseKind::Synapse { connection_string, schema } => {
            WarehouseConfig::Synapse { connection_string, schema }
        }
        WarehouseKind::Redshift { database, cluster_identifier, workgroup_name, db_user, schema, region } => {
            WarehouseConfig::Redshift {
                database, cluster_identifier, workgroup_name, db_user, schema, region,
                staging_s3_bucket: None, staging_s3_prefix: None, iam_role_arn: None,
            }
        }
        WarehouseKind::Clickhouse { url, database, user, password } => {
            WarehouseConfig::Clickhouse { url, database, user, password }
        }
        WarehouseKind::Duckdb { connection_string, motherduck_token, database, schema } => {
            WarehouseConfig::Duckdb { connection_string, motherduck_token, database, schema }
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
            eprintln!("Run 'skippr init <project>' first.");
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
        SourceKind::Mysql { connection_string, tables } => {
            SourceConfig::Mysql { connection_string, tables }
        }
        SourceKind::PostgresSource { host, port, user, password, database, connection_string, tables, query } => {
            SourceConfig::PostgresSource { host, port, user, password, database, connection_string, tables, query }
        }
        SourceKind::RedshiftSource { cluster_identifier, workgroup_name, database, db_user, tables, region } => {
            SourceConfig::RedshiftSource { cluster_identifier, workgroup_name, database, db_user, tables, region }
        }
        SourceKind::Mongodb { connection_string, database, collection, filter } => {
            SourceConfig::Mongodb { connection_string, database, collection, filter }
        }
        SourceKind::Dynamodb { table_name, region, endpoint_url } => {
            SourceConfig::Dynamodb { table_name, region, endpoint_url }
        }
        SourceKind::ClickhouseSource { url, database, user, password, tables, query } => {
            SourceConfig::ClickhouseSource { url, database, user, password, tables, query }
        }
        SourceKind::DuckdbSource { connection_string, motherduck_token, database, tables, query } => {
            SourceConfig::DuckdbSource { connection_string, motherduck_token, database, tables, query }
        }
        SourceKind::Sftp { host, port, username, password, private_key_path, remote_path } => {
            SourceConfig::Sftp { host, port, username, password, private_key_path, remote_path }
        }
        SourceKind::File { path } => {
            SourceConfig::File { path }
        }
        SourceKind::DeltaLake { table_uri, filter } => {
            SourceConfig::DeltaLake { table_uri, storage_options: None, version: None, filter }
        }
        SourceKind::Kafka { brokers, topic, group_id, mode } => {
            SourceConfig::Kafka { brokers, topic, group_id, auto_offset_reset: None, security_protocol: None, sasl_mechanism: None, sasl_username: None, sasl_password: None, mode }
        }
        SourceKind::Sqs { queue_url, region, mode } => {
            SourceConfig::Sqs { queue_url, region, endpoint_url: None, mode }
        }
        SourceKind::Kinesis { stream_name, region, mode } => {
            SourceConfig::Kinesis { stream_name, region, endpoint_url: None, mode }
        }
        SourceKind::Amqp { connection_string, queue, mode } => {
            SourceConfig::Amqp { connection_string, queue, exchange: None, routing_key: None, prefetch_count: None, mode }
        }
        SourceKind::Sns { topic_arn, sqs_queue_url, region } => {
            SourceConfig::Sns { topic_arn, sqs_queue_url, region, endpoint_url: None }
        }
        SourceKind::Eventbridge { event_bus_name, sqs_queue_url, region } => {
            SourceConfig::Eventbridge { event_bus_name, sqs_queue_url, region, endpoint_url: None }
        }
        SourceKind::Mqtt { broker_url, topic, mode } => {
            SourceConfig::Mqtt { broker_url, port: None, topic, client_id: None, qos: None, username: None, password: None, mode }
        }
        SourceKind::Websocket { url, mode } => {
            SourceConfig::Websocket { url, headers: None, mode }
        }
        SourceKind::HttpClient { url, method, scrape_interval_seconds } => {
            SourceConfig::HttpClient { url, method, headers: None, body: None, auth_strategy: None, auth_user: None, auth_password: None, auth_token: None, scrape_interval_seconds }
        }
        SourceKind::HttpServer { listen_address, path } => {
            SourceConfig::HttpServer { listen_address, path, auth_token: None }
        }
        SourceKind::Socket { mode, address } => {
            SourceConfig::Socket { mode, address, framing: None }
        }
        SourceKind::Statsd { listen_address } => {
            SourceConfig::Statsd { listen_address }
        }
        SourceKind::Stdin { mode } => {
            SourceConfig::Stdin { mode }
        }
    };

    let kind_label = match &src {
        SourceConfig::Mssql { .. } => "mssql",
        SourceConfig::S3 { .. } => "s3",
        SourceConfig::Mysql { .. } => "mysql",
        SourceConfig::PostgresSource { .. } => "postgres_source",
        SourceConfig::RedshiftSource { .. } => "redshift_source",
        SourceConfig::Mongodb { .. } => "mongodb",
        SourceConfig::Dynamodb { .. } => "dynamodb",
        SourceConfig::ClickhouseSource { .. } => "clickhouse_source",
        SourceConfig::DuckdbSource { .. } => "duckdb_source",
        SourceConfig::Sftp { .. } => "sftp",
        SourceConfig::File { .. } => "file",
        SourceConfig::DeltaLake { .. } => "delta_lake",
        SourceConfig::Kafka { .. } => "kafka",
        SourceConfig::Sqs { .. } => "sqs",
        SourceConfig::Kinesis { .. } => "kinesis",
        SourceConfig::Amqp { .. } => "amqp",
        SourceConfig::Sns { .. } => "sns",
        SourceConfig::Eventbridge { .. } => "eventbridge",
        SourceConfig::Mqtt { .. } => "mqtt",
        SourceConfig::Websocket { .. } => "websocket",
        SourceConfig::HttpClient { .. } => "http_client",
        SourceConfig::HttpServer { .. } => "http_server",
        SourceConfig::Socket { .. } => "socket",
        SourceConfig::Statsd { .. } => "statsd",
        SourceConfig::Stdin { .. } => "stdin",
    };
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
            check_pass("skippr.yaml found");
            c
        }
        Err(_) => {
            check_fail("skippr.yaml not found — run 'skippr init <project>'");
            std::process::exit(1);
        }
    };

    if cfg.warehouse.is_some() {
        check_pass(&format!(
            "warehouse configured ({})",
            cfg.warehouse_kind_str().unwrap_or("unknown")
        ));
    } else {
        check_fail("warehouse not configured — run 'skippr connect warehouse <kind>'");
        ok = false;
    }

    if cfg.source.is_some() {
        check_pass(&format!(
            "source configured ({})",
            cfg.source_kind_str().unwrap_or("unknown")
        ));
    } else {
        check_fail("source not configured — run 'skippr connect source <kind>'");
        ok = false;
    }

    if which("skippr") {
        check_pass("skippr binary found on PATH (user-managed)");
    } else if skippr_bin::managed_binary_path()
        .map(|p| p.is_file())
        .unwrap_or(false)
    {
        check_pass(&format!(
            "skippr v{} installed (managed by skippr)",
            skippr_bin::SKIPPR_VERSION
        ));
    } else {
        check_pass(&format!(
            "skippr not found — v{} will be downloaded automatically on first run",
            skippr_bin::SKIPPR_VERSION
        ));
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
        check_fail("not authenticated — run 'skippr user login' or set SKIPPR_API_KEY");
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

    if let Some(WarehouseConfig::Athena { .. }) = &cfg.warehouse {
        check_athena_env(&mut ok);
    }

    if let Some(WarehouseConfig::Snowflake { .. }) = &cfg.warehouse {
        check_snowflake_env(&mut ok);
    }

    if let Some(WarehouseConfig::Postgres { .. }) = &cfg.warehouse {
        check_postgres_env(&mut ok);
    }

    if let Some(WarehouseConfig::Databricks { .. }) = &cfg.warehouse {
        check_databricks_env(&mut ok);
    }

    if let Some(WarehouseConfig::Synapse { .. }) = &cfg.warehouse {
        check_synapse_env(&mut ok);
    }

    if let Some(WarehouseConfig::Redshift { .. }) = &cfg.warehouse {
        check_redshift_env(&mut ok);
    }

    if let Some(WarehouseConfig::Clickhouse { .. }) = &cfg.warehouse {
        check_clickhouse_env(&mut ok);
    }

    println!();
    if ok {
        println!("All checks passed. Run 'skippr run' to start.");
    } else {
        println!("Some checks failed. Fix the issues above and re-run 'skippr doctor'.");
        std::process::exit(1);
    }
}

fn check_athena_env(_ok: &mut bool) {
    if env_set("AWS_REGION") || env_set("AWS_DEFAULT_REGION") {
        check_pass("AWS region configured (AWS_REGION or AWS_DEFAULT_REGION)");
    } else {
        check_pass("AWS region not set (will use SDK default or --region flag)");
    }
    if env_set("ATHENA_WORKGROUP") {
        check_pass("ATHENA_WORKGROUP is set");
    } else {
        check_pass("ATHENA_WORKGROUP not set (will use Athena default workgroup)");
    }
    if env_set("ATHENA_RESULT_S3") {
        check_pass("ATHENA_RESULT_S3 is set");
    } else {
        check_pass("ATHENA_RESULT_S3 not set (will rely on workgroup output location)");
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

fn check_databricks_env(ok: &mut bool) {
    if env_set("DATABRICKS_HOST") || env_set("DATABRICKS_WORKSPACE_URL") {
        check_pass("DATABRICKS_HOST or DATABRICKS_WORKSPACE_URL is set");
    } else {
        check_fail("DATABRICKS_HOST / DATABRICKS_WORKSPACE_URL is not set");
        *ok = false;
    }
    if env_set("DATABRICKS_TOKEN") {
        check_pass("DATABRICKS_TOKEN is set");
    } else {
        check_fail("DATABRICKS_TOKEN is not set");
        *ok = false;
    }
}

fn check_synapse_env(ok: &mut bool) {
    if env_set("SYNAPSE_CONNECTION_STRING") {
        check_pass("SYNAPSE_CONNECTION_STRING is set");
    } else {
        check_fail("SYNAPSE_CONNECTION_STRING is not set");
        *ok = false;
    }
}

fn check_redshift_env(ok: &mut bool) {
    if env_set("REDSHIFT_HOST") || env_set("REDSHIFT_CLUSTER_IDENTIFIER") || env_set("REDSHIFT_WORKGROUP_NAME") {
        check_pass("Redshift connection configured (host, cluster, or workgroup)");
    } else {
        check_fail("Redshift connection not configured — set REDSHIFT_HOST, REDSHIFT_CLUSTER_IDENTIFIER, or REDSHIFT_WORKGROUP_NAME");
        *ok = false;
    }
}

fn check_clickhouse_env(_ok: &mut bool) {
    if env_set("CLICKHOUSE_URL") {
        check_pass("CLICKHOUSE_URL is set");
    } else {
        check_pass("CLICKHOUSE_URL not set (defaults to http://localhost:8123)");
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
            eprintln!("Run 'skippr init <project>' first.");
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

    let skippr_binary = match skippr_bin::resolve_skippr_binary().await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("[skippr] ERROR: {}", e);
            std::process::exit(1);
        }
    };
    translate::set_skippr_binary(&mut internal_file, &skippr_binary);

    // Authentication is mandatory. SKIPPR_API_KEY env var takes priority, then credentials.json.
    let creds = if let Ok(api_key) = std::env::var("SKIPPR_API_KEY") {
        if api_key.trim().is_empty() {
            eprintln!("[skippr] ERROR: SKIPPR_API_KEY is set but empty.");
            std::process::exit(1);
        }
        let base_url = auth::auth_base_url();
        let client = api_client::ApiClient::new(&base_url);
        match client.exchange_api_key(api_key.trim()).await {
            Ok(tokens) => {
                eprintln!("[skippr] authenticated via API key");
                tokens
            }
            Err(e) => {
                eprintln!("[skippr] ERROR: API key authentication failed: {}", e);
                std::process::exit(1);
            }
        }
    } else if let Some(creds) = auth::load_credentials() {
        eprintln!("[skippr] authenticated via stored credentials");
        refresh_user_credentials_or_exit(&api_client::ApiClient::new(&auth::auth_base_url()), creds).await
    } else {
        eprintln!("[skippr] ERROR: Authentication required.");
        eprintln!("[skippr]   Run 'skippr user login' to authenticate interactively,");
        eprintln!("[skippr]   or set SKIPPR_API_KEY for CI/CD.");
        std::process::exit(1);
    };

    let base_url = auth::auth_base_url();
    let tokens = create_token_provider(&creds);
    let client = api_client::ApiClient::authenticated(&base_url, std::sync::Arc::clone(&tokens));

    let initial_balance = match client.get_account().await {
        Ok(account) => {
            let bal = account.balance.balance;
            if bal <= 0.0 {
                eprintln!("[skippr] ERROR: Balance is $0.00. Add funds to continue.");
                eprintln!("[skippr]   skippr user buy-credits --amount 25");
                std::process::exit(1);
            } else if bal < LOW_BALANCE_USD_THRESHOLD {
                eprintln!("[skippr] WARNING: Low balance (${:.2}). The run may exhaust your balance.", bal);
            } else {
                eprintln!("[skippr] balance: ${:.2}", bal);
            }
            bal
        }
        Err(e) => {
            eprintln!("[skippr] ERROR: Could not verify account balance ({}). Refusing to run.", e);
            eprintln!("[skippr]   Check your connection and login status (skippr user login).");
            std::process::exit(1);
        }
    };

    match client.get_credentials().await {
        Ok(srv_creds) => {
            translate::apply_authenticated_overlay(
                &mut internal_file,
                &srv_creds,
                std::sync::Arc::clone(&tokens),
                initial_balance,
            );
            eprintln!("[skippr] cloud storage + metering active");
        }
        Err(e) => {
            eprintln!("[skippr] ERROR: Failed to fetch server credentials: {}", e);
            eprintln!("[skippr]   Check your connection and login status.");
            std::process::exit(1);
        }
    }

    let run_id = uuid::Uuid::new_v4().to_string();
    react_suite_data_engineer::metering::set_metering_run_id(&run_id);
    eprintln!("[skippr] run {run_id}");

    let metering = react_suite_data_engineer::metering::global_metering();
    let _ = metering
        .record_batch(&[react_suite_data_engineer::metering::UsageEvent::PipelineRun {
            project_id: cfg.project.clone(),
        }])
        .await;

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
        react_suite_data_engineer::metering::set_metering_thread_id(tid);
        eprintln!("[skippr] resuming thread {tid}");
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
/// Scans `.skippr/local/dev/<project>/threads/` for `<uuid>.json` files
/// (skipping companion files like `<uuid>__gather_0.json`) and returns the
/// thread ID with the newest modification time, if any.
fn find_latest_thread(project: &str) -> Option<String> {
    let threads_dir = PathBuf::from(format!(
        ".skippr/local/dev/{}/threads",
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
            UserAction::BuyCredits { amount } => cmd_user_buy_credits(amount).await,
            UserAction::CreateApiKey { name } => cmd_user_create_api_key(&name).await,
            UserAction::RevokeApiKey { key_id } => cmd_user_revoke_api_key(&key_id).await,
            UserAction::ListApiKeys => cmd_user_list_api_keys().await,
        },
    }
}

async fn cmd_user_login() {
    if auth::load_credentials().is_some() {
        eprintln!("Already logged in. Run 'skippr user logout' first to switch accounts.");
        std::process::exit(1);
    }

    println!("Enter your email address:");
    let mut email = String::new();
    std::io::stdin().read_line(&mut email).unwrap();
    let email = email.trim();

    if email.is_empty() || !email.contains('@') {
        eprintln!("Invalid email address.");
        std::process::exit(1);
    }

    let base_url = auth::auth_base_url();
    let client = api_client::ApiClient::new(&base_url);

    match client.sign_in(email).await {
        Ok(_) => {
            println!("Verification code sent to {}.", email);
            println!("Enter the 6-digit code:");
            let mut code = String::new();
            std::io::stdin().read_line(&mut code).unwrap();
            let code = code.trim();

            match client.confirm(email, code).await {
                Ok(tokens) => {
                    auth::save_credentials(&tokens);
                    println!();
                    println!("  Logged in successfully.");
                    println!();
                    println!("  Next steps:");
                    println!("    skippr user account       — view balance");
                    println!("    skippr user buy-credits   — add funds");
                    println!("    skippr run                — start a pipeline");
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

fn load_stored_credentials_or_exit() -> auth::StoredCredentials {
    match auth::load_credentials() {
        Some(creds) => creds,
        None => {
            eprintln!("Not logged in. Run: skippr user login");
            std::process::exit(1);
        }
    }
}

async fn refresh_user_credentials_or_exit(
    client: &api_client::ApiClient,
    creds: auth::StoredCredentials,
) -> auth::StoredCredentials {
    if creds.refresh_token.trim().is_empty() {
        return creds;
    }

    match client.refresh(&creds.refresh_token).await {
        Ok(refreshed) => {
            auth::save_credentials(&refreshed);
            refreshed
        }
        Err(e) => {
            eprintln!("Session refresh failed: {}", e);
            eprintln!("Run: skippr user login");
            std::process::exit(1);
        }
    }
}

async fn load_authenticated_user_credentials(client: &api_client::ApiClient) -> auth::StoredCredentials {
    let creds = load_stored_credentials_or_exit();
    refresh_user_credentials_or_exit(client, creds).await
}

fn create_token_provider(creds: &auth::StoredCredentials) -> std::sync::Arc<react_suite_data_engineer::metering::TokenProvider> {
    let base_url = auth::auth_base_url();
    let rt = if creds.refresh_token.is_empty() { None } else { Some(creds.refresh_token.clone()) };
    std::sync::Arc::new(react_suite_data_engineer::metering::TokenProvider::new(
        Some(creds.access_token.clone()),
        rt,
        Some(base_url),
    ))
}

async fn authenticated_api_client() -> api_client::ApiClient {
    let base_url = auth::auth_base_url();
    let unauthenticated = api_client::ApiClient::new(&base_url);
    let creds = load_authenticated_user_credentials(&unauthenticated).await;
    let tokens = create_token_provider(&creds);
    api_client::ApiClient::authenticated(&base_url, tokens)
}

async fn cmd_user_account() {
    let client = authenticated_api_client().await;
    match client.get_account().await {
        Ok(account) => {
            println!();
            println!("  Account");
            println!("  {}", "-".repeat(50));
            let plan_label = match account.profile.plan.as_str() {
                "free" => "Pay as you go",
                "pro" => "Pro",
                other => other,
            };
            println!("  Plan:              {}", plan_label);
            println!("  Balance:           ${:.2}", account.balance.balance);
            if let Some(ref sub) = account.subscription {
                println!("  Subscription:      {} ({})", sub.status, sub.price_id);
            }
            println!();

            print_low_balance_warning(&account.balance);

            {
                let today = chrono::Utc::now().date_naive();

                println!("  Last 7 days:");
                println!("  {:<12} {:>8}", "Date", "Cost");
                println!("  {}", "-".repeat(22));
                for dc in &account.daily_costs_est {
                    println!("  {:<12} {:>8}", dc.date, format!("${:.2}", dc.cost));
                }
                println!("  {}", "-".repeat(22));
                println!("  {:<12} {:>8}", format!("~{}", today.format("%B")), format!("${:.2}", account.monthly_cost_est));
                println!();
            }
        }
        Err(e) => {
            eprintln!("Failed to fetch account: {}", e);
            std::process::exit(1);
        }
    }
}

async fn cmd_user_buy_credits(amount: Option<f64>) {
    let amount = match amount {
        Some(a) => a,
        None => {
            println!("Add funds to your Skippr account.");
            println!();
            println!("Usage:");
            println!("  skippr user buy-credits --amount <DOLLARS>");
            println!();
            println!("Examples:");
            println!("  skippr user buy-credits --amount 25     # add $25");
            println!("  skippr user buy-credits --amount 100    # add $100");
            println!("  skippr user buy-credits --amount 500    # add $500");
            println!();
            println!("Minimum $5, maximum $10,000 per transaction.");
            println!("Your balance is visible via: skippr user account");
            return;
        }
    };

    if amount < 5.0 {
        eprintln!("Minimum top-up is $5.");
        std::process::exit(1);
    }
    if amount > 10_000.0 {
        eprintln!("Maximum top-up is $10,000 per transaction.");
        std::process::exit(1);
    }
    let client = authenticated_api_client().await;
    match client.add_funds(amount).await {
        Ok(url) => {
            let browser_result = open_in_default_browser(&url);
            println!();
            println!("  Adding ${:.2} to your account.", amount);
            println!();
            match browser_result {
                Ok(()) => println!("  Opened your default browser to complete your purchase."),
                Err(err) => println!("  Could not open your default browser automatically: {}", err),
            }
            println!();
            println!("  Open this URL to complete your purchase:");
            println!("  {}", url);
            println!();
            println!("  Funds will appear in your balance as soon as payment completes.");
            println!();
        }
        Err(e) => {
            eprintln!("Failed to start checkout: {}", e);
            std::process::exit(1);
        }
    }
}

fn open_in_default_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("open failed: {}", e))
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("explorer failed: {}", e))
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("xdg-open failed: {}", e))
    }
}

async fn cmd_user_create_api_key(name: &str) {
    let client = authenticated_api_client().await;
    match client.create_api_key(name).await {
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
    let client = authenticated_api_client().await;
    match client.revoke_api_key(key_id).await {
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
    let client = authenticated_api_client().await;
    match client.list_api_keys().await {
        Ok(keys) => {
            println!();
            if keys.is_empty() {
                println!("  No API keys found.");
                println!("  Create one with: skippr user create-api-key --name \"my-key\"");
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

const LOW_BALANCE_USD_THRESHOLD: f64 = 5.0;

fn print_low_balance_warning(balance: &api_client::Balance) {
    if balance.balance <= 0.0 {
        eprintln!("  WARNING: Your balance is $0.00. Billable operations will fail.");
        eprintln!("  Run: skippr user buy-credits --amount 25");
        eprintln!();
    } else if balance.balance < LOW_BALANCE_USD_THRESHOLD {
        eprintln!("  WARNING: Low balance (${:.2}). Consider adding funds.", balance.balance);
        eprintln!("  Run: skippr user buy-credits --amount 25");
        eprintln!();
    }
}
