use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::keyspace_for_scope;
use react_core::error::CoreError;
use react_core::provider_traits::NullSecretsProvider;
use react_core::session::analysis;
use react_core::session::ThreadStore;
use react_core::storage::{ConditionalWriteStatus, StorageAdapter};
use react_core::suite::SuiteCtx;
use react_suite_debugger::SuiteDebugger;

use crate::accounting;
use crate::debug;
use crate::display;
use crate::nav::ShellState;

/// Wraps a real storage adapter but silently drops all writes.
/// Used for debug sessions so the agent loop doesn't pollute real data.
struct ReadOnlyStorage(Arc<dyn StorageAdapter>);

#[async_trait]
impl StorageAdapter for ReadOnlyStorage {
    async fn get_json(&self, key: &str) -> Result<Value, CoreError> {
        self.0.get_json(key).await
    }
    async fn put_json(&self, _key: &str, _value: &Value) -> Result<(), CoreError> {
        Ok(())
    }
    async fn put_json_if_etag_matches(
        &self,
        _key: &str,
        _value: &Value,
        _expected_etag: Option<&str>,
    ) -> Result<ConditionalWriteStatus, CoreError> {
        Ok(ConditionalWriteStatus::Written)
    }
    async fn get_bytes(&self, key: &str) -> Result<Vec<u8>, CoreError> {
        self.0.get_bytes(key).await
    }
    async fn put_bytes(
        &self,
        _key: &str,
        _bytes: &[u8],
        _content_type: &str,
    ) -> Result<(), CoreError> {
        Ok(())
    }
    async fn delete_object(&self, _key: &str) -> Result<(), CoreError> {
        Ok(())
    }
    async fn head_etag(&self, key: &str) -> Result<Option<String>, CoreError> {
        self.0.head_etag(key).await
    }
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>, CoreError> {
        self.0.list_prefix(prefix).await
    }
}

pub struct AppCtx {
    pub ddb_client: aws_sdk_dynamodb::Client,
    pub ddb_table: String,
    pub storage: Arc<dyn react_core::storage::StorageAdapter>,
    pub llm: react_core::llm::DynLlm,
    pub suite_debugger: SuiteDebugger,
}

pub async fn dispatch(line: &str, state: &mut ShellState, app: &AppCtx) {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return;
    }

    let cmd = parts[0];
    let args = &parts[1..];
    let depth = state.depth();

    match cmd {
        "help" => print_help(depth),
        "quit" | "exit" => std::process::exit(0),
        "ls" => handle_ls(state).await,
        "cd" => handle_cd(args, state).await,
        ".." => {
            state.cd_up();
            state.refresh_children().await;
        }
        "account" if depth == 1 => {
            handle_account(state, app).await;
        }
        "ledger" if depth == 1 => {
            let limit = args
                .first()
                .and_then(|s| s.parse::<i32>().ok())
                .unwrap_or(20);
            handle_ledger(state, app, limit).await;
        }
        "threads" if depth == 3 => {
            handle_threads(state, app).await;
        }
        "thread" if depth == 3 => {
            if let Some(id) = args.first() {
                handle_thread(state, app, id).await;
            } else {
                println!("  Usage: thread <thread_id>");
            }
        }
        "log" if depth == 3 => {
            if let Some(id) = args.first() {
                handle_log(state, app, id).await;
            } else {
                println!("  Usage: log <thread_id>");
            }
        }
        "debug" if depth == 3 => {
            if let Some(id) = args.first() {
                handle_debug(state, app, id).await;
            } else {
                println!("  Usage: debug <thread_id>");
            }
        }
        _ => {
            println!("  Unknown command '{cmd}'. Type 'help' for available commands.");
        }
    }
}

fn print_help(depth: usize) {
    println!();
    println!("  Available commands:");
    println!("    ls             List items at current scope");
    println!("    cd [name]      Navigate into scope (fuzzy select if no name)");
    println!("    ..             Navigate up one level");
    if depth == 1 {
        println!("    account        Show tenant account summary");
        println!("    ledger [N]     Show last N ledger entries (default 20)");
    }
    if depth == 3 {
        println!("    threads        List threads in current project");
        println!("    thread <id>    Show thread summary");
        println!("    log <id>       Show raw thread run log");
        println!("    debug <id>     Start interactive LLM debug session");
    }
    println!("    help           Show this help");
    println!("    quit           Exit");
    println!();
}

async fn handle_ls(state: &mut ShellState) {
    state.refresh_children().await;
    let label = match state.depth() {
        0 => "Tenants",
        1 => "Workspaces",
        2 => "Projects",
        _ => "Items",
    };
    let children = state.children();
    display::print_children(&children, label);
}

