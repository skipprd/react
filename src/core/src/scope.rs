use crate::error::CoreError;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TenantId(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceId(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectId(String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestScope {
    pub tenant: TenantId,
    pub workspace: WorkspaceId,
    pub project_id: ProjectId,
}

impl TenantId {
    pub fn parse(raw: impl Into<String>) -> Result<Self, CoreError> {
        let raw = raw.into();
        ensure_safe_scope_segment("tenant", &raw)?;
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for TenantId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for TenantId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<TenantId> for String {
    fn from(id: TenantId) -> Self {
        id.0
    }
}

impl WorkspaceId {
    pub fn parse(raw: impl Into<String>) -> Result<Self, CoreError> {
        let raw = raw.into();
        ensure_safe_scope_segment("workspace", &raw)?;
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for WorkspaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for WorkspaceId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<WorkspaceId> for String {
    fn from(id: WorkspaceId) -> Self {
        id.0
    }
}

impl ProjectId {
    pub fn parse(raw: impl Into<String>) -> Result<Self, CoreError> {
        let raw = raw.into();
        ensure_safe_scope_segment("project_id", &raw)?;
        Ok(Self(raw))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ProjectId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ProjectId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<ProjectId> for String {
    fn from(id: ProjectId) -> Self {
        id.0
    }
}

impl RequestScope {
    pub fn new(tenant: TenantId, workspace: WorkspaceId, project_id: ProjectId) -> Self {
        Self {
            tenant,
            workspace,
            project_id,
        }
    }

    pub fn parse(
        tenant: impl Into<String>,
        workspace: impl Into<String>,
        project_id: impl Into<String>,
    ) -> Result<Self, CoreError> {
        Ok(Self::new(
            TenantId::parse(tenant)?,
            WorkspaceId::parse(workspace)?,
            ProjectId::parse(project_id)?,
        ))
    }
}

pub fn ensure_safe_scope_segment(field: &str, value: &str) -> Result<(), CoreError> {
    if value.trim().is_empty() {
        return Err(CoreError::generic(format!(
            "invalid {field}: empty segment"
        )));
    }
    if value.contains("..") {
        return Err(CoreError::generic(format!(
            "invalid {field}: path traversal '..' is not allowed"
        )));
    }
    if value.contains('/') || value.contains('\\') {
        return Err(CoreError::generic(format!(
            "invalid {field}: path separators are not allowed in scope segments"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_scope_accepts_safe_segments() {
        let scope = RequestScope::parse("tenant1", "workspace_1", "project-1")
            .expect("safe segments should parse");
        assert_eq!(scope.tenant.as_str(), "tenant1");
        assert_eq!(scope.workspace.as_str(), "workspace_1");
        assert_eq!(scope.project_id.as_str(), "project-1");
    }

    #[test]
    fn parse_scope_rejects_path_segments() {
        let err =
            RequestScope::parse("tenant/../x", "w", "p").expect_err("unsafe scope should fail");
        assert!(err.to_string().contains("invalid tenant"));
    }
}
