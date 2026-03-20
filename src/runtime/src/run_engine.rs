use std::path::{Path, PathBuf};

use react_core::resolved_config as rc;
use react_core::suite::SuiteRegistry;
use tokio::sync::mpsc;
use tracing_subscriber::prelude::*;

/// Options for a headless agent run.
pub struct HeadlessRunOpts {
    pub log_level: Option<String>,
    pub verbose_debug: bool,
    pub terminal: bool,
    pub thread_id: Option<String>,
    pub suite_id: Option<String>,
    pub agent: String,
    /// When `true`, skip `init_logging` (caller already called it).
    pub skip_logging_init: bool,
}

pub(crate) struct TracingGuards {
    pub _file: tracing_appender::non_blocking::WorkerGuard,
    pub _run: Option<tracing_appender::non_blocking::WorkerGuard>,
}

pub(crate) fn init_logging(log: &Option<String>, verbose_debug: bool) {
    if std::env::var("RUST_LOG")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_none()
    {
        let level = log.as_deref().unwrap_or("info");
        if (level == "debug" || level == "trace") && !verbose_debug {
            let quiet = format!(
                "{level},\
aws_smithy_http=info,\
aws_smithy_http_tower=info,\
aws_smithy_runtime=info,\
aws_sdk_s3=info,\
aws_sdk_sts=info,\
aws_sdk_athena=info,\
aws_sdk_glue=info,\
aws_config=info,\
tokio_tungstenite=info,\
tungstenite=info,\
lance=info,\
lance_core=info,\
lance_io=info,\
lance_table=info,\
react_transport::ws=info,\
react::vector=info,\
h2=info,\
rustls=info,\
tokio_rustls=info,\
hyper_rustls=info,\
hyper=info,\
reqwest=info",
                level = level
            );
            std::env::set_var("RUST_LOG", quiet);
        } else {
            std::env::set_var("RUST_LOG", level);
        }
    }
}