async fn handle_cd(args: &[&str], state: &mut ShellState) {
    let mut children = state.children();
    if children.is_empty() {
        state.refresh_children().await;
        children = state.children();
    }

    let name = if let Some(arg) = args.first() {
        arg.to_string()
    } else if children.is_empty() {
        println!("  No items to navigate into.");
        return;
    } else if children.len() == 1 {
        children[0].clone()
    } else {
        let selection = dialoguer::FuzzySelect::new()
            .with_prompt("Select")
            .items(&children)
            .default(0)
            .interact_opt();
        match selection {
            Ok(Some(idx)) => children[idx].clone(),
            _ => return,
        }
    };

    state.cd_into(name);
    state.refresh_children().await;
}

async fn handle_account(state: &ShellState, app: &AppCtx) {
    let tenant = match state.tenant() {
        Some(t) => t,
        None => {
            println!("  Navigate to a tenant first.");
            return;
        }
    };

    let profile = match accounting::get_profile(&app.ddb_client, &app.ddb_table, tenant).await {
        Ok(p) => p,
        Err(e) => {
            println!("  Error: {e}");
            return;
        }
    };
    let balance = match accounting::get_balance(&app.ddb_client, &app.ddb_table, tenant).await {
        Ok(b) => b,
        Err(e) => {
            println!("  Error: {e}");
            return;
        }
    };
    let costs =
        match accounting::get_daily_costs_est(&app.ddb_client, &app.ddb_table, tenant, 7).await {
            Ok(c) => c,
            Err(e) => {
                println!("  Error loading daily costs: {e}");
                Vec::new()
            }
        };

    display::print_account_summary(&profile, &balance, &costs);
}

async fn handle_ledger(state: &ShellState, app: &AppCtx, limit: i32) {
    let tenant = match state.tenant() {
        Some(t) => t,
        None => {
            println!("  Navigate to a tenant first.");
            return;
        }
    };

    match accounting::get_ledger(&app.ddb_client, &app.ddb_table, tenant, limit).await {
        Ok(entries) => display::print_ledger(&entries),
        Err(e) => println!("  Error: {e}"),
    }
}

async fn handle_threads(state: &ShellState, app: &AppCtx) {
    let scope = match state.request_scope() {
        Some(s) => s,
        None => {
            println!("  Navigate to a project first (tenant/workspace/project).");
            return;
        }
    };

    let keyspace = keyspace_for_scope();
    let store = ThreadStore::new(app.storage.clone(), scope, keyspace);
    let all = store.list().await;
    let root_threads: Vec<String> = all
        .into_iter()
        .filter(|id| !id.contains('/') && !id.contains("__"))
        .collect();
    display::print_thread_list(&root_threads);
}

async fn handle_thread(state: &ShellState, app: &AppCtx, thread_id: &str) {
    let scope = match state.request_scope() {
        Some(s) => s,
        None => {
            println!("  Navigate to a project first.");
            return;
        }
    };

    let keyspace = keyspace_for_scope();
    let store = ThreadStore::new(app.storage.clone(), scope, keyspace);
    match store.get(thread_id).await {
        Ok(log) => {
            let summary = analysis::summarize(&log);
            display::print_thread_summary(thread_id, &summary);
        }
        Err(e) => println!("  Error: {e}"),
    }
}

async fn handle_log(state: &ShellState, app: &AppCtx, thread_id: &str) {
    let scope = match state.request_scope() {
        Some(s) => s,
        None => {
            println!("  Navigate to a project first.");
            return;
        }
    };

    let keyspace = keyspace_for_scope();
    let key = match keyspace.thread_log_key(&scope, thread_id) {
        Ok(k) => k,
        Err(e) => {
            println!("  Error: {e}");
            return;
        }
    };

    match app.storage.get_bytes(&key).await {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            println!("{text}");
        }
        Err(e) => println!("  Error reading log: {e}"),
    }
}

async fn handle_debug(state: &ShellState, app: &AppCtx, thread_id: &str) {
    let scope = match state.request_scope() {
        Some(s) => s,
        None => {
            println!("  Navigate to a project first.");
            return;
        }
    };

    let keyspace = keyspace_for_scope();
    let ro_storage: Arc<dyn StorageAdapter> = Arc::new(ReadOnlyStorage(app.storage.clone()));
    let mut ctx = SuiteCtx::new(
        ro_storage,
        Arc::new(NullSecretsProvider),
        app.llm.clone(),
        scope,
        keyspace,
    );
    debug::wire_debug_capabilities(&mut ctx);

    println!("  Starting debug session for thread {thread_id}...");
    println!("  Type '/quit' to exit.\n");

    if let Err(e) = debug::run_debug(thread_id, &app.suite_debugger, &ctx).await {
        println!("  Debug error: {e}");
    }
}
