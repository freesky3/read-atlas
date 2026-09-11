// Task 3 builds the dormant schema-7 job boundary before Task 8 atomically
// switches production commands and workers to it.
#![allow(dead_code)]

use crate::db;
use crate::library_workflow::{bump_library_revisions, LibraryDomain};
use crate::model_settings::ProviderKind;
use crate::provider_routing::{
    load_frozen_mistral_ocr_route, load_frozen_route, persist_frozen_mistral_ocr_route,
    persist_frozen_route, BoundProviderRoute, FrozenMistralOcrRoute, FrozenProviderRoute,
    ModelRole, ProviderRequirement, ProviderRequirementCode,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub type JobResult<T> = Result<T, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
    InterruptedUnknown,
}

impl JobState {
    fn from_database(value: &str) -> JobResult<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "paused" => Ok(Self::Paused),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "interrupted_unknown" => Ok(Self::InterruptedUnknown),
            _ => Err(format!("Unknown job state: {value}")),
        }
    }

    fn as_database(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::InterruptedUnknown => "interrupted_unknown",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobSpec {
    pub kind: String,
    pub provider: Option<String>,
    pub paper_id: Option<String>,
    pub revision_id: Option<String>,
    pub root_key: Option<String>,
    pub artifact_key: Option<String>,
    pub dedupe_key: String,
    pub priority: i64,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum JobExecutionRoute {
    Local,
    MistralOcr(FrozenMistralOcrRoute),
    Paper(FrozenProviderRoute),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StoredJobRoute {
    Executable(JobExecutionRoute),
    LegacyUnattributed,
    Unavailable { provider_kind: Option<ProviderKind> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JobRouteOrigin {
    Captured,
    LegacyUniqueVerified,
    ExplicitRebind,
}

impl JobRouteOrigin {
    fn as_database(self) -> &'static str {
        match self {
            Self::Captured => "captured",
            Self::LegacyUniqueVerified => "legacy_unique_verified",
            Self::ExplicitRebind => "explicit_rebind",
        }
    }

    fn from_database(value: &str) -> JobResult<Self> {
        match value {
            "captured" => Ok(Self::Captured),
            "legacy_unique_verified" => Ok(Self::LegacyUniqueVerified),
            "explicit_rebind" => Ok(Self::ExplicitRebind),
            _ => Err("Stored job route origin is invalid".to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProviderRouteStatus {
    Ready,
    Legacy,
    ActionRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderRouteModelsProjection {
    pub(crate) paper: Option<String>,
    pub(crate) translation: Option<String>,
    pub(crate) operation: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderRouteProjection {
    pub(crate) instance_id: Option<String>,
    pub(crate) instance_name: Option<String>,
    pub(crate) kind: Option<ProviderKind>,
    pub(crate) models: ProviderRouteModelsProjection,
    pub(crate) endpoint_label: Option<String>,
    pub(crate) route_status: ProviderRouteStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderRequirementProjection {
    pub(crate) code: ProviderRequirementCode,
    pub(crate) provider_kind: Option<ProviderKind>,
    pub(crate) provider_instance_id: Option<String>,
    pub(crate) can_rebind: bool,
    pub(crate) provider_committed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProjection {
    pub id: String,
    pub kind: String,
    pub provider: Option<String>,
    pub paper_id: Option<String>,
    pub revision_id: Option<String>,
    pub root_key: Option<String>,
    pub artifact_key: Option<String>,
    pub dedupe_key: String,
    pub state: JobState,
    pub stage: String,
    pub provider_committed: bool,
    pub priority: i64,
    pub payload: Value,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) provider_route: Option<ProviderRouteProjection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) provider_requirement: Option<ProviderRequirementProjection>,
    pub(crate) retry_disposition: RetryDisposition,
    pub(crate) retry_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) source_locator: Option<JobSourceLocatorProjection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RetryDisposition {
    Safe,
    ConfirmPossibleCharge,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JobSourceLocatorProjection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) paper_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) revision_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) artifact_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) object_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct JobRecord {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) provider: Option<String>,
    pub(crate) paper_id: Option<String>,
    pub(crate) revision_id: Option<String>,
    pub(crate) root_key: Option<String>,
    pub(crate) artifact_key: Option<String>,
    pub(crate) dedupe_key: String,
    pub(crate) state: JobState,
    pub(crate) stage: String,
    pub(crate) provider_committed: bool,
    pub(crate) priority: i64,
    pub(crate) payload: Value,
    pub(crate) last_error: Option<String>,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) progress: Option<Value>,
    pub(crate) route: StoredJobRoute,
    pub(crate) route_origin: Option<JobRouteOrigin>,
    pub(crate) provider_requirement: Option<ProviderRequirement>,
}

impl JobRecord {
    pub(crate) fn retry_disposition(&self) -> (RetryDisposition, &'static str) {
        if self.state != JobState::Failed && self.state != JobState::InterruptedUnknown {
            return (
                RetryDisposition::Unavailable,
                "只有失败或中断状态不确定的任务可以重试",
            );
        }
        if self.provider_requirement.is_some() {
            return (
                RetryDisposition::Unavailable,
                "必须先处理原 Provider 要求；系统不会自动切换账号或路由",
            );
        }
        let route_is_executable = match &self.route {
            StoredJobRoute::Executable(JobExecutionRoute::MistralOcr(_)) => self.kind == "ocr",
            StoredJobRoute::Executable(JobExecutionRoute::Paper(_)) => matches!(
                self.kind.as_str(),
                "reading_artifact"
                    | "orientation_pack"
                    | "document_artifact"
                    | "reading_roadmap"
                    | "reading_guide"
                    | "outline_overview"
                    | "outline_deep_dive"
            ),
            StoredJobRoute::Executable(JobExecutionRoute::Local)
            | StoredJobRoute::LegacyUnattributed
            | StoredJobRoute::Unavailable { .. } => false,
        };
        if !route_is_executable {
            return (
                RetryDisposition::Unavailable,
                "任务缺少可验证且可执行的原始 Provider 路由，请回到来源重新生成",
            );
        }
        if self.provider_committed || self.state == JobState::InterruptedUnknown {
            return (
                RetryDisposition::ConfirmPossibleCharge,
                "远程请求可能已经提交；重试可能产生重复请求或费用",
            );
        }
        (
            RetryDisposition::Safe,
            "失败发生在未确认提交远程请求的阶段，可沿原始 Provider 路由安全重试",
        )
    }

    pub(crate) fn project(&self) -> JobProjection {
        let route_status = self
            .provider_requirement
            .as_ref()
            .map(|_| ProviderRouteStatus::ActionRequired);
        let provider_route = match &self.route {
            StoredJobRoute::Executable(JobExecutionRoute::Paper(route)) => {
                let summary = route.safe_summary();
                Some(ProviderRouteProjection {
                    instance_id: Some(summary.instance_id),
                    instance_name: Some(summary.instance_name),
                    kind: Some(summary.kind),
                    models: ProviderRouteModelsProjection {
                        paper: Some(summary.paper_model),
                        translation: summary.translation_model,
                        operation: Some(summary.operation),
                    },
                    endpoint_label: summary.endpoint_label,
                    route_status: route_status.unwrap_or(ProviderRouteStatus::Ready),
                })
            }
            StoredJobRoute::LegacyUnattributed => Some(ProviderRouteProjection {
                instance_id: None,
                instance_name: None,
                kind: self.provider.as_deref().and_then(parse_provider_kind),
                models: ProviderRouteModelsProjection {
                    paper: None,
                    translation: None,
                    operation: None,
                },
                endpoint_label: None,
                route_status: route_status.unwrap_or(ProviderRouteStatus::Legacy),
            }),
            StoredJobRoute::Unavailable { provider_kind } => Some(ProviderRouteProjection {
                instance_id: None,
                instance_name: None,
                kind: provider_kind.clone(),
                models: ProviderRouteModelsProjection {
                    paper: None,
                    translation: None,
                    operation: None,
                },
                endpoint_label: None,
                route_status: ProviderRouteStatus::ActionRequired,
            }),
            StoredJobRoute::Executable(
                JobExecutionRoute::Local | JobExecutionRoute::MistralOcr(_),
            ) => None,
        };
        let provider_requirement =
            self.provider_requirement
                .as_ref()
                .map(|requirement| ProviderRequirementProjection {
                    code: requirement.code(),
                    provider_kind: requirement.provider_kind().cloned(),
                    provider_instance_id: requirement
                        .instance_id()
                        .map(|id| id.as_str().to_string()),
                    can_rebind: requirement.can_rebind(),
                    provider_committed: self.provider_committed,
                });
        let (retry_disposition, retry_reason) = self.retry_disposition();
        let source_locator = if self.paper_id.is_some()
            || self.revision_id.is_some()
            || self.artifact_key.is_some()
            || self.root_key.is_some()
        {
            Some(JobSourceLocatorProjection {
                paper_id: self.paper_id.clone(),
                revision_id: self.revision_id.clone(),
                artifact_kind: matches!(
                    self.kind.as_str(),
                    "reading_artifact"
                        | "orientation_pack"
                        | "document_artifact"
                        | "reading_roadmap"
                        | "reading_guide"
                        | "outline_overview"
                        | "outline_deep_dive"
                )
                .then(|| self.kind.clone()),
                object_key: self.artifact_key.clone().or_else(|| self.root_key.clone()),
            })
        } else {
            None
        };
        JobProjection {
            id: self.id.clone(),
            kind: self.kind.clone(),
            provider: self.provider.clone(),
            paper_id: self.paper_id.clone(),
            revision_id: self.revision_id.clone(),
            root_key: self.root_key.clone(),
            artifact_key: self.artifact_key.clone(),
            dedupe_key: self.dedupe_key.clone(),
            state: self.state,
            stage: self.stage.clone(),
            provider_committed: self.provider_committed,
            priority: self.priority,
            payload: self.payload.clone(),
            last_error: self.last_error.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            progress: self.progress.clone(),
            provider_route,
            provider_requirement,
            retry_disposition,
            retry_reason: retry_reason.to_string(),
            source_locator,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueResult {
    pub job: JobProjection,
    pub coalesced: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct EnqueueRecordResult {
    pub(crate) job: JobRecord,
    pub(crate) coalesced: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct EnqueueOnResult {
    pub(crate) job_id: String,
    pub(crate) coalesced: bool,
}

/// JobModule 在 Plan 阶段持久化冻结 route 后返回的不透明句柄。
/// 它不是可 claim 的 Job；只有 Start 在同一事务里 consume 后才会入队。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PreparedJobHandle {
    pub(crate) id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct ConsumePreparedResult {
    pub(crate) job_id: String,
    pub(crate) coalesced: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreparedJobSnapshot {
    spec: JobSpec,
    route: PreparedRouteRef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum PreparedRouteRef {
    #[serde(rename_all = "camelCase")]
    MistralOcr { route_id: String, model: String },
    #[serde(rename_all = "camelCase")]
    Paper {
        route_id: String,
        provider_kind: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RebindProviderRouteError {
    AlreadyActiveOnRoute { existing_job_id: String },
    Rejected(&'static str),
    Database,
}

impl std::fmt::Display for RebindProviderRouteError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyActiveOnRoute { .. } => {
                formatter.write_str("Another active job already uses that provider route")
            }
            Self::Rejected(message) => formatter.write_str(message),
            Self::Database => formatter.write_str("Provider job update failed"),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitPreparation {
    pub paused: usize,
    pub cancelled: usize,
    pub interrupted_unknown: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LegacyProviderJobReconciliation {
    pub(crate) adopted: usize,
    pub(crate) quarantined: usize,
    pub(crate) adopted_local: usize,
}

#[derive(Debug, Clone)]
pub struct JobModule {
    database_path: PathBuf,
}

impl JobModule {
    pub fn open(database_path: impl AsRef<Path>) -> JobResult<Self> {
        Ok(Self {
            database_path: database_path.as_ref().to_path_buf(),
        })
    }

    /// Startup recovery is explicit so the workspace lifecycle can complete
    /// migration, recovery, legacy reconciliation, and invariant checks before
    /// it starts any worker.
    pub(crate) fn recover_for_startup(&self) -> JobResult<()> {
        self.recover_interrupted()
    }

    pub(crate) fn reconcile_legacy_provider_jobs(
        &self,
        ready_routes: &[FrozenProviderRoute],
    ) -> JobResult<LegacyProviderJobReconciliation> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let timestamp = now();
        let adopted_local = transaction
            .execute(
                "UPDATE jobs
                 SET provider_route_origin = 'captured',
                     stage = CASE WHEN state = 'queued' THEN 'legacy_local_verified' ELSE stage END,
                     updated_at = ?1
                 WHERE provider_route_id IS NULL AND provider_route_origin IS NULL
                   AND provider IS NULL AND provider_committed = 0
                   AND state IN ('queued', 'paused')",
                params![timestamp],
            )
            .map_err(|error| error.to_string())?;
        let legacy_jobs = {
            let mut statement = transaction
                .prepare(
                    "SELECT jobs.id, jobs.kind, jobs.provider, jobs.dedupe_key, jobs.state,
                            jobs.provider_committed, jobs.payload_json,
                            EXISTS(
                              SELECT 1 FROM job_provider_requirements requirement
                              WHERE requirement.job_id = jobs.id
                            )
                     FROM jobs
                     WHERE jobs.provider_route_id IS NULL
                       AND jobs.provider_route_origin IS NULL
                       AND jobs.provider IS NOT NULL
                       AND jobs.state IN ('queued', 'running', 'paused', 'interrupted_unknown')
                     ORDER BY jobs.created_at, jobs.id",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, i64>(5)? != 0,
                        row.get::<_, String>(6)?,
                        row.get::<_, i64>(7)? != 0,
                    ))
                })
                .map_err(|error| error.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| error.to_string())?;
            rows
        };

        let mut result = LegacyProviderJobReconciliation {
            adopted_local,
            ..LegacyProviderJobReconciliation::default()
        };
        for (
            id,
            kind,
            provider,
            dedupe_key,
            state,
            provider_committed,
            payload_json,
            has_requirement,
        ) in legacy_jobs
        {
            if has_requirement {
                continue;
            }
            let payload = serde_json::from_str(&payload_json).unwrap_or(Value::Null);
            let matching_routes = legacy_matching_routes(&kind, &provider, &payload, ready_routes);
            let can_adopt = !provider_committed
                && matches!(state.as_str(), "queued" | "paused")
                && matching_routes.len() == 1;
            if can_adopt {
                let route = matching_routes[0];
                let route_id = persist_frozen_route(&transaction, route)
                    .map_err(|error| error.to_string())?
                    .route_id_database_value()
                    .to_string();
                let conflict = transaction
                    .query_row(
                        "SELECT id FROM jobs
                         WHERE id <> ?1 AND provider_route_id = ?2 AND dedupe_key = ?3
                           AND state IN ('queued', 'running', 'paused', 'interrupted_unknown')
                         ORDER BY created_at, id LIMIT 1",
                        params![id, route_id, dedupe_key],
                        |row| row.get::<_, String>(0),
                    )
                    .optional()
                    .map_err(|error| error.to_string())?;
                if conflict.is_none() {
                    let changed = transaction
                        .execute(
                            "UPDATE jobs
                             SET provider = ?2, provider_route_id = ?3,
                                 provider_route_origin = 'legacy_unique_verified',
                                 stage = CASE
                                   WHEN state = 'queued' THEN 'legacy_route_verified'
                                   ELSE stage
                                 END,
                                 updated_at = ?4
                             WHERE id = ?1 AND provider_route_id IS NULL
                               AND provider_route_origin IS NULL
                               AND provider_committed = 0
                               AND state IN ('queued', 'paused')",
                            params![id, route.provider_kind().as_str(), route_id, timestamp,],
                        )
                        .map_err(|error| error.to_string())?;
                    if changed == 1 {
                        result.adopted += 1;
                        continue;
                    }
                }
            }

            let code = legacy_reconciliation_requirement_code(
                &kind,
                &provider,
                &payload,
                ready_routes,
                matching_routes.len(),
                provider_committed || matches!(state.as_str(), "running" | "interrupted_unknown"),
            );
            quarantine_legacy_provider_job(
                &transaction,
                &id,
                &provider,
                provider_committed || state == "interrupted_unknown",
                code,
                &timestamp,
            )?;
            result.quarantined += 1;
        }
        if result.quarantined > 0 {
            bump_jobs_state(&transaction)?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(result)
    }

    pub(crate) fn assert_active_route_invariants(&self) -> JobResult<()> {
        for record in self.list_records()? {
            if !matches!(
                record.state,
                JobState::Queued
                    | JobState::Running
                    | JobState::Paused
                    | JobState::InterruptedUnknown
            ) {
                continue;
            }
            if record.provider_requirement.is_some()
                && !matches!(
                    record.state,
                    JobState::Paused | JobState::InterruptedUnknown
                )
            {
                return Err(format!(
                    "Active job {} has a provider requirement in an executable state",
                    record.id
                ));
            }
            if record.state == JobState::InterruptedUnknown && record.provider_requirement.is_none()
            {
                return Err(format!(
                    "Interrupted job {} must retain a provider requirement",
                    record.id
                ));
            }
            match &record.route {
                StoredJobRoute::Executable(JobExecutionRoute::Local) => {
                    if record.provider.is_some() {
                        return Err(format!("Local job {} names a remote provider", record.id));
                    }
                }
                StoredJobRoute::Executable(JobExecutionRoute::MistralOcr(_)) => {
                    if record.kind != "ocr" || record.provider.as_deref() != Some("mistral") {
                        return Err(format!(
                            "OCR job {} has an invalid Mistral route",
                            record.id
                        ));
                    }
                }
                StoredJobRoute::Executable(JobExecutionRoute::Paper(route)) => {
                    if record.provider.as_deref() != Some(route.provider_kind().as_str()) {
                        return Err(format!(
                            "Paper job {} conflicts with its frozen provider route",
                            record.id
                        ));
                    }
                }
                StoredJobRoute::LegacyUnattributed | StoredJobRoute::Unavailable { .. } => {
                    if record.provider_requirement.is_none()
                        || !matches!(
                            record.state,
                            JobState::Paused | JobState::InterruptedUnknown
                        )
                    {
                        return Err(format!(
                            "Unattributed job {} must be paused with a provider requirement",
                            record.id
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn enqueue(&self, spec: JobSpec) -> JobResult<EnqueueResult> {
        if spec.kind.trim().is_empty() || spec.dedupe_key.trim().is_empty() {
            return Err("Job kind and dedupe key are required".to_string());
        }
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let existing = transaction
            .query_row(
                "SELECT id FROM jobs
                 WHERE dedupe_key = ?1 AND state IN ('queued', 'running', 'paused')",
                params![spec.dedupe_key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(id) = existing {
            transaction.commit().map_err(|error| error.to_string())?;
            return Ok(EnqueueResult {
                job: self.get(&id)?,
                coalesced: true,
            });
        }

        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        let payload = serde_json::to_string(&spec.payload).map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO jobs(
                   id, kind, provider, paper_id, revision_id, root_key, artifact_key,
                   dedupe_key, state, stage, provider_committed, priority, payload_json,
                   created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'queued', 'queued', 0, ?9, ?10, ?11, ?11)",
                params![
                    id,
                    spec.kind,
                    spec.provider,
                    spec.paper_id,
                    spec.revision_id,
                    spec.root_key,
                    spec.artifact_key,
                    spec.dedupe_key,
                    spec.priority,
                    payload,
                    timestamp,
                ],
            )
            .map_err(|error| error.to_string())?;
        bump_jobs_state(&transaction)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(EnqueueResult {
            job: self.get(&id)?,
            coalesced: false,
        })
    }

    pub(crate) fn enqueue_record(
        &self,
        spec: JobSpec,
        route: JobExecutionRoute,
    ) -> JobResult<EnqueueRecordResult> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let enqueued = enqueue_record_on(&transaction, spec, route)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(EnqueueRecordResult {
            job: self.get_record(&enqueued.job_id)?,
            coalesced: enqueued.coalesced,
        })
    }

    pub(crate) fn get_record(&self, id: &str) -> JobResult<JobRecord> {
        let connection = self.connect()?;
        job_record_by_id(&connection, id)
    }

    pub(crate) fn list_records(&self) -> JobResult<Vec<JobRecord>> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT jobs.id, jobs.kind, jobs.provider, jobs.paper_id, jobs.revision_id,
                        jobs.root_key, jobs.artifact_key, jobs.dedupe_key, jobs.state, jobs.stage,
                        jobs.provider_committed, jobs.priority, jobs.payload_json, jobs.last_error,
                        jobs.created_at, jobs.updated_at, job_checkpoints.checkpoint_json,
                        jobs.provider_route_id, jobs.provider_route_origin,
                        requirements.code, requirements.provider_kind,
                        requirements.provider_instance_id, requirements.can_rebind,
                        requirements.safe_details_json
                 FROM jobs
                 LEFT JOIN job_checkpoints ON job_checkpoints.job_id = jobs.id
                 LEFT JOIN job_provider_requirements requirements ON requirements.job_id = jobs.id
                 ORDER BY jobs.updated_at DESC, jobs.id DESC",
            )
            .map_err(|error| error.to_string())?;
        let mut rows = statement.query([]).map_err(|error| error.to_string())?;
        let mut jobs = Vec::new();
        while let Some(row) = rows.next().map_err(|error| error.to_string())? {
            jobs.push(job_record_from_row(row, &connection).map_err(|error| error.to_string())?);
        }
        Ok(jobs)
    }

    pub(crate) fn claim_next_record(&self) -> JobResult<Option<JobRecord>> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let candidates = {
            let mut statement = transaction
                .prepare(
                    "SELECT j.id, j.kind, j.provider, j.paper_id,
                            COALESCE(pc.active_count, 0),
                            COALESCE(oc.active_count, 0)
                     FROM jobs j
                     LEFT JOIN (
                       SELECT provider, COUNT(*) AS active_count
                       FROM jobs WHERE state = 'running' AND provider IS NOT NULL
                       GROUP BY provider
                     ) pc ON pc.provider = j.provider
                     LEFT JOIN (
                       SELECT paper_id, COUNT(*) AS active_count
                       FROM jobs
                       WHERE state = 'running' AND kind = 'ocr' AND paper_id IS NOT NULL
                       GROUP BY paper_id
                     ) oc ON oc.paper_id = j.paper_id
                     WHERE j.state = 'queued'
                       AND j.provider_route_origin IS NOT NULL
                       AND NOT EXISTS (
                         SELECT 1 FROM job_provider_requirements requirement
                         WHERE requirement.job_id = j.id
                       )
                     ORDER BY j.priority DESC, j.created_at",
                )
                .map_err(|error| error.to_string())?;
            let candidates = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, i64>(5)?,
                    ))
                })
                .map_err(|error| error.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| error.to_string())?;
            candidates
        };
        let mut claimed = None;
        let mut state_changed = false;
        for (id, kind, provider, _paper_id, provider_active, ocr_active) in candidates {
            if provider_active >= 2 || (kind == "ocr" && ocr_active > 0) {
                continue;
            }
            let candidate = match job_record_by_id(&transaction, &id) {
                Ok(candidate) => candidate,
                Err(_) => {
                    quarantine_queued_provider_job(
                        &transaction,
                        &id,
                        provider.as_deref(),
                        ProviderRequirementCode::InvalidSnapshot,
                    )?;
                    state_changed = true;
                    continue;
                }
            };
            if !matches!(&candidate.route, StoredJobRoute::Executable(_)) {
                quarantine_queued_provider_job(
                    &transaction,
                    &id,
                    provider.as_deref(),
                    ProviderRequirementCode::LegacyUnattributed,
                )?;
                state_changed = true;
                continue;
            }
            let changed = transaction
                .execute(
                    "UPDATE jobs
                     SET state = 'running', stage = 'preparing', updated_at = ?2
                     WHERE id = ?1 AND state = 'queued'
                       AND NOT EXISTS (
                         SELECT 1 FROM job_provider_requirements requirement
                         WHERE requirement.job_id = jobs.id
                       )",
                    params![id, now()],
                )
                .map_err(|error| error.to_string())?;
            if changed == 0 {
                continue;
            }
            state_changed = true;
            let attempt_number: i64 = transaction
                .query_row(
                    "SELECT COALESCE(MAX(attempt_number), 0) + 1
                     FROM job_attempts WHERE job_id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "INSERT INTO job_attempts(
                       id, job_id, attempt_number, state, started_at
                     ) VALUES (?1, ?2, ?3, 'running', ?4)",
                    params![Uuid::new_v4().to_string(), id, attempt_number, now()],
                )
                .map_err(|error| error.to_string())?;
            claimed = Some(job_record_by_id(&transaction, &id)?);
            let _ = sync_linked_batch_items(&transaction, &id);
            break;
        }
        if state_changed {
            bump_jobs_state(&transaction)?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(claimed)
    }

    pub(crate) fn block_for_provider_action(
        &self,
        id: &str,
        requirement: ProviderRequirement,
    ) -> JobResult<JobRecord> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let provider_committed = transaction
            .query_row(
                "SELECT provider_committed FROM jobs WHERE id = ?1 AND state = 'running'",
                params![id],
                |row| Ok(row.get::<_, i64>(0)? != 0),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Only a running job can require provider action".to_string())?;
        if provider_committed && requirement.can_rebind() {
            return Err("A committed provider request cannot be rebound".to_string());
        }
        let timestamp = now();
        transaction
            .execute(
                "UPDATE job_attempts
                 SET state = 'blocked', finished_at = ?2
                 WHERE job_id = ?1 AND state = 'running'",
                params![id, timestamp],
            )
            .map_err(|error| error.to_string())?;
        let changed = transaction
            .execute(
                "UPDATE jobs
                 SET state = 'paused', stage = 'provider_action_required', updated_at = ?2
                 WHERE id = ?1 AND state = 'running'",
                params![id, timestamp],
            )
            .map_err(|error| error.to_string())?;
        if changed != 1 {
            return Err("Only a running job can require provider action".to_string());
        }
        upsert_provider_requirement(&transaction, id, &requirement, &timestamp)?;
        bump_jobs_state(&transaction)?;
        transaction.commit().map_err(|error| error.to_string())?;
        self.get_record(id)
    }

    pub(crate) fn resume_record(&self, id: &str) -> JobResult<()> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let record = job_record_by_id(&transaction, id)?;
        if record.state != JobState::Paused {
            return Err("Only a paused job can resume".to_string());
        }
        if record.provider_requirement.is_some() {
            return Err("Provider action is required before this job can resume".to_string());
        }
        match record.route {
            StoredJobRoute::Executable(_) => {}
            StoredJobRoute::LegacyUnattributed => {
                return Err(
                    "Legacy provider work must be reconciled before it can resume".to_string(),
                )
            }
            StoredJobRoute::Unavailable { .. } => {
                return Err("This job has no verified provider route and cannot resume".to_string())
            }
        }
        let changed = transaction
            .execute(
                "UPDATE jobs SET state = 'queued', stage = 'queued', updated_at = ?2
                 WHERE id = ?1 AND state = 'paused'",
                params![id, now()],
            )
            .map_err(|error| error.to_string())?;
        if changed != 1 {
            return Err("Job changed before it could resume".to_string());
        }
        bump_jobs_state(&transaction)?;
        transaction.commit().map_err(|error| error.to_string())
    }

    pub(crate) fn clear_provider_requirement_after_recheck(
        &self,
        id: &str,
        expected_route: &FrozenProviderRoute,
    ) -> JobResult<JobRecord> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let current = transaction
            .query_row(
                "SELECT state, provider_committed, provider_route_id
                 FROM jobs WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)? != 0,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Job does not exist".to_string())?;
        if current.2.as_deref() != Some(expected_route.route_id().database_value().as_str()) {
            return Err("Provider route changed before recheck completed".to_string());
        }
        if current.0 == "interrupted_unknown" && current.1 {
            let requirement = ProviderRequirement::for_job(
                ProviderRequirementCode::ProviderCommitted,
                Some(expected_route.instance_id().clone()),
                Some(expected_route.provider_kind().clone()),
                false,
            );
            let timestamp = now();
            upsert_provider_requirement(&transaction, id, &requirement, &timestamp)?;
            transaction
                .execute(
                    "UPDATE jobs
                     SET stage = 'interrupted_unknown', updated_at = ?2
                     WHERE id = ?1 AND state = 'interrupted_unknown'
                       AND provider_committed = 1",
                    params![id, timestamp],
                )
                .map_err(|error| error.to_string())?;
            transaction.commit().map_err(|error| error.to_string())?;
            return Err("A committed interrupted request cannot be retried".to_string());
        }
        if !matches!(current.0.as_str(), "paused" | "interrupted_unknown") {
            return Err("Only blocked provider work can be rechecked".to_string());
        }
        let deleted = transaction
            .execute(
                "DELETE FROM job_provider_requirements WHERE job_id = ?1",
                params![id],
            )
            .map_err(|error| error.to_string())?;
        if deleted != 1 {
            return Err("Job has no provider requirement to clear".to_string());
        }
        transaction
            .execute(
                "UPDATE jobs
                 SET state = 'queued', stage = 'provider_rechecked', last_error = NULL,
                     updated_at = ?2
                 WHERE id = ?1",
                params![id, now()],
            )
            .map_err(|error| error.to_string())?;
        bump_jobs_state(&transaction)?;
        transaction.commit().map_err(|error| error.to_string())?;
        self.get_record(id)
    }

    pub(crate) fn rebind_provider_route(
        &self,
        id: &str,
        replacement: &BoundProviderRoute,
    ) -> Result<JobRecord, RebindProviderRouteError> {
        let mut connection = self
            .connect()
            .map_err(|_| RebindProviderRouteError::Database)?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| RebindProviderRouteError::Database)?;
        let current = transaction
            .query_row(
                "SELECT jobs.state, jobs.provider_committed, jobs.provider_route_id,
                        jobs.dedupe_key, requirements.can_rebind
                 FROM jobs
                 LEFT JOIN job_provider_requirements requirements
                   ON requirements.job_id = jobs.id
                 WHERE jobs.id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)? != 0,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, Option<i64>>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|_| RebindProviderRouteError::Database)?
            .ok_or(RebindProviderRouteError::Rejected("Job does not exist"))?;
        if current.0 != "paused" {
            return Err(RebindProviderRouteError::Rejected(
                "Only paused provider work can be rebound",
            ));
        }
        if current.1 {
            return Err(RebindProviderRouteError::Rejected(
                "A committed provider request cannot be rebound",
            ));
        }
        if current.4 != Some(1) {
            return Err(RebindProviderRouteError::Rejected(
                "This provider requirement cannot be rebound",
            ));
        }
        let current_route_id = current.2.ok_or(RebindProviderRouteError::Rejected(
            "Legacy provider work needs verified attribution before rebind",
        ))?;
        let original = load_frozen_route(&transaction, &current_route_id)
            .map_err(|_| RebindProviderRouteError::Database)?
            .ok_or(RebindProviderRouteError::Rejected(
                "Original provider route snapshot is unavailable",
            ))?;
        let replacement = replacement.frozen();
        if original.provider_kind() != replacement.provider_kind()
            || original.models() != replacement.models()
            || original.operation() != replacement.operation()
        {
            return Err(RebindProviderRouteError::Rejected(
                "Replacement provider kind and frozen models must match",
            ));
        }
        let target_route_id = persist_frozen_route(&transaction, replacement)
            .map_err(|_| RebindProviderRouteError::Database)?
            .route_id_database_value()
            .to_string();
        let conflict = transaction
            .query_row(
                "SELECT id FROM jobs
                 WHERE id <> ?1 AND provider_route_id = ?2 AND dedupe_key = ?3
                   AND state IN ('queued', 'running', 'paused', 'interrupted_unknown')
                 ORDER BY created_at, id LIMIT 1",
                params![id, target_route_id, current.3],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|_| RebindProviderRouteError::Database)?;
        if let Some(existing_job_id) = conflict {
            return Err(RebindProviderRouteError::AlreadyActiveOnRoute { existing_job_id });
        }
        let changed = transaction
            .execute(
                "UPDATE jobs
                 SET provider = ?2, provider_route_id = ?3,
                     provider_route_origin = ?4, state = 'queued',
                     stage = 'provider_rebound', last_error = NULL, updated_at = ?5
                 WHERE id = ?1 AND state = 'paused' AND provider_committed = 0",
                params![
                    id,
                    replacement.provider_kind().as_str(),
                    target_route_id,
                    JobRouteOrigin::ExplicitRebind.as_database(),
                    now(),
                ],
            )
            .map_err(|_| RebindProviderRouteError::Database)?;
        if changed != 1 {
            return Err(RebindProviderRouteError::Rejected(
                "Provider job changed before rebind completed",
            ));
        }
        transaction
            .execute(
                "DELETE FROM job_provider_requirements WHERE job_id = ?1",
                params![id],
            )
            .map_err(|_| RebindProviderRouteError::Database)?;
        bump_jobs_state(&transaction).map_err(|_| RebindProviderRouteError::Database)?;
        transaction
            .commit()
            .map_err(|_| RebindProviderRouteError::Database)?;
        self.get_record(id)
            .map_err(|_| RebindProviderRouteError::Database)
    }

    pub(crate) fn abandon_legacy_provider_job(
        &self,
        id: &str,
        confirmed_potential_charge: bool,
    ) -> JobResult<JobRecord> {
        if !confirmed_potential_charge {
            return Err(
                "Confirm that the original provider request may have incurred a charge".to_string(),
            );
        }
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let current = transaction
            .query_row(
                "SELECT jobs.state, jobs.provider_route_id, jobs.provider_route_origin,
                        EXISTS(
                          SELECT 1 FROM job_provider_requirements requirement
                          WHERE requirement.job_id = jobs.id
                        )
                 FROM jobs WHERE jobs.id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, i64>(3)? != 0,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Job does not exist".to_string())?;
        if current.1.is_some() || current.2.is_some() {
            return Err("Only legacy unattributed provider work can be abandoned".to_string());
        }
        if !matches!(current.0.as_str(), "paused" | "interrupted_unknown") || !current.3 {
            return Err("Legacy provider work is not awaiting user action".to_string());
        }
        let timestamp = now();
        transaction
            .execute(
                "UPDATE job_attempts
                 SET state = 'cancelled', finished_at = COALESCE(finished_at, ?2)
                 WHERE job_id = ?1 AND state = 'running'",
                params![id, timestamp],
            )
            .map_err(|error| error.to_string())?;
        let changed = transaction
            .execute(
                "UPDATE jobs
                 SET state = 'cancelled', stage = 'provider_job_abandoned',
                     last_error = NULL, updated_at = ?2
                 WHERE id = ?1 AND state IN ('paused', 'interrupted_unknown')
                   AND provider_route_id IS NULL AND provider_route_origin IS NULL",
                params![id, timestamp],
            )
            .map_err(|error| error.to_string())?;
        if changed != 1 {
            return Err("Legacy provider job changed before abandon completed".to_string());
        }
        transaction
            .execute(
                "DELETE FROM job_provider_requirements WHERE job_id = ?1",
                params![id],
            )
            .map_err(|error| error.to_string())?;
        bump_jobs_state(&transaction)?;
        transaction.commit().map_err(|error| error.to_string())?;
        self.get_record(id)
    }

    pub fn get(&self, id: &str) -> JobResult<JobProjection> {
        self.get_record(id).map(|record| record.project())
    }

    pub fn list(&self) -> JobResult<Vec<JobProjection>> {
        self.list_records()
            .map(|records| records.into_iter().map(|record| record.project()).collect())
    }

    pub fn list_active_of(&self, kind: &str, revision_id: &str) -> JobResult<Vec<JobProjection>> {
        self.list_records().map(|records| {
            records
                .into_iter()
                .filter(|record| {
                    record.kind == kind
                        && record.revision_id.as_deref() == Some(revision_id)
                        && matches!(
                            record.state,
                            JobState::Queued | JobState::Running | JobState::Paused
                        )
                })
                .map(|record| record.project())
                .collect()
        })
    }

    pub fn active_of(&self, kind: &str, revision_id: &str) -> JobResult<Option<JobProjection>> {
        Ok(self.list_active_of(kind, revision_id)?.into_iter().next())
    }

    pub fn claim_next(&self) -> JobResult<Option<JobProjection>> {
        self.claim_next_record()
            .map(|record| record.map(|record| record.project()))
    }

    /// Progress only: `stage` and the checkpoint payload are not part of the Hub
    /// projection, so this write intentionally leaves the revision alone.
    pub fn save_checkpoint(&self, id: &str, stage: &str, checkpoint: &Value) -> JobResult<()> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let timestamp = now();
        let checkpoint = serde_json::to_string(checkpoint).map_err(|error| error.to_string())?;
        let changed = transaction
            .execute(
                "UPDATE jobs SET stage = ?2, updated_at = ?3
                 WHERE id = ?1 AND state = 'running'",
                params![id, stage, timestamp],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err("Only a running job can save a checkpoint".to_string());
        }
        transaction
            .execute(
                "INSERT INTO job_checkpoints(job_id, stage, checkpoint_json, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(job_id) DO UPDATE SET
                   stage = excluded.stage,
                   checkpoint_json = excluded.checkpoint_json,
                   updated_at = excluded.updated_at",
                params![id, stage, checkpoint, timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())
    }

    pub fn get_checkpoint(&self, id: &str) -> JobResult<Option<Value>> {
        let connection = self.connect()?;
        let raw: Option<String> = connection
            .query_row(
                "SELECT checkpoint_json FROM job_checkpoints WHERE job_id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        raw.map(|text| serde_json::from_str(&text).map_err(|error| error.to_string()))
            .transpose()
    }

    pub fn mark_provider_committed(
        &self,
        id: &str,
        provider_request_id: Option<&str>,
    ) -> JobResult<()> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let changed = transaction
            .execute(
                "UPDATE jobs
                 SET provider_committed = 1, stage = 'provider_committed', updated_at = ?2
                 WHERE id = ?1 AND state = 'running'",
                params![id, now()],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err("Only a running job can commit a provider request".to_string());
        }
        transaction
            .execute(
                "UPDATE job_attempts SET provider_request_id = ?2
                 WHERE job_id = ?1 AND state = 'running'",
                params![id, provider_request_id],
            )
            .map_err(|error| error.to_string())?;
        bump_jobs_state(&transaction)?;
        transaction.commit().map_err(|error| error.to_string())
    }

    pub fn complete(&self, id: &str) -> JobResult<()> {
        self.finish(id, JobState::Completed, None)
    }

    pub fn fail(&self, id: &str, error: &str) -> JobResult<()> {
        self.finish(id, JobState::Failed, Some(error))
    }

    pub fn pause(&self, id: &str) -> JobResult<()> {
        self.transition(id, &["queued"], JobState::Paused)
    }

    pub fn resume(&self, id: &str) -> JobResult<()> {
        self.resume_record(id)
    }

    /// Queue order only: see [`bump_jobs_state`] for why this is not a Hub change.
    pub fn reprioritize(&self, id: &str, priority: i64) -> JobResult<()> {
        let connection = self.connect()?;
        let changed = connection
            .execute(
                "UPDATE jobs SET priority = ?1, updated_at = ?2
                 WHERE id = ?3 AND state IN ('queued', 'paused')",
                params![priority.clamp(-1_000, 1_000), now(), id],
            )
            .map_err(|error| error.to_string())?;
        if changed != 1 {
            return Err("Only queued or paused jobs can be reprioritized".to_string());
        }
        Ok(())
    }

    pub fn cancel(&self, id: &str) -> JobResult<()> {
        self.transition(id, &["queued", "paused", "running"], JobState::Cancelled)
    }

    pub fn prepare_for_pause_exit(&self) -> JobResult<ExitPreparation> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let timestamp = now();
        transaction
            .execute(
                "UPDATE job_attempts
                 SET state = 'interrupted', finished_at = ?1
                 WHERE state = 'running'",
                params![timestamp],
            )
            .map_err(|error| error.to_string())?;

        let running = {
            let mut statement = transaction
                .prepare(
                    "SELECT id, kind, provider_committed, payload_json
                     FROM jobs WHERE state = 'running'",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)? != 0,
                        row.get::<_, String>(3)?,
                    ))
                })
                .map_err(|error| error.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| error.to_string())?;
            rows
        };

        let mut result = ExitPreparation::default();
        let mut touched = 0usize;
        for (id, kind, provider_committed, payload_json) in running {
            let payload: Value = serde_json::from_str(&payload_json).unwrap_or(Value::Null);
            let (state, stage) =
                if provider_committed && !self.has_durable_staged_response(&id, &kind, &payload) {
                    result.interrupted_unknown += 1;
                    ("interrupted_unknown", "interrupted_unknown")
                } else {
                    result.paused += 1;
                    ("paused", "paused_for_exit")
                };
            transaction
                .execute(
                    "UPDATE jobs SET state = ?2, stage = ?3, updated_at = ?4
                     WHERE id = ?1 AND state = 'running'",
                    params![id, state, stage, timestamp],
                )
                .map(|changed| touched += changed)
                .map_err(|error| error.to_string())?;
        }
        let queued_paused = transaction
            .execute(
                "UPDATE jobs SET state = 'paused', stage = 'paused_for_exit', updated_at = ?1
                 WHERE state = 'queued'",
                params![timestamp],
            )
            .map_err(|error| error.to_string())?;
        result.paused += queued_paused;
        touched += queued_paused;
        if touched > 0 {
            bump_jobs_state(&transaction)?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(result)
    }

    pub fn cancel_all_active(&self) -> JobResult<ExitPreparation> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let timestamp = now();
        transaction
            .execute(
                "UPDATE job_attempts
                 SET state = 'cancelled', finished_at = ?1
                 WHERE state = 'running'",
                params![timestamp],
            )
            .map_err(|error| error.to_string())?;
        let cancelled = transaction
            .execute(
                "UPDATE jobs SET state = 'cancelled', stage = 'cancelled', updated_at = ?1
                 WHERE state IN ('queued', 'running', 'paused')",
                params![timestamp],
            )
            .map_err(|error| error.to_string())?;
        if cancelled > 0 {
            bump_jobs_state(&transaction)?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(ExitPreparation {
            paused: 0,
            cancelled,
            interrupted_unknown: 0,
        })
    }

    fn finish(&self, id: &str, state: JobState, error: Option<&str>) -> JobResult<()> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|database_error| database_error.to_string())?;
        let changed = transaction
            .execute(
                "UPDATE jobs
                 SET state = ?2, stage = ?2, last_error = ?3, updated_at = ?4
                 WHERE id = ?1 AND state = 'running'",
                params![id, state.as_database(), error, now()],
            )
            .map_err(|database_error| database_error.to_string())?;
        if changed == 0 {
            return Err("Only a running job can be finished".to_string());
        }
        transaction
            .execute(
                "UPDATE job_attempts
                 SET state = ?2, finished_at = ?3, error_detail = ?4
                 WHERE job_id = ?1 AND state = 'running'",
                params![id, state.as_database(), now(), error],
            )
            .map_err(|database_error| database_error.to_string())?;
        bump_jobs_state(&transaction)?;
        sync_linked_batch_items(&transaction, id)?;
        transaction
            .commit()
            .map_err(|database_error| database_error.to_string())
    }

    fn transition(&self, id: &str, from: &[&str], to: JobState) -> JobResult<()> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let placeholders = (0..from.len())
            .map(|index| format!("?{}", index + 3))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "UPDATE jobs SET state = ?2, stage = ?2, updated_at = ?1
             WHERE id = ?{} AND state IN ({placeholders})",
            from.len() + 3
        );
        let timestamp = now();
        let target_state = to.as_database();
        let mut values: Vec<&dyn rusqlite::ToSql> = vec![&timestamp, &target_state];
        for state in from {
            values.push(state);
        }
        values.push(&id);
        let changed = transaction
            .execute(&sql, values.as_slice())
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err(format!("Job cannot transition to {}", to.as_database()));
        }
        if to == JobState::Cancelled {
            transaction
                .execute(
                    "UPDATE job_attempts SET state = 'cancelled', finished_at = ?2
                     WHERE job_id = ?1 AND state = 'running'",
                    params![id, timestamp],
                )
                .map_err(|error| error.to_string())?;
        }
        bump_jobs_state(&transaction)?;
        sync_linked_batch_items(&transaction, id)?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(())
    }

    fn has_durable_staged_response(&self, id: &str, kind: &str, payload: &Value) -> bool {
        let explicit_staging = payload
            .get("stagingPath")
            .and_then(Value::as_str)
            .is_some_and(|path| Path::new(path).is_file());
        let durable_ocr_staging = kind == "ocr"
            && self
                .database_path
                .parent()
                .map(|directory| {
                    directory
                        .join("staging")
                        .join("ocr")
                        .join(format!("{id}.json"))
                })
                .is_some_and(|path| path.is_file());
        explicit_staging || durable_ocr_staging
    }

    fn recover_interrupted(&self) -> JobResult<()> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "UPDATE job_attempts
                 SET state = 'interrupted', finished_at = ?1
                 WHERE state = 'running'",
                params![now()],
            )
            .map_err(|error| error.to_string())?;
        let interrupted = {
            let mut statement = transaction
                .prepare(
                    "SELECT id, kind, provider, provider_route_id,
                            provider_committed, payload_json
                     FROM jobs WHERE state = 'running'",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, i64>(4)? != 0,
                        row.get::<_, String>(5)?,
                    ))
                })
                .map_err(|error| error.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| error.to_string())?;
            rows
        };
        let mut recovered = 0usize;
        for (id, kind, provider, route_id, provider_committed, payload_json) in interrupted {
            let payload: Value = serde_json::from_str(&payload_json).unwrap_or(Value::Null);
            let has_staged_response = self.has_durable_staged_response(&id, &kind, &payload);
            let (state, stage) = if !provider_committed {
                ("queued", "recovered")
            } else if has_staged_response {
                ("queued", "recovered_staged")
            } else {
                ("interrupted_unknown", "interrupted_unknown")
            };
            let timestamp = now();
            transaction
                .execute(
                    "UPDATE jobs SET state = ?2, stage = ?3, updated_at = ?4
                     WHERE id = ?1 AND state = 'running'",
                    params![id, state, stage, timestamp],
                )
                .map(|changed| recovered += changed)
                .map_err(|error| error.to_string())?;
            if state == "interrupted_unknown" {
                let requirement = provider_committed_requirement(
                    &transaction,
                    provider.as_deref(),
                    route_id.as_deref(),
                );
                upsert_provider_requirement(&transaction, &id, &requirement, &timestamp)?;
            }
        }
        let missing_requirements = {
            let mut statement = transaction
                .prepare(
                    "SELECT jobs.id, jobs.provider, jobs.provider_route_id
                     FROM jobs
                     WHERE jobs.state = 'interrupted_unknown'
                       AND jobs.provider_committed = 1
                       AND NOT EXISTS (
                         SELECT 1 FROM job_provider_requirements requirement
                         WHERE requirement.job_id = jobs.id
                       )",
                )
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map([], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                })
                .map_err(|error| error.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| error.to_string())?;
            rows
        };
        for (id, provider, route_id) in missing_requirements {
            let requirement = provider_committed_requirement(
                &transaction,
                provider.as_deref(),
                route_id.as_deref(),
            );
            upsert_provider_requirement(&transaction, &id, &requirement, &now())?;
        }
        if recovered > 0 {
            bump_jobs_state(&transaction)?;
        }
        transaction.commit().map_err(|error| error.to_string())
    }

    fn connect(&self) -> JobResult<Connection> {
        db::open(&self.database_path).map_err(|error| error.to_string())
    }
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn enqueue_record_on(
    transaction: &Transaction<'_>,
    mut spec: JobSpec,
    route: JobExecutionRoute,
) -> JobResult<EnqueueOnResult> {
    if spec.kind.trim().is_empty() || spec.dedupe_key.trim().is_empty() {
        return Err("Job kind and dedupe key are required".to_string());
    }
    let route_id = match &route {
        JobExecutionRoute::Paper(frozen) => {
            let kind = frozen.provider_kind().as_str();
            if spec.provider.as_deref().is_some_and(|value| value != kind) {
                return Err("Job provider kind does not match its frozen route".to_string());
            }
            spec.provider = Some(kind.to_string());
            Some(
                persist_frozen_route(transaction, frozen)
                    .map_err(|error| error.to_string())?
                    .route_id_database_value()
                    .to_string(),
            )
        }
        JobExecutionRoute::MistralOcr(frozen) => {
            if spec.kind != "ocr" {
                return Err("Mistral route is only valid for OCR jobs".to_string());
            }
            spec.provider = Some("mistral".to_string());
            Some(
                persist_frozen_mistral_ocr_route(transaction, frozen)
                    .map_err(|error| error.to_string())?
                    .route_id_database_value()
                    .to_string(),
            )
        }
        JobExecutionRoute::Local => {
            if spec.provider.is_some() {
                return Err("Local jobs cannot name a remote provider".to_string());
            }
            None
        }
    };
    if let Some(id) = lookup_active_job(transaction, route_id.as_deref(), &spec.dedupe_key)? {
        return Ok(EnqueueOnResult {
            job_id: id,
            coalesced: true,
        });
    }

    let id = Uuid::new_v4().to_string();
    let timestamp = now();
    let payload = serde_json::to_string(&spec.payload).map_err(|error| error.to_string())?;
    transaction
        .execute(
            "INSERT INTO jobs(
               id, kind, provider, paper_id, revision_id, root_key, artifact_key,
               dedupe_key, state, stage, provider_committed, priority, payload_json,
               created_at, updated_at, provider_route_id, provider_route_origin
             ) VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'queued', 'queued', 0,
               ?9, ?10, ?11, ?11, ?12, ?13
             )",
            params![
                id,
                spec.kind,
                spec.provider,
                spec.paper_id,
                spec.revision_id,
                spec.root_key,
                spec.artifact_key,
                spec.dedupe_key,
                spec.priority,
                payload,
                timestamp,
                route_id,
                JobRouteOrigin::Captured.as_database(),
            ],
        )
        .map_err(|error| error.to_string())?;
    bump_jobs_state(transaction)?;
    Ok(EnqueueOnResult {
        job_id: id,
        coalesced: false,
    })
}

