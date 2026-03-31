use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;

use react_core::scope::RequestScope;
use react_suite_data_engineer::de_config::{ElToolResolved, WarehouseKind, WarehouseResolved};
use react_suite_data_engineer::providers::{
    SkipprDiscoverResult, SkipprFieldSchema, SkipprNamespaceSchema, SkipprNamespaceStatus,
    SkipprPipelineConfig, SkipprPipelineStatus, SkipprProvider, SkipprSyncResult,
};

#[derive(Clone)]
pub struct SkipprCliProvider {
    pub binary: String,
    pub data_dir: PathBuf,
    pub el_config: ElToolResolved,
    pub warehouse: WarehouseResolved,
}

impl SkipprCliProvider {
    pub fn new(
        el_config: ElToolResolved,
        warehouse: WarehouseResolved,
        data_dir: PathBuf,
    ) -> Self {
        let binary = if el_config.skippr_binary.is_empty() {
            "skippr-el".to_string()
        } else {
            el_config.skippr_binary.clone()
        };
        Self {
            binary,
            data_dir,
            el_config,
            warehouse,
        }
    }

    fn skippr_yml_path(&self) -> PathBuf {
        self.data_dir.join("skippr-el.yaml")
    }

    fn env_vars(&self) -> HashMap<String, String> {
        let mut env = HashMap::new();
        env.insert("SKIPPR_STORAGE_MODE".into(), "local".into());
        env.insert(
            "DATA_DIR".into(),
            self.data_dir.to_string_lossy().to_string(),
        );
        env.insert(
            "SKIPPR_CONFIG_FILE".into(),
            self.skippr_yml_path().to_string_lossy().to_string(),
        );

        let input = &self.el_config.skippr_input;
        if let Some(kind) = input.get("kind").and_then(|v| v.as_str()) {
            let plugin_name = capitalize_first(kind);
            env.insert("DATA_SOURCE_PLUGIN_NAME".into(), plugin_name);

            if kind.eq_ignore_ascii_case("mssql") {
                if let Some(cs) = input.get("connection_string").and_then(|v| v.as_str()) {
                    let resolved = resolve_env_ref(cs);
                    env.insert("MSSQL_CONNECTION_STRING".into(), resolved);
                }
            }

            if kind.eq_ignore_ascii_case("s3") {
                if let Some(b) = input.get("s3_bucket").and_then(|v| v.as_str()) {
                    env.insert("DATA_SOURCE_S3_BUCKET".into(), resolve_env_ref(b));
                }
                if let Some(p) = input.get("s3_prefix").and_then(|v| v.as_str()) {
                    env.insert("DATA_SOURCE_S3_PREFIX".into(), resolve_env_ref(p));
                }
            }

            if kind.eq_ignore_ascii_case("mysql") {
                if let Some(cs) = input.get("connection_string").and_then(|v| v.as_str()) {
                    env.insert("MYSQL_CONNECTION_STRING".into(), resolve_env_ref(cs));
                }
            }

            if kind.eq_ignore_ascii_case("postgres") {
                if let Some(v) = input.get("host").and_then(|v| v.as_str()) { env.insert("POSTGRES_HOST".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("user").and_then(|v| v.as_str()) { env.insert("POSTGRES_USER".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("password").and_then(|v| v.as_str()) { env.insert("POSTGRES_PASSWORD".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("database").and_then(|v| v.as_str()) { env.insert("POSTGRES_DATABASE".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("connection_string").and_then(|v| v.as_str()) { env.insert("POSTGRES_CONNECTION_STRING".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("redshift") {
                if let Some(v) = input.get("cluster_identifier").and_then(|v| v.as_str()) { env.insert("REDSHIFT_CLUSTER_IDENTIFIER".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("database").and_then(|v| v.as_str()) { env.insert("REDSHIFT_DATABASE".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("region").and_then(|v| v.as_str()) { env.insert("AWS_DEFAULT_REGION".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("mongodb") {
                if let Some(v) = input.get("connection_string").and_then(|v| v.as_str()) { env.insert("MONGODB_CONNECTION_STRING".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("dynamodb") {
                if let Some(v) = input.get("table_name").and_then(|v| v.as_str()) { env.insert("DYNAMODB_TABLE_NAME".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("region").and_then(|v| v.as_str()) { env.insert("AWS_DEFAULT_REGION".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("clickhouse") {
                if let Some(v) = input.get("url").and_then(|v| v.as_str()) { env.insert("CLICKHOUSE_URL".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("database").and_then(|v| v.as_str()) { env.insert("CLICKHOUSE_DATABASE".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("user").and_then(|v| v.as_str()) { env.insert("CLICKHOUSE_USER".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("duckdb") {
                if let Some(v) = input.get("connection_string").and_then(|v| v.as_str()) { env.insert("DUCKDB_CONNECTION_STRING".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("motherduck_token").and_then(|v| v.as_str()) { env.insert("MOTHERDUCK_TOKEN".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("sftp") {
                if let Some(v) = input.get("host").and_then(|v| v.as_str()) { env.insert("SFTP_HOST".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("username").and_then(|v| v.as_str()) { env.insert("SFTP_USERNAME".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("password").and_then(|v| v.as_str()) { env.insert("SFTP_PASSWORD".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("remote_path").and_then(|v| v.as_str()) { env.insert("SFTP_REMOTE_PATH".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("kafka") {
                if let Some(v) = input.get("brokers").and_then(|v| v.as_str()) { env.insert("KAFKA_BROKERS".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("topic").and_then(|v| v.as_str()) { env.insert("KAFKA_TOPIC".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("sqs") {
                if let Some(v) = input.get("queue_url").and_then(|v| v.as_str()) { env.insert("SQS_QUEUE_URL".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("region").and_then(|v| v.as_str()) { env.insert("AWS_DEFAULT_REGION".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("kinesis") {
                if let Some(v) = input.get("stream_name").and_then(|v| v.as_str()) { env.insert("KINESIS_STREAM_NAME".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("region").and_then(|v| v.as_str()) { env.insert("AWS_DEFAULT_REGION".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("amqp") {
                if let Some(v) = input.get("connection_string").and_then(|v| v.as_str()) { env.insert("AMQP_CONNECTION_STRING".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("queue").and_then(|v| v.as_str()) { env.insert("AMQP_QUEUE".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("sns") {
                if let Some(v) = input.get("topic_arn").and_then(|v| v.as_str()) { env.insert("SNS_TOPIC_ARN".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("sqs_queue_url").and_then(|v| v.as_str()) { env.insert("SNS_SQS_QUEUE_URL".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("region").and_then(|v| v.as_str()) { env.insert("AWS_DEFAULT_REGION".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("eventbridge") {
                if let Some(v) = input.get("event_bus_name").and_then(|v| v.as_str()) { env.insert("EVENTBRIDGE_EVENT_BUS_NAME".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("sqs_queue_url").and_then(|v| v.as_str()) { env.insert("EVENTBRIDGE_SQS_QUEUE_URL".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("region").and_then(|v| v.as_str()) { env.insert("AWS_DEFAULT_REGION".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("mqtt") {
                if let Some(v) = input.get("broker_url").and_then(|v| v.as_str()) { env.insert("MQTT_BROKER_URL".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("topic").and_then(|v| v.as_str()) { env.insert("MQTT_TOPIC".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("websocket") {
                if let Some(v) = input.get("url").and_then(|v| v.as_str()) { env.insert("WEBSOCKET_URL".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("socket") {
                if let Some(v) = input.get("mode").and_then(|v| v.as_str()) { env.insert("SOCKET_MODE".into(), resolve_env_ref(v)); }
                if let Some(v) = input.get("address").and_then(|v| v.as_str()) { env.insert("SOCKET_ADDRESS".into(), resolve_env_ref(v)); }
            }

            if kind.eq_ignore_ascii_case("delta_lake") {
                if let Some(v) = input.get("table_uri").and_then(|v| v.as_str()) { env.insert("DELTA_TABLE_URI".into(), resolve_env_ref(v)); }
            }

        }

        match self.warehouse.kind {
            WarehouseKind::Snowflake => {
                env.insert("DATA_OUTPUT_PLUGIN_NAME".into(), "Snowflake".into());
                insert_snowflake_env(&self.warehouse, &mut env);
            }
            WarehouseKind::Athena => {
                env.insert("DATA_OUTPUT_PLUGIN_NAME".into(), "Athena".into());
            }
            WarehouseKind::Bigquery => {
                env.insert("DATA_OUTPUT_PLUGIN_NAME".into(), "Bigquery".into());
                if let Some(v) = std::env::var("BIGQUERY_PROJECT").ok().filter(|v| !v.trim().is_empty()) {
                    env.insert("BIGQUERY_PROJECT".into(), v);
                } else if !self.warehouse.container.is_empty() {
                    env.insert("BIGQUERY_PROJECT".into(), resolve_env_ref(&self.warehouse.container));
                }
                if let Some(v) = std::env::var("BIGQUERY_DATASET").ok().filter(|v| !v.trim().is_empty()) {
                    env.insert("BIGQUERY_DATASET".into(), v);
                } else if !self.warehouse.namespace.is_empty() {
                    env.insert("BIGQUERY_DATASET".into(), resolve_env_ref(&self.warehouse.namespace));
                }
                if let Some(loc) = self.warehouse.extras.get("location").and_then(|v| v.as_str()) {
                    env.insert("BIGQUERY_LOCATION".into(), loc.to_string());
                }
            }
            WarehouseKind::Postgres => {
                env.insert("DATA_OUTPUT_PLUGIN_NAME".into(), "Postgres".into());
                if let Some(v) = std::env::var("POSTGRES_DATABASE").ok().filter(|v| !v.trim().is_empty()) {
                    env.insert("POSTGRES_DATABASE".into(), v);
                } else if !self.warehouse.container.is_empty() {
                    env.insert("POSTGRES_DATABASE".into(), resolve_env_ref(&self.warehouse.container));
                }
                if let Some(v) = std::env::var("POSTGRES_SCHEMA").ok().filter(|v| !v.trim().is_empty()) {
                    env.insert("POSTGRES_SCHEMA".into(), v);
                } else if !self.warehouse.namespace.is_empty() {
                    env.insert("POSTGRES_SCHEMA".into(), resolve_env_ref(&self.warehouse.namespace));
                }
            }
            WarehouseKind::Databricks => {
                env.insert("DATA_OUTPUT_PLUGIN_NAME".into(), "Databricks".into());
                if let Some(v) = std::env::var("DATABRICKS_WORKSPACE_URL").ok().filter(|v| !v.trim().is_empty()) { env.insert("DATABRICKS_WORKSPACE_URL".into(), v); }
                else if let Some(v) = self.warehouse.extras.get("workspace_url").and_then(|v| v.as_str()) { env.insert("DATABRICKS_WORKSPACE_URL".into(), v.to_string()); }
                if let Some(v) = std::env::var("DATABRICKS_TOKEN").ok().filter(|v| !v.trim().is_empty()) { env.insert("DATABRICKS_TOKEN".into(), v); }
                else if let Some(v) = self.warehouse.extras.get("token").and_then(|v| v.as_str()) { env.insert("DATABRICKS_TOKEN".into(), v.to_string()); }
                if let Some(v) = std::env::var("DATABRICKS_WAREHOUSE_ID").ok().filter(|v| !v.trim().is_empty()) { env.insert("DATABRICKS_WAREHOUSE_ID".into(), v); }
                else if let Some(v) = self.warehouse.extras.get("warehouse_id").and_then(|v| v.as_str()) { env.insert("DATABRICKS_WAREHOUSE_ID".into(), v.to_string()); }
            }
            WarehouseKind::Synapse => {
                env.insert("DATA_OUTPUT_PLUGIN_NAME".into(), "Synapse".into());
                if let Some(v) = std::env::var("SYNAPSE_CONNECTION_STRING").ok().filter(|v| !v.trim().is_empty()) { env.insert("SYNAPSE_CONNECTION_STRING".into(), v); }
                else if let Some(v) = self.warehouse.extras.get("connection_string").and_then(|v| v.as_str()) { env.insert("SYNAPSE_CONNECTION_STRING".into(), resolve_env_ref(v)); }
            }
            WarehouseKind::Redshift => {
                env.insert("DATA_OUTPUT_PLUGIN_NAME".into(), "Redshift".into());
                for (env_key, json_key) in [
                    ("REDSHIFT_DATABASE", "database"), ("REDSHIFT_CLUSTER_IDENTIFIER", "cluster_identifier"),
                    ("REDSHIFT_WORKGROUP_NAME", "workgroup_name"), ("REDSHIFT_DB_USER", "db_user"),
                    ("REDSHIFT_REGION", "region"), ("REDSHIFT_STAGING_S3_BUCKET", "staging_s3_bucket"),
                    ("REDSHIFT_STAGING_S3_PREFIX", "staging_s3_prefix"), ("REDSHIFT_IAM_ROLE_ARN", "iam_role_arn"),
                ] {
                    if let Some(v) = std::env::var(env_key).ok().filter(|v| !v.trim().is_empty()) { env.insert(env_key.into(), v); }
                    else if let Some(v) = self.warehouse.extras.get(json_key).and_then(|v| v.as_str()).filter(|v| !v.is_empty()) { env.insert(env_key.into(), resolve_env_ref(v)); }
                }
                if !self.warehouse.container.is_empty() && !env.contains_key("REDSHIFT_DATABASE") {
                    env.insert("REDSHIFT_DATABASE".into(), resolve_env_ref(&self.warehouse.container));
                }
            }
            WarehouseKind::Clickhouse => {
                env.insert("DATA_OUTPUT_PLUGIN_NAME".into(), "Clickhouse".into());
                if let Some(v) = std::env::var("CLICKHOUSE_URL").ok().filter(|v| !v.trim().is_empty()) { env.insert("CLICKHOUSE_URL".into(), v); }
                else if let Some(v) = self.warehouse.extras.get("url").and_then(|v| v.as_str()) { env.insert("CLICKHOUSE_URL".into(), resolve_env_ref(v)); }
                if let Some(v) = std::env::var("CLICKHOUSE_DATABASE").ok().filter(|v| !v.trim().is_empty()) { env.insert("CLICKHOUSE_DATABASE".into(), v); }
                else if !self.warehouse.container.is_empty() { env.insert("CLICKHOUSE_DATABASE".into(), resolve_env_ref(&self.warehouse.container)); }
                if let Some(v) = std::env::var("CLICKHOUSE_USER").ok().filter(|v| !v.trim().is_empty()) { env.insert("CLICKHOUSE_USER".into(), v); }
                else if let Some(v) = self.warehouse.extras.get("user").and_then(|v| v.as_str()) { env.insert("CLICKHOUSE_USER".into(), v.to_string()); }
                if let Some(v) = std::env::var("CLICKHOUSE_PASSWORD").ok().filter(|v| !v.trim().is_empty()) { env.insert("CLICKHOUSE_PASSWORD".into(), v); }
                else if let Some(v) = self.warehouse.extras.get("password").and_then(|v| v.as_str()) { env.insert("CLICKHOUSE_PASSWORD".into(), v.to_string()); }
            }
            WarehouseKind::Duckdb => {
                env.insert("DATA_OUTPUT_PLUGIN_NAME".into(), "Duckdb".into());
                if let Some(v) = std::env::var("DUCKDB_CONNECTION_STRING").ok().filter(|v| !v.trim().is_empty()) { env.insert("DUCKDB_CONNECTION_STRING".into(), v); }
                else if let Some(v) = self.warehouse.extras.get("connection_string").and_then(|v| v.as_str()) { env.insert("DUCKDB_CONNECTION_STRING".into(), resolve_env_ref(v)); }
                if let Some(v) = std::env::var("MOTHERDUCK_TOKEN").ok().filter(|v| !v.trim().is_empty()) { env.insert("MOTHERDUCK_TOKEN".into(), v); }
                else if let Some(v) = self.warehouse.extras.get("motherduck_token").and_then(|v| v.as_str()) { env.insert("MOTHERDUCK_TOKEN".into(), resolve_env_ref(v)); }
            }
            WarehouseKind::Mssql => {}
        }

        if let Some(ref ss) = self.el_config.schema_sink {
            if let Some(kind) = ss.get("kind").and_then(|v| v.as_str()) {
                env.insert("DATA_SCHEMA_PLUGIN_NAME".into(), capitalize_first(kind));
                if kind.eq_ignore_ascii_case("glue") {
                    if let Some(v) = ss.get("glue_database_name").and_then(|v| v.as_str()) {
                        env.insert("GLUE_DATABASE_NAME".into(), resolve_env_ref(v));
                    }
                }
            }
        }

        env
    }

    async fn run_skippr(&self, args: &[&str]) -> Result<std::process::Output, String> {
        self.run_skippr_with_timeout(args, std::time::Duration::from_secs(120)).await
    }

    async fn run_skippr_with_timeout(
        &self,
        args: &[&str],
        timeout: std::time::Duration,
    ) -> Result<std::process::Output, String> {
        let env = self.env_vars();
        let mut cmd = tokio::process::Command::new(&self.binary);
        cmd.arg("--log");
        cmd.arg("info");
        cmd.args(args);
        cmd.current_dir(&self.data_dir);
        cmd.env("RUST_BACKTRACE", "1");
        for (k, v) in &env {
            cmd.env(k, v);
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        tracing::info!(
            binary = %self.binary,
            args = ?args,
            data_dir = %self.data_dir.display(),
            timeout_secs = timeout.as_secs(),
            "spawning skippr"
        );

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn skippr: {}", e))?;

        let child_id = child.id();

        let stdout_handle = child.stdout.take();
        let stdout_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut stdout) = stdout_handle {
                use tokio::io::AsyncReadExt;
                let _ = stdout.read_to_end(&mut buf).await;
            }
            buf
        });

        let stderr_handle = child.stderr.take();
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut stderr) = stderr_handle {
                use tokio::io::AsyncReadExt;
                let _ = stderr.read_to_end(&mut buf).await;
            }
            buf
        });

        let result = tokio::time::timeout(timeout, child.wait()).await;

        match result {
            Ok(Ok(status)) => {
                let stdout_bytes = stdout_task.await.unwrap_or_default();
                let stderr_bytes = stderr_task.await.unwrap_or_default();
                let output = std::process::Output {
                    status: std::process::ExitStatus::from(status),
                    stdout: stdout_bytes,
                    stderr: stderr_bytes,
                };
                if !output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!(
                        exit_code = ?output.status.code(),
                        stdout = %stdout,
                        stderr = %stderr,
                        "skippr command failed"
                    );
                }
                Ok(output)
            }
            Ok(Err(e)) => Err(format!("skippr process error: {}", e)),
            Err(_) => {
                if let Some(pid) = child_id {
                    let _ = tokio::process::Command::new("kill")
                        .args(["-9", &pid.to_string()])
                        .status()
                        .await;
                }
                Err(format!(
                    "skippr command timed out after {}s (args: {:?})",
                    timeout.as_secs(),
                    args
                ))
            }
        }
    }

    /// Runs skippr with streaming stdout. Instead of a fixed wall-clock timeout,
    /// this uses an *activity* timeout: the process can run for hours as long as
    /// it keeps emitting JSON lines (heartbeats or real events) within the
    /// `idle_timeout` window.  Returns all collected JSON events plus any errors.
    async fn run_skippr_streaming(
        &self,
        args: &[&str],
        idle_timeout: std::time::Duration,
    ) -> Result<(Vec<serde_json::Value>, Vec<u8>, bool), String> {
        use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

        let env = self.env_vars();
        let mut cmd = tokio::process::Command::new(&self.binary);
        cmd.arg("--log");
        cmd.arg("info");
        cmd.args(args);
        cmd.current_dir(&self.data_dir);
        cmd.env("RUST_BACKTRACE", "1");
        for (k, v) in &env {
            cmd.env(k, v);
        }
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        tracing::info!(
            binary = %self.binary,
            args = ?args,
            data_dir = %self.data_dir.display(),
            idle_timeout_secs = idle_timeout.as_secs(),
            "spawning skippr (streaming)"
        );

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn skippr: {}", e))?;

        let child_id = child.id();

        let stderr_handle = child.stderr.take();
        let stderr_task = tokio::spawn(async move {
            let mut buf = Vec::new();
            if let Some(mut stderr) = stderr_handle {
                let _ = stderr.read_to_end(&mut buf).await;
            }
            buf
        });

        let stdout_handle = child
            .stdout
            .take()
            .ok_or_else(|| "skippr stdout not available".to_string())?;
        let mut reader = BufReader::new(stdout_handle).lines();

        let mut events: Vec<serde_json::Value> = Vec::new();
        let mut timed_out = false;

        loop {
            match tokio::time::timeout(idle_timeout, reader.next_line()).await {
                Ok(Ok(Some(line))) => {
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&line) {
                        if let Some(kind) = val.get("event").and_then(|v| v.as_str()) {
                            match kind {
                                "sync_status" => {
                                    let msgs = val.get("total_rows").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let bytes = val.get("bytes").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let rows_written = val.get("rows_written").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let elapsed = val.get("elapsed_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                                    let uploads = val.get("uploads_in_flight").and_then(|v| v.as_u64()).unwrap_or(0);
                                    tracing::info!(
                                        messages = msgs,
                                        source_bytes = bytes,
                                        rows_written = rows_written,
                                        uploads_in_flight = uploads,
                                        elapsed_ms = elapsed,
                                        "skippr sync heartbeat"
                                    );
                                }
                                _ => {
                                    tracing::debug!(event = kind, "skippr sync event");
                                }
                            }
                        }
                        events.push(val);
                    }
                }
                Ok(Ok(None)) => break,
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "error reading skippr stdout");
                    break;
                }
                Err(_) => {
                    tracing::error!(
                        idle_timeout_secs = idle_timeout.as_secs(),
                        "skippr sync idle timeout — no output received"
                    );
                    timed_out = true;
                    if let Some(pid) = child_id {
                        let _ = tokio::process::Command::new("kill")
                            .args(["-9", &pid.to_string()])
                            .status()
                            .await;
                    }
                    break;
                }
            }
        }

        let exit_status = child.wait().await;
        let stderr_bytes = stderr_task.await.unwrap_or_default();

        if timed_out {
            return Err(format!(
                "skippr sync stalled — no output for {}s (args: {:?})",
                idle_timeout.as_secs(),
                args,
            ));
        }

        let success = exit_status.as_ref().map(|s| s.success()).unwrap_or(false);
        if !success {
            let stderr = String::from_utf8_lossy(&stderr_bytes);
            tracing::warn!(
                exit_code = ?exit_status.as_ref().ok().and_then(|s| s.code()),
                stderr = %stderr,
                "skippr command failed"
            );
        }

        Ok((events, stderr_bytes, success))
    }

    fn generate_skippr_yml(&self, config: &SkipprPipelineConfig) -> String {
        let input_block = &config.skippr_input;
        let input_kind = input_block
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown");
        let capitalized_kind = capitalize_first(input_kind);

        let mut input_config = input_block.clone();
        let transform_block = if let Some(obj) = input_config.as_object_mut() {
            obj.remove("kind");
            obj.remove("transform")
        } else {
            None
        };
        resolve_env_refs_in_value(&mut input_config);

        let output_kind = capitalize_first(&config.output_plugin.kind);
        let output_config = self.build_output_config();

        let mut pipeline_block = serde_json::json!({
            "data_source": "data_sources.source",
            "data_sink": "data_sinks.destination"
        });

        if let Some(t) = transform_block {
            if t.is_object() {
                pipeline_block["transform"] = t;
            }
        }

        let mut dest_block = serde_json::json!({ output_kind: output_config });

        let has_schema_sink = config.schema_sink.is_some()
            || self.el_config.schema_sink.is_some();

        if has_schema_sink {
            dest_block["schema_sink"] = serde_json::Value::String("schema_sinks.schema".into());
        }

        let mut root = serde_json::json!({
            "skippr": {
                "storage_mode": "local"
            },
            "pipelines": {
                &config.pipeline_name: pipeline_block
            },
            "data_sources": {
                "source": {
                    capitalized_kind: input_config
                }
            },
            "data_sinks": {
                "destination": dest_block
            }
        });

        if has_schema_sink {
            let ss = config.schema_sink.as_ref().map(|s| {
                let mut m = serde_json::Map::new();
                let kind = capitalize_first(&s.kind);
                let mut inner = serde_json::Map::new();
                if let Some(ref db) = s.glue_database_name {
                    inner.insert("glue_database_name".into(), serde_json::Value::String(db.clone()));
                }
                m.insert(kind, serde_json::Value::Object(inner));
                serde_json::Value::Object(m)
            }).or_else(|| {
                self.el_config.schema_sink.as_ref().map(|ss_val| {
                    let kind = ss_val.get("kind").and_then(|v| v.as_str()).unwrap_or("unknown");
                    let cap = capitalize_first(kind);
                    let mut inner = serde_json::Map::new();
                    if let Some(v) = ss_val.get("glue_database_name").and_then(|v| v.as_str()) {
                        inner.insert("glue_database_name".into(), serde_json::Value::String(v.to_string()));
                    }
                    let mut m = serde_json::Map::new();
                    m.insert(cap, serde_json::Value::Object(inner));
                    serde_json::Value::Object(m)
                })
            });

            if let Some(ss_block) = ss {
                root["schema_sinks"] = serde_json::json!({ "schema": ss_block });
            }
        }

        let yml = serde_yaml::to_value(&root).unwrap_or_default();
        serde_yaml::to_string(&yml).unwrap_or_default()
    }

    fn build_output_config(&self) -> serde_json::Value {
        fn getenv(key: &str) -> Option<String> {
            std::env::var(key).ok().filter(|v| !v.trim().is_empty())
        }
        fn extra_str(extras: &serde_json::Value, key: &str) -> Option<String> {
            extras.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
        }

        match self.warehouse.kind {
            WarehouseKind::Snowflake => {
                let extras = &self.warehouse.extras;
                let mut cfg = serde_json::Map::new();
                if let Some(v) = getenv("SNOWFLAKE_ACCOUNT")
                    .or_else(|| extra_str(extras, "account"))
                {
                    cfg.insert("account".into(), serde_json::Value::String(v));
                }
                if let Some(v) = getenv("SNOWFLAKE_USER")
                    .or_else(|| extra_str(extras, "user"))
                {
                    cfg.insert("user".into(), serde_json::Value::String(v));
                }
                if let Some(v) = getenv("SNOWFLAKE_PASSWORD")
                    .or_else(|| extra_str(extras, "password"))
                {
                    cfg.insert("password".into(), serde_json::Value::String(v));
                }
                if let Some(v) = getenv("SNOWFLAKE_WAREHOUSE")
                    .or_else(|| extra_str(extras, "warehouse"))
                {
                    cfg.insert("warehouse".into(), serde_json::Value::String(v));
                }
                if let Some(v) = getenv("SNOWFLAKE_ROLE")
                    .or_else(|| extra_str(extras, "role"))
                {
                    cfg.insert("role".into(), serde_json::Value::String(v));
                }
                if let Some(v) = getenv("SNOWFLAKE_DATABASE") {
                    cfg.insert("database".into(), serde_json::Value::String(v));
                } else if !self.warehouse.container.is_empty() {
                    cfg.insert(
                        "database".into(),
                        serde_json::Value::String(resolve_env_ref(&self.warehouse.container)),
                    );
                }
                if let Some(v) = getenv("SNOWFLAKE_SCHEMA") {
                    cfg.insert("schema".into(), serde_json::Value::String(v));
                } else if !self.warehouse.namespace.is_empty() {
                    cfg.insert(
                        "schema".into(),
                        serde_json::Value::String(resolve_env_ref(&self.warehouse.namespace)),
                    );
                }
                if let Some(v) = getenv("SNOWFLAKE_PRIVATE_KEY_PATH")
                    .or_else(|| extra_str(extras, "private_key_path"))
                {
                    cfg.insert(
                        "private_key_path".into(),
                        serde_json::Value::String(resolve_to_absolute(&v)),
                    );
                }
                serde_json::Value::Object(cfg)
            }
            WarehouseKind::Bigquery => {
                let extras = &self.warehouse.extras;
                let mut cfg = serde_json::Map::new();
                if let Some(v) = getenv("BIGQUERY_PROJECT") {
                    cfg.insert("project".into(), serde_json::Value::String(v));
                } else if !self.warehouse.container.is_empty() {
                    cfg.insert(
                        "project".into(),
                        serde_json::Value::String(resolve_env_ref(&self.warehouse.container)),
                    );
                }
                if let Some(v) = getenv("BIGQUERY_DATASET") {
                    cfg.insert("dataset".into(), serde_json::Value::String(v));
                } else if !self.warehouse.namespace.is_empty() {
                    cfg.insert(
                        "dataset".into(),
                        serde_json::Value::String(resolve_env_ref(&self.warehouse.namespace)),
                    );
                }
                if let Some(v) = getenv("BIGQUERY_LOCATION")
                    .or_else(|| extra_str(extras, "location"))
                {
                    cfg.insert("location".into(), serde_json::Value::String(v));
                }
                if let Some(v) = getenv("GOOGLE_APPLICATION_CREDENTIALS") {
                    cfg.insert(
                        "credentials_path".into(),
                        serde_json::Value::String(resolve_to_absolute(&v)),
                    );
                }
                serde_json::Value::Object(cfg)
            }
            WarehouseKind::Postgres => {
                let mut cfg = serde_json::Map::new();
                if let Some(v) = getenv("POSTGRES_HOST") {
                    cfg.insert("host".into(), serde_json::Value::String(v));
                }
                if let Some(v) = getenv("POSTGRES_PORT") {
                    if let Ok(port) = v.parse::<u16>() {
                        cfg.insert("port".into(), serde_json::Value::Number(port.into()));
                    }
                }
                if let Some(v) = getenv("POSTGRES_USER") {
                    cfg.insert("user".into(), serde_json::Value::String(v));
                }
                if let Some(v) = getenv("POSTGRES_PASSWORD") {
                    cfg.insert("password".into(), serde_json::Value::String(v));
                }
                if let Some(v) = getenv("POSTGRES_DATABASE") {
                    cfg.insert("database".into(), serde_json::Value::String(v));
                } else if !self.warehouse.container.is_empty() {
                    cfg.insert(
                        "database".into(),
                        serde_json::Value::String(resolve_env_ref(&self.warehouse.container)),
                    );
                }
                if let Some(v) = getenv("POSTGRES_SCHEMA") {
                    cfg.insert("schema".into(), serde_json::Value::String(v));
                } else if !self.warehouse.namespace.is_empty() {
                    cfg.insert(
                        "schema".into(),
                        serde_json::Value::String(resolve_env_ref(&self.warehouse.namespace)),
                    );
                }
                if let Some(v) = getenv("POSTGRES_SSLMODE") {
                    cfg.insert("sslmode".into(), serde_json::Value::String(v));
                }
                serde_json::Value::Object(cfg)
            }
            WarehouseKind::Databricks => {
                let extras = &self.warehouse.extras;
                let mut cfg = serde_json::Map::new();
                if let Some(v) = getenv("DATABRICKS_WORKSPACE_URL").or_else(|| extra_str(extras, "workspace_url")) { cfg.insert("workspace_url".into(), serde_json::Value::String(v)); }
                if let Some(v) = getenv("DATABRICKS_TOKEN").or_else(|| extra_str(extras, "token")) { cfg.insert("token".into(), serde_json::Value::String(v)); }
                if let Some(v) = getenv("DATABRICKS_WAREHOUSE_ID").or_else(|| extra_str(extras, "warehouse_id")) { cfg.insert("warehouse_id".into(), serde_json::Value::String(v)); }
                if !self.warehouse.container.is_empty() { cfg.insert("catalog".into(), serde_json::Value::String(resolve_env_ref(&self.warehouse.container))); }
                if !self.warehouse.namespace.is_empty() { cfg.insert("schema".into(), serde_json::Value::String(resolve_env_ref(&self.warehouse.namespace))); }
                serde_json::Value::Object(cfg)
            }
            WarehouseKind::Synapse => {
                let extras = &self.warehouse.extras;
                let mut cfg = serde_json::Map::new();
                if let Some(v) = getenv("SYNAPSE_CONNECTION_STRING").or_else(|| extra_str(extras, "connection_string")) { cfg.insert("connection_string".into(), serde_json::Value::String(v)); }
                if !self.warehouse.namespace.is_empty() { cfg.insert("schema".into(), serde_json::Value::String(resolve_env_ref(&self.warehouse.namespace))); }
                serde_json::Value::Object(cfg)
            }
            WarehouseKind::Redshift => {
                let extras = &self.warehouse.extras;
                let mut cfg = serde_json::Map::new();
                if !self.warehouse.container.is_empty() { cfg.insert("database".into(), serde_json::Value::String(resolve_env_ref(&self.warehouse.container))); }
                for (json_key, env_key) in [("cluster_identifier", "REDSHIFT_CLUSTER_IDENTIFIER"), ("workgroup_name", "REDSHIFT_WORKGROUP_NAME"), ("db_user", "REDSHIFT_DB_USER"), ("region", "REDSHIFT_REGION"), ("staging_s3_bucket", "REDSHIFT_STAGING_S3_BUCKET"), ("staging_s3_prefix", "REDSHIFT_STAGING_S3_PREFIX"), ("iam_role_arn", "REDSHIFT_IAM_ROLE_ARN")] {
                    if let Some(v) = getenv(env_key).or_else(|| extra_str(extras, json_key)) { cfg.insert(json_key.into(), serde_json::Value::String(v)); }
                }
                serde_json::Value::Object(cfg)
            }
            WarehouseKind::Clickhouse => {
                let extras = &self.warehouse.extras;
                let mut cfg = serde_json::Map::new();
                if let Some(v) = getenv("CLICKHOUSE_URL").or_else(|| extra_str(extras, "url")) { cfg.insert("url".into(), serde_json::Value::String(v)); }
                if !self.warehouse.container.is_empty() { cfg.insert("database".into(), serde_json::Value::String(resolve_env_ref(&self.warehouse.container))); }
                if let Some(v) = getenv("CLICKHOUSE_USER").or_else(|| extra_str(extras, "user")) { cfg.insert("user".into(), serde_json::Value::String(v)); }
                if let Some(v) = getenv("CLICKHOUSE_PASSWORD").or_else(|| extra_str(extras, "password")) { cfg.insert("password".into(), serde_json::Value::String(v)); }
                serde_json::Value::Object(cfg)
            }
            WarehouseKind::Duckdb => {
                let extras = &self.warehouse.extras;
                let mut cfg = serde_json::Map::new();
                if let Some(v) = getenv("DUCKDB_CONNECTION_STRING").or_else(|| extra_str(extras, "connection_string")) { cfg.insert("connection_string".into(), serde_json::Value::String(v)); }
                if let Some(v) = getenv("MOTHERDUCK_TOKEN").or_else(|| extra_str(extras, "motherduck_token")) { cfg.insert("motherduck_token".into(), serde_json::Value::String(v)); }
                if !self.warehouse.container.is_empty() { cfg.insert("database".into(), serde_json::Value::String(resolve_env_ref(&self.warehouse.container))); }
                serde_json::Value::Object(cfg)
            }
            WarehouseKind::Athena | WarehouseKind::Mssql => serde_json::json!({}),
        }
    }
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
    }
}

fn resolve_env_ref(value: &str) -> String {
    if value.starts_with("${") && value.ends_with('}') {
        let var_name = &value[2..value.len() - 1];
        std::env::var(var_name).unwrap_or_else(|_| value.to_string())
    } else {
        value.to_string()
    }
}

fn resolve_env_refs_in_value(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::String(s) => {
            let resolved = resolve_env_ref(s);
            if resolved != *s {
                *s = resolved;
            }
        }
        serde_json::Value::Object(map) => {
            for v in map.values_mut() {
                resolve_env_refs_in_value(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr.iter_mut() {
                resolve_env_refs_in_value(v);
            }
        }
        _ => {}
    }
}

fn resolve_to_absolute(path: &str) -> String {
    let p = std::path::Path::new(path);
    if p.is_absolute() {
        return path.to_string();
    }
    std::env::current_dir()
        .ok()
        .map(|cwd| cwd.join(p).to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string())
}

fn insert_snowflake_env(wh: &WarehouseResolved, env: &mut HashMap<String, String>) {
    fn getenv(key: &str) -> Option<String> {
        std::env::var(key).ok().filter(|v| !v.trim().is_empty())
    }
    fn extra_str(extras: &serde_json::Value, key: &str) -> Option<String> {
        extras.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
    }

    if let Some(v) = getenv("SNOWFLAKE_ACCOUNT")
        .or_else(|| extra_str(&wh.extras, "account"))
    {
        env.insert("SNOWFLAKE_ACCOUNT".into(), v);
    }
    if let Some(v) = getenv("SNOWFLAKE_USER")
        .or_else(|| extra_str(&wh.extras, "user"))
    {
        env.insert("SNOWFLAKE_USER".into(), v);
    }
    if let Some(v) = getenv("SNOWFLAKE_PASSWORD")
        .or_else(|| extra_str(&wh.extras, "password"))
    {
        env.insert("SNOWFLAKE_PASSWORD".into(), v);
    }
    if let Some(v) = getenv("SNOWFLAKE_PRIVATE_KEY_PATH")
        .or_else(|| extra_str(&wh.extras, "private_key_path"))
    {
        env.insert("SNOWFLAKE_PRIVATE_KEY_PATH".into(), resolve_to_absolute(&v));
    }
    if let Some(v) = getenv("SNOWFLAKE_WAREHOUSE")
        .or_else(|| extra_str(&wh.extras, "warehouse"))
    {
        env.insert("SNOWFLAKE_WAREHOUSE".into(), v);
    }
    if let Some(v) = getenv("SNOWFLAKE_ROLE").or_else(|| extra_str(&wh.extras, "role")) {
        env.insert("SNOWFLAKE_ROLE".into(), v);
    }
    if let Some(v) = getenv("SNOWFLAKE_DATABASE") {
        env.insert("SNOWFLAKE_DATABASE".into(), v);
    } else if !wh.container.is_empty() {
        env.insert("SNOWFLAKE_DATABASE".into(), resolve_env_ref(&wh.container));
    }
    if let Some(v) = getenv("SNOWFLAKE_SCHEMA") {
        env.insert("SNOWFLAKE_SCHEMA".into(), v);
    } else if !wh.namespace.is_empty() {
        env.insert("SNOWFLAKE_SCHEMA".into(), resolve_env_ref(&wh.namespace));
    }
}

fn parse_json_lines(stdout: &[u8]) -> Vec<serde_json::Value> {
    let text = String::from_utf8_lossy(stdout);
    text.lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}

#[async_trait]
impl SkipprProvider for SkipprCliProvider {
    async fn write_pipeline_config(
        &self,
        _scope: &RequestScope,
        config: &SkipprPipelineConfig,
    ) -> Result<String, String> {
        let yml_content = self.generate_skippr_yml(config);
        let yml_path = self.skippr_yml_path();

        std::fs::create_dir_all(&self.data_dir)
            .map_err(|e| format!("failed to create data dir: {}", e))?;

        std::fs::write(&yml_path, yml_content.as_bytes())
            .map_err(|e| format!("failed to write skippr-el.yml: {}", e))?;

        tracing::info!(path = %yml_path.display(), "wrote skippr-el.yml");
        Ok(yml_path.to_string_lossy().to_string())
    }

    async fn discover_pipeline(
        &self,
        _scope: &RequestScope,
        pipeline: &str,
    ) -> Result<SkipprDiscoverResult, String> {
        let output = self
            .run_skippr(&["discover", "--pipeline", pipeline, "--output", "json"])
            .await?;

        let events = parse_json_lines(&output.stdout);
        let mut namespaces = Vec::new();
        let mut errors = Vec::new();

        for event in &events {
            if let Some(kind) = event.get("event").and_then(|v| v.as_str()) {
                if kind == "namespace_discovered" {
                    if let Some(ns) = event.get("namespace").and_then(|v| v.as_str()) {
                        let fields = event
                            .get("fields")
                            .and_then(|v| v.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|f| {
                                        Some(SkipprFieldSchema {
                                            name: f.get("name")?.as_str()?.to_string(),
                                            field_type: f.get("type")?.as_str()?.to_string(),
                                            nullable: f
                                                .get("nullable")
                                                .and_then(|v| v.as_bool())
                                                .unwrap_or(true),
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        namespaces.push(SkipprNamespaceSchema {
                            namespace: ns.to_string(),
                            fields,
                        });
                    }
                } else if kind == "error" {
                    if let Some(msg) = event.get("message").and_then(|v| v.as_str()) {
                        errors.push(msg.to_string());
                    }
                }
            }
        }

        if !output.status.success() && errors.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            errors.push(format!("skippr discover failed: {}", stderr.trim()));
        }

        Ok(SkipprDiscoverResult {
            ok: output.status.success(),
            namespaces,
            errors,
        })
    }

    async fn show_pipeline(
        &self,
        _scope: &RequestScope,
        pipeline: &str,
    ) -> Result<SkipprPipelineStatus, String> {
        let sql = format!("SHOW PIPELINE {}", pipeline);
        let output = self
            .run_skippr(&["query", "--plain", "--sql", &sql])
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("SHOW PIPELINE failed: {}", stderr.trim()));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let parsed: serde_json::Value = serde_json::from_str(stdout.trim())
            .map_err(|e| format!("failed to parse SHOW PIPELINE output: {}", e))?;

        let status = parsed
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let metadata_location = parsed
            .get("metadata_location")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let namespaces = parsed
            .get("namespaces")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|ns| {
                        let namespace = ns.get("name")
                            .or_else(|| ns.get("namespace"))
                            ?.as_str()?.to_string();
                        let fields = ns
                            .get("fields")
                            .and_then(|v| v.as_array())
                            .map(|farr| {
                                farr.iter()
                                    .filter_map(|f| {
                                        Some(SkipprFieldSchema {
                                            name: f.get("name")?.as_str()?.to_string(),
                                            field_type: f.get("type")?.as_str()?.to_string(),
                                            nullable: f
                                                .get("nullable")
                                                .and_then(|v| v.as_bool())
                                                .unwrap_or(true),
                                        })
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        let offset = ns.get("offset").cloned();
                        Some(SkipprNamespaceStatus {
                            namespace,
                            fields,
                            offset,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(SkipprPipelineStatus {
            pipeline: pipeline.to_string(),
            status,
            namespaces,
            metadata_location,
        })
    }

    async fn load_schema(
        &self,
        _scope: &RequestScope,
        pipeline: &str,
        schema_json_path: &str,
    ) -> Result<(), String> {
        let sql = format!("LOAD SCHEMA '{}' INTO {}", schema_json_path, pipeline);
        let output = self
            .run_skippr(&["query", "--plain", "--sql", &sql])
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("LOAD SCHEMA failed: {}", stderr.trim()));
        }
        Ok(())
    }

    async fn sync_pipeline(
        &self,
        _scope: &RequestScope,
        pipeline: &str,
    ) -> Result<SkipprSyncResult, String> {
        let idle_timeout = std::time::Duration::from_secs(120);
        let (events, stderr_bytes, success) = self
            .run_skippr_streaming(
                &[
                    "sync",
                    "--pipeline",
                    pipeline,
                    "--once",
                    "--output",
                    "json",
                ],
                idle_timeout,
            )
            .await?;

        let mut tables_synced = 0usize;
        let mut errors = Vec::new();

        for event in &events {
            if let Some(kind) = event.get("event").and_then(|v| v.as_str()) {
                match kind {
                    "sync_complete" | "table_synced" | "namespace_synced" => {
                        tables_synced += 1;
                    }
                    "error" => {
                        if let Some(msg) = event.get("message").and_then(|v| v.as_str()) {
                            errors.push(msg.to_string());
                        }
                    }
                    _ => {}
                }
            }
        }

        if !success && errors.is_empty() {
            let stderr = String::from_utf8_lossy(&stderr_bytes);
            errors.push(format!("skippr sync failed: {}", stderr.trim()));
        }

        Ok(SkipprSyncResult {
            ok: success,
            tables_synced,
            events,
            errors,
        })
    }
}
