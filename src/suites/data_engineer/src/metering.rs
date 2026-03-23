use once_cell::sync::OnceCell;
use serde::Serialize;

static METERING_CLIENT: OnceCell<MeteringClient> = OnceCell::new();

pub fn init_metering(accounting_url: Option<String>, auth_token: Option<String>) {
    let _ = METERING_CLIENT.set(MeteringClient::new(accounting_url, auth_token));
}

pub fn global_metering() -> &'static MeteringClient {
    static NOOP: once_cell::sync::Lazy<MeteringClient> = once_cell::sync::Lazy::new(MeteringClient::noop);
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
        }
    }

    pub fn event_type(&self) -> &str {
        match self {
            Self::FieldsDiscovered { .. } => "fields_discovered",
            Self::TablesSynced { .. } => "tables_synced",
            Self::ModelsAuthored { .. } => "models_authored",
            Self::PlanApproved { .. } => "plan_approved",
            Self::RepairCycle { .. } => "repair_cycle",
            Self::PipelineRun { .. } => "pipeline_run",
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
        }
    }
}

pub struct MeteringClient {
    accounting_url: Option<String>,
    auth_token: Option<String>,
    http: Option<reqwest::Client>,
}

impl MeteringClient {
    pub fn new(accounting_url: Option<String>, auth_token: Option<String>) -> Self {
        let http = accounting_url.as_ref().map(|_| reqwest::Client::new());
        Self { accounting_url, auth_token, http }
    }

    pub fn noop() -> Self {
        Self { accounting_url: None, auth_token: None, http: None }
    }

    pub fn is_enabled(&self) -> bool {
        self.accounting_url.is_some()
    }

    pub async fn record_batch(&self, events: &[UsageEvent]) {
        let Some(base_url) = &self.accounting_url else { return };
        let Some(client) = &self.http else { return };
        let record_url = format!("{}/usage/record", base_url);

        for event in events {
            let payload = serde_json::json!({
                "user_id": "",
                "event_type": event.event_type(),
                "quantity": event.credits(),
                "unit": "credits",
                "metadata": serde_json::to_value(event).ok(),
                "project_id": event.project_id(),
                "phase": event.event_type(),
            });

            let mut req = client.post(&record_url).json(&payload);
            if let Some(token) = &self.auth_token {
                req = req.header("Authorization", format!("Bearer {}", token));
            }

            if let Err(e) = req.send().await {
                tracing::warn!(event_type = event.event_type(), error = ?e, "Failed to record usage event");
            }
        }
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
        ];
        let credits: Vec<f64> = events.iter().map(|e| e.credits()).collect();
        assert!((credits[0] - 20.0).abs() < f64::EPSILON);
        assert!((credits[1] - 50.0).abs() < f64::EPSILON);
        assert!((credits[2] - 95.0).abs() < f64::EPSILON);
        assert!((credits[3] - 0.0).abs() < f64::EPSILON);
        assert!((credits[4] - 0.0).abs() < f64::EPSILON);
        assert!((credits[5] - 3.0).abs() < f64::EPSILON);
    }
}
