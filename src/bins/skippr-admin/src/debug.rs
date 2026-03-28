use std::sync::Arc;

use react_core::suite::{DebugProviderRegistry, FlowFrame, Suite, SuiteCtx};
use react_suite_data_engineer::debug::DataEngineerDebugProvider;
use react_suite_debugger::SuiteDebugger;
use react_suite_kb::debug::KbDebugProvider;

pub fn build_debug_provider_registry() -> DebugProviderRegistry {
    let mut reg = DebugProviderRegistry::new();
    reg.register(DataEngineerDebugProvider);
    reg.register(KbDebugProvider);
    reg
}

pub async fn run_debug(
    thread_id: &str,
    suite: &SuiteDebugger,
    ctx: &SuiteCtx,
) -> Result<(), String> {
    let debug_tid = uuid::Uuid::new_v4().to_string();
    let question = format!("Analyze thread {thread_id}");

    let frames = suite
        .handle_new(&debug_tid, &question, "suite_debugger", ctx)
        .await?;
    display_frames(&frames);

    let mut rl = rustyline::DefaultEditor::new().map_err(|e| e.to_string())?;
    loop {
        let readline = rl.readline("debug> ");
        match readline {
            Ok(line) => {
                let input = line.trim();
                if input.is_empty() {
                    continue;
                }
                if input == "/quit" || input == "quit" || input == "exit" {
                    break;
                }
                let _ = rl.add_history_entry(input);
                let frames = suite
                    .handle_user(&debug_tid, input, "suite_debugger", ctx)
                    .await?;
                display_frames(&frames);
            }
            Err(rustyline::error::ReadlineError::Interrupted | rustyline::error::ReadlineError::Eof) => {
                break;
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}

fn display_frames(frames: &[FlowFrame]) {
    for frame in frames {
        match frame {
            FlowFrame::Complete {
                display, payload, ..
            } => {
                if let Some(text) = display {
                    println!("\n{text}\n");
                } else if let Some(text) = payload.as_str() {
                    println!("\n{text}\n");
                } else {
                    println!(
                        "\n{}\n",
                        serde_json::to_string_pretty(payload).unwrap_or_default()
                    );
                }
            }
            FlowFrame::Interrupt { prompt, .. } => {
                println!("\n[Interrupt] {prompt}\n");
            }
            FlowFrame::Review { text, .. } => {
                println!("\n[Review] {text}\n");
            }
            FlowFrame::Checkpoint { display, .. } => {
                if let Some(text) = display {
                    println!("\n[Checkpoint] {text}\n");
                }
            }
        }
    }
}

pub fn wire_debug_capabilities(ctx: &mut SuiteCtx) {
    let registry = build_debug_provider_registry();
    ctx.set_capability(Arc::new(registry));
}
