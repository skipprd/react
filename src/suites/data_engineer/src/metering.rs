use once_cell::sync::OnceCell;
use serde::Serialize;
use std::sync::Mutex;

const LOW_CREDIT_THRESHOLD: f64 = 20.0;

static METERING_CLIENT: OnceCell<MeteringClient> = OnceCell::new();

pub fn init_metering(
    accounting_url: Option<String>,
    auth_token: Option<String>,
    initial_credit_balance: f64,
) {
    let _ = METERING_CLIENT.set(MeteringClient::new(
        accounting_url,
        auth_token,
        initial_credit_balance,
    ));
}

pub fn report_llm_usage(input_tokens: u64, output_tokens: u64, model: String) {
    let event = UsageEvent::LlmRequest { input_tokens, output_tokens, model };
    let client = global_metering();
    if !client.is_enabled() {
        return;
    }
    client.budget.deduct(event.credits());
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        handle.spawn(async move {
            let client = global_metering();
            let _ = client.record_batch(&[event]).await;
        });
    }
}

/// Sync check of the in-memory credit budget. Returns Err if exhausted.
pub fn check_credit_budget() -> Result<(), String> {
    global_metering().budget.check_local()
}

pub fn global_metering() -> &'static MeteringClient {
    static NOOP: once_cell::sync::Lazy<MeteringClient> =
        once_cell::sync::Lazy::new(MeteringClient::noop);
    METERING_CLIENT.get().unwrap_or(&NOOP)
}

#[derive(Clone, Debug, Serialize)]
pub enum UsageEvent {
    FieldsDiscovered { count: u64, project_id: String },
    TablesSynced { count: u64, project_id: String },
    ModelsAuthored { silver: u64, gold: u64, project_id: String },
    PlanApproved { tasks: u64, batches: u64, project_id: String },
    RepairCycle { cycle: u64, project_id: String },
    PipelineRun { project_id: String },
    LlmRequest { input_tokens: u64, output_tokens: u64, model: String },
}

impl UsageEvent {
    pub fn credits(&self) -> f64 {
        match self {
            Self::FieldsDiscovered { count, .. } => *count as f64 * 0.2,
            Self::TablesSynced { count, .. } => *count as f64 * 5.0,
            Self::ModelsAuthored { silver, gold, .. } => {
                *silver as f64 * 10.0 + *gold as f64 * 15.0
            }
            Self::PlanApproved { .. } => 0.0,
            Self::RepairCycle { .. } => 0.0,
            Self::PipelineRun { .. } => 3.0,
            Self::LlmRequest { input_tokens, output_tokens, .. } => {
                (*input_tokens + *output_tokens) as f64 * 0.001
            }
        }
    }

    /// Returns server-compatible (event_type, raw_quantity) tuples.
    /// Compound events are expanded into multiple records that match
    /// the server's `credits_for_event` vocabulary.
    pub fn to_server_records(&self) -> Vec<(&str, f64)> {
        match self {
            Self::FieldsDiscovered { count, .. } => vec![("fields_discovered", *count as f64)],
            Self::TablesSynced { count, .. } => vec![("tables_synced", *count as f64)],
            Self::ModelsAuthored { silver, gold, .. } => {
                let mut v = Vec::new();
                if *silver > 0 {
                    v.push(("models_authored_silver", *silver as f64));
                }
                if *gold > 0 {
                    v.push(("models_authored_gold", *gold as f64));
                }
                v
            }
            Self::LlmRequest { input_tokens, output_tokens, .. } => vec![
                ("llm_input_tokens", *input_tokens as f64),
                ("llm_output_tokens", *output_tokens as f64),
            ],
            Self::PipelineRun { .. } => vec![("pipeline_run", 1.0)],
            Self::PlanApproved { tasks, .. } => vec![("plan_approved", *tasks as f64)],
            Self::RepairCycle { cycle, .. } => vec![("repair_cycle", *cycle as f64)],
        }
    }