fn lookup_active_job(
    connection: &Connection,
    route_id: Option<&str>,
    dedupe_key: &str,
) -> JobResult<Option<String>> {
    match route_id {
        Some(route_id) => connection
            .query_row(
                "SELECT id FROM jobs
                 WHERE provider_route_id = ?1 AND dedupe_key = ?2
                   AND state IN ('queued', 'running', 'paused', 'interrupted_unknown')
                 ORDER BY created_at, id LIMIT 1",
                params![route_id, dedupe_key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string()),
        None => connection
            .query_row(
                "SELECT id FROM jobs
                 WHERE provider_route_id IS NULL AND dedupe_key = ?1
                   AND state IN ('queued', 'running', 'paused', 'interrupted_unknown')
                 ORDER BY created_at, id LIMIT 1",
                params![dedupe_key],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string()),
    }
}

fn persist_route_ref(
    transaction: &Transaction<'_>,
    route: &JobExecutionRoute,
) -> JobResult<PreparedRouteRef> {
    match route {
        JobExecutionRoute::MistralOcr(frozen) => {
            let persisted = persist_frozen_mistral_ocr_route(transaction, frozen)
                .map_err(|error| error.to_string())?;
            Ok(PreparedRouteRef::MistralOcr {
                route_id: persisted.route_id_database_value().to_string(),
                model: frozen.model().to_string(),
            })
        }
        JobExecutionRoute::Paper(frozen) => {
            let persisted =
                persist_frozen_route(transaction, frozen).map_err(|error| error.to_string())?;
            Ok(PreparedRouteRef::Paper {
                route_id: persisted.route_id_database_value().to_string(),
                provider_kind: frozen.provider_kind().as_str().to_string(),
            })
        }
        JobExecutionRoute::Local => Err("Local jobs do not use prepared handles".to_string()),
    }
}

fn load_prepared_route(
    connection: &Connection,
    snapshot: &PreparedRouteRef,
) -> JobResult<JobExecutionRoute> {
    match snapshot {
        PreparedRouteRef::MistralOcr { route_id, .. } => {
            load_frozen_mistral_ocr_route(connection, route_id)
                .map_err(|error| error.to_string())?
                .map(JobExecutionRoute::MistralOcr)
                .ok_or_else(|| "Prepared OCR route snapshot is missing".to_string())
        }
        PreparedRouteRef::Paper { route_id, .. } => load_frozen_route(connection, route_id)
            .map_err(|error| error.to_string())?
            .map(JobExecutionRoute::Paper)
            .ok_or_else(|| "Prepared paper route snapshot is missing".to_string()),
    }
}

fn spec_digest(spec: &JobSpec) -> JobResult<String> {
    let payload = serde_json::to_string(&json!({
        "kind": spec.kind,
        "provider": spec.provider,
        "paperId": spec.paper_id,
        "revisionId": spec.revision_id,
        "rootKey": spec.root_key,
        "artifactKey": spec.artifact_key,
        "dedupeKey": spec.dedupe_key,
        "payload": spec.payload,
    }))
    .map_err(|error| error.to_string())?;
    Ok(format!("{:x}", Sha256::digest(payload.as_bytes())))
}

pub(crate) fn prepare_exact_on(
    transaction: &Transaction<'_>,
    spec: &JobSpec,
    route: &JobExecutionRoute,
    expires_at: &str,
) -> JobResult<PreparedJobHandle> {
    if spec.kind.trim().is_empty() || spec.dedupe_key.trim().is_empty() {
        return Err("Job kind and dedupe key are required".to_string());
    }
    let route_ref = persist_route_ref(transaction, route)?;
    let snapshot = PreparedJobSnapshot {
        spec: spec.clone(),
        route: route_ref,
    };
    let frozen_json = serde_json::to_string(&snapshot).map_err(|error| error.to_string())?;
    let id = Uuid::new_v4().to_string();
    let timestamp = now();
    transaction
        .execute(
            "INSERT INTO job_preparations(
               id, spec_digest, frozen_route_json, state, expires_at,
               consumed_job_id, created_at, updated_at
             ) VALUES (?1, ?2, ?3, 'prepared', ?4, NULL, ?5, ?5)",
            params![id, spec_digest(spec)?, frozen_json, expires_at, timestamp],
        )
        .map_err(|error| error.to_string())?;
    Ok(PreparedJobHandle { id })
}

