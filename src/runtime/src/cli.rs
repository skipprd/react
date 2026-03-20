use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use clap::{Parser, Subcommand};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;
use tokio::task::JoinSet;

use react_core::suite::SuiteRegistry;

use crate::run_engine;

#[derive(Parser, Debug)]
struct Cli {
    /// Enable logging (defaults to `info` when present). Respects `RUST_LOG` if set.
    #[arg(long, num_args = 0..=1, default_missing_value = "info")]
    log: Option<String>,

    /// Render a live terminal UI (phases/tasks/tools) instead of relying on logs.
    ///
    /// - Requires a TTY stdout.
    /// - Press `q` to close the UI (server keeps running).
    #[arg(long, global = true, default_value_t = false)]
    terminal: bool,

    /// Include very verbose AWS S3/Smithy HTTP logs when using `--log debug` / `--log trace`.
    ///
    /// By default, `--log debug` suppresses noisy AWS request/response logging to keep output readable.
    #[arg(long, default_value_t = false)]
    verbose_debug: bool,

    #[command(subcommand)]
    cmd: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Start the WebSocket server.
    Serve {
        /// Path to YAML config file.
        #[arg(long, value_name = "PATH")]
        config: String,

        /// WebSocket port to listen on.
        #[arg(long)]
        port: Option<u16>,

        /// Storage mode for artifacts/threads/catalog/vectors.
        ///
        /// - `local` stores under `--storage-path` (default).
        /// - `s3` stores in an S3 bucket (requires `--bucket` or env `SKIPPR_S3_BUCKET`).
        #[arg(long, value_name = "MODE")]
        storage_mode: Option<String>,

        /// Local storage root directory (only used when `--storage-mode local`).
        #[arg(long, value_name = "PATH")]
        storage_path: Option<String>,

        /// S3 bucket for artifacts (only used when `--storage-mode s3`).
        /// Defaults to env `SKIPPR_S3_BUCKET` if set.
        #[arg(long)]
        bucket: Option<String>,

        /// Tenant scope (artifact partition).
        #[arg(long)]
        tenant: Option<String>,

        /// Workspace scope (artifact partition).
        #[arg(long)]
        workspace: Option<String>,

        /// Project identifier (artifact partition).
        #[arg(long)]
        project_id: Option<String>,
    },

    /// Run a headless thread in the terminal (no WebSocket server).
    ///
    /// - If `--thread-id` is provided, the thread must exist and the initial prompt is `continue`.
    /// - Otherwise a new thread is created with initial prompt `go`.
    Run {
        /// Path(s) to YAML config file(s). Repeat `--config` to run multiple.
        #[arg(long, value_name = "PATH", required = true, num_args = 1..)]
        config: Vec<String>,

        /// Run multiple `--config` entries concurrently in this process.
        ///
        /// - Requires at least two `--config` values.
        /// - Prints compact per-config progress and exits non-zero if any run fails.
        #[arg(long, default_value_t = false)]
        parallel: bool,

        /// Existing thread id to continue.
        #[arg(long)]
        thread_id: Option<String>,

        /// Suite to run (defaults to first registered suite).
        #[arg(long)]
        suite_id: Option<String>,

        /// Agent type to run (defaults to agent).
        #[arg(long, default_value = "agent")]
        agent: String,

        /// Storage mode for artifacts/threads/catalog/vectors.
        #[arg(long, value_name = "MODE")]
        storage_mode: Option<String>,

        /// Local storage root directory (only used when `--storage-mode local`).
        #[arg(long, value_name = "PATH")]
        storage_path: Option<String>,

        /// S3 bucket for artifacts (only used when `--storage-mode s3`).
        /// Defaults to env `SKIPPR_S3_BUCKET` if set.
        #[arg(long)]
        bucket: Option<String>,

        /// Tenant scope (artifact partition).
        #[arg(long)]
        tenant: Option<String>,

        /// Workspace scope (artifact partition).
        #[arg(long)]
        workspace: Option<String>,

        /// Project identifier (artifact partition).
        #[arg(long)]
        project_id: Option<String>,
    },
}

