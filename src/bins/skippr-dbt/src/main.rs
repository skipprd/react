#[tokio::main]
async fn main() {
    let mut reg = react_core::suite::SuiteRegistry::new();
    reg.register(react_suite_data_engineer::DataEngineerSuite);
    react::cli::run(reg).await;
}