pub(crate) fn resolve_log_dir(cfg: &crate::config::ReactResolvedConfig) -> PathBuf {
    if let Ok(v) = std::env::var("REACT_LOG_DIR") {
        let p = v.trim();
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    if cfg.storage.mode == rc::StorageMode::Local {
        if let Some(ref root) = cfg.storage.path {
            return PathBuf::from(root).join("logs");
        }
    }
    PathBuf::from("./.react/logs")
}

pub(crate) fn resolve_default_suite_id(registry: &SuiteRegistry) -> String {
    registry
        .list_ids()
        .into_iter()
        .next()
        .unwrap_or("kb")
        .to_string()
}

pub(crate) fn bind_runtime_scope_preference(scope: &react_core::scope::RequestScope) {
    crate::runtime_settings::set_scope_preference(
        scope.tenant.as_str().to_string(),
        scope.workspace.as_str().to_string(),
        scope.project_id.as_str().to_string(),
    );
}

pub(crate) fn init_tracing(
    log_dir: &Path,
    enable_console: bool,
    run_writer: Option<crate::thread_logs::RunThreadLogWriter>,
) -> TracingGuards {
    let _ = std::fs::create_dir_all(log_dir);
    let appender = tracing_appender::rolling::daily(log_dir, "react.log");
    let (nb, guard) = tracing_appender::non_blocking(appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(nb)
        .with_target(true)
        .with_filter(tracing_subscriber::EnvFilter::from_default_env());

    let (run_layer_opt, run_guard) = if let Some(w) = run_writer {
        let (run_nb, run_guard) = tracing_appender::non_blocking(w);
        let run_layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(run_nb)
            .with_target(true)
            .with_filter(tracing_subscriber::filter::LevelFilter::DEBUG);
        (Some(run_layer), Some(run_guard))
    } else {
        (None, None)
    };

    let console_layer_opt = if enable_console {
        Some(
            tracing_subscriber::fmt::layer()
                .with_ansi(true)
                .with_target(true)
                .with_filter(tracing_subscriber::EnvFilter::from_default_env()),
        )
    } else {
        None
    };

    let _ = tracing_subscriber::registry()
        .with(file_layer)
        .with(run_layer_opt)
        .with(console_layer_opt)
        .try_init();

    TracingGuards {
        _file: guard,
        _run: run_guard,
    }
}

/// Run a headless agent pipeline from an already-resolved config.
///
/// Returns the process exit code (0 = success). The caller decides whether to
/// call `std::process::exit` or handle the code differently.
///
/// Set `skip_logging_init` to `true` when the caller has already called
/// `init_logging` (e.g. from the generic CLI path).
pub async fn run_headless_from_config(
    cfg: rc::ReactResolvedConfig,
    registry: SuiteRegistry,
    opts: HeadlessRunOpts,
) -> i32 {
    if !opts.skip_logging_init {
        init_logging(&opts.log_level, opts.verbose_debug);
    }

    bind_runtime_scope_preference(&cfg.scope);
    crate::runtime_settings::bind_resolved_config(&cfg);

    let log_dir = resolve_log_dir(&cfg);
    let terminal_default = true;
    let terminal_enabled = opts.terminal || (terminal_default && opts.log_level.is_none());
    let enable_console = opts.log_level.is_some() && !terminal_enabled;

    if terminal_enabled {
        if std::env::var("REACT_LOG_LLM_CALLS")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .is_none()
        {
            std::env::set_var("REACT_LOG_LLM_CALLS", "1");
        }
        if std::env::var("REACT_LOG_LLM_RESPONSE_TEXT")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .is_none()
        {
            std::env::set_var("REACT_LOG_LLM_RESPONSE_TEXT", "1");
        }
    }

    let run_logs = if terminal_enabled {
        if cfg.storage.mode == rc::StorageMode::Local {
            if let Some(root) = cfg.storage.path.as_ref() {
                crate::thread_logs::RunThreadLogs::new_local(root.clone(), cfg.scope.clone()).ok()
            } else {
                None
            }
        } else {
            crate::thread_logs::RunThreadLogs::new_buffered(cfg.scope.clone()).ok()
        }
    } else {
        None
    };
    let run_writer = run_logs.as_ref().map(|l| l.make_writer());
    let guards = init_tracing(&log_dir, enable_console, run_writer);

    if std::env::var("REACT_HEADLESS")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .is_none()
    {
        std::env::set_var("REACT_HEADLESS", "1");
    }
    if terminal_enabled {
        if let Err(e) = react_transport::ws::terminal::init() {
            tracing::warn!("terminal mode not enabled: {}", e);
        }
    }

    let suite_ctx = match crate::bootstrap::build_suite_ctx(&cfg).await {
        Ok(ctx) => ctx,
        Err(e) => {
            tracing::error!("{}", e);
            return 1;
        }
    };
    let keyspace = suite_ctx.keyspace().clone();

    let requested_thread_id = opts.thread_id.clone();

    if let (Some(ref tid), Some(ref logs)) = (opts.thread_id.as_ref(), run_logs.as_ref()) {
        let _ = logs.bind_thread_id(keyspace.as_ref(), tid);
    }

    let storage_for_logs = suite_ctx.storage().clone();
    let keyspace_for_logs = suite_ctx.keyspace().clone();

    let (thread_id_tx, tid_rx) = if run_logs.is_some() && requested_thread_id.is_none() {
        let (tx, rx) = mpsc::unbounded_channel::<String>();
        (Some(tx), Some(rx))
    } else {
        (None, None)
    };

    if let (Some(mut rx), Some(logs)) = (tid_rx, run_logs.clone()) {
        let ks = keyspace.clone();
        tokio::spawn(async move {
            if let Some(tid) = rx.recv().await {
                let _ = logs.bind_thread_id(ks.as_ref(), &tid);
            }
        });
    }

    let suite_id = opts
        .suite_id
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| resolve_default_suite_id(&registry));
    let run_fut = react_transport::headless::run_headless(
        suite_ctx,
        react_transport::headless::RunOpts {
            thread_id: opts.thread_id,
            suite_id,
            agent: opts.agent,
            thread_id_tx,
        },
        registry,
    );

    let mut thread_id_for_logs: Option<String> = None;
    let exit_code: i32 = tokio::select! {
        r = run_fut => {
            match r {
                Ok((code, tid)) => {
                    thread_id_for_logs = Some(tid);
                    code
                }
                Err(e) => {
                    tracing::error!("{}", e);
                    1
                }
            }
        },
        _ = tokio::signal::ctrl_c() => 130,
    };
    if terminal_enabled {
        react_transport::ws::terminal::shutdown();
    }

    drop(guards);

    if let Some(logs) = run_logs.as_ref() {
        let tid = thread_id_for_logs
            .clone()
            .or_else(|| requested_thread_id.clone());
        if let Some(tid) = tid {
            let _ = logs.bind_thread_id(keyspace.as_ref(), &tid);
            let _ = logs
                .upload_if_needed(storage_for_logs.clone(), keyspace_for_logs.clone(), &tid)
                .await;
        }
    }

    exit_code
}
