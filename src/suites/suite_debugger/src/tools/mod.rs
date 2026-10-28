pub mod admin_repo_query;
pub mod list_threads;
pub mod read_log;
pub mod read_thread;
pub mod summarize;

use react_core::tools::{DuplicateTool, ToolRegistry};

pub fn register_all(
    registry: &mut ToolRegistry,
    enable_admin_repo_query: bool,
) -> Result<(), DuplicateTool> {
    registry.register(list_threads::ListThreadsTool)?;
    registry.register(read_thread::ReadThreadTool)?;
    registry.register(read_log::ReadRunLogTool)?;
    registry.register(summarize::SummarizeThreadTool)?;
    if enable_admin_repo_query {
        registry.register(admin_repo_query::AdminRepoQueryTool)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admin_repo_query_only_registers_when_enabled() {
        let mut base = ToolRegistry::new();
        register_all(&mut base, false).expect("unique");
        assert!(!base.contains("admin_repo_query"));

        let mut admin = ToolRegistry::new();
        register_all(&mut admin, true).expect("unique");
        assert!(admin.contains("admin_repo_query"));
    }

    #[test]
    fn tool_card_comes_from_registry() {
        let mut reg = ToolRegistry::new();
        register_all(&mut reg, false).expect("unique");
        let card = reg.tool_card();
        assert!(card.contains("list_threads"));
        assert!(card.contains("read_thread"));
        assert!(!card.contains("admin_repo_query"));
    }
}