fn config_run_label(index: usize, config_path: &str) -> String {
    let stem = Path::new(config_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("config");
    format!("{:02}-{}", index + 1, stem)
}

fn sanitize_for_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

async fn run_parallel_configs(
    log: &Option<String>,
    verbose_debug: bool,
    configs: &[String],
    suite_id: &Option<String>,
    agent: &str,
    storage_mode: &Option<String>,
    storage_path: &Option<String>,
    bucket: &Option<String>,
    tenant: &Option<String>,
    workspace: &Option<String>,
    project_id: &Option<String>,
) -> Result<i32, String> {
    if configs.len() < 2 {
        return Err("parallel mode requires at least two --config values".to_string());
    }

    let exe = std::env::current_exe()
        .map_err(|e| format!("failed to resolve current executable path: {e}"))?;
    let logs_dir = PathBuf::from("./.react/multi-run-logs");
    std::fs::create_dir_all(&logs_dir).map_err(|e| {
        format!(
            "failed to create multi-run log dir '{}': {e}",
            logs_dir.display()
        )
    })?;

    println!("Starting parallel run for {} config(s)", configs.len());

    let mut joins = JoinSet::new();
    for (idx, config_path) in configs.iter().enumerate() {
        let label = config_run_label(idx, config_path);
        let log_file = logs_dir.join(format!("{}.log", sanitize_for_filename(&label)));
        let file = tokio::fs::File::create(&log_file).await.map_err(|e| {
            format!(
                "failed to create log file '{}' for {}: {e}",
                log_file.display(),
                label
            )
        })?;
        let shared_file = Arc::new(Mutex::new(file));

        let mut cmd = tokio::process::Command::new(&exe);
        if let Some(level) = log.as_ref() {
            cmd.arg("--log").arg(level);
        } else {
            cmd.arg("--log").arg("info");
        }
        if verbose_debug {
            cmd.arg("--verbose-debug");
        }
        cmd.env("REACT_PLAIN_PROGRESS", "1");
        cmd.arg("run")
            .arg("--config")
            .arg(config_path)
            .arg("--agent")
            .arg(agent);
        if let Some(s) = suite_id.as_ref().filter(|s| !s.trim().is_empty()) {
            cmd.arg("--suite-id").arg(s);
        }

        if let Some(v) = storage_mode.as_ref() {
            cmd.arg("--storage-mode").arg(v);
        }
        if let Some(v) = storage_path.as_ref() {
            cmd.arg("--storage-path").arg(v);
        }
        if let Some(v) = bucket.as_ref() {
            cmd.arg("--bucket").arg(v);
        }
        if let Some(v) = tenant.as_ref() {
            cmd.arg("--tenant").arg(v);
        }
        if let Some(v) = workspace.as_ref() {
            cmd.arg("--workspace").arg(v);
        }
        if let Some(v) = project_id.as_ref() {
            cmd.arg("--project-id").arg(v);
        }

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        let started_at = Instant::now();
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn {} ({config_path}): {e}", label))?;
        println!(
            "[{}] started ({}) log={}",
            label,
            config_path,
            log_file.display()
        );
        let stdout = child.stdout.take().ok_or_else(|| {
            format!(
                "failed to capture stdout pipe for {} ({})",
                label, config_path
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            format!(
                "failed to capture stderr pipe for {} ({})",
                label, config_path
            )
        })?;
        let stdout_task = tokio::spawn(stream_child_output(
            label.clone(),
            false,
            stdout,
            shared_file.clone(),
        ));
        let stderr_task = tokio::spawn(stream_child_output(
            label.clone(),
            true,
            stderr,
            shared_file.clone(),
        ));

        let cfg = config_path.clone();
        let lb = label.clone();
        let lf = log_file.clone();
        joins.spawn(async move {
            let status = child.wait().await;
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            (lb, cfg, lf, started_at, status)
        });
    }

    let mut failures = 0usize;
    while let Some(next) = joins.join_next().await {
        let (label, config_path, log_file, started_at, status) =
            next.map_err(|e| format!("parallel runner task join failed: {e}"))?;
        let elapsed = started_at.elapsed().as_secs_f32();
        match status {
            Ok(s) if s.success() => {
                println!("[{}] ok ({:.1}s) {}", label, elapsed, config_path);
            }
            Ok(s) => {
                failures += 1;
                let code = s
                    .code()
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "signal".to_string());
                println!(
                    "[{}] failed ({:.1}s, exit={}) {} log={}",
                    label,
                    elapsed,
                    code,
                    config_path,
                    log_file.display()
                );
            }
            Err(e) => {
                failures += 1;
                println!(
                    "[{}] failed ({:.1}s, spawn/wait error={}) {} log={}",
                    label,
                    elapsed,
                    e,
                    config_path,
                    log_file.display()
                );
            }
        }
    }

    if failures > 0 {
        println!(
            "Parallel run finished: {} failed, {} succeeded",
            failures,
            configs.len().saturating_sub(failures)
        );
        Ok(1)
    } else {
        println!(
            "Parallel run finished: all {} config(s) succeeded",
            configs.len()
        );
        Ok(0)
    }
}

async fn stream_child_output<R>(
    label: String,
    is_stderr: bool,
    reader: R,
    file: Arc<Mutex<tokio::fs::File>>,
) where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        println!("[{}] {}", label, line);
        let prefix = if is_stderr { "stderr" } else { "stdout" };
        let mut f = file.lock().await;
        let _ = tokio::io::AsyncWriteExt::write_all(
            &mut *f,
            format!("[{}] {}\n", prefix, line).as_bytes(),
        )
        .await;
    }
}

