use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};
use crate::keyspace::Keyspace;
use crate::scope::RequestScope;
use crate::storage::StorageAdapter;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadFeedbackVerdict {
    Good,
    Bad,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadFeedback {
    pub feedback_id: String,
    pub thread_id: String,
    pub verdict: ThreadFeedbackVerdict,
    pub comment: String,
    pub created_at: String,
    pub resolved: bool,
    pub resolved_at: Option<String>,
}

pub struct ThreadFeedbackStore {
    storage: Arc<dyn StorageAdapter>,
    scope: RequestScope,
    keyspace: Arc<dyn Keyspace>,
}

impl ThreadFeedbackStore {
    pub fn new(
        storage: Arc<dyn StorageAdapter>,
        scope: RequestScope,
        keyspace: Arc<dyn Keyspace>,
    ) -> Self {
        Self {
            storage,
            scope,
            keyspace,
        }
    }

    pub async fn submit(
        &self,
        thread_id: &str,
        verdict: ThreadFeedbackVerdict,
        comment: impl Into<String>,
    ) -> CoreResult<ThreadFeedback> {
        let comment = normalize_comment(comment.into())?;
        ensure_thread_exists(self.storage.as_ref(), self.keyspace.as_ref(), &self.scope, thread_id)
            .await?;
        let feedback = ThreadFeedback {
            feedback_id: uuid::Uuid::new_v4().to_string(),
            thread_id: thread_id.to_string(),
            verdict,
            comment,
            created_at: Utc::now().to_rfc3339(),
            resolved: false,
            resolved_at: None,
        };
        self.write(&feedback).await?;
        Ok(feedback)
    }

    pub async fn list_all(&self) -> CoreResult<Vec<ThreadFeedback>> {
        let prefix = self.keyspace.feedback_prefix(&self.scope);
        let mut feedback = self.load_prefix(&prefix).await?;
        feedback.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(feedback)
    }

    pub async fn list_for_thread(&self, thread_id: &str) -> CoreResult<Vec<ThreadFeedback>> {
        let prefix = self.keyspace.thread_feedback_prefix(&self.scope, thread_id)?;
        let mut feedback = self.load_prefix(&prefix).await?;
        feedback.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(feedback)
    }

    pub async fn mark_resolved(&self, feedback_id: &str) -> CoreResult<Option<ThreadFeedback>> {
        let mut feedback = match self.get_by_id(feedback_id).await? {
            Some(feedback) => feedback,
            None => return Ok(None),
        };
        if !feedback.resolved {
            feedback.resolved = true;
            feedback.resolved_at = Some(Utc::now().to_rfc3339());
            self.write(&feedback).await?;
        }
        Ok(Some(feedback))
    }

    pub async fn get_by_id(&self, feedback_id: &str) -> CoreResult<Option<ThreadFeedback>> {
        let prefix = self.keyspace.feedback_prefix(&self.scope);
        let entries = self.load_prefix(&prefix).await?;
        Ok(entries
            .into_iter()
            .find(|feedback| feedback.feedback_id == feedback_id))
    }

    async fn load_prefix(&self, prefix: &str) -> CoreResult<Vec<ThreadFeedback>> {
        let keys = self.storage.list_prefix(prefix).await?;
        let mut out = Vec::new();
        for key in keys {
            if !key.ends_with(".json") {
                continue;
            }
            let value = self.storage.get_json(&key).await?;
            let feedback: ThreadFeedback = serde_json::from_value(value)?;
            out.push(feedback);
        }
        Ok(out)
    }

    async fn write(&self, feedback: &ThreadFeedback) -> CoreResult<()> {
        let key = self.keyspace.thread_feedback_key(
            &self.scope,
            &feedback.thread_id,
            &feedback.feedback_id,
        )?;
        let value = serde_json::to_value(feedback)?;
        self.storage.put_json(&key, &value).await
    }
}

fn normalize_comment(comment: String) -> CoreResult<String> {
    let trimmed = comment.trim();
    if trimmed.is_empty() {
        return Err(CoreError::generic("feedback comment cannot be empty"));
    }
    Ok(trimmed.to_string())
}

async fn ensure_thread_exists(
    storage: &dyn StorageAdapter,
    keyspace: &dyn Keyspace,
    scope: &RequestScope,
    thread_id: &str,
) -> CoreResult<()> {
    let thread_key = keyspace.thread_key(scope, thread_id)?;
    match storage.head_etag(&thread_key).await? {
        Some(_) => Ok(()),
        None => Err(CoreError::generic(format!(
            "thread '{thread_id}' was not found in project storage"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::keyspace::DefaultKeyspace;
    use crate::scope::RequestScope;
    use crate::storage::StorageAdapter;
    use crate::test_support::InMemoryStorageAdapter;

    #[tokio::test]
    async fn submit_lists_and_resolves_feedback() {
        let storage = Arc::new(InMemoryStorageAdapter::default()) as Arc<dyn StorageAdapter>;
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string())) as Arc<dyn Keyspace>;
        let scope = RequestScope::parse("t", "w", "p").unwrap();
        storage
            .put_json(
                &keyspace.thread_key(&scope, "thread-1").unwrap(),
                &serde_json::json!({}),
            )
            .await
            .unwrap();
        let store = ThreadFeedbackStore::new(storage.clone(), scope.clone(), keyspace.clone());

        let submitted = store
            .submit("thread-1", ThreadFeedbackVerdict::Bad, "Needs a schema retry")
            .await
            .unwrap();
        assert_eq!(submitted.thread_id, "thread-1");
        assert!(!submitted.resolved);

        let listed = store.list_for_thread("thread-1").await.unwrap();
        assert_eq!(listed, vec![submitted.clone()]);

        let resolved = store
            .mark_resolved(&submitted.feedback_id)
            .await
            .unwrap()
            .unwrap();
        assert!(resolved.resolved);
        assert!(resolved.resolved_at.is_some());

        let all_feedback = store.list_all().await.unwrap();
        assert_eq!(all_feedback.len(), 1);
        assert_eq!(all_feedback[0].feedback_id, submitted.feedback_id);
        assert!(all_feedback[0].resolved);
    }

    #[tokio::test]
    async fn submit_requires_existing_thread() {
        let storage = Arc::new(InMemoryStorageAdapter::default()) as Arc<dyn StorageAdapter>;
        let keyspace = Arc::new(DefaultKeyspace::new("b".to_string())) as Arc<dyn Keyspace>;
        let scope = RequestScope::parse("t", "w", "p").unwrap();
        let store = ThreadFeedbackStore::new(storage, scope, keyspace);

        let err = store
            .submit("missing-thread", ThreadFeedbackVerdict::Good, "looks good")
            .await
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("thread 'missing-thread' was not found"));
    }
}