pub(crate) fn consume_prepared_on(
    transaction: &Transaction<'_>,
    handle: &PreparedJobHandle,
    expected_route: &JobExecutionRoute,
) -> JobResult<ConsumePreparedResult> {
    let row: Option<(String, String, String, Option<String>)> = transaction
        .query_row(
            "SELECT spec_digest, frozen_route_json, state, expires_at
             FROM job_preparations WHERE id = ?1",
            params![handle.id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((_digest, frozen_json, state, expires_at)) = row else {
        return Err("Prepared job handle does not exist".to_string());
    };
    if state != "prepared" {
        return Err(format!("Prepared job handle is {state}, not prepared"));
    }
    if expires_at
        .as_deref()
        .is_some_and(|expires| expires <= now().as_str())
    {
        transaction
            .execute(
                "UPDATE job_preparations SET state = 'expired', updated_at = ?2 WHERE id = ?1",
                params![handle.id, now()],
            )
            .map_err(|error| error.to_string())?;
        return Err("Prepared job handle expired".to_string());
    }
    let snapshot: PreparedJobSnapshot =
        serde_json::from_str(&frozen_json).map_err(|error| error.to_string())?;
    let stored_route = load_prepared_route(transaction, &snapshot.route)?;
    if stored_route != *expected_route {
        return Err("Prepared job cannot exact-bind the current provider route".to_string());
    }
    let enqueued = enqueue_record_on(transaction, snapshot.spec, stored_route)?;
    let timestamp = now();
    let changed = transaction
        .execute(
            "UPDATE job_preparations
             SET state = 'consumed', consumed_job_id = ?2, updated_at = ?3
             WHERE id = ?1 AND state = 'prepared'",
            params![handle.id, enqueued.job_id, timestamp],
        )
        .map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("Prepared job handle changed before it could be consumed".to_string());
    }
    Ok(ConsumePreparedResult {
        job_id: enqueued.job_id,
        coalesced: enqueued.coalesced,
    })
}

pub(crate) fn release_prepared_on(connection: &Connection, handle_id: &str) -> JobResult<()> {
    connection
        .execute(
            "UPDATE job_preparations SET state = 'released', updated_at = ?2
             WHERE id = ?1 AND state = 'prepared'",
            params![handle_id, now()],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn expire_due_preparations(
    connection: &Connection,
    timestamp: &str,
) -> JobResult<usize> {
    let changed = connection
        .execute(
            "UPDATE job_preparations SET state = 'expired', updated_at = ?1
             WHERE state = 'prepared' AND expires_at <= ?1",
            params![timestamp],
        )
        .map_err(|error| error.to_string())?;
    Ok(changed)
}

pub(crate) fn find_active_job_id(
    connection: &Connection,
    route: &JobExecutionRoute,
    dedupe_key: &str,
) -> JobResult<Option<String>> {
    let route_id = match route {
        JobExecutionRoute::Paper(frozen) => Some(frozen.route_id().database_value()),
        JobExecutionRoute::MistralOcr(frozen) => Some(frozen.route_id().database_value()),
        JobExecutionRoute::Local => None,
    };
    lookup_active_job(connection, route_id.as_deref(), dedupe_key)
}

pub(crate) fn cancel_job_on(connection: &Connection, id: &str) -> JobResult<bool> {
    let timestamp = now();
    let committed: Option<i64> = connection
        .query_row(
            "SELECT provider_committed FROM jobs WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if committed == Some(1) {
        return Ok(false);
    }
    let changed = connection
        .execute(
            "UPDATE jobs SET state = 'cancelled', stage = 'cancelled', updated_at = ?2
             WHERE id = ?1 AND state IN ('queued', 'paused', 'running')",
            params![id, timestamp],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Ok(false);
    }
    connection
        .execute(
            "UPDATE job_attempts SET state = 'cancelled', finished_at = ?2
             WHERE job_id = ?1 AND state = 'running'",
            params![id, timestamp],
        )
        .map_err(|error| error.to_string())?;
    bump_jobs_state(connection)?;
    sync_linked_batch_items(connection, id)?;
    Ok(true)
}

/// 把 Job 终态抄到仍 active 的 Batch Item 上。JobModule 拥有 Job 状态，
/// 所以同步写在这里，避免 library_batch 再开第二套状态机。
pub(crate) fn sync_linked_batch_items(connection: &Connection, job_id: &str) -> JobResult<()> {
    let job: Option<(String, i64, Option<String>)> = connection
        .query_row(
            "SELECT state, provider_committed, last_error FROM jobs WHERE id = ?1",
            params![job_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let Some((job_state, committed, last_error)) = job else {
        return Ok(());
    };
    let (item_state, consumer_state, error_code, error_summary, retryable): (
        Option<&str>,
        &str,
        Option<&str>,
        Option<String>,
        i64,
    ) = match job_state.as_str() {
        "queued" => (Some("queued"), "active", None, None, 0),
        "running" => (Some("running"), "active", None, None, 0),
        "paused" => (Some("paused"), "active", None, None, 0),
        "completed" => (Some("succeeded"), "completed", None, None, 0),
        "failed" | "interrupted_unknown" => {
            let retryable = if committed != 0 { 1 } else { 1 };
            (
                Some("failed"),
                "completed",
                Some("workspace_busy"),
                Some(
                    last_error
                        .as_deref()
                        .map(|_| "the provider job failed; retry uses the Job retry disposition")
                        .unwrap_or("the provider job failed")
                        .to_string(),
                ),
                retryable,
            )
        }
        "cancelled" => (Some("cancelled"), "detached", None, None, 0),
        _ => (None, "active", None, None, 0),
    };
    let Some(item_state) = item_state else {
        return Ok(());
    };
    let timestamp = now();
    let terminal = matches!(item_state, "succeeded" | "failed" | "cancelled");
    let item_ids: Vec<String> = {
        let mut statement = connection
            .prepare(
                "SELECT batch_item_id FROM library_batch_job_links
                 WHERE job_id = ?1 AND consumer_state = 'active'",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![job_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        rows
    };
    if item_ids.is_empty() {
        return Ok(());
    }
    let receipt_id: Option<String> = connection
        .query_row(
            "SELECT id FROM usage_receipts WHERE job_id = ?1 ORDER BY created_at DESC LIMIT 1",
            params![job_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    for item_id in &item_ids {
        connection
            .execute(
                "UPDATE library_batch_items
                 SET state = ?2,
                     error_code = CASE WHEN ?3 IS NULL THEN error_code ELSE ?3 END,
                     error_summary = CASE WHEN ?4 IS NULL THEN error_summary ELSE ?4 END,
                     started_at = COALESCE(started_at, CASE WHEN ?2 IN ('running','succeeded','failed','cancelled') THEN ?5 END),
                     finished_at = CASE WHEN ?6 = 1 THEN COALESCE(finished_at, ?5) ELSE finished_at END,
                     updated_at = ?5
                 WHERE id = ?1 AND state IN ('planned','queued','running','paused')",
                params![
                    item_id,
                    item_state,
                    error_code,
                    error_summary.as_deref(),
                    timestamp,
                    if terminal { 1 } else { 0 },
                ],
            )
            .map_err(|error| error.to_string())?;
        if retryable == 1 && item_state == "failed" {
            // retryable 列不存在：它是投影时从 error_code 推导的。
        }
        connection
            .execute(
                "UPDATE library_batch_job_links
                 SET consumer_state = ?2,
                     usage_receipt_id = CASE
                       WHEN ownership = 'created' AND ?3 IS NOT NULL THEN ?3
                       ELSE usage_receipt_id
                     END,
                     detached_at = CASE WHEN ?2 != 'active' THEN COALESCE(detached_at, ?4) ELSE detached_at END
                 WHERE batch_item_id = ?1 AND job_id = ?5 AND consumer_state = 'active'",
                params![item_id, consumer_state, receipt_id, timestamp, job_id],
            )
            .map_err(|error| error.to_string())?;
    }
    let batch_ids: Vec<String> = {
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT items.batch_id
                 FROM library_batch_items items
                 JOIN library_batch_job_links links ON links.batch_item_id = items.id
                 WHERE links.job_id = ?1",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![job_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        rows
    };
    for batch_id in batch_ids {
        refresh_batch_aggregate(connection, &batch_id, &timestamp)?;
    }
    Ok(())
}

fn refresh_batch_aggregate(
    connection: &Connection,
    batch_id: &str,
    timestamp: &str,
) -> JobResult<()> {
    let mut counts = [0_i64; 11];
    {
        let mut statement = connection
            .prepare(
                "SELECT state, COUNT(*) FROM library_batch_items WHERE batch_id = ?1 GROUP BY state",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![batch_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })
            .map_err(|error| error.to_string())?;
        for row in rows {
            let (state, count) = row.map_err(|error| error.to_string())?;
            let index = match state.as_str() {
                "planned" => 0,
                "queued" => 1,
                "running" => 2,
                "paused" => 3,
                "action_required" => 4,
                "interrupted_unknown" => 5,
                "succeeded" => 6,
                "failed" => 7,
                "skipped" => 8,
                "cancelled" => 9,
                _ => 10,
            };
            counts[index] += count;
        }
    }
    let total: i64 = counts.iter().sum();
    let derived = if counts[2] > 0 {
        "running"
    } else if counts[1] + counts[0] > 0 {
        "queued"
    } else if counts[5] > 0 {
        "interrupted_unknown"
    } else if counts[4] > 0 {
        "action_required"
    } else if counts[3] > 0 {
        "paused"
    } else if counts[9] == total && total > 0 {
        "cancelled"
    } else if counts[7] == total && total > 0 {
        "failed"
    } else if counts[6] + counts[8] > 0 && counts[7] + counts[9] > 0 {
        "completed_with_errors"
    } else {
        "completed"
    };
    let finished = matches!(
        derived,
        "completed" | "completed_with_errors" | "failed" | "cancelled"
    );
    connection
        .execute(
            "UPDATE library_batches
             SET state = ?2,
                 finished_at = CASE WHEN ?4 = 1 THEN COALESCE(finished_at, ?3) ELSE finished_at END,
                 updated_at = ?3
             WHERE id = ?1 AND state != 'planned'",
            params![batch_id, derived, timestamp, if finished { 1 } else { 0 }],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Schema 8 invariant: a write that moves `jobs.state` moves the `jobs`
/// revision inside the same transaction, because `hub_page` declares `jobs` as
/// a dependency and the card status is derived from that column. Writes that
/// only carry progress (`stage`, checkpoint payloads) or queue order
/// (`priority`) are deliberately excluded: they change nothing the Hub
/// projection reads, and bumping on every checkpoint would invalidate a page
/// while an OCR run is reporting progress.
fn bump_jobs_state(connection: &Connection) -> JobResult<()> {
    bump_library_revisions(connection, &[LibraryDomain::Jobs])
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn legacy_matching_routes<'a>(
    kind: &str,
    provider: &str,
    payload: &Value,
    ready_routes: &'a [FrozenProviderRoute],
) -> Vec<&'a FrozenProviderRoute> {
    ready_routes
        .iter()
        .filter(|route| {
            route.provider_kind().as_str() == provider
                && legacy_payload_provider_matches(payload, provider)
                && legacy_endpoint_matches(payload, route)
                && legacy_models_match(kind, payload, route)
        })
        .collect()
}

fn legacy_payload_provider_matches(payload: &Value, provider: &str) -> bool {
    payload
        .get("provider")
        .and_then(Value::as_str)
        .map(str::trim)
        == Some(provider)
}

fn legacy_endpoint_matches(payload: &Value, route: &FrozenProviderRoute) -> bool {
    let legacy_base_url = payload
        .get("baseUrl")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match route.endpoint_base_url() {
        Some(expected) => legacy_base_url == Some(expected),
        None => legacy_base_url.is_none(),
    }
}

fn legacy_models_match(kind: &str, payload: &Value, route: &FrozenProviderRoute) -> bool {
    let has_artifact_models = kind == "reading_artifact"
        || payload.get("paperModel").is_some()
        || payload.get("translationModel").is_some();
    if has_artifact_models {
        let paper = payload
            .get("paperModel")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let translation = payload
            .get("translationModel")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let action = payload
            .get("action")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let expected_role = match action {
            Some("translate") => ModelRole::Translation,
            Some("explain" | "lens") => ModelRole::Paper,
            _ => return false,
        };
        return paper == Some(route.models().paper())
            && translation == route.models().translation()
            && action.is_some()
            && route.operation() == expected_role;
    }

    payload
        .get("model")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        == Some(route.models().paper())
        && route.operation() == ModelRole::Paper
}

fn legacy_payload_has_complete_route_details(kind: &str, provider: &str, payload: &Value) -> bool {
    if !legacy_payload_provider_matches(payload, provider) {
        return false;
    }
    let has_artifact_models = kind == "reading_artifact"
        || payload.get("paperModel").is_some()
        || payload.get("translationModel").is_some();
    if has_artifact_models {
        return payload
            .get("paperModel")
            .and_then(Value::as_str)
            .is_some_and(|value| !value.trim().is_empty())
            && payload
                .get("translationModel")
                .and_then(Value::as_str)
                .is_some_and(|value| !value.trim().is_empty())
            && payload
                .get("action")
                .and_then(Value::as_str)
                .is_some_and(|value| matches!(value.trim(), "translate" | "explain" | "lens"));
    }
    payload
        .get("model")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.trim().is_empty())
}

fn legacy_reconciliation_requirement_code(
    kind: &str,
    provider: &str,
    payload: &Value,
    ready_routes: &[FrozenProviderRoute],
    matching_routes: usize,
    provider_committed_or_interrupted: bool,
) -> ProviderRequirementCode {
    if provider_committed_or_interrupted {
        return ProviderRequirementCode::ProviderCommitted;
    }
    if !legacy_payload_has_complete_route_details(kind, provider, payload) {
        return ProviderRequirementCode::LegacyUnattributed;
    }
    if matching_routes > 1 {
        return ProviderRequirementCode::LegacyAmbiguous;
    }
    let same_provider = ready_routes
        .iter()
        .filter(|route| route.provider_kind().as_str() == provider)
        .collect::<Vec<_>>();
    if same_provider.is_empty() {
        return ProviderRequirementCode::ProviderMissing;
    }
    if same_provider
        .iter()
        .all(|route| !legacy_endpoint_matches(payload, route))
    {
        return ProviderRequirementCode::EndpointMismatch;
    }
    if same_provider
        .iter()
        .any(|route| legacy_endpoint_matches(payload, route))
    {
        return ProviderRequirementCode::ModelMismatch;
    }
    ProviderRequirementCode::LegacyUnattributed
}

fn quarantine_legacy_provider_job(
    transaction: &Transaction<'_>,
    id: &str,
    provider: &str,
    preserve_interrupted_unknown: bool,
    code: ProviderRequirementCode,
    timestamp: &str,
) -> JobResult<()> {
    let (state, stage, attempt_state) = if preserve_interrupted_unknown {
        ("interrupted_unknown", "interrupted_unknown", "interrupted")
    } else {
        ("paused", "provider_action_required", "blocked")
    };
    let changed = transaction
        .execute(
            "UPDATE jobs
             SET state = ?2, stage = ?3, updated_at = ?4
             WHERE id = ?1 AND provider_route_id IS NULL
               AND provider_route_origin IS NULL
               AND state IN ('queued', 'running', 'paused', 'interrupted_unknown')",
            params![id, state, stage, timestamp],
        )
        .map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("Legacy provider job changed before quarantine completed".to_string());
    }
    transaction
        .execute(
            "UPDATE job_attempts SET state = ?2, finished_at = COALESCE(finished_at, ?3)
             WHERE job_id = ?1 AND state = 'running'",
            params![id, attempt_state, timestamp],
        )
        .map_err(|error| error.to_string())?;
    let requirement =
        ProviderRequirement::for_job(code, None, parse_provider_kind(provider), false);
    upsert_provider_requirement(transaction, id, &requirement, timestamp)
}

fn job_record_by_id(connection: &Connection, id: &str) -> JobResult<JobRecord> {
    connection
        .query_row(
            "SELECT jobs.id, jobs.kind, jobs.provider, jobs.paper_id, jobs.revision_id,
                    jobs.root_key, jobs.artifact_key, jobs.dedupe_key, jobs.state, jobs.stage,
                    jobs.provider_committed, jobs.priority, jobs.payload_json, jobs.last_error,
                    jobs.created_at, jobs.updated_at, job_checkpoints.checkpoint_json,
                    jobs.provider_route_id, jobs.provider_route_origin,
                    requirements.code, requirements.provider_kind,
                    requirements.provider_instance_id, requirements.can_rebind,
                    requirements.safe_details_json
             FROM jobs
             LEFT JOIN job_checkpoints ON job_checkpoints.job_id = jobs.id
             LEFT JOIN job_provider_requirements requirements ON requirements.job_id = jobs.id
             WHERE jobs.id = ?1",
            params![id],
            |row| job_record_from_row(row, connection),
        )
        .map_err(|error| error.to_string())
}

fn load_stored_execution_route(
    connection: &Connection,
    route_id: &str,
    projection: &JobProjection,
) -> Result<JobExecutionRoute, String> {
    match load_frozen_route(connection, route_id) {
        Ok(Some(route)) => {
            if projection.provider.as_deref() != Some(route.provider_kind().as_str()) {
                return Err("Job provider kind conflicts with its route snapshot".to_string());
            }
            return Ok(JobExecutionRoute::Paper(route));
        }
        Ok(None) | Err(_) => {}
    }

    match load_frozen_mistral_ocr_route(connection, route_id) {
        Ok(Some(route))
            if projection.kind == "ocr" && projection.provider.as_deref() == Some("mistral") =>
        {
            Ok(JobExecutionRoute::MistralOcr(route))
        }
        Ok(Some(_)) => Err("Mistral OCR route conflicts with its job discriminator".to_string()),
        Ok(None) => Err("Provider route snapshot is missing".to_string()),
        Err(error) => Err(error.to_string()),
    }
}

fn provider_committed_requirement(
    connection: &Connection,
    provider: Option<&str>,
    route_id: Option<&str>,
) -> ProviderRequirement {
    if let Some(route_id) = route_id {
        if let Ok(Some(route)) = load_frozen_route(connection, route_id) {
            return ProviderRequirement::for_job(
                ProviderRequirementCode::ProviderCommitted,
                Some(route.instance_id().clone()),
                Some(route.provider_kind().clone()),
                false,
            );
        }
    }
    ProviderRequirement::for_job(
        ProviderRequirementCode::ProviderCommitted,
        None,
        provider.and_then(parse_provider_kind),
        false,
    )
}

fn quarantine_queued_provider_job(
    transaction: &Transaction<'_>,
    id: &str,
    provider: Option<&str>,
    code: ProviderRequirementCode,
) -> JobResult<()> {
    let timestamp = now();
    let changed = transaction
        .execute(
            "UPDATE jobs
             SET state = 'paused', stage = 'provider_action_required', updated_at = ?2
             WHERE id = ?1 AND state = 'queued'",
            params![id, timestamp],
        )
        .map_err(|error| error.to_string())?;
    if changed != 1 {
        return Err("Provider job changed before quarantine completed".to_string());
    }
    let requirement =
        ProviderRequirement::for_job(code, None, provider.and_then(parse_provider_kind), false);
    upsert_provider_requirement(transaction, id, &requirement, &timestamp)
}

fn upsert_provider_requirement(
    transaction: &Transaction<'_>,
    job_id: &str,
    requirement: &ProviderRequirement,
    timestamp: &str,
) -> JobResult<()> {
    transaction
        .execute(
            "INSERT INTO job_provider_requirements(
               job_id, code, provider_kind, provider_instance_id,
               can_rebind, safe_details_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, '{}', ?6, ?6)
             ON CONFLICT(job_id) DO UPDATE SET
               code = excluded.code,
               provider_kind = excluded.provider_kind,
               provider_instance_id = excluded.provider_instance_id,
               can_rebind = excluded.can_rebind,
               safe_details_json = excluded.safe_details_json,
               updated_at = excluded.updated_at",
            params![
                job_id,
                requirement.database_code(),
                requirement.provider_kind().map(ProviderKind::as_str),
                requirement.instance_id().map(|id| id.as_str()),
                i64::from(requirement.can_rebind()),
                timestamp,
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn job_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<JobProjection> {
    let state = row.get::<_, String>(8)?;
    let payload = row.get::<_, String>(12)?;
    Ok(JobProjection {
        id: row.get(0)?,
        kind: row.get(1)?,
        provider: row.get(2)?,
        paper_id: row.get(3)?,
        revision_id: row.get(4)?,
        root_key: row.get(5)?,
        artifact_key: row.get(6)?,
        dedupe_key: row.get(7)?,
        state: JobState::from_database(&state).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                8,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
            )
        })?,
        stage: row.get(9)?,
        provider_committed: row.get::<_, i64>(10)? != 0,
        priority: row.get(11)?,
        payload: serde_json::from_str(&payload).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                12,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        last_error: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
        progress: progress_from_checkpoint(row.get::<_, Option<String>>(16)?),
        provider_route: None,
        provider_requirement: None,
        retry_disposition: RetryDisposition::Unavailable,
        retry_reason: "该旧版任务投影缺少可验证的执行路由，请回到来源重新生成".to_string(),
        source_locator: None,
    })
}

fn job_record_from_row(
    row: &rusqlite::Row<'_>,
    connection: &Connection,
) -> rusqlite::Result<JobRecord> {
    let projection = job_from_row(row)?;
    let route_id = row.get::<_, Option<String>>(17)?;
    let origin = row
        .get::<_, Option<String>>(18)?
        .map(|value| JobRouteOrigin::from_database(&value))
        .transpose()
        .map_err(|error| row_data_error(18, error))?;
    let provider_requirement = match row.get::<_, Option<String>>(19)? {
        Some(code) => {
            let provider_kind = row.get::<_, Option<String>>(20)?;
            let provider_instance_id = row.get::<_, Option<String>>(21)?;
            let can_rebind = row.get::<_, Option<i64>>(22)?.unwrap_or(0) != 0;
            let safe_details = row
                .get::<_, Option<String>>(23)?
                .ok_or_else(|| row_data_error(23, "Requirement details are missing"))?;
            let safe_details: Value =
                serde_json::from_str(&safe_details).map_err(|error| row_data_error(23, error))?;
            if !safe_details.is_object() {
                return Err(row_data_error(23, "Requirement details must be an object"));
            }
            Some(
                ProviderRequirement::from_database(
                    &code,
                    provider_instance_id.as_deref(),
                    provider_kind.as_deref(),
                    can_rebind,
                )
                .map_err(|error| row_data_error(19, error))?,
            )
        }
        None => None,
    };
    let unavailable_route = || StoredJobRoute::Unavailable {
        provider_kind: projection.provider.as_deref().and_then(parse_provider_kind),
    };
    let route = match route_id {
        Some(route_id) => {
            if origin.is_none() {
                if provider_requirement.is_some() {
                    unavailable_route()
                } else {
                    return Err(row_data_error(18, "Scoped jobs must retain a route origin"));
                }
            } else {
                match load_stored_execution_route(connection, &route_id, &projection) {
                    Ok(route) => StoredJobRoute::Executable(route),
                    Err(_error) if provider_requirement.is_some() => unavailable_route(),
                    Err(error) => return Err(row_data_error(17, error)),
                }
            }
        }
        None => match origin {
            None => StoredJobRoute::LegacyUnattributed,
            Some(JobRouteOrigin::Captured) => {
                if projection.provider.is_none() {
                    StoredJobRoute::Executable(JobExecutionRoute::Local)
                } else if provider_requirement.is_some() {
                    unavailable_route()
                } else {
                    return Err(row_data_error(
                        18,
                        "Captured remote work must retain an exact route snapshot",
                    ));
                }
            }
            Some(JobRouteOrigin::LegacyUniqueVerified | JobRouteOrigin::ExplicitRebind) => {
                if provider_requirement.is_some() {
                    unavailable_route()
                } else {
                    return Err(row_data_error(
                        18,
                        "Attributed provider jobs must retain a route snapshot",
                    ));
                }
            }
        },
    };
    Ok(JobRecord {
        id: projection.id,
        kind: projection.kind,
        provider: projection.provider,
        paper_id: projection.paper_id,
        revision_id: projection.revision_id,
        root_key: projection.root_key,
        artifact_key: projection.artifact_key,
        dedupe_key: projection.dedupe_key,
        state: projection.state,
        stage: projection.stage,
        provider_committed: projection.provider_committed,
        priority: projection.priority,
        payload: projection.payload,
        last_error: projection.last_error,
        created_at: projection.created_at,
        updated_at: projection.updated_at,
        progress: projection.progress,
        route,
        route_origin: origin,
        provider_requirement,
    })
}

fn row_data_error(index: usize, message: impl ToString) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            message.to_string(),
        )),
    )
}

fn parse_provider_kind(value: &str) -> Option<ProviderKind> {
    match value {
        "gemini" => Some(ProviderKind::Gemini),
        "openai_compatible" => Some(ProviderKind::OpenaiCompatible),
        "grok" => Some(ProviderKind::Grok),
        "gemini_proxy" => Some(ProviderKind::GeminiProxy),
        _ => None,
    }
}

fn progress_from_checkpoint(raw: Option<String>) -> Option<Value> {
    let raw = raw?;
    let value: Value = serde_json::from_str(&raw).ok()?;
    if !value.is_object() {
        return None;
    }
    let keys = [
        "step",
        "steps",
        "batch",
        "batches",
        "unitCount",
        "inputTokens",
        "outputTokens",
        "cachedInputTokens",
    ];
    if keys.iter().all(|key| value.get(*key).is_none()) {
        return None;
    }
    let mut progress = serde_json::Map::new();
    for key in keys {
        if let Some(item) = value.get(key) {
            progress.insert(key.to_string(), item.clone());
        }
    }
    Some(Value::Object(progress))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_settings::{
        paper_probe_fingerprint, PaperProbeRecord, ProviderInstance, ProviderKind,
        StoredModelSettings, MODEL_SETTINGS_SCHEMA,
    };
    use crate::provider_routing::{
        capture_mistral_ocr_route, FrozenModels, ModelRole, ProviderCredentialPort,
        ProviderInstanceId, ProviderRouteDecision, ProviderRouting, ProviderRoutingError,
    };
    use crate::v2_workspace::WorkspaceModule;
    use crate::GeminiModelOption;
    use rusqlite::params;
    use tempfile::TempDir;

    struct JobFixture {
        _root: TempDir,
        _workspace: WorkspaceModule,
        jobs: JobModule,
        database_path: PathBuf,
    }

    impl JobFixture {
        fn new() -> Self {
            let root = tempfile::tempdir().expect("temporary workspace");
            let workspace = WorkspaceModule::new();
            let projection = workspace.open(root.path()).expect("open workspace");
            let jobs = JobModule::open(&projection.database_path).expect("open job module");
            Self {
                _root: root,
                _workspace: workspace,
                jobs,
                database_path: projection.database_path,
            }
        }

        fn spec(kind: &str, provider: Option<&str>, dedupe_key: &str) -> JobSpec {
            JobSpec {
                kind: kind.to_string(),
                provider: provider.map(str::to_string),
                paper_id: None,
                revision_id: None,
                root_key: None,
                artifact_key: None,
                dedupe_key: dedupe_key.to_string(),
                priority: 0,
                payload: serde_json::json!({}),
            }
        }

        fn seed_paper(&self, id: &str) {
            let connection = Connection::open(&self.database_path).expect("database");
            let timestamp = now();
            connection
                .execute(
                    "INSERT INTO collections(id, name, relative_path, created_at, updated_at)
                     VALUES ('inbox', 'Inbox', 'Inbox', ?1, ?1)
                     ON CONFLICT(id) DO NOTHING",
                    params![timestamp],
                )
                .expect("collection");
            connection
                .execute(
                    "INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                     VALUES (?1, 'inbox', 'paper.pdf', ?2, ?3, ?3)",
                    params![id, format!("Inbox/{id}.pdf"), timestamp],
                )
                .expect("paper");
        }

        fn enable_v7_job_schema(&self) {
            let connection = Connection::open(&self.database_path).expect("database");
            let already_enabled = connection
                .query_row(
                    "SELECT EXISTS(
                       SELECT 1 FROM pragma_table_info('jobs')
                       WHERE name = 'provider_route_id'
                     )",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("inspect jobs schema")
                != 0;
            if already_enabled {
                return;
            }
            connection
                .execute_batch(
                    "DROP INDEX idx_jobs_active_dedupe;
                     CREATE TABLE remote_endpoint_snapshots (
                       endpoint_scope TEXT PRIMARY KEY,
                       version INTEGER NOT NULL,
                       owner_type TEXT NOT NULL,
                       provider_instance_id TEXT,
                       provider_name_at_capture TEXT,
                       provider_kind TEXT,
                       base_url TEXT,
                       created_at TEXT NOT NULL
                     );
                     CREATE TABLE provider_route_snapshots (
                       route_id TEXT PRIMARY KEY,
                       endpoint_scope TEXT NOT NULL REFERENCES remote_endpoint_snapshots(endpoint_scope),
                       version INTEGER NOT NULL,
                       models_json TEXT NOT NULL,
                       operation_role TEXT NOT NULL,
                       created_at TEXT NOT NULL,
                       UNIQUE(endpoint_scope, models_json, operation_role)
                     );
                     ALTER TABLE jobs ADD COLUMN provider_route_id TEXT
                       REFERENCES provider_route_snapshots(route_id);
                     ALTER TABLE jobs ADD COLUMN provider_route_origin TEXT;
                     CREATE TABLE job_provider_requirements (
                       job_id TEXT PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
                       code TEXT NOT NULL,
                       provider_kind TEXT,
                       provider_instance_id TEXT,
                       can_rebind INTEGER NOT NULL,
                       safe_details_json TEXT NOT NULL DEFAULT '{}',
                       created_at TEXT NOT NULL,
                       updated_at TEXT NOT NULL
                     );
                     CREATE UNIQUE INDEX jobs_active_route_dedupe
                       ON jobs(provider_route_id, dedupe_key)
                       WHERE provider_route_id IS NOT NULL
                         AND state IN ('queued', 'running', 'paused', 'interrupted_unknown');
                     CREATE UNIQUE INDEX jobs_active_legacy_dedupe
                       ON jobs(dedupe_key)
                       WHERE provider_route_id IS NULL
                         AND state IN ('queued', 'running', 'paused');
                     CREATE INDEX jobs_active_legacy_dedupe_lookup
                       ON jobs(dedupe_key, state)
                       WHERE provider_route_id IS NULL
                         AND state IN ('queued', 'running', 'paused', 'interrupted_unknown');",
                )
                .expect("v7 job schema");
        }

        fn bound_route(
            id: &str,
            name: &str,
            key: &str,
            base_url: &str,
        ) -> crate::provider_routing::BoundProviderRoute {
            let paper_model = "gpt-4.1".to_string();
            let translation_model = "gpt-4.1-mini".to_string();
            let instance = ProviderInstance {
                id: id.to_string(),
                name: name.to_string(),
                kind: ProviderKind::OpenaiCompatible,
                base_url: Some(base_url.to_string()),
                paper_model: paper_model.clone(),
                translation_model: translation_model.clone(),
                models: Vec::<GeminiModelOption>::new(),
                models_fetched_at: None,
                connection_verified_at: Some("2026-08-24T00:00:00Z".to_string()),
                paper_probe: Some(PaperProbeRecord {
                    fingerprint: paper_probe_fingerprint(base_url, key, &paper_model),
                    passed_at: "2026-08-24T00:00:00Z".to_string(),
                    paper_model: paper_model.clone(),
                }),
                sort_order: 0,
            };
            let settings = StoredModelSettings {
                schema_version: MODEL_SETTINGS_SCHEMA,
                current_provider_id: Some(id.to_string()),
                providers: vec![instance],
            };
            let credentials = RouteCredential {
                id: id.to_string(),
                key: key.to_string(),
            };
            match ProviderRouting::new(&settings, &credentials)
                .capture(
                    crate::provider_routing::ProviderSelection::Current,
                    FrozenModels::new(paper_model, Some(translation_model)).expect("models"),
                    ModelRole::Paper,
                )
                .expect("capture")
            {
                ProviderRouteDecision::Ready(bound) => bound,
                ProviderRouteDecision::ActionRequired(requirement) => {
                    panic!("route unexpectedly unavailable: {requirement:?}")
                }
            }
        }

        fn frozen_route(
            id: &str,
            name: &str,
            key: &str,
            base_url: &str,
        ) -> crate::provider_routing::FrozenProviderRoute {
            Self::bound_route(id, name, key, base_url).freeze()
        }

        fn default_paper_route() -> crate::provider_routing::FrozenProviderRoute {
            Self::frozen_route(
                "99999999-9999-4999-8999-999999999999",
                "Test account",
                "test-paper-key",
                "https://test.example.com/v1",
            )
        }

        fn enqueue_paper(&self, mut spec: JobSpec) -> JobResult<EnqueueResult> {
            spec.provider = Some(ProviderKind::OpenaiCompatible.as_str().to_string());
            self.jobs
                .enqueue_record(spec, JobExecutionRoute::Paper(Self::default_paper_route()))
                .map(|result| EnqueueResult {
                    job: result.job.project(),
                    coalesced: result.coalesced,
                })
        }

        fn enqueue_mistral(&self, mut spec: JobSpec) -> JobResult<EnqueueResult> {
            spec.provider = Some("mistral".to_string());
            let model = spec
                .payload
                .get("model")
                .and_then(Value::as_str)
                .unwrap_or("mistral-ocr-latest");
            let route = capture_mistral_ocr_route("test-mistral-key", model)
                .map_err(|error| error.to_string())?;
            self.jobs
                .enqueue_record(spec, JobExecutionRoute::MistralOcr(route))
                .map(|result| EnqueueResult {
                    job: result.job.project(),
                    coalesced: result.coalesced,
                })
        }

        fn stored_job_bytes(&self, id: &str) -> Vec<u8> {
            let connection = Connection::open(&self.database_path).expect("database");
            let job: String = connection
                .query_row(
                    "SELECT printf(
                       '%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q',
                       id, kind, provider, paper_id, revision_id, root_key, artifact_key,
                       dedupe_key, state, stage, provider_committed, priority, payload_json,
                       last_error, created_at, updated_at, provider_route_id, provider_route_origin
                     ) FROM jobs WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .expect("job bytes");
            let requirement: Option<String> = connection
                .query_row(
                    "SELECT printf(
                       '%Q|%Q|%Q|%Q|%Q|%Q|%Q|%Q',
                       job_id, code, provider_kind, provider_instance_id,
                       can_rebind, safe_details_json, created_at, updated_at
                     ) FROM job_provider_requirements WHERE job_id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .optional()
                .expect("requirement bytes");
            format!("{job}\n{}", requirement.unwrap_or_default()).into_bytes()
        }
    }

    struct RouteCredential {
        id: String,
        key: String,
    }

    impl ProviderCredentialPort for RouteCredential {
        fn read_exact(
            &self,
            instance_id: &ProviderInstanceId,
        ) -> Result<Option<String>, ProviderRoutingError> {
            Ok((instance_id.as_str() == self.id).then(|| self.key.clone()))
        }
    }

    #[test]
    fn retry_disposition_is_backend_authoritative_and_fails_closed() {
        let fixture = JobFixture::new();
        fixture.enable_v7_job_schema();

        let safe = fixture
            .enqueue_paper(JobFixture::spec("orientation_pack", None, "retry-safe"))
            .expect("enqueue safe paper job")
            .job;
        let claimed = fixture
            .jobs
            .claim_next_record()
            .expect("claim safe job")
            .expect("safe job available");
        assert_eq!(claimed.id, safe.id);
        fixture
            .jobs
            .fail(&safe.id, "local preflight failed")
            .expect("fail");
        let safe_record = fixture.jobs.get_record(&safe.id).expect("safe record");
        assert_eq!(safe_record.retry_disposition().0, RetryDisposition::Safe);

        let charged = fixture
            .enqueue_paper(JobFixture::spec("outline_overview", None, "retry-charge"))
            .expect("enqueue charged paper job")
            .job;
        let claimed = fixture
            .jobs
            .claim_next_record()
            .expect("claim charged job")
            .expect("charged job available");
        assert_eq!(claimed.id, charged.id);
        fixture
            .jobs
            .mark_provider_committed(&charged.id, Some("request-1"))
            .expect("mark provider committed");
        fixture
            .jobs
            .fail(&charged.id, "connection lost after submit")
            .expect("fail charged job");
        let charged_record = fixture
            .jobs
            .get_record(&charged.id)
            .expect("charged record");
        assert_eq!(
            charged_record.retry_disposition().0,
            RetryDisposition::ConfirmPossibleCharge
        );

        let local = fixture
            .jobs
            .enqueue_record(
                JobFixture::spec("local_cleanup", None, "retry-local"),
                JobExecutionRoute::Local,
            )
            .expect("enqueue local job")
            .job
            .project();
        let claimed = fixture
            .jobs
            .claim_next_record()
            .expect("claim local job")
            .expect("local job available");
        assert_eq!(claimed.id, local.id);
        fixture
            .jobs
            .fail(&local.id, "no worker")
            .expect("fail local");
        let local_record = fixture.jobs.get_record(&local.id).expect("local record");
        assert_eq!(
            local_record.retry_disposition().0,
            RetryDisposition::Unavailable
        );
    }

    #[test]
    fn route_aware_enqueue_scopes_dedupe_and_round_trips_one_record_mapper() {
        let fixture = JobFixture::new();
        fixture.enable_v7_job_schema();
        let route_a = JobFixture::frozen_route(
            "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "Account A",
            "key-a",
            "https://a.example.com/private/v1",
        );
        let route_b = JobFixture::frozen_route(
            "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
            "Account B",
            "key-b",
            "https://b.example.com/private/v1",
        );
        let spec = JobFixture::spec("artifact", Some("openai_compatible"), "artifact:paper-1");

        let first = fixture
            .jobs
            .enqueue_record(spec.clone(), JobExecutionRoute::Paper(route_a.clone()))
            .expect("route A");
        let duplicate = fixture
            .jobs
            .enqueue_record(spec.clone(), JobExecutionRoute::Paper(route_a.clone()))
            .expect("same route");
        let second = fixture
            .jobs
            .enqueue_record(spec, JobExecutionRoute::Paper(route_b.clone()))
            .expect("route B");

        assert!(!first.coalesced);
        assert!(duplicate.coalesced);
        assert_eq!(duplicate.job.id, first.job.id);
        assert!(!second.coalesced);
        assert_ne!(second.job.id, first.job.id);
        assert_eq!(
            fixture
                .jobs
                .get_record(&first.job.id)
                .expect("record")
                .route,
            StoredJobRoute::Executable(JobExecutionRoute::Paper(route_a.clone()))
        );
        assert_eq!(fixture.jobs.list_records().expect("records").len(), 2);

        let projection = serde_json::to_string(&first.job.project()).expect("projection");
        assert!(projection.contains("Account A"));
        assert!(projection.contains("a.example.com"));
        assert!(!projection.contains("key-a"));
        assert!(!projection.contains("/private/v1"));
        assert!(!projection.contains(&route_a.route_id().database_value()));
    }

    #[test]
    fn provider_requirement_atomically_blocks_claim_and_guards_resume_until_recheck() {
        let fixture = JobFixture::new();
        fixture.enable_v7_job_schema();
        let instance_id = "cccccccc-cccc-4ccc-8ccc-cccccccccccc";
        let route = JobFixture::frozen_route(
            instance_id,
            "Account C",
            "key-c",
            "https://c.example.com/v1",
        );
        let enqueued = fixture
            .jobs
            .enqueue_record(
                JobFixture::spec(
                    "artifact",
                    Some("openai_compatible"),
                    "artifact:requirement",
                ),
                JobExecutionRoute::Paper(route.clone()),
            )
            .expect("enqueue")
            .job;
        let claimed = fixture
            .jobs
            .claim_next_record()
            .expect("claim")
            .expect("claimed");
        assert_eq!(claimed.id, enqueued.id);

        let requirement = crate::provider_routing::ProviderRequirement::for_job(
            crate::provider_routing::ProviderRequirementCode::CredentialMissing,
            Some(ProviderInstanceId::parse(instance_id).expect("instance")),
            Some(ProviderKind::OpenaiCompatible),
            true,
        );
        let blocked = fixture
            .jobs
            .block_for_provider_action(&enqueued.id, requirement)
            .expect("block");
        assert_eq!(blocked.state, JobState::Paused);
        assert_eq!(blocked.stage, "provider_action_required");
        assert_eq!(
            blocked
                .provider_requirement
                .as_ref()
                .map(crate::provider_routing::ProviderRequirement::code),
            Some(crate::provider_routing::ProviderRequirementCode::CredentialMissing)
        );
        assert!(fixture.jobs.resume_record(&enqueued.id).is_err());
        assert!(fixture
            .jobs
            .claim_next_record()
            .expect("blocked claim")
            .is_none());

        let connection = Connection::open(&fixture.database_path).expect("database");
        let attempt: (String, Option<String>) = connection
            .query_row(
                "SELECT state, finished_at FROM job_attempts WHERE job_id = ?1",
                params![enqueued.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("attempt");
        assert_eq!(attempt.0, "blocked");
        assert!(attempt.1.is_some());
        drop(connection);

        fixture
            .jobs
            .clear_provider_requirement_after_recheck(&enqueued.id, &route)
            .expect("ready route");
        let reclaimed = fixture
            .jobs
            .claim_next_record()
            .expect("reclaim")
            .expect("reclaimed");
        assert_eq!(reclaimed.id, enqueued.id);
        assert!(reclaimed.provider_requirement.is_none());
        assert_eq!(
            reclaimed.route,
            StoredJobRoute::Executable(JobExecutionRoute::Paper(route))
        );
    }

    #[test]
    fn corrupt_route_is_quarantined_before_claim_without_leaving_running_attempt() {
        let fixture = JobFixture::new();
        fixture.enable_v7_job_schema();
        let route = JobFixture::frozen_route(
            "12121212-1212-4121-8121-121212121212",
            "Corrupt route",
            "key-corrupt",
            "https://corrupt.example.com/v1",
        );
        let enqueued = fixture
            .jobs
            .enqueue_record(
                JobFixture::spec(
                    "artifact",
                    Some("openai_compatible"),
                    "artifact:corrupt-route",
                ),
                JobExecutionRoute::Paper(route.clone()),
            )
            .expect("enqueue")
            .job;
        let connection = Connection::open(&fixture.database_path).expect("database");
        connection
            .execute(
                "UPDATE provider_route_snapshots SET version = 99 WHERE route_id = ?1",
                params![route.route_id().database_value()],
            )
            .expect("corrupt route version");
        drop(connection);

        assert!(fixture
            .jobs
            .claim_next_record()
            .expect("claim must fail closed")
            .is_none());
        let quarantined = fixture.jobs.get(&enqueued.id).expect("safe projection");
        assert_eq!(quarantined.state, JobState::Paused);
        assert_eq!(quarantined.stage, "provider_action_required");
        assert_eq!(
            quarantined
                .provider_requirement
                .as_ref()
                .map(|requirement| requirement.code),
            Some(ProviderRequirementCode::InvalidSnapshot)
        );
        let connection = Connection::open(&fixture.database_path).expect("database");
        let running_attempts: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM job_attempts WHERE job_id = ?1",
                params![enqueued.id],
                |row| row.get(0),
            )
            .expect("attempt count");
        assert_eq!(running_attempts, 0);
    }

    #[test]
    fn interrupted_scoped_recheck_splits_committed_from_uncommitted_work() {
        let fixture = JobFixture::new();
        fixture.enable_v7_job_schema();
        let route = JobFixture::frozen_route(
            "ffffffff-ffff-4fff-8fff-ffffffffffff",
            "Account F",
            "key-f",
            "https://f.example.com/v1",
        );
        let committed = fixture
            .jobs
            .enqueue_record(
                JobFixture::spec(
                    "artifact",
                    Some("openai_compatible"),
                    "artifact:committed-interrupted",
                ),
                JobExecutionRoute::Paper(route.clone()),
            )
            .expect("committed enqueue")
            .job;
        fixture
            .jobs
            .claim_next_record()
            .expect("claim committed")
            .expect("committed running");
        fixture
            .jobs
            .mark_provider_committed(&committed.id, Some("request-committed"))
            .expect("mark committed");
        fixture
            .jobs
            .block_for_provider_action(
                &committed.id,
                crate::provider_routing::ProviderRequirement::for_job(
                    crate::provider_routing::ProviderRequirementCode::CredentialMissing,
                    None,
                    Some(ProviderKind::OpenaiCompatible),
                    false,
                ),
            )
            .expect("block committed");
        let connection = Connection::open(&fixture.database_path).expect("database");
        connection
            .execute(
                "UPDATE jobs
                 SET state = 'interrupted_unknown', stage = 'interrupted_unknown'
                 WHERE id = ?1",
                params![committed.id],
            )
            .expect("interrupt committed");
        drop(connection);

        assert!(fixture
            .jobs
            .clear_provider_requirement_after_recheck(&committed.id, &route)
            .is_err());
        let quarantined = fixture.jobs.get_record(&committed.id).expect("quarantine");
        assert_eq!(quarantined.state, JobState::InterruptedUnknown);
        assert_eq!(
            quarantined
                .provider_requirement
                .as_ref()
                .map(crate::provider_routing::ProviderRequirement::code),
            Some(crate::provider_routing::ProviderRequirementCode::ProviderCommitted)
        );

        let uncommitted = fixture
            .jobs
            .enqueue_record(
                JobFixture::spec(
                    "artifact",
                    Some("openai_compatible"),
                    "artifact:uncommitted-interrupted",
                ),
                JobExecutionRoute::Paper(route.clone()),
            )
            .expect("uncommitted enqueue")
            .job;
        fixture
            .jobs
            .claim_next_record()
            .expect("claim uncommitted")
            .expect("uncommitted running");
        fixture
            .jobs
            .block_for_provider_action(
                &uncommitted.id,
                crate::provider_routing::ProviderRequirement::for_job(
                    crate::provider_routing::ProviderRequirementCode::CredentialMissing,
                    None,
                    Some(ProviderKind::OpenaiCompatible),
                    true,
                ),
            )
            .expect("block uncommitted");
        let connection = Connection::open(&fixture.database_path).expect("database");
        connection
            .execute(
                "UPDATE jobs
                 SET state = 'interrupted_unknown', stage = 'interrupted_unknown'
                 WHERE id = ?1",
                params![uncommitted.id],
            )
            .expect("interrupt uncommitted");
        drop(connection);
        let requeued = fixture
            .jobs
            .clear_provider_requirement_after_recheck(&uncommitted.id, &route)
            .expect("safe uncommitted recheck");
        assert_eq!(requeued.state, JobState::Queued);
        assert!(requeued.provider_requirement.is_none());
    }

    #[test]
    fn rebind_conflict_returns_existing_job_and_preserves_original_bytes() {
        let fixture = JobFixture::new();
        fixture.enable_v7_job_schema();
        let route_a = JobFixture::frozen_route(
            "dddddddd-dddd-4ddd-8ddd-dddddddddddd",
            "Account D",
            "key-d",
            "https://d.example.com/v1",
        );
        let bound_b = JobFixture::bound_route(
            "eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee",
            "Account E",
            "key-e",
            "https://e.example.com/v1",
        );
        let route_b = bound_b.freeze();
        let spec = JobFixture::spec(
            "artifact",
            Some("openai_compatible"),
            "artifact:rebind-conflict",
        );
        let original = fixture
            .jobs
            .enqueue_record(spec.clone(), JobExecutionRoute::Paper(route_a))
            .expect("original")
            .job;
        fixture
            .jobs
            .claim_next_record()
            .expect("claim")
            .expect("running");
        fixture
            .jobs
            .block_for_provider_action(
                &original.id,
                crate::provider_routing::ProviderRequirement::for_job(
                    crate::provider_routing::ProviderRequirementCode::CredentialChanged,
                    None,
                    Some(ProviderKind::OpenaiCompatible),
                    true,
                ),
            )
            .expect("block");
        let existing = fixture
            .jobs
            .enqueue_record(spec, JobExecutionRoute::Paper(route_b))
            .expect("target route")
            .job;
        let before = fixture.stored_job_bytes(&original.id);

        let error = fixture
            .jobs
            .rebind_provider_route(&original.id, &bound_b)
            .expect_err("target route is already active");

        assert_eq!(
            error,
            RebindProviderRouteError::AlreadyActiveOnRoute {
                existing_job_id: existing.id,
            }
        );
        assert_eq!(fixture.stored_job_bytes(&original.id), before);
        let unchanged = fixture.jobs.get_record(&original.id).expect("original");
        assert_eq!(unchanged.state, JobState::Paused);
        assert!(unchanged.provider_requirement.is_some());
    }

    #[test]
    fn abandoning_legacy_unknown_work_is_local_only_and_releases_active_dedupe() {
        let fixture = JobFixture::new();
        fixture.enable_v7_job_schema();
        let connection = Connection::open(&fixture.database_path).expect("database");
        let timestamp = now();
        connection
            .execute(
                "INSERT INTO jobs(
                   id, kind, provider, dedupe_key, state, stage, provider_committed,
                   priority, payload_json, created_at, updated_at,
                   provider_route_id, provider_route_origin
                 ) VALUES (
                   'legacy-job', 'artifact', 'openai_compatible', 'legacy:dedupe',
                   'interrupted_unknown', 'interrupted_unknown', 1, 0, '{}', ?1, ?1,
                   NULL, NULL
                 )",
                params![timestamp],
            )
            .expect("legacy job");
        connection
            .execute(
                "INSERT INTO job_provider_requirements(
                   job_id, code, provider_kind, provider_instance_id,
                   can_rebind, safe_details_json, created_at, updated_at
                 ) VALUES (
                   'legacy-job', 'quarantined', 'openai_compatible', NULL,
                   0, '{}', ?1, ?1
                 )",
                params![timestamp],
            )
            .expect("requirement");
        connection
            .execute(
                "INSERT INTO remote_tombstones(
                   id, provider, resource_kind, remote_id, state, attempts,
                   created_at, updated_at
                 ) VALUES (
                   'legacy-tombstone', 'openai_compatible', 'file', 'remote-1',
                   'pending', 0, ?1, ?1
                 )",
                params![timestamp],
            )
            .expect("tombstone");
        drop(connection);

        assert!(fixture
            .jobs
            .abandon_legacy_provider_job("legacy-job", false)
            .is_err());
        let abandoned = fixture
            .jobs
            .abandon_legacy_provider_job("legacy-job", true)
            .expect("confirmed abandon");
        assert_eq!(abandoned.state, JobState::Cancelled);
        assert_eq!(abandoned.stage, "provider_job_abandoned");
        assert!(abandoned.provider_requirement.is_none());
        assert_eq!(abandoned.route, StoredJobRoute::LegacyUnattributed);

        let connection = Connection::open(&fixture.database_path).expect("database");
        let tombstone: (String, i64, Option<String>) = connection
            .query_row(
                "SELECT state, attempts, last_error
                 FROM remote_tombstones WHERE id = 'legacy-tombstone'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("unchanged tombstone");
        assert_eq!(tombstone, ("pending".to_string(), 0, None));
        drop(connection);

        let replacement = fixture
            .jobs
            .enqueue_record(
                JobFixture::spec("local", None, "legacy:dedupe"),
                JobExecutionRoute::Local,
            )
            .expect("dedupe released");
        assert!(!replacement.coalesced);
        assert_ne!(replacement.job.id, "legacy-job");
    }

    #[test]
    fn frozen_mistral_ocr_route_round_trips_through_record_and_claim_mappers() {
        let fixture = JobFixture::new();
        fixture.enable_v7_job_schema();
        let mut spec = JobFixture::spec("ocr", Some("mistral"), "ocr:frozen-route");
        spec.payload = serde_json::json!({ "model": "mistral-ocr-latest" });
        let route =
            capture_mistral_ocr_route("key-before-change", "mistral-ocr-latest").expect("route");
        let changed_key_route = capture_mistral_ocr_route("key-after-change", "mistral-ocr-latest")
            .expect("changed key route");
        assert_ne!(route.route_id(), changed_key_route.route_id());
        assert_ne!(
            route.endpoint_scope_database_value(),
            changed_key_route.endpoint_scope_database_value()
        );

        let enqueued = fixture
            .jobs
            .enqueue_record(spec, JobExecutionRoute::MistralOcr(route.clone()))
            .expect("enqueue frozen OCR route")
            .job;
        assert_eq!(
            enqueued.route,
            StoredJobRoute::Executable(JobExecutionRoute::MistralOcr(route.clone()))
        );
        let loaded = fixture.jobs.get_record(&enqueued.id).expect("load record");
        assert_eq!(
            loaded.route,
            StoredJobRoute::Executable(JobExecutionRoute::MistralOcr(route.clone()))
        );
        fixture
            .jobs
            .assert_active_route_invariants()
            .expect("valid queued OCR route");

        let claimed = fixture
            .jobs
            .claim_next_record()
            .expect("claim")
            .expect("frozen OCR job");
        assert_eq!(claimed.id, enqueued.id);
        assert_eq!(
            claimed.route,
            StoredJobRoute::Executable(JobExecutionRoute::MistralOcr(route))
        );
    }

    #[test]
    fn startup_reconciliation_adopts_only_unique_uncommitted_legacy_routes() {
        let fixture = JobFixture::new();
        fixture.enable_v7_job_schema();
        let route_a = JobFixture::frozen_route(
            "11111111-1111-4111-8111-111111111111",
            "Legacy account A",
            "key-a",
            "https://legacy.example.com/v1",
        );
        let route_b = JobFixture::frozen_route(
            "22222222-2222-4222-8222-222222222222",
            "Legacy account B",
            "key-b",
            "https://legacy.example.com/v1",
        );
        let payload = serde_json::json!({
            "provider": "openai_compatible",
            "baseUrl": "https://legacy.example.com/v1",
            "model": "gpt-4.1"
        })
        .to_string();
        let timestamp = now();
        let connection = Connection::open(&fixture.database_path).expect("database");
        connection
            .execute(
                "INSERT INTO jobs(
                   id, kind, provider, dedupe_key, state, stage, provider_committed,
                   priority, payload_json, created_at, updated_at,
                   provider_route_id, provider_route_origin
                 ) VALUES (
                   'legacy-unique', 'outline_overview', 'openai_compatible',
                   'legacy:unique', 'queued', 'queued', 0, 0, ?1, ?2, ?2, NULL, NULL
                 )",
                params![payload, timestamp],
            )
            .expect("legacy unique job");
        drop(connection);

        let adopted = fixture
            .jobs
            .reconcile_legacy_provider_jobs(&[route_a.clone()])
            .expect("reconcile uniquely attributable job");
        assert_eq!(adopted.adopted, 1);
        assert_eq!(adopted.quarantined, 0);
        let unique = fixture
            .jobs
            .get_record("legacy-unique")
            .expect("adopted record");
        assert_eq!(unique.state, JobState::Queued);
        assert_eq!(
            unique.route_origin,
            Some(JobRouteOrigin::LegacyUniqueVerified)
        );
        assert_eq!(
            unique.route,
            StoredJobRoute::Executable(JobExecutionRoute::Paper(route_a.clone()))
        );
        assert!(unique.provider_requirement.is_none());

        let connection = Connection::open(&fixture.database_path).expect("database");
        for (id, state, provider_committed) in [
            ("legacy-ambiguous", "queued", 0_i64),
            ("legacy-committed", "running", 1_i64),
        ] {
            connection
                .execute(
                    "INSERT INTO jobs(
                       id, kind, provider, dedupe_key, state, stage, provider_committed,
                       priority, payload_json, created_at, updated_at,
                       provider_route_id, provider_route_origin
                     ) VALUES (
                       ?1, 'outline_overview', 'openai_compatible', ?2, ?3, ?3, ?4,
                       0, ?5, ?6, ?6, NULL, NULL
                     )",
                    params![
                        id,
                        format!("{id}:dedupe"),
                        state,
                        provider_committed,
                        payload,
                        now()
                    ],
                )
                .expect("legacy provider job");
        }
        drop(connection);

        let isolated = fixture
            .jobs
            .reconcile_legacy_provider_jobs(&[route_a.clone(), route_b])
            .expect("isolate unsafe legacy jobs");
        assert_eq!(isolated.adopted, 0);
        assert_eq!(isolated.quarantined, 2);

        let ambiguous = fixture
            .jobs
            .get_record("legacy-ambiguous")
            .expect("ambiguous record");
        assert_eq!(ambiguous.state, JobState::Paused);
        assert_eq!(ambiguous.route, StoredJobRoute::LegacyUnattributed);
        assert_eq!(
            ambiguous
                .provider_requirement
                .as_ref()
                .map(ProviderRequirement::code),
            Some(ProviderRequirementCode::LegacyAmbiguous)
        );
        assert!(fixture.jobs.resume_record("legacy-ambiguous").is_err());

        let committed = fixture
            .jobs
            .get_record("legacy-committed")
            .expect("committed record");
        assert_eq!(committed.state, JobState::InterruptedUnknown);
        assert_eq!(committed.route, StoredJobRoute::LegacyUnattributed);
        assert_eq!(
            committed
                .provider_requirement
                .as_ref()
                .map(ProviderRequirement::code),
            Some(ProviderRequirementCode::ProviderCommitted)
        );
        fixture
            .jobs
            .assert_active_route_invariants()
            .expect("reconciled jobs satisfy active-route invariant");
    }

    #[test]
    fn coalesces_active_jobs_with_the_same_logical_key() {
        let fixture = JobFixture::new();
        let first = fixture
            .jobs
            .enqueue(JobFixture::spec(
                "artifact",
                Some("gemini"),
                "brief:paper-1",
            ))
            .expect("first enqueue");
        let second = fixture
            .jobs
            .enqueue(JobFixture::spec(
                "artifact",
                Some("gemini"),
                "brief:paper-1",
            ))
            .expect("second enqueue");

        assert!(!first.coalesced);
        assert!(second.coalesced);
        assert_eq!(first.job.id, second.job.id);
        assert_eq!(fixture.jobs.list().expect("jobs").len(), 1);
    }

    #[test]
    fn reprioritize_changes_claim_order_and_rejects_running_work() {
        let fixture = JobFixture::new();
        let first = fixture
            .enqueue_paper(JobFixture::spec(
                "artifact",
                Some("gemini"),
                "priority:first",
            ))
            .expect("first")
            .job;
        let second = fixture
            .enqueue_paper(JobFixture::spec(
                "artifact",
                Some("gemini"),
                "priority:second",
            ))
            .expect("second")
            .job;
        fixture
            .jobs
            .reprioritize(&second.id, 200)
            .expect("reprioritize");
        assert_eq!(fixture.jobs.get(&second.id).expect("updated").priority, 200);
        let claimed = fixture.jobs.claim_next().expect("claim").expect("job");
        assert_eq!(claimed.id, second.id);
        assert!(fixture.jobs.reprioritize(&claimed.id, 0).is_err());
        assert_eq!(
            fixture.jobs.get(&first.id).expect("first queued").state,
            JobState::Queued
        );
    }

    #[test]
    fn limits_paid_provider_work_to_two_running_requests() {
        let fixture = JobFixture::new();
        for index in 0..3 {
            fixture
                .enqueue_paper(JobFixture::spec(
                    "artifact",
                    Some("gemini"),
                    &format!("artifact:{index}"),
                ))
                .expect("enqueue");
        }

        let first = fixture.jobs.claim_next().expect("claim").expect("first");
        let _second = fixture.jobs.claim_next().expect("claim").expect("second");
        assert!(fixture.jobs.claim_next().expect("claim").is_none());

        fixture.jobs.complete(&first.id).expect("complete");
        assert!(fixture.jobs.claim_next().expect("claim").is_some());
    }

    #[test]
    fn serializes_ocr_for_the_same_paper() {
        let fixture = JobFixture::new();
        fixture.seed_paper("paper-1");
        for index in 0..2 {
            let mut spec = JobFixture::spec("ocr", Some("mistral"), &format!("ocr:{index}"));
            spec.paper_id = Some("paper-1".to_string());
            fixture.enqueue_mistral(spec).expect("enqueue");
        }

        let first = fixture.jobs.claim_next().expect("claim").expect("first");
        assert!(fixture.jobs.claim_next().expect("claim").is_none());
        fixture.jobs.complete(&first.id).expect("complete");
        assert!(fixture.jobs.claim_next().expect("claim").is_some());
    }

    #[test]
    fn pause_exit_preserves_safe_work_and_quarantines_unknown_paid_work() {
        let fixture = JobFixture::new();
        let safe = fixture
            .enqueue_paper(JobFixture::spec("artifact", Some("gemini"), "safe"))
            .expect("safe enqueue")
            .job;
        fixture
            .jobs
            .claim_next()
            .expect("claim")
            .expect("safe running");

        let paid = fixture
            .enqueue_paper(JobFixture::spec("artifact", Some("gemini"), "paid"))
            .expect("paid enqueue")
            .job;
        fixture
            .jobs
            .claim_next()
            .expect("claim")
            .expect("paid running");
        fixture
            .jobs
            .mark_provider_committed(&paid.id, Some("request-paid"))
            .expect("provider commit");

        let queued = fixture
            .enqueue_paper(JobFixture::spec("artifact", Some("gemini"), "queued"))
            .expect("queued enqueue")
            .job;
        let prepared = fixture
            .jobs
            .prepare_for_pause_exit()
            .expect("prepare pause exit");
        assert_eq!(prepared.paused, 2);
        assert_eq!(prepared.interrupted_unknown, 1);
        assert_eq!(
            fixture.jobs.get(&safe.id).expect("safe").state,
            JobState::Paused
        );
        assert_eq!(
            fixture.jobs.get(&paid.id).expect("paid").state,
            JobState::InterruptedUnknown
        );
        assert_eq!(
            fixture.jobs.get(&queued.id).expect("queued").state,
            JobState::Paused
        );

        let reopened = JobModule::open(&fixture.database_path).expect("reopen");
        assert_eq!(
            reopened.get(&safe.id).expect("safe").state,
            JobState::Paused
        );
        assert_eq!(
            reopened.get(&paid.id).expect("paid").state,
            JobState::InterruptedUnknown
        );
    }

    #[test]
    fn cancel_exit_atomically_cancels_queued_paused_and_running_jobs() {
        let fixture = JobFixture::new();
        let running = fixture
            .enqueue_paper(JobFixture::spec("artifact", Some("gemini"), "running"))
            .expect("running enqueue")
            .job;
        fixture.jobs.claim_next().expect("claim").expect("running");
        let paused = fixture
            .enqueue_paper(JobFixture::spec("artifact", Some("gemini"), "paused"))
            .expect("paused enqueue")
            .job;
        fixture.jobs.pause(&paused.id).expect("pause");
        let queued = fixture
            .enqueue_paper(JobFixture::spec(
                "artifact",
                Some("gemini"),
                "queued-cancel",
            ))
            .expect("queued enqueue")
            .job;

        let prepared = fixture.jobs.cancel_all_active().expect("cancel active");
        assert_eq!(prepared.cancelled, 3);
        for id in [running.id, paused.id, queued.id] {
            assert_eq!(
                fixture.jobs.get(&id).expect("cancelled").state,
                JobState::Cancelled
            );
        }
    }

    #[test]
    fn recovery_requeues_uncommitted_work_and_quarantines_unknown_paid_work() {
        let fixture = JobFixture::new();
        let retry = fixture
            .enqueue_paper(JobFixture::spec("artifact", Some("gemini"), "retry"))
            .expect("enqueue")
            .job;
        let claimed_retry = fixture.jobs.claim_next().expect("claim").expect("retry");
        assert_eq!(retry.id, claimed_retry.id);

        let recovered = JobModule::open(&fixture.database_path).expect("recover");
        assert_eq!(
            recovered.get(&retry.id).expect("unrecovered job").state,
            JobState::Running
        );
        recovered.recover_for_startup().expect("recover startup");
        assert_eq!(
            recovered.get(&retry.id).expect("retry job").state,
            JobState::Queued
        );
        let resumed = recovered
            .claim_next()
            .expect("claim recovered")
            .expect("retry");
        assert_eq!(resumed.id, retry.id);
        recovered.complete(&resumed.id).expect("complete recovered");

        let unknown = fixture
            .enqueue_paper(JobFixture::spec("artifact", Some("gemini"), "unknown"))
            .expect("enqueue")
            .job;
        let claimed_unknown = recovered.claim_next().expect("claim").expect("unknown");
        assert_eq!(unknown.id, claimed_unknown.id);
        recovered
            .mark_provider_committed(&unknown.id, Some("request-1"))
            .expect("provider commit");

        let restarted = JobModule::open(&fixture.database_path).expect("restart");
        assert_eq!(
            restarted
                .get(&unknown.id)
                .expect("unrecovered paid job")
                .state,
            JobState::Running
        );
        restarted.recover_for_startup().expect("recover startup");
        let recovered_unknown = restarted.get_record(&unknown.id).expect("unknown paid job");
        assert_eq!(recovered_unknown.state, JobState::InterruptedUnknown);
        assert_eq!(
            recovered_unknown
                .provider_requirement
                .as_ref()
                .map(ProviderRequirement::code),
            Some(ProviderRequirementCode::ProviderCommitted)
        );
        restarted
            .assert_active_route_invariants()
            .expect("recovered job route invariant");
    }

    #[test]
    fn recovery_requeues_paid_ocr_when_the_raw_response_is_staged() {
        let fixture = JobFixture::new();
        let spec = JobFixture::spec("ocr", Some("mistral"), "staged-ocr");
        let job = fixture.enqueue_mistral(spec).expect("enqueue").job;
        let staging = fixture
            .database_path
            .parent()
            .expect("data directory")
            .join("staging")
            .join("ocr")
            .join(format!("{}.json", job.id));
        std::fs::create_dir_all(staging.parent().expect("staging directory")).expect("directory");
        std::fs::write(&staging, b"{\"pages\":[]}").expect("staging");
        fixture.jobs.claim_next().expect("claim").expect("running");
        fixture
            .jobs
            .mark_provider_committed(&job.id, None)
            .expect("provider commit");

        let recovered = JobModule::open(&fixture.database_path).expect("recover");
        recovered.recover_for_startup().expect("recover startup");
        let staged = recovered.get(&job.id).expect("staged job");
        assert_eq!(staged.state, JobState::Queued);
        assert_eq!(staged.stage, "recovered_staged");
    }

    #[test]
    fn checkpoint_progress_is_projected_on_get() {
        let fixture = JobFixture::new();
        let job = fixture
            .enqueue_paper(JobFixture::spec(
                "outline_overview",
                Some("gemini"),
                "outline-1",
            ))
            .expect("enqueue")
            .job;
        fixture.jobs.claim_next().expect("claim").expect("running");
        fixture
            .jobs
            .save_checkpoint(
                &job.id,
                "extracting",
                &serde_json::json!({
                    "step": 1,
                    "steps": 3,
                    "inputTokens": 1200,
                    "outputTokens": 80,
                    "cachedInputTokens": 400
                }),
            )
            .expect("checkpoint");
        let loaded = fixture.jobs.get(&job.id).expect("get");
        assert_eq!(loaded.stage, "extracting");
        let progress = loaded.progress.expect("progress");
        assert_eq!(progress["step"], 1);
        assert_eq!(progress["inputTokens"], 1200);
        assert_eq!(progress["outputTokens"], 80);
    }

    #[test]
    fn active_of_ignores_completed_jobs() {
        let fixture = JobFixture::new();
        fixture.seed_paper("paper-1");
        let connection = Connection::open(&fixture.database_path).expect("db");
        connection
            .execute(
                "INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                 VALUES ('rev-1', 'paper-1', 'sha', 1, 1, 'Inbox/paper-1.pdf', ?1)",
                params![now()],
            )
            .expect("revision");
        let mut spec = JobFixture::spec("outline_overview", Some("gemini"), "outline-active");
        spec.revision_id = Some("rev-1".to_string());
        spec.paper_id = Some("paper-1".to_string());
        fixture.jobs.enqueue(spec).expect("enqueue");
        assert!(fixture
            .jobs
            .active_of("outline_overview", "rev-1")
            .expect("active")
            .is_some());
        let connection = Connection::open(&fixture.database_path).expect("db");
        connection
            .execute(
                "UPDATE jobs SET state = 'completed' WHERE revision_id = 'rev-1'",
                [],
            )
            .expect("complete");
        assert!(fixture
            .jobs
            .active_of("outline_overview", "rev-1")
            .expect("active after complete")
            .is_none());
    }

    #[test]
    fn state_transitions_move_the_jobs_revision_while_progress_writes_do_not() {
        let fixture = JobFixture::new();
        let connection = Connection::open(&fixture.database_path).expect("database");
        let jobs_revision = |connection: &Connection| -> i64 {
            crate::library_workflow::library_revisions(connection)
                .expect("revisions")
                .domain(LibraryDomain::Jobs)
        };
        let first = fixture
            .enqueue_paper(JobFixture::spec("artifact", None, "artifact:1"))
            .expect("enqueue")
            .job;
        let second = fixture
            .enqueue_paper(JobFixture::spec("artifact", None, "artifact:2"))
            .expect("enqueue")
            .job;
        let after_enqueue = jobs_revision(&connection);
        assert!(after_enqueue > 0, "enqueue must bump the jobs domain");

        let claimed = fixture.jobs.claim_next().expect("claim").expect("a job");
        let pending = if claimed.id == first.id {
            &second
        } else {
            &first
        };
        let after_claim = jobs_revision(&connection);
        assert!(
            after_claim > after_enqueue,
            "queued -> running is a Hub-visible state move"
        );

        fixture
            .jobs
            .save_checkpoint(&claimed.id, "chunk_1", &serde_json::json!({ "done": 1 }))
            .expect("checkpoint");
        fixture
            .jobs
            .reprioritize(&pending.id, 9)
            .expect("reprioritize");
        assert_eq!(
            jobs_revision(&connection),
            after_claim,
            "stage and queue order must not invalidate a cached Hub page"
        );

        fixture.jobs.complete(&claimed.id).expect("complete");
        assert!(
            jobs_revision(&connection) > after_claim,
            "a finished job must invalidate the cached card status"
        );
    }

    #[test]
    fn prepare_exact_does_not_create_a_claimable_job() {
        let fixture = JobFixture::new();
        let route =
            capture_mistral_ocr_route("test-mistral-key", "mistral-ocr-latest").expect("ocr route");
        let mut connection = Connection::open(&fixture.database_path).expect("database");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("tx");
        let handle = prepare_exact_on(
            &transaction,
            &JobFixture::spec("ocr", Some("mistral"), "ocr:rev-prepare"),
            &JobExecutionRoute::MistralOcr(route),
            &(Utc::now() + chrono::Duration::minutes(15)).to_rfc3339(),
        )
        .expect("prepare");
        transaction.commit().expect("commit");
        assert!(!handle.id.is_empty());
        let jobs: i64 = Connection::open(&fixture.database_path)
            .expect("database")
            .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
            .expect("count");
        assert_eq!(jobs, 0);
        assert!(fixture.jobs.claim_next_record().expect("claim").is_none());
    }

    #[test]
    fn consume_enqueues_and_rejects_a_changed_key() {
        let fixture = JobFixture::new();
        let original =
            capture_mistral_ocr_route("key-before-change", "mistral-ocr-latest").expect("original");
        let changed =
            capture_mistral_ocr_route("key-after-change", "mistral-ocr-latest").expect("changed");
        let spec = JobSpec {
            kind: "ocr".to_string(),
            provider: Some("mistral".to_string()),
            paper_id: None,
            revision_id: None,
            root_key: None,
            artifact_key: Some("ocr".to_string()),
            dedupe_key: "ocr:rev-1".to_string(),
            priority: 100,
            payload: json!({"model": "mistral-ocr-latest"}),
        };
        let expires = (Utc::now() + chrono::Duration::minutes(15)).to_rfc3339();
        let mut connection = Connection::open(&fixture.database_path).expect("database");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("tx");
        let handle = prepare_exact_on(
            &transaction,
            &spec,
            &JobExecutionRoute::MistralOcr(original.clone()),
            &expires,
        )
        .expect("prepare");
        let mismatch = consume_prepared_on(
            &transaction,
            &handle,
            &JobExecutionRoute::MistralOcr(changed),
        );
        assert!(mismatch.is_err(), "switching the key must fail closed");
        let consumed = consume_prepared_on(
            &transaction,
            &handle,
            &JobExecutionRoute::MistralOcr(original),
        )
        .expect("consume original route");
        assert!(!consumed.coalesced);
        transaction.commit().expect("commit");
        let claimed = fixture
            .jobs
            .claim_next_record()
            .expect("claim")
            .expect("prepared job became claimable");
        assert_eq!(claimed.id, consumed.job_id);
        assert_eq!(claimed.kind, "ocr");
    }

    #[test]
    fn consume_coalesces_onto_an_existing_active_job() {
        let fixture = JobFixture::new();
        let route =
            capture_mistral_ocr_route("test-mistral-key", "mistral-ocr-latest").expect("ocr route");
        let spec = JobSpec {
            kind: "ocr".to_string(),
            provider: Some("mistral".to_string()),
            paper_id: None,
            revision_id: None,
            root_key: None,
            artifact_key: Some("ocr".to_string()),
            dedupe_key: "ocr:rev-join".to_string(),
            priority: 100,
            payload: json!({"model": "mistral-ocr-latest"}),
        };
        let first = fixture
            .jobs
            .enqueue_record(spec.clone(), JobExecutionRoute::MistralOcr(route.clone()))
            .expect("first enqueue");
        let expires = (Utc::now() + chrono::Duration::minutes(15)).to_rfc3339();
        let mut connection = Connection::open(&fixture.database_path).expect("database");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("tx");
        let handle = prepare_exact_on(
            &transaction,
            &spec,
            &JobExecutionRoute::MistralOcr(route.clone()),
            &expires,
        )
        .expect("prepare");
        let consumed =
            consume_prepared_on(&transaction, &handle, &JobExecutionRoute::MistralOcr(route))
                .expect("consume");
        transaction.commit().expect("commit");
        assert!(consumed.coalesced);
        assert_eq!(consumed.job_id, first.job.id);
    }

    #[test]
    fn rolled_back_prepare_leaves_no_handle_and_no_job() {
        let fixture = JobFixture::new();
        let route =
            capture_mistral_ocr_route("test-mistral-key", "mistral-ocr-latest").expect("ocr route");
        let mut connection = Connection::open(&fixture.database_path).expect("database");
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .expect("tx");
        prepare_exact_on(
            &transaction,
            &JobFixture::spec("ocr", Some("mistral"), "ocr:rev-rollback"),
            &JobExecutionRoute::MistralOcr(route),
            &(Utc::now() + chrono::Duration::minutes(15)).to_rfc3339(),
        )
        .expect("prepare");
        transaction.rollback().expect("rollback");
        let connection = Connection::open(&fixture.database_path).expect("database");
        let preparations: i64 = connection
            .query_row("SELECT COUNT(*) FROM job_preparations", [], |row| {
                row.get(0)
            })
            .expect("count preparations");
        let jobs: i64 = connection
            .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
            .expect("count jobs");
        assert_eq!(preparations, 0);
        assert_eq!(jobs, 0);
    }
}