    pub fn project_id(&self) -> &str {
        match self {
            Self::FieldsDiscovered { project_id, .. }
            | Self::TablesSynced { project_id, .. }
            | Self::ModelsAuthored { project_id, .. }
            | Self::PlanApproved { project_id, .. }
            | Self::RepairCycle { project_id, .. }
            | Self::PipelineRun { project_id, .. } => project_id,
            Self::LlmRequest { .. } => "",
        }
    }
}

// ---------------------------------------------------------------------------
// CreditBudget — in-memory tracker with API-backed verification
// ---------------------------------------------------------------------------

pub struct CreditBudget {
    estimate: Mutex<f64>,
    accounting_url: Option<String>,
    auth_token: Option<String>,
    http: Option<reqwest::Client>,
}

impl CreditBudget {
    pub fn new(
        initial: f64,
        accounting_url: Option<String>,
        auth_token: Option<String>,
    ) -> Self {
        let http = accounting_url.as_ref().map(|_| reqwest::Client::new());
        Self {
            estimate: Mutex::new(initial),
            accounting_url,
            auth_token,
            http,
        }
    }

    fn noop() -> Self {
        Self {
            estimate: Mutex::new(f64::MAX),
            accounting_url: None,
            auth_token: None,
            http: None,
        }
    }

    pub fn deduct(&self, credits: f64) {
        let mut est = self.estimate.lock().unwrap();
        *est -= credits;
    }

    pub fn local_estimate(&self) -> f64 {
        *self.estimate.lock().unwrap()
    }

    /// Sync check: returns Err if local estimate says we're out of credits.
    pub fn check_local(&self) -> Result<(), String> {
        if self.local_estimate() <= 0.0 {
            Err("credit budget exhausted".to_string())
        } else {
            Ok(())
        }
    }

    /// Async check: if local estimate is low, verify with the API.
    /// Returns Err if truly out of credits.
    pub async fn check_and_refresh(&self) -> Result<f64, String> {
        let est = self.local_estimate();
        if est > LOW_CREDIT_THRESHOLD {
            return Ok(est);
        }
        let real = self.fetch_remote_balance().await?;
        *self.estimate.lock().unwrap() = real;
        if real <= 0.0 {
            Err(format!(
                "No credits remaining ({:.1}). Purchase credits to continue.",
                real
            ))
        } else {
            Ok(real)
        }
    }

    async fn fetch_remote_balance(&self) -> Result<f64, String> {
        let Some(base_url) = &self.accounting_url else {
            return Ok(f64::MAX);
        };
        let Some(client) = &self.http else {
            return Ok(f64::MAX);
        };
        let url = format!("{}/usage/check", base_url);
        let mut req = client.get(&url);
        if let Some(token) = &self.auth_token {
            req = req.header("Authorization", format!("Bearer {}", token));
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("credit check failed: {e}"))?;
        let data: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("credit check parse: {e}"))?;
        Ok(data["credits_remaining"].as_f64().unwrap_or(0.0))
    }
}

// ---------------------------------------------------------------------------
// MeteringClient
// ---------------------------------------------------------------------------

pub struct MeteringClient {
    accounting_url: Option<String>,
    auth_token: Option<String>,
    http: Option<reqwest::Client>,
    pub budget: CreditBudget,
}

impl MeteringClient {
    pub fn new(
        accounting_url: Option<String>,
        auth_token: Option<String>,
        initial_balance: f64,
    ) -> Self {
        let http = accounting_url.as_ref().map(|_| reqwest::Client::new());
        let budget = CreditBudget::new(
            initial_balance,
            accounting_url.clone(),
            auth_token.clone(),
        );
        Self { accounting_url, auth_token, http, budget }
    }

