use crate::auth::StoredCredentials;
use serde::Deserialize;

pub struct ApiClient {
    base_url: String,
    http: reqwest::Client,
}

#[derive(Debug, Deserialize)]
pub struct AccountResponse {
    pub profile: AccountProfile,
    pub balance: Balance,
    pub recent_usage: Vec<UsageRecord>,
    pub subscription: Option<Subscription>,
}

#[derive(Debug, Deserialize)]
pub struct Subscription {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub price_id: String,
}

#[derive(Debug, Deserialize)]
pub struct AccountProfile {
    pub plan: String,
}

#[derive(Debug, Deserialize)]
pub struct Balance {
    pub credits_remaining: f64,
    #[serde(default)]
    pub credits_purchased: f64,
    #[serde(default)]
    pub credits_used: f64,
    #[serde(default)]
    pub period: String,
}

#[derive(Debug, Deserialize)]
pub struct UsageRecord {
    pub timestamp: String,
    pub event_type: String,
    pub credits_charged: f64,
    #[serde(default)]
    pub quantity: f64,
    pub project_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SignInResponse {
    pub message: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConfirmResponse {
    pub token: Option<String>,
    pub refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BuyCreditsResponse {
    pub checkout_url: String,
}

impl ApiClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub async fn sign_in(&self, email: &str) -> Result<(), String> {
        let url = format!("{}/auth/sign-in", self.base_url);
        let body = serde_json::json!({ "email": email });
        let resp = self.http.post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Sign-in failed: {}", text));
        }
        Ok(())
    }

    pub async fn confirm(&self, email: &str, code: &str) -> Result<StoredCredentials, String> {
        let url = format!("{}/auth/confirm", self.base_url);
        let body = serde_json::json!({ "email": email, "code": code });
        let resp = self.http.post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Confirm failed: {}", text));
        }

        let data: ConfirmResponse = resp.json().await
            .map_err(|e| format!("Parse error: {}", e))?;

        Ok(StoredCredentials {
            access_token: data.token.unwrap_or_default(),
            refresh_token: data.refresh_token.unwrap_or_default(),
        })
    }

    pub async fn get_account(&self, token: &str) -> Result<AccountResponse, String> {
        let url = format!("{}/account", self.base_url);
        let resp = self.http.get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Account fetch failed: {}", text));
        }

        resp.json().await.map_err(|e| format!("Parse error: {}", e))
    }

    pub async fn buy_credits(&self, token: &str, pack: &str) -> Result<String, String> {
        let url = format!("{}/account/buy-credits", self.base_url);
        let body = serde_json::json!({ "credit_pack": pack });
        let resp = self.http.post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Buy credits failed: {}", text));
        }

        let data: BuyCreditsResponse = resp.json().await
            .map_err(|e| format!("Parse error: {}", e))?;
        Ok(data.checkout_url)
    }

    pub async fn exchange_api_key(&self, api_key: &str) -> Result<StoredCredentials, String> {
        let url = format!("{}/auth/api-key-exchange", self.base_url);
        let resp = self.http.post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("API key exchange failed: {}", text));
        }

        let data: TokenExchangeResponse = resp.json().await
            .map_err(|e| format!("Parse error: {}", e))?;

        Ok(StoredCredentials {
            access_token: data.token,
            refresh_token: data.refresh_token,
        })
    }

    pub async fn create_api_key(&self, token: &str, name: &str) -> Result<CreateApiKeyResponse, String> {
        let url = format!("{}/auth/api-keys", self.base_url);
        let body = serde_json::json!({ "name": name });
        let resp = self.http.post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Create API key failed: {}", text));
        }

        resp.json().await.map_err(|e| format!("Parse error: {}", e))
    }

    pub async fn list_api_keys(&self, token: &str) -> Result<Vec<ApiKeyInfo>, String> {
        let url = format!("{}/auth/api-keys", self.base_url);
        let resp = self.http.get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("List API keys failed: {}", text));
        }

        resp.json().await.map_err(|e| format!("Parse error: {}", e))
    }

    pub async fn revoke_api_key(&self, token: &str, key_id: &str) -> Result<(), String> {
        let url = format!("{}/auth/api-keys/{}", self.base_url, key_id);
        let resp = self.http.delete(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Revoke API key failed: {}", text));
        }

        Ok(())
    }

    pub async fn get_credentials(&self, token: &str) -> Result<CredentialsResponse, String> {
        let url = format!("{}/auth/credentials", self.base_url);
        let resp = self.http.post(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| format!("Network error: {}", e))?;

        if !resp.status().is_success() {
            let text = resp.text().await.unwrap_or_default();
            return Err(format!("Credentials fetch failed: {}", text));
        }

        resp.json().await.map_err(|e| format!("Parse error: {}", e))
    }
}

#[derive(Debug, Deserialize)]
pub struct CredentialsResponse {
    pub credentials: StsCreds,
    pub bucket: String,
    pub key_prefix: String,
    #[serde(default)]
    pub tenant_id: String,
    pub llm_api_key: String,
    pub accounting_url: String,
}

#[derive(Debug, Deserialize)]
pub struct StsCreds {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: String,
    pub expiration: String,
}

#[derive(Debug, Deserialize)]
struct TokenExchangeResponse {
    pub token: String,
    pub refresh_token: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateApiKeyResponse {
    pub key_id: String,
    pub raw_key: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct ApiKeyInfo {
    pub key_id: String,
    pub name: String,
    pub created_at: String,
    pub status: String,
}
