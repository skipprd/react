use async_trait::async_trait;
use react_core::provider_traits::secrets::SecretsProvider;

/// Env-backed secrets provider used by the generic runtime bootstrap.
#[derive(Clone, Default)]
pub struct EnvSecretsProvider;

#[async_trait]
impl SecretsProvider for EnvSecretsProvider {
    async fn get_secret(&self, name: &str) -> Result<Option<String>, String> {
        Ok(std::env::var(name).ok())
    }
}
