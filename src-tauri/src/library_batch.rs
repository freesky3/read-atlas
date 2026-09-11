//! D-063 PR 3：持久 Batch 引擎（计划 §4.3 / §5.2 / §7 / §9 / §10.2）。
//!
//! 这里只有编排，不复制其他 Module 的所有权：路径 / hash / 回收站规则属于
//! `PaperModule`，Provider 路由与限流属于 `JobModule`。本模块负责的四件事：
//!
//! 1. `plan` 在一个 `BEGIN IMMEDIATE` 里解析成员、冻结逐项 precondition digest、
//!    写入 Batch / Items / idempotency receipt，一次提交；
//! 2. Item state 是权威，`library_batches.state` 只是同一事务内重算的缓存；
//! 3. Retry / Undo 都另开 child Batch（`relation`），绝不改写父记录的原终态；
//! 4. 崩溃后由 [`reconcile_interrupted`] 从 Items 修复缓存态。
//!
//! 计划里的 §10.1 `batch` / `batch_items` 读投影也住在这里（[`batch_projection`] /
//! [`batch_items_page`]），由 `library_read` 分派——批次投影是读，不是写。

use crate::job_module::{
    cancel_job_on, consume_prepared_on, expire_due_preparations, find_active_job_id,
    prepare_exact_on, release_prepared_on, sync_linked_batch_items, JobExecutionRoute, JobSpec,
    PreparedJobHandle,
};
use crate::library_cost::{
    add_item_estimate, estimate_exceeds_ceiling, is_long_pdf, CostPreview, ProviderWorkKind,
    PRICE_CATALOG_VERSION,
};
use crate::library_query::{selection_dependencies, selection_digest, sha256_hex, QueryFilter};
use crate::library_workflow::{
    bump_library_revisions, library_revisions, DomainRevision, LibraryDomain,
};
use crate::provider_routing::{FrozenMistralOcrRoute, FrozenProviderRoute};
use crate::{now, open_db};
use chrono::{Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

/// §6.6 / §7：一次批次的硬上限。超出必须分批，不能静默截断成员。
pub(crate) const MAX_BATCH_ITEMS: usize = 500;
/// §4.2：`idempotencyKey` 由调用方生成；长度上限防止把整段 SQL 塞进主键。
pub(crate) const MAX_IDEMPOTENCY_KEY_CHARS: usize = 64;
/// §9：prepared handle 与 Batch Plan 一起在 15 分钟后过期。
pub(crate) const PLAN_TTL_MINUTES: i64 = 15;
/// §7.3：Undo Token 的默认能力窗口。8 秒 Toast 只是快捷入口。
pub(crate) const UNDO_TTL_MINUTES: i64 = 10;
pub(crate) const BATCH_ITEMS_DEFAULT_PAGE: i64 = 100;
pub(crate) const BATCH_ITEMS_MAX_PAGE: i64 = 200;
/// §6.1 / §10.1：任务中心要能在重启后找回批次。缺省 50、上限 100，
/// 只按 `updated_at` 倒序取最近的，不做跨 Workspace 的历史档案。
pub(crate) const RECENT_BATCHES_DEFAULT: i64 = 50;
pub(crate) const RECENT_BATCHES_MAX: i64 = 100;
/// 单个标签名的长度上限，与 `library_query` 的筛选侧规则一致。
const MAX_TAG_NAME: usize = 64;
const MAX_TAG_PATCH: usize = 64;
/// child 批次向上解析动作时的父链深度上限，与 [`effective_command`] 的剥壳上限一致。
const MAX_LINEAGE_HOPS: usize = 8;
/// 标签逐项计划的判别值；写入与两处执行路径共用同一个常量。
const TAG_PLAN_ACTION: &str = "patch_tags";
/// Move / Trash / Import / Export 的逐项计划判别值，与 `BatchCommandKind::as_str` 同源。
const MOVE_PLAN_ACTION: &str = "move";
const TRASH_PLAN_ACTION: &str = "trash";
const IMPORT_PLAN_ACTION: &str = "import";
const EXPORT_PLAN_ACTION: &str = "export";
/// 导入项唯一的补偿形态：把本批新建的 Paper 移进回收站。源文件永不删除。
const IMPORT_COMPENSATION_MODE: &str = "trash_created";
/// 导出项唯一的补偿形态：删掉本批新建的那一份输出。被覆盖掉的旧文件不属于本批，
/// 那一类项根本没有补偿路径（§7「仅删除本 Batch 新建且 digest 未变化的输出」）。
const EXPORT_COMPENSATION_MODE: &str = "delete_output";
/// §7：Import Item 的结论。前四个是文档规定的四类，后三个是 `skipped` 这一类里
/// 计划阶段就能定论的三个原因——一次拖放里混进目录或重复路径时，
/// 每一项都要有自己的结论，不能让整批无反馈地消失。
const IMPORT_OUTCOME_CREATED: &str = "created_new";
const IMPORT_OUTCOME_REUSED: &str = "reused_existing";
const IMPORT_OUTCOME_RESTORED: &str = "restored_existing";
const IMPORT_OUTCOME_CONFLICT: &str = "conflict";
const IMPORT_OUTCOME_DUPLICATE_SOURCE: &str = "skipped_duplicate_source";
const IMPORT_OUTCOME_NOT_PDF: &str = "skipped_not_pdf";
const IMPORT_OUTCOME_SOURCE_MISSING: &str = "skipped_source_missing";
/// §7 Export 行的两种结论。这一批的输出要么是新写的，是撤销的对象；要么是覆盖，
/// 被盖掉的那一份回不来、所以它根本没有补偿路径——结论里必须看得出是哪一种。
const EXPORT_OUTCOME_WRITTEN: &str = "wrote_new";
const EXPORT_OUTCOME_OVERWRITTEN: &str = "overwrote_existing";
/// §8.2：Start 必须逐字回报的确认项 id。文案可以改，id 不能。
const REQUIREMENT_KIND_CHANGE: &str = "kind_change";
const REQUIREMENT_DESTRUCTIVE: &str = "destructive";
const REQUIREMENT_CONFLICT: &str = "conflict";
/// §7 Export 行：覆盖已有输出是「不可逆影响」，确认必须发生在写之前。
const REQUIREMENT_OVERWRITE: &str = "overwrite";
const REQUIREMENT_COST: &str = "cost";
const REQUIREMENT_UNKNOWN_COST: &str = "unknown_cost";
const REQUIREMENT_LONG_PDF: &str = "long_pdf";
const REQUIREMENT_POSSIBLE_CHARGE: &str = "possible_duplicate_charge";
const OCR_PLAN_ACTION: &str = "ocr";
const BRIEF_PLAN_ACTION: &str = "brief";
const OCR_DEDUPE_PREFIX: &str = "ocr:";
const BRIEF_DEDUPE_PREFIX: &str = "orientation:";
const MISTRAL_OCR_MODEL: &str = "mistral-ocr-latest";

/// Plan / Start OCR 与 Brief 时由桌面层注入的当前冻结 route。
/// Workflow 不读 Key；它只拿 JobModule 已经冻结过的 route 做 exact bind。
#[derive(Debug, Clone, Default)]
pub(crate) struct ProviderActContext {
    pub ocr: Option<FrozenMistralOcrRoute>,
    pub paper: Option<FrozenProviderRoute>,
    /// Independent Brief and PDF source prompts frozen for each document kind.
    pub orientation_prompt: Option<String>,
    pub textbook_orientation_prompt: Option<String>,
    pub textbook_brief_protocol: Option<String>,
    pub paper_root_prompt: Option<String>,
    pub textbook_root_prompt: Option<String>,
}

pub(crate) type LibraryActResult<T> = Result<T, LibraryActError>;

/// §10.2：请求级错误码。`batch_not_found` / `confirmation_required` /
/// `batch_too_large` / `invalid_source` 是相对文档列表的四处补充，理由见
/// data-protocols.md。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryActErrorCode {
    /// 请求本身不合法：未知字段、越界分页、空标签名、协议版本不符。
    InvalidQuery,
    /// Workspace 未打开或数据库不可读。
    WorkspaceUnavailable,
    /// 同一 key 配上了不同请求。
    IdempotencyConflict,
    /// Selection Snapshot 已经过期：成员或依赖域变了。
    StaleSelection,
    /// 计划已过期或 `planDigest` 与库中不符，必须重新预览。
    StaleBatchPlan,
    /// 解析后没有任何成员。
    SelectionEmpty,
    /// 引用的 Batch 不存在（UI 拿着已被清理的 batchId 再来操作）。
    BatchNotFound,
    /// 成员数超过 [`MAX_BATCH_ITEMS`]，必须分批。
    BatchTooLarge,
    /// 显式目标里有不存在或已删除的 Paper。
    PaperNotFound,
    /// §10.2：计划里的目标路径已经被别的文件占住，冲突已经记档。
    TargetPathConflict,
    /// §10.2：库记着的路径上已经没有那个 PDF 文件。
    SourceMissing,
    /// §10.2 / §7 Import 行：源文件根本不是一个能读的 PDF（目录、非 `.pdf`、
    /// 文件头坏了、打不开）。重跑同一个文件不会变好，所以不算可重试。
    InvalidSource,
    /// §10.2：源文件在计划之后又变了（字节数不同），必须重新预览。
    SourceChanged,
    /// 计划冻结了确认项而 Start 没有全部接受。
    ConfirmationRequired,
    /// 该动作没有补偿路径（付费动作、或结果已被后续修改）。
    NotReversible,
    /// Undo Token 已经越过能力窗口。
    UndoExpired,
    /// Undo Token 已用过、已作废，或目标状态已被后续写改变。
    UndoConflict,
    /// 该范围不支持这个动作（例如 Smart Collection 手排）。
    UnsupportedForScope,
    /// Workspace 正在被别的写占住。
    WorkspaceBusy,
    /// 只用于逐项失败：进程在 Item 执行中退出，效果未知。
    InterruptedUnknown,
    /// Lifecycle version 或相对日期快照已经不是确认时那一份。
    StaleLibrarySnapshot,
    /// 当前 Provider / Key 不能 exact bind 计划里冻结的 route。
    ProviderRouteUnavailable,
    /// 付费批次的费用确认项没有被全部接受。
    CostConfirmationRequired,
    /// 重试可能对已经向 Provider 提交过的请求再计一次费。
    PossibleDuplicateCharge,
}

impl LibraryActErrorCode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidQuery => "invalid_query",
            Self::WorkspaceUnavailable => "workspace_unavailable",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::StaleSelection => "stale_selection",
            Self::StaleBatchPlan => "stale_batch_plan",
            Self::SelectionEmpty => "selection_empty",
            Self::BatchNotFound => "batch_not_found",
            Self::BatchTooLarge => "batch_too_large",
            Self::PaperNotFound => "paper_not_found",
            Self::TargetPathConflict => "target_path_conflict",
            Self::SourceMissing => "source_missing",
            Self::InvalidSource => "invalid_source",
            Self::SourceChanged => "source_changed",
            Self::ConfirmationRequired => "confirmation_required",
            Self::NotReversible => "not_reversible",
            Self::UndoExpired => "undo_expired",
            Self::UndoConflict => "undo_conflict",
            Self::UnsupportedForScope => "unsupported_for_scope",
            Self::WorkspaceBusy => "workspace_busy",
            Self::InterruptedUnknown => "interrupted_unknown",
            Self::StaleLibrarySnapshot => "stale_library_snapshot",
            Self::ProviderRouteUnavailable => "provider_route_unavailable",
            Self::CostConfirmationRequired => "cost_confirmation_required",
            Self::PossibleDuplicateCharge => "possible_duplicate_charge",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        [
            Self::InvalidQuery,
            Self::WorkspaceUnavailable,
            Self::IdempotencyConflict,
            Self::StaleSelection,
            Self::StaleBatchPlan,
            Self::SelectionEmpty,
            Self::BatchNotFound,
            Self::BatchTooLarge,
            Self::PaperNotFound,
            Self::TargetPathConflict,
            Self::SourceMissing,
            Self::InvalidSource,
            Self::SourceChanged,
            Self::ConfirmationRequired,
            Self::NotReversible,
            Self::UndoExpired,
            Self::UndoConflict,
            Self::UnsupportedForScope,
            Self::WorkspaceBusy,
            Self::InterruptedUnknown,
            Self::StaleLibrarySnapshot,
            Self::ProviderRouteUnavailable,
            Self::CostConfirmationRequired,
            Self::PossibleDuplicateCharge,
        ]
        .into_iter()
        .find(|code| code.as_str() == value)
    }

    /// 逐项 `retryable` 的唯一来源：只有「重跑一次可能成功」的瞬时条件才算可重试。
    /// `source_changed` 算：重试会按当前磁盘事实重新冻结字节数，正在被写的源文件
    /// 写完之后再试就是另一回事了。`invalid_source` 不算——同一个文件重跑不会变好。
    pub(crate) const fn is_retryable(self) -> bool {
        matches!(
            self,
            Self::StaleSelection
                | Self::WorkspaceBusy
                | Self::SourceChanged
                | Self::PossibleDuplicateCharge
        )
    }
}

/// §10.2：错误也是 typed projection；`message` 只带业务摘要。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryActError {
    pub code: LibraryActErrorCode,
    pub message: String,
}

impl LibraryActError {
    fn of(code: LibraryActErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::of(LibraryActErrorCode::InvalidQuery, message)
    }

    pub(crate) fn unavailable(message: impl Into<String>) -> Self {
        Self::of(LibraryActErrorCode::WorkspaceUnavailable, message)
    }

    /// 读侧工具的失败不能伪装成「请求不合法」。
    fn from_query(error: crate::library_query::LibraryQueryError) -> Self {
        match error.code {
            crate::library_query::LibraryQueryErrorCode::WorkspaceUnavailable => {
                Self::unavailable(error.message)
            }
            crate::library_query::LibraryQueryErrorCode::InvalidQuery => {
                Self::invalid(error.message)
            }
        }
    }
}

/// §5.2：Batch 聚合态。命名与 `library_batches.state` 的 CHECK 逐字一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchState {
    Planned,
    Queued,
    Running,
    Paused,
    ActionRequired,
    InterruptedUnknown,
    Completed,
    CompletedWithErrors,
    Failed,
    Cancelled,
}

impl BatchState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::ActionRequired => "action_required",
            Self::InterruptedUnknown => "interrupted_unknown",
            Self::Completed => "completed",
            Self::CompletedWithErrors => "completed_with_errors",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        [
            Self::Planned,
            Self::Queued,
            Self::Running,
            Self::Paused,
            Self::ActionRequired,
            Self::InterruptedUnknown,
            Self::Completed,
            Self::CompletedWithErrors,
            Self::Failed,
            Self::Cancelled,
        ]
        .into_iter()
        .find(|state| state.as_str() == value)
    }
}

/// §5.2：Item 态是整台状态机的权威。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemState {
    Planned,
    Queued,
    Running,
    Paused,
    ActionRequired,
    InterruptedUnknown,
    Succeeded,
    Failed,
    Skipped,
    Cancelled,
}

impl ItemState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Planned => "planned",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::ActionRequired => "action_required",
            Self::InterruptedUnknown => "interrupted_unknown",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        [
            Self::Planned,
            Self::Queued,
            Self::Running,
            Self::Paused,
            Self::ActionRequired,
            Self::InterruptedUnknown,
            Self::Succeeded,
            Self::Failed,
            Self::Skipped,
            Self::Cancelled,
        ]
        .into_iter()
        .find(|state| state.as_str() == value)
    }

    pub(crate) const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Skipped | Self::Cancelled
        )
    }
}

/// `library_batches.undo_policy` 的四个取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndoPolicy {
    None,
    Full,
    Compensating,
    CancelOnly,
}

impl UndoPolicy {
    const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Full => "full",
            Self::Compensating => "compensating",
            Self::CancelOnly => "cancel_only",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        [Self::None, Self::Full, Self::Compensating, Self::CancelOnly]
            .into_iter()
            .find(|policy| policy.as_str() == value)
    }

    const fn compensable(self) -> bool {
        matches!(self, Self::Compensating | Self::Full)
    }
}

/// `library_undo_tokens.state`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UndoTokenState {
    Available,
    Consumed,
    Expired,
    Revoked,
}

impl UndoTokenState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Available => "available",
            Self::Consumed => "consumed",
            Self::Expired => "expired",
            Self::Revoked => "revoked",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        [
            Self::Available,
            Self::Consumed,
            Self::Expired,
            Self::Revoked,
        ]
        .into_iter()
        .find(|state| state.as_str() == value)
    }
}

/// §5.2 / §9：已经有策略的动作种类。表里的 CHECK 保留计划 §9 的全部取值，
/// 所以 Export / OCR / Brief / lifecycle 落地时不需要再改 schema——反过来，
/// 这一列的取值范围是 schema，不是这个枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchCommandKind {
    PatchTags,
    Move,
    Trash,
    Import,
    Export,
    PatchLifecycle,
    Ocr,
    Brief,
    Retry,
    Compensation,
}

impl BatchCommandKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::PatchTags => "patch_tags",
            Self::Move => "move",
            Self::Trash => "trash",
            Self::Import => "import",
            Self::Export => "export",
            Self::PatchLifecycle => "patch_lifecycle",
            Self::Ocr => "ocr",
            Self::Brief => "brief",
            Self::Retry => "retry",
            Self::Compensation => "compensation",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        [
            Self::PatchTags,
            Self::Move,
            Self::Trash,
            Self::Import,
            Self::Export,
            Self::PatchLifecycle,
            Self::Ocr,
            Self::Brief,
            Self::Retry,
            Self::Compensation,
        ]
        .into_iter()
        .find(|kind| kind.as_str() == value)
    }

    /// `retry` / `compensation` 只是封套：它们自己没有策略，要沿父链解出真命令。
    const fn is_concrete(self) -> bool {
        matches!(
            self,
            Self::PatchTags
                | Self::Move
                | Self::Trash
                | Self::Import
                | Self::Export
                | Self::PatchLifecycle
                | Self::Ocr
                | Self::Brief
        )
    }
}

/// §5.2：聚合真值表的输入。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemCounts {
    pub planned: i64,
    pub queued: i64,
    pub running: i64,
    pub paused: i64,
    pub action_required: i64,
    pub interrupted_unknown: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub skipped: i64,
    pub cancelled: i64,
}

impl ItemCounts {
    fn total(&self) -> i64 {
        self.planned
            + self.queued
            + self.running
            + self.paused
            + self.action_required
            + self.interrupted_unknown
            + self.succeeded
            + self.failed
            + self.skipped
            + self.cancelled
    }
}

/// §8.2：Plan 必须冻结的确认项。成本类确认属于 PR 5，这里只有本地动作的两种。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatchRequirementKind {
    /// 跨根 Move 会摘除成果类别。
    KindChange,
    /// 移入回收站等破坏性动作。
    Destructive,
    /// §7：计划里有项按现状执行不下去（目标路径被占 / 源文件已不在）。
    /// 不是「做了之后会怎样」，而是「其中几项会失败」，同样必须显式确认。
    Conflict,
    /// §7 Export 行：这一项的输出会替换掉 `export/` 下已有的文件。
    /// 被替换的那一份回不来（撤销只删本批新建的输出），所以确认必须发生在写之前。
    Overwrite,
    /// §8：已知或区间费用，必须在 Start 前确认。
    Cost,
    /// §8：部分项没有价格，不能显示为 0。
    UnknownCost,
    /// 长 PDF 的一次性确认，复用单篇 OCR / Brief 的同一阈值。
    LongPdf,
    /// §7.2：重试可能对已提交的 Provider 请求再计一次费。
    PossibleDuplicateCharge,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRequirement {
    /// Start 必须逐字回报这个 id，UI 不能靠位置或文案猜。
    pub id: String,
    pub kind: BatchRequirementKind,
    pub item_count: i64,
    pub label: String,
}

/// 存在 `library_batches.cost_preview_json` 里的计划摘要。
///
/// 本地动作的成本恒为空——这一列在 schema 8 是 NOT NULL，PR 5 的成本预览复用同一个
/// JSON 形状。确认项也冻结在这里，投影时不必扫全表 Items。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlanSummary {
    #[serde(default)]
    requirements: Vec<BatchRequirement>,
    #[serde(default)]
    cost: CostPreview,
}

impl PlanSummary {
    fn local(requirements: Vec<BatchRequirement>) -> Self {
        Self {
            requirements,
            cost: CostPreview::empty(),
        }
    }

    fn provider(requirements: Vec<BatchRequirement>, cost: CostPreview) -> Self {
        Self { requirements, cost }
    }

    fn store(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_string())
    }

    fn load(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or_else(|_| Self {
            requirements: Vec::new(),
            cost: CostPreview::empty(),
        })
    }
}

/// §5.2：父 Projection 只派生 child 摘要，不改写父记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChildSummary {
    pub batch_id: String,
    pub state: BatchState,
    pub total_items: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoSummary {
    pub state: UndoTokenState,
    pub expires_at: String,
}

/// §10.1 `read({kind:'batch'})` 与 `act` 响应共用的批次投影。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchProjection {
    pub protocol_version: u32,
    pub id: String,
    /// 本批次自己的 `command_kind` 列：`patch_tags` / `retry` / `compensation`。
    pub command_kind: BatchCommandKind,
    /// child 批次真正跑的那个具体动作：沿父链解析出的第一个命令，与执行侧同一套规则。
    pub parent_command_kind: Option<BatchCommandKind>,
    pub parent_batch_id: Option<String>,
    pub relation: Option<String>,
    pub state: BatchState,
    pub plan_digest: String,
    pub target_digest: String,
    pub undo_policy: UndoPolicy,
    pub total_items: i64,
    pub counts: ItemCounts,
    pub requirements: Vec<BatchRequirement>,
    /// §5.2：`cancel_requested_at` 只投影成「正在取消」，不是另一终态。
    pub is_cancelling: bool,
    pub plan_expires_at: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub updated_at: String,
    pub retry_summary: Option<ChildSummary>,
    pub compensation_summary: Option<ChildSummary>,
    pub undo: Option<UndoSummary>,
    pub cost_preview: CostPreview,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchItemProjection {
    pub id: String,
    pub ordinal: i64,
    pub target_key: String,
    pub paper_id: Option<String>,
    pub revision_id: Option<String>,
    pub state: ItemState,
    /// §7：这一项**做成了什么**。导入的四类结论（`created_new` /
    /// `reused_existing` / `restored_existing` / `conflict`）和「已经在目标位」
    /// 这类跳过都从这里出去——`state` 只说「跑没跑成」，说不清「库里多了什么」。
    /// 取的是 `result_json.outcome` 一个字段，其余结果载荷只留档不外发。
    pub outcome: Option<String>,
    pub attempt_count: i64,
    pub error_code: Option<LibraryActErrorCode>,
    pub error_summary: Option<String>,
    /// 单项失败是逐项记录 `code / retryable / safeSummary / attempt / failedAt`；
    /// 补偿只对被记为可重试的项开放 retry_failed。
    pub retryable: bool,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub updated_at: String,
    pub source_item_id: Option<String>,
}

/// §5.2：单项失败的三要素。`retryable` 由 code 推导，调用方不能自行宣称「可重试」。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ItemFailure {
    code: LibraryActErrorCode,
    retryable: bool,
    safe_summary: String,
}

impl ItemFailure {
    /// `summary` 只允许业务文案：调用方拿到的字符串一律先经过这里的选择，
    /// 底层文本没有直通通道。
    fn new(code: LibraryActErrorCode, summary: impl Into<String>) -> Self {
        Self {
            code,
            retryable: code.is_retryable(),
            safe_summary: summary.into(),
        }
    }

    /// 底层读写失败：原始 SQLite 文本一个字节都不许进 `error_summary`（§10.2）。
    fn storage() -> Self {
        Self::new(
            LibraryActErrorCode::WorkspaceBusy,
            "the library could not be read or written; try again",
        )
    }
}

/// `BatchTarget`：显式 ID 列表，或带 Selection Snapshot 的逻辑选择。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", deny_unknown_fields)]
pub enum BatchTarget {
    #[serde(rename_all = "camelCase")]
    Explicit { paper_ids: Vec<String> },
    #[serde(rename_all = "camelCase")]
    Query {
        filters: Vec<QueryFilter>,
        excluded_ids: Vec<String>,
        /// §5.1：来自 `hub_page` 的**成员**摘要，不是页宽/排序参与的那一个。
        selection_digest: String,
        dependency_revisions: Vec<DomainRevision>,
        #[serde(default)]
        evaluation_anchor: Option<String>,
        #[serde(default)]
        evaluation_timezone: Option<String>,
    },
    /// §7 Import 行：导入的成员是**磁盘上的源文件**，不是库里的 Paper——
    /// 提交那一刻它们还没有身份可引用。绝对路径只写进逐项 `plan_json`，
    /// 永远不进摘要、事件或投影（§10.2）；批次里代表它的键是 `dedupe_key`。
    #[serde(rename_all = "camelCase")]
    Sources { paths: Vec<String> },
}

/// §7 Export 行：导出的「格式」是这一批唯一的可变约定，目标目录与命名规则由
/// `export_module` 拥有（输出永远落在 Workspace 的 `export/` 下），所以计划里
/// 不需要用户再给路径。取值只有一个意味着「其余格式还没实现」，而不是
/// 「传什么都行」——字符串参数会把这件事藏起来。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    ReadingBundle,
}

/// §4.2 / §10.2：`library_act` 的判别联合。禁止 `{ command, payload }` 形状。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", deny_unknown_fields)]
pub enum BatchCommand {
    #[serde(rename = "patch_tags", rename_all = "camelCase")]
    PatchTags {
        add: Vec<String>,
        remove: Vec<String>,
    },
    /// 批量 Move 只换目录：文件名保持每个 Paper 自己的 `file_name`，
    /// 所以重命名继续走单篇 `move_paper`，不混进批次语义里。
    #[serde(rename = "move", rename_all = "camelCase")]
    Move { collection_path: String },
    #[serde(rename = "trash")]
    Trash,
    /// §7 Import 行：把一批源文件收进同一个物理目录。目标目录必填——
    /// 落在「全部」或 Smart Collection 上时没有可推导的目录，必须由调用方选定，
    /// 猜一个目录就是把文件放进用户没看过的地方。
    #[serde(rename = "import", rename_all = "camelCase")]
    Import { collection_path: String },
    /// §7 Export 行：把一批 Paper 的阅读成果写成 Markdown 文件。
    #[serde(rename = "export", rename_all = "camelCase")]
    Export {
        format: ExportFormat,
        #[serde(default)]
        locale: crate::ui_locale::UiLocale,
    },
    #[serde(rename = "patch_lifecycle", rename_all = "camelCase")]
    PatchLifecycle {
        patch: crate::library_lifecycle::LifecyclePatch,
    },
    #[serde(rename = "ocr")]
    Ocr,
    #[serde(rename = "brief")]
    Brief,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", deny_unknown_fields)]
pub enum BatchControl {
    /// §7.1：立即取消尚未开始的 Item。
    CancelRemaining,
    /// §7.2：默认只复制 `failed && retryable`，也可以显式勾选。
    #[serde(rename_all = "camelCase")]
    RetryFailed { item_ids: Option<Vec<String>> },
    /// §7.3：整批补偿撤销。
    Undo { token: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", deny_unknown_fields)]
pub enum LibraryActRequest {
    #[serde(rename = "plan_batch", rename_all = "camelCase")]
    PlanBatch {
        protocol_version: u32,
        idempotency_key: String,
        command: BatchCommand,
        target: BatchTarget,
    },
    #[serde(rename = "start_batch", rename_all = "camelCase")]
    StartBatch {
        protocol_version: u32,
        idempotency_key: String,
        batch_id: String,
        plan_digest: String,
        accepted_requirement_ids: Vec<String>,
        /// §8.2：只是「确认后估算不得悄悄上升」的本地门槛，不是账单上限。
        maximum_accepted_estimate_by_currency: BTreeMap<String, String>,
    },
    #[serde(rename = "control_batch", rename_all = "camelCase")]
    ControlBatch {
        protocol_version: u32,
        idempotency_key: String,
        batch_id: String,
        control: BatchControl,
    },
    #[serde(rename = "change", rename_all = "camelCase")]
    Change {
        protocol_version: u32,
        idempotency_key: String,
        change: LibraryChange,
    },
    #[serde(rename = "record_reader_activity", rename_all = "camelCase")]
    RecordReaderActivity {
        protocol_version: u32,
        idempotency_key: String,
        activity: crate::library_lifecycle::ReaderActivity,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", deny_unknown_fields)]
pub enum LibraryChange {
    #[serde(rename = "patch_lifecycle", rename_all = "camelCase")]
    PatchLifecycle {
        paper_id: String,
        expected_version: i64,
        patch: crate::library_lifecycle::LifecyclePatch,
    },
    #[serde(rename = "create_smart_collection", rename_all = "camelCase")]
    CreateSmartCollection {
        name: String,
        filters: Vec<QueryFilter>,
    },
    #[serde(rename = "rename_smart_collection", rename_all = "camelCase")]
    RenameSmartCollection { id: String, name: String },
    #[serde(rename = "delete_smart_collection", rename_all = "camelCase")]
    DeleteSmartCollection { id: String },
}

impl LibraryActRequest {
    fn protocol_version(&self) -> u32 {
        match self {
            Self::PlanBatch {
                protocol_version, ..
            }
            | Self::StartBatch {
                protocol_version, ..
            }
            | Self::ControlBatch {
                protocol_version, ..
            }
            | Self::Change {
                protocol_version, ..
            }
            | Self::RecordReaderActivity {
                protocol_version, ..
            } => *protocol_version,
        }
    }

    fn idempotency_key(&self) -> &str {
        match self {
            Self::PlanBatch {
                idempotency_key, ..
            }
            | Self::StartBatch {
                idempotency_key, ..
            }
            | Self::ControlBatch {
                idempotency_key, ..
            }
            | Self::Change {
                idempotency_key, ..
            }
            | Self::RecordReaderActivity {
                idempotency_key, ..
            } => idempotency_key,
        }
    }

    /// receipt 要区分「同一个请求重放」和「同一个 key 换了请求」。
    fn digest(&self) -> String {
        canonical_digest(&json!({
            "request": self,
        }))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", deny_unknown_fields)]
pub enum LibraryActResponse {
    #[serde(rename = "plan_batch", rename_all = "camelCase")]
    PlanBatch { batch: BatchProjection },
    #[serde(rename = "start_batch", rename_all = "camelCase")]
    StartBatch {
        batch: BatchProjection,
        /// §7.3：明文只在响应里出现一次，库里只有 hash。
        undo_token: Option<String>,
    },
    #[serde(rename = "control_batch", rename_all = "camelCase")]
    ControlBatch {
        batch: BatchProjection,
        undo_token: Option<String>,
    },
    #[serde(rename = "change", rename_all = "camelCase")]
    Change { result: ChangeResult },
    #[serde(rename = "record_reader_activity", rename_all = "camelCase")]
    RecordReaderActivity {
        context: crate::library_lifecycle::ReadingContextProjection,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", deny_unknown_fields)]
pub enum ChangeResult {
    #[serde(rename = "lifecycle", rename_all = "camelCase")]
    Lifecycle {
        lifecycle: crate::library_lifecycle::LifecycleProjection,
    },
    #[serde(rename = "smart_collection", rename_all = "camelCase")]
    SmartCollection {
        collection: crate::library_lifecycle::SmartCollectionProjection,
    },
    #[serde(rename = "smart_collection_deleted", rename_all = "camelCase")]
    SmartCollectionDeleted { id: String },
}

impl LibraryActResponse {
    #[cfg(test)]
    pub(crate) fn batch(&self) -> &BatchProjection {
        match self {
            Self::PlanBatch { batch }
            | Self::StartBatch { batch, .. }
            | Self::ControlBatch { batch, .. } => batch,
            Self::Change { .. } | Self::RecordReaderActivity { .. } => {
                panic!("this act response is not a batch")
            }
        }
    }

    pub(crate) fn batch_id(&self) -> Option<&str> {
        match self {
            Self::PlanBatch { batch }
            | Self::StartBatch { batch, .. }
            | Self::ControlBatch { batch, .. } => Some(batch.id.as_str()),
            Self::Change { .. } | Self::RecordReaderActivity { .. } => None,
        }
    }

    /// `library_action_receipts.result_ref`：Batch 用 batch id，change 用目标 id。
    fn receipt_ref(&self) -> Option<&str> {
        match self {
            Self::PlanBatch { batch }
            | Self::StartBatch { batch, .. }
            | Self::ControlBatch { batch, .. } => Some(batch.id.as_str()),
            Self::Change { result } => match result {
                ChangeResult::Lifecycle { lifecycle } => Some(lifecycle.paper_id.as_str()),
                ChangeResult::SmartCollection { collection } => Some(collection.id.as_str()),
                ChangeResult::SmartCollectionDeleted { id } => Some(id.as_str()),
            },
            Self::RecordReaderActivity { context } => Some(context.lifecycle.paper_id.as_str()),
        }
    }

    /// 只有真正执行过的响应才值得发事件；`plan` 没有改任何 Hub 可见状态。
    pub(crate) fn notified_kind(&self) -> Option<&'static str> {
        match self {
            Self::PlanBatch { .. } => None,
            Self::StartBatch { .. } | Self::ControlBatch { .. } => Some("batch"),
            Self::Change { .. } | Self::RecordReaderActivity { .. } => Some("library"),
        }
    }

    pub(crate) fn notified_state(&self) -> Option<String> {
        match self {
            Self::PlanBatch { .. } => None,
            Self::StartBatch { batch, .. } | Self::ControlBatch { batch, .. } => {
                Some(batch.state.as_str().to_string())
            }
            Self::Change { .. } => Some("changed".to_string()),
            Self::RecordReaderActivity { .. } => Some("activity".to_string()),
        }
    }
}

/// §7 末尾：一次 `start` / `control` 有两种收场。纯 SQLite 动作在同一事务里跑完；
/// 要动文件系统的动作只能把「已开始」先提交，再由事务外的一次接力逐项执行。
enum ActOutcome {
    Done(LibraryActResponse),
    Deferred(DeferredRun),
}

/// Move / Trash 用不了「一个 `BEGIN IMMEDIATE` + 每 Item 一个 SAVEPOINT」：
/// `fs::rename` 不回滚，`PaperModule` 又自带连接（WAL 只允许一个写者）。
/// 所以这三步各自独立提交：认领一项 → 事务外执行 → 另一个短事务记结果。
struct DeferredRun {
    batch_id: String,
    action: ItemAction,
    /// 只有写下新效果的批次配 Undo Token；补偿批次不配（撤销的撤销不属于合同）。
    mints_undo: bool,
    shape: ResponseShape,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ResponseShape {
    Start,
    Control,
}

impl ResponseShape {
    fn build(self, batch: BatchProjection, undo_token: Option<String>) -> LibraryActResponse {
        match self {
            Self::Start => LibraryActResponse::StartBatch { batch, undo_token },
            Self::Control => LibraryActResponse::ControlBatch { batch, undo_token },
        }
    }
}

/// §4.2 / §10.2 的 `act` 入口。本地动作不需要 Provider context。
pub(crate) fn act(
    root: &Path,
    request: &LibraryActRequest,
) -> LibraryActResult<LibraryActResponse> {
    act_with_provider(root, request, &ProviderActContext::default())
}

/// OCR / Brief 必须带上当前冻结 route，Start 时 exact bind，不能回退到「现在的 Provider」。
pub(crate) fn act_with_provider(
    root: &Path,
    request: &LibraryActRequest,
    provider: &ProviderActContext,
) -> LibraryActResult<LibraryActResponse> {
    if request.protocol_version() != crate::library_query::LIBRARY_PROTOCOL_VERSION {
        return Err(LibraryActError::invalid(format!(
            "unsupported protocolVersion {}, expected {}",
            request.protocol_version(),
            crate::library_query::LIBRARY_PROTOCOL_VERSION
        )));
    }
    validate_idempotency_key(request.idempotency_key())?;
    let mut connection = open_db(root).map_err(LibraryActError::unavailable)?;
    // 进程可能在任何一步被杀：任何一次 act 之前先把上一次的残骸修好。
    reconcile_interrupted(&connection)?;
    let _ = expire_due_preparations(&connection, &now());
    let digest = request.digest();
    match request {
        LibraryActRequest::PlanBatch {
            idempotency_key,
            command,
            target,
            ..
        } => {
            if let Some(replay) = replay_receipt(&connection, idempotency_key, &digest)? {
                return Ok(replay);
            }
            let transaction = immediate_transaction(&mut connection)?;
            let response = plan_batch(&transaction, root, command, target, provider)?;
            record_receipt(&transaction, idempotency_key, &digest, "plan", &response)?;
            transaction.commit().map_err(commit_failed)?;
            Ok(response)
        }
        LibraryActRequest::StartBatch {
            idempotency_key,
            batch_id,
            plan_digest,
            accepted_requirement_ids,
            maximum_accepted_estimate_by_currency,
            ..
        } => {
            if let Some(replay) = replay_receipt(&connection, idempotency_key, &digest)? {
                return Ok(replay);
            }
            let mut transaction = immediate_transaction(&mut connection)?;
            let outcome = start_batch(
                &mut transaction,
                batch_id,
                plan_digest,
                accepted_requirement_ids,
                maximum_accepted_estimate_by_currency,
                provider,
            )?;
            finish_act(
                root,
                transaction,
                outcome,
                idempotency_key,
                &digest,
                "start",
            )
        }
        LibraryActRequest::ControlBatch {
            idempotency_key,
            batch_id,
            control,
            ..
        } => {
            if let Some(replay) = replay_receipt(&connection, idempotency_key, &digest)? {
                return Ok(replay);
            }
            let mut transaction = immediate_transaction(&mut connection)?;
            let outcome = control_batch(&mut transaction, root, batch_id, control, provider)?;
            finish_act(
                root,
                transaction,
                outcome,
                idempotency_key,
                &digest,
                "control",
            )
        }
        LibraryActRequest::Change {
            idempotency_key,
            change,
            ..
        } => {
            if let Some(replay) = replay_receipt(&connection, idempotency_key, &digest)? {
                return Ok(replay);
            }
            let transaction = immediate_transaction(&mut connection)?;
            let response = apply_change(&transaction, change)?;
            record_receipt(&transaction, idempotency_key, &digest, "change", &response)?;
            transaction.commit().map_err(commit_failed)?;
            Ok(response)
        }
        LibraryActRequest::RecordReaderActivity {
            idempotency_key,
            activity,
            ..
        } => {
            if let Some(replay) = replay_receipt(&connection, idempotency_key, &digest)? {
                return Ok(replay);
            }
            let transaction = immediate_transaction(&mut connection)?;
            let context = crate::library_lifecycle::record_activity(&transaction, activity, &now())
                .map_err(from_lifecycle)?;
            let response = LibraryActResponse::RecordReaderActivity { context };
            record_receipt(&transaction, idempotency_key, &digest, "change", &response)?;
            transaction.commit().map_err(commit_failed)?;
            Ok(response)
        }
    }
}

fn from_lifecycle(error: crate::library_lifecycle::LifecycleError) -> LibraryActError {
    let code = match error.code {
        "invalid_query" => LibraryActErrorCode::InvalidQuery,
        "paper_not_found" => LibraryActErrorCode::PaperNotFound,
        "stale_library_snapshot" => LibraryActErrorCode::StaleLibrarySnapshot,
        _ => LibraryActErrorCode::WorkspaceUnavailable,
    };
    LibraryActError {
        code,
        message: error.message,
    }
}

fn apply_change(
    connection: &Connection,
    change: &LibraryChange,
) -> LibraryActResult<LibraryActResponse> {
    let timestamp = now();
    match change {
        LibraryChange::PatchLifecycle {
            paper_id,
            expected_version,
            patch,
        } => {
            let (lifecycle, wrote) = crate::library_lifecycle::apply_lifecycle_patch(
                connection,
                paper_id,
                *expected_version,
                patch,
                &timestamp,
            )
            .map_err(from_lifecycle)?;
            if wrote {
                bump_library_revisions(connection, &[LibraryDomain::Lifecycle])
                    .map_err(LibraryActError::unavailable)?;
            }
            Ok(LibraryActResponse::Change {
                result: ChangeResult::Lifecycle { lifecycle },
            })
        }
        LibraryChange::CreateSmartCollection { name, filters } => {
            let collection = crate::library_lifecycle::create_smart_collection(
                connection, name, filters, &timestamp,
            )
            .map_err(from_lifecycle)?;
            Ok(LibraryActResponse::Change {
                result: ChangeResult::SmartCollection { collection },
            })
        }
        LibraryChange::RenameSmartCollection { id, name } => {
            let collection =
                crate::library_lifecycle::rename_smart_collection(connection, id, name, &timestamp)
                    .map_err(from_lifecycle)?;
            Ok(LibraryActResponse::Change {
                result: ChangeResult::SmartCollection { collection },
            })
        }
        LibraryChange::DeleteSmartCollection { id } => {
            crate::library_lifecycle::delete_smart_collection(connection, id)
                .map_err(from_lifecycle)?;
            Ok(LibraryActResponse::Change {
                result: ChangeResult::SmartCollectionDeleted { id: id.clone() },
            })
        }
    }
}

/// 事务内跑完的分支直接落 receipt 并提交；需要事务外执行的分支先把「已开始」
/// 提交出去（释放写锁），跑完后再用最后一个事务把响应写进 receipt。
fn finish_act(
    root: &Path,
    transaction: Transaction<'_>,
    outcome: ActOutcome,
    idempotency_key: &str,
    digest: &str,
    operation: &str,
) -> LibraryActResult<LibraryActResponse> {
    let run = match outcome {
        ActOutcome::Done(response) => {
            record_receipt(&transaction, idempotency_key, digest, operation, &response)?;
            transaction.commit().map_err(commit_failed)?;
            return Ok(response);
        }
        ActOutcome::Deferred(run) => run,
    };
    transaction.commit().map_err(commit_failed)?;
    run_deferred_with_receipt(root, run, idempotency_key, digest, operation)
}

fn run_deferred_with_receipt(
    root: &Path,
    run: DeferredRun,
    idempotency_key: &str,
    digest: &str,
    operation: &str,
) -> LibraryActResult<LibraryActResponse> {
    let response = run_deferred(root, &run)?;
    let mut connection = open_db(root).map_err(LibraryActError::unavailable)?;
    let transaction = immediate_transaction(&mut connection)?;
    record_receipt(&transaction, idempotency_key, digest, operation, &response)?;
    transaction.commit().map_err(commit_failed)?;
    Ok(response)
}

/// COMMIT 失败不是「请求不合法」：锁冲突还能重试，其他写失败才是库不可用。
fn commit_failed(error: rusqlite::Error) -> LibraryActError {
    if is_busy(&error) {
        return LibraryActError::of(
            LibraryActErrorCode::WorkspaceBusy,
            "the library is busy; the batch was not committed, try again",
        );
    }
    LibraryActError::unavailable("the batch could not be committed")
}

fn is_busy(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(failure, _)
            if matches!(
                failure.code,
                rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
            )
    )
}

fn immediate_transaction(connection: &mut Connection) -> LibraryActResult<Transaction<'_>> {
    // §5.1：plan 必须 `BEGIN IMMEDIATE`。延迟升级会在并发写上进退为 SQLITE_BUSY。
    connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| LibraryActError::unavailable(error.to_string()))
}

fn validate_idempotency_key(key: &str) -> LibraryActResult<()> {
    let trimmed = key.trim();
    if trimmed.is_empty() || trimmed != key {
        return Err(LibraryActError::invalid(
            "idempotencyKey must be a non-empty key without surrounding spaces",
        ));
    }
    if key.chars().count() > MAX_IDEMPOTENCY_KEY_CHARS {
        return Err(LibraryActError::invalid(format!(
            "idempotencyKey is limited to {MAX_IDEMPOTENCY_KEY_CHARS} characters"
        )));
    }
    Ok(())
}

/// §5.2：同一 key + 同一请求返回原结果；同一 key + 不同请求返回 `idempotency_conflict`。
fn replay_receipt(
    connection: &Connection,
    key: &str,
    digest: &str,
) -> LibraryActResult<Option<LibraryActResponse>> {
    let row: Option<(String, String)> = connection
        .query_row(
            "SELECT request_digest, result_json FROM library_action_receipts WHERE idempotency_key = ?1",
            params![key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let Some((stored_digest, result_json)) = row else {
        return Ok(None);
    };
    if stored_digest != digest {
        return Err(LibraryActError::of(
            LibraryActErrorCode::IdempotencyConflict,
            "this idempotencyKey was already used for a different request",
        ));
    }
    let response: LibraryActResponse = serde_json::from_str(&result_json).map_err(|error| {
        LibraryActError::unavailable(format!("stored receipt is unreadable: {error}"))
    })?;
    Ok(Some(response))
}

/// receipt 与业务效果同一事务写入；只缓存成功，不缓存可重试的临时错误。
fn record_receipt(
    connection: &Connection,
    key: &str,
    digest: &str,
    result_kind: &str,
    response: &LibraryActResponse,
) -> LibraryActResult<()> {
    let result_json = serde_json::to_string(response)
        .map_err(|error| LibraryActError::invalid(error.to_string()))?;
    connection
        .execute(
            "INSERT INTO library_action_receipts(
               idempotency_key, request_digest, result_kind, result_ref, result_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(idempotency_key) DO NOTHING",
            params![
                key,
                digest,
                result_kind,
                response.receipt_ref(),
                result_json,
                now()
            ],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// plan
// ---------------------------------------------------------------------------

/// Item 的目标类型。schema 8 的 CHECK 只允许这两个值；导入项在执行成功前
/// 只有 `source` 身份，Paper 身份是执行的结果，不是计划的输入。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    Paper,
    Source,
}

impl TargetKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Paper => "paper",
            Self::Source => "source",
        }
    }
}

/// 一个待冻结的 Item 计划。
struct PlannedItem {
    ordinal: i64,
    target_kind: TargetKind,
    target_key: String,
    /// 源文件身份（canonical path 的 hash）。Paper 项留空：它的身份就是 `paper_id`。
    dedupe_key: Option<String>,
    paper_id: Option<String>,
    revision_id: Option<String>,
    precondition_digest: String,
    plan_json: String,
    compensation_json: Option<String>,
    /// 计划阶段就能定论的项（例如已经在目标目录里）直接落终态，
    /// 不给它一个「跑一次然后什么都不改」的执行。
    state: ItemState,
    /// 与 `state` 配套的结果摘要；`planned` / `queued` 项没有。
    result_json: Option<String>,
    prepared_job_handle: Option<String>,
}

/// 计划阶段的产物：逐项 Item、需要用户确认的聚合项、这一批的撤销能力。
struct PlanDraft {
    items: Vec<PlannedItem>,
    requirements: Vec<BatchRequirement>,
    undo_policy: UndoPolicy,
    cost: CostPreview,
}

fn plan_batch(
    transaction: &Transaction<'_>,
    root: &Path,
    command: &BatchCommand,
    target: &BatchTarget,
    provider: &ProviderActContext,
) -> LibraryActResult<LibraryActResponse> {
    // 规范化一次，之后计划摘要、入库的 command_json 和执行侧读到的载荷就是同一份值。
    let command = normalize_command(command)?;
    let kind = command_kind(&command);
    // 两种成员形态：Paper 命令选的是库里的 Paper，Import 选的是磁盘上的源文件。
    // 混用直接拒绝——猜一个形态就是把 500 个文件导入到用户没选的目录里。
    let (draft, target_digest) = match &command {
        BatchCommand::Import { collection_path } => {
            let BatchTarget::Sources { paths } = target else {
                return Err(LibraryActError::invalid(
                    "import needs a sources target: it imports files, not Papers already in the library",
                ));
            };
            let sources = resolve_sources(paths)?;
            let draft = plan_import_items(transaction, root, &sources, collection_path)?;
            let target_digest = canonical_digest(&json!({
                "sources": sources
                    .iter()
                    .map(|source| json!({ "dedupeKey": source.dedupe_key }))
                    .collect::<Vec<_>>(),
            }));
            (draft, target_digest)
        }
        _ => {
            if matches!(target, BatchTarget::Sources { .. }) {
                return Err(LibraryActError::invalid(
                    "this command works on Papers; it needs paperIds or a query selection",
                ));
            }
            let members = resolve_target(transaction, target)?;
            if members.is_empty() {
                return Err(LibraryActError::of(
                    LibraryActErrorCode::SelectionEmpty,
                    "the selection resolved to no Paper; nothing to do",
                ));
            }
            let draft = plan_items(transaction, root, &command, &members, provider)?;
            let target_digest = canonical_digest(&json!({
                "members": members
                    .iter()
                    .map(|member| json!({ "paperId": member.paper_id, "revisionId": member.revision_id }))
                    .collect::<Vec<_>>(),
            }));
            (draft, target_digest)
        }
    };
    let plan_digest = canonical_digest(&json!({
        "protocolVersion": crate::library_query::LIBRARY_PROTOCOL_VERSION,
        "commandKind": kind.as_str(),
        "command": command,
        "targetDigest": target_digest,
        // 确认项与撤销能力都是用户在 Start 前看到的那份承诺：确认文案或可逆程度变了，
        // 就不是同一个计划，不能拿旧 planDigest 蒙混过去。
        "requirements": draft.requirements,
        "undoPolicy": draft.undo_policy.as_str(),
        "cost": draft.cost,
        "priceCatalogVersion": PRICE_CATALOG_VERSION,
        "items": draft
            .items
            .iter()
            .map(|item| json!({
                "ordinal": item.ordinal,
                "targetKey": item.target_key,
                "state": item.state.as_str(),
                "preconditionDigest": item.precondition_digest,
            }))
            .collect::<Vec<_>>(),
    }));
    let batch_id = uuid::Uuid::new_v4().to_string();
    insert_batch(
        transaction,
        &batch_id,
        kind,
        &serde_json::to_string(&command).unwrap_or_else(|_| "{}".to_string()),
        &plan_digest,
        &target_digest,
        draft.undo_policy,
        &PlanSummary::provider(draft.requirements, draft.cost),
        None,
    )?;
    for item in &draft.items {
        insert_item(
            transaction,
            &uuid::Uuid::new_v4().to_string(),
            &batch_id,
            item,
            None,
        )?;
    }
    let batch = batch_projection(transaction, &batch_id)?;
    Ok(LibraryActResponse::PlanBatch { batch })
}

fn command_kind(command: &BatchCommand) -> BatchCommandKind {
    match command {
        BatchCommand::PatchTags { .. } => BatchCommandKind::PatchTags,
        BatchCommand::Move { .. } => BatchCommandKind::Move,
        BatchCommand::Trash => BatchCommandKind::Trash,
        BatchCommand::Import { .. } => BatchCommandKind::Import,
        BatchCommand::Export { .. } => BatchCommandKind::Export,
        BatchCommand::PatchLifecycle { .. } => BatchCommandKind::PatchLifecycle,
        BatchCommand::Ocr => BatchCommandKind::Ocr,
        BatchCommand::Brief => BatchCommandKind::Brief,
    }
}

/// 载荷规范化：标签走 `normalize_tags`，目录走单篇 `move_paper` 用的同一个
/// canonicalize，否则 `"Inbox"` 与 `"Papers/Inbox"` 会计划成两个不同的目标。
fn normalize_command(command: &BatchCommand) -> LibraryActResult<BatchCommand> {
    match command {
        BatchCommand::PatchTags { add, remove } => {
            let add = normalize_tags(add)?;
            let remove = normalize_tags(remove)?;
            if add.is_empty() && remove.is_empty() {
                return Err(LibraryActError::invalid(
                    "patch_tags needs at least one tag to add or remove",
                ));
            }
            Ok(BatchCommand::PatchTags { add, remove })
        }
        BatchCommand::Move { collection_path } => Ok(BatchCommand::Move {
            collection_path: normalize_collection_path(collection_path)?,
        }),
        BatchCommand::Trash => Ok(BatchCommand::Trash),
        // 导入的目标目录不能猜：UI 落在「全部」或 Smart Collection 上时必须先
        // 让用户选定 Papers / Textbooks 下的一个目录（§6.6）。
        BatchCommand::Import { collection_path } => {
            if collection_path.trim().is_empty() {
                return Err(LibraryActError::invalid(
                    "import needs the Collection the files are imported into",
                ));
            }
            Ok(BatchCommand::Import {
                collection_path: normalize_collection_path(collection_path)?,
            })
        }
        // 导出格式是封闭枚举，没有要规范化的载荷。这一支写在这里是为了
        // 第二种格式落地时必须在这里被显式判一次，而不是被通配分支放过去。
        BatchCommand::Export { format, locale } => Ok(BatchCommand::Export {
            format: *format,
            locale: *locale,
        }),
        BatchCommand::PatchLifecycle { patch } => {
            patch
                .validate()
                .map_err(|error| LibraryActError::invalid(error.message))?;
            Ok(BatchCommand::PatchLifecycle {
                patch: patch.clone(),
            })
        }
        BatchCommand::Ocr => Ok(BatchCommand::Ocr),
        BatchCommand::Brief => Ok(BatchCommand::Brief),
    }
}

fn normalize_collection_path(raw: &str) -> LibraryActResult<String> {
    if raw.trim().is_empty() {
        return Err(LibraryActError::invalid(
            "move needs a target collection path",
        ));
    }
    let path = crate::library_paths::canonicalize_collection_argument(Some(raw)).map_err(|_| {
        LibraryActError::invalid("collectionPath must be a Collection under Papers or Textbooks")
    })?;
    Ok(crate::library_paths::normalize_slashes(
        &path.to_string_lossy(),
    ))
}

/// 逐动作的计划策略。每个动作自己决定确认项与撤销能力，公共流程不猜。
fn plan_items(
    transaction: &Transaction<'_>,
    root: &Path,
    command: &BatchCommand,
    members: &[crate::library_query::SelectionMember],
    provider: &ProviderActContext,
) -> LibraryActResult<PlanDraft> {
    match command {
        BatchCommand::PatchTags { add, remove } => {
            plan_tag_items(transaction, members, add, remove)
        }
        BatchCommand::Move { collection_path } => {
            plan_move_items(transaction, root, members, collection_path)
        }
        BatchCommand::Trash => plan_trash_items(transaction, root, members),
        // 导入的成员是源文件，不是 Paper 成员：它走 [`plan_import_items`]。
        // 走到这里说明调用方把两种目标形态混在了一起，宁可拒绝也不猜。
        BatchCommand::Import { .. } => Err(LibraryActError::invalid(
            "import plans source items, not Paper members",
        )),
        BatchCommand::Export { format, locale } => {
            plan_export_items(transaction, root, members, *format, *locale)
        }
        BatchCommand::PatchLifecycle { patch } => plan_lifecycle_items(transaction, members, patch),
        BatchCommand::Ocr => {
            plan_provider_items(transaction, members, ProviderWorkKind::Ocr, provider)
        }
        BatchCommand::Brief => {
            plan_provider_items(transaction, members, ProviderWorkKind::Brief, provider)
        }
    }
}

fn plan_tag_items(
    transaction: &Connection,
    members: &[crate::library_query::SelectionMember],
    add: &[String],
    remove: &[String],
) -> LibraryActResult<PlanDraft> {
    let mut items = Vec::with_capacity(members.len());
    for (ordinal, member) in members.iter().enumerate() {
        let current = current_tags(transaction, &member.paper_id)?.ok_or_else(|| {
            LibraryActError::of(
                LibraryActErrorCode::PaperNotFound,
                "a target Paper disappeared while the batch was being planned",
            )
        })?;
        let (planned, compensation) = tag_item_plan(&member.paper_id, &current, add, remove);
        items.push(PlannedItem {
            ordinal: ordinal as i64,
            target_kind: TargetKind::Paper,
            target_key: member.paper_id.clone(),
            dedupe_key: None,
            paper_id: Some(member.paper_id.clone()),
            revision_id: Some(member.revision_id.clone()),
            precondition_digest: tag_precondition(&member.paper_id, &current),
            plan_json: store_plan(&planned),
            compensation_json: Some(store_plan(&compensation)),
            state: ItemState::Planned,
            result_json: None,
            prepared_job_handle: None,
        });
    }
    // 标签补丁没有确认项：before-image 是完整的，撤销能回到逐字相同的集合。
    Ok(PlanDraft {
        items,
        requirements: Vec::new(),
        undo_policy: UndoPolicy::Full,
        cost: CostPreview::empty(),
    })
}

fn plan_lifecycle_items(
    connection: &Connection,
    members: &[crate::library_query::SelectionMember],
    patch: &crate::library_lifecycle::LifecyclePatch,
) -> LibraryActResult<PlanDraft> {
    patch
        .validate()
        .map_err(|error| LibraryActError::invalid(error.message))?;
    let mut items = Vec::with_capacity(members.len());
    for (ordinal, member) in members.iter().enumerate() {
        let current = crate::library_lifecycle::load_lifecycle(connection, &member.paper_id)
            .map_err(|error| LibraryActError::unavailable(error.message))?;
        let compensation = json!({
            "status": current.status,
            "favorite": current.favorite,
            "priority": current.priority,
            "readLater": current.read_later,
            "reviewAt": current.review_at,
            "version": current.version,
        });
        items.push(PlannedItem {
            ordinal: ordinal as i64,
            target_kind: TargetKind::Paper,
            target_key: member.paper_id.clone(),
            dedupe_key: None,
            paper_id: Some(member.paper_id.clone()),
            revision_id: Some(member.revision_id.clone()),
            precondition_digest: crate::library_query::sha256_hex(
                format!("lifecycle:{}:{}", member.paper_id, current.version).as_bytes(),
            ),
            plan_json: store_plan(&json!({
                "action": "patch_lifecycle",
                "paperId": member.paper_id,
                "expectedVersion": current.version,
                "patch": patch,
            })),
            compensation_json: Some(store_plan(&compensation)),
            state: ItemState::Planned,
            result_json: None,
            prepared_job_handle: None,
        });
    }
    Ok(PlanDraft {
        items,
        requirements: Vec::new(),
        undo_policy: UndoPolicy::Full,
        cost: CostPreview::empty(),
    })
}

fn plan_provider_items(
    transaction: &Transaction<'_>,
    members: &[crate::library_query::SelectionMember],
    kind: ProviderWorkKind,
    provider: &ProviderActContext,
) -> LibraryActResult<PlanDraft> {
    let route = match kind {
        ProviderWorkKind::Ocr => {
            let frozen = provider.ocr.clone().ok_or_else(|| {
                LibraryActError::of(
                    LibraryActErrorCode::ProviderRouteUnavailable,
                    "configure Mistral OCR before planning a batch OCR",
                )
            })?;
            JobExecutionRoute::MistralOcr(frozen)
        }
        ProviderWorkKind::Brief => {
            if provider
                .textbook_brief_protocol
                .as_deref()
                .is_some_and(|p| !["v1", crate::textbook_contract::BRIEF_PROTOCOL].contains(&p))
            {
                return Err(LibraryActError::of(
                    LibraryActErrorCode::ProviderRouteUnavailable,
                    "无法识别教材 Brief 协议",
                ));
            }
            if [
                &provider.orientation_prompt,
                &provider.textbook_orientation_prompt,
                &provider.paper_root_prompt,
                &provider.textbook_root_prompt,
            ]
            .iter()
            .any(|prompt| prompt.as_deref().is_none_or(|text| text.trim().is_empty()))
            {
                return Err(LibraryActError::of(
                    LibraryActErrorCode::ProviderRouteUnavailable,
                    "Brief 与 PDF 来源根提示词必须在批量入队前冻结",
                ));
            }
            let frozen = provider.paper.clone().ok_or_else(|| {
                LibraryActError::of(
                    LibraryActErrorCode::ProviderRouteUnavailable,
                    "configure a paper provider before planning a batch Brief",
                )
            })?;
            JobExecutionRoute::Paper(frozen)
        }
    };
    let expires_at = future_minutes(PLAN_TTL_MINUTES);
    let mut items = Vec::with_capacity(members.len());
    let mut cost = CostPreview::empty();
    let mut long_pdfs = 0i64;
    let mut runnable = 0i64;
    for (ordinal, member) in members.iter().enumerate() {
        let facts = provider_item_facts(transaction, member)?;
        let skip_reason = match kind {
            ProviderWorkKind::Ocr if facts.has_ocr => Some("skipped_existing_ocr"),
            ProviderWorkKind::Brief if !facts.has_ocr => Some("skipped_missing_ocr"),
            ProviderWorkKind::Brief if facts.has_brief => Some("skipped_existing_brief"),
            _ => None,
        };
        if let Some(outcome) = skip_reason {
            items.push(PlannedItem {
                ordinal: ordinal as i64,
                target_kind: TargetKind::Paper,
                target_key: member.paper_id.clone(),
                dedupe_key: Some(facts.dedupe_key(kind)),
                paper_id: Some(member.paper_id.clone()),
                revision_id: Some(member.revision_id.clone()),
                precondition_digest: facts.precondition(kind),
                plan_json: store_plan(&facts.plan_json(kind, None)),
                compensation_json: None,
                state: ItemState::Skipped,
                result_json: Some(store_plan(&json!({ "outcome": outcome }))),
                prepared_job_handle: None,
            });
            continue;
        }
        if is_long_pdf(facts.page_count) {
            long_pdfs += 1;
        }
        let spec = facts.job_spec(kind, provider);
        let handle =
            prepare_exact_on(transaction, &spec, &route, &expires_at).map_err(|error| {
                LibraryActError::of(
                    LibraryActErrorCode::ProviderRouteUnavailable,
                    format!("the provider route could not be frozen: {error}"),
                )
            })?;
        let joined = find_active_job_id(transaction, &route, &spec.dedupe_key)
            .map_err(LibraryActError::unavailable)?
            .is_some();
        add_item_estimate(
            &mut cost,
            kind,
            facts.page_count,
            provider.paper.as_ref().map(|route| route.provider_kind()),
            joined,
        );
        runnable += 1;
        items.push(PlannedItem {
            ordinal: ordinal as i64,
            target_kind: TargetKind::Paper,
            target_key: member.paper_id.clone(),
            dedupe_key: Some(spec.dedupe_key.clone()),
            paper_id: Some(member.paper_id.clone()),
            revision_id: Some(member.revision_id.clone()),
            precondition_digest: facts.precondition(kind),
            plan_json: store_plan(&facts.plan_json(kind, Some(&handle.id))),
            compensation_json: None,
            state: ItemState::Planned,
            result_json: None,
            prepared_job_handle: Some(handle.id),
        });
    }
    let mut requirements = Vec::new();
    if runnable > 0 {
        requirements.push(BatchRequirement {
            id: REQUIREMENT_COST.to_string(),
            kind: BatchRequirementKind::Cost,
            item_count: runnable,
            label: format!(
                "{runnable} item(s) may incur provider charges; preview is not the final bill"
            ),
        });
    }
    if cost.unknown_item_count > 0 {
        requirements.push(BatchRequirement {
            id: REQUIREMENT_UNKNOWN_COST.to_string(),
            kind: BatchRequirementKind::UnknownCost,
            item_count: cost.unknown_item_count,
            label: format!(
                "{} item(s) have unknown cost and will not be shown as 0",
                cost.unknown_item_count
            ),
        });
    }
    if long_pdfs > 0 {
        requirements.push(BatchRequirement {
            id: REQUIREMENT_LONG_PDF.to_string(),
            kind: BatchRequirementKind::LongPdf,
            item_count: long_pdfs,
            label: format!("{long_pdfs} PDF(s) have {LONG_PDF_NOTE}"),
        });
    }
    Ok(PlanDraft {
        items,
        requirements,
        undo_policy: UndoPolicy::CancelOnly,
        cost,
    })
}

const LONG_PDF_NOTE: &str = "80 or more pages; confirm before sending them to a provider";

struct ProviderItemFacts {
    paper_id: String,
    revision_id: String,
    page_count: Option<i64>,
    has_ocr: bool,
    has_brief: bool,
    document_kind: String,
}

impl ProviderItemFacts {
    fn dedupe_key(&self, kind: ProviderWorkKind) -> String {
        match kind {
            ProviderWorkKind::Ocr => format!("{OCR_DEDUPE_PREFIX}{}", self.revision_id),
            ProviderWorkKind::Brief => format!("{BRIEF_DEDUPE_PREFIX}{}", self.revision_id),
        }
    }

    fn precondition(&self, kind: ProviderWorkKind) -> String {
        canonical_digest(&json!({
            "paperId": self.paper_id,
            "revisionId": self.revision_id,
            "kind": match kind {
                ProviderWorkKind::Ocr => "ocr",
                ProviderWorkKind::Brief => "brief",
            },
            "hasOcr": self.has_ocr,
            "hasBrief": self.has_brief,
            "pageCount": self.page_count,
            "catalog": PRICE_CATALOG_VERSION,
        }))
    }

    fn plan_json(&self, kind: ProviderWorkKind, handle: Option<&str>) -> Value {
        json!({
            "action": match kind {
                ProviderWorkKind::Ocr => OCR_PLAN_ACTION,
                ProviderWorkKind::Brief => BRIEF_PLAN_ACTION,
            },
            "paperId": self.paper_id,
            "revisionId": self.revision_id,
            "pageCount": self.page_count,
            "hasOcr": self.has_ocr,
            "hasBrief": self.has_brief,
            "priceCatalogVersion": PRICE_CATALOG_VERSION,
            "preparedJobHandle": handle,
        })
    }

    fn job_spec(&self, kind: ProviderWorkKind, provider: &ProviderActContext) -> JobSpec {
        match kind {
            ProviderWorkKind::Ocr => JobSpec {
                kind: "ocr".to_string(),
                provider: Some("mistral".to_string()),
                paper_id: Some(self.paper_id.clone()),
                revision_id: Some(self.revision_id.clone()),
                root_key: None,
                artifact_key: Some("ocr".to_string()),
                dedupe_key: self.dedupe_key(kind),
                priority: 100,
                payload: json!({ "model": MISTRAL_OCR_MODEL }),
            },
            ProviderWorkKind::Brief => {
                let document_kind = self.document_kind.clone();
                let textbook_v2 = document_kind == "textbook"
                    && provider.textbook_brief_protocol.as_deref()
                        == Some(crate::textbook_contract::BRIEF_PROTOCOL);
                let root_key = provider.paper.as_ref().map(|route| {
                    format!("{}:{}", self.revision_id, route.route_id().database_value())
                });
                JobSpec {
                    kind: "orientation_pack".to_string(),
                    provider: None,
                    paper_id: Some(self.paper_id.clone()),
                    revision_id: Some(self.revision_id.clone()),
                    root_key,
                    artifact_key: Some("brief".to_string()),
                    dedupe_key: self.dedupe_key(kind),
                    priority: 90,
                    payload: json!({
                        "documentArtifactProtocol": "v1",
                        "briefProtocol": if textbook_v2 {Some(crate::textbook_contract::BRIEF_PROTOCOL)}else{None},
                        "responseSchema": if textbook_v2 {crate::textbook_contract::response_schema()}else{crate::document_artifacts::response_schema(crate::document_artifacts::DocumentArtifactKind::Brief, document_kind=="textbook")},
                        "documentArtifactKind": "brief",
                        "prompts": {
                            "orientation": if document_kind == "textbook" { provider.textbook_orientation_prompt.clone() } else { provider.orientation_prompt.clone() }.unwrap_or_default(),
                            "paperRoot": if document_kind == "textbook" { provider.textbook_root_prompt.clone() } else { provider.paper_root_prompt.clone() }.unwrap_or_default(),
                            "documentKind": document_kind,
                        }
                    }),
                }
            }
        }
    }
}

fn provider_item_facts(
    connection: &Connection,
    member: &crate::library_query::SelectionMember,
) -> LibraryActResult<ProviderItemFacts> {
    let row: Option<(Option<i64>, String)> = connection
        .query_row(
            "SELECT r.page_count, p.relative_path
             FROM papers p
             JOIN paper_heads h ON h.paper_id = p.id
             JOIN document_revisions r ON r.id = h.revision_id
             WHERE p.id = ?1 AND p.deleted_at IS NULL AND r.id = ?2",
            params![member.paper_id, member.revision_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let Some((page_count, relative_path)) = row else {
        return Err(LibraryActError::of(
            LibraryActErrorCode::PaperNotFound,
            "this Paper is gone or no longer has that revision",
        ));
    };
    let has_ocr: i64 = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM ocr_revisions WHERE revision_id = ?1)",
            params![member.revision_id],
            |row| row.get(0),
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let has_brief: i64 = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM artifact_heads WHERE paper_id = ?1 AND kind = 'brief')",
            params![member.paper_id],
            |row| row.get(0),
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let document_kind = if relative_path.starts_with("Textbooks") {
        "textbook"
    } else {
        "paper"
    };
    Ok(ProviderItemFacts {
        paper_id: member.paper_id.clone(),
        revision_id: member.revision_id.clone(),
        page_count,
        has_ocr: has_ocr != 0,
        has_brief: has_brief != 0,
        document_kind: document_kind.to_string(),
    })
}

/// §7 Move 行：当前路径、目标、同根 / 跨根、kind-change 影响都要在计划里冻结。
fn plan_move_items(
    transaction: &Connection,
    root: &Path,
    members: &[crate::library_query::SelectionMember],
    collection_path: &str,
) -> LibraryActResult<PlanDraft> {
    let mut items = Vec::with_capacity(members.len());
    let mut claimed: BTreeSet<String> = BTreeSet::new();
    let mut kind_changes = 0i64;
    let mut conflicts = 0i64;
    for (ordinal, member) in members.iter().enumerate() {
        let location = live_location(transaction, &member.paper_id)?;
        let file_name = file_name_of(&location.relative_path);
        let target_relative = format!("{collection_path}/{file_name}");
        let same_destination = target_relative == location.relative_path;
        // 同批两项撞同一个目标也要算冲突：文件系统只会让后一项失败，
        // 但那是一次「计划里就看得出来」的失败，不该等 Start 之后才发现。
        let collision = !same_destination
            && (claimed.contains(&target_relative)
                || path_is_taken(transaction, root, &target_relative)?);
        claimed.insert(target_relative.clone());
        let kind_changed =
            crate::library_paths::document_kind_from_relative(&location.relative_path)
                != crate::library_paths::document_kind_from_relative(&target_relative);
        let dropped = if kind_changed {
            dropped_categories(transaction, &member.paper_id)?
        } else {
            Vec::new()
        };
        if kind_changed {
            kind_changes += 1;
        }
        if collision {
            conflicts += 1;
        }
        let plan = MoveItemPlan {
            action: MOVE_PLAN_ACTION.to_string(),
            paper_id: member.paper_id.clone(),
            collection_path: collection_path.to_string(),
            file_name: file_name.clone(),
            before_relative_path: location.relative_path.clone(),
            target_relative_path: target_relative.clone(),
            same_destination,
            collision,
            kind_changed,
            dropped_categories: dropped.clone(),
        };
        let compensation = MoveCompensation {
            paper_id: member.paper_id.clone(),
            collection_path: parent_of(&location.relative_path),
            file_name,
            expected_relative_path: target_relative,
            kind_changed,
        };
        let (state, result_json) = if same_destination {
            (
                ItemState::Skipped,
                Some(
                    json!({ "outcome": "already_in_target", "relativePath": location.relative_path })
                        .to_string(),
                ),
            )
        } else {
            (ItemState::Planned, None)
        };
        items.push(PlannedItem {
            ordinal: ordinal as i64,
            target_kind: TargetKind::Paper,
            target_key: member.paper_id.clone(),
            dedupe_key: None,
            paper_id: Some(member.paper_id.clone()),
            revision_id: Some(member.revision_id.clone()),
            precondition_digest: path_precondition(&member.paper_id, &location.relative_path),
            plan_json: store_plan(&plan),
            compensation_json: Some(store_plan(&compensation)),
            state,
            result_json,
            prepared_job_handle: None,
        });
    }
    let mut requirements = Vec::new();
    if kind_changes > 0 {
        requirements.push(BatchRequirement {
            id: REQUIREMENT_KIND_CHANGE.to_string(),
            kind: BatchRequirementKind::KindChange,
            item_count: kind_changes,
            label: format!(
                "{kind_changes} Paper(s) change document kind; their Brief, outline and guide artifacts are dropped"
            ),
        });
    }
    if conflicts > 0 {
        requirements.push(BatchRequirement {
            id: REQUIREMENT_CONFLICT.to_string(),
            kind: BatchRequirementKind::Conflict,
            item_count: conflicts,
            label: format!(
                "{conflicts} Paper(s) already have a file at the target path; those items will fail"
            ),
        });
    }
    Ok(PlanDraft {
        items,
        requirements,
        // 跨根 Move 会摘掉 kind-sensitive 成果，反向 Move 拿不回来：只有同根 Move
        // 才谈得上 full，其余都是补偿（§7.3「不做数据库时间旅行」）。
        undo_policy: if kind_changes > 0 {
            UndoPolicy::Compensating
        } else {
            UndoPolicy::Full
        },
        cost: CostPreview::empty(),
    })
}

/// §7 Trash 行：回收站资格、活跃 Job / 远端副作用提示在计划里冻结，destructive 确认。
fn plan_trash_items(
    transaction: &Connection,
    root: &Path,
    members: &[crate::library_query::SelectionMember],
) -> LibraryActResult<PlanDraft> {
    let mut items = Vec::with_capacity(members.len());
    let mut blocked = 0i64;
    for (ordinal, member) in members.iter().enumerate() {
        let location = live_location(transaction, &member.paper_id)?;
        let (active_jobs, committed_jobs) = job_exposure(transaction, &member.paper_id)?;
        // 回收站合同要求文件还在库里记着的位置；已经不在的项只能失败，
        // 让 `trash_paper` 去猜路径会把一次成功的移动写成第二次损失。
        let missing = !file_on_disk(root, &location.relative_path);
        if missing {
            blocked += 1;
        }
        let plan = TrashItemPlan {
            action: TRASH_PLAN_ACTION.to_string(),
            paper_id: member.paper_id.clone(),
            before_relative_path: location.relative_path.clone(),
            active_jobs,
            committed_jobs,
            source_missing: missing,
        };
        let compensation = TrashCompensation {
            paper_id: member.paper_id.clone(),
            original_relative_path: location.relative_path.clone(),
        };
        items.push(PlannedItem {
            ordinal: ordinal as i64,
            target_kind: TargetKind::Paper,
            target_key: member.paper_id.clone(),
            dedupe_key: None,
            paper_id: Some(member.paper_id.clone()),
            revision_id: Some(member.revision_id.clone()),
            precondition_digest: path_precondition(&member.paper_id, &location.relative_path),
            plan_json: store_plan(&plan),
            compensation_json: Some(store_plan(&compensation)),
            state: ItemState::Planned,
            result_json: None,
            prepared_job_handle: None,
        });
    }
    let mut requirements = vec![BatchRequirement {
        id: REQUIREMENT_DESTRUCTIVE.to_string(),
        kind: BatchRequirementKind::Destructive,
        item_count: items.len() as i64,
        label: format!(
            "{} Paper(s) move to Trash; running jobs are cancelled and only files inside the Workspace come back on undo",
            items.len()
        ),
    }];
    if blocked > 0 {
        requirements.push(BatchRequirement {
            id: REQUIREMENT_CONFLICT.to_string(),
            kind: BatchRequirementKind::Conflict,
            item_count: blocked,
            label: format!(
                "{blocked} Paper(s) have no file at the recorded path; those items will fail"
            ),
        });
    }
    Ok(PlanDraft {
        items,
        requirements,
        // Trash 撤销只能把文件放回原路径：被取消的 Job 与手排位置都不回来。
        undo_policy: UndoPolicy::Compensating,
        cost: CostPreview::empty(),
    })
}

/// 一个候选源文件。`path` 是 canonical 之后的绝对路径，只允许出现在逐项
/// `plan_json`（库里留档）——绝对路径不进摘要、事件或投影（§10.2）。
struct SourceCandidate {
    path: PathBuf,
    file_name: String,
    dedupe_key: String,
}

/// §6.6：把 `Sources` 目标解析成候选源。上限就是「同一次最多 500 个 source」。
/// 空白路径属于请求不合法；**路径上没有文件不算**——它要变成一个带结论的 Item，
/// 不能让整批拖放无反馈地消失。
fn resolve_sources(paths: &[String]) -> LibraryActResult<Vec<SourceCandidate>> {
    if paths.is_empty() {
        return Err(LibraryActError::invalid(
            "import needs at least one source file",
        ));
    }
    if paths.len() > MAX_BATCH_ITEMS {
        return Err(batch_too_large());
    }
    let mut sources = Vec::with_capacity(paths.len());
    for path in paths {
        let raw = path.trim();
        if raw.is_empty() {
            return Err(LibraryActError::invalid("source paths must not be blank"));
        }
        // canonicalize 失败也按原样留着：「文件不在」是一个逐项结论，不是请求错误。
        let resolved = fs::canonicalize(raw).unwrap_or_else(|_| PathBuf::from(raw));
        let Some(file_name) = resolved.file_name().and_then(|value| value.to_str()) else {
            return Err(LibraryActError::invalid(
                "a source path needs a file name to be imported",
            ));
        };
        sources.push(SourceCandidate {
            // 身份是 canonical path 的 hash，不是路径本身：同一份文件被拖进来两次时，
            // 两个 Item 的 dedupe_key 相同、ordinal 不同（§7）。
            dedupe_key: canonical_digest(&json!({
                "sourcePath": crate::library_paths::normalize_slashes(&resolved.to_string_lossy()),
            })),
            file_name: file_name.to_string(),
            path: resolved,
        });
    }
    Ok(sources)
}

/// 从失败项冻结的 `plan_json` 里取回它的源文件身份，供重试再预检一次。
/// 解不出来或不是导入计划都属于库里记录自相矛盾：那一项就是本模块自己写下的。
fn source_candidate_of(item: &ItemRow) -> LibraryActResult<SourceCandidate> {
    let plan = row_import_plan(item).map_err(|failure| {
        LibraryActError::unavailable(format!(
            "the retry could not read this item's source plan: {}",
            failure.safe_summary
        ))
    })?;
    Ok(SourceCandidate {
        path: PathBuf::from(&plan.source_path),
        file_name: plan.file_name,
        dedupe_key: plan.dedupe_key,
    })
}

/// §7 Import 行：逐项预检只做「便宜且不会与 `PaperModule` 漂移」的判断——是不是普通
/// 文件、是不是 `.pdf`、同批是不是重复源、目标路径是不是已经被占。
/// hash 与回收站归属**留给执行时**：那两条规则只有 `PaperModule` 一份实现，计划里
/// 复算一遍就会漂移；它的结论通过逐项 `outcome`
/// （`created_new | reused_existing | restored_existing | conflict`）回报。
fn plan_import_items(
    transaction: &Connection,
    root: &Path,
    sources: &[SourceCandidate],
    collection_path: &str,
) -> LibraryActResult<PlanDraft> {
    let mut items = Vec::with_capacity(sources.len());
    let mut seen_sources: BTreeSet<String> = BTreeSet::new();
    let mut claimed_targets: BTreeSet<String> = BTreeSet::new();
    let mut conflicts = 0i64;
    for (ordinal, source) in sources.iter().enumerate() {
        let target_relative = format!("{collection_path}/{}", source.file_name);
        let duplicate_source = !seen_sources.insert(source.dedupe_key.clone());
        let metadata = fs::metadata(&source.path).ok();
        let is_file = metadata.as_ref().is_some_and(|value| value.is_file());
        let is_pdf = source
            .file_name
            .rsplit_once('.')
            .is_some_and(|(_, extension)| extension.eq_ignore_ascii_case("pdf"));
        let byte_size = metadata
            .filter(|value| value.is_file())
            .map(|value| value.len())
            .unwrap_or(0);
        let (state, result_json, collision) = if !is_file || !is_pdf {
            // 目录、非 PDF 与丢失路径都在计划里定论：不给它一次「跑完然后什么都不改」的执行。
            let outcome = if !is_file {
                IMPORT_OUTCOME_SOURCE_MISSING
            } else {
                IMPORT_OUTCOME_NOT_PDF
            };
            (
                ItemState::Skipped,
                Some(json!({ "outcome": outcome }).to_string()),
                false,
            )
        } else if duplicate_source {
            (
                ItemState::Skipped,
                Some(json!({ "outcome": IMPORT_OUTCOME_DUPLICATE_SOURCE }).to_string()),
                false,
            )
        } else {
            let collision = claimed_targets.contains(&target_relative)
                || path_is_taken(transaction, root, &target_relative)?;
            if collision {
                conflicts += 1;
            }
            // 只有真会写文件的项才占住目标路径：计划里已经定论为 skipped 的源
            // 永远不会产出文件，占位会让后面那份同名 PDF 白报一个冲突。
            claimed_targets.insert(target_relative.clone());
            (ItemState::Planned, None, collision)
        };
        let plan = ImportItemPlan {
            action: IMPORT_PLAN_ACTION.to_string(),
            source_path: source.path.to_string_lossy().to_string(),
            file_name: source.file_name.clone(),
            collection_path: collection_path.to_string(),
            target_relative_path: target_relative,
            dedupe_key: source.dedupe_key.clone(),
            byte_size,
            collision,
        };
        let short_key: String = source.dedupe_key.chars().take(12).collect();
        items.push(PlannedItem {
            ordinal: ordinal as i64,
            target_kind: TargetKind::Source,
            // §7：source 项的 target key 含 ordinal——同一份文件在一个 input 里出现
            // 两次也要留下两个独立项，只按文件身份去重会让第二项无处可写。
            target_key: format!("source-{ordinal}-{short_key}"),
            dedupe_key: Some(source.dedupe_key.clone()),
            paper_id: None,
            revision_id: None,
            precondition_digest: source_precondition(&source.dedupe_key, byte_size),
            plan_json: store_plan(&plan),
            // 补偿要等执行结果才存在：只有 `created_new` 才配回收站补偿。
            compensation_json: None,
            state,
            result_json,
            prepared_job_handle: None,
        });
    }
    let mut requirements = Vec::new();
    if conflicts > 0 {
        requirements.push(BatchRequirement {
            id: REQUIREMENT_CONFLICT.to_string(),
            kind: BatchRequirementKind::Conflict,
            item_count: conflicts,
            label: format!(
                "{conflicts} source(s) already have a file at the target path; those items end as a conflict instead of a new Paper"
            ),
        });
    }
    Ok(PlanDraft {
        items,
        requirements,
        // §7：只有本批新建的 Paper 能补偿回回收站；复用 / restore / 冲突都不配补偿，
        // 撤销绝不能伤害导入前就已存在的 Paper。
        undo_policy: UndoPolicy::Compensating,
        cost: CostPreview::empty(),
    })
}

/// §7 Export 行：格式、输出目标与命名与覆盖策略都要在计划里冻结。
///
/// 「预览说会写到哪」与「执行时真的写到哪」用的是 `export_module` 里同一条规则
/// （[`crate::export_module::planned_output_relative`]）——导出文件是用户拿出去给
/// 别人看的产物，预览与落盘不一致是最糟的一种漂移。
fn plan_export_items(
    transaction: &Connection,
    root: &Path,
    members: &[crate::library_query::SelectionMember],
    format: ExportFormat,
    locale: crate::ui_locale::UiLocale,
) -> LibraryActResult<PlanDraft> {
    let mut items = Vec::with_capacity(members.len());
    let mut replacements = 0i64;
    for (ordinal, member) in members.iter().enumerate() {
        let facts = export_item_facts(transaction, &member.paper_id)?;
        let planned_relative_path =
            crate::export_module::planned_output_relative(root, &facts.naming);
        // 覆盖只看磁盘上那一份真实存在的文件。同批里两份同名 PDF 不算：命名规则会把
        // 后一份让到 `父目录_名.md`，谁也不会盖掉谁，那不是「不可逆影响」，
        // 也就无处需要确认——真实落点与预览不符时会留下 `matchesPlan`。
        let replaces = crate::export_module::output_on_disk(root, &planned_relative_path).is_some();
        if replaces {
            replacements += 1;
        }
        let plan = ExportItemPlan {
            locale,
            action: EXPORT_PLAN_ACTION.to_string(),
            paper_id: member.paper_id.clone(),
            format,
            planned_relative_path,
            replaces,
        };
        items.push(PlannedItem {
            ordinal: ordinal as i64,
            target_kind: TargetKind::Paper,
            target_key: member.paper_id.clone(),
            dedupe_key: None,
            paper_id: Some(member.paper_id.clone()),
            revision_id: Some(member.revision_id.clone()),
            // 导出正文印着库内相对路径，输出文件名又看着它所在目录：
            // Paper 在计划之后换了位置，这一项要写的就不是预览里那一份。
            precondition_digest: path_precondition(&member.paper_id, &facts.relative_path),
            plan_json: store_plan(&plan),
            // 能不能撤销要等写下去那一刻才知道：被覆盖的那一份回不来。
            compensation_json: None,
            state: ItemState::Planned,
            result_json: None,
            prepared_job_handle: None,
        });
    }
    let mut requirements = Vec::new();
    if replacements > 0 {
        requirements.push(BatchRequirement {
            id: REQUIREMENT_OVERWRITE.to_string(),
            kind: BatchRequirementKind::Overwrite,
            item_count: replacements,
            label: format!(
                "{replacements} export(s) replace a file already in the export folder; the replaced version is not recoverable"
            ),
        });
    }
    Ok(PlanDraft {
        items,
        requirements,
        // 撤销只删本批新建的输出；覆盖掉的旧文件不是这个批次写的，删掉才是损失。
        undo_policy: UndoPolicy::Compensating,
        cost: CostPreview::empty(),
    })
}

/// 一项导出的计划要读的库内事实：命名规则的输入，加上这一项的 precondition
/// 要看的那个当前位置。合并成一次查询，是因为「这个 Paper 还在库里」这件事
/// 不该由两条 SQL 各判一次——两条都说得通的时候，谁也不知道哪一条算。
fn export_item_facts(connection: &Connection, paper_id: &str) -> LibraryActResult<ExportItemFacts> {
    let row: Option<(String, String, String, String)> = connection
        .query_row(
            "SELECT p.file_name, p.relative_path, c.relative_path, r.sha256
             FROM papers p
             JOIN paper_heads h ON h.paper_id = p.id
             JOIN document_revisions r ON r.id = h.revision_id
             JOIN collections c ON c.id = p.collection_id
             WHERE p.id = ?1 AND p.deleted_at IS NULL",
            params![paper_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let Some((file_name, relative_path, collection_path, sha256)) = row else {
        return Err(LibraryActError::of(
            LibraryActErrorCode::PaperNotFound,
            "a target Paper is not in the library",
        ));
    };
    Ok(ExportItemFacts {
        relative_path: crate::library_paths::normalize_slashes(&relative_path),
        naming: crate::export_module::NamingFacts {
            file_name,
            collection_path: crate::library_paths::normalize_slashes(&collection_path),
            sha256,
        },
    })
}

/// [`export_item_facts`] 的产物。
struct ExportItemFacts {
    relative_path: String,
    naming: crate::export_module::NamingFacts,
}

/// Paper 在库内的位置。`relative_path` 是文件系统与库里共同的事实，
/// `file_name` 一律从它推导，不信任可能滞后的列。
#[derive(Debug, Clone)]
struct PaperLocation {
    relative_path: String,
    deleted: bool,
}

fn paper_location(
    connection: &Connection,
    paper_id: &str,
) -> LibraryActResult<Option<PaperLocation>> {
    let row: Option<(String, Option<String>)> = connection
        .query_row(
            "SELECT p.relative_path, p.deleted_at FROM papers p WHERE p.id = ?1",
            params![paper_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    Ok(row.map(|(relative_path, deleted_at)| PaperLocation {
        relative_path: crate::library_paths::normalize_slashes(&relative_path),
        deleted: deleted_at.is_some(),
    }))
}

/// 与 `current_tags` 同一口径：行不存在或已在回收站里，都算「这个 Paper 不在库里」。
fn live_location(connection: &Connection, paper_id: &str) -> LibraryActResult<PaperLocation> {
    paper_location(connection, paper_id)?
        .filter(|location| !location.deleted)
        .ok_or_else(|| {
            LibraryActError::of(
                LibraryActErrorCode::PaperNotFound,
                "a target Paper is not in the library",
            )
        })
}

/// Trash 的影响面：多少个在跑，其中多少个已经让 Provider 侧产生了副作用。
fn job_exposure(connection: &Connection, paper_id: &str) -> LibraryActResult<(i64, i64)> {
    let row: (i64, i64) = connection
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(provider_committed), 0) FROM jobs
             WHERE paper_id = ?1 AND state IN ('queued','running','paused')",
            params![paper_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap_or((0, 0));
    Ok(row)
}

fn file_name_of(relative_path: &str) -> String {
    relative_path
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(relative_path)
        .to_string()
}

fn parent_of(relative_path: &str) -> String {
    match relative_path.rsplit_once('/') {
        Some((parent, _)) if !parent.is_empty() => parent.to_string(),
        _ => String::new(),
    }
}

/// 目标路径是否已被占用：库里的另一个 Paper，或磁盘上还没有归属的文件。
/// 两种占用都必须失败而不能覆盖——`move_paper` 自己也是这么处理的。
fn path_is_taken(
    connection: &Connection,
    root: &Path,
    relative_path: &str,
) -> LibraryActResult<bool> {
    let owned: Option<String> = connection
        .query_row(
            "SELECT p.id FROM papers p WHERE p.relative_path = ?1 LIMIT 1",
            params![relative_path],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    if owned.is_some() {
        return Ok(true);
    }
    Ok(file_on_disk(root, relative_path))
}

/// 库记着的路径上是否真有一个文件。解析不出合法 Workspace 相对路径同样算「没有文件」：
/// 资格判断在计划里只做提示，真正的拒绝留给执行时的 `PaperModule`。
fn file_on_disk(root: &Path, relative_path: &str) -> bool {
    crate::library_paths::join_workspace_relative(root, relative_path)
        .map(|path| path.is_file())
        .unwrap_or(false)
}

/// 跨根 Move 会真的删掉的成果类别（`apply_kind_change` 的清单）。逐项列出来，
/// 确认页才能按 §7 的要求「单独列出会摘除的成果类别」。
fn dropped_categories(connection: &Connection, paper_id: &str) -> LibraryActResult<Vec<String>> {
    let mut categories: BTreeSet<String> = BTreeSet::new();
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT kind FROM artifact_heads WHERE paper_id = ?1
             AND kind IN ('brief','glossary','symbol_table','metadata','reading_roadmap')",
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    for kind in statement
        .query_map(params![paper_id], |row| row.get::<_, String>(0))
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?
    {
        categories.insert(kind.map_err(|error| LibraryActError::unavailable(error.to_string()))?);
    }
    for (table, label) in [
        ("outline_heads", "outline"),
        ("reading_guide_heads", "reading_guide"),
    ] {
        let exists: i64 = connection
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} h JOIN document_revisions r ON r.id = h.revision_id WHERE r.paper_id = ?1"),
                params![paper_id],
                |row| row.get(0),
            )
            .unwrap_or(0);
        if exists > 0 {
            categories.insert(label.to_string());
        }
    }
    Ok(categories.into_iter().collect())
}

fn store_plan<T: Serialize>(plan: &T) -> String {
    serde_json::to_string(plan).unwrap_or_else(|_| "{}".to_string())
}

fn tag_precondition_value(paper_id: &str, tags: &[String]) -> Value {
    json!({ "paperId": paper_id, "tags": tags })
}

fn tag_precondition(paper_id: &str, tags: &[String]) -> String {
    canonical_digest(&tag_precondition_value(paper_id, tags))
}

/// 路径类前置条件：正向动作记「还在原位」，执行成功后记「在新位」。
fn path_precondition_value(paper_id: &str, relative_path: &str) -> Value {
    json!({ "paperId": paper_id, "relativePath": relative_path })
}

fn path_precondition(paper_id: &str, relative_path: &str) -> String {
    canonical_digest(&path_precondition_value(paper_id, relative_path))
}

/// 回收站里的 after-image：库里的事实是「已删除 + 原路径」，撤销按这个形状复验。
fn trash_precondition_value(paper_id: &str, original_relative_path: &str) -> Value {
    json!({
        "paperId": paper_id,
        "relativePath": original_relative_path,
        "trashed": true,
    })
}

/// 源文件的前置条件：文件身份 + 计划时看到的字节数。绝对路径不进摘要（§10.2），
/// 所以进的是 `dedupe_key`。字节数没变而内容变了的情况这里挡不住，也不需要挡——
/// 执行时 `PaperModule` 按 hash 判定，同一份内容会得到 `reused_existing`，
/// 不同的内容会得到 `conflict`，两种都不会写坏库。
fn source_precondition_value(dedupe_key: &str, byte_size: u64) -> Value {
    json!({ "sourceKey": dedupe_key, "byteSize": byte_size })
}

fn source_precondition(dedupe_key: &str, byte_size: u64) -> String {
    canonical_digest(&source_precondition_value(dedupe_key, byte_size))
}

/// 标签名统一 trim + 去空 + 去重，保持调用方给出的顺序。
fn normalize_tags(tags: &[String]) -> LibraryActResult<Vec<String>> {
    if tags.len() > MAX_TAG_PATCH {
        return Err(LibraryActError::invalid(format!(
            "a tag patch accepts at most {MAX_TAG_PATCH} names"
        )));
    }
    let mut seen = BTreeSet::new();
    let mut cleaned = Vec::new();
    for tag in tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return Err(LibraryActError::invalid("tag names cannot be empty"));
        }
        if trimmed.chars().count() > MAX_TAG_NAME {
            return Err(LibraryActError::invalid(format!(
                "tag is limited to {MAX_TAG_NAME} characters"
            )));
        }
        if seen.insert(trimmed.to_lowercase()) {
            cleaned.push(trimmed.to_string());
        }
    }
    Ok(cleaned)
}

/// 一个 Paper 当前的标签名（字典序，便于摘要稳定）。
fn current_tags(connection: &Connection, paper_id: &str) -> LibraryActResult<Option<Vec<String>>> {
    let exists: Option<String> = connection
        .query_row(
            "SELECT p.id FROM papers p WHERE p.id = ?1 AND p.deleted_at IS NULL",
            params![paper_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    if exists.is_none() {
        return Ok(None);
    }
    let mut statement = connection
        .prepare(
            "SELECT t.name FROM paper_tags pt JOIN tags t ON t.id = pt.tag_id
             WHERE pt.paper_id = ?1 ORDER BY lower(t.name) ASC, t.name ASC",
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let tags = statement
        .query_map(params![paper_id], |row| row.get::<_, String>(0))
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    Ok(Some(tags))
}

/// 逐项计划与补偿 before-image。§7：标签必须走 add/remove patch，
/// 不允许「读出后覆盖完整数组」的 read-modify-write。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TagItemPlan {
    /// 判别列：`patch_tags`。同一张 Items 表之后还要放 Move / Trash 的计划，
    /// 所以执行前必须核对，不能靠「能解析成这个结构」来推断。
    action: String,
    paper_id: String,
    add: Vec<String>,
    remove: Vec<String>,
    before_tags: Vec<String>,
    /// 预演的最终集合；执行时仍以库内当前值重算，这里只用于审计与摘要。
    planned_tags: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TagCompensation {
    paper_id: String,
    tags: Vec<String>,
}

fn tag_item_plan(
    paper_id: &str,
    current: &[String],
    add: &[String],
    remove: &[String],
) -> (TagItemPlan, TagCompensation) {
    let planned = apply_tag_patch(current, add, remove);
    (
        TagItemPlan {
            action: TAG_PLAN_ACTION.to_string(),
            paper_id: paper_id.to_string(),
            add: add.to_vec(),
            remove: remove.to_vec(),
            before_tags: current.to_vec(),
            planned_tags: planned.clone(),
        },
        TagCompensation {
            paper_id: paper_id.to_string(),
            tags: planned,
        },
    )
}

fn apply_tag_patch(current: &[String], add: &[String], remove: &[String]) -> Vec<String> {
    let removed: BTreeSet<String> = remove.iter().map(|tag| tag.to_lowercase()).collect();
    let mut merged: BTreeMap<String, String> = BTreeMap::new();
    for tag in current.iter().chain(add.iter()) {
        let key = tag.to_lowercase();
        if removed.contains(&key) {
            continue;
        }
        merged.entry(key).or_insert_with(|| tag.to_string());
    }
    merged.into_values().collect()
}

/// §7 Move 行的逐项计划。路径一律是 Workspace 相对路径：绝对路径不允许进
/// 任何投影或事件（§10.2），计划文本也一样。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveItemPlan {
    /// 判别列：`move`。
    action: String,
    paper_id: String,
    collection_path: String,
    file_name: String,
    before_relative_path: String,
    target_relative_path: String,
    same_destination: bool,
    collision: bool,
    kind_changed: bool,
    dropped_categories: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveCompensation {
    paper_id: String,
    /// 反向 Move 的目标：计划时冻结的原目录。
    collection_path: String,
    file_name: String,
    /// 撤销的前置条件：只有 Paper 仍在批次放下的位置上时才补偿。
    expected_relative_path: String,
    kind_changed: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrashItemPlan {
    /// 判别列：`trash`。
    action: String,
    paper_id: String,
    before_relative_path: String,
    active_jobs: i64,
    committed_jobs: i64,
    source_missing: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrashCompensation {
    paper_id: String,
    original_relative_path: String,
}

/// §7 Import 行的逐项计划。`sourcePath` 是绝对路径，而 `plan_json` 只写进库里留档、
/// 不进任何投影与事件（§10.2），所以这是绝对路径在本模块唯一的容身处。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportItemPlan {
    /// 判别列：`import`。
    action: String,
    source_path: String,
    file_name: String,
    collection_path: String,
    target_relative_path: String,
    dedupe_key: String,
    /// 计划时看到的字节数：Start 前源文件又被人写过就要重新预览。
    byte_size: u64,
    /// 同批另一个源占住了同一个目标路径，或库里 / 磁盘上已经有文件。
    collision: bool,
}

/// 导入项的补偿：只有 `created_new` 才有，动作就是把新建的 Paper 移进回收站。
/// 用户的源文件永远不动——复制进来的那份归库管，原件不属于这次批次。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportCompensation {
    /// 判别列：`trash_created`。
    mode: String,
    paper_id: String,
    /// 撤销的前置条件：这个 Paper 还在本批放下的位置上。
    original_relative_path: String,
}

/// §7 Export 行的逐项计划。`planned_relative_path` 是预览承诺的输出位置，
/// `replaces` 是「写下去会盖掉已有文件」这件事在 Start 之前就要用户认过。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportItemPlan {
    #[serde(default)]
    locale: crate::ui_locale::UiLocale,
    /// 判别列：`export`。
    action: String,
    paper_id: String,
    /// 执行时按它分派渲染器；计划与执行读同一份冻结值，不会跑成另一种格式。
    format: ExportFormat,
    planned_relative_path: String,
    replaces: bool,
}

/// 导出项的补偿：删掉本批新建的那一份输出。`content_digest` 是这一条补偿的全部
/// 安全边界——路径上的文件字节变了就说明有人在动它，那时宁可什么都不删。
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportCompensation {
    /// 判别列：`delete_output`。
    mode: String,
    paper_id: String,
    relative_path: String,
    content_digest: String,
}

#[allow(clippy::too_many_arguments)]
fn insert_batch(
    connection: &Connection,
    id: &str,
    kind: BatchCommandKind,
    command_json: &str,
    plan_digest: &str,
    target_digest: &str,
    undo_policy: UndoPolicy,
    summary: &PlanSummary,
    // `(parentBatchId, relation)`：child 两者总是一起出现，拆成独立参数会让
    // 「有 relation 没有父」这种脏行有机可乘。child 跑哪个动作由父链解析得到，
    // 不在 command_json 里再存一份可能过期的副本。
    parent: Option<(&str, &str)>,
) -> LibraryActResult<()> {
    let (parent_batch_id, relation) = match parent {
        Some((parent_id, relation)) => (Some(parent_id.to_string()), Some(relation.to_string())),
        None => (None, None),
    };
    let command_value: Value = serde_json::from_str(command_json)
        .map_err(|error| LibraryActError::invalid(error.to_string()))?;
    let timestamp = now();
    connection
        .execute(
            "INSERT INTO library_batches(
               id, command_kind, command_json, state, plan_digest, target_digest,
               cost_preview_json, approval_json, parent_batch_id, relation, undo_policy,
               plan_expires_at, cancel_requested_at, created_at, started_at, finished_at, updated_at
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,NULL,?8,?9,?10,?11,NULL,?12,NULL,NULL,?12)",
            params![
                id,
                kind.as_str(),
                command_value.to_string(),
                BatchState::Planned.as_str(),
                plan_digest,
                target_digest,
                summary.store(),
                parent_batch_id,
                relation,
                undo_policy.as_str(),
                future_minutes(PLAN_TTL_MINUTES),
                timestamp,
            ],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    Ok(())
}

fn insert_item(
    connection: &Connection,
    id: &str,
    batch_id: &str,
    item: &PlannedItem,
    source_item_id: Option<&str>,
) -> LibraryActResult<()> {
    let timestamp = now();
    let finished: Option<&str> = item.state.is_terminal().then_some(timestamp.as_str());
    connection
        .execute(
            "INSERT INTO library_batch_items(
               id, batch_id, source_item_id, ordinal, target_kind, target_key, dedupe_key,
               paper_id, revision_id, prepared_job_handle, precondition_digest, state,
               plan_json, result_json, compensation_json, error_code, error_summary,
               attempt_count, started_at, finished_at, updated_at
             ) VALUES (?1,?2,?14,?3,?15,?4,?16,?5,?6,?17,?7,?8,?9,?10,?11,NULL,NULL,0,NULL,?12,?13)",
            params![
                id,
                batch_id,
                item.ordinal,
                item.target_key,
                item.paper_id,
                item.revision_id,
                item.precondition_digest,
                item.state.as_str(),
                item.plan_json,
                item.result_json,
                item.compensation_json,
                finished,
                timestamp,
                source_item_id,
                item.target_kind.as_str(),
                item.dedupe_key,
                item.prepared_job_handle,
            ],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    Ok(())
}

/// §5.1：把选择解析成成员，并复验 Snapshot。显式列表按调用顺序去重。
fn resolve_target(
    connection: &Connection,
    target: &BatchTarget,
) -> LibraryActResult<Vec<crate::library_query::SelectionMember>> {
    match target {
        BatchTarget::Explicit { paper_ids } => {
            if paper_ids.len() > MAX_BATCH_ITEMS {
                return Err(batch_too_large());
            }
            let mut seen = BTreeSet::new();
            let mut members = Vec::new();
            for paper_id in paper_ids {
                if paper_id.trim().is_empty() {
                    return Err(LibraryActError::invalid("paperIds must not contain blanks"));
                }
                if !seen.insert(paper_id.clone()) {
                    continue;
                }
                let revision_id: Option<String> = connection
                    .query_row(
                        "SELECT h.revision_id FROM papers p
                         JOIN paper_heads h ON h.paper_id = p.id
                         WHERE p.id = ?1 AND p.deleted_at IS NULL",
                        params![paper_id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
                let Some(revision_id) = revision_id else {
                    return Err(LibraryActError::of(
                        LibraryActErrorCode::PaperNotFound,
                        format!("paper {paper_id} is not in the library"),
                    ));
                };
                members.push(crate::library_query::SelectionMember {
                    paper_id: paper_id.clone(),
                    revision_id,
                });
            }
            Ok(members)
        }
        BatchTarget::Query {
            filters,
            excluded_ids,
            selection_digest: expected_digest,
            dependency_revisions,
            evaluation_anchor,
            ..
        } => {
            let current_digest = selection_digest(filters);
            if current_digest != *expected_digest {
                return Err(LibraryActError::of(
                    LibraryActErrorCode::StaleSelection,
                    "the query behind this selection changed; re-select before submitting",
                ));
            }
            let revisions = library_revisions(connection).map_err(LibraryActError::unavailable)?;
            let domains = selection_dependencies(filters);
            let current_vector = revisions.domain_revisions(&domains);
            if !same_vector(&current_vector, dependency_revisions) {
                return Err(LibraryActError::of(
                    LibraryActErrorCode::StaleSelection,
                    "a dependency of this selection moved since it was evaluated; re-select first",
                ));
            }
            let anchor = evaluation_anchor.clone().unwrap_or_else(now);
            let resolved = crate::library_query::resolve_selection_at(
                connection,
                filters,
                MAX_BATCH_ITEMS,
                &anchor,
            )
            .map_err(LibraryActError::from_query)?;
            let Some(members) = resolved else {
                return Err(batch_too_large());
            };
            let excluded: BTreeSet<String> = excluded_ids.iter().cloned().collect();
            Ok(members
                .into_iter()
                .filter(|member| !excluded.contains(&member.paper_id))
                .collect())
        }
        // 源文件目标解析不出 Paper 成员：它走 [`resolve_sources`]。`plan_batch`
        // 已经按命令把两种形态分开路由，走到这里只剩「调用方绕过了路由」一种可能，
        // 所以这是一道守卫而不是一个策略。
        BatchTarget::Sources { .. } => Err(LibraryActError::invalid(
            "a source target resolves to files, not to Papers already in the library",
        )),
    }
}

fn batch_too_large() -> LibraryActError {
    LibraryActError::of(
        LibraryActErrorCode::BatchTooLarge,
        format!("a batch covers at most {MAX_BATCH_ITEMS} targets; split the selection"),
    )
}

fn same_vector(stored: &[DomainRevision], presented: &[DomainRevision]) -> bool {
    let left: BTreeMap<_, _> = stored
        .iter()
        .map(|entry| (entry.domain.clone(), entry.value))
        .collect();
    let right: BTreeMap<_, _> = presented
        .iter()
        .map(|entry| (entry.domain.clone(), entry.value))
        .collect();
    left == right
}

// ---------------------------------------------------------------------------
// start / execute
// ---------------------------------------------------------------------------

fn start_batch(
    transaction: &mut Transaction<'_>,
    batch_id: &str,
    plan_digest: &str,
    accepted_requirement_ids: &[String],
    maximum_accepted_estimate_by_currency: &BTreeMap<String, String>,
    provider: &ProviderActContext,
) -> LibraryActResult<ActOutcome> {
    let row = load_batch(transaction, batch_id)?;
    if row.plan_digest != plan_digest {
        return Err(LibraryActError::of(
            LibraryActErrorCode::StaleBatchPlan,
            "planDigest does not match the stored plan; preview again",
        ));
    }
    let state = BatchState::parse(&row.state).ok_or_else(|| {
        LibraryActError::unavailable(format!("unknown stored batch state {}", row.state))
    })?;
    if state != BatchState::Planned {
        // Gate：重复 start 不重复执行。终结项永远不重跑；但事务外执行的中途崩溃
        // 会留下没开始的项，那属于同一批次的续跑，不是第二次执行。
        if let Some(run) = resume_deferred_run(transaction, batch_id)? {
            return Ok(ActOutcome::Deferred(run));
        }
        return Ok(ActOutcome::Done(start_response(
            transaction,
            batch_id,
            None,
        )?));
    }
    if row
        .plan_expires_at
        .as_deref()
        .is_some_and(|expires| expires <= now().as_str())
    {
        return Err(LibraryActError::of(
            LibraryActErrorCode::StaleBatchPlan,
            "the plan expired; preview again before running it",
        ));
    }
    let summary = PlanSummary::load(&row.cost_preview_json);
    let accepted: BTreeSet<&str> = accepted_requirement_ids
        .iter()
        .map(String::as_str)
        .collect();
    for requirement in &summary.requirements {
        if !accepted.contains(requirement.id.as_str()) {
            return Err(LibraryActError::of(
                LibraryActErrorCode::ConfirmationRequired,
                format!(
                    "the plan needs an explicit confirmation: {}",
                    requirement.label
                ),
            ));
        }
    }
    // §8.2：`maximumAcceptedEstimateByCurrency` 只是「确认后估算不得悄悄上升」的
    // 本地门槛，不是 Provider 账单上限。估算上升必须重新预览。
    validate_estimate_ceilings(maximum_accepted_estimate_by_currency)?;
    enforce_cost_ceilings(&summary.cost, maximum_accepted_estimate_by_currency)?;
    // 批次创建后成员固定：Start 不重新解析选择，只按逐项 precondition 检查实际依赖。
    let pending = load_items(transaction, batch_id, None)?;
    let started_at = now();
    transaction
        .execute(
            "UPDATE library_batches SET state = ?2, started_at = ?3, updated_at = ?3 WHERE id = ?1",
            params![batch_id, BatchState::Running.as_str(), started_at],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let action = resolve_action(action_kind_for(transaction, batch_id)?, false)?;
    if action.provider() {
        start_provider_items(transaction, batch_id, &pending, action, provider)?;
        finish_batch(transaction, batch_id)?;
        return Ok(ActOutcome::Done(start_response(
            transaction,
            batch_id,
            None,
        )?));
    }
    if action.external() {
        // 入队而不是就地执行：这一批的效果在事务外，`running` 必须先提交出去，
        // 进程被杀时 `reconcile_interrupted` 才看得到「开始过但没跑完」。
        mark_items_queued(transaction, batch_id)?;
        return Ok(ActOutcome::Deferred(DeferredRun {
            batch_id: batch_id.to_string(),
            action,
            mints_undo: true,
            shape: ResponseShape::Start,
        }));
    }
    let patch = stored_patch(transaction, batch_id)?;
    let outcome = run_items(transaction, &pending, &patch, action)?;
    if outcome.bumped_domains {
        bump_library_revisions(transaction, &outcome.bumped_list)
            .map_err(LibraryActError::unavailable)?;
    }
    finish_batch(transaction, batch_id)?;
    let undo_token = if outcome.compensable_ran {
        Some(mint_undo_token(transaction, batch_id)?)
    } else {
        None
    };
    // 投影要在提交前读，读的是同一事务里的最终态。
    Ok(ActOutcome::Done(start_response(
        transaction,
        batch_id,
        undo_token,
    )?))
}

/// 把还没开始的项推成 `queued`：事务外的执行者按 ordinal 逐项认领。
fn mark_items_queued(connection: &Connection, batch_id: &str) -> LibraryActResult<()> {
    let timestamp = now();
    connection
        .execute(
            "UPDATE library_batch_items SET state = ?2, updated_at = ?3
             WHERE batch_id = ?1 AND state = ?4",
            params![
                batch_id,
                ItemState::Queued.as_str(),
                timestamp,
                ItemState::Planned.as_str(),
            ],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    Ok(())
}

fn start_provider_items(
    transaction: &Transaction<'_>,
    batch_id: &str,
    items: &[ItemRow],
    action: ItemAction,
    provider: &ProviderActContext,
) -> LibraryActResult<()> {
    let expected_route = provider_route_for(action, provider)?;
    let timestamp = now();
    for item in items {
        let state = ItemState::parse(&item.state).ok_or_else(|| {
            LibraryActError::unavailable(format!("unknown stored item state {}", item.state))
        })?;
        if state.is_terminal() {
            continue;
        }
        let Some(handle_id) = item.prepared_job_handle.as_deref() else {
            record_item_failure(
                transaction,
                &item.id,
                &ItemFailure::new(
                    LibraryActErrorCode::ProviderRouteUnavailable,
                    "this item has no prepared job handle",
                ),
            )?;
            continue;
        };
        if let Some(skip) = provider_skip_at_start(transaction, item, action)? {
            let _ = release_prepared_on(transaction, handle_id);
            transaction
                .execute(
                    "UPDATE library_batch_items
                     SET state = ?2, result_json = ?3, finished_at = ?4, updated_at = ?4
                     WHERE id = ?1",
                    params![
                        item.id,
                        ItemState::Skipped.as_str(),
                        store_plan(&json!({ "outcome": skip })),
                        timestamp
                    ],
                )
                .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
            continue;
        }
        let consumed = consume_prepared_on(
            transaction,
            &PreparedJobHandle {
                id: handle_id.to_string(),
            },
            &expected_route,
        )
        .map_err(|error| {
            if error.contains("exact-bind") || error.contains("expired") {
                LibraryActError::of(LibraryActErrorCode::StaleBatchPlan, error)
            } else {
                LibraryActError::of(LibraryActErrorCode::ProviderRouteUnavailable, error)
            }
        })?;
        insert_job_link(transaction, &item.id, &consumed.job_id, consumed.coalesced)?;
        transaction
            .execute(
                "UPDATE library_batch_items
                 SET state = ?2, started_at = COALESCE(started_at, ?3), updated_at = ?3,
                     result_json = ?4
                 WHERE id = ?1",
                params![
                    item.id,
                    ItemState::Queued.as_str(),
                    timestamp,
                    store_plan(&json!({
                        "outcome": if consumed.coalesced { "joined_existing" } else { "created_job" },
                        "jobId": consumed.job_id,
                    })),
                ],
            )
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    }
    let _ = batch_id;
    Ok(())
}

fn provider_route_for(
    action: ItemAction,
    provider: &ProviderActContext,
) -> LibraryActResult<JobExecutionRoute> {
    match action {
        ItemAction::ApplyOcr => provider
            .ocr
            .clone()
            .map(JobExecutionRoute::MistralOcr)
            .ok_or_else(|| {
                LibraryActError::of(
                    LibraryActErrorCode::ProviderRouteUnavailable,
                    "configure Mistral OCR before starting this batch",
                )
            }),
        ItemAction::ApplyBrief => provider
            .paper
            .clone()
            .map(JobExecutionRoute::Paper)
            .ok_or_else(|| {
                LibraryActError::of(
                    LibraryActErrorCode::ProviderRouteUnavailable,
                    "configure a paper provider before starting this batch",
                )
            }),
        _ => Err(LibraryActError::unavailable(
            "this action is not a provider batch",
        )),
    }
}

fn provider_skip_at_start(
    connection: &Connection,
    item: &ItemRow,
    action: ItemAction,
) -> LibraryActResult<Option<&'static str>> {
    let Some(revision_id) = item.revision_id.as_deref() else {
        return Ok(None);
    };
    let Some(paper_id) = item.paper_id.as_deref() else {
        return Ok(None);
    };
    let current: Option<String> = connection
        .query_row(
            "SELECT h.revision_id FROM papers p
             JOIN paper_heads h ON h.paper_id = p.id
             WHERE p.id = ?1 AND p.deleted_at IS NULL",
            params![paper_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    if current.as_deref() != Some(revision_id) {
        return Err(LibraryActError::of(
            LibraryActErrorCode::StaleSelection,
            "this Paper's revision changed after the batch was planned",
        ));
    }
    let has_ocr: i64 = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM ocr_revisions WHERE revision_id = ?1)",
            params![revision_id],
            |row| row.get(0),
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let has_brief: i64 = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM artifact_heads WHERE paper_id = ?1 AND kind = 'brief')",
            params![paper_id],
            |row| row.get(0),
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    Ok(match action {
        ItemAction::ApplyOcr if has_ocr != 0 => Some("skipped_existing_ocr"),
        ItemAction::ApplyBrief if has_ocr == 0 => Some("skipped_missing_ocr"),
        ItemAction::ApplyBrief if has_brief != 0 => Some("skipped_existing_brief"),
        _ => None,
    })
}

fn insert_job_link(
    connection: &Connection,
    item_id: &str,
    job_id: &str,
    coalesced: bool,
) -> LibraryActResult<()> {
    let ownership = if coalesced { "joined" } else { "created" };
    let attribution = if coalesced {
        "shared_no_incremental"
    } else {
        "creator"
    };
    let snapshot = json!({
        "kind": "job",
        "jobId": job_id,
        "ownership": ownership,
    });
    connection
        .execute(
            "INSERT INTO library_batch_job_links(
               id, batch_item_id, job_id, ownership, consumer_state,
               cost_attribution, usage_receipt_id, job_snapshot_json, created_at, detached_at
             ) VALUES (?1, ?2, ?3, ?4, 'active', ?5, NULL, ?6, ?7, NULL)",
            params![
                uuid::Uuid::new_v4().to_string(),
                item_id,
                job_id,
                ownership,
                attribution,
                snapshot.to_string(),
                now(),
            ],
        )
        .map_err(|error| {
            if error.to_string().contains("UNIQUE") {
                LibraryActError::of(
                    LibraryActErrorCode::PossibleDuplicateCharge,
                    "this job already has a created owner; the batch did not take a second one",
                )
            } else {
                LibraryActError::unavailable(error.to_string())
            }
        })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// 事务外执行（Move / Trash，计划 §7 末尾）
// ---------------------------------------------------------------------------

/// 一项被认领后，执行侧需要的全部事实。
struct ClaimedItem {
    id: String,
    /// 导入项在执行成功前没有 Paper：它这一项的身份是源文件，Paper 是结果。
    paper_id: Option<String>,
    precondition_digest: String,
    plan_json: String,
    compensation_json: Option<String>,
    /// 认领那一刻库记着的真实位置：执行前还要在磁盘上确认一次。
    source_relative_path: String,
}

/// 认领前复验的当前事实：冻结 precondition 的现算值 + 文件应当所在的位置。
struct FreshState {
    precondition: String,
    relative_path: String,
}

/// 事务外一项的成功结果：写回 Item 的结果摘要 + 撤销要复验的 after-image。
struct ItemEffect {
    result: Value,
    /// after-image：撤销与续跑复验用的真实事实。`None` 表示这一项没改变任何东西，
    /// 库里计划阶段冻结的那一份继续算数。
    precondition: Option<Value>,
    /// 执行才产出新身份时回填 `(paperId, revisionId)`：导入新建的 Paper 全靠它，
    /// 否则撤销与「点开这一项」都没有目标可指。
    paper: Option<(String, String)>,
    /// 执行后才存在的补偿路径。`None` 的含义是「保持计划阶段冻结的那一份」——
    /// Move / Trash 的补偿在计划里就写好了，只有导入要等结果出来才知道
    /// 这一项到底可不可补偿（`created_new` 才配）。
    compensation: Option<Value>,
    /// 终态：`succeeded`，或「执行时才发现这一项无事可做」的 `skipped`。
    /// 两者都要写结果摘要，但只有前者留下 after-image。
    state: ItemState,
}

impl ItemEffect {
    /// Move / Trash 用：结果 + after-image，身份与补偿都沿用计划阶段冻结的值。
    fn settled(result: Value, precondition: Value) -> Self {
        Self {
            result,
            precondition: Some(precondition),
            paper: None,
            compensation: None,
            state: ItemState::Succeeded,
        }
    }

    /// §7：导入撞上了库里已有的记录（`duplicate_hash` / `target_path_conflict`）——
    /// 结论要逐项写清，但那不是一次失败，也不是库里多出来的效果。
    fn skipped(result: Value) -> Self {
        Self {
            result,
            precondition: None,
            paper: None,
            compensation: None,
            state: ItemState::Skipped,
        }
    }
}

/// 队列的续跑入口：按 ordinal 逐项「认领 → 事务外执行 → 记结果」。
///
/// 每一步都是独立提交的一次事务，所以进程在任何一点被杀，库里留下的都已经是一致
/// 事实：`running` 由 [`reconcile_interrupted`] 判成 `interrupted_unknown`，
/// 剩下的 `queued` 由下一次 start 续跑（见 [`resume_deferred_run`]）。
fn run_deferred(root: &Path, run: &DeferredRun) -> LibraryActResult<LibraryActResponse> {
    let module = crate::paper_module::PaperModule::open(root)
        .map_err(|_| LibraryActError::unavailable("the Workspace could not be opened"))?;
    loop {
        let claimed = {
            let mut connection = open_db(root).map_err(LibraryActError::unavailable)?;
            let transaction = immediate_transaction(&mut connection)?;
            let claimed = claim_next_item(&transaction, &run.batch_id, run.action)?;
            transaction.commit().map_err(commit_failed)?;
            claimed
        };
        let Some(item) = claimed else { break };
        let effect = execute_external(&module, root, run.action, &item);
        let mut connection = open_db(root).map_err(LibraryActError::unavailable)?;
        let transaction = immediate_transaction(&mut connection)?;
        match effect {
            // after-image 取执行后的真实事实，不取计划里的预期值：两者不一致时
            // 撤销必须以真实事实为准。
            Ok(effect) => finish_item(&transaction, &item.id, &effect).map_err(|failure| {
                LibraryActError::unavailable(format!(
                    "the item result could not be recorded: {}",
                    failure.safe_summary
                ))
            })?,
            Err(failure) => record_item_failure(&transaction, &item.id, &failure)?,
        }
        transaction.commit().map_err(commit_failed)?;
    }
    finish_deferred(root, run)
}

/// 上一次进程在事务外执行中途退出：重复 start 绝不重跑任何终结项，但队列里
/// 还没开始的项要跑完——那是同一批次的续跑，不是第二次执行。
fn resume_deferred_run(
    connection: &Connection,
    batch_id: &str,
) -> LibraryActResult<Option<DeferredRun>> {
    let action = resolve_action(action_kind_for(connection, batch_id)?, false)?;
    if !action.external() {
        return Ok(None);
    }
    let pending: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM library_batch_items
             WHERE batch_id = ?1 AND state IN (?2,?3)",
            params![
                batch_id,
                ItemState::Planned.as_str(),
                ItemState::Queued.as_str()
            ],
            |row| row.get(0),
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    if pending == 0 {
        return Ok(None);
    }
    mark_items_queued(connection, batch_id)?;
    Ok(Some(DeferredRun {
        batch_id: batch_id.to_string(),
        action,
        mints_undo: true,
        shape: ResponseShape::Start,
    }))
}

/// 认领下一项：把它的 precondition 复验通过后立刻落成 `running`。
///
/// 复验失败的项就地记成失败并继续找下一项——一次崩溃或一次并发改写都不该把
/// 整批卡住，而 `running` 这个中间态正是崩溃恢复的边界。
fn claim_next_item(
    connection: &Connection,
    batch_id: &str,
    action: ItemAction,
) -> LibraryActResult<Option<ClaimedItem>> {
    let batch = load_batch(connection, batch_id)?;
    let timestamp = now();
    if batch.cancel_requested_at.is_some() {
        // §7.1：取消只处理尚未开始的项；已经在文件系统上落下效果的不动。
        connection
            .execute(
                "UPDATE library_batch_items
                 SET state = ?3, finished_at = ?2, updated_at = ?2
                 WHERE batch_id = ?1 AND state IN ('planned','queued')",
                params![batch_id, timestamp, ItemState::Cancelled.as_str()],
            )
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        return Ok(None);
    }
    loop {
        let row: Option<ItemRow> = connection
            .query_row(
                &format!(
                    "SELECT {ITEM_COLUMNS} FROM library_batch_items
                     WHERE batch_id = ?1 AND state = ?2 ORDER BY ordinal ASC LIMIT 1"
                ),
                params![batch_id, ItemState::Queued.as_str()],
                item_row_from,
            )
            .optional()
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        let Some(row) = row else { return Ok(None) };
        // 复验要读的是这一行自己的事实：导入项在成功之前没有 Paper，它的身份
        // 是冻结在 `plan_json` 里的源文件，拿 paper_id 当唯一入参会把它读成空串。
        let fresh = fresh_state(connection, action, &row);
        let item = ClaimedItem {
            id: row.id.clone(),
            paper_id: row.paper_id.clone(),
            precondition_digest: row.precondition_digest.clone(),
            plan_json: row.plan_json.clone(),
            compensation_json: row.compensation_json.clone(),
            source_relative_path: String::new(),
        };
        let fresh = match fresh {
            Ok(fresh) => fresh,
            Err(failure) => {
                record_item_failure(connection, &item.id, &failure)?;
                continue;
            }
        };
        if fresh.precondition != item.precondition_digest {
            record_item_failure(connection, &item.id, &stale_failure(action))?;
            continue;
        }
        connection
            .execute(
                "UPDATE library_batch_items
                 SET state = ?2, started_at = COALESCE(started_at, ?3), updated_at = ?3
                 WHERE id = ?1",
                params![item.id, ItemState::Running.as_str(), timestamp],
            )
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        return Ok(Some(ClaimedItem {
            source_relative_path: fresh.relative_path,
            ..item
        }));
    }
}

/// 队列跑完（或被取消）后收口：重算批次终态，必要时铸撤销 Token。
fn finish_deferred(root: &Path, run: &DeferredRun) -> LibraryActResult<LibraryActResponse> {
    let mut connection = open_db(root).map_err(LibraryActError::unavailable)?;
    let transaction = immediate_transaction(&mut connection)?;
    // 终态与 Token 同一个事务：宁可这一批没有撤销入口，也不要「已经撤销过一半」
    // 的中间态——事务外的执行者本来就拿不到一次性的全局回滚。
    finish_batch(&transaction, &run.batch_id)?;
    let undo_token = if run.mints_undo && has_compensable_effect(&transaction, &run.batch_id)? {
        Some(mint_undo_token(&transaction, &run.batch_id)?)
    } else {
        None
    };
    let response = run
        .shape
        .build(batch_projection(&transaction, &run.batch_id)?, undo_token);
    transaction.commit().map_err(commit_failed)?;
    Ok(response)
}

/// 这一批是否真的写下过可补偿的效果：全项 no-op 的批次不配撤销 Token。
fn has_compensable_effect(connection: &Connection, batch_id: &str) -> LibraryActResult<bool> {
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM library_batch_items
             WHERE batch_id = ?1 AND state = 'succeeded' AND compensation_json IS NOT NULL",
            params![batch_id],
            |row| row.get(0),
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    Ok(count > 0)
}

/// 复验用的当前事实。撤销侧唯一读回收站的位置，因为那时库里的事实是「已删除」。
fn fresh_state(
    connection: &Connection,
    action: ItemAction,
    row: &ItemRow,
) -> Result<FreshState, ItemFailure> {
    if !action.external() {
        return Err(ItemFailure::storage());
    }
    // 正向导入复验的是磁盘上的源文件：这一项在成功之前库里什么都没有，
    // 拿 Paper 路径去复验只会把每一次认领都读成「Paper 不在库里」。
    // 补偿（`RevertImport`）反过来——它动的是本批新建的那个 Paper，
    // 源文件从头到尾没被碰过，所以它按 Paper 形态复验库里的当前位置。
    if action == ItemAction::ApplyImport {
        return import_fresh_state(row);
    }
    let paper_id = row.paper_id.as_deref().ok_or_else(|| {
        ItemFailure::new(
            LibraryActErrorCode::UnsupportedForScope,
            "this item has no Paper to act on",
        )
    })?;
    if action.target_must_be_trashed() {
        let (original, trash_relative) = trashed_position(connection, paper_id)?;
        return Ok(FreshState {
            precondition: canonical_digest(&trash_precondition_value(paper_id, &original)),
            relative_path: trash_relative,
        });
    }
    let location = paper_location(connection, paper_id)
        .map_err(|_| ItemFailure::storage())?
        .filter(|location| !location.deleted)
        .ok_or_else(|| {
            ItemFailure::new(
                LibraryActErrorCode::PaperNotFound,
                "the Paper is not in the library",
            )
        })?;
    Ok(FreshState {
        precondition: canonical_digest(&path_precondition_value(paper_id, &location.relative_path)),
        relative_path: location.relative_path,
    })
}

/// 导入项的认领复验。三种结论要分得开，因为用户看到的下一步不一样：
/// 文件没了（换个文件重新拖）、文件又被写过（等它写完再试）、计划与行不是
/// 同一份身份（库里的记录自相矛盾，只能报存储故障）。
fn import_fresh_state(row: &ItemRow) -> Result<FreshState, ItemFailure> {
    let plan = row_import_plan(row)?;
    let metadata = fs::metadata(&plan.source_path).map_err(|_| missing_import_source())?;
    if !metadata.is_file() {
        return Err(missing_import_source());
    }
    if metadata.len() != plan.byte_size {
        return Err(source_changed());
    }
    Ok(FreshState {
        precondition: source_precondition(&plan.dedupe_key, plan.byte_size),
        relative_path: String::new(),
    })
}

/// 读出并复验这一行的导入计划：`dedupe_key` 必须同时是行上的身份和计划里的身份，
/// 否则冻结的 precondition 描述的就不是这个源文件。
fn row_import_plan(row: &ItemRow) -> Result<ImportItemPlan, ItemFailure> {
    let value: Value = serde_json::from_str(&row.plan_json).map_err(|_| ItemFailure::storage())?;
    if value.get("action").and_then(Value::as_str) != Some(IMPORT_PLAN_ACTION) {
        return Err(ItemFailure::new(
            LibraryActErrorCode::UnsupportedForScope,
            "this item is not an import",
        ));
    }
    let plan: ImportItemPlan = serde_json::from_value(value).map_err(|_| ItemFailure::storage())?;
    if row.dedupe_key.as_deref() != Some(plan.dedupe_key.as_str()) {
        return Err(ItemFailure::new(
            LibraryActErrorCode::UnsupportedForScope,
            "this item's source plan belongs to another source",
        ));
    }
    Ok(plan)
}

fn missing_import_source() -> ItemFailure {
    ItemFailure::new(
        LibraryActErrorCode::SourceMissing,
        "the source file is no longer readable where it was picked",
    )
}

fn source_changed() -> ItemFailure {
    ItemFailure::new(
        LibraryActErrorCode::SourceChanged,
        "this source file changed after the plan was made; run it again",
    )
}

/// precondition 不再成立时的逐项失败：正向动作是「计划过期」，补偿是「撤销冲突」。
fn stale_failure(action: ItemAction) -> ItemFailure {
    if action.compensating() {
        ItemFailure::new(
            LibraryActErrorCode::UndoConflict,
            "this Paper changed after the batch; the undo stopped for it",
        )
    } else {
        ItemFailure::new(
            LibraryActErrorCode::StaleSelection,
            "this target changed after the plan was made",
        )
    }
}

/// 当前在回收站里的位置：`(原始路径, 回收站内路径)`。
fn trashed_position(
    connection: &Connection,
    paper_id: &str,
) -> Result<(String, String), ItemFailure> {
    let row: Option<(Option<String>, String, String)> = connection
        .query_row(
            "SELECT p.deleted_at, t.original_relative_path, t.trash_relative_path
             FROM trash_entries t JOIN papers p ON p.id = t.paper_id
             WHERE t.paper_id = ?1 AND t.restored_at IS NULL
             ORDER BY t.deleted_at DESC LIMIT 1",
            params![paper_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|_| ItemFailure::storage())?;
    let Some((deleted_at, original, trash_relative)) = row else {
        return Err(ItemFailure::new(
            LibraryActErrorCode::UndoConflict,
            "this Paper is no longer in Trash",
        ));
    };
    if deleted_at.is_none() {
        return Err(ItemFailure::new(
            LibraryActErrorCode::UndoConflict,
            "this Paper was restored outside the batch",
        ));
    }
    Ok((
        crate::library_paths::normalize_slashes(&original),
        crate::library_paths::normalize_slashes(&trash_relative),
    ))
}

/// 执行前在磁盘上确认来源还在。绝对路径只在这里出现一次，且永远不进摘要或事件。
fn source_exists(root: &Path, action: ItemAction, relative_path: &str) -> bool {
    if action.target_must_be_trashed() {
        return root.join(".read-desktop").join(relative_path).is_file();
    }
    file_on_disk(root, relative_path)
}

fn missing_source() -> ItemFailure {
    ItemFailure::new(
        LibraryActErrorCode::SourceMissing,
        "the PDF is not at the path the library has for it",
    )
}

/// 一项事务外动作的执行分派。失败一律是逐项失败，绝不把整批回滚。
fn execute_external(
    module: &crate::paper_module::PaperModule,
    root: &Path,
    action: ItemAction,
    item: &ClaimedItem,
) -> Result<ItemEffect, ItemFailure> {
    match action {
        ItemAction::ApplyMove => apply_move_item(module, root, action, item),
        ItemAction::RevertMove => revert_move_item(module, root, action, item),
        ItemAction::ApplyTrash => apply_trash_item(module, root, action, item),
        ItemAction::RevertTrash => revert_trash_item(module, root, action, item),
        // 正向导入的「来源还在不在」在认领那一刻已经按字节数复验过，
        // 而且它读的永远是用户选的源文件路径，不是库记着的位置。
        ItemAction::ApplyImport => apply_import_item(module, item),
        ItemAction::RevertImport => revert_import_item(module, root, action, item),
        // 导出读的是库里的投影，不碰用户的 PDF；撤销只删 `export/` 下那一份输出。
        ItemAction::ApplyExport => apply_export_item(module, root, item),
        ItemAction::RevertExport => revert_export_item(root, item),
        ItemAction::ApplyTags
        | ItemAction::RevertTags
        | ItemAction::ApplyLifecycle
        | ItemAction::RevertLifecycle
        | ItemAction::ApplyOcr
        | ItemAction::ApplyBrief => Err(ItemFailure::new(
            LibraryActErrorCode::UnsupportedForScope,
            "a tag, lifecycle or provider item never runs outside the batch transaction",
        )),
    }
}

fn apply_move_item(
    module: &crate::paper_module::PaperModule,
    root: &Path,
    action: ItemAction,
    item: &ClaimedItem,
) -> Result<ItemEffect, ItemFailure> {
    let plan: MoveItemPlan = item_plan(&item.plan_json, MOVE_PLAN_ACTION, item)?;
    let paper_id = paper_of(item)?;
    require_source(root, action, &item.source_relative_path)?;
    // 批量 Move 只换目录：文件名沿用计划里冻结的那一个，重命名继续走单篇入口。
    // 跨根时 kind-sensitive 成果的摘除由 PaperModule 在同一动作里完成，
    // 计划阶段已经把它列成确认项。
    let moved = module
        .move_paper(paper_id, &plan.collection_path, &plan.file_name, true)
        .map_err(|error| paper_failure(&error))?;
    let after = crate::library_paths::normalize_slashes(&moved.relative_path);
    Ok(ItemEffect::settled(
        json!({
            "beforeRelativePath": plan.before_relative_path,
            "afterRelativePath": after,
            "kindChanged": plan.kind_changed,
            "droppedCategories": plan.dropped_categories,
        }),
        path_precondition_value(paper_id, &after),
    ))
}

fn revert_move_item(
    module: &crate::paper_module::PaperModule,
    root: &Path,
    action: ItemAction,
    item: &ClaimedItem,
) -> Result<ItemEffect, ItemFailure> {
    let compensation: MoveCompensation = item_compensation(&item.compensation_json, item)?;
    let paper_id = paper_of(item)?;
    require_source(root, action, &item.source_relative_path)?;
    let moved = module
        .move_paper(
            paper_id,
            &compensation.collection_path,
            &compensation.file_name,
            true,
        )
        .map_err(|error| paper_failure(&error))?;
    let after = crate::library_paths::normalize_slashes(&moved.relative_path);
    Ok(ItemEffect::settled(
        // §7.3：反向 Move 只把文件放回原位；跨根时被摘掉的成果不回来。
        json!({
            "compensated": true,
            "restoredTo": after,
            "artifactsRestored": !compensation.kind_changed,
        }),
        path_precondition_value(paper_id, &after),
    ))
}

fn apply_trash_item(
    module: &crate::paper_module::PaperModule,
    root: &Path,
    action: ItemAction,
    item: &ClaimedItem,
) -> Result<ItemEffect, ItemFailure> {
    let plan: TrashItemPlan = item_plan(&item.plan_json, TRASH_PLAN_ACTION, item)?;
    let paper_id = paper_of(item)?;
    require_source(root, action, &item.source_relative_path)?;
    module
        .trash_paper(paper_id)
        .map_err(|error| paper_failure(&error))?;
    Ok(ItemEffect::settled(
        json!({
            "trashed": true,
            "beforeRelativePath": plan.before_relative_path,
        }),
        trash_precondition_value(paper_id, &item.source_relative_path),
    ))
}

fn revert_trash_item(
    module: &crate::paper_module::PaperModule,
    root: &Path,
    action: ItemAction,
    item: &ClaimedItem,
) -> Result<ItemEffect, ItemFailure> {
    let compensation: TrashCompensation = item_compensation(&item.compensation_json, item)?;
    let paper_id = paper_of(item)?;
    require_source(root, action, &item.source_relative_path)?;
    let restored = module
        .restore_paper(paper_id)
        .map_err(|error| paper_failure(&error))?;
    let after = crate::library_paths::normalize_slashes(&restored.relative_path);
    Ok(ItemEffect::settled(
        json!({
            "compensated": true,
            "restoredTo": after,
            // 计划里冻结的原路径与真实放回位置并列：两者不一致就是有人在我们之外动过。
            "plannedOriginalPath": compensation.original_relative_path,
            // 回收站撤销只放回文件：被取消的 Job 与手排位置都不回来。
            "jobsRestored": false,
        }),
        path_precondition_value(paper_id, &after),
    ))
}

/// §7 Import 行：一份源文件进库的四种结论都来自 `PaperModule` 的同一个入口，
/// 所以这里只翻译它的结论，不复制它的规则。hash、回收站归属、冲突记录都归它管。
fn apply_import_item(
    module: &crate::paper_module::PaperModule,
    item: &ClaimedItem,
) -> Result<ItemEffect, ItemFailure> {
    let plan: ImportItemPlan =
        serde_json::from_str(&item.plan_json).map_err(|_| ItemFailure::storage())?;
    if plan.action != IMPORT_PLAN_ACTION {
        return Err(ItemFailure::new(
            LibraryActErrorCode::UnsupportedForScope,
            "this item is not an import",
        ));
    }
    let projection = module
        .import_pdf(Path::new(&plan.source_path), Some(&plan.collection_path))
        .map_err(|error| paper_failure(&error))?;
    let Some(paper) = projection.paper else {
        // 撞上了库里已有的记录：`PaperModule` 已经把冲突写成行，这一项的结论要能
        // 读出「为什么没有多出一个 Paper」——但那不是一次失败，也不新增任何效果。
        let conflict = projection.conflict.ok_or_else(ItemFailure::storage)?;
        return Ok(ItemEffect::skipped(json!({
            "outcome": IMPORT_OUTCOME_CONFLICT,
            "conflictKind": conflict.kind,
            "relativePath": conflict.relative_path,
        })));
    };
    let relative_path = crate::library_paths::normalize_slashes(&paper.relative_path);
    let outcome = match projection.outcome {
        crate::paper_module::ImportOutcome::CreatedNew => IMPORT_OUTCOME_CREATED,
        crate::paper_module::ImportOutcome::ReusedExisting => IMPORT_OUTCOME_REUSED,
        crate::paper_module::ImportOutcome::RestoredExisting => IMPORT_OUTCOME_RESTORED,
        // 有 Paper 就没有冲突；这个组合只可能是 `PaperModule` 与我们读的不一致。
        crate::paper_module::ImportOutcome::Conflict => return Err(ItemFailure::storage()),
    };
    let effect = ItemEffect::settled(
        json!({
            "outcome": outcome,
            "paperId": paper.id,
            "revisionId": paper.revision_id,
            "relativePath": relative_path,
            "fileName": plan.file_name,
        }),
        path_precondition_value(&paper.id, &relative_path),
    );
    // 只有本批新建的那一种配补偿：复用与 restore 都在动导入前就存在的 Paper，
    // 撤销它们等于「撤销一个本来就在库里的东西」，宁可没有撤销入口也不猜。
    if outcome != IMPORT_OUTCOME_CREATED {
        return Ok(effect);
    }
    Ok(ItemEffect {
        compensation: Some(
            serde_json::to_value(ImportCompensation {
                mode: IMPORT_COMPENSATION_MODE.to_string(),
                paper_id: paper.id.clone(),
                original_relative_path: relative_path,
            })
            .map_err(|_| ItemFailure::storage())?,
        ),
        paper: Some((paper.id, paper.revision_id)),
        ..effect
    })
}

/// 导入的补偿：把本批新建的那个 Paper 移进回收站。用户的源文件永远不动——
/// 复制进库的那一份归库管，原件不属于这个批次。
fn revert_import_item(
    module: &crate::paper_module::PaperModule,
    root: &Path,
    action: ItemAction,
    item: &ClaimedItem,
) -> Result<ItemEffect, ItemFailure> {
    let compensation: ImportCompensation = item_compensation(&item.compensation_json, item)?;
    let paper_id = paper_of(item)?;
    if compensation.mode != IMPORT_COMPENSATION_MODE {
        return Err(ItemFailure::new(
            LibraryActErrorCode::NotReversible,
            "this item's compensation is not an import compensation",
        ));
    }
    require_source(root, action, &item.source_relative_path)?;
    module
        .trash_paper(paper_id)
        .map_err(|error| paper_failure(&error))?;
    Ok(ItemEffect::settled(
        json!({
            "compensated": true,
            "trashed": true,
            // 计划里放下的位置与撤销时看到的位置并列：两者不一致就是有人在我们之外动过。
            "plannedOriginalPath": compensation.original_relative_path,
        }),
        trash_precondition_value(paper_id, &item.source_relative_path),
    ))
}

/// §7 Export 行：渲染与落盘规则全在 `export_module`，单篇导出与批量导出共用同一个
/// 入口——用户拿出去的文件不该因为它是批量跑的就长得不一样。
fn apply_export_item(
    module: &crate::paper_module::PaperModule,
    root: &Path,
    item: &ClaimedItem,
) -> Result<ItemEffect, ItemFailure> {
    let plan: ExportItemPlan = item_plan(&item.plan_json, EXPORT_PLAN_ACTION, item)?;
    let paper_id = paper_of(item)?;
    // 格式是封闭枚举，加第二种时这一支必须显式判过，才不会被通配分支悄悄放过去。
    let bundle = match plan.format {
        ExportFormat::ReadingBundle => {
            crate::export_module::export_bundle(module, root, paper_id, plan.locale)
                .map_err(export_failure)?
        }
    };
    let effect = ItemEffect::settled(
        json!({
            "outcome": if bundle.replaced_existing {
                EXPORT_OUTCOME_OVERWRITTEN
            } else {
                EXPORT_OUTCOME_WRITTEN
            },
            "paperId": bundle.paper_id,
            "revisionId": bundle.revision_id,
            "relativePath": bundle.relative_path,
            "byteSize": bundle.byte_size,
            // 真实输出与预览承诺的路径不一致时要留得下来：磁盘上多了一个同名文件
            // 就会走 fallback 名，那不叫「按预览写好」，也不该被悄悄当成同一件事。
            "matchesPlan": bundle.relative_path == plan.planned_relative_path,
        }),
        // 导出一个字都不改库里的 Paper 事实：after-image 就是认领时看到的那个位置。
        path_precondition_value(paper_id, &item.source_relative_path),
    );
    // 覆盖掉的那一份不是本批写的，删它才是损失：那一类项没有补偿路径。
    if bundle.replaced_existing {
        return Ok(effect);
    }
    Ok(ItemEffect {
        compensation: Some(
            serde_json::to_value(ExportCompensation {
                mode: EXPORT_COMPENSATION_MODE.to_string(),
                paper_id: paper_id.to_string(),
                relative_path: bundle.relative_path,
                content_digest: bundle.content_digest,
            })
            .map_err(|_| ItemFailure::storage())?,
        ),
        ..effect
    })
}

/// 导出项的补偿：只删本批新建且字节没变的那一份输出（§7 给 Export 的撤销权限
/// 就这么大）。路径上的文件不是我们写下的那一份时宁可什么都不动。
fn revert_export_item(root: &Path, item: &ClaimedItem) -> Result<ItemEffect, ItemFailure> {
    let compensation: ExportCompensation = item_compensation(&item.compensation_json, item)?;
    let paper_id = paper_of(item)?;
    if compensation.mode != EXPORT_COMPENSATION_MODE {
        return Err(ItemFailure::new(
            LibraryActErrorCode::NotReversible,
            "this item's compensation is not an export cleanup",
        ));
    }
    let after = path_precondition_value(paper_id, &item.source_relative_path);
    match crate::export_module::undo_output(
        root,
        &compensation.relative_path,
        &compensation.content_digest,
    ) {
        Ok(crate::export_module::UndoOutput::Deleted) => Ok(ItemEffect::settled(
            json!({
                "compensated": true,
                "deleted": true,
                "relativePath": compensation.relative_path,
            }),
            after,
        )),
        Ok(crate::export_module::UndoOutput::Missing) => {
            // 用户已经自己删掉了：要达成的结果本来就成立，只是不是我们做的。
            Ok(ItemEffect::skipped(json!({
                "outcome": "output_already_removed",
                "relativePath": compensation.relative_path,
            })))
        }
        Ok(crate::export_module::UndoOutput::Changed) => Err(ItemFailure::new(
            LibraryActErrorCode::UndoConflict,
            "this exported file is no longer the one the batch wrote; the undo left it alone",
        )),
        // 删除被拒（占用、权限）：撤销没完成，报冲突而不是假装干净。
        Err(()) => Err(ItemFailure::new(
            LibraryActErrorCode::UndoConflict,
            "the exported file could not be removed; the undo left it alone",
        )),
    }
}

/// 导出失败到逐项失败的翻译。这里只挑错误码，用户看到的结论文案归
/// [`crate::export_module::ExportError::safe_summary`] 一份实现；OS 原文留在
/// `ExportError` 里给日志看，永远不进 `error_summary`（§10.2）。
fn export_failure(error: crate::export_module::ExportError) -> ItemFailure {
    let summary = error.safe_summary();
    match error {
        crate::export_module::ExportError::PaperMissing => {
            ItemFailure::new(LibraryActErrorCode::PaperNotFound, summary)
        }
        // 写不进去（磁盘满、目录被占）与库读不出来都属于「再跑一次就可能成功」，
        // 但都不是调用方的请求问题，所以不能报成计划过期。
        crate::export_module::ExportError::WriteFailed(_)
        | crate::export_module::ExportError::Storage(_) => {
            ItemFailure::new(LibraryActErrorCode::WorkspaceBusy, summary)
        }
    }
}

/// 需要 Paper 身份的项：`fresh_state` 已经复验过它存在，所以这里读到 `None`
/// 只会是「计划形态与行的形态不一致」，属于存储故障而不是业务失败。
fn paper_of(item: &ClaimedItem) -> Result<&str, ItemFailure> {
    match item
        .paper_id
        .as_deref()
        .filter(|paper_id| !paper_id.is_empty())
    {
        Some(paper_id) => Ok(paper_id),
        None => Err(ItemFailure::storage()),
    }
}

fn require_source(root: &Path, action: ItemAction, relative_path: &str) -> Result<(), ItemFailure> {
    if source_exists(root, action, relative_path) {
        return Ok(());
    }
    Err(missing_source())
}

/// 逐项计划的判别与反序列化。计划与补偿都必须属于这一项自己的 Paper：
/// 认错了目标就是把 A 的文件搬到 B 的位置上，宁可失败也不能猜。
fn item_plan<T: serde::de::DeserializeOwned>(
    raw: &str,
    discriminator: &str,
    item: &ClaimedItem,
) -> Result<T, ItemFailure> {
    let value: Value = serde_json::from_str(raw).map_err(|_| ItemFailure::storage())?;
    if value.get("action").and_then(Value::as_str) != Some(discriminator)
        || value.get("paperId").and_then(Value::as_str) != Some(paper_of(item)?)
    {
        return Err(ItemFailure::new(
            LibraryActErrorCode::UnsupportedForScope,
            "this item is not the action the batch claims to run",
        ));
    }
    serde_json::from_value(value).map_err(|_| ItemFailure::storage())
}

fn item_compensation<T: serde::de::DeserializeOwned>(
    raw: &Option<String>,
    item: &ClaimedItem,
) -> Result<T, ItemFailure> {
    let text = raw.as_deref().ok_or_else(|| {
        ItemFailure::new(
            LibraryActErrorCode::NotReversible,
            "this item recorded no compensation path",
        )
    })?;
    let value: Value = serde_json::from_str(text).map_err(|_| {
        ItemFailure::new(
            LibraryActErrorCode::NotReversible,
            "this item's compensation path is unreadable",
        )
    })?;
    if value.get("paperId").and_then(Value::as_str) != Some(paper_of(item)?) {
        return Err(ItemFailure::new(
            LibraryActErrorCode::UnsupportedForScope,
            "this item's compensation belongs to another Paper",
        ));
    }
    serde_json::from_value(value).map_err(|_| {
        ItemFailure::new(
            LibraryActErrorCode::NotReversible,
            "this item's compensation path is unreadable",
        )
    })
}

/// `PaperModule` 的错误是自由文本（OS 错误信息、绝对路径都可能在内）。
/// 逐项失败要落的是类型化错误码 + 固定摘要，所以这里只做分类，
/// 原始文本永远不进 `error_summary`（§10.2）。
fn paper_failure(text: &str) -> ItemFailure {
    if text.contains("already exists") {
        return ItemFailure::new(
            LibraryActErrorCode::TargetPathConflict,
            "another file already occupies the target path",
        );
    }
    if text.contains("kind_change_confirmation_required") {
        return ItemFailure::new(
            LibraryActErrorCode::ConfirmationRequired,
            "this move changes the document kind; preview it again",
        );
    }
    if text.contains("Paper was not found") {
        return ItemFailure::new(
            LibraryActErrorCode::PaperNotFound,
            "the Paper is no longer in the library",
        );
    }
    if text.contains("not in Trash") {
        return ItemFailure::new(
            LibraryActErrorCode::UndoConflict,
            "this Paper is no longer in Trash",
        );
    }
    // 源文件根本不是一个能读的 PDF（扩展名不对、`%PDF-` 头缺失、内容损坏）。
    // 这绝不能落到下面那个「可重试」的兜底里：重跑同一份字节不会变好。
    if text.contains("is not a PDF") || text.contains("Only native PDF") {
        return ItemFailure::new(
            LibraryActErrorCode::InvalidSource,
            "the source file is not a readable PDF",
        );
    }
    // 源文件路径已经解不开：拖进来之后被移走或删掉了。
    if text.contains("Unable to resolve PDF") {
        return ItemFailure::new(
            LibraryActErrorCode::SourceMissing,
            "the source file is no longer readable where it was picked",
        );
    }
    // 剩下的都是文件系统层的写失败（占用、权限、目录没了）：可重试，
    // 但不能被说成「计划过期」——那是调用方的请求问题。
    ItemFailure::new(
        LibraryActErrorCode::WorkspaceBusy,
        "the file system rejected this operation; try again",
    )
}

fn enforce_cost_ceilings(
    preview: &CostPreview,
    ceilings: &BTreeMap<String, String>,
) -> LibraryActResult<()> {
    if preview.is_empty() {
        return Ok(());
    }
    for estimate in &preview.marginal_estimates {
        let Some(ceiling) = ceilings.get(&estimate.currency) else {
            return Err(LibraryActError::of(
                LibraryActErrorCode::StaleBatchPlan,
                "the confirmed estimate ceiling is missing a currency from the plan; preview again",
            ));
        };
        if estimate.price_catalog_version != PRICE_CATALOG_VERSION
            || estimate_exceeds_ceiling(&estimate.maximum, ceiling)
        {
            return Err(LibraryActError::of(
                LibraryActErrorCode::StaleBatchPlan,
                "the cost estimate rose or the price catalog changed; preview again",
            ));
        }
    }
    Ok(())
}

fn validate_estimate_ceilings(ceilings: &BTreeMap<String, String>) -> LibraryActResult<()> {
    if ceilings.len() > 8 {
        return Err(LibraryActError::invalid(
            "a start request accepts at most 8 currency ceilings",
        ));
    }
    for (currency, ceiling) in ceilings {
        if currency.trim().is_empty() || currency.chars().count() > 8 {
            return Err(LibraryActError::invalid(
                "currency codes in the estimate ceiling must be non-empty and short",
            ));
        }
        if ceiling.trim().is_empty() {
            return Err(LibraryActError::invalid(
                "an estimate ceiling needs a decimal string value",
            ));
        }
    }
    Ok(())
}

/// 执行一批 Item：纯 SQLite 动作走「一个 `BEGIN IMMEDIATE` + 每 Item 一个 SAVEPOINT」。
///
/// 返回的三元组告诉调用方要不要推进 revision、要不要发 Undo Token。
struct RunOutcome {
    bumped_domains: bool,
    bumped_list: Vec<LibraryDomain>,
    compensable_ran: bool,
}
/// child Batch 的 `command` 必须存「真正要跑的那个命令」。直接套用父批次的
/// `{"kind":"retry","command":…}` 封套会让「重试的重试」解不出补丁，所以先把
/// 封套剥到最内层，剥不动就原样返回。
fn effective_command(command_json: &str) -> Value {
    let mut current = match serde_json::from_str::<Value>(command_json) {
        Ok(value) => value,
        Err(_) => return Value::Null,
    };
    for _ in 0..MAX_LINEAGE_HOPS {
        let kind = current
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if kind != BatchCommandKind::Retry.as_str()
            && kind != BatchCommandKind::Compensation.as_str()
        {
            return current;
        }
        match current.get("command") {
            Some(inner) if !inner.is_null() => current = inner.clone(),
            _ => return current,
        }
    }
    current
}

/// 批次要跑的标签补丁。执行哪个动作由 `command_kind` 列决定（见
/// [`action_kind_for`]），这里只负责把载荷取出来：child 批次存的是
/// `{"kind":"retry","command":…}` 封套，所以要认剥壳后的内层命令。
fn stored_patch(
    connection: &Connection,
    batch_id: &str,
) -> LibraryActResult<Option<(Vec<String>, Vec<String>)>> {
    let text: String = connection
        .query_row(
            "SELECT command_json FROM library_batches WHERE id = ?1",
            params![batch_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?
        .ok_or_else(|| {
            LibraryActError::of(LibraryActErrorCode::BatchNotFound, "batch disappeared")
        })?;
    let stored_command = effective_command(&text);
    if stored_command.get("kind").and_then(Value::as_str)
        != Some(BatchCommandKind::PatchTags.as_str())
    {
        // Move / Trash 的载荷不属于标签补丁：它们的目标路径已经逐项冻结在
        // Item 的 plan_json 里，由 [`execute_external`] 自己读。
        return Ok(None);
    }
    let argument: PatchTagsArgument = serde_json::from_value(stored_command).map_err(|error| {
        LibraryActError::unavailable(format!("stored tag patch is unreadable: {error}"))
    })?;
    Ok(Some((argument.add, argument.remove)))
}

/// 批次实际要跑的那个命令（已剥掉 retry / compensation 封套）。
/// 只有需要「整批共用一份载荷」的动作才读它；Move / Trash 的载荷在逐项计划里。
fn stored_command(connection: &Connection, batch_id: &str) -> LibraryActResult<BatchCommand> {
    let text: String = connection
        .query_row(
            "SELECT command_json FROM library_batches WHERE id = ?1",
            params![batch_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?
        .ok_or_else(|| {
            LibraryActError::of(LibraryActErrorCode::BatchNotFound, "batch disappeared")
        })?;
    serde_json::from_value(effective_command(&text)).map_err(|error| {
        LibraryActError::unavailable(format!("the stored command is not runnable: {error}"))
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PatchTagsArgument {
    add: Vec<String>,
    remove: Vec<String>,
}

/// 这个批次实际执行哪个动作：根批次用自己的 `command_kind`，child 批次只记
/// `retry` / `compensation`，要顺着父链解到第一个具体命令。
fn action_kind_for(
    connection: &Connection,
    batch_id: &str,
) -> LibraryActResult<Option<BatchCommandKind>> {
    let row = load_batch(connection, batch_id)?;
    let kind = parse_kind(&row.command_kind)?;
    if kind.is_concrete() {
        return Ok(Some(kind));
    }
    action_kind_of_ancestor(connection, row.parent_batch_id)
}

/// 顺着父链找第一个具体命令。「重试的重试」的直接父批次记的是 `retry`，只看它
/// 就没有策略可跑，所以 child 的动作要一直向上解到根命令为止。
fn action_kind_of_ancestor(
    connection: &Connection,
    ancestor_id: Option<String>,
) -> LibraryActResult<Option<BatchCommandKind>> {
    let mut current = ancestor_id;
    for _ in 0..MAX_LINEAGE_HOPS {
        let Some(id) = current else {
            return Ok(None);
        };
        let row = load_batch(connection, &id)?;
        current = row.parent_batch_id.clone();
        let kind = parse_kind(&row.command_kind)?;
        if kind.is_concrete() {
            return Ok(Some(kind));
        }
    }
    Err(LibraryActError::of(
        LibraryActErrorCode::UnsupportedForScope,
        "this batch's retry chain is too deep to resolve its action",
    ))
}

fn parse_kind(text: &str) -> LibraryActResult<BatchCommandKind> {
    BatchCommandKind::parse(text)
        .ok_or_else(|| LibraryActError::unavailable(format!("unknown stored batch kind {text}")))
}

/// 批次离根多远：根批次算 1，它的一个 child 算 2。
fn lineage_depth(connection: &Connection, batch_id: &str) -> LibraryActResult<usize> {
    let mut depth = 1usize;
    let mut current = load_batch(connection, batch_id)?.parent_batch_id;
    while let Some(id) = current {
        depth += 1;
        if depth > MAX_LINEAGE_HOPS + 1 {
            return Err(LibraryActError::of(
                LibraryActErrorCode::UnsupportedForScope,
                "this batch's retry chain is too deep to resolve its action",
            ));
        }
        current = load_batch(connection, &id)?.parent_batch_id;
    }
    Ok(depth)
}

fn run_items(
    transaction: &mut Transaction<'_>,
    items: &[ItemRow],
    patch: &Option<(Vec<String>, Vec<String>)>,
    action: ItemAction,
) -> LibraryActResult<RunOutcome> {
    if action.external() {
        return Err(LibraryActError::unavailable(
            "a file-system action cannot run inside the batch transaction",
        ));
    }
    let mut outcome = RunOutcome {
        bumped_domains: false,
        bumped_list: Vec::new(),
        compensable_ran: false,
    };
    let mut wrote_any = false;
    for item in items {
        let state = ItemState::parse(&item.state).ok_or_else(|| {
            LibraryActError::unavailable(format!("unknown stored item state {}", item.state))
        })?;
        if state.is_terminal() {
            // 已经终结的项永不改写；重试另开 child。
            continue;
        }
        // 每个 Item 一个 SAVEPOINT：失败只回滚到该项之前，其他项的成功结果保留。
        // `rollback` 只借 &mut Savepoint 不消耗它，所以整个生命周期关进一个块，
        // 失败记录等块结束、Savepoint 释放事务的可借之后再写。
        let outcome = {
            let mut savepoint = transaction
                .savepoint()
                .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
            let result = match action {
                ItemAction::ApplyTags => {
                    let (add, remove) = patch.clone().ok_or_else(|| {
                        LibraryActError::unavailable("the stored plan has no tag patch")
                    })?;
                    apply_tags_to_item(&savepoint, item, &add, &remove)
                }
                ItemAction::RevertTags => revert_tags_for_item(&savepoint, item),
                ItemAction::ApplyLifecycle => apply_lifecycle_to_item(&savepoint, item),
                ItemAction::RevertLifecycle => revert_lifecycle_for_item(&savepoint, item),
                // 函数开头已经拒掉所有事务外动作，能走到这里说明调用方绕过了分派。
                ItemAction::ApplyMove
                | ItemAction::RevertMove
                | ItemAction::ApplyTrash
                | ItemAction::RevertTrash
                | ItemAction::ApplyImport
                | ItemAction::RevertImport
                | ItemAction::ApplyExport
                | ItemAction::RevertExport
                | ItemAction::ApplyOcr
                | ItemAction::ApplyBrief => {
                    unreachable!(
                        "file-system and provider actions never run inside the batch transaction"
                    )
                }
            };
            match result {
                Ok(wrote) => {
                    savepoint
                        .commit()
                        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
                    Ok(wrote)
                }
                Err(failure) => {
                    // 回滚业务写；失败结果不属于这次回滚的范围。
                    savepoint.rollback().map_err(|_| {
                        LibraryActError::unavailable("the item savepoint could not be rolled back")
                    })?;
                    Err(failure)
                }
            }
        };
        match outcome {
            Ok(wrote) => wrote_any |= wrote,
            Err(failure) => record_item_failure(transaction, &item.id, &failure)?,
        }
    }
    if wrote_any {
        outcome.bumped_domains = true;
        outcome.bumped_list = match action {
            ItemAction::ApplyTags | ItemAction::RevertTags => {
                vec![LibraryDomain::Tags, LibraryDomain::Artifacts]
            }
            ItemAction::ApplyLifecycle | ItemAction::RevertLifecycle => {
                vec![LibraryDomain::Lifecycle]
            }
            _ => Vec::new(),
        };
        outcome.compensable_ran = !action.compensating();
    }
    Ok(outcome)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemAction {
    ApplyTags,
    RevertTags,
    ApplyMove,
    RevertMove,
    ApplyTrash,
    RevertTrash,
    ApplyImport,
    RevertImport,
    ApplyExport,
    RevertExport,
    ApplyLifecycle,
    RevertLifecycle,
    ApplyOcr,
    ApplyBrief,
}

impl ItemAction {
    /// §7 末尾：只有纯 SQLite 动作能待在批次事务里；Move / Trash / Import / Export
    /// 要改文件系统，而且复用 `PaperModule`（自带连接），必须在事务外逐项跑。
    const fn external(self) -> bool {
        matches!(
            self,
            Self::ApplyMove
                | Self::RevertMove
                | Self::ApplyTrash
                | Self::RevertTrash
                | Self::ApplyImport
                | Self::RevertImport
                | Self::ApplyExport
                | Self::RevertExport
        )
    }

    const fn provider(self) -> bool {
        matches!(self, Self::ApplyOcr | Self::ApplyBrief)
    }

    /// 撤销策略：错误码要用「冲突」而不是「计划过期」，也不配新的 Undo Token。
    const fn compensating(self) -> bool {
        matches!(
            self,
            Self::RevertTags
                | Self::RevertMove
                | Self::RevertTrash
                | Self::RevertImport
                | Self::RevertExport
                | Self::RevertLifecycle
        )
    }

    /// 「目标此刻必须躺在回收站里」的策略：只有 Trash 的撤销是这种复验——库里的
    /// 事实是已删除，after-image 记的也是 trashed 快照。导入的撤销方向相反
    /// （把本批新建的 Paper 移进回收站），它要的是普通的路径复验。
    const fn target_must_be_trashed(self) -> bool {
        matches!(self, Self::RevertTrash)
    }
}

/// 策略分派。批次跑哪个动作由 [`action_kind_for`] 沿父链解析得到：child 批次的
/// `command_kind` 列只记 `retry` / `compensation`，解不出具体命令时直接失败，
/// 绝不猜一个策略来跑。
fn resolve_action(
    action_kind: Option<BatchCommandKind>,
    compensating: bool,
) -> LibraryActResult<ItemAction> {
    let unresolved = || {
        LibraryActError::of(
            LibraryActErrorCode::UnsupportedForScope,
            "a child batch must record which action it retries or compensates",
        )
    };
    let effective = action_kind.ok_or_else(unresolved)?;
    match (effective, compensating) {
        (BatchCommandKind::PatchTags, false) => Ok(ItemAction::ApplyTags),
        (BatchCommandKind::PatchTags, true) => Ok(ItemAction::RevertTags),
        (BatchCommandKind::Move, false) => Ok(ItemAction::ApplyMove),
        (BatchCommandKind::Move, true) => Ok(ItemAction::RevertMove),
        (BatchCommandKind::Trash, false) => Ok(ItemAction::ApplyTrash),
        (BatchCommandKind::Trash, true) => Ok(ItemAction::RevertTrash),
        (BatchCommandKind::Import, false) => Ok(ItemAction::ApplyImport),
        (BatchCommandKind::Import, true) => Ok(ItemAction::RevertImport),
        (BatchCommandKind::Export, false) => Ok(ItemAction::ApplyExport),
        (BatchCommandKind::Export, true) => Ok(ItemAction::RevertExport),
        (BatchCommandKind::PatchLifecycle, false) => Ok(ItemAction::ApplyLifecycle),
        (BatchCommandKind::PatchLifecycle, true) => Ok(ItemAction::RevertLifecycle),
        (BatchCommandKind::Ocr, false) => Ok(ItemAction::ApplyOcr),
        (BatchCommandKind::Brief, false) => Ok(ItemAction::ApplyBrief),
        (BatchCommandKind::Ocr | BatchCommandKind::Brief, true) => Err(LibraryActError::of(
            LibraryActErrorCode::NotReversible,
            "provider batches cannot be compensated; cancel remaining jobs instead",
        )),
        (BatchCommandKind::Retry | BatchCommandKind::Compensation, _) => Err(unresolved()),
    }
}

fn apply_lifecycle_to_item(connection: &Connection, item: &ItemRow) -> Result<bool, ItemFailure> {
    let plan: Value = serde_json::from_str(&item.plan_json).map_err(|_| ItemFailure::storage())?;
    let paper_id = plan
        .get("paperId")
        .and_then(Value::as_str)
        .ok_or_else(ItemFailure::storage)?;
    let expected = plan
        .get("expectedVersion")
        .and_then(Value::as_i64)
        .ok_or_else(ItemFailure::storage)?;
    let patch: crate::library_lifecycle::LifecyclePatch =
        serde_json::from_value(plan.get("patch").cloned().unwrap_or(Value::Null))
            .map_err(|_| ItemFailure::storage())?;
    crate::library_lifecycle::apply_lifecycle_patch(connection, paper_id, expected, &patch, &now())
        .map(|(_, wrote)| wrote)
        .map_err(|error| {
            if error.code == "stale_library_snapshot" {
                ItemFailure::new(
                    LibraryActErrorCode::StaleLibrarySnapshot,
                    "this Paper's reading state changed after the batch was planned",
                )
            } else {
                ItemFailure::storage()
            }
        })
}

fn revert_lifecycle_for_item(connection: &Connection, item: &ItemRow) -> Result<bool, ItemFailure> {
    let compensation: Value = serde_json::from_str(
        item.compensation_json
            .as_deref()
            .ok_or_else(ItemFailure::storage)?,
    )
    .map_err(|_| ItemFailure::storage())?;
    let paper_id = item.paper_id.as_deref().ok_or_else(ItemFailure::storage)?;
    let current = crate::library_lifecycle::load_lifecycle(connection, paper_id)
        .map_err(|_| ItemFailure::storage())?;
    let patch = crate::library_lifecycle::LifecyclePatch {
        status: compensation
            .get("status")
            .and_then(Value::as_str)
            .and_then(|value| serde_json::from_value(json!(value)).ok()),
        favorite: compensation.get("favorite").and_then(Value::as_bool),
        priority: compensation.get("priority").and_then(Value::as_i64),
        read_later: compensation.get("readLater").and_then(Value::as_bool),
        review_at: Some(
            compensation
                .get("reviewAt")
                .and_then(|value| {
                    if value.is_null() {
                        Some(None)
                    } else {
                        value.as_str().map(|text| Some(text.to_string()))
                    }
                })
                .unwrap_or(None),
        ),
    };
    crate::library_lifecycle::apply_lifecycle_patch(
        connection,
        paper_id,
        current.version,
        &patch,
        &now(),
    )
    .map(|(_, wrote)| wrote)
    .map_err(|_| ItemFailure::storage())
}

/// 执行一个标签补丁 Item：先按冻结的 precondition 复验，再写最终集合。
/// 返回「这一项是否真的改了库」——幂等补丁不该换来 revision 和一个撤销不了
/// 任何东西的 Undo Token。
fn apply_tags_to_item(
    connection: &Connection,
    item: &ItemRow,
    add: &[String],
    remove: &[String],
) -> Result<bool, ItemFailure> {
    let plan: TagItemPlan =
        serde_json::from_str(&item.plan_json).map_err(|_| ItemFailure::storage())?;
    if plan.action != TAG_PLAN_ACTION {
        return Err(ItemFailure::new(
            LibraryActErrorCode::UnsupportedForScope,
            "this item is not a tag patch",
        ));
    }
    let current = current_tags(connection, &plan.paper_id)
        .map_err(|_| ItemFailure::storage())?
        .ok_or_else(|| ItemFailure::new(LibraryActErrorCode::PaperNotFound, "the Paper is gone"))?;
    let fresh = canonical_digest(&tag_precondition_value(&plan.paper_id, &current));
    if fresh != item.precondition_digest {
        // 只拒这一项：计划之后有人改过它的标签，静默覆盖就是 lost update。
        return Err(ItemFailure::new(
            LibraryActErrorCode::StaleSelection,
            "these tags changed after the plan was made",
        ));
    }
    let final_tags = apply_tag_patch(&current, add, remove);
    let changed = !same_tags(&current, &final_tags);
    if changed {
        write_tags(connection, &plan.paper_id, &final_tags)?;
    }
    let after = if changed {
        current_tags(connection, &plan.paper_id)
            .map_err(|_| ItemFailure::storage())?
            .unwrap_or_default()
    } else {
        current.clone()
    };
    finish_item(
        connection,
        &item.id,
        &ItemEffect::settled(
            json!({ "beforeTags": current, "afterTags": after }),
            json!({ "paperId": plan.paper_id, "tags": after }),
        ),
    )?;
    Ok(changed)
}

/// 补偿一个标签 Item：只有当前标签仍等于本批写下的结果时才能撤销。
/// 返回值同样是「这一项是否真的改了库」。
fn revert_tags_for_item(connection: &Connection, item: &ItemRow) -> Result<bool, ItemFailure> {
    let compensation: TagCompensation = item
        .compensation_json
        .as_deref()
        .and_then(|text| serde_json::from_str(text).ok())
        .ok_or_else(|| {
            ItemFailure::new(
                LibraryActErrorCode::NotReversible,
                "this item recorded no tag before-image",
            )
        })?;
    let current = current_tags(connection, &compensation.paper_id)
        .map_err(|_| ItemFailure::storage())?
        .ok_or_else(|| ItemFailure::new(LibraryActErrorCode::PaperNotFound, "the Paper is gone"))?;
    if !same_tags(&current, &compensation.tags) {
        return Err(ItemFailure::new(
            LibraryActErrorCode::UndoConflict,
            "these tags were changed again after the batch; the undo stopped for this Paper",
        ));
    }
    // 回到 before_tags 只能读父 Item 冻结的那份计划；缺了就报失败，
    // 拿 after-image「撤销」会伪装成一次成功的空写。
    let plan: TagItemPlan =
        serde_json::from_str(&item.plan_json).map_err(|_| ItemFailure::storage())?;
    if plan.action != TAG_PLAN_ACTION {
        return Err(ItemFailure::new(
            LibraryActErrorCode::UnsupportedForScope,
            "this item is not a tag patch",
        ));
    }
    let changed = !same_tags(&current, &plan.before_tags);
    if changed {
        write_tags(connection, &compensation.paper_id, &plan.before_tags)?;
    }
    let after = if changed {
        current_tags(connection, &compensation.paper_id)
            .map_err(|_| ItemFailure::storage())?
            .unwrap_or_default()
    } else {
        current.clone()
    };
    finish_item(
        connection,
        &item.id,
        &ItemEffect::settled(
            json!({ "beforeTags": current, "afterTags": after, "compensated": true }),
            json!({ "paperId": compensation.paper_id, "tags": after }),
        ),
    )?;
    Ok(changed)
}

/// 写最终标签集合，并把 brief artifact 的 keywords 同步到同一份值——单篇
/// `update_paper_tags` 一直是这个语义，批量路径不能给出第二套含义。
fn write_tags(connection: &Connection, paper_id: &str, tags: &[String]) -> Result<(), ItemFailure> {
    let timestamp = now();
    connection
        .execute(
            "DELETE FROM paper_tags WHERE paper_id = ?1",
            params![paper_id],
        )
        .map_err(|_| ItemFailure::storage())?;
    for tag_name in tags {
        connection
            .execute(
                "INSERT OR IGNORE INTO tags(id, name, created_at) VALUES (?1, ?2, ?3)",
                params![uuid::Uuid::new_v4().to_string(), tag_name, timestamp],
            )
            .map_err(|_| ItemFailure::storage())?;
        let tag_id: String = connection
            .query_row(
                "SELECT id FROM tags WHERE name = ?1",
                params![tag_name],
                |row| row.get(0),
            )
            .map_err(|_| ItemFailure::storage())?;
        connection
            .execute(
                "INSERT OR IGNORE INTO paper_tags(paper_id, tag_id) VALUES (?1, ?2)",
                params![paper_id, tag_id],
            )
            .map_err(|_| ItemFailure::storage())?;
    }
    // keywords 镜像：没有 brief 就没有 artifact，跳过即可，不算失败。
    let brief: Option<(String, String)> = connection
        .query_row(
            "SELECT a.id, a.content_json FROM artifacts a
             JOIN artifact_heads h ON h.artifact_id = a.id
             WHERE a.paper_id = ?1 AND a.kind = 'brief'",
            params![paper_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|_| ItemFailure::storage())?;
    if let Some((brief_id, raw)) = brief {
        if let Ok(mut content) = serde_json::from_str::<Value>(&raw) {
            if let Some(object) = content.as_object_mut() {
                object.insert("keywords".to_string(), json!(tags));
                let text = serde_json::to_string(&content).unwrap_or(raw);
                connection
                    .execute(
                        "UPDATE artifacts SET content_json = ?1 WHERE id = ?2",
                        params![text, brief_id],
                    )
                    .map_err(|_| ItemFailure::storage())?;
            }
        }
    }
    Ok(())
}

fn same_tags(left: &[String], right: &[String]) -> bool {
    let normalize = |tags: &[String]| -> Vec<String> {
        let mut keys: Vec<String> = tags
            .iter()
            .map(|tag| tag.trim().to_lowercase())
            .filter(|tag| !tag.is_empty())
            .collect();
        keys.sort();
        keys.dedup();
        keys
    };
    normalize(left) == normalize(right)
}

/// 一项跑完后的落库，事务内外两条路径共用同一个写者：终态、结果摘要，加上
/// 「只有执行才产出」的三样东西（after-image、Paper 身份、补偿路径）。
///
/// 三样都是 COALESCE：`None` 的含义是「保持计划阶段冻结的那一份」，不是「清空」。
/// 导入的 reused / conflict 结论没有新效果，它们的 precondition 仍然是计划里那一个；
/// 把「什么都没产出」写成「把产出抹掉」会让撤销凭空失去依据。
fn finish_item(
    connection: &Connection,
    item_id: &str,
    effect: &ItemEffect,
) -> Result<(), ItemFailure> {
    let timestamp = now();
    let (paper_id, revision_id) = match &effect.paper {
        Some((paper_id, revision_id)) => (Some(paper_id.clone()), Some(revision_id.clone())),
        None => (None, None),
    };
    connection
        .execute(
            "UPDATE library_batch_items
             SET state = ?2, result_json = ?3,
                 precondition_digest = COALESCE(?4, precondition_digest),
                 paper_id = COALESCE(?5, paper_id),
                 revision_id = COALESCE(?6, revision_id),
                 compensation_json = COALESCE(?7, compensation_json),
                 error_code = NULL, error_summary = NULL, attempt_count = attempt_count + 1,
                 started_at = COALESCE(started_at, ?8), finished_at = ?8, updated_at = ?8
             WHERE id = ?1",
            params![
                item_id,
                effect.state.as_str(),
                effect.result.to_string(),
                effect.precondition.as_ref().map(canonical_digest),
                paper_id,
                revision_id,
                effect.compensation.as_ref().map(|value| value.to_string()),
                timestamp,
            ],
        )
        .map_err(|_| ItemFailure::storage())?;
    Ok(())
}

fn record_item_failure(
    connection: &Connection,
    item_id: &str,
    failure: &ItemFailure,
) -> LibraryActResult<()> {
    let timestamp = now();
    connection
        .execute(
            "UPDATE library_batch_items
             SET state = ?2, error_code = ?3, error_summary = ?4,
                 attempt_count = attempt_count + 1, finished_at = ?5, updated_at = ?5
             WHERE id = ?1",
            params![
                item_id,
                ItemState::Failed.as_str(),
                failure.code.as_str(),
                failure.safe_summary,
                timestamp,
            ],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    Ok(())
}

fn start_response(
    connection: &Connection,
    batch_id: &str,
    undo_token: Option<String>,
) -> LibraryActResult<LibraryActResponse> {
    let batch = batch_projection(connection, batch_id)?;
    Ok(LibraryActResponse::StartBatch { batch, undo_token })
}

/// 批次终态由 Items 重算；`finished_at` 只在全员 terminal 时写入，
/// `updated_at` 则每一次都要落下——聚合结果还没终结（例如还剩效果未知的项）
/// 不等于这一行没有更新时间。
fn finish_batch(connection: &Connection, batch_id: &str) -> LibraryActResult<()> {
    let counts = item_counts(connection, batch_id)?;
    let state = aggregate_state(&counts, true);
    let timestamp = now();
    let finished: Option<&str> = matches!(
        state,
        BatchState::Completed
            | BatchState::CompletedWithErrors
            | BatchState::Failed
            | BatchState::Cancelled
    )
    .then_some(timestamp.as_str());
    connection
        .execute(
            "UPDATE library_batches SET state = ?2, finished_at = COALESCE(finished_at, ?3), updated_at = ?4 WHERE id = ?1",
            params![batch_id, state.as_str(), finished, timestamp],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    Ok(())
}

/// §5.2 的聚合真值表，逐条按文档优先级实现。
pub(crate) fn aggregate_state(counts: &ItemCounts, started: bool) -> BatchState {
    if !started {
        return BatchState::Planned;
    }
    if counts.running > 0 {
        return BatchState::Running;
    }
    if counts.queued + counts.planned > 0 {
        return BatchState::Queued;
    }
    if counts.interrupted_unknown > 0 {
        return BatchState::InterruptedUnknown;
    }
    if counts.action_required > 0 {
        return BatchState::ActionRequired;
    }
    if counts.paused > 0 {
        return BatchState::Paused;
    }
    if counts.cancelled == counts.total() && counts.total() > 0 {
        return BatchState::Cancelled;
    }
    if counts.failed == counts.total() && counts.total() > 0 {
        return BatchState::Failed;
    }
    let reversible_side = counts.succeeded + counts.skipped;
    let error_side = counts.failed + counts.cancelled;
    if reversible_side > 0 && error_side > 0 {
        return BatchState::CompletedWithErrors;
    }
    BatchState::Completed
}

// ---------------------------------------------------------------------------
// undo token
// ---------------------------------------------------------------------------

/// §7.3：Token 单次、不透明、库内只存 hash，默认窗口 10 分钟。
///
/// 冻结的是「执行成功后重算的 precondition」（after-image）：撤销时目标若被再次
/// 改过，那一项只会自己失败，不会覆盖新操作。
fn mint_undo_token(connection: &Connection, batch_id: &str) -> LibraryActResult<String> {
    let mut preconditions: Vec<String> =
        load_items(connection, batch_id, Some(&[ItemState::Succeeded]))?
            .into_iter()
            .filter(|item| item.compensation_json.is_some())
            .map(|item| item.precondition_digest)
            .collect();
    preconditions.sort();
    let token = uuid::Uuid::new_v4().simple().to_string();
    let timestamp = now();
    connection
        .execute(
            "INSERT INTO library_undo_tokens(
               token_hash, batch_id, precondition_digest, state, expires_at, created_at, consumed_at
             ) VALUES (?1,?2,?3,'available',?4,?5,NULL)",
            params![
                sha256_hex(token.as_bytes()),
                batch_id,
                canonical_digest(&json!({ "items": preconditions })),
                future_minutes(UNDO_TTL_MINUTES),
                timestamp,
            ],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    Ok(token)
}

/// §7.3：撤销 = 开一个 `relation='compensation'` 的 child Batch，逐项校验
/// precondition，冲突项只失败自己，绝不改写父记录。
fn undo_batch(
    transaction: &mut Transaction<'_>,
    batch_id: &str,
    token: &str,
) -> LibraryActResult<ActOutcome> {
    let hash = sha256_hex(token.as_bytes());
    let row: Option<(String, String, String)> = transaction
        .query_row(
            "SELECT batch_id, state, expires_at FROM library_undo_tokens WHERE token_hash = ?1",
            params![hash],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let Some((token_batch_id, state_text, expires_at)) = row else {
        return Err(LibraryActError::of(
            LibraryActErrorCode::UndoConflict,
            "this undo token is unknown or belongs to another workspace",
        ));
    };
    if token_batch_id != batch_id {
        return Err(LibraryActError::of(
            LibraryActErrorCode::UndoConflict,
            "this undo token belongs to a different batch",
        ));
    }
    let token_state = UndoTokenState::parse(&state_text).ok_or_else(|| {
        LibraryActError::unavailable(format!("unknown undo token state {state_text}"))
    })?;
    // 「窗口已过」和「已经用过」必须是不同的错误码，而且不能取决于
    // `reconcile_interrupted` 是否已经先把状态改成 expired。
    if token_state == UndoTokenState::Expired {
        return Err(LibraryActError::of(
            LibraryActErrorCode::UndoExpired,
            "the undo window has passed; the batch can no longer be reversed",
        ));
    }
    if token_state != UndoTokenState::Available {
        return Err(LibraryActError::of(
            LibraryActErrorCode::UndoConflict,
            match token_state {
                UndoTokenState::Consumed => "this undo has already been used",
                _ => "this undo is no longer available",
            },
        ));
    }
    if expires_at.as_str() <= now().as_str() {
        transaction
            .execute(
                "UPDATE library_undo_tokens SET state = 'expired' WHERE token_hash = ?1",
                params![hash],
            )
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        return Err(LibraryActError::of(
            LibraryActErrorCode::UndoExpired,
            "the undo window has passed; the batch can no longer be reversed",
        ));
    }
    let parent = load_batch(transaction, batch_id)?;
    let undo_policy = UndoPolicy::parse(&parent.undo_policy).ok_or_else(|| {
        LibraryActError::unavailable(format!("unknown undo policy {}", parent.undo_policy))
    })?;
    if !undo_policy.compensable() {
        return Err(LibraryActError::of(
            LibraryActErrorCode::NotReversible,
            "this batch has no reversible effect to compensate",
        ));
    }
    let parent_kind = action_kind_for(transaction, batch_id)?;
    let sources = load_items(transaction, batch_id, Some(&[ItemState::Succeeded]))?;
    let compensable: Vec<&ItemRow> = sources
        .iter()
        .filter(|item| item.compensation_json.is_some())
        .collect();
    if compensable.is_empty() {
        return Err(LibraryActError::of(
            LibraryActErrorCode::NotReversible,
            "nothing in this batch recorded a compensation path",
        ));
    }
    let child_id = uuid::Uuid::new_v4().to_string();
    insert_batch(
        transaction,
        &child_id,
        BatchCommandKind::Compensation,
        &json!({
            "kind": "compensation",
            "command": effective_command(&parent.command_json),
        })
        .to_string(),
        &canonical_digest(&json!({
            "compensates": batch_id,
            "items": compensable.iter().map(|item| item.precondition_digest.clone()).collect::<Vec<_>>(),
        })),
        &canonical_digest(&json!({
            "compensates": batch_id,
            "targets": compensable.iter().map(|item| item.target_key.clone()).collect::<Vec<_>>(),
        })),
        UndoPolicy::None,
        &PlanSummary::local(Vec::new()),
        Some((batch_id, "compensation")),
    )?;
    for (ordinal, source) in compensable.iter().enumerate() {
        let item_id = uuid::Uuid::new_v4().to_string();
        // child Item 的 precondition 是父 Item 执行后重算的那一个（after-image），
        // 目标被再次改过时补偿自己失败，不覆盖新操作。
        let planned = PlannedItem {
            ordinal: ordinal as i64,
            target_kind: source.target_kind(),
            target_key: source.target_key.clone(),
            dedupe_key: source.dedupe_key.clone(),
            paper_id: source.paper_id.clone(),
            revision_id: source.revision_id.clone(),
            precondition_digest: source.precondition_digest.clone(),
            plan_json: source.plan_json.clone(),
            compensation_json: source.compensation_json.clone(),
            state: ItemState::Planned,
            result_json: None,
            prepared_job_handle: None,
        };
        insert_item(transaction, &item_id, &child_id, &planned, Some(&source.id))?;
    }
    transaction
        .execute(
            "UPDATE library_undo_tokens SET state = 'consumed', consumed_at = ?2 WHERE token_hash = ?1",
            params![hash, now()],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    transaction
        .execute(
            "UPDATE library_batches SET state = ?2, started_at = ?3, updated_at = ?3 WHERE id = ?1",
            params![child_id, BatchState::Running.as_str(), now()],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let action = resolve_action(parent_kind, true)?;
    if action.external() {
        mark_items_queued(transaction, &child_id)?;
        return Ok(ActOutcome::Deferred(DeferredRun {
            batch_id: child_id,
            action,
            // 「撤销的撤销」不属于合同：补偿批次不再配新的 Token。
            mints_undo: false,
            shape: ResponseShape::Control,
        }));
    }
    let pending = load_items(transaction, &child_id, None)?;
    let patch = stored_patch(transaction, &child_id)?;
    let run = run_items(transaction, &pending, &patch, action)?;
    if run.bumped_domains {
        bump_library_revisions(transaction, &run.bumped_list)
            .map_err(LibraryActError::unavailable)?;
    }
    finish_batch(transaction, &child_id)?;
    let batch = batch_projection(transaction, &child_id)?;
    Ok(ActOutcome::Done(LibraryActResponse::ControlBatch {
        batch,
        undo_token: None,
    }))
}

// ---------------------------------------------------------------------------
// retry / cancel
// ---------------------------------------------------------------------------

/// §7.2：重试创建 child Batch，父记录不可改写；只复制失败且可重试的项。
///
/// 三种动作共用一条规则：重试 = 拿当前事实把同一份命令再计划一次。所以逐项
/// precondition 在冻结时重算，「已经在目标位」这类项直接落 skipped，不给它一次
/// 空跑；确认项不重开——用户在父批次上接受的就是这份命令。
fn retry_batch(
    transaction: &mut Transaction<'_>,
    root: &Path,
    batch_id: &str,
    item_ids: Option<&[String]>,
    provider: &ProviderActContext,
) -> LibraryActResult<ActOutcome> {
    let parent = load_batch(transaction, batch_id)?;
    let parent_state = BatchState::parse(&parent.state).ok_or_else(|| {
        LibraryActError::unavailable(format!("unknown stored batch state {}", parent.state))
    })?;
    if parent_state != BatchState::CompletedWithErrors && parent_state != BatchState::Failed {
        return Err(LibraryActError::of(
            LibraryActErrorCode::UnsupportedForScope,
            "only a failed or partially failed batch can be retried",
        ));
    }
    // §7.2 的重试语义是「把没做完的正向动作再做一次」。补偿批次的失败项是撤销冲突，
    // 它的 before/after 镜像只属于那一次撤销：重跑要么变成一次伪装成功的空写，
    // 要么把新旧镜像混在一起，所以这里明确不开重试。
    if parent.command_kind == BatchCommandKind::Compensation.as_str() {
        return Err(LibraryActError::of(
            LibraryActErrorCode::UnsupportedForScope,
            "a compensation batch cannot be retried; its failed items need a new decision",
        ));
    }
    let parent_kind = action_kind_for(transaction, batch_id)?;
    let action = resolve_action(parent_kind, false)?;
    // child 会比父批次再深一跳；解不出策略的批次连投影都做不出来，
    // 所以宁可现在拒绝，也不要留下一个任务中心永远读不懂的记录。
    if lineage_depth(transaction, batch_id)? > MAX_LINEAGE_HOPS {
        return Err(LibraryActError::of(
            LibraryActErrorCode::UnsupportedForScope,
            "this batch is already a long retry chain; run a new operation instead",
        ));
    }
    let failed = load_items(transaction, batch_id, Some(&[ItemState::Failed]))?;
    let wanted: Vec<&ItemRow> = failed
        .iter()
        .filter(|item| match item_ids {
            None => item.retryable(),
            Some(ids) => ids.iter().any(|id| *id == item.id),
        })
        .collect();
    if wanted.is_empty() {
        return Err(LibraryActError::of(
            LibraryActErrorCode::SelectionEmpty,
            "no failed item in this batch is retryable",
        ));
    }
    let command = stored_command(transaction, batch_id)?;
    let sources: BTreeMap<String, &ItemRow> = wanted
        .iter()
        .map(|item| (item.target_key.clone(), *item))
        .collect();
    let draft = match &command {
        // 导入的成员是磁盘上的源文件，不是 Paper 成员：重试时按每一项自己冻结的
        // `plan_json` 再预检一次（文件回来了、或写完了，结论就会不一样），
        // `target_key` 沿用父项那一个，任务中心才能把重试项对回它替代的那一项。
        BatchCommand::Import { collection_path } => {
            let mut retry_sources = Vec::with_capacity(wanted.len());
            for item in &wanted {
                retry_sources.push(source_candidate_of(item)?);
            }
            let mut draft = plan_import_items(transaction, root, &retry_sources, collection_path)?;
            for (item, parent) in draft.items.iter_mut().zip(wanted.iter()) {
                item.target_key = parent.target_key.clone();
            }
            draft
        }
        _ => {
            let members: Vec<crate::library_query::SelectionMember> = wanted
                .iter()
                .map(|item| crate::library_query::SelectionMember {
                    paper_id: item.paper_id.clone().unwrap_or_default(),
                    revision_id: item.revision_id.clone().unwrap_or_default(),
                })
                .collect();
            plan_items(transaction, root, &command, &members, provider)?
        }
    };
    let mut draft = draft;
    let committed_retries = wanted
        .iter()
        .filter(|item| parent_job_was_committed(transaction, &item.id).unwrap_or(false))
        .count() as i64;
    if committed_retries > 0 {
        draft.requirements.push(BatchRequirement {
            id: REQUIREMENT_POSSIBLE_CHARGE.to_string(),
            kind: BatchRequirementKind::PossibleDuplicateCharge,
            item_count: committed_retries,
            label: format!(
                "{committed_retries} retry item(s) may charge again because the original provider request was already committed"
            ),
        });
    }
    let child_id = uuid::Uuid::new_v4().to_string();
    insert_batch(
        transaction,
        &child_id,
        BatchCommandKind::Retry,
        &json!({
            "kind": "retry",
            "command": effective_command(&parent.command_json),
        })
        .to_string(),
        &canonical_digest(&json!({
            "retries": batch_id,
            "items": wanted.iter().map(|item| item.target_key.clone()).collect::<Vec<_>>(),
        })),
        &canonical_digest(&json!({
            "retries": batch_id,
            "targets": wanted.iter().map(|item| item.target_key.clone()).collect::<Vec<_>>(),
        })),
        draft.undo_policy,
        &PlanSummary::provider(draft.requirements.clone(), draft.cost.clone()),
        Some((batch_id, "retry")),
    )?;
    for item in &draft.items {
        let source = sources
            .get(&item.target_key)
            .ok_or_else(|| LibraryActError::unavailable("the retry plan lost a target"))?;
        insert_item(
            transaction,
            &uuid::Uuid::new_v4().to_string(),
            &child_id,
            item,
            Some(&source.id),
        )?;
    }
    transaction
        .execute(
            "UPDATE library_batches SET state = ?2, started_at = ?3, updated_at = ?3 WHERE id = ?1",
            params![child_id, BatchState::Running.as_str(), now()],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    if action.external() {
        mark_items_queued(transaction, &child_id)?;
        return Ok(ActOutcome::Deferred(DeferredRun {
            batch_id: child_id,
            action,
            mints_undo: true,
            shape: ResponseShape::Control,
        }));
    }
    if action.provider() {
        let pending = load_items(transaction, &child_id, None)?;
        start_provider_items(transaction, &child_id, &pending, action, provider)?;
        finish_batch(transaction, &child_id)?;
        let batch = batch_projection(transaction, &child_id)?;
        return Ok(ActOutcome::Done(LibraryActResponse::ControlBatch {
            batch,
            undo_token: None,
        }));
    }
    let patch = stored_patch(transaction, &child_id)?;
    let pending = load_items(transaction, &child_id, None)?;
    let run = run_items(transaction, &pending, &patch, action)?;
    if run.bumped_domains {
        bump_library_revisions(transaction, &run.bumped_list)
            .map_err(LibraryActError::unavailable)?;
    }
    finish_batch(transaction, &child_id)?;
    let undo_token = if run.compensable_ran {
        Some(mint_undo_token(transaction, &child_id)?)
    } else {
        None
    };
    let batch = batch_projection(transaction, &child_id)?;
    Ok(ActOutcome::Done(LibraryActResponse::ControlBatch {
        batch,
        undo_token,
    }))
}

/// §7.1：取消只处理尚未开始的 Item；已完成的保留 succeeded。
/// 共享 Job 只解除本批消费者；独占且未 commit 的 Job 才真正取消。
fn cancel_remaining(transaction: &Connection, batch_id: &str) -> LibraryActResult<ActOutcome> {
    load_batch(transaction, batch_id)?;
    let timestamp = now();
    detach_or_cancel_provider_jobs(transaction, batch_id)?;
    transaction
        .execute(
            "UPDATE library_batch_items
             SET state = ?3, finished_at = ?2, updated_at = ?2
             WHERE batch_id = ?1 AND state IN ('planned','queued')",
            params![batch_id, timestamp, ItemState::Cancelled.as_str()],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    transaction
        .execute(
            "UPDATE library_batches SET cancel_requested_at = COALESCE(cancel_requested_at, ?2), updated_at = ?2 WHERE id = ?1",
            params![batch_id, timestamp],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    finish_batch(transaction, batch_id)?;
    let batch = batch_projection(transaction, batch_id)?;
    Ok(ActOutcome::Done(LibraryActResponse::ControlBatch {
        batch,
        undo_token: None,
    }))
}

fn parent_job_was_committed(connection: &Connection, item_id: &str) -> LibraryActResult<bool> {
    let committed: Option<i64> = connection
        .query_row(
            "SELECT jobs.provider_committed
             FROM library_batch_job_links links
             JOIN jobs ON jobs.id = links.job_id
             WHERE links.batch_item_id = ?1
             ORDER BY links.created_at DESC LIMIT 1",
            params![item_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    Ok(committed == Some(1))
}

fn detach_or_cancel_provider_jobs(connection: &Connection, batch_id: &str) -> LibraryActResult<()> {
    let links: Vec<(String, String, String, i64)> = {
        let mut statement = connection
            .prepare(
                "SELECT links.batch_item_id, links.job_id, links.ownership, jobs.provider_committed
                 FROM library_batch_job_links links
                 JOIN library_batch_items items ON items.id = links.batch_item_id
                 JOIN jobs ON jobs.id = links.job_id
                 WHERE items.batch_id = ?1 AND links.consumer_state = 'active'",
            )
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        let rows = statement
            .query_map(params![batch_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        rows
    };
    let timestamp = now();
    for (item_id, job_id, ownership, committed) in links {
        connection
            .execute(
                "UPDATE library_batch_job_links
                 SET consumer_state = 'detached', detached_at = ?2
                 WHERE batch_item_id = ?1 AND job_id = ?3 AND consumer_state = 'active'",
                params![item_id, timestamp, job_id],
            )
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        if ownership != "created" || committed != 0 {
            continue;
        }
        let others: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM library_batch_job_links
                 WHERE job_id = ?1 AND consumer_state = 'active'",
                params![job_id],
                |row| row.get(0),
            )
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        if others == 0 {
            let _ = cancel_job_on(connection, &job_id);
        }
    }
    Ok(())
}

fn control_batch(
    transaction: &mut Transaction<'_>,
    root: &Path,
    batch_id: &str,
    control: &BatchControl,
    provider: &ProviderActContext,
) -> LibraryActResult<ActOutcome> {
    match control {
        BatchControl::CancelRemaining => cancel_remaining(transaction, batch_id),
        BatchControl::RetryFailed { item_ids } => {
            retry_batch(transaction, root, batch_id, item_ids.as_deref(), provider)
        }
        BatchControl::Undo { token } => undo_batch(transaction, batch_id, token),
    }
}

// ---------------------------------------------------------------------------
// 读投影
// ---------------------------------------------------------------------------

const BATCH_COLUMNS: &str = "command_kind, command_json, state, plan_digest, target_digest,
                             undo_policy, parent_batch_id, relation, plan_expires_at,
                             cancel_requested_at, created_at, started_at, finished_at,
                             updated_at, cost_preview_json";

#[derive(Debug, Clone)]
struct BatchRow {
    id: String,
    command_kind: String,
    command_json: String,
    state: String,
    plan_digest: String,
    target_digest: String,
    undo_policy: String,
    parent_batch_id: Option<String>,
    relation: Option<String>,
    plan_expires_at: Option<String>,
    cancel_requested_at: Option<String>,
    created_at: String,
    started_at: Option<String>,
    finished_at: Option<String>,
    updated_at: String,
    cost_preview_json: String,
}

fn load_batch(connection: &Connection, batch_id: &str) -> LibraryActResult<BatchRow> {
    let sql = format!("SELECT {BATCH_COLUMNS} FROM library_batches WHERE id = ?1");
    let row = connection
        .query_row(&sql, params![batch_id], |row| {
            Ok(BatchRow {
                id: String::new(),
                command_kind: row.get(0)?,
                command_json: row.get(1)?,
                state: row.get(2)?,
                plan_digest: row.get(3)?,
                target_digest: row.get(4)?,
                undo_policy: row.get(5)?,
                parent_batch_id: row.get(6)?,
                relation: row.get(7)?,
                plan_expires_at: row.get(8)?,
                cancel_requested_at: row.get(9)?,
                created_at: row.get(10)?,
                started_at: row.get(11)?,
                finished_at: row.get(12)?,
                updated_at: row.get(13)?,
                cost_preview_json: row.get(14)?,
            })
        })
        .optional()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    row.map(|mut row| {
        row.id = batch_id.to_string();
        row
    })
    .ok_or_else(|| {
        LibraryActError::of(
            LibraryActErrorCode::BatchNotFound,
            "that batch is not in this workspace",
        )
    })
}

#[derive(Debug, Clone)]
struct ItemRow {
    id: String,
    ordinal: i64,
    target_kind: String,
    target_key: String,
    dedupe_key: Option<String>,
    paper_id: Option<String>,
    revision_id: Option<String>,
    precondition_digest: String,
    state: String,
    plan_json: String,
    compensation_json: Option<String>,
    error_code: Option<String>,
    error_summary: Option<String>,
    attempt_count: i64,
    started_at: Option<String>,
    finished_at: Option<String>,
    updated_at: String,
    source_item_id: Option<String>,
    outcome: Option<String>,
    prepared_job_handle: Option<String>,
}

/// 引擎要读回的列。`result_json` 整体只写不读（审计留档，可能带路径），
/// 但 `outcome` 一个字段要进投影，所以在 SQL 里就地取出来，不在 Rust 侧解析 blob。
const ITEM_COLUMNS: &str = "id, ordinal, target_kind, target_key, dedupe_key,
                            paper_id, revision_id, precondition_digest,
                            state, plan_json, compensation_json, error_code, error_summary,
                            attempt_count, started_at, finished_at, updated_at, source_item_id,
                            json_extract(result_json, '$.outcome'), prepared_job_handle";

fn item_row_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<ItemRow> {
    Ok(ItemRow {
        id: row.get(0)?,
        ordinal: row.get(1)?,
        target_kind: row.get(2)?,
        target_key: row.get(3)?,
        dedupe_key: row.get(4)?,
        paper_id: row.get(5)?,
        revision_id: row.get(6)?,
        precondition_digest: row.get(7)?,
        state: row.get(8)?,
        plan_json: row.get(9)?,
        compensation_json: row.get(10)?,
        error_code: row.get(11)?,
        error_summary: row.get(12)?,
        attempt_count: row.get(13)?,
        started_at: row.get(14)?,
        finished_at: row.get(15)?,
        updated_at: row.get(16)?,
        source_item_id: row.get(17)?,
        outcome: row.get(18)?,
        prepared_job_handle: row.get(19)?,
    })
}

fn load_items(
    connection: &Connection,
    batch_id: &str,
    states: Option<&[ItemState]>,
) -> LibraryActResult<Vec<ItemRow>> {
    let mut sql = format!("SELECT {ITEM_COLUMNS} FROM library_batch_items WHERE batch_id = ?1");
    let names: Vec<String> = states
        .unwrap_or_default()
        .iter()
        .map(|state| state.as_str().to_string())
        .collect();
    let mut bind: Vec<String> = vec![batch_id.to_string()];
    if !names.is_empty() {
        let placeholders: Vec<String> = names
            .iter()
            .enumerate()
            .map(|(index, _)| format!("?{}", index + 2))
            .collect();
        sql.push_str(&format!(" AND state IN ({})", placeholders.join(",")));
        bind.extend(names);
    }
    sql.push_str(" ORDER BY ordinal ASC");
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let items = statement
        .query_map(rusqlite::params_from_iter(bind.iter()), item_row_from)
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    Ok(items)
}

impl ItemRow {
    /// `retryable` 由落库的 error code 推导，前端不需要维护第二套映射。
    fn retryable(&self) -> bool {
        self.error_code
            .as_deref()
            .and_then(LibraryActErrorCode::parse)
            .is_some_and(LibraryActErrorCode::is_retryable)
    }

    /// 列上有 `CHECK (target_kind IN ('paper','source'))`，写入侧只有
    /// [`TargetKind::as_str`] 一个来源，所以这里不存在第三种可能。
    fn target_kind(&self) -> TargetKind {
        match self.target_kind.as_str() {
            "source" => TargetKind::Source,
            _ => TargetKind::Paper,
        }
    }
}

fn item_counts(connection: &Connection, batch_id: &str) -> LibraryActResult<ItemCounts> {
    let mut statement = connection
        .prepare(
            "SELECT state, COUNT(*) FROM library_batch_items WHERE batch_id = ?1 GROUP BY state",
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let rows = statement
        .query_map(params![batch_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let mut counts = ItemCounts::default();
    for row in rows {
        let (text, count) = row.map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        let state = ItemState::parse(&text).ok_or_else(|| {
            LibraryActError::unavailable(format!("unknown stored item state {text}"))
        })?;
        match state {
            ItemState::Planned => counts.planned += count,
            ItemState::Queued => counts.queued += count,
            ItemState::Running => counts.running += count,
            ItemState::Paused => counts.paused += count,
            ItemState::ActionRequired => counts.action_required += count,
            ItemState::InterruptedUnknown => counts.interrupted_unknown += count,
            ItemState::Succeeded => counts.succeeded += count,
            ItemState::Failed => counts.failed += count,
            ItemState::Skipped => counts.skipped += count,
            ItemState::Cancelled => counts.cancelled += count,
        }
    }
    Ok(counts)
}

/// §10.1 `read({kind:'batch'})`：单个批次的聚合投影。
pub(crate) fn batch_projection(
    connection: &Connection,
    batch_id: &str,
) -> LibraryActResult<BatchProjection> {
    let row = load_batch(connection, batch_id)?;
    let counts = item_counts(connection, batch_id)?;
    let summary = PlanSummary::load(&row.cost_preview_json);
    let state = BatchState::parse(&row.state).ok_or_else(|| {
        LibraryActError::unavailable(format!("unknown stored batch state {}", row.state))
    })?;
    let command_kind = parse_kind(&row.command_kind)?;
    // 根批次没有父链，helper 直接给出 None。
    let parent_command_kind = action_kind_of_ancestor(connection, row.parent_batch_id.clone())?;
    let terminal = matches!(
        state,
        BatchState::Completed
            | BatchState::CompletedWithErrors
            | BatchState::Failed
            | BatchState::Cancelled
    );
    let mut statement = connection
        .prepare(
            "SELECT id, state, created_at FROM library_batches
             WHERE parent_batch_id = ?1 ORDER BY created_at ASC, id ASC",
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let children = statement
        .query_map(params![batch_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let mut retry_summary = None;
    let mut compensation_summary = None;
    for (child_id, _, _) in children {
        let child = batch_projection(connection, &child_id)?;
        let summary = ChildSummary {
            batch_id: child.id.clone(),
            state: child.state,
            total_items: child.total_items,
            succeeded: child.counts.succeeded,
            failed: child.counts.failed,
            created_at: child.created_at.clone(),
        };
        match child.relation.as_deref() {
            Some("retry") => retry_summary = Some(summary),
            Some("compensation") => compensation_summary = Some(summary),
            _ => {}
        }
    }
    let undo = {
        let token: Option<(String, String)> = connection
            .query_row(
                "SELECT state, expires_at FROM library_undo_tokens
                 WHERE batch_id = ?1 ORDER BY created_at DESC LIMIT 1",
                params![batch_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        token.and_then(|(state_text, expires_at)| {
            UndoTokenState::parse(&state_text).map(|state| UndoSummary { state, expires_at })
        })
    };
    Ok(BatchProjection {
        protocol_version: crate::library_query::LIBRARY_PROTOCOL_VERSION,
        id: row.id,
        command_kind,
        parent_command_kind,
        parent_batch_id: row.parent_batch_id,
        relation: row.relation,
        state,
        plan_digest: row.plan_digest,
        target_digest: row.target_digest,
        undo_policy: UndoPolicy::parse(&row.undo_policy).ok_or_else(|| {
            LibraryActError::unavailable(format!("unknown stored undo policy {}", row.undo_policy))
        })?,
        total_items: counts.total(),
        counts,
        requirements: summary.requirements,
        is_cancelling: row.cancel_requested_at.is_some() && !terminal,
        plan_expires_at: row.plan_expires_at,
        created_at: row.created_at,
        started_at: row.started_at,
        finished_at: row.finished_at,
        updated_at: row.updated_at,
        retry_summary,
        compensation_summary,
        undo,
        cost_preview: summary.cost,
    })
}

/// §10.1 `read({kind:'batch_items'})`：批次内 Item 的 ordinal keyset 分页。
///
/// 批次内不需要签名游标：`batchId` 已经是不可变边界，`ordinal` 在其上全序。
pub(crate) fn batch_items_page(
    connection: &Connection,
    batch_id: &str,
    after_ordinal: Option<i64>,
    states: Option<&[ItemState]>,
    limit: Option<i64>,
) -> LibraryActResult<BatchItemsPage> {
    load_batch(connection, batch_id)?;
    let page_size = match limit {
        None => BATCH_ITEMS_DEFAULT_PAGE,
        Some(value) if (1..=BATCH_ITEMS_MAX_PAGE).contains(&value) => value,
        Some(value) => {
            return Err(LibraryActError::invalid(format!(
                "limit must be between 1 and {BATCH_ITEMS_MAX_PAGE}, default {BATCH_ITEMS_DEFAULT_PAGE}; got {value}"
            )))
        }
    };
    let mut sql = format!("SELECT {ITEM_COLUMNS} FROM library_batch_items WHERE batch_id = ?1");
    let mut bind: Vec<rusqlite::types::Value> =
        vec![rusqlite::types::Value::Text(batch_id.to_string())];
    if let Some(after) = after_ordinal {
        if after < 0 {
            return Err(LibraryActError::invalid("afterOrdinal cannot be negative"));
        }
        sql.push_str(" AND ordinal > ?2");
        bind.push(after.into());
    }
    let names: Vec<String> = states
        .unwrap_or_default()
        .iter()
        .map(|state| state.as_str().to_string())
        .collect();
    if !names.is_empty() {
        // 游标位点取决于绑定的位置偏移，所以只追加不重排。
        let start = bind.len() + 1;
        let placeholders: Vec<String> = names
            .iter()
            .enumerate()
            .map(|(index, _)| format!("?{}", start + index))
            .collect();
        sql.push_str(&format!(" AND state IN ({})", placeholders.join(",")));
        bind.extend(names.into_iter().map(rusqlite::types::Value::Text));
    }
    sql.push_str(" ORDER BY ordinal ASC LIMIT ");
    sql.push_str(&(page_size + 1).to_string());
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(bind.iter()), item_row_from)
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let has_more = rows.len() as i64 > page_size;
    let items: Vec<BatchItemProjection> = rows
        .into_iter()
        .take(page_size as usize)
        .map(|item| {
            // 先取需要 &self 的派生值，后面的字段是逐个搬出。
            let retryable = item.retryable();
            BatchItemProjection {
                id: item.id,
                ordinal: item.ordinal,
                target_key: item.target_key,
                paper_id: item.paper_id,
                revision_id: item.revision_id,
                state: ItemState::parse(&item.state).unwrap_or(ItemState::InterruptedUnknown),
                outcome: item.outcome,
                attempt_count: item.attempt_count,
                error_code: item
                    .error_code
                    .as_deref()
                    .and_then(LibraryActErrorCode::parse),
                error_summary: item.error_summary,
                retryable,
                started_at: item.started_at,
                finished_at: item.finished_at,
                updated_at: item.updated_at,
                source_item_id: item.source_item_id,
            }
        })
        .collect();
    let next_ordinal = if has_more {
        items.last().map(|item| item.ordinal)
    } else {
        None
    };
    Ok(BatchItemsPage {
        protocol_version: crate::library_query::LIBRARY_PROTOCOL_VERSION,
        batch_id: batch_id.to_string(),
        page_size,
        items,
        next_ordinal,
        has_more,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchItemsPage {
    pub protocol_version: u32,
    pub batch_id: String,
    pub page_size: i64,
    pub items: Vec<BatchItemProjection>,
    pub next_ordinal: Option<i64>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentBatchesPage {
    pub protocol_version: u32,
    pub page_size: i64,
    pub batches: Vec<BatchProjection>,
}

/// §6.1：任务中心的批次分组。读之前先 reconcile，这样重启后看到的是
/// Items 推导出来的态，而不是被杀掉时留下的 `running` 缓存。
pub(crate) fn recent_batches(
    connection: &Connection,
    limit: Option<i64>,
) -> LibraryActResult<RecentBatchesPage> {
    reconcile_interrupted(connection)?;
    let page_size = match limit {
        None => RECENT_BATCHES_DEFAULT,
        Some(value) if (1..=RECENT_BATCHES_MAX).contains(&value) => value,
        Some(value) => {
            return Err(LibraryActError::invalid(format!(
                "recent_batches limit must be between 1 and {RECENT_BATCHES_MAX}, got {value}"
            )));
        }
    };
    let ids: Vec<String> = {
        let mut statement = connection
            .prepare(
                "SELECT id FROM library_batches
                 ORDER BY updated_at DESC, id DESC
                 LIMIT ?1",
            )
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        let rows = statement
            .query_map(params![page_size], |row| row.get(0))
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?
    };
    let mut batches = Vec::with_capacity(ids.len());
    for id in ids {
        batches.push(batch_projection(connection, &id)?);
    }
    Ok(RecentBatchesPage {
        protocol_version: crate::library_query::LIBRARY_PROTOCOL_VERSION,
        page_size,
        batches,
    })
}

// ---------------------------------------------------------------------------
// 崩溃恢复
// ---------------------------------------------------------------------------

/// §5.2：Item state 是权威，`library_batches.state` 是缓存，启动必须能由 Items 修复。
///
/// 任何被持久化成 `running` 的 Item 都没有在进程里的执行者（同步执行不会留下这个
/// 中间态），所以它只能来自一次被杀掉的执行——按 `interrupted_unknown` 报，绝不
/// 吞成普通 failed。
pub(crate) fn reconcile_interrupted(connection: &Connection) -> LibraryActResult<usize> {
    let timestamp = now();
    let _ = expire_due_preparations(connection, &timestamp);
    let orphaned = connection
        .execute(
            "UPDATE library_batch_items
             SET state = 'interrupted_unknown',
                 error_code = 'interrupted_unknown',
                 error_summary = 'the app stopped while this item was running; check its real effect before retrying',
                 finished_at = COALESCE(finished_at, ?1),
                 updated_at = ?1
             WHERE state = 'running'
               AND id NOT IN (
                 SELECT batch_item_id FROM library_batch_job_links
                 WHERE consumer_state = 'active' AND job_id IS NOT NULL
               )",
            params![timestamp],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    let linked_jobs: Vec<String> = {
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT job_id FROM library_batch_job_links
                 WHERE consumer_state = 'active' AND job_id IS NOT NULL",
            )
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        rows
    };
    for job_id in linked_jobs {
        let _ = sync_linked_batch_items(connection, &job_id);
    }
    let batches: Vec<String> = {
        let mut statement = connection
            .prepare("SELECT DISTINCT batch_id FROM library_batch_items WHERE state != 'planned'")
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        rows
    };
    let mut repaired = 0;
    for batch_id in &batches {
        let counts = item_counts(connection, batch_id)?;
        let derived = aggregate_state(&counts, true);
        let stored: Option<String> = connection
            .query_row(
                "SELECT state FROM library_batches WHERE id = ?1",
                params![batch_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
        let Some(current) = stored else { continue };
        let current_state = BatchState::parse(&current).ok_or_else(|| {
            LibraryActError::unavailable(format!("unknown stored batch state {current}"))
        })?;
        if current_state == BatchState::Planned {
            // 从没 start 过的批次不能被修成 running 派生态。
            continue;
        }
        if current_state != derived {
            connection
                .execute(
                    "UPDATE library_batches SET state = ?2, updated_at = ?3 WHERE id = ?1",
                    params![batch_id, derived.as_str(), timestamp],
                )
                .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
            repaired += 1;
        }
    }
    if orphaned > 0 {
        repaired += 1;
    }
    // §7.3：过期 Token 显式落到 expired，UI 才能区分「用过」和「窗口已过」。
    let expired = connection
        .execute(
            "UPDATE library_undo_tokens SET state = 'expired' WHERE state = 'available' AND expires_at <= ?1",
            params![now()],
        )
        .map_err(|error| LibraryActError::unavailable(error.to_string()))?;
    if expired > 0 && repaired == 0 {
        repaired = 1;
    }
    Ok(repaired)
}

fn future_minutes(minutes: i64) -> String {
    (Utc::now() + Duration::minutes(minutes)).to_rfc3339()
}

/// canonical JSON：`serde_json` 的 Map 是 BTreeMap，键序稳定，可跨语言比对。
pub(crate) fn canonical_digest(value: &Value) -> String {
    let text = serde_json::to_string(value).unwrap_or_else(|_| String::new());
    sha256_hex(text.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::library_query::{LibraryQueryErrorCode, LibraryReadRequest, LibraryReadResult};
    use crate::v2_workspace::WorkspaceModule;
    use std::path::PathBuf;

    /// `act` 每次都自己开连接，所以断言必须重新读盘，不能复用写入时的那条连接。
    struct Fixture {
        root: tempfile::TempDir,
    }

    fn fixture() -> Fixture {
        let root = tempfile::tempdir().expect("workspace directory");
        WorkspaceModule::new()
            .open(root.path())
            .expect("initialize workspace");
        Fixture { root }
    }

    impl Fixture {
        fn path(&self) -> &Path {
            self.root.path()
        }

        fn act(&self, request: &LibraryActRequest) -> LibraryActResult<LibraryActResponse> {
            act(self.root.path(), request)
        }

        fn act_provider(
            &self,
            request: &LibraryActRequest,
            provider: &ProviderActContext,
        ) -> LibraryActResult<LibraryActResponse> {
            act_with_provider(self.root.path(), request, provider)
        }

        fn act_provider_ok(
            &self,
            request: &LibraryActRequest,
            provider: &ProviderActContext,
        ) -> LibraryActResponse {
            self.act_provider(request, provider)
                .expect("provider batch request")
        }

        fn act_ok(&self, request: &LibraryActRequest) -> LibraryActResponse {
            self.act(request).expect("batch request")
        }

        fn error(&self, request: &LibraryActRequest) -> LibraryActErrorCode {
            let error = self.act(request).expect_err("expected a typed error");
            error.code
        }

        fn connection(&self) -> Connection {
            db::open(self.db_path()).expect("open workspace database")
        }

        fn db_path(&self) -> PathBuf {
            self.root.path().join(".read-desktop/workspace.sqlite3")
        }

        fn read(&self, request: &LibraryReadRequest) -> LibraryReadResult {
            crate::library_query::read(self.path(), request).expect("library read")
        }

        fn tags_of(&self, paper_id: &str) -> Vec<String> {
            let connection = self.connection();
            current_tags(&connection, paper_id)
                .expect("tags")
                .unwrap_or_else(|| panic!("{paper_id} disappeared"))
        }

        fn revisions(&self) -> crate::library_workflow::LibraryRevisions {
            library_revisions(&self.connection()).expect("revisions")
        }
    }

    // ---------------------------------------------------------------------
    // 造数据
    // ---------------------------------------------------------------------

    fn names(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| value.to_string()).collect()
    }

    /// 只造 SQL 行：本模块关心状态机，不关心文件复制。
    fn seed_paper(connection: &Connection, label: &str, tags: &[&str]) -> String {
        seed_paper_in(connection, "Papers/Inbox", label, tags)
    }

    /// Move / Trash 要把成员摆在不同的根下，所以位置是参数而不是写死的 Inbox。
    fn seed_paper_in(
        connection: &Connection,
        directory: &str,
        label: &str,
        tags: &[&str],
    ) -> String {
        let collection_id = format!("coll-{}", directory.to_lowercase().replace('/', "-"));
        let name = directory.rsplit('/').next().unwrap_or(directory);
        connection
            .execute(
                "INSERT INTO collections(id, parent_id, name, relative_path, created_at, updated_at)
                 VALUES (?1, NULL, ?2, ?3, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')
                 ON CONFLICT(relative_path) DO NOTHING",
                params![collection_id, name, directory],
            )
            .expect("collection");
        let relative_path = format!("{directory}/{label}.pdf");
        let paper_id = format!("paper-{label}");
        connection
            .execute(
                "INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z', NULL)",
                params![paper_id, collection_id, format!("{label}.pdf"), relative_path],
            )
            .expect("paper");
        connection
            .execute(
                "INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                 VALUES (?1, ?2, ?3, 1024, 12, ?4, '2026-08-01T00:00:00Z')",
                params![
                    format!("rev-{label}"),
                    paper_id,
                    format!("{:0>64x}", label.len()),
                    relative_path
                ],
            )
            .expect("revision");
        connection
            .execute(
                "INSERT INTO paper_heads(paper_id, revision_id, updated_at) VALUES (?1, ?2, '2026-08-01T00:00:00Z')",
                params![paper_id, format!("rev-{label}")],
            )
            .expect("head");
        connection
            .execute(
                "INSERT INTO paper_metadata(revision_id, title, authors_json, publication_year, source, created_at)
                 VALUES (?1, ?2, '[]', 2026, 'user', '2026-08-01T00:00:00Z')",
                params![format!("rev-{label}"), label],
            )
            .expect("metadata");
        for tag in tags {
            attach_tag(connection, &paper_id, tag);
        }
        paper_id
    }

    fn attach_tag(connection: &Connection, paper_id: &str, tag: &str) {
        connection
            .execute(
                "INSERT INTO tags(id, name, created_at) VALUES (?1, ?2, '2026-08-01T00:00:00Z')
                 ON CONFLICT(name) DO NOTHING",
                params![uuid::Uuid::new_v4().to_string(), tag],
            )
            .expect("tag");
        // 名字可能已经由引擎写过（id 是 uuid），必须按名字取回真正的外键值。
        let tag_id: String = connection
            .query_row("SELECT id FROM tags WHERE name = ?1", params![tag], |row| {
                row.get(0)
            })
            .expect("tag id");
        connection
            .execute(
                "INSERT INTO paper_tags(paper_id, tag_id) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
                params![paper_id, tag_id],
            )
            .expect("paper tag");
    }

    /// 模拟「另一个写者在计划之后改了标签」：直接写库并推进 tags 域。
    fn retag(fixture: &Fixture, paper_id: &str, tags: &[&str]) {
        let connection = fixture.connection();
        connection
            .execute(
                "DELETE FROM paper_tags WHERE paper_id = ?1",
                params![paper_id],
            )
            .expect("clear tags");
        for tag in tags {
            attach_tag(&connection, paper_id, tag);
        }
        bump_library_revisions(&connection, &[LibraryDomain::Tags]).expect("bump");
    }

    fn seed_three(fixture: &Fixture) -> Vec<String> {
        let connection = fixture.connection();
        ["alpha", "beta", "gamma"]
            .into_iter()
            .map(|label| seed_paper(&connection, label, &["inbox"]))
            .collect()
    }

    /// 两项就够看出「一项失败、一项成功」，比三项少一次渲染。
    fn seed_two(fixture: &Fixture) -> Vec<String> {
        let connection = fixture.connection();
        ["alpha", "beta"]
            .into_iter()
            .map(|label| seed_paper(&connection, label, &["inbox"]))
            .collect()
    }

    /// 某个 Paper 当前 revision 的 sha：导出的命名规则看的就是它。
    fn sha_of(fixture: &Fixture, paper_id: &str) -> String {
        fixture
            .connection()
            .query_row(
                "SELECT r.sha256 FROM document_revisions r
                 JOIN paper_heads h ON h.revision_id = r.id WHERE r.paper_id = ?1",
                params![paper_id],
                |row| row.get(0),
            )
            .expect("revision sha")
    }

    // ---------------------------------------------------------------------
    // 造数据：事务外动作要看文件系统，所以这几个测试有真的 PDF
    // ---------------------------------------------------------------------

    fn write_pdf(root: &Path, relative_path: &str) {
        let path = crate::library_paths::join_workspace_relative(root, relative_path)
            .expect("workspace relative path");
        std::fs::create_dir_all(path.parent().expect("parent directory")).expect("directory");
        std::fs::write(&path, b"%PDF-1.4\n").expect("write pdf");
    }

    fn has_pdf(root: &Path, relative_path: &str) -> bool {
        file_on_disk(root, relative_path)
    }

    fn remove_pdf(root: &Path, relative_path: &str) {
        let path = crate::library_paths::join_workspace_relative(root, relative_path)
            .expect("workspace relative path");
        std::fs::remove_file(&path).expect("remove pdf");
    }

    /// 库里一行 + 磁盘上一个文件：Move / Trash 的验收要看两边都动了。
    fn seed_file_paper(fixture: &Fixture, directory: &str, label: &str) -> String {
        let paper_id = {
            let connection = fixture.connection();
            seed_paper_in(&connection, directory, label, &["inbox"])
        };
        write_pdf(fixture.path(), &format!("{directory}/{label}.pdf"));
        paper_id
    }

    /// 库记着的位置：`(relativePath, 是否已在回收站)`。
    fn location_of(fixture: &Fixture, paper_id: &str) -> (String, bool) {
        let connection = fixture.connection();
        let location = paper_location(&connection, paper_id)
            .expect("location")
            .expect("the paper is still there");
        (location.relative_path, location.deleted)
    }

    /// 挂一个 Brief 成果头：跨根 Move 会真的删掉它，撤销不回来。
    fn attach_brief(fixture: &Fixture, paper_id: &str) {
        let connection = fixture.connection();
        let revision_id: String = connection
            .query_row(
                "SELECT id FROM document_revisions WHERE paper_id = ?1",
                params![paper_id],
                |row| row.get(0),
            )
            .expect("revision");
        let artifact_id = uuid::Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO artifacts(id, paper_id, revision_id, kind, version, status, content_json, created_at)
                 VALUES (?1, ?2, ?3, 'brief', 1, 'ready', '{}', '2026-08-01T00:00:00Z')",
                params![artifact_id, paper_id, revision_id],
            )
            .expect("artifact");
        connection
            .execute(
                "INSERT INTO artifact_heads(paper_id, kind, object_key, artifact_id, updated_at)
                 VALUES (?1, 'brief', '', ?2, '2026-08-01T00:00:00Z')",
                params![paper_id, artifact_id],
            )
            .expect("artifact head");
    }

    fn brief_heads(fixture: &Fixture, paper_id: &str) -> i64 {
        let connection = fixture.connection();
        connection
            .query_row(
                "SELECT COUNT(*) FROM artifact_heads WHERE paper_id = ?1 AND kind = 'brief'",
                params![paper_id],
                |row| row.get(0),
            )
            .expect("count")
    }

    /// 逐项计划里冻结的事实（§7）：Move / Trash 的确认页全靠它。
    fn plan_json(fixture: &Fixture, batch_id: &str, ordinal: i64) -> Value {
        read_json(fixture, "plan_json", "a frozen plan", batch_id, ordinal)
    }

    /// Item 上记的效果摘要：撤销以执行后的真实事实为准，不是以计划里的预期为准。
    fn result_json(fixture: &Fixture, batch_id: &str, ordinal: i64) -> Value {
        read_json(
            fixture,
            "result_json",
            "a recorded effect",
            batch_id,
            ordinal,
        )
    }

    fn read_json(
        fixture: &Fixture,
        column: &str,
        what: &str,
        batch_id: &str,
        ordinal: i64,
    ) -> Value {
        let connection = fixture.connection();
        let text: Option<String> = connection
            .query_row(
                &format!(
                    "SELECT {column} FROM library_batch_items WHERE batch_id = ?1 AND ordinal = ?2"
                ),
                params![batch_id, ordinal],
                |row| row.get(0),
            )
            .expect("item row");
        serde_json::from_str(&text.expect(what)).expect("json column")
    }

    /// 回收站里还开着的那一条：`(原始路径, 回收站内路径)`。
    fn trash_of(fixture: &Fixture, paper_id: &str) -> (String, String) {
        let connection = fixture.connection();
        connection
            .query_row(
                "SELECT original_relative_path, trash_relative_path FROM trash_entries
                 WHERE paper_id = ?1 AND restored_at IS NULL",
                params![paper_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("an open trash entry")
    }

    /// 逐项状态，按 ordinal 升序——批次合同的权威就是这一列。
    fn item_states(fixture: &Fixture, batch_id: &str) -> Vec<ItemState> {
        items_page(fixture, batch_id, None, None, None)
            .items
            .iter()
            .map(|item| item.state)
            .collect()
    }

    /// 一个批次覆盖了哪些目标（按 ordinal 升序）。补偿 child 只该含真的动过的那些。
    fn target_keys(fixture: &Fixture, batch_id: &str) -> Vec<String> {
        items_page(fixture, batch_id, None, None, None)
            .items
            .into_iter()
            .map(|item| item.target_key)
            .collect()
    }

    fn item_failure(
        fixture: &Fixture,
        batch_id: &str,
        ordinal: i64,
    ) -> Option<LibraryActErrorCode> {
        items_page(fixture, batch_id, None, None, None)
            .items
            .into_iter()
            .find(|item| item.ordinal == ordinal)
            .and_then(|item| item.error_code)
    }

    // ---------------------------------------------------------------------
    // 请求构造
    // ---------------------------------------------------------------------

    fn protocol() -> u32 {
        crate::library_query::LIBRARY_PROTOCOL_VERSION
    }

    fn plan_explicit(
        key: &str,
        paper_ids: &[String],
        add: &[&str],
        remove: &[&str],
    ) -> LibraryActRequest {
        LibraryActRequest::PlanBatch {
            protocol_version: protocol(),
            idempotency_key: key.to_string(),
            command: BatchCommand::PatchTags {
                add: names(add),
                remove: names(remove),
            },
            target: BatchTarget::Explicit {
                paper_ids: paper_ids.to_vec(),
            },
        }
    }

    fn plan_move(key: &str, paper_ids: &[String], collection_path: &str) -> LibraryActRequest {
        LibraryActRequest::PlanBatch {
            protocol_version: protocol(),
            idempotency_key: key.to_string(),
            command: BatchCommand::Move {
                collection_path: collection_path.to_string(),
            },
            target: BatchTarget::Explicit {
                paper_ids: paper_ids.to_vec(),
            },
        }
    }

    fn plan_trash(key: &str, paper_ids: &[String]) -> LibraryActRequest {
        LibraryActRequest::PlanBatch {
            protocol_version: protocol(),
            idempotency_key: key.to_string(),
            command: BatchCommand::Trash,
            target: BatchTarget::Explicit {
                paper_ids: paper_ids.to_vec(),
            },
        }
    }

    /// 前端从 `hub_page` 拿到的成员快照：摘要 + 依赖向量。
    fn snapshot(fixture: &Fixture, filters: &[QueryFilter]) -> (String, Vec<DomainRevision>) {
        let connection = fixture.connection();
        let revisions = library_revisions(&connection).expect("revisions");
        (
            selection_digest(filters),
            revisions.domain_revisions(&selection_dependencies(filters)),
        )
    }

    fn collection_filter() -> QueryFilter {
        QueryFilter::Collection {
            path: "Papers/Inbox".to_string(),
            recursive: false,
        }
    }

    fn plan_query(
        key: &str,
        filters: &[QueryFilter],
        excluded_ids: &[String],
        digest: &str,
        vector: &[DomainRevision],
    ) -> LibraryActRequest {
        LibraryActRequest::PlanBatch {
            protocol_version: protocol(),
            idempotency_key: key.to_string(),
            command: BatchCommand::PatchTags {
                add: names(&["reviewed"]),
                remove: vec![],
            },
            target: BatchTarget::Query {
                filters: filters.to_vec(),
                excluded_ids: excluded_ids.to_vec(),
                selection_digest: digest.to_string(),
                dependency_revisions: vector.to_vec(),
                evaluation_anchor: None,
                evaluation_timezone: None,
            },
        }
    }

    fn start(key: &str, batch: &BatchProjection, accepted: &[&str]) -> LibraryActRequest {
        start_with_ceilings(key, batch, accepted, BTreeMap::new())
    }

    fn start_with_ceilings(
        key: &str,
        batch: &BatchProjection,
        accepted: &[&str],
        ceilings: BTreeMap<String, String>,
    ) -> LibraryActRequest {
        LibraryActRequest::StartBatch {
            protocol_version: protocol(),
            idempotency_key: key.to_string(),
            batch_id: batch.id.clone(),
            plan_digest: batch.plan_digest.clone(),
            accepted_requirement_ids: names(accepted),
            maximum_accepted_estimate_by_currency: ceilings,
        }
    }

    fn plan_ocr(key: &str, paper_ids: &[String]) -> LibraryActRequest {
        LibraryActRequest::PlanBatch {
            protocol_version: protocol(),
            idempotency_key: key.to_string(),
            command: BatchCommand::Ocr,
            target: BatchTarget::Explicit {
                paper_ids: paper_ids.to_vec(),
            },
        }
    }

    fn plan_brief(key: &str, paper_ids: &[String]) -> LibraryActRequest {
        LibraryActRequest::PlanBatch {
            protocol_version: protocol(),
            idempotency_key: key.to_string(),
            command: BatchCommand::Brief,
            target: BatchTarget::Explicit {
                paper_ids: paper_ids.to_vec(),
            },
        }
    }

    fn ocr_provider() -> ProviderActContext {
        ProviderActContext {
            ocr: Some(
                crate::provider_routing::capture_mistral_ocr_route(
                    "test-mistral-key",
                    "mistral-ocr-latest",
                )
                .expect("ocr route"),
            ),
            paper: None,
            orientation_prompt: None,
            ..Default::default()
        }
    }

    #[test]
    fn brief_batch_freezes_the_matching_document_kind_and_single_output_protocol() {
        let provider = paper_provider();
        for (kind, expected, root) in [
            ("paper", "batch orientation", "paper source root"),
            ("textbook", "batch textbook brief", "textbook source root"),
        ] {
            let facts = ProviderItemFacts {
                paper_id: "p".into(),
                revision_id: "r".into(),
                page_count: Some(4),
                has_ocr: false,
                has_brief: false,
                document_kind: kind.into(),
            };
            let job = facts.job_spec(ProviderWorkKind::Brief, &provider);
            assert_eq!(job.payload["documentArtifactProtocol"], "v1");
            assert_eq!(job.payload["documentArtifactKind"], "brief");
            assert_eq!(job.payload["prompts"]["orientation"], expected);
            assert_eq!(job.payload["prompts"]["paperRoot"], root);
            assert_eq!(job.artifact_key.as_deref(), Some("brief"));
        }
    }

    #[test]
    fn textbook_brief_batch_freezes_new_contract_without_changing_paper_jobs() {
        let mut provider = paper_provider();
        provider.textbook_brief_protocol = Some(crate::textbook_contract::BRIEF_PROTOCOL.into());
        for kind in ["paper", "textbook"] {
            let facts = ProviderItemFacts {
                paper_id: "p".into(),
                revision_id: "r".into(),
                page_count: Some(3),
                has_ocr: false,
                has_brief: false,
                document_kind: kind.into(),
            };
            let spec = facts.job_spec(ProviderWorkKind::Brief, &provider);
            let fields =
                &spec.payload["responseSchema"]["schema"]["properties"]["brief"]["properties"];
            assert_eq!(fields.get("learningScope").is_some(), kind == "textbook");
            assert_eq!(
                spec.payload["briefProtocol"].as_str(),
                if kind == "textbook" {
                    Some(crate::textbook_contract::BRIEF_PROTOCOL)
                } else {
                    None
                }
            );
        }
    }

    fn paper_provider() -> ProviderActContext {
        use crate::model_settings::{
            paper_probe_fingerprint, PaperProbeRecord, ProviderInstance, ProviderKind,
            StoredModelSettings, MODEL_SETTINGS_SCHEMA,
        };
        use crate::provider_routing::{
            FrozenModels, ModelRole, ProviderCredentialPort, ProviderInstanceId,
            ProviderRouteDecision, ProviderRouting,
        };
        struct RouteCredential {
            id: String,
            key: String,
        }
        impl ProviderCredentialPort for RouteCredential {
            fn read_exact(
                &self,
                instance_id: &ProviderInstanceId,
            ) -> Result<Option<String>, crate::provider_routing::ProviderRoutingError> {
                Ok((instance_id.as_str() == self.id).then(|| self.key.clone()))
            }
        }
        let paper_model = "gpt-4.1".to_string();
        let translation_model = "gpt-4.1-mini".to_string();
        let id = "99999999-9999-4999-8999-999999999999";
        let key = "test-paper-key";
        let base_url = "https://test.example.com/v1";
        let instance = ProviderInstance {
            id: id.to_string(),
            name: "Test account".to_string(),
            kind: ProviderKind::OpenaiCompatible,
            base_url: Some(base_url.to_string()),
            paper_model: paper_model.clone(),
            translation_model: translation_model.clone(),
            models: Vec::<crate::GeminiModelOption>::new(),
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
        let paper = match ProviderRouting::new(&settings, &credentials)
            .capture(
                crate::provider_routing::ProviderSelection::Current,
                FrozenModels::new(paper_model, Some(translation_model)).expect("models"),
                ModelRole::Paper,
            )
            .expect("capture")
        {
            ProviderRouteDecision::Ready(bound) => bound.freeze(),
            ProviderRouteDecision::ActionRequired(requirement) => {
                panic!("route unexpectedly unavailable: {requirement:?}")
            }
        };
        ProviderActContext {
            ocr: None,
            paper: Some(paper),
            orientation_prompt: Some("batch orientation".to_string()),
            textbook_orientation_prompt: Some("batch textbook brief".to_string()),
            textbook_brief_protocol: None,
            paper_root_prompt: Some("paper source root".to_string()),
            textbook_root_prompt: Some("textbook source root".to_string()),
        }
    }

    fn attach_ocr(fixture: &Fixture, paper_id: &str) {
        let connection = fixture.connection();
        let revision_id: String = connection
            .query_row(
                "SELECT id FROM document_revisions WHERE paper_id = ?1",
                params![paper_id],
                |row| row.get(0),
            )
            .expect("revision");
        connection
            .execute(
                "INSERT INTO ocr_revisions(id, revision_id, status, provider, model, created_at)
                 VALUES (?1, ?2, 'ready', 'mistral', 'mistral-ocr-latest', '2026-08-01T00:00:00Z')",
                params![uuid::Uuid::new_v4().to_string(), revision_id],
            )
            .expect("ocr");
    }

    fn set_page_count(fixture: &Fixture, paper_id: &str, pages: i64) {
        fixture
            .connection()
            .execute(
                "UPDATE document_revisions SET page_count = ?2 WHERE paper_id = ?1",
                params![paper_id, pages],
            )
            .expect("page count");
    }

    fn jobs_count(fixture: &Fixture) -> i64 {
        fixture
            .connection()
            .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get(0))
            .expect("jobs")
    }

    fn accepted_ids(batch: &BatchProjection) -> Vec<String> {
        batch
            .requirements
            .iter()
            .map(|item| item.id.clone())
            .collect()
    }

    fn start_provider(
        fixture: &Fixture,
        key: &str,
        batch: &BatchProjection,
        provider: &ProviderActContext,
    ) -> LibraryActResponse {
        let accepted = accepted_ids(batch);
        let accepted_refs: Vec<&str> = accepted.iter().map(String::as_str).collect();
        let mut ceilings = BTreeMap::new();
        for estimate in &batch.cost_preview.marginal_estimates {
            ceilings.insert(estimate.currency.clone(), estimate.maximum.clone());
        }
        fixture.act_provider_ok(
            &start_with_ceilings(key, batch, &accepted_refs, ceilings),
            provider,
        )
    }

    fn control(key: &str, batch_id: &str, control: BatchControl) -> LibraryActRequest {
        LibraryActRequest::ControlBatch {
            protocol_version: protocol(),
            idempotency_key: key.to_string(),
            batch_id: batch_id.to_string(),
            control,
        }
    }

    fn retry_of(item_ids: Option<&[String]>) -> BatchControl {
        BatchControl::RetryFailed {
            item_ids: item_ids.map(<[String]>::to_vec),
        }
    }

    fn undo_with(token: &str) -> BatchControl {
        BatchControl::Undo {
            token: token.to_string(),
        }
    }

    fn undo_of(response: &LibraryActResponse) -> Option<String> {
        match response {
            LibraryActResponse::StartBatch { undo_token, .. }
            | LibraryActResponse::ControlBatch { undo_token, .. } => undo_token.clone(),
            LibraryActResponse::PlanBatch { .. }
            | LibraryActResponse::Change { .. }
            | LibraryActResponse::RecordReaderActivity { .. } => None,
        }
    }

    /// 计划 + 启动：返回最终投影与那次写入换来的 Undo Token。
    fn run(
        fixture: &Fixture,
        key: &str,
        paper_ids: &[String],
        add: &[&str],
    ) -> (BatchProjection, String) {
        let planned = fixture.act_ok(&plan_explicit(&format!("{key}-plan"), paper_ids, add, &[]));
        let batch = planned.batch().clone();
        let started = fixture.act_ok(&start(&format!("{key}-start"), &batch, &[]));
        let token = undo_of(&started).expect("a batch that wrote something owes an undo token");
        (started.batch().clone(), token)
    }

    /// Start 必须逐字回报预览页的确认项清单，所以这里直接把 requirements 的 id 带上。
    fn start_planned(
        fixture: &Fixture,
        key: &str,
        planned: &BatchProjection,
    ) -> (BatchProjection, Option<String>) {
        let accepted: Vec<&str> = planned
            .requirements
            .iter()
            .map(|requirement| requirement.id.as_str())
            .collect();
        let started = fixture.act_ok(&start(&format!("{key}-start"), planned, &accepted));
        (started.batch().clone(), undo_of(&started))
    }

    fn planned_batch(fixture: &Fixture, request: &LibraryActRequest) -> BatchProjection {
        fixture.act_ok(request).batch().clone()
    }

    fn run_move(
        fixture: &Fixture,
        key: &str,
        paper_ids: &[String],
        collection_path: &str,
    ) -> (BatchProjection, Option<String>) {
        let planned = planned_batch(
            fixture,
            &plan_move(&format!("{key}-plan"), paper_ids, collection_path),
        );
        start_planned(fixture, key, &planned)
    }

    /// 撤销也是事务外执行：走同一条「认领 → 执行 → 记结果」的队列。
    fn undo(fixture: &Fixture, key: &str, batch_id: &str, token: &str) -> BatchProjection {
        fixture
            .act_ok(&control(&format!("{key}-undo"), batch_id, undo_with(token)))
            .batch()
            .clone()
    }

    fn items_page(
        fixture: &Fixture,
        batch_id: &str,
        after_ordinal: Option<i64>,
        states: Option<Vec<ItemState>>,
        limit: Option<i64>,
    ) -> BatchItemsPage {
        match fixture.read(&LibraryReadRequest::BatchItems {
            protocol_version: protocol(),
            batch_id: batch_id.to_string(),
            after_ordinal,
            states,
            limit,
        }) {
            LibraryReadResult::BatchItems { page } => page,
            other => panic!("unexpected read result {other:?}"),
        }
    }

    fn projection_of(fixture: &Fixture, batch_id: &str) -> BatchProjection {
        match fixture.read(&LibraryReadRequest::Batch {
            protocol_version: protocol(),
            batch_id: batch_id.to_string(),
        }) {
            LibraryReadResult::Batch { batch } => batch,
            other => panic!("unexpected read result {other:?}"),
        }
    }

    fn ordinals(page: &BatchItemsPage) -> Vec<i64> {
        page.items.iter().map(|item| item.ordinal).collect()
    }

    fn counts(build: impl FnOnce(&mut ItemCounts)) -> ItemCounts {
        let mut value = ItemCounts::default();
        build(&mut value);
        value
    }

    // ---------------------------------------------------------------------
    // §5.2 聚合真值表
    // ---------------------------------------------------------------------

    #[test]
    fn aggregate_state_follows_the_documented_priority() {
        // 没 start 过就没有「运行中」这一说，Item 全在 planned。
        assert_eq!(
            aggregate_state(&counts(|counts| counts.running = 1), false),
            BatchState::Planned
        );
        // running 优先：还有一项在跑，就不能把整批说成结束。
        assert_eq!(
            aggregate_state(
                &counts(|counts| {
                    counts.running = 1;
                    counts.succeeded = 4;
                    counts.failed = 4;
                }),
                true
            ),
            BatchState::Running
        );
        assert_eq!(
            aggregate_state(
                &counts(|counts| {
                    counts.queued = 1;
                    counts.interrupted_unknown = 3;
                }),
                true
            ),
            BatchState::Queued
        );
        assert_eq!(
            aggregate_state(
                &counts(|counts| {
                    counts.planned = 1;
                    counts.action_required = 2;
                }),
                true
            ),
            BatchState::Queued
        );
        // 效果未知排在需要处理之前：先说清「我们不知道」，再谈「等你决定」。
        assert_eq!(
            aggregate_state(
                &counts(|counts| {
                    counts.interrupted_unknown = 1;
                    counts.action_required = 1;
                    counts.paused = 1;
                }),
                true
            ),
            BatchState::InterruptedUnknown
        );
        assert_eq!(
            aggregate_state(
                &counts(|counts| {
                    counts.action_required = 1;
                    counts.paused = 2;
                }),
                true
            ),
            BatchState::ActionRequired
        );
        assert_eq!(
            aggregate_state(
                &counts(|counts| {
                    counts.paused = 1;
                    counts.succeeded = 5;
                }),
                true
            ),
            BatchState::Paused
        );
        assert_eq!(
            aggregate_state(&counts(|counts| counts.cancelled = 3), true),
            BatchState::Cancelled
        );
        assert_eq!(
            aggregate_state(&counts(|counts| counts.failed = 3), true),
            BatchState::Failed
        );
        assert_eq!(
            aggregate_state(
                &counts(|counts| {
                    counts.succeeded = 2;
                    counts.failed = 1;
                }),
                true
            ),
            BatchState::CompletedWithErrors
        );
        assert_eq!(
            aggregate_state(
                &counts(|counts| {
                    counts.succeeded = 2;
                    counts.cancelled = 1;
                }),
                true
            ),
            BatchState::CompletedWithErrors
        );
        assert_eq!(
            aggregate_state(
                &counts(|counts| {
                    counts.succeeded = 2;
                    counts.skipped = 1;
                }),
                true
            ),
            BatchState::Completed
        );
    }

    // ---------------------------------------------------------------------
    // plan / start
    // ---------------------------------------------------------------------

    #[test]
    fn planning_freezes_items_without_writing_tags_or_bumping_revisions() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let before = fixture.revisions();
        let response = fixture.act_ok(&plan_explicit("plan-1", &papers, &["reviewed"], &[]));
        let batch = response.batch();
        assert_eq!(batch.state, BatchState::Planned);
        assert_eq!(batch.command_kind, BatchCommandKind::PatchTags);
        assert_eq!(batch.parent_batch_id, None);
        assert_eq!(batch.total_items, 3);
        assert_eq!(batch.counts.planned, 3);
        // 标签补丁的 before-image 是完整的，所以撤销是 `full`；只有会丢效果的
        // 动作（跨根 Move、Trash）才降级成 `compensating`。
        assert_eq!(batch.undo_policy, UndoPolicy::Full);
        assert!(batch.requirements.is_empty());
        assert!(batch.plan_expires_at.is_some());
        assert_eq!(batch.started_at, None);
        assert_eq!(batch.finished_at, None);
        // §5.2：预览阶段一个字节都不该落到业务表上。
        for paper in &papers {
            assert_eq!(fixture.tags_of(paper), names(&["inbox"]));
        }
        let after = fixture.revisions();
        assert_eq!(after.global, before.global);
        assert_eq!(
            after.domain(LibraryDomain::Tags),
            before.domain(LibraryDomain::Tags)
        );
        assert_eq!(batch.undo, None);
        // 响应与 `library_read` 的投影是同一份真相。
        assert_eq!(&projection_of(&fixture, &batch.id), batch);
    }

    #[test]
    fn change_and_reader_activity_write_receipts_without_a_batch_id() {
        let fixture = fixture();
        let paper_id = seed_paper(&fixture.connection(), "alpha", &["inbox"]);
        let favorite = fixture.act_ok(&LibraryActRequest::Change {
            protocol_version: protocol(),
            idempotency_key: "fav-1".to_string(),
            change: LibraryChange::PatchLifecycle {
                paper_id: paper_id.clone(),
                expected_version: 0,
                patch: crate::library_lifecycle::LifecyclePatch {
                    favorite: Some(true),
                    ..Default::default()
                },
            },
        });
        match favorite {
            LibraryActResponse::Change {
                result: ChangeResult::Lifecycle { lifecycle },
            } => assert!(lifecycle.favorite),
            other => panic!("expected lifecycle change, got {other:?}"),
        }

        let opened = fixture.act_ok(&LibraryActRequest::RecordReaderActivity {
            protocol_version: protocol(),
            idempotency_key: "open-1".to_string(),
            activity: crate::library_lifecycle::ReaderActivity {
                paper_id: paper_id.clone(),
                revision_id: "rev-alpha".to_string(),
                page_number: 1,
                page_count: 12,
            },
        });
        assert!(matches!(
            opened,
            LibraryActResponse::RecordReaderActivity { .. }
        ));

        let saved = fixture.act_ok(&LibraryActRequest::Change {
            protocol_version: protocol(),
            idempotency_key: "smart-1".to_string(),
            change: LibraryChange::CreateSmartCollection {
                name: "稍后".to_string(),
                filters: vec![QueryFilter::Status {
                    value: "unread".to_string(),
                }],
            },
        });
        assert!(matches!(
            saved,
            LibraryActResponse::Change {
                result: ChangeResult::SmartCollection { .. }
            }
        ));

        let receipts: i64 = fixture
            .connection()
            .query_row("SELECT COUNT(*) FROM library_action_receipts", [], |row| {
                row.get(0)
            })
            .expect("receipts");
        assert_eq!(receipts, 3);

        let replay = fixture.act_ok(&LibraryActRequest::Change {
            protocol_version: protocol(),
            idempotency_key: "fav-1".to_string(),
            change: LibraryChange::PatchLifecycle {
                paper_id,
                expected_version: 0,
                patch: crate::library_lifecycle::LifecyclePatch {
                    favorite: Some(true),
                    ..Default::default()
                },
            },
        });
        assert!(matches!(replay, LibraryActResponse::Change { .. }));
    }

    #[test]
    fn starting_applies_the_patch_once_and_mints_exactly_one_undo_token() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let before = fixture.revisions();
        let (batch, token) = run(&fixture, "start", &papers, &["reviewed"]);
        assert_eq!(batch.state, BatchState::Completed);
        assert_eq!(batch.counts.succeeded, 3);
        assert!(batch.started_at.is_some());
        assert!(batch.finished_at.is_some());
        for paper in &papers {
            assert_eq!(fixture.tags_of(paper), names(&["inbox", "reviewed"]));
        }
        let after = fixture.revisions();
        // 整批只推进一次 tags/artifacts；不相关的域保持不动。
        assert_eq!(
            after.domain(LibraryDomain::Tags),
            before.domain(LibraryDomain::Tags) + 1
        );
        assert_eq!(
            after.domain(LibraryDomain::Artifacts),
            before.domain(LibraryDomain::Artifacts) + 1
        );
        assert_eq!(
            after.domain(LibraryDomain::Sort),
            before.domain(LibraryDomain::Sort)
        );
        let connection = fixture.connection();
        let tokens: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM library_undo_tokens WHERE batch_id = ?1",
                params![batch.id],
                |row| row.get(0),
            )
            .expect("token count");
        assert_eq!(tokens, 1);
        // §7.3：库里只有 hash，明文只出现在这一次响应里。
        let stored_hash: String = connection
            .query_row(
                "SELECT token_hash FROM library_undo_tokens WHERE batch_id = ?1",
                params![batch.id],
                |row| row.get(0),
            )
            .expect("token hash");
        assert_ne!(stored_hash, token);
        assert_eq!(stored_hash, sha256_hex(token.as_bytes()));
        assert_eq!(
            batch.undo.as_ref().map(|undo| undo.state),
            Some(UndoTokenState::Available)
        );

        // Gate：换一个 key 再 start 也不会重跑，因此不再发 token、也不再推进 revision。
        let again = fixture.act_ok(&start("start-again", &batch, &[]));
        assert_eq!(again.batch().state, BatchState::Completed);
        assert_eq!(undo_of(&again), None);
        let reread = fixture.revisions();
        assert_eq!(
            reread.domain(LibraryDomain::Tags),
            after.domain(LibraryDomain::Tags)
        );
        // 重跑会把 attempt_count 变成 2，所以这里必须是 1。
        assert!(items_page(&fixture, &batch.id, None, None, None)
            .items
            .iter()
            .all(|item| item.attempt_count == 1));
    }

    #[test]
    fn the_tag_patch_is_applied_per_item_as_add_and_remove() {
        // 覆盖式写会把别人的标签抹掉；patch 只动补丁点名的那些。
        let fixture = fixture();
        let papers = seed_three(&fixture);
        retag(&fixture, &papers[1], &["Keep", "inbox", "stale"]);
        let planned = fixture
            .act_ok(&plan_explicit("patch-plan", &papers, &["reviewed"], &[]))
            .batch()
            .clone();
        // 重复名与两侧空白在 normalize 里合并成一个补丁项。
        let started = fixture.act_ok(&LibraryActRequest::StartBatch {
            protocol_version: protocol(),
            idempotency_key: "patch-start".to_string(),
            batch_id: planned.id.clone(),
            plan_digest: planned.plan_digest.clone(),
            accepted_requirement_ids: vec![],
            maximum_accepted_estimate_by_currency: BTreeMap::new(),
        });
        assert_eq!(started.batch().state, BatchState::Completed);
        assert_eq!(fixture.tags_of(&papers[0]), names(&["inbox", "reviewed"]));
        // 大小写原样保留，别人新加的 `Keep` 没被抹掉；读侧按 lower(name) 排序。
        assert_eq!(
            fixture.tags_of(&papers[1]),
            names(&["inbox", "Keep", "reviewed", "stale"])
        );
        //  remove 只删点名的标签。
        let with_removal = fixture.act_ok(&plan_explicit("remove-plan", &papers, &[], &["STALE"]));
        let removed = fixture
            .act_ok(&start("remove-start", with_removal.batch(), &[]))
            .batch()
            .clone();
        assert_eq!(removed.state, BatchState::Completed);
        assert_eq!(
            fixture.tags_of(&papers[1]),
            names(&["inbox", "Keep", "reviewed"])
        );
        assert_eq!(fixture.tags_of(&papers[0]), names(&["inbox", "reviewed"]));
    }

    #[test]
    fn oversized_empty_and_unknown_targets_are_refused_before_anything_is_written() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let mut too_many = papers.clone();
        too_many.resize(MAX_BATCH_ITEMS + 1, papers[0].clone());
        assert_eq!(
            fixture.error(&plan_explicit("big", &too_many, &["x"], &[])),
            LibraryActErrorCode::BatchTooLarge
        );
        // 空补丁 / 空选择 / 不存在的 Paper 都停在计划阶段。
        assert_eq!(
            fixture.error(&plan_explicit("empty-patch", &papers, &[], &[])),
            LibraryActErrorCode::InvalidQuery
        );
        assert_eq!(
            fixture.error(&plan_explicit("blank-tag", &papers, &["   "], &[])),
            LibraryActErrorCode::InvalidQuery
        );
        assert_eq!(
            fixture.error(&plan_explicit("no-members", &[], &["x"], &[])),
            LibraryActErrorCode::SelectionEmpty
        );
        assert_eq!(
            fixture.error(&plan_explicit(
                "ghost",
                &["paper-ghost".to_string()],
                &["x"],
                &[]
            )),
            LibraryActErrorCode::PaperNotFound
        );
        let connection = fixture.connection();
        let batches: i64 = connection
            .query_row("SELECT COUNT(*) FROM library_batches", [], |row| row.get(0))
            .expect("batch count");
        assert_eq!(batches, 0);
        // 协议版本与幂等 key 的形状在打开库之前就检查。
        assert_eq!(
            fixture.error(&LibraryActRequest::PlanBatch {
                protocol_version: protocol() + 1,
                idempotency_key: "version".to_string(),
                command: BatchCommand::PatchTags {
                    add: names(&["x"]),
                    remove: vec![]
                },
                target: BatchTarget::Explicit {
                    paper_ids: papers.clone(),
                },
            }),
            LibraryActErrorCode::InvalidQuery
        );
    }

    #[test]
    fn a_selection_bigger_than_the_cap_is_refused_not_truncated() {
        let fixture = fixture();
        let seeded = {
            let connection = fixture.connection();
            (0..=MAX_BATCH_ITEMS)
                .map(|index| seed_paper(&connection, &format!("bulk-{index:04}"), &[]))
                .collect::<Vec<_>>()
        };
        let filters = vec![collection_filter()];
        let (digest, vector) = snapshot(&fixture, &filters);
        assert_eq!(
            fixture.error(&plan_query("cap", &filters, &[], &digest, &vector)),
            LibraryActErrorCode::BatchTooLarge
        );
        // 上限判断在成员集合上：排除一项救不回来，也不允许「先裁再跑」。
        assert_eq!(
            fixture.error(&plan_query(
                "cap-excluded",
                &filters,
                &seeded[0..1],
                &digest,
                &vector
            )),
            LibraryActErrorCode::BatchTooLarge
        );
    }

    // ---------------------------------------------------------------------
    // 幂等
    // ---------------------------------------------------------------------

    #[test]
    fn the_same_key_replays_the_batch_while_a_changed_request_conflicts() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let request = plan_explicit("dup-plan", &papers, &["reviewed"], &[]);
        let first = fixture.act_ok(&request);
        let replay = fixture.act_ok(&request);
        assert_eq!(first, replay);
        assert_eq!(replay.batch().counts.planned, 3);
        let connection = fixture.connection();
        let batches: i64 = connection
            .query_row("SELECT COUNT(*) FROM library_batches", [], |row| row.get(0))
            .expect("batch count");
        assert_eq!(batches, 1);
        // 同一个 key 换了命令：不能当成重放，否则 UI 拿到的是别人的批次。
        assert_eq!(
            fixture.error(&plan_explicit("dup-plan", &papers, &["different"], &[])),
            LibraryActErrorCode::IdempotencyConflict
        );
        assert_eq!(
            fixture.error(&plan_explicit("  ", &papers, &["x"], &[])),
            LibraryActErrorCode::InvalidQuery
        );
        assert_eq!(
            fixture.error(&plan_explicit(
                &"k".repeat(MAX_IDEMPOTENCY_KEY_CHARS + 1),
                &papers,
                &["x"],
                &[]
            )),
            LibraryActErrorCode::InvalidQuery
        );
        let batches: i64 = connection
            .query_row("SELECT COUNT(*) FROM library_batches", [], |row| row.get(0))
            .expect("batch count");
        assert_eq!(batches, 1);
    }

    #[test]
    fn a_replayed_start_never_runs_the_batch_again() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let planned = fixture
            .act_ok(&plan_explicit("replay-plan", &papers, &["reviewed"], &[]))
            .batch()
            .clone();
        let request = start("replay-start", &planned, &[]);
        let before = fixture.revisions();
        let first = fixture.act_ok(&request);
        let replay = fixture.act_ok(&request);
        assert_eq!(first, replay);
        assert_eq!(undo_of(&first), undo_of(&replay));
        let after = fixture.revisions();
        assert_eq!(
            after.domain(LibraryDomain::Tags),
            before.domain(LibraryDomain::Tags) + 1
        );
        // receipt 只在事务里写一次。
        let connection = fixture.connection();
        let receipts: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM library_action_receipts WHERE idempotency_key = ?1",
                params!["replay-start"],
                |row| row.get(0),
            )
            .expect("receipt count");
        assert_eq!(receipts, 1);
    }

    // ---------------------------------------------------------------------
    // 逐项隔离与重试
    // ---------------------------------------------------------------------

    #[test]
    fn one_stale_item_fails_without_rolling_back_its_siblings() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let planned = fixture
            .act_ok(&plan_explicit("mixed-plan", &papers, &["reviewed"], &[]))
            .batch()
            .clone();
        // 计划之后、执行之前，另一个人改了 beta 的标签。
        retag(&fixture, &papers[1], &["hand-labelled", "inbox"]);
        let started = fixture.act_ok(&start("mixed-start", &planned, &[]));
        let batch = started.batch();
        assert_eq!(batch.state, BatchState::CompletedWithErrors);
        assert_eq!(batch.counts.succeeded, 2);
        assert_eq!(batch.counts.failed, 1);
        // 每个 Item 一个 SAVEPOINT：兄弟项的结果留下来。
        assert_eq!(fixture.tags_of(&papers[0]), names(&["inbox", "reviewed"]));
        assert_eq!(fixture.tags_of(&papers[2]), names(&["inbox", "reviewed"]));
        // 冲突项自己保持计划前的值——既没有被覆盖，也没有被部分应用。
        assert_eq!(
            fixture.tags_of(&papers[1]),
            names(&["hand-labelled", "inbox"])
        );
        let page = items_page(&fixture, &batch.id, None, None, None);
        let failed = page
            .items
            .iter()
            .find(|item| item.state == ItemState::Failed)
            .expect("one failed item");
        assert_eq!(failed.ordinal, 1);
        assert_eq!(failed.paper_id.as_deref(), Some(papers[1].as_str()));
        assert_eq!(failed.error_code, Some(LibraryActErrorCode::StaleSelection));
        assert!(failed.retryable);
        assert_eq!(failed.attempt_count, 1);
        assert!(failed.finished_at.is_some());
        // §10.2：单项摘要只说业务，底层文本没有直通通道。
        let summary = failed.error_summary.clone().expect("a safe summary");
        assert!(!summary.contains("SQLITE"));
        assert!(!summary.contains("library_batch_items"));
        assert!(!summary.contains("near "));
        // 成功项没有错误字段。
        assert!(page
            .items
            .iter()
            .filter(|item| item.state == ItemState::Succeeded)
            .all(|item| item.error_code.is_none() && item.error_summary.is_none()));
    }

    #[test]
    fn a_batch_where_every_item_fails_writes_nothing_and_mints_no_token() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let planned = fixture
            .act_ok(&plan_explicit("all-fail-plan", &papers, &["reviewed"], &[]))
            .batch()
            .clone();
        for paper in &papers {
            retag(&fixture, paper, &["inbox", "late"]);
        }
        let before = fixture.revisions();
        let started = fixture.act_ok(&start("all-fail-start", &planned, &[]));
        assert_eq!(started.batch().state, BatchState::Failed);
        assert_eq!(started.batch().counts.failed, 3);
        assert_eq!(undo_of(&started), None);
        assert_eq!(started.batch().undo, None);
        // 一次真实写入都没有 → 不能惊动任何缓存。
        let after = fixture.revisions();
        assert_eq!(after.global, before.global);
        for paper in &papers {
            assert_eq!(fixture.tags_of(paper), names(&["inbox", "late"]));
        }
    }

    #[test]
    fn retry_copies_only_the_failed_item_into_a_child_batch() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let planned = fixture
            .act_ok(&plan_explicit("retry-plan", &papers, &["reviewed"], &[]))
            .batch()
            .clone();
        retag(&fixture, &papers[1], &["hand-labelled", "inbox"]);
        let parent = fixture
            .act_ok(&start("retry-start", &planned, &[]))
            .batch()
            .clone();
        let failed_id = items_page(
            &fixture,
            &parent.id,
            None,
            Some(vec![ItemState::Failed]),
            None,
        )
        .items[0]
            .id
            .clone();
        let child = fixture
            .act_ok(&control("retry-1", &parent.id, retry_of(None)))
            .batch()
            .clone();
        assert_eq!(child.command_kind, BatchCommandKind::Retry);
        assert_eq!(child.parent_batch_id.as_deref(), Some(parent.id.as_str()));
        assert_eq!(child.relation.as_deref(), Some("retry"));
        assert_eq!(child.parent_command_kind, Some(BatchCommandKind::PatchTags));
        assert_eq!(child.state, BatchState::Completed);
        assert_eq!(child.total_items, 1);
        assert_eq!(
            fixture.tags_of(&papers[1]),
            names(&["hand-labelled", "inbox", "reviewed"])
        );
        // child Item 指向父 Item，任务中心才能讲清「这是哪一项的第二次尝试」。
        let page = items_page(&fixture, &child.id, None, None, None);
        assert_eq!(
            page.items[0].source_item_id.as_deref(),
            Some(failed_id.as_str())
        );
        assert_eq!(page.items[0].attempt_count, 1);
        // 父记录不被改写，只挂上 child 摘要。
        let reread = projection_of(&fixture, &parent.id);
        assert_eq!(reread.state, BatchState::CompletedWithErrors);
        assert_eq!(reread.counts.succeeded, 2);
        assert_eq!(reread.counts.failed, 1);
        assert_eq!(reread.finished_at, parent.finished_at);
        assert_eq!(
            reread.retry_summary.map(|summary| summary.batch_id),
            Some(child.id.clone())
        );
        // 全员成功的批次没有可重试的东西；重试一个已经完成的 child 亦然。
        assert_eq!(
            fixture.error(&control("retry-2", &child.id, retry_of(None))),
            LibraryActErrorCode::UnsupportedForScope
        );
    }

    #[test]
    fn a_compensation_batch_stays_closed_to_retry() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let (batch, token) = run(&fixture, "comp", &papers[0..2], &["reviewed"]);
        // 让撤销的第二项撞上「之后又被人改过」。
        retag(&fixture, &papers[1], &["extra", "inbox", "reviewed"]);
        let child = fixture
            .act_ok(&control("comp-undo", &batch.id, undo_with(&token)))
            .batch()
            .clone();
        assert_eq!(child.state, BatchState::CompletedWithErrors);
        // 部分失败可以重试的是正向动作；补偿批次的失败项需要一次新的决定。
        assert_eq!(
            fixture.error(&control("comp-retry", &child.id, retry_of(None))),
            LibraryActErrorCode::UnsupportedForScope
        );
    }

    /// 造一条 `retry` 父链：`hops` 是 child 层数，根批次始终是 patch_tags。
    fn seed_lineage(connection: &Connection, hops: usize) -> Vec<String> {
        let root_command = json!({ "kind": "patch_tags", "add": ["reviewed"], "remove": [] });
        insert_batch(
            connection,
            "lineage-0",
            BatchCommandKind::PatchTags,
            &root_command.to_string(),
            "plan-0",
            "target-0",
            UndoPolicy::Compensating,
            &PlanSummary::local(Vec::new()),
            None,
        )
        .expect("root batch");
        let mut chain = vec!["lineage-0".to_string()];
        let mut parent_command = root_command;
        for index in 1..=hops {
            // 与 retry_batch 一样：封套里放剥到最内层的那个命令。
            let command = json!({
                "kind": "retry",
                "command": effective_command(&parent_command.to_string()),
            });
            let id = format!("lineage-{index}");
            insert_batch(
                connection,
                &id,
                BatchCommandKind::Retry,
                &command.to_string(),
                &format!("plan-{index}"),
                &format!("target-{index}"),
                UndoPolicy::Compensating,
                &PlanSummary::local(Vec::new()),
                Some((chain[index - 1].as_str(), "retry")),
            )
            .expect("child batch");
            chain.push(id);
            parent_command = command;
        }
        chain
    }

    #[test]
    fn a_retry_lineage_still_resolves_the_root_command() {
        let fixture = fixture();
        let connection = fixture.connection();
        let chain = seed_lineage(&connection, MAX_LINEAGE_HOPS);
        for (index, id) in chain.iter().enumerate().skip(1) {
            // 只看直接父批次会解出 `retry`，策略就没了——「重试的重试」曾经正是在这里失败。
            assert_eq!(
                action_kind_for(&connection, id).expect("lineage"),
                Some(BatchCommandKind::PatchTags),
                "{id}"
            );
            assert_eq!(
                stored_patch(&connection, id).expect("payload"),
                Some((names(&["reviewed"]), vec![])),
                "{id}"
            );
            // 任务中心要能投影链条上的每一个批次：解不出策略就等于记录消失。
            assert_eq!(
                batch_projection(&connection, id)
                    .expect("projection")
                    .parent_command_kind,
                Some(BatchCommandKind::PatchTags),
                "{id}"
            );
            assert_eq!(lineage_depth(&connection, id).expect("depth"), index + 1);
        }
    }

    #[test]
    fn an_over_long_lineage_fails_closed_instead_of_guessing_a_strategy() {
        let fixture = fixture();
        let connection = fixture.connection();
        // 越过上限的那一跳只能来自坏数据：retry_batch 会在创建 child 之前拒绝。
        let chain = seed_lineage(&connection, MAX_LINEAGE_HOPS + 1);
        let deepest = chain.last().expect("deepest");
        assert_eq!(
            action_kind_for(&connection, deepest)
                .expect_err("too deep")
                .code,
            LibraryActErrorCode::UnsupportedForScope
        );
        assert_eq!(
            lineage_depth(&connection, deepest)
                .expect_err("too deep")
                .code,
            LibraryActErrorCode::UnsupportedForScope
        );
        // 上限处的批次自己仍然可解，只是再挂一个 child 就会越界。
        let at_limit = &chain[chain.len() - 2];
        assert_eq!(
            lineage_depth(&connection, at_limit).expect("depth"),
            MAX_LINEAGE_HOPS + 1
        );
        assert_eq!(
            action_kind_for(&connection, at_limit).expect("kind"),
            Some(BatchCommandKind::PatchTags)
        );
    }

    // ---------------------------------------------------------------------
    // §7.1 取消
    // ---------------------------------------------------------------------

    #[test]
    fn cancel_stops_pending_items_and_never_rewrites_a_finished_one() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let planned = fixture
            .act_ok(&plan_explicit("cancel-plan", &papers, &["reviewed"], &[]))
            .batch()
            .clone();
        let cancelled = fixture
            .act_ok(&control(
                "cancel-1",
                &planned.id,
                BatchControl::CancelRemaining,
            ))
            .batch()
            .clone();
        assert_eq!(cancelled.state, BatchState::Cancelled);
        assert_eq!(cancelled.counts.cancelled, 3);
        // 取消不是撤销：没有 token，也没有写任何东西。
        assert_eq!(cancelled.undo, None);
        for paper in &papers {
            assert_eq!(fixture.tags_of(paper), names(&["inbox"]));
        }
        let page = items_page(&fixture, &cancelled.id, None, None, None);
        assert!(page
            .items
            .iter()
            .all(|item| item.state == ItemState::Cancelled));
        // §5.2：cancel_requested_at 只投影成「正在取消」，不是另一个终态。
        assert!(!cancelled.is_cancelling);
        let (done, _) = run(&fixture, "cancel-done", &papers[0..1], &["later"]);
        let after_cancel = fixture
            .act_ok(&control(
                "cancel-2",
                &done.id,
                BatchControl::CancelRemaining,
            ))
            .batch()
            .clone();
        assert_eq!(after_cancel.state, BatchState::Completed);
        assert_eq!(after_cancel.counts.succeeded, 1);
        assert!(!after_cancel.is_cancelling);
        // 被取消的那一批从没写过 `reviewed`，这里只剩后来批次加的 `later`。
        assert_eq!(fixture.tags_of(&papers[0]), names(&["inbox", "later"]));
    }

    // ---------------------------------------------------------------------
    // §7.3 撤销
    // ---------------------------------------------------------------------

    #[test]
    fn undo_reverts_only_what_the_batch_wrote() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let (batch, token) = run(&fixture, "undo", &papers, &["reviewed"]);
        assert_eq!(batch.state, BatchState::Completed);
        // 撤销前又有人动了 gamma：那一项冲突，其他项照常回到 before-image。
        retag(&fixture, &papers[2], &["extra", "inbox", "reviewed"]);
        let response = fixture.act_ok(&control("undo-1", &batch.id, undo_with(&token)));
        let child = response.batch();
        assert_eq!(child.command_kind, BatchCommandKind::Compensation);
        assert_eq!(child.relation.as_deref(), Some("compensation"));
        assert_eq!(child.parent_batch_id.as_deref(), Some(batch.id.as_str()));
        assert_eq!(child.parent_command_kind, Some(BatchCommandKind::PatchTags));
        assert_eq!(child.state, BatchState::CompletedWithErrors);
        assert_eq!(child.counts.succeeded, 2);
        assert_eq!(child.counts.failed, 1);
        assert_eq!(undo_of(&response), None);
        assert_eq!(fixture.tags_of(&papers[0]), names(&["inbox"]));
        assert_eq!(fixture.tags_of(&papers[1]), names(&["inbox"]));
        assert_eq!(
            fixture.tags_of(&papers[2]),
            names(&["extra", "inbox", "reviewed"])
        );
        let failed = items_page(
            &fixture,
            &child.id,
            None,
            Some(vec![ItemState::Failed]),
            None,
        )
        .items
        .into_iter()
        .next()
        .expect("the conflicting item");
        assert_eq!(failed.error_code, Some(LibraryActErrorCode::UndoConflict));
        assert!(!failed.retryable, "冲突不是瞬时失败");
        // 补偿批次自己不再开放撤销。
        assert_eq!(child.undo_policy, UndoPolicy::None);
        assert_eq!(child.undo, None);
        // 父记录保持原终态，只挂上 compensation 摘要。
        let parent = projection_of(&fixture, &batch.id);
        assert_eq!(parent.state, BatchState::Completed);
        assert_eq!(parent.counts.succeeded, 3);
        assert_eq!(
            parent.compensation_summary.map(|summary| summary.batch_id),
            Some(child.id.clone())
        );
        // Token 单次：再拿同一个 token 来撤销会被拒。
        assert_eq!(
            fixture.error(&control("undo-2", &batch.id, undo_with(&token))),
            LibraryActErrorCode::UndoConflict
        );
    }

    #[test]
    fn an_undo_token_is_bound_to_its_batch_and_expires() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let (first, first_token) = run(&fixture, "token-a", &papers[0..1], &["one"]);
        let (second, _) = run(&fixture, "token-b", &papers[1..2], &["two"]);
        // 认得这个 token，但它不属于这个批次。
        assert_eq!(
            fixture.error(&control("cross", &second.id, undo_with(&first_token))),
            LibraryActErrorCode::UndoConflict
        );
        assert_eq!(fixture.tags_of(&papers[1]), names(&["inbox", "two"]));
        // 完全不认识的 token 也不能撤销任何东西。
        assert_eq!(
            fixture.error(&control(
                "unknown",
                &first.id,
                undo_with("0123456789abcdef")
            )),
            LibraryActErrorCode::UndoConflict
        );
        // 窗口已过：无论 reconcile 有没有先把状态落成 expired，错误码都稳定。
        {
            let connection = fixture.connection();
            connection
                .execute(
                    "UPDATE library_undo_tokens SET expires_at = '2000-01-01T00:00:00Z' WHERE batch_id = ?1",
                    params![first.id],
                )
                .expect("expire token");
        }
        assert_eq!(
            fixture.error(&control("expired", &first.id, undo_with(&first_token))),
            LibraryActErrorCode::UndoExpired
        );
        assert_eq!(
            projection_of(&fixture, &first.id)
                .undo
                .map(|undo| undo.state),
            Some(UndoTokenState::Expired)
        );
        // 过期不等于可以覆盖：那一项仍然是批次写下的值。
        assert_eq!(fixture.tags_of(&papers[0]), names(&["inbox", "one"]));
    }

    #[test]
    fn a_batch_that_wrote_nothing_owes_no_undo() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        // 补丁是「加一个已经全有的标签」：没有任何一项真的写入。
        let planned = fixture
            .act_ok(&plan_explicit("noop-plan", &papers, &["inbox"], &[]))
            .batch()
            .clone();
        let before = fixture.revisions();
        let started = fixture.act_ok(&start("noop-start", &planned, &[]));
        assert_eq!(started.batch().state, BatchState::Completed);
        assert_eq!(started.batch().counts.succeeded, 3);
        assert_eq!(undo_of(&started), None);
        let after = fixture.revisions();
        assert_eq!(
            after.domain(LibraryDomain::Tags),
            before.domain(LibraryDomain::Tags)
        );
        assert_eq!(
            after.domain(LibraryDomain::Artifacts),
            before.domain(LibraryDomain::Artifacts)
        );
        let connection = fixture.connection();
        let tokens: i64 = connection
            .query_row("SELECT COUNT(*) FROM library_undo_tokens", [], |row| {
                row.get(0)
            })
            .expect("token count");
        assert_eq!(tokens, 0);
        // 没有真实写入就不推进 revision，但取消这类批次仍然是允许的。
        let cancelled = fixture
            .act_ok(&control(
                "noop-cancel",
                &started.batch().id,
                BatchControl::CancelRemaining,
            ))
            .batch()
            .clone();
        assert_eq!(cancelled.state, BatchState::Completed);
    }

    // ---------------------------------------------------------------------
    // §9 计划过期与 §8.2 确认
    // ---------------------------------------------------------------------

    #[test]
    fn an_expired_or_mismatched_plan_has_to_be_previewed_again() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let planned = fixture
            .act_ok(&plan_explicit("expire-plan", &papers, &["reviewed"], &[]))
            .batch()
            .clone();
        let mut wrong_digest = planned.clone();
        wrong_digest.plan_digest = "0".repeat(64);
        assert_eq!(
            fixture.error(&start("digest-start", &wrong_digest, &[])),
            LibraryActErrorCode::StaleBatchPlan
        );
        assert_eq!(fixture.tags_of(&papers[0]), names(&["inbox"]));
        {
            let connection = fixture.connection();
            connection
                .execute(
                    "UPDATE library_batches SET plan_expires_at = '2000-01-01T00:00:00Z' WHERE id = ?1",
                    params![planned.id],
                )
                .expect("expire plan");
        }
        assert_eq!(
            fixture.error(&start("expired-start", &planned, &[])),
            LibraryActErrorCode::StaleBatchPlan
        );
        assert_eq!(fixture.tags_of(&papers[0]), names(&["inbox"]));
        // 不存在的批次要报「找不到」，而不是被 digest 检查掩盖成「计划过期」。
        let mut ghost = planned.clone();
        ghost.id = "no-such-batch".to_string();
        assert_eq!(
            fixture.error(&start("ghost-start", &ghost, &[])),
            LibraryActErrorCode::BatchNotFound
        );
    }

    #[test]
    fn a_frozen_requirement_blocks_start_until_it_is_accepted_by_id() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let planned = fixture
            .act_ok(&plan_explicit("confirm-plan", &papers, &["reviewed"], &[]))
            .batch()
            .clone();
        // 本地打标没有确认项；这里写入同一个形状来验证 Gate（Move/Trash 用它）。
        {
            let connection = fixture.connection();
            connection
                .execute(
                    "UPDATE library_batches SET cost_preview_json = ?1 WHERE id = ?2",
                    params![
                        r#"{"requirements":[{"id":"trash-3","kind":"destructive","itemCount":3,"label":"3 papers move to the trash"}],"cost":{}}"#,
                        planned.id
                    ],
                )
                .expect("freeze a requirement");
        }
        assert_eq!(
            fixture.error(&start("confirm-missing", &planned, &[])),
            LibraryActErrorCode::ConfirmationRequired
        );
        assert_eq!(fixture.tags_of(&papers[0]), names(&["inbox"]));
        // 只接受另一个 id 也不行：确认按 id 逐字回报。
        assert_eq!(
            fixture.error(&start("confirm-other", &planned, &["kind-change"])),
            LibraryActErrorCode::ConfirmationRequired
        );
        let started = fixture
            .act_ok(&start("confirm-accepted", &planned, &["trash-3"]))
            .batch()
            .clone();
        assert_eq!(started.state, BatchState::Completed);
        assert_eq!(started.requirements.len(), 1);
        assert_eq!(started.requirements[0].id, "trash-3");
        assert_eq!(started.requirements[0].item_count, 3);
        assert_eq!(fixture.tags_of(&papers[0]), names(&["inbox", "reviewed"]));
    }

    // ---------------------------------------------------------------------
    // 崩溃恢复
    // ---------------------------------------------------------------------

    #[test]
    fn reconcile_reports_a_killed_run_as_interrupted_unknown() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let (batch, _) = run(&fixture, "crash", &papers, &["reviewed"]);
        // 伪造一次「进程在 Item 执行中被杀」：Item 留在 running，缓存留在 running。
        {
            let connection = fixture.connection();
            connection
                .execute(
                    "UPDATE library_batches SET state = 'running' WHERE id = ?1",
                    params![batch.id],
                )
                .expect("crash batch");
            connection
                .execute(
                    "UPDATE library_batch_items SET state = 'running' WHERE batch_id = ?1 AND ordinal = 0",
                    params![batch.id],
                )
                .expect("crash item");
            assert!(reconcile_interrupted(&connection).expect("reconcile") >= 1);
        }
        let page = items_page(&fixture, &batch.id, None, None, None);
        let interrupted = page
            .items
            .iter()
            .find(|item| item.state == ItemState::InterruptedUnknown)
            .expect("the killed item");
        assert_eq!(interrupted.ordinal, 0);
        assert_eq!(
            interrupted.error_code,
            Some(LibraryActErrorCode::InterruptedUnknown)
        );
        // 效果未知的项不能自动重试，也不能被当成一次普通失败。
        assert!(!interrupted.retryable);
        assert_ne!(
            interrupted.error_code,
            Some(LibraryActErrorCode::WorkspaceBusy)
        );
        let repaired = projection_of(&fixture, &batch.id);
        assert_eq!(repaired.state, BatchState::InterruptedUnknown);
        assert_eq!(repaired.counts.succeeded, 2);
        // 修复是幂等的：第二次跑不再改任何东西。
        assert_eq!(
            reconcile_interrupted(&fixture.connection()).expect("again"),
            0
        );
        // 从没 start 过的批次不会被修成 running 派生态。
        let untouched = fixture
            .act_ok(&plan_explicit("crash-planned", &papers[0..1], &["x"], &[]))
            .batch()
            .clone();
        reconcile_interrupted(&fixture.connection()).expect("third reconcile");
        assert_eq!(
            projection_of(&fixture, &untouched.id).state,
            BatchState::Planned
        );
    }

    // ---------------------------------------------------------------------
    // §7 Move / Trash：效果在文件系统上，逐项 journal 与补偿
    // ---------------------------------------------------------------------

    #[test]
    fn a_same_root_move_relocates_every_file_and_puts_them_back_on_undo() {
        let fixture = fixture();
        let papers = ["alpha", "beta"]
            .into_iter()
            .map(|label| seed_file_paper(&fixture, "Papers/Inbox", label))
            .collect::<Vec<_>>();
        let (batch, token) = run_move(&fixture, "move-home", &papers, "Papers/Archive");
        assert_eq!(batch.state, BatchState::Completed);
        // 同根 Move 不改文档种类，也不摘任何成果：撤销能回到逐字相同的状态。
        assert_eq!(batch.undo_policy, UndoPolicy::Full);
        assert_eq!(
            item_states(&fixture, &batch.id),
            vec![ItemState::Succeeded, ItemState::Succeeded]
        );
        assert_eq!(
            location_of(&fixture, &papers[0]).0,
            "Papers/Archive/alpha.pdf"
        );
        assert!(has_pdf(fixture.path(), "Papers/Archive/beta.pdf"));
        assert!(!has_pdf(fixture.path(), "Papers/Inbox/beta.pdf"));
        // 结果摘要记的是执行后的真实位置，不是计划里的预期值。
        assert_eq!(
            result_json(&fixture, &batch.id, 0)["afterRelativePath"],
            json!("Papers/Archive/alpha.pdf")
        );
        assert_eq!(
            result_json(&fixture, &batch.id, 0)["kindChanged"],
            json!(false)
        );

        let compensation = undo(&fixture, "move-home", &batch.id, &token.expect("full undo"));
        assert_eq!(compensation.state, BatchState::Completed);
        assert_eq!(compensation.relation.as_deref(), Some("compensation"));
        assert_eq!(
            compensation.parent_batch_id.as_deref(),
            Some(batch.id.as_str())
        );
        assert_eq!(
            location_of(&fixture, &papers[0]).0,
            "Papers/Inbox/alpha.pdf"
        );
        assert_eq!(location_of(&fixture, &papers[1]).0, "Papers/Inbox/beta.pdf");
        assert!(has_pdf(fixture.path(), "Papers/Inbox/beta.pdf"));
        assert!(!has_pdf(fixture.path(), "Papers/Archive/alpha.pdf"));
        // 「撤销的撤销」不属于合同：补偿批次自己不再配 Token。
        assert_eq!(compensation.undo_policy, UndoPolicy::None);
        assert!(compensation.undo.is_none());
    }

    #[test]
    fn a_cross_root_move_needs_the_kind_confirmation_and_its_undo_is_only_a_compensation() {
        let fixture = fixture();
        let paper = seed_file_paper(&fixture, "Papers/Inbox", "alpha");
        attach_brief(&fixture, &paper);
        let planned = planned_batch(
            &fixture,
            &plan_move("kind-plan", &[paper.clone()], "Textbooks/CLRS"),
        );
        // 确认页要单独列出被摘掉的成果类别，不能只说「有一些影响」。
        assert_eq!(
            plan_json(&fixture, &planned.id, 0)["droppedCategories"],
            json!(["brief"])
        );
        assert_eq!(planned.requirements.len(), 1);
        assert_eq!(
            planned.requirements[0].kind,
            BatchRequirementKind::KindChange
        );
        assert_eq!(planned.requirements[0].item_count, 1);
        // 成果一旦丢掉就回不来，所以这一批只有补偿，没有 full 撤销。
        assert_eq!(planned.undo_policy, UndoPolicy::Compensating);
        // 没逐字接受确认项之前，文件系统一个字节都不许动。
        assert_eq!(
            fixture.error(&start("kind-refused", &planned, &[])),
            LibraryActErrorCode::ConfirmationRequired
        );
        assert_eq!(location_of(&fixture, &paper).0, "Papers/Inbox/alpha.pdf");
        assert_eq!(brief_heads(&fixture, &paper), 1);

        let (batch, token) = start_planned(&fixture, "kind", &planned);
        assert_eq!(batch.state, BatchState::Completed);
        assert_eq!(location_of(&fixture, &paper).0, "Textbooks/CLRS/alpha.pdf");
        assert!(has_pdf(fixture.path(), "Textbooks/CLRS/alpha.pdf"));
        assert_eq!(brief_heads(&fixture, &paper), 0);

        let compensation = undo(
            &fixture,
            "kind",
            &batch.id,
            &token.expect("compensating undo"),
        );
        assert_eq!(compensation.counts.succeeded, 1);
        assert_eq!(location_of(&fixture, &paper).0, "Papers/Inbox/alpha.pdf");
        assert!(has_pdf(fixture.path(), "Papers/Inbox/alpha.pdf"));
        // §7.3：反向 Move 只把文件放回原位——被摘掉的成果与 Job 都不回来。
        assert_eq!(brief_heads(&fixture, &paper), 0);
        assert_eq!(
            result_json(&fixture, &compensation.id, 0)["artifactsRestored"],
            json!(false)
        );
    }

    #[test]
    fn a_colliding_target_fails_only_that_item_until_the_path_is_free() {
        let fixture = fixture();
        let alpha = seed_file_paper(&fixture, "Papers/Inbox", "alpha");
        let beta = seed_file_paper(&fixture, "Papers/Inbox", "beta");
        // 目标位上有一个还没入库的文件：宁可失败也不能覆盖别人的文件。
        write_pdf(fixture.path(), "Papers/Archive/beta.pdf");
        let planned = planned_batch(
            &fixture,
            &plan_move("hit-plan", &[alpha.clone(), beta.clone()], "Papers/Archive"),
        );
        assert_eq!(planned.requirements.len(), 1);
        assert_eq!(planned.requirements[0].kind, BatchRequirementKind::Conflict);
        assert_eq!(planned.requirements[0].item_count, 1);

        let (batch, token) = start_planned(&fixture, "hit", &planned);
        assert_eq!(batch.state, BatchState::CompletedWithErrors);
        assert_eq!(
            item_states(&fixture, &batch.id),
            vec![ItemState::Succeeded, ItemState::Failed]
        );
        assert_eq!(
            item_failure(&fixture, &batch.id, 1),
            Some(LibraryActErrorCode::TargetPathConflict)
        );
        // 局部失败不丢成功结果：撞车的那一项失败，另一项照常搬完。
        assert_eq!(location_of(&fixture, &alpha).0, "Papers/Archive/alpha.pdf");
        assert_eq!(location_of(&fixture, &beta).0, "Papers/Inbox/beta.pdf");
        assert!(has_pdf(fixture.path(), "Papers/Archive/beta.pdf"));
        // §10.2：失败摘要只用固定业务文案，底层文本与绝对路径都不许进来。
        let summary = items_page(&fixture, &batch.id, None, None, None).items[1]
            .error_summary
            .clone()
            .expect("a typed summary");
        assert!(!summary.contains("Papers"));
        assert!(!summary.contains(&fixture.path().display().to_string()));

        // 路径冲突不是「重试一下就好」：默认重试集合为空，UI 拿不到假的希望。
        assert_eq!(
            fixture.error(&control("hit-default", &batch.id, retry_of(None))),
            LibraryActErrorCode::SelectionEmpty
        );
        // 用户把占位的文件清掉，再显式勾选那一项：重试批次只跑它。
        remove_pdf(fixture.path(), "Papers/Archive/beta.pdf");
        let failed_id = items_page(
            &fixture,
            &batch.id,
            None,
            Some(vec![ItemState::Failed]),
            None,
        )
        .items[0]
            .id
            .clone();
        let retried = fixture
            .act_ok(&control(
                "hit-retry",
                &batch.id,
                retry_of(Some(&[failed_id])),
            ))
            .batch()
            .clone();
        assert_eq!(retried.state, BatchState::Completed);
        assert_eq!(retried.total_items, 1);
        assert_eq!(retried.relation.as_deref(), Some("retry"));
        assert_eq!(location_of(&fixture, &beta).0, "Papers/Archive/beta.pdf");
        // 父批次的 Token 只覆盖父批次自己写下的效果，重试批次要各自撤销。
        let compensation = undo(&fixture, "hit", &batch.id, &token.expect("alpha did move"));
        assert_eq!(compensation.total_items, 1);
        assert_eq!(location_of(&fixture, &alpha).0, "Papers/Inbox/alpha.pdf");
        assert_eq!(location_of(&fixture, &beta).0, "Papers/Archive/beta.pdf");
    }

    #[test]
    fn a_paper_already_in_the_target_is_skipped_instead_of_running_again() {
        let fixture = fixture();
        let alpha = seed_file_paper(&fixture, "Papers/Archive", "alpha");
        let beta = seed_file_paper(&fixture, "Papers/Inbox", "beta");
        let (batch, token) = run_move(
            &fixture,
            "same",
            &[alpha.clone(), beta.clone()],
            "Papers/Archive",
        );
        // 已经在目标位不算冲突，也不算成功：它没有产生任何效果。
        assert_eq!(
            item_states(&fixture, &batch.id),
            vec![ItemState::Skipped, ItemState::Succeeded]
        );
        assert_eq!(batch.counts.skipped, 1);
        assert_eq!(
            result_json(&fixture, &batch.id, 0)["outcome"],
            json!("already_in_target")
        );
        assert_eq!(location_of(&fixture, &alpha).0, "Papers/Archive/alpha.pdf");
        assert_eq!(location_of(&fixture, &beta).0, "Papers/Archive/beta.pdf");

        // 补偿只覆盖真的动过的那一项。
        let compensation = undo(&fixture, "same", &batch.id, &token.expect("beta moved"));
        assert_eq!(compensation.total_items, 1);
        assert_eq!(target_keys(&fixture, &compensation.id), vec![beta.clone()]);
        assert_eq!(location_of(&fixture, &beta).0, "Papers/Inbox/beta.pdf");
        assert_eq!(location_of(&fixture, &alpha).0, "Papers/Archive/alpha.pdf");
    }

    #[test]
    fn a_killed_file_system_run_finishes_its_queue_without_running_anything_twice() {
        let fixture = fixture();
        let alpha = seed_file_paper(&fixture, "Papers/Inbox", "alpha");
        let beta = seed_file_paper(&fixture, "Papers/Inbox", "beta");
        let planned = planned_batch(
            &fixture,
            &plan_move(
                "kill-plan",
                &[alpha.clone(), beta.clone()],
                "Papers/Archive",
            ),
        );
        // 伪造一次「进程在两项之间被杀」：第一项的效果已经落在文件系统与库里，
        // 但它的 Item 还留在 running，第二项刚被推成 queued。
        let module = crate::paper_module::PaperModule::open(fixture.path()).expect("module");
        module
            .move_paper(&alpha, "Papers/Archive", "alpha.pdf", true)
            .expect("the first move landed");
        let timestamp = now();
        {
            let connection = fixture.connection();
            connection
                .execute(
                    "UPDATE library_batches SET state = 'running', started_at = ?2, updated_at = ?2 WHERE id = ?1",
                    params![planned.id, timestamp],
                )
                .expect("crash the batch");
            connection
                .execute(
                    "UPDATE library_batch_items SET state = 'running', started_at = ?2, updated_at = ?2
                     WHERE batch_id = ?1 AND ordinal = 0",
                    params![planned.id, timestamp],
                )
                .expect("crash the first item");
            connection
                .execute(
                    "UPDATE library_batch_items SET state = 'queued', updated_at = ?2
                     WHERE batch_id = ?1 AND ordinal = 1",
                    params![planned.id, timestamp],
                )
                .expect("queue the second item");
        }

        let accepted = planned
            .requirements
            .iter()
            .map(|requirement| requirement.id.as_str())
            .collect::<Vec<_>>();
        let response = fixture.act_ok(&start("kill-start", &planned, &accepted));
        let resumed = response.batch().clone();
        // 效果未知的那一项按 interrupted_unknown 报，绝不悄悄重跑；队列里的下一项跑完。
        assert_eq!(resumed.state, BatchState::InterruptedUnknown);
        assert_eq!(resumed.counts.interrupted_unknown, 1);
        assert_eq!(resumed.counts.succeeded, 1);
        assert_eq!(
            item_states(&fixture, &resumed.id),
            vec![ItemState::InterruptedUnknown, ItemState::Succeeded]
        );
        // alpha 的真实效果由 PaperModule 自己提交过：批次不猜、也不回滚它。
        assert_eq!(location_of(&fixture, &alpha).0, "Papers/Archive/alpha.pdf");
        assert_eq!(location_of(&fixture, &beta).0, "Papers/Archive/beta.pdf");
        assert!(has_pdf(fixture.path(), "Papers/Archive/alpha.pdf"));

        // 重复 start 不重复执行：队列空了以后再 start，只是重新读一遍投影。
        let again = fixture
            .act_ok(&start("kill-again", &planned, &accepted))
            .batch()
            .clone();
        assert_eq!(again.id, resumed.id);
        assert_eq!(again.counts.succeeded, 1);
        assert_eq!(again.counts.interrupted_unknown, 1);
        assert_eq!(location_of(&fixture, &beta).0, "Papers/Archive/beta.pdf");

        // 续跑批次同样配撤销入口，但 Token 只覆盖它确证动过的那一项。
        let token = undo_of(&response).expect("the resumed queue still owes an undo entry");
        let compensation = undo(&fixture, "kill", &resumed.id, &token);
        assert_eq!(target_keys(&fixture, &compensation.id), vec![beta.clone()]);
        assert_eq!(location_of(&fixture, &beta).0, "Papers/Inbox/beta.pdf");
        assert_eq!(location_of(&fixture, &alpha).0, "Papers/Archive/alpha.pdf");
    }

    #[test]
    fn a_trash_batch_needs_the_destructive_confirmation_and_undo_restores_the_files() {
        let fixture = fixture();
        let alpha = seed_file_paper(&fixture, "Papers/Inbox", "alpha");
        let beta = seed_file_paper(&fixture, "Papers/Inbox", "beta");
        let planned = planned_batch(
            &fixture,
            &plan_trash("trash-plan", &[alpha.clone(), beta.clone()]),
        );
        assert_eq!(planned.requirements.len(), 1);
        assert_eq!(
            planned.requirements[0].kind,
            BatchRequirementKind::Destructive
        );
        assert_eq!(planned.requirements[0].item_count, 2);
        // 回收站撤销不恢复 Job，也不恢复手排位置：只有补偿，没有 full。
        assert_eq!(planned.undo_policy, UndoPolicy::Compensating);
        assert_eq!(
            fixture.error(&start("trash-refused", &planned, &[])),
            LibraryActErrorCode::ConfirmationRequired
        );
        assert_eq!(
            location_of(&fixture, &alpha),
            ("Papers/Inbox/alpha.pdf".to_string(), false)
        );

        let (batch, token) = start_planned(&fixture, "trash", &planned);
        assert_eq!(batch.state, BatchState::Completed);
        assert_eq!(location_of(&fixture, &alpha).1, true);
        // 文件进的是回收站，不是消失了。
        let (original, trash_relative) = trash_of(&fixture, &alpha);
        assert_eq!(original, "Papers/Inbox/alpha.pdf");
        assert!(fixture
            .path()
            .join(".read-desktop")
            .join(&trash_relative)
            .is_file());
        assert!(!has_pdf(fixture.path(), "Papers/Inbox/beta.pdf"));

        let compensation = undo(
            &fixture,
            "trash",
            &batch.id,
            &token.expect("compensating undo"),
        );
        assert_eq!(compensation.state, BatchState::Completed);
        assert_eq!(compensation.counts.succeeded, 2);
        assert_eq!(location_of(&fixture, &alpha).0, "Papers/Inbox/alpha.pdf");
        assert!(!location_of(&fixture, &beta).1);
        assert!(has_pdf(fixture.path(), "Papers/Inbox/alpha.pdf"));
        assert!(has_pdf(fixture.path(), "Papers/Inbox/beta.pdf"));
        // 放回的位置以执行后的真实事实为准，同时保留计划里冻结的原路径供审计。
        let effect = result_json(&fixture, &compensation.id, 0);
        assert_eq!(effect["restoredTo"], json!("Papers/Inbox/alpha.pdf"));
        assert_eq!(
            effect["plannedOriginalPath"],
            json!("Papers/Inbox/alpha.pdf")
        );
        assert_eq!(effect["jobsRestored"], json!(false));
    }

    #[test]
    fn a_missing_file_fails_only_its_own_trash_item() {
        let fixture = fixture();
        let alpha = seed_file_paper(&fixture, "Papers/Inbox", "alpha");
        let beta = seed_file_paper(&fixture, "Papers/Inbox", "beta");
        remove_pdf(fixture.path(), "Papers/Inbox/beta.pdf");
        let planned = planned_batch(
            &fixture,
            &plan_trash("miss-plan", &[alpha.clone(), beta.clone()]),
        );
        // 「其中几项会失败」在预览页就得说清楚，而不是等 Start 之后糊成一团。
        assert_eq!(planned.requirements.len(), 2);
        assert_eq!(planned.requirements[1].kind, BatchRequirementKind::Conflict);
        assert_eq!(planned.requirements[1].item_count, 1);
        assert_eq!(
            plan_json(&fixture, &planned.id, 1)["sourceMissing"],
            json!(true)
        );

        let (batch, token) = start_planned(&fixture, "miss", &planned);
        assert_eq!(batch.state, BatchState::CompletedWithErrors);
        assert_eq!(
            item_states(&fixture, &batch.id),
            vec![ItemState::Succeeded, ItemState::Failed]
        );
        assert_eq!(
            item_failure(&fixture, &batch.id, 1),
            Some(LibraryActErrorCode::SourceMissing)
        );
        assert_eq!(location_of(&fixture, &alpha).1, true);
        assert_eq!(
            location_of(&fixture, &beta),
            ("Papers/Inbox/beta.pdf".to_string(), false)
        );

        // 失败的那一项根本没有效果可补，所以不进补偿队列。
        let compensation = undo(
            &fixture,
            "miss",
            &batch.id,
            &token.expect("alpha did trash"),
        );
        assert_eq!(compensation.total_items, 1);
        assert_eq!(location_of(&fixture, &alpha).1, false);
        assert_eq!(location_of(&fixture, &beta).1, false);
        assert!(has_pdf(fixture.path(), "Papers/Inbox/alpha.pdf"));
    }

    #[test]
    fn an_undo_stops_for_a_paper_someone_moved_again() {
        let fixture = fixture();
        let alpha = seed_file_paper(&fixture, "Papers/Inbox", "alpha");
        let beta = seed_file_paper(&fixture, "Papers/Inbox", "beta");
        let (batch, token) = run_move(
            &fixture,
            "oob",
            &[alpha.clone(), beta.clone()],
            "Papers/Archive",
        );
        // 有人在批次之外又把 alpha 搬走了一次：撤销不能覆盖他刚做的操作。
        let module = crate::paper_module::PaperModule::open(fixture.path()).expect("module");
        module
            .move_paper(&alpha, "Papers/Elsewhere", "alpha.pdf", true)
            .expect("an out of band move");

        let compensation = undo(&fixture, "oob", &batch.id, &token.expect("full undo"));
        assert_eq!(compensation.state, BatchState::CompletedWithErrors);
        assert_eq!(
            item_failure(&fixture, &compensation.id, 0),
            Some(LibraryActErrorCode::UndoConflict)
        );
        assert_eq!(
            location_of(&fixture, &alpha).0,
            "Papers/Elsewhere/alpha.pdf"
        );
        // 冲突只挡住那一项，其余照原样放回。
        assert_eq!(location_of(&fixture, &beta).0, "Papers/Inbox/beta.pdf");
        assert!(has_pdf(fixture.path(), "Papers/Inbox/beta.pdf"));
    }

    #[test]
    fn a_move_target_path_must_be_a_library_folder_before_any_file_touches_disk() {
        let fixture = fixture();
        let alpha = seed_file_paper(&fixture, "Papers/Inbox", "alpha");
        // 越出 Papers / Textbooks 的目标在计划阶段就拒，不给它任何碰文件的机会。
        assert_eq!(
            fixture.error(&LibraryActRequest::PlanBatch {
                protocol_version: protocol(),
                idempotency_key: "bad-target".to_string(),
                command: BatchCommand::Move {
                    collection_path: "../escape".to_string(),
                },
                target: BatchTarget::Explicit {
                    paper_ids: vec![alpha.clone()],
                },
            }),
            LibraryActErrorCode::InvalidQuery
        );
        assert_eq!(location_of(&fixture, &alpha).0, "Papers/Inbox/alpha.pdf");
        assert_eq!(
            fixture.error(&plan_move(
                "traverse-target",
                &[alpha.clone()],
                "Papers/../../outside"
            )),
            LibraryActErrorCode::InvalidQuery
        );
        assert_eq!(
            fixture.error(&plan_move("empty-target", &[alpha], "")),
            LibraryActErrorCode::InvalidQuery
        );
    }

    // ---------------------------------------------------------------------
    // §5.1 成员快照
    // ---------------------------------------------------------------------

    #[test]
    fn a_query_target_needs_both_the_membership_digest_and_the_dependency_vector() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let filters = vec![collection_filter()];
        let (digest, vector) = snapshot(&fixture, &filters);
        let planned = fixture
            .act_ok(&plan_query(
                "q-plan",
                &filters,
                &papers[2..],
                &digest,
                &vector,
            ))
            .batch()
            .clone();
        // excludedIds 从成员里去掉，剩下的仍然按成员集合冻结。
        assert_eq!(planned.total_items, 2);
        let page = items_page(&fixture, &planned.id, None, None, None);
        assert_eq!(
            page.items
                .iter()
                .map(|item| item.paper_id.clone())
                .collect::<Vec<_>>(),
            vec![Some(papers[0].clone()), Some(papers[1].clone()),]
        );
        assert_eq!(
            fixture.error(&plan_query(
                "q-digest",
                &filters,
                &[],
                "0".repeat(64).as_str(),
                &vector
            )),
            LibraryActErrorCode::StaleSelection
        );
        assert_eq!(
            fixture.error(&plan_query("q-vector", &filters, &[], &digest, &[])),
            LibraryActErrorCode::StaleSelection
        );
    }

    #[test]
    fn the_membership_snapshot_only_depends_on_what_the_filter_actually_reads() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let by_collection = vec![collection_filter()];
        let (digest, vector) = snapshot(&fixture, &by_collection);
        assert_eq!(
            vector
                .iter()
                .map(|entry| entry.domain.as_str())
                .collect::<Vec<_>>(),
            vec!["structure"]
        );
        retag(&fixture, &papers[0], &["inbox", "unrelated"]);
        // 纯目录选择不该被一次无关的打标判成漂移。
        assert_eq!(
            fixture
                .act_ok(&plan_query("coll", &by_collection, &[], &digest, &vector))
                .batch()
                .total_items,
            3
        );
        // 带标签的筛选把 tags 记进依赖，打标就让它失效。
        let by_tag = vec![QueryFilter::Tag {
            tag: "unrelated".to_string(),
        }];
        let (tag_digest, tag_vector) = snapshot(&fixture, &by_tag);
        assert_eq!(tag_vector.len(), 2);
        retag(&fixture, &papers[1], &["else", "inbox"]);
        assert_eq!(
            fixture.error(&plan_query("tag", &by_tag, &[], &tag_digest, &tag_vector)),
            LibraryActErrorCode::StaleSelection
        );
        // 摘要与 filters 不匹配（旧摘要配新筛选）同样是漂移。
        assert_eq!(
            fixture.error(&plan_query("mix", &by_tag, &[], &digest, &tag_vector)),
            LibraryActErrorCode::StaleSelection
        );
    }

    // ---------------------------------------------------------------------
    // §10.1 读投影
    // ---------------------------------------------------------------------

    #[test]
    fn batch_items_page_walks_ordinals_and_reports_the_failed_items() {
        let fixture = fixture();
        let papers = {
            let connection = fixture.connection();
            (0..5)
                .map(|index| seed_paper(&connection, &format!("page-{index}"), &["inbox"]))
                .collect::<Vec<_>>()
        };
        let planned = fixture
            .act_ok(&plan_explicit("page-plan", &papers, &["reviewed"], &[]))
            .batch()
            .clone();
        let first = items_page(&fixture, &planned.id, None, None, Some(2));
        assert_eq!(first.batch_id, planned.id);
        assert_eq!(first.page_size, 2);
        assert_eq!(ordinals(&first), vec![0, 1]);
        assert!(first.has_more);
        assert_eq!(first.next_ordinal, Some(1));
        let second = items_page(&fixture, &planned.id, Some(1), None, Some(2));
        assert_eq!(ordinals(&second), vec![2, 3]);
        assert_eq!(second.next_ordinal, Some(3));
        let third = items_page(&fixture, &planned.id, Some(3), None, Some(2));
        assert_eq!(ordinals(&third), vec![4]);
        assert!(!third.has_more);
        assert_eq!(third.next_ordinal, None);
        // 默认页宽覆盖整批，任务中心一次就能拉到全部失败项。
        let all = items_page(
            &fixture,
            &planned.id,
            None,
            Some(vec![ItemState::Planned]),
            None,
        );
        assert_eq!(all.items.len(), 5);
        assert_eq!(
            items_page(
                &fixture,
                &planned.id,
                None,
                Some(vec![ItemState::Failed]),
                None
            )
            .items
            .len(),
            0
        );
        // 越界分页是请求问题；库没有问题。
        let error = crate::library_query::read(
            fixture.path(),
            &LibraryReadRequest::BatchItems {
                protocol_version: protocol(),
                batch_id: planned.id.clone(),
                after_ordinal: None,
                states: None,
                limit: Some(BATCH_ITEMS_MAX_PAGE + 1),
            },
        )
        .expect_err("limit above the maximum");
        assert_eq!(error.code, LibraryQueryErrorCode::InvalidQuery);
        let missing = crate::library_query::read(
            fixture.path(),
            &LibraryReadRequest::Batch {
                protocol_version: protocol(),
                batch_id: "no-such-batch".to_string(),
            },
        )
        .expect_err("unknown batch");
        assert_eq!(missing.code, LibraryQueryErrorCode::InvalidQuery);
        assert_eq!(
            fixture.error(&control(
                "page-cancel",
                "no-such-batch",
                BatchControl::CancelRemaining
            )),
            LibraryActErrorCode::BatchNotFound
        );
    }

    // ---------------------------------------------------------------------
    // §7 Import 行：多文件导入（多选 / 拖放）
    // ---------------------------------------------------------------------

    /// 源文件必须放在 Workspace **之外**：库内的路径走的是「就地登记」那条分支，
    /// 复制与 hash 的语义都不一样。
    fn write_source(dir: &Path, name: &str, body: &[u8]) -> String {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().expect("parent directory")).expect("source directory");
        fs::write(&path, body).expect("write source");
        path.to_string_lossy().to_string()
    }

    /// 内容 = 这个源文件自己的路径：不同的路径必须是不同的字节，否则「两个不同的文件」
    /// 会先撞上 hash 重复，测试想验的那条规则就被另一条抢先；同名的两个文件尤其如此。
    /// 反过来，同一个路径重算必须得到同样的内容，重复路径项才是真的重复。
    fn pdf_source(dir: &Path, name: &str) -> String {
        let identity = format!("{}::{}", dir.to_string_lossy(), name);
        write_source(dir, name, format!("%PDF-1.4\n{identity}\n").as_bytes())
    }

    fn plan_import(key: &str, paths: &[String], collection_path: &str) -> LibraryActRequest {
        LibraryActRequest::PlanBatch {
            protocol_version: protocol(),
            idempotency_key: key.to_string(),
            command: BatchCommand::Import {
                collection_path: collection_path.to_string(),
            },
            target: BatchTarget::Sources {
                paths: paths.to_vec(),
            },
        }
    }

    fn outcome(text: &str) -> Option<String> {
        Some(text.to_string())
    }

    /// 逐项结论，按 ordinal 升序。`state` 只说「跑没跑成」，导入要看的是 `outcome`。
    fn outcomes(fixture: &Fixture, batch_id: &str) -> Vec<Option<String>> {
        items_page(fixture, batch_id, None, None, None)
            .items
            .into_iter()
            .map(|item| item.outcome)
            .collect()
    }

    /// 库里当前在册的 Paper 路径（回收站里的不算），按路径升序。
    fn registered_papers(fixture: &Fixture) -> Vec<String> {
        let connection = fixture.connection();
        let mut statement = connection
            .prepare(
                "SELECT relative_path FROM papers WHERE deleted_at IS NULL ORDER BY relative_path",
            )
            .expect("prepare");
        statement
            .query_map([], |row| row.get(0))
            .expect("query")
            .map(|row| row.expect("row"))
            .collect()
    }

    fn open_conflict_kind(fixture: &Fixture) -> Option<String> {
        let connection = fixture.connection();
        connection
            .query_row(
                "SELECT conflict_kind FROM reconciliation_conflicts
                 WHERE status = 'open' ORDER BY created_at DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .expect("conflict")
    }

    #[test]
    fn an_import_batch_gives_every_source_its_own_outcome() {
        let fixture = fixture();
        let sources = tempfile::tempdir().expect("source directory");
        let alpha = pdf_source(sources.path(), "alpha.pdf");
        let beta = pdf_source(sources.path(), "beta.pdf");
        let planned = planned_batch(
            &fixture,
            &plan_import(
                "import-plan",
                &[
                    alpha.clone(),
                    alpha.clone(),
                    write_source(sources.path(), "notes.txt", b"plain text"),
                    sources
                        .path()
                        .join("gone.pdf")
                        .to_string_lossy()
                        .to_string(),
                    beta.clone(),
                ],
                "Papers/Inbox",
            ),
        );
        // 一次拖放里混进重复路径、非 PDF 与已经不在的文件：每一项都要有自己的结论，
        // 整批不能无反馈地消失，也不能为它们仨各跑一次空执行。
        assert_eq!(
            item_states(&fixture, &planned.id),
            vec![
                ItemState::Planned,
                ItemState::Skipped,
                ItemState::Skipped,
                ItemState::Skipped,
                ItemState::Planned,
            ]
        );
        assert_eq!(
            outcomes(&fixture, &planned.id),
            vec![
                None,
                outcome(IMPORT_OUTCOME_DUPLICATE_SOURCE),
                outcome(IMPORT_OUTCOME_NOT_PDF),
                outcome(IMPORT_OUTCOME_SOURCE_MISSING),
                None,
            ]
        );
        // 计划阶段一个文件都不许进库。
        assert_eq!(registered_papers(&fixture), Vec::<String>::new());
        assert_eq!(planned.undo_policy, UndoPolicy::Compensating);
        assert_eq!(
            plan_json(&fixture, &planned.id, 0)["targetRelativePath"],
            "Papers/Inbox/alpha.pdf"
        );

        let (batch, token) = start_planned(&fixture, "import", &planned);
        assert_eq!(batch.state, BatchState::Completed);
        assert_eq!(batch.counts.succeeded, 2);
        assert_eq!(batch.counts.skipped, 3);
        assert_eq!(
            outcomes(&fixture, &batch.id),
            vec![
                outcome(IMPORT_OUTCOME_CREATED),
                outcome(IMPORT_OUTCOME_DUPLICATE_SOURCE),
                outcome(IMPORT_OUTCOME_NOT_PDF),
                outcome(IMPORT_OUTCOME_SOURCE_MISSING),
                outcome(IMPORT_OUTCOME_CREATED),
            ]
        );
        assert_eq!(
            registered_papers(&fixture),
            vec![
                "Papers/Inbox/alpha.pdf".to_string(),
                "Papers/Inbox/beta.pdf".to_string()
            ]
        );
        assert!(has_pdf(fixture.path(), "Papers/Inbox/beta.pdf"));
        // 用户的源文件一个都不动：复制进库的那一份归库管，原件不属于这一批。
        assert!(Path::new(&alpha).is_file() && Path::new(&beta).is_file());
        // 导入项的身份是执行才产出的：回填之后任务中心才点得开这一项。
        let items = items_page(&fixture, &batch.id, None, None, None).items;
        assert!(items[0].paper_id.is_some() && items[0].revision_id.is_some());
        assert!(items[1].paper_id.is_none());
        // §10.2：绝对路径与文件名都不进投影，只有逐项 `plan_json` 里留一份归档。
        let projected = serde_json::to_string(&items).expect("item projections");
        assert!(!projected.contains("alpha.pdf"));
        assert!(!projected.contains(&sources.path().to_string_lossy().to_string()));

        // 撤销只带走本批新建的那两个；跳过的那些库里本来就没有东西可还。
        let token = token.expect("a batch that created two Papers owes an undo entry");
        let compensation = undo(&fixture, "import", &batch.id, &token);
        assert_eq!(
            target_keys(&fixture, &compensation.id),
            vec![items[0].target_key.clone(), items[4].target_key.clone()]
        );
        assert_eq!(compensation.state, BatchState::Completed);
        assert_eq!(registered_papers(&fixture), Vec::<String>::new());
        assert!(!has_pdf(fixture.path(), "Papers/Inbox/alpha.pdf"));
        assert!(Path::new(&alpha).is_file());
    }

    #[test]
    fn a_skipped_source_does_not_claim_its_target_path() {
        let fixture = fixture();
        let sources = tempfile::tempdir().expect("source directory");
        let missing = sources
            .path()
            .join("gone.pdf")
            .to_string_lossy()
            .to_string();
        let steady = pdf_source(sources.path(), "sub/gone.pdf");
        let planned = planned_batch(
            &fixture,
            &plan_import("claim-plan", &[missing, steady], "Papers"),
        );
        // 「不在原处」的那一份永远不会写文件：它占住 `Papers/gone.pdf` 只会让真能进的
        // 那一份白报一个冲突，用户还得为它多点一次确认。
        assert_eq!(
            item_states(&fixture, &planned.id),
            vec![ItemState::Skipped, ItemState::Planned]
        );
        assert!(planned.requirements.is_empty());
        let (batch, _) = start_planned(&fixture, "claim", &planned);
        assert_eq!(batch.state, BatchState::Completed);
        assert_eq!(
            outcomes(&fixture, &batch.id),
            vec![
                outcome(IMPORT_OUTCOME_SOURCE_MISSING),
                outcome(IMPORT_OUTCOME_CREATED)
            ]
        );
        assert_eq!(
            registered_papers(&fixture),
            vec!["Papers/gone.pdf".to_string()]
        );
        assert!(has_pdf(fixture.path(), "Papers/gone.pdf"));
    }

    #[test]
    fn a_source_already_in_the_library_ends_as_a_conflict_instead_of_a_second_paper() {
        let fixture = fixture();
        let sources = tempfile::tempdir().expect("source directory");
        let first = write_source(sources.path(), "first.pdf", b"%PDF-1.4 same bytes\n");
        let second = write_source(sources.path(), "second.pdf", b"%PDF-1.4 same bytes\n");
        let planned = planned_batch(
            &fixture,
            &plan_import("dup-plan", &[first, second], "Papers"),
        );
        // hash 规则只有 `PaperModule` 一份实现，计划阶段不复算：预览时两项都像能进，
        // 真正的结论要到执行时才逐项写下来（与 Move 的「预览就能看见的冲突」相对）。
        assert!(planned.requirements.is_empty());
        let (batch, token) = start_planned(&fixture, "dup", &planned);
        assert_eq!(batch.state, BatchState::Completed);
        assert_eq!(batch.counts.succeeded, 1);
        assert_eq!(batch.counts.skipped, 1);
        assert_eq!(
            outcomes(&fixture, &batch.id),
            vec![
                outcome(IMPORT_OUTCOME_CREATED),
                outcome(IMPORT_OUTCOME_CONFLICT)
            ]
        );
        assert_eq!(
            registered_papers(&fixture),
            vec!["Papers/first.pdf".to_string()]
        );
        assert_eq!(
            result_json(&fixture, &batch.id, 1)["conflictKind"],
            "duplicate_hash"
        );
        assert_eq!(
            open_conflict_kind(&fixture).as_deref(),
            Some("duplicate_hash")
        );
        // 冲突不写第二个文件：库里没登记的东西不该躺在 Collection 里。
        assert!(!has_pdf(fixture.path(), "Papers/second.pdf"));
        // 新建的那一份照样可撤销。
        let token = token.expect("one created Paper still owes an undo entry");
        undo(&fixture, "dup", &batch.id, &token);
        assert_eq!(registered_papers(&fixture), Vec::<String>::new());
    }

    #[test]
    fn two_different_sources_with_one_file_name_do_not_report_the_second_as_reused() {
        let fixture = fixture();
        let sources = tempfile::tempdir().expect("source directory");
        let top = pdf_source(sources.path(), "paper.pdf");
        let nested = pdf_source(sources.path(), "sub/paper.pdf");
        let planned = planned_batch(
            &fixture,
            &plan_import("name-plan", &[top, nested], "Papers"),
        );
        // 同批两项撞同一个目标路径是「预览就该看见」的冲突，不该等 Start 之后才发现。
        assert_eq!(planned.requirements.len(), 1);
        assert_eq!(planned.requirements[0].kind, BatchRequirementKind::Conflict);
        assert_eq!(planned.requirements[0].id, REQUIREMENT_CONFLICT);
        assert_eq!(planned.requirements[0].item_count, 1);
        assert_eq!(
            fixture.error(&start("name-refused", &planned, &[])),
            LibraryActErrorCode::ConfirmationRequired
        );
        assert_eq!(registered_papers(&fixture), Vec::<String>::new());

        let (batch, _) = start_planned(&fixture, "name", &planned);
        assert_eq!(
            outcomes(&fixture, &batch.id),
            vec![
                outcome(IMPORT_OUTCOME_CREATED),
                outcome(IMPORT_OUTCOME_CONFLICT)
            ]
        );
        // 关键的一条：同名不同内容绝不能报成「复用了那个 Paper」。把 B 报成 A，
        // 用户就会以为第二个文件已经进了库。
        assert_eq!(
            result_json(&fixture, &batch.id, 1)["conflictKind"],
            "target_path_conflict"
        );
        assert_eq!(
            registered_papers(&fixture),
            vec!["Papers/paper.pdf".to_string()]
        );
        assert_eq!(
            open_conflict_kind(&fixture).as_deref(),
            Some("target_path_conflict")
        );
    }

    #[test]
    fn a_corrupt_pdf_fails_only_its_own_item_and_is_not_retryable() {
        let fixture = fixture();
        let sources = tempfile::tempdir().expect("source directory");
        let good = pdf_source(sources.path(), "good.pdf");
        let broken = write_source(sources.path(), "broken.pdf", b"this is not a pdf at all");
        let planned = planned_batch(
            &fixture,
            &plan_import("broken-plan", &[good, broken], "Papers/Inbox"),
        );
        // 扩展名对得上就先入计划：内容坏没坏只有读过的 `PaperModule` 知道。
        assert_eq!(
            item_states(&fixture, &planned.id),
            vec![ItemState::Planned, ItemState::Planned]
        );
        let (batch, _) = start_planned(&fixture, "broken", &planned);
        assert_eq!(batch.state, BatchState::CompletedWithErrors);
        assert_eq!(
            item_states(&fixture, &batch.id),
            vec![ItemState::Succeeded, ItemState::Failed]
        );
        assert_eq!(
            item_failure(&fixture, &batch.id, 1),
            Some(LibraryActErrorCode::InvalidSource)
        );
        // 局部失败不丢成功结果：那一份好文件已经登记在库里。
        assert_eq!(
            registered_papers(&fixture),
            vec!["Papers/Inbox/good.pdf".to_string()]
        );
        assert_eq!(
            result_json(&fixture, &batch.id, 0)["outcome"],
            IMPORT_OUTCOME_CREATED
        );
        assert!(!has_pdf(fixture.path(), "Papers/Inbox/broken.pdf"));
        let items = items_page(&fixture, &batch.id, None, None, None).items;
        // 同一份字节重跑不会变好，所以它不配重试。
        assert!(!items[1].retryable);
        assert_eq!(
            fixture.error(&control("broken-retry", &batch.id, retry_of(None))),
            LibraryActErrorCode::SelectionEmpty
        );
        // §10.2：失败摘要只有业务文案，OS 文本与绝对路径都不许进来。
        let summary = items[1]
            .error_summary
            .clone()
            .expect("a failed item carries a summary");
        assert!(!summary.contains(&sources.path().to_string_lossy().to_string()));
        assert!(!summary.contains('\\'));
    }

    #[test]
    fn a_rewritten_source_retries_while_a_deleted_one_does_not() {
        let fixture = fixture();
        let sources = tempfile::tempdir().expect("source directory");
        let steady = pdf_source(sources.path(), "steady.pdf");
        let rewritten = pdf_source(sources.path(), "rewritten.pdf");
        let deleted = pdf_source(sources.path(), "deleted.pdf");
        let planned = planned_batch(
            &fixture,
            &plan_import(
                "drift-plan",
                &[steady.clone(), rewritten.clone(), deleted.clone()],
                "Papers/Inbox",
            ),
        );
        // 计划之后：一项的源文件被人续写，一项被移走，一项没动。
        fs::write(&rewritten, b"%PDF-1.4 rewritten.pdf plus trailing bytes\n").expect("rewrite");
        fs::remove_file(&deleted).expect("delete source");
        let (batch, _) = start_planned(&fixture, "drift", &planned);
        assert_eq!(batch.state, BatchState::CompletedWithErrors);
        assert_eq!(
            outcomes(&fixture, &batch.id),
            vec![outcome(IMPORT_OUTCOME_CREATED), None, None]
        );
        assert_eq!(
            item_failure(&fixture, &batch.id, 1),
            Some(LibraryActErrorCode::SourceChanged)
        );
        assert_eq!(
            item_failure(&fixture, &batch.id, 2),
            Some(LibraryActErrorCode::SourceMissing)
        );
        assert_eq!(
            registered_papers(&fixture),
            vec!["Papers/Inbox/steady.pdf".to_string()]
        );

        // 重试按当前磁盘事实再预检一次：续写完成的那一项现在进得去，
        // 「源文件已被移走」的那一项不配自动重试。
        let retried = fixture
            .act_ok(&control("drift-retry", &batch.id, retry_of(None)))
            .batch()
            .clone();
        assert_eq!(retried.parent_batch_id.as_deref(), Some(batch.id.as_str()));
        assert_eq!(retried.state, BatchState::Completed);
        assert_eq!(retried.counts.succeeded, 1);
        // `target_key` 沿用父项那一个：任务中心才能把重试项对回它替代的那一项。
        assert_eq!(
            target_keys(&fixture, &retried.id),
            vec![target_keys(&fixture, &batch.id)[1].clone()]
        );
        assert_eq!(
            registered_papers(&fixture),
            vec![
                "Papers/Inbox/rewritten.pdf".to_string(),
                "Papers/Inbox/steady.pdf".to_string()
            ]
        );
    }

    #[test]
    fn a_crash_between_publish_and_registration_surfaces_as_a_conflict() {
        let fixture = fixture();
        let sources = tempfile::tempdir().expect("source directory");
        let alpha = pdf_source(sources.path(), "alpha.pdf");
        let planned = planned_batch(
            &fixture,
            &plan_import("publish-plan", &[alpha], "Papers/Inbox"),
        );
        // 伪造「文件已经重命名到位、登记还没提交」：目标路径上有文件，库里没有它的行。
        write_pdf(fixture.path(), "Papers/Inbox/alpha.pdf");
        let (batch, token) = start_planned(&fixture, "publish", &planned);
        // 既没写坏库也没多出一个 Paper：结论可读，撤销入口也不该凭空铸。
        assert_eq!(batch.state, BatchState::Completed);
        assert_eq!(
            outcomes(&fixture, &batch.id),
            vec![outcome(IMPORT_OUTCOME_CONFLICT)]
        );
        assert_eq!(registered_papers(&fixture), Vec::<String>::new());
        assert_eq!(
            open_conflict_kind(&fixture).as_deref(),
            Some("target_path_conflict")
        );
        assert!(
            token.is_none(),
            "a batch that created nothing owes no undo token"
        );
        // 库里那一份留在原处等人处理：导入绝不擅自删掉一个自己没有登记的文件。
        assert!(has_pdf(fixture.path(), "Papers/Inbox/alpha.pdf"));
    }

    #[test]
    fn a_killed_import_run_finishes_its_queue_without_importing_anything_twice() {
        let fixture = fixture();
        let sources = tempfile::tempdir().expect("source directory");
        let alpha = pdf_source(sources.path(), "alpha.pdf");
        let beta = pdf_source(sources.path(), "beta.pdf");
        let planned = planned_batch(
            &fixture,
            &plan_import("kill-plan", &[alpha.clone(), beta], "Papers/Inbox"),
        );
        // 伪造一次「进程在两项之间被杀」：第一项的效果已经落在文件系统与库里，
        // 但它的 Item 还停在 running，第二项刚被推成 queued。
        let module = crate::paper_module::PaperModule::open(fixture.path()).expect("module");
        module
            .import_pdf(Path::new(&alpha), Some("Papers/Inbox"))
            .expect("the first import landed");
        let timestamp = now();
        {
            let connection = fixture.connection();
            connection
                .execute(
                    "UPDATE library_batches SET state = 'running', started_at = ?2, updated_at = ?2 WHERE id = ?1",
                    params![planned.id, timestamp],
                )
                .expect("crash the batch");
            connection
                .execute(
                    "UPDATE library_batch_items SET state = 'running', started_at = ?2, updated_at = ?2
                     WHERE batch_id = ?1 AND ordinal = 0",
                    params![planned.id, timestamp],
                )
                .expect("crash the first item");
            connection
                .execute(
                    "UPDATE library_batch_items SET state = 'queued', updated_at = ?2
                     WHERE batch_id = ?1 AND ordinal = 1",
                    params![planned.id, timestamp],
                )
                .expect("queue the second item");
        }

        let response = fixture.act_ok(&start("kill-start", &planned, &[]));
        let resumed = response.batch().clone();
        assert_eq!(resumed.state, BatchState::InterruptedUnknown);
        assert_eq!(resumed.counts.interrupted_unknown, 1);
        assert_eq!(resumed.counts.succeeded, 1);
        // 续跑绝不重跑第一项，也不给它一次第二次执行：库里 alpha 只有一份。
        assert_eq!(
            registered_papers(&fixture),
            vec![
                "Papers/Inbox/alpha.pdf".to_string(),
                "Papers/Inbox/beta.pdf".to_string()
            ]
        );
        let again = fixture
            .act_ok(&start("kill-again", &planned, &[]))
            .batch()
            .clone();
        assert_eq!(again.id, resumed.id);
        assert_eq!(again.counts.succeeded, 1);
        assert_eq!(registered_papers(&fixture).len(), 2);

        // Token 只覆盖确证由本批新建的那一项：running 那一项连身份都没记下来。
        let token = undo_of(&response).expect("the resumed queue still owes an undo entry");
        let compensation = undo(&fixture, "kill", &resumed.id, &token);
        assert_eq!(
            target_keys(&fixture, &compensation.id),
            vec![target_keys(&fixture, &resumed.id)[1].clone()]
        );
        assert_eq!(
            registered_papers(&fixture),
            vec!["Papers/Inbox/alpha.pdf".to_string()]
        );
    }

    #[test]
    fn five_hundred_sources_are_accepted_and_run_through_one_queue() {
        let fixture = fixture();
        let sources = tempfile::tempdir().expect("source directory");
        let paths: Vec<String> = (0..MAX_BATCH_ITEMS)
            .map(|index| pdf_source(sources.path(), &format!("p{index}.pdf")))
            .collect();
        let planned = planned_batch(&fixture, &plan_import("cap-plan", &paths, "Papers/Inbox"));
        // 上限是「一项都不许被静默截断」，不是「取前 500 个」。
        assert_eq!(planned.total_items as usize, MAX_BATCH_ITEMS);
        let (batch, token) = start_planned(&fixture, "cap", &planned);
        assert_eq!(batch.state, BatchState::Completed);
        assert_eq!(batch.counts.succeeded, MAX_BATCH_ITEMS as i64);
        assert_eq!(registered_papers(&fixture).len(), MAX_BATCH_ITEMS);

        let mut over = paths.clone();
        over.push(pdf_source(sources.path(), "over.pdf"));
        assert_eq!(
            fixture.error(&plan_import("over-plan", &over, "Papers/Inbox")),
            LibraryActErrorCode::BatchTooLarge
        );
        assert_eq!(registered_papers(&fixture).len(), MAX_BATCH_ITEMS);

        // 500 项的撤销走的还是同一条队列。
        let token = token.expect("five hundred new Papers owe one undo entry");
        let compensation = undo(&fixture, "cap", &batch.id, &token);
        assert_eq!(compensation.total_items as usize, MAX_BATCH_ITEMS);
        assert_eq!(registered_papers(&fixture), Vec::<String>::new());
    }

    #[test]
    fn an_import_request_has_to_name_both_its_sources_and_its_collection() {
        let fixture = fixture();
        let sources = tempfile::tempdir().expect("source directory");
        let alpha = pdf_source(sources.path(), "alpha.pdf");
        // 目标目录不能猜：落在「全部」或 Smart Collection 上没有可推导的目录。
        assert_eq!(
            fixture.error(&plan_import("blank-dir", &[alpha.clone()], "   ")),
            LibraryActErrorCode::InvalidQuery
        );
        // 空选择与空白路径都是请求问题，不是一个空批次。
        assert_eq!(
            fixture.error(&plan_import("empty", &[], "Papers/Inbox")),
            LibraryActErrorCode::InvalidQuery
        );
        assert_eq!(
            fixture.error(&plan_import(
                "blank-path",
                &[String::from("  ")],
                "Papers/Inbox"
            )),
            LibraryActErrorCode::InvalidQuery
        );
        // 两种成员形态不可互换：猜一个形态就是把文件导进用户没选的目录，
        // 或把一批 Paper 当成磁盘路径。
        assert_eq!(
            fixture.error(&LibraryActRequest::PlanBatch {
                protocol_version: protocol(),
                idempotency_key: "mixed-command".to_string(),
                command: BatchCommand::Import {
                    collection_path: "Papers/Inbox".to_string(),
                },
                target: BatchTarget::Explicit {
                    paper_ids: names(&["paper-alpha"]),
                },
            }),
            LibraryActErrorCode::InvalidQuery
        );
        assert_eq!(
            fixture.error(&LibraryActRequest::PlanBatch {
                protocol_version: protocol(),
                idempotency_key: "mixed-target".to_string(),
                command: BatchCommand::Trash,
                target: BatchTarget::Sources { paths: vec![alpha] },
            }),
            LibraryActErrorCode::InvalidQuery
        );
        assert_eq!(registered_papers(&fixture), Vec::<String>::new());
    }

    // ---------------------------------------------------------------------
    // §7 Export 行：批量导出阅读成果包
    // ---------------------------------------------------------------------

    fn plan_export(key: &str, paper_ids: &[String]) -> LibraryActRequest {
        LibraryActRequest::PlanBatch {
            protocol_version: protocol(),
            idempotency_key: key.to_string(),
            command: BatchCommand::Export {
                locale: crate::ui_locale::UiLocale::ZhCn,
                format: ExportFormat::ReadingBundle,
            },
            target: BatchTarget::Explicit {
                paper_ids: paper_ids.to_vec(),
            },
        }
    }

    fn run_export(
        fixture: &Fixture,
        key: &str,
        paper_ids: &[String],
    ) -> (BatchProjection, Option<String>) {
        let planned = planned_batch(fixture, &plan_export(&format!("{key}-plan"), paper_ids));
        assert_eq!(planned.undo_policy, UndoPolicy::Compensating);
        start_planned(fixture, key, &planned)
    }

    /// `export/` 下那一份输出在不在：导出唯一的外在效果就是这个文件。
    fn export_exists(fixture: &Fixture, name: &str) -> bool {
        crate::export_module::output_on_disk(fixture.path(), &format!("export/{name}")).is_some()
    }

    fn export_body(fixture: &Fixture, name: &str) -> String {
        fs::read_to_string(fixture.path().join("export").join(name)).expect("export file")
    }

    fn write_export_file(fixture: &Fixture, name: &str, body: &str) {
        let directory = fixture.path().join("export");
        fs::create_dir_all(&directory).expect("export directory");
        fs::write(directory.join(name), body).expect("export file");
    }

    #[test]
    fn an_export_batch_writes_one_file_per_paper_and_undo_removes_them() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let before = fixture.revisions();
        let planned = planned_batch(&fixture, &plan_export("export-plan", &papers));
        assert_eq!(planned.total_items, 3);
        // 计划一个字节都不写：预览不该在库里留下一个看起来已经导出的文件。
        assert!(!fixture.path().join("export").exists());
        assert_eq!(
            plan_json(&fixture, &planned.id, 0)
                .get("plannedRelativePath")
                .and_then(Value::as_str),
            Some("export/alpha.md")
        );

        let (batch, token) = start_planned(&fixture, "export", &planned);
        assert_eq!(batch.state, BatchState::Completed);
        assert_eq!(batch.counts.succeeded, 3);
        assert_eq!(
            outcomes(&fixture, &batch.id),
            vec![outcome(EXPORT_OUTCOME_WRITTEN); 3]
        );
        for label in ["alpha", "beta", "gamma"] {
            assert!(export_exists(&fixture, &format!("{label}.md")));
        }
        assert!(export_body(&fixture, "alpha.md").contains("# alpha"));
        // 导出不动库里的任何事实：一个域都不该因此重新拉一遍。
        assert_eq!(fixture.revisions(), before);

        let token = token.expect("three new files owe one undo entry");
        let compensation = undo(&fixture, "export", &batch.id, &token);
        assert_eq!(
            compensation.parent_command_kind,
            Some(BatchCommandKind::Export)
        );
        assert_eq!(compensation.state, BatchState::Completed);
        for label in ["alpha", "beta", "gamma"] {
            assert!(!export_exists(&fixture, &format!("{label}.md")));
        }
    }

    #[test]
    fn re_exporting_the_same_paper_is_an_overwrite_that_needs_confirming_and_owes_no_undo() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let alpha = vec![papers[0].clone()];
        let (first, _) = run_export(&fixture, "again-first", &alpha);
        // 第一次没有任何不可逆影响：新建的输出既要能删回去，也不必先确认什么。
        assert_eq!(first.requirements, Vec::new());
        assert!(export_exists(&fixture, "alpha.md"));

        let second = planned_batch(&fixture, &plan_export("again-plan", &alpha));
        assert_eq!(second.requirements.len(), 1);
        assert_eq!(second.requirements[0].id, REQUIREMENT_OVERWRITE);
        assert_eq!(second.requirements[0].kind, BatchRequirementKind::Overwrite);
        // 被盖掉的那一份回不来，所以确认必须发生在写之前，且要逐字回报 id。
        assert_eq!(
            fixture.error(&start("again-refuse", &second, &[])),
            LibraryActErrorCode::ConfirmationRequired
        );
        // 用户在这一份输出上写过东西：命名规则认的是那行 sha，所以它仍然算同一份导出。
        write_export_file(
            &fixture,
            "alpha.md",
            &format!(
                "# alpha\n\n手写的一版\n- SHA-256：{}\n",
                sha_of(&fixture, &papers[0])
            ),
        );

        let started = fixture.act_ok(&start("again-start", &second, &[REQUIREMENT_OVERWRITE]));
        let batch = started.batch().clone();
        assert_eq!(batch.state, BatchState::Completed);
        assert_eq!(
            outcomes(&fixture, &batch.id),
            vec![outcome(EXPORT_OUTCOME_OVERWRITTEN)]
        );
        assert!(!export_body(&fixture, "alpha.md").contains("手写"));
        // 覆盖掉的那一份不是这个批次写的：删它才是损失，所以这一批不配撤销入口。
        assert_eq!(undo_of(&started), None);
        assert!(export_exists(&fixture, "alpha.md"));
    }

    #[test]
    fn export_undo_only_deletes_the_file_the_batch_wrote() {
        let fixture = fixture();
        let papers = seed_two(&fixture);
        let (batch, token) = run_export(&fixture, "kept", &papers);
        let token = token.expect("two new files owe one undo entry");
        // 用户自己删掉了 alpha 的输出：要达成的结果本来就成立，那一项该是 skipped。
        fs::remove_file(fixture.path().join("export/alpha.md")).expect("remove output");
        // beta 的输出被人改过：那已经不是本批写下的文件了，宁可什么都不删。
        write_export_file(&fixture, "beta.md", "# beta\n\n手写的一版\n");

        let compensation = undo(&fixture, "kept", &batch.id, &token);
        assert_eq!(compensation.state, BatchState::CompletedWithErrors);
        assert_eq!(
            item_states(&fixture, &compensation.id),
            vec![ItemState::Skipped, ItemState::Failed]
        );
        assert_eq!(
            item_failure(&fixture, &compensation.id, 1),
            Some(LibraryActErrorCode::UndoConflict)
        );
        assert_eq!(export_body(&fixture, "beta.md"), "# beta\n\n手写的一版\n");
        assert!(!export_exists(&fixture, "alpha.md"));
    }

    #[test]
    fn a_paper_that_left_the_library_fails_only_its_own_export_item() {
        let fixture = fixture();
        let papers = seed_two(&fixture);
        let planned = planned_batch(&fixture, &plan_export("left-plan", &papers));
        // 计划之后、Start 之前有人把 alpha 移进了回收站：那只停掉这一项。
        fixture
            .connection()
            .execute(
                "UPDATE papers SET deleted_at = ?2 WHERE id = ?1",
                params![papers[0], now()],
            )
            .expect("trash alpha");

        let (batch, token) = start_planned(&fixture, "left", &planned);
        assert_eq!(batch.state, BatchState::CompletedWithErrors);
        assert_eq!(
            item_states(&fixture, &batch.id),
            vec![ItemState::Failed, ItemState::Succeeded]
        );
        assert_eq!(
            item_failure(&fixture, &batch.id, 0),
            Some(LibraryActErrorCode::PaperNotFound)
        );
        // 局部失败不许丢掉成功结果：beta 那份文件照样写出来了。
        assert_eq!(
            outcomes(&fixture, &batch.id),
            vec![None, outcome(EXPORT_OUTCOME_WRITTEN)]
        );
        assert!(!export_exists(&fixture, "alpha.md"));
        assert!(export_exists(&fixture, "beta.md"));
        let token = token.expect("the one file this batch wrote still owes an undo");
        undo(&fixture, "left", &batch.id, &token);
        assert!(!export_exists(&fixture, "beta.md"));
    }

    #[test]
    fn recent_batches_lists_newest_first_and_reconciles_before_projecting() {
        let fixture = fixture();
        let papers = seed_three(&fixture);
        let older = run(&fixture, "list-old", &papers, &["reviewed"]).0;
        let newer = run(&fixture, "list-new", &papers, &["done"]).0;
        let LibraryReadResult::RecentBatches { page } =
            fixture.read(&LibraryReadRequest::RecentBatches {
                protocol_version: protocol(),
                limit: Some(10),
            })
        else {
            panic!("expected recent_batches");
        };
        assert_eq!(page.page_size, 10);
        assert_eq!(page.batches.len(), 2);
        assert_eq!(page.batches[0].id, newer.id);
        assert_eq!(page.batches[1].id, older.id);
        assert_eq!(page.batches[0].state, BatchState::Completed);
        let oversize = crate::library_query::read(
            fixture.path(),
            &LibraryReadRequest::RecentBatches {
                protocol_version: protocol(),
                limit: Some(101),
            },
        )
        .expect_err("oversize recent_batches is invalid_query");
        assert_eq!(oversize.code, LibraryQueryErrorCode::InvalidQuery);
    }

    #[test]
    fn ocr_plan_freezes_handles_without_creating_jobs() {
        let fixture = fixture();
        let papers = seed_two(&fixture);
        let provider = ocr_provider();
        let planned = fixture
            .act_provider_ok(&plan_ocr("ocr-plan", &papers), &provider)
            .batch()
            .clone();
        assert_eq!(planned.command_kind, BatchCommandKind::Ocr);
        assert_eq!(planned.undo_policy, UndoPolicy::CancelOnly);
        assert_eq!(planned.cost_preview.marginal_estimates.len(), 1);
        assert_eq!(planned.cost_preview.marginal_estimates[0].minimum, "0.024");
        assert!(planned
            .requirements
            .iter()
            .any(|item| item.kind == BatchRequirementKind::Cost));
        assert_eq!(jobs_count(&fixture), 0);
        let handles: i64 = fixture
            .connection()
            .query_row("SELECT COUNT(*) FROM job_preparations", [], |row| {
                row.get(0)
            })
            .expect("handles");
        assert_eq!(handles, 2);
    }

    #[test]
    fn ocr_without_route_fails_closed() {
        let fixture = fixture();
        let papers = seed_two(&fixture);
        let error = fixture
            .act_provider(
                &plan_ocr("ocr-missing", &papers),
                &ProviderActContext::default(),
            )
            .expect_err("missing mistral");
        assert_eq!(error.code, LibraryActErrorCode::ProviderRouteUnavailable);
        assert_eq!(jobs_count(&fixture), 0);
    }

    #[test]
    fn ocr_start_enqueues_jobs_and_keeps_items_queued() {
        let fixture = fixture();
        let papers = seed_two(&fixture);
        let provider = ocr_provider();
        let planned = fixture
            .act_provider_ok(&plan_ocr("ocr-start-plan", &papers), &provider)
            .batch()
            .clone();
        let started = start_provider(&fixture, "ocr-start", &planned, &provider);
        let batch = started.batch();
        assert_eq!(batch.state, BatchState::Queued);
        assert_eq!(batch.counts.queued, 2);
        assert_eq!(jobs_count(&fixture), 2);
        let created: i64 = fixture
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM library_batch_job_links WHERE ownership = 'created'",
                [],
                |row| row.get(0),
            )
            .expect("created owners");
        assert_eq!(created, 2);
    }

    #[test]
    fn existing_ocr_is_skipped_and_second_batch_joins_the_job() {
        let fixture = fixture();
        let papers = seed_two(&fixture);
        attach_ocr(&fixture, &papers[0]);
        let provider = ocr_provider();
        let planned = fixture
            .act_provider_ok(&plan_ocr("ocr-skip-plan", &papers), &provider)
            .batch()
            .clone();
        assert_eq!(planned.counts.skipped, 1);
        assert_eq!(planned.counts.planned, 1);
        let started = start_provider(&fixture, "ocr-skip-start", &planned, &provider);
        assert_eq!(started.batch().counts.skipped, 1);
        assert_eq!(jobs_count(&fixture), 1);

        let again = fixture
            .act_provider_ok(&plan_ocr("ocr-join-plan", &[papers[1].clone()]), &provider)
            .batch()
            .clone();
        let joined = start_provider(&fixture, "ocr-join-start", &again, &provider);
        assert_eq!(jobs_count(&fixture), 1);
        assert_eq!(joined.batch().cost_preview.joined_existing_job_count, 1);
        let created: i64 = fixture
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM library_batch_job_links WHERE ownership = 'created'",
                [],
                |row| row.get(0),
            )
            .expect("created");
        let joined_links: i64 = fixture
            .connection()
            .query_row(
                "SELECT COUNT(*) FROM library_batch_job_links WHERE ownership = 'joined'",
                [],
                |row| row.get(0),
            )
            .expect("joined");
        assert_eq!(created, 1);
        assert_eq!(joined_links, 1);
    }

    #[test]
    fn switching_ocr_key_at_start_is_stale() {
        let fixture = fixture();
        let papers = seed_two(&fixture);
        let original = ocr_provider();
        let planned = fixture
            .act_provider_ok(&plan_ocr("ocr-key-plan", &papers), &original)
            .batch()
            .clone();
        let changed = ProviderActContext {
            ocr: Some(
                crate::provider_routing::capture_mistral_ocr_route(
                    "other-mistral-key",
                    "mistral-ocr-latest",
                )
                .expect("changed"),
            ),
            paper: None,
            orientation_prompt: None,
            ..Default::default()
        };
        let accepted: Vec<String> = accepted_ids(&planned);
        let accepted_refs: Vec<&str> = accepted.iter().map(String::as_str).collect();
        let mut ceilings = BTreeMap::new();
        for estimate in &planned.cost_preview.marginal_estimates {
            ceilings.insert(estimate.currency.clone(), estimate.maximum.clone());
        }
        let error = fixture
            .act_provider(
                &start_with_ceilings("ocr-key-start", &planned, &accepted_refs, ceilings),
                &changed,
            )
            .expect_err("key change");
        assert_eq!(error.code, LibraryActErrorCode::StaleBatchPlan);
        assert_eq!(jobs_count(&fixture), 0);
    }

    #[test]
    fn a_lower_cost_ceiling_forces_a_new_preview() {
        let fixture = fixture();
        let papers = seed_two(&fixture);
        let provider = ocr_provider();
        let planned = fixture
            .act_provider_ok(&plan_ocr("ocr-ceil-plan", &papers), &provider)
            .batch()
            .clone();
        let accepted: Vec<String> = accepted_ids(&planned);
        let accepted_refs: Vec<&str> = accepted.iter().map(String::as_str).collect();
        let mut ceilings = BTreeMap::new();
        ceilings.insert("USD".to_string(), "0.001".to_string());
        let error = fixture
            .act_provider(
                &start_with_ceilings("ocr-ceil-start", &planned, &accepted_refs, ceilings),
                &provider,
            )
            .expect_err("ceiling");
        assert_eq!(error.code, LibraryActErrorCode::StaleBatchPlan);
    }

    #[test]
    fn long_pdf_is_a_start_requirement() {
        let fixture = fixture();
        let papers = seed_two(&fixture);
        set_page_count(&fixture, &papers[0], 80);
        let provider = ocr_provider();
        let planned = fixture
            .act_provider_ok(&plan_ocr("ocr-long-plan", &papers), &provider)
            .batch()
            .clone();
        assert!(planned
            .requirements
            .iter()
            .any(|item| item.kind == BatchRequirementKind::LongPdf));
    }

    #[test]
    fn completing_the_job_succeeds_the_batch_item() {
        let fixture = fixture();
        let papers = seed_two(&fixture);
        let provider = ocr_provider();
        let planned = fixture
            .act_provider_ok(
                &plan_ocr("ocr-complete-plan", &[papers[0].clone()]),
                &provider,
            )
            .batch()
            .clone();
        start_provider(&fixture, "ocr-complete-start", &planned, &provider);
        let jobs = crate::job_module::JobModule::open(fixture.db_path()).expect("jobs");
        let claimed = jobs.claim_next_record().expect("claim").expect("job");
        jobs.complete(&claimed.id).expect("complete");
        let batch = batch_projection(&fixture.connection(), &planned.id).expect("batch");
        assert_eq!(batch.state, BatchState::Completed);
        assert_eq!(batch.counts.succeeded, 1);
    }

    #[test]
    fn cancel_remaining_cancels_a_unique_created_job() {
        let fixture = fixture();
        let papers = seed_two(&fixture);
        let provider = ocr_provider();
        let planned = fixture
            .act_provider_ok(
                &plan_ocr("ocr-cancel-plan", &[papers[0].clone()]),
                &provider,
            )
            .batch()
            .clone();
        start_provider(&fixture, "ocr-cancel-start", &planned, &provider);
        fixture.act_provider_ok(
            &control("ocr-cancel", &planned.id, BatchControl::CancelRemaining),
            &provider,
        );
        let state: String = fixture
            .connection()
            .query_row("SELECT state FROM jobs", [], |row| row.get(0))
            .expect("job state");
        assert_eq!(state, "cancelled");
        let batch = batch_projection(&fixture.connection(), &planned.id).expect("batch");
        assert_eq!(batch.state, BatchState::Cancelled);
    }

    #[test]
    fn cancel_remaining_does_not_cancel_a_shared_job() {
        let fixture = fixture();
        let papers = seed_two(&fixture);
        let provider = ocr_provider();
        let first = fixture
            .act_provider_ok(&plan_ocr("ocr-share-a", &[papers[0].clone()]), &provider)
            .batch()
            .clone();
        start_provider(&fixture, "ocr-share-a-start", &first, &provider);
        let second = fixture
            .act_provider_ok(&plan_ocr("ocr-share-b", &[papers[0].clone()]), &provider)
            .batch()
            .clone();
        start_provider(&fixture, "ocr-share-b-start", &second, &provider);
        fixture.act_provider_ok(
            &control(
                "ocr-share-cancel",
                &second.id,
                BatchControl::CancelRemaining,
            ),
            &provider,
        );
        let state: String = fixture
            .connection()
            .query_row("SELECT state FROM jobs", [], |row| row.get(0))
            .expect("job");
        assert_eq!(state, "queued");
    }

    #[test]
    fn brief_without_ocr_is_skipped_and_custom_endpoint_is_unknown() {
        let fixture = fixture();
        let papers = seed_two(&fixture);
        attach_ocr(&fixture, &papers[0]);
        let provider = paper_provider();
        let planned = fixture
            .act_provider_ok(&plan_brief("brief-plan", &papers), &provider)
            .batch()
            .clone();
        assert_eq!(planned.counts.skipped, 1);
        assert_eq!(planned.counts.planned, 1);
        assert!(planned.cost_preview.unknown_item_count >= 1);
        assert!(planned
            .cost_preview
            .unknown_reason_codes
            .iter()
            .any(|code| code == "custom_endpoint_unpriced"));
        assert!(planned
            .requirements
            .iter()
            .any(|item| item.kind == BatchRequirementKind::UnknownCost));
    }

    #[test]
    fn provider_batches_are_not_reversible() {
        let fixture = fixture();
        let papers = seed_two(&fixture);
        let provider = ocr_provider();
        let planned = fixture
            .act_provider_ok(&plan_ocr("ocr-undo-plan", &[papers[0].clone()]), &provider)
            .batch()
            .clone();
        let started = start_provider(&fixture, "ocr-undo-start", &planned, &provider);
        assert!(undo_of(&started).is_none());
        let error = fixture
            .act_provider(
                &control(
                    "ocr-undo",
                    &planned.id,
                    BatchControl::Undo {
                        token: "nope".to_string(),
                    },
                ),
                &provider,
            )
            .expect_err("no undo");
        assert!(matches!(
            error.code,
            LibraryActErrorCode::UndoConflict | LibraryActErrorCode::NotReversible
        ));
    }
}