/// Shared CLI entrypoint. Parses args, runs the requested command using the
/// provided suite registry. Binary name in `--help` is inferred from `argv[0]`.
pub async fn run(registry: SuiteRegistry) {
    let cli = Cli::parse();

    match cli.cmd {
        Command::Serve {
            config,
            port,
            storage_mode,
            storage_path,
            bucket,
            tenant,
            workspace,
            project_id,
        } => {
            run_engine::init_logging(&cli.log, cli.verbose_debug);
            let file_cfg = match crate::config::ReactConfigFile::load_yaml(Path::new(&config)) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("{}", e);
                    std::process::exit(1);
                }
            };
            let cfg = match crate::config::resolve_config(
                file_cfg,
                crate::config::ServeOverrides {
                    port,
                    storage_mode,
                    bucket,
                    storage_path,
                    tenant,
                    workspace,
                    project_id,
                },
            ) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("{}", e);
                    std::process::exit(1);
                }
            };
            run_engine::bind_runtime_scope_preference(&cfg.scope);
            crate::runtime_settings::bind_resolved_config(&cfg);

            let log_dir = run_engine::resolve_log_dir(&cfg);
            let enable_console = cli.log.is_some() && !cli.terminal;
            let _guards = run_engine::init_tracing(&log_dir, enable_console, None);

            if cli.terminal {
                if std::env::var("REACT_HEADLESS")
                    .ok()
                    .filter(|v| !v.trim().is_empty())
                    .is_none()
                {
                    std::env::set_var("REACT_HEADLESS", "1");
                }
                if let Err(e) = react_transport::ws::terminal::init() {
                    tracing::warn!("terminal mode not enabled: {}", e);
                }
            }

            let suite_ctx = match crate::bootstrap::build_suite_ctx(&cfg).await {
                Ok(ctx) => ctx,
                Err(e) => {
                    tracing::error!("{}", e);
                    std::process::exit(1);
                }
            };

            if let Err(e) =
                react_transport::ws::server::start_with_ctx(cfg.server.port, suite_ctx, registry)
                    .await
            {
                tracing::error!("{}", e);
                std::process::exit(1);
            }
        }
        Command::Run {
            config,
            parallel,
            thread_id,
            suite_id,
            agent,
            storage_mode,
            storage_path,
            bucket,
            tenant,
            workspace,
            project_id,
        } => {
            if config.len() > 1 {
                if cli.terminal {
                    tracing::error!("--terminal is not supported with parallel multi-config runs");
                    std::process::exit(2);
                }
                if !parallel {
                    tracing::error!("multiple --config values require --parallel");
                    std::process::exit(2);
                }
                let exit_code = match run_parallel_configs(
                    &cli.log,
                    cli.verbose_debug,
                    &config,
                    &suite_id,
                    &agent,
                    &storage_mode,
                    &storage_path,
                    &bucket,
                    &tenant,
                    &workspace,
                    &project_id,
                )
                .await
                {
                    Ok(code) => code,
                    Err(e) => {
                        tracing::error!("{}", e);
                        1
                    }
                };
                std::process::exit(exit_code);
            }
            if parallel {
                tracing::warn!("--parallel ignored with a single --config");
            }
            let config = match config.into_iter().next() {
                Some(v) => v,
                None => {
                    tracing::error!("missing --config");
                    std::process::exit(2);
                }
            };

            run_engine::init_logging(&cli.log, cli.verbose_debug);

            let file_cfg = match crate::config::ReactConfigFile::load_yaml(Path::new(&config)) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("{}", e);
                    std::process::exit(1);
                }
            };
            let cfg = match crate::config::resolve_config(
                file_cfg,
                crate::config::ServeOverrides {
                    port: None,
                    storage_mode,
                    bucket,
                    storage_path,
                    tenant,
                    workspace,
                    project_id,
                },
            ) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!("{}", e);
                    std::process::exit(1);
                }
            };

            let exit_code = run_engine::run_headless_from_config(
                cfg,
                registry,
                run_engine::HeadlessRunOpts {
                    log_level: cli.log,
                    verbose_debug: cli.verbose_debug,
                    terminal: cli.terminal,
                    thread_id,
                    suite_id,
                    agent,
                    skip_logging_init: true,
                },
            )
            .await;
            std::process::exit(exit_code);
        }
    }
}