    pub fn noop() -> Self {
        Self {
            accounting_url: None,
            auth_token: None,
            http: None,
            budget: CreditBudget::noop(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.accounting_url.is_some()
    }

    /// Posts usage events to the accounting API and deducts from the local
    /// credit budget. Returns Err if the budget is exhausted after deductions.
    pub async fn record_batch(&self, events: &[UsageEvent]) -> Result<(), String> {
        let Some(base_url) = &self.accounting_url else {
            return Ok(());
        };
        let Some(client) = &self.http else {
            return Ok(());
        };
        let record_url = format!("{}/usage/record", base_url);

        for event in events {
            self.budget.deduct(event.credits());

            for (event_type, quantity) in event.to_server_records() {
                let payload = serde_json::json!({
                    "user_id": "",
                    "event_type": event_type,
                    "quantity": quantity,
                    "unit": event_type,
                    "metadata": serde_json::to_value(event).ok(),
                    "project_id": event.project_id(),
                    "phase": event_type,
                });

                let mut req = client.post(&record_url).json(&payload);
                if let Some(token) = &self.auth_token {
                    req = req.header("Authorization", format!("Bearer {}", token));
                }

                if let Err(e) = req.send().await {
                    tracing::warn!(event_type = event_type, error = ?e, "Failed to record usage event");
                }
            }
        }

        self.budget.check_and_refresh().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn credits_no_wildcard_arm() {
        let events = vec![
            UsageEvent::FieldsDiscovered { count: 100, project_id: "test".into() },
            UsageEvent::TablesSynced { count: 10, project_id: "test".into() },
            UsageEvent::ModelsAuthored { silver: 5, gold: 3, project_id: "test".into() },
            UsageEvent::PlanApproved { tasks: 2, batches: 1, project_id: "test".into() },
            UsageEvent::RepairCycle { cycle: 1, project_id: "test".into() },
            UsageEvent::PipelineRun { project_id: "test".into() },
            UsageEvent::LlmRequest { input_tokens: 1000, output_tokens: 500, model: "gpt-4o".into() },
        ];
        let credits: Vec<f64> = events.iter().map(|e| e.credits()).collect();
        assert!((credits[0] - 20.0).abs() < f64::EPSILON);
        assert!((credits[1] - 50.0).abs() < f64::EPSILON);
        assert!((credits[2] - 95.0).abs() < f64::EPSILON);
        assert!((credits[3] - 0.0).abs() < f64::EPSILON);
        assert!((credits[4] - 0.0).abs() < f64::EPSILON);
        assert!((credits[5] - 3.0).abs() < f64::EPSILON);
        assert!((credits[6] - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn to_server_records_expands_compound_events() {
        let ev = UsageEvent::ModelsAuthored { silver: 3, gold: 2, project_id: "p".into() };
        let records = ev.to_server_records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0], ("models_authored_silver", 3.0));
        assert_eq!(records[1], ("models_authored_gold", 2.0));

        let ev = UsageEvent::LlmRequest { input_tokens: 5000, output_tokens: 1000, model: "m".into() };
        let records = ev.to_server_records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0], ("llm_input_tokens", 5000.0));
        assert_eq!(records[1], ("llm_output_tokens", 1000.0));
    }

    #[test]
    fn to_server_records_simple_events() {
        let ev = UsageEvent::TablesSynced { count: 10, project_id: "p".into() };
        assert_eq!(ev.to_server_records(), vec![("tables_synced", 10.0)]);

        let ev = UsageEvent::FieldsDiscovered { count: 50, project_id: "p".into() };
        assert_eq!(ev.to_server_records(), vec![("fields_discovered", 50.0)]);

        let ev = UsageEvent::PipelineRun { project_id: "p".into() };
        assert_eq!(ev.to_server_records(), vec![("pipeline_run", 1.0)]);
    }

    #[test]
    fn credit_budget_deduct_and_check() {
        let budget = CreditBudget::new(100.0, None, None);
        assert!(budget.check_local().is_ok());
        assert!((budget.local_estimate() - 100.0).abs() < f64::EPSILON);

        budget.deduct(80.0);
        assert!(budget.check_local().is_ok());
        assert!((budget.local_estimate() - 20.0).abs() < f64::EPSILON);

        budget.deduct(25.0);
        assert!(budget.check_local().is_err());
        assert!(budget.local_estimate() < 0.0);
    }

    #[test]
    fn models_authored_zero_silver_omitted() {
        let ev = UsageEvent::ModelsAuthored { silver: 0, gold: 5, project_id: "p".into() };
        let records = ev.to_server_records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0], ("models_authored_gold", 5.0));
    }
}
