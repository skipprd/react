#[tokio::main]
async fn main() {
    let mut reg = react_core::suite::SuiteRegistry::new();
    reg.register(react_suite_data_engineer::DataEngineerSuite);
    reg.register(react_suite_kb::KbSuite);
    reg.register(react_suite_debugger::SuiteDebugger);
    reg.register(react_suite_goggles_review::GogglesReviewSuite);
    react::cli::run(reg).await;
}
