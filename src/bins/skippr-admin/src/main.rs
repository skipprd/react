use std::sync::Arc;

use clap::Parser;
use react_module_storage_s3::S3StorageAdapter;
use react_suite_debugger::SuiteDebugger;

mod accounting;
mod commands;
mod debug;
mod display;
mod nav;

use react_core::keyspace::{DefaultKeyspace, Keyspace};

pub fn keyspace_for_scope() -> Arc<dyn Keyspace> {
    Arc::new(DefaultKeyspace::new(String::new()))
}

#[derive(Parser)]
#[command(name = "skippr-admin", about = "Skippr admin CLI")]
struct Cli {
    #[arg(long, env = "SKIPPR_BUCKET")]
    bucket: String,

    #[arg(long, env = "SKIPPR_ACCOUNTING_TABLE", default_value = "skippr-accounting")]
    table: String,
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    let storage = Arc::new(S3StorageAdapter::from_env(cli.bucket).await);

    let aws_cfg = aws_config::defaults(aws_config::BehaviorVersion::latest())
        .load()
        .await;
    let ddb_client = aws_sdk_dynamodb::Client::new(&aws_cfg);

    let llm_cfg = react::llm::LlmConfig::default();
    let llm = react::llm::create_llm(&llm_cfg);

    let app = commands::AppCtx {
        ddb_client,
        ddb_table: cli.table,
        storage: storage.clone(),
        llm,
        suite_debugger: SuiteDebugger,
    };

    let (mut state, shared) = nav::ShellState::new(storage);
    state.refresh_children().await;

    let helper = nav::ShellHelper::new(shared);
    let config = rustyline::Config::builder()
        .auto_add_history(true)
        .build();
    let mut rl = rustyline::Editor::with_config(config).expect("failed to create readline editor");
    rl.set_helper(Some(helper));

    println!("skippr-admin — type 'help' for commands, 'quit' to exit.\n");

    loop {
        let prompt = state.prompt();
        match rl.readline(&prompt) {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                commands::dispatch(trimmed, &mut state, &app).await;
            }
            Err(
                rustyline::error::ReadlineError::Interrupted
                | rustyline::error::ReadlineError::Eof,
            ) => {
                println!("Bye.");
                break;
            }
            Err(e) => {
                eprintln!("Error: {e}");
                break;
            }
        }
    }
}
