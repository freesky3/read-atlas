//! D-063 PR 1：Library Workspace 读取侧的 `read` 入口（计划 §4.2 / §10.1）。
//!
//! `library_read` 是 Hub 唯一要学习的 seam：一次有界读取返回一页卡片、`totalCount`、
//! `queryDigest` 与依赖 revision 向量。投影里只出现相对路径与脱敏后的摘要，绝不出现
//! 绝对路径、Key、route 或 Provider 原文（§10.2）。

use crate::library_batch::ItemState;
use crate::library_paths::{self, DocumentKind, PAPERS_DIR};
use crate::library_workflow::{library_revisions, DomainRevision, LibraryDomain};
use crate::{in_placeholders, load_paper_card_extras, open_db, HUB_PROJECTION_CHUNK};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use rusqlite::{params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::Path;

/// §4.2：每个 `read` 请求都带协议版本，不匹配的调用直接拒绝，不做「猜意思」。
pub(crate) const LIBRARY_PROTOCOL_VERSION: u32 = 1;
pub(crate) const HUB_DEFAULT_PAGE_SIZE: i64 = 100;
pub(crate) const HUB_MAX_PAGE_SIZE: i64 = 200;
const MAX_FILTERS: usize = 8;
const MAX_TEXT_TERM: usize = 200;
const MAX_TAG_NAME: usize = 64;
/// §5.1：时间锚点由后端统一给出。当前读侧固定 UTC，PR 4 的相对日期谓词在**前端**
/// 按用户时区渲染，快照里存的仍是这份锚点，避免确认瞬间悄悄换成员。
pub(crate) const EVALUATION_TIMEZONE: &str = "UTC";
/// `printf('%010d', …)`：固定宽度的零填充让文本键的字典序等于数值序。
const KEY_DIGITS: usize = 10;
const YEAR_SHIFT: i64 = 2_147_483_648;

/// `hub_page` 的真实依赖域。卡片现在带 lifecycle / engagement，这两域的写必须让 Hub 失效。
pub(crate) const HUB_PAGE_DEPENDENCIES: [LibraryDomain; 7] = [
    LibraryDomain::Structure,
    LibraryDomain::Tags,
    LibraryDomain::Lifecycle,
    LibraryDomain::Engagement,
    LibraryDomain::Artifacts,
    LibraryDomain::Jobs,
    LibraryDomain::Sort,
];

pub(crate) type LibraryQueryResult<T> = Result<T, LibraryQueryError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryQueryErrorCode {
    /// 请求本身不合法：未知字段、越界分页、AST 之外的筛选项、被篡改的 cursor。
    InvalidQuery,
    /// Workspace 尚未打开或数据库不可读。
    WorkspaceUnavailable,
}

/// §10.2：错误也是 typed projection，不把 SQLite 原文交给 UI。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryQueryError {
    pub code: LibraryQueryErrorCode,
    pub message: String,
}

impl LibraryQueryError {
    fn invalid(message: impl Into<String>) -> Self {
        Self {
            code: LibraryQueryErrorCode::InvalidQuery,
            message: message.into(),
        }
    }

    pub(crate) fn unavailable(message: impl Into<String>) -> Self {
        Self {
            code: LibraryQueryErrorCode::WorkspaceUnavailable,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum LibraryReadRequest {
    #[serde(rename = "hub_page", rename_all = "camelCase")]
    HubPage {
        protocol_version: u32,
        page: HubPageQuery,
    },
    #[serde(rename = "watch_handshake", rename_all = "camelCase")]
    WatchHandshake {
        protocol_version: u32,
        after: Option<i64>,
    },
    /// §10.1：任务中心读单个批次投影。投影住在 `library_batch`，读仍然走这个入口。
    #[serde(rename = "batch", rename_all = "camelCase")]
    Batch {
        protocol_version: u32,
        batch_id: String,
    },
    #[serde(rename = "batch_items", rename_all = "camelCase")]
    BatchItems {
        protocol_version: u32,
        batch_id: String,
        /// ordinal 键集分页：`nextOrdinal` 之后的一页。
        after_ordinal: Option<i64>,
        states: Option<Vec<ItemState>>,
        limit: Option<i64>,
    },
    /// §6.1：任务中心批次分组。按 `updated_at` 倒序，缺省 50 / 上限 100。
    #[serde(rename = "recent_batches", rename_all = "camelCase")]
    RecentBatches {
        protocol_version: u32,
        limit: Option<i64>,
    },
    #[serde(rename = "smart_collections", rename_all = "camelCase")]
    SmartCollections { protocol_version: u32 },
    #[serde(rename = "reading_context", rename_all = "camelCase")]
    ReadingContext {
        protocol_version: u32,
        paper_id: String,
        revision_id: String,
    },
    /// 物理叶子 collection 的全部 live id，供 D-062 精确置换。不带卡片投影。
    #[serde(rename = "collection_layer", rename_all = "camelCase")]
    CollectionLayer {
        protocol_version: u32,
        collection_path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum LibraryReadResult {
    #[serde(rename = "hub_page", rename_all = "camelCase")]
    HubPage { page: HubPageResult },
    #[serde(rename = "watch_handshake", rename_all = "camelCase")]
    WatchHandshake {
        current_revision: i64,
        invalidated: bool,
    },
    #[serde(rename = "batch", rename_all = "camelCase")]
    Batch {
        batch: crate::library_batch::BatchProjection,
    },
    #[serde(rename = "batch_items", rename_all = "camelCase")]
    BatchItems {
        page: crate::library_batch::BatchItemsPage,
    },
    #[serde(rename = "recent_batches", rename_all = "camelCase")]
    RecentBatches {
        page: crate::library_batch::RecentBatchesPage,
    },
    #[serde(rename = "smart_collections", rename_all = "camelCase")]
    SmartCollections {
        collections: Vec<crate::library_lifecycle::SmartCollectionProjection>,
    },
    #[serde(rename = "reading_context", rename_all = "camelCase")]
    ReadingContext {
        context: crate::library_lifecycle::ReadingContextProjection,
    },
    #[serde(rename = "collection_layer", rename_all = "camelCase")]
    CollectionLayer {
        collection_id: Option<String>,
        paper_ids: Vec<String>,
    },
}

/// `deny_unknown_fields` 是有意的：拼错的筛选字段必须失败。静默忽略等于把一次
/// `all_matching` 快照悄悄放大成整个文库。
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HubPageQuery {
    #[serde(default)]
    pub filters: Vec<QueryFilter>,
    #[serde(default)]
    pub sort: HubSort,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction: Option<SortDirection>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HubSort {
    #[default]
    Recent,
    Year,
    Title,
    Manual,
    LastOpened,
    Chapter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    Desc,
}
impl HubPageQuery {
    fn descending(&self) -> bool {
        self.direction
            .map(|d| d == SortDirection::Desc)
            .unwrap_or_else(|| self.sort.descending())
    }
}

impl HubSort {
    fn descending(self) -> bool {
        match self {
            Self::Recent | Self::Year | Self::LastOpened => true,
            Self::Title | Self::Manual | Self::Chapter => false,
        }
    }

    /// 白名单 SQL 片段：只出现常量列名或已注册的确定性函数，用户输入走绑定变量。
    fn key_expression(self) -> String {
        match self {
            Self::Recent => "p.created_at".to_string(),
            Self::Year => format!(
                "printf('%0{KEY_DIGITS}d', COALESCE(CASE WHEN json_extract(m.metadata_json, '$._format') = 'auxiliary-v2' THEN json_extract(m.metadata_json, '$._display.year') ELSE m.publication_year END, -{YEAR_SHIFT}) + {YEAR_SHIFT})"
            ),
            Self::Title => "lower(COALESCE(NULLIF(m.title, ''), p.file_name))".to_string(),
            Self::Manual => format!(
                "printf('%0{KEY_DIGITS}d', COALESCE((SELECT o.position FROM collection_paper_order o \
                 WHERE o.collection_id = p.collection_id AND o.paper_id = p.id), {}))",
                i64::MAX
            ),
            Self::LastOpened => "COALESCE(eng.last_opened_at, '')".to_string(),
            Self::Chapter => {
                "chapter_sort_key((SELECT CASE WHEN json_extract(a.content_json, '$._format') = 'auxiliary-v2' THEN json_extract(a.content_json, '$._display.chapterNumber') ELSE json_extract(a.content_json, '$.chapterNumber') END
                    FROM artifacts a
                    JOIN artifact_heads ah ON ah.artifact_id = a.id
                    WHERE ah.paper_id = p.id AND ah.kind = 'metadata' AND ah.object_key = ''
                    LIMIT 1))"
                    .to_string()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind", deny_unknown_fields)]
pub enum QueryFilter {
    #[serde(rename_all = "camelCase")]
    Collection { path: String, recursive: bool },
    #[serde(rename_all = "camelCase")]
    Text { term: String },
    #[serde(rename_all = "camelCase")]
    Tag { tag: String },
    #[serde(rename = "document", rename_all = "camelCase")]
    Kind { value: DocumentKind },
    #[serde(rename_all = "camelCase")]
    Status { value: String },
    #[serde(rename_all = "camelCase")]
    Favorite { value: bool },
    #[serde(rename = "read_later", rename_all = "camelCase")]
    ReadLater { value: bool },
    #[serde(rename = "review_due")]
    ReviewDue,
    #[serde(rename_all = "camelCase")]
    Imported { relative: String },
    #[serde(rename = "ocr_failed")]
    OcrFailed,
    #[serde(rename = "opened")]
    Opened,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CursorToken {
    digest: String,
    key: String,
    id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubPaperCard {
    pub id: String,
    pub revision_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub publication_year: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_metadata: Option<serde_json::Value>,
    pub page_count: Option<i64>,
    pub collection_path: String,
    pub relative_path: String,
    pub file_name: String,
    pub kind: String,
    pub sha256: String,
    pub byte_size: i64,
    pub imported_at: String,
    pub tags: Vec<String>,
    pub has_ocr: bool,
    pub brief_status: String,
    pub brief_takeaway: Option<String>,
    pub keywords: Vec<String>,
    pub chapter_number: Option<String>,
    /// evaluation anchor：本页最后一行的排序键，与 `nextCursor` 同源。
    pub sort_key: String,
    pub lifecycle_status: String,
    pub favorite: bool,
    pub priority: i64,
    pub read_later: bool,
    pub review_at: Option<String>,
    pub lifecycle_version: i64,
    pub furthest_page: Option<i64>,
    pub last_opened_at: Option<String>,
    pub ocr_failed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HubPageResult {
    pub protocol_version: u32,
    pub revision: i64,
    pub dependencies: Vec<DomainRevision>,
    pub dependency_revision: String,
    pub query_digest: String,
    pub sort: HubSort,
    pub page_size: i64,
    pub total_count: i64,
    pub papers: Vec<HubPaperCard>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    /// §5.1 Selection Snapshot：成员摘要只覆盖 filters——排序、页宽和 cursor
    /// 都不改变「这一屏之外还有谁」。用 `queryDigest` 当快照键会把换排序
    /// 误判成选择漂移。
    pub selection_digest: String,
    pub selection_dependencies: Vec<DomainRevision>,
    /// 相对时间谓词的锚点由后端给出：不能等用户点确认时再换成员。
    /// `this_week` 按这个时刻所在周的周一 00:00 UTC 求值，不在保存查询时固化。
    pub evaluation_anchor: String,
    pub evaluation_timezone: String,
}

pub(crate) fn read(
    root: &Path,
    request: &LibraryReadRequest,
) -> LibraryQueryResult<LibraryReadResult> {
    match request {
        LibraryReadRequest::HubPage {
            protocol_version,
            page,
        } => {
            check_protocol(*protocol_version)?;
            let connection = connect(root)?;
            let (page, _stats) = hub_page_on_connection(&connection, page)?;
            Ok(LibraryReadResult::HubPage { page })
        }
        LibraryReadRequest::WatchHandshake {
            protocol_version,
            after,
        } => {
            check_protocol(*protocol_version)?;
            let connection = connect(root)?;
            // revision 计数器读不到是 Workspace 状态问题，不是请求不合法：分类错了
            // 会让调用方去改一个没毛病的查询。
            let revisions =
                library_revisions(&connection).map_err(LibraryQueryError::unavailable)?;
            // §10.3：`after >= current` 才允许保留旧快照；缺少 `after` 的首次握手
            // 一律按失效处理，逼调用方先 read。
            Ok(LibraryReadResult::WatchHandshake {
                current_revision: revisions.global,
                invalidated: !after.is_some_and(|after| after >= revisions.global),
            })
        }
        LibraryReadRequest::Batch {
            protocol_version,
            batch_id,
        } => {
            check_protocol(*protocol_version)?;
            let connection = connect(root)?;
            let batch =
                crate::library_batch::batch_projection(&connection, batch_id).map_err(from_act)?;
            Ok(LibraryReadResult::Batch { batch })
        }
        LibraryReadRequest::BatchItems {
            protocol_version,
            batch_id,
            after_ordinal,
            states,
            limit,
        } => {
            check_protocol(*protocol_version)?;
            let connection = connect(root)?;
            let page = crate::library_batch::batch_items_page(
                &connection,
                batch_id,
                *after_ordinal,
                states.as_deref(),
                *limit,
            )
            .map_err(from_act)?;
            Ok(LibraryReadResult::BatchItems { page })
        }
        LibraryReadRequest::RecentBatches {
            protocol_version,
            limit,
        } => {
            check_protocol(*protocol_version)?;
            let connection = connect(root)?;
            let page =
                crate::library_batch::recent_batches(&connection, *limit).map_err(from_act)?;
            Ok(LibraryReadResult::RecentBatches { page })
        }
        LibraryReadRequest::SmartCollections { protocol_version } => {
            check_protocol(*protocol_version)?;
            let connection = connect(root)?;
            let collections = crate::library_lifecycle::list_smart_collections(&connection)
                .map_err(from_lifecycle)?;
            Ok(LibraryReadResult::SmartCollections { collections })
        }
        LibraryReadRequest::ReadingContext {
            protocol_version,
            paper_id,
            revision_id,
        } => {
            check_protocol(*protocol_version)?;
            let connection = connect(root)?;
            let context =
                crate::library_lifecycle::reading_context(&connection, paper_id, revision_id)
                    .map_err(from_lifecycle)?;
            Ok(LibraryReadResult::ReadingContext { context })
        }
        LibraryReadRequest::CollectionLayer {
            protocol_version,
            collection_path,
        } => {
            check_protocol(*protocol_version)?;
            let connection = connect(root)?;
            collection_layer_on_connection(&connection, collection_path)
        }
    }
}

fn collection_layer_on_connection(
    connection: &Connection,
    collection_path: &str,
) -> LibraryQueryResult<LibraryReadResult> {
    let normalized = normalize_collection_path(collection_path)?;
    let collection_id: Option<String> = connection
        .query_row(
            "SELECT id FROM collections WHERE relative_path = ?1",
            rusqlite::params![normalized],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| LibraryQueryError::unavailable(error.to_string()))?;
    let Some(collection_id) = collection_id else {
        return Ok(LibraryReadResult::CollectionLayer {
            collection_id: None,
            paper_ids: Vec::new(),
        });
    };
    let mut statement = connection
        .prepare(
            "SELECT p.id FROM papers p
             LEFT JOIN collection_paper_order o
               ON o.collection_id = p.collection_id AND o.paper_id = p.id
             WHERE p.collection_id = ?1 AND p.deleted_at IS NULL
             ORDER BY COALESCE(o.position, 9223372036854775807) ASC, p.created_at ASC, p.id ASC",
        )
        .map_err(|error| LibraryQueryError::unavailable(error.to_string()))?;
    let mapped = statement
        .query_map(rusqlite::params![collection_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| LibraryQueryError::unavailable(error.to_string()))?;
    let mut paper_ids = Vec::new();
    for row in mapped {
        paper_ids.push(row.map_err(|error| LibraryQueryError::unavailable(error.to_string()))?);
    }
    Ok(LibraryReadResult::CollectionLayer {
        collection_id: Some(collection_id),
        paper_ids,
    })
}

fn from_lifecycle(error: crate::library_lifecycle::LifecycleError) -> LibraryQueryError {
    match error.code {
        "invalid_query" | "paper_not_found" => LibraryQueryError::invalid(error.message),
        _ => LibraryQueryError::unavailable(error.message),
    }
}

/// 批次投影的失败也要保持 typed，不能把 `act` 的错误码当成读侧的第三种形状。
/// 读侧只有两种可能：这个请求本身不合法（含「批次不存在」），或者库读不出来。
fn from_act(error: crate::library_batch::LibraryActError) -> LibraryQueryError {
    match error.code {
        crate::library_batch::LibraryActErrorCode::InvalidQuery
        | crate::library_batch::LibraryActErrorCode::BatchNotFound => {
            LibraryQueryError::invalid(error.message)
        }
        _ => LibraryQueryError::unavailable(error.message),
    }
}

fn connect(root: &Path) -> LibraryQueryResult<Connection> {
    open_db(root).map_err(LibraryQueryError::unavailable)
}

fn check_protocol(protocol_version: u32) -> LibraryQueryResult<()> {
    if protocol_version != LIBRARY_PROTOCOL_VERSION {
        return Err(LibraryQueryError::invalid(format!(
            "unsupported protocolVersion {protocol_version}, expected {LIBRARY_PROTOCOL_VERSION}"
        )));
    }
    Ok(())
}

/// 一次 Hub 读的全部数据库访问：一条分页语句、一条 `totalCount` 语句，加上按 id
/// 分块的派生字段语句。整页共用同一个连接，逐 Paper 的 `open_db` 在这里不可能发生。
pub(crate) fn hub_page_on_connection(
    connection: &Connection,
    query: &HubPageQuery,
) -> LibraryQueryResult<(HubPageResult, crate::HubProjectionStats)> {
    let page_size = resolve_page_size(query.limit)?;
    validate_query(query)?;
    let digest = query_digest(query, page_size);
    let mut key_sql = query.sort.key_expression();
    if query.sort == HubSort::Year && !query.descending() {
        key_sql = key_sql.replace(&format!(", -{YEAR_SHIFT})"), &format!(", {YEAR_SHIFT})"));
    }
    if query.sort == HubSort::Chapter && query.descending() {
        key_sql = format!("CASE WHEN ({key_sql}) LIKE '~%' THEN '' ELSE ({key_sql}) END");
    }
    let order = if query.descending() { "DESC" } else { "ASC" };
    let step = if query.descending() { "<" } else { ">" };

    // 分页投影与 Selection Snapshot 必须走同一份 filters 展开，否则「页上看到的」
    // 和「批量动作冻住的」就不是同一批 Paper。
    let evaluation_anchor = crate::now();
    let (filter_sql, mut bind) = selection_clause(&query.filters, &evaluation_anchor)?;

    const FROM_SQL: &str = " FROM papers p
             JOIN paper_heads h ON h.paper_id = p.id
             JOIN document_revisions r ON r.id = h.revision_id
             JOIN collections c ON c.id = p.collection_id
             LEFT JOIN paper_metadata m ON m.revision_id = r.id
             LEFT JOIN paper_lifecycle lc ON lc.paper_id = p.id
             LEFT JOIN reading_engagement eng ON eng.paper_id = p.id AND eng.revision_id = h.revision_id";

    let mut stats = crate::HubProjectionStats::default();
    stats.statement_count += 1;
    // totalCount 只跟随筛选，不跟随 cursor：`all_matching` 的 Selection Snapshot 需要
    // 整个 query 的篇数，而不是「剩余篇数」。
    let total_count: i64 = connection
        .query_row(
            &format!("SELECT COUNT(*){FROM_SQL}{filter_sql}"),
            params_from_iter(bind.iter().map(String::as_str)),
            |row| row.get(0),
        )
        .map_err(|error| LibraryQueryError::invalid(error.to_string()))?;

    let mut page_sql = filter_sql.clone();
    if let Some(cursor) = query.cursor.as_deref() {
        let token = decode_cursor(cursor, &digest)?;
        page_sql.push_str(&format!(
            " AND (({key_sql}) {step} ? OR (({key_sql}) = ? AND p.id > ?))"
        ));
        // 三个占位符：严格比较、相等比较、以及 id 决胜。sort key 因此要绑两次。
        bind.extend([token.key.clone(), token.key, token.id]);
    }

    // 多取一行来判定 has_more，省掉第二条 EXISTS 查询。
    let fetch = page_size + 1;
    stats.statement_count += 1;
    let rows = {
        let sql = format!(
            "SELECT p.id, r.id, COALESCE(NULLIF(m.title, ''), p.file_name), m.authors_json,
                    m.publication_year, r.page_count, c.relative_path, p.relative_path,
                    p.file_name, r.sha256, r.byte_size, r.created_at,
                    ({key_sql}) AS sort_key,
                    COALESCE(lc.status, 'unread'), COALESCE(lc.favorite, 0), COALESCE(lc.priority, 0),
                    COALESCE(lc.read_later, 0), lc.review_at, COALESCE(lc.version, 0),
                    eng.furthest_page, eng.last_opened_at,
                    CASE WHEN NOT EXISTS (
                        SELECT 1 FROM ocr_revisions o WHERE o.revision_id = h.revision_id
                    ) AND (
                        SELECT j.state FROM jobs j
                        WHERE j.revision_id = h.revision_id AND j.kind = 'ocr'
                        ORDER BY j.updated_at DESC, j.id DESC LIMIT 1
                    ) IN ('failed', 'interrupted_unknown') THEN 1 ELSE 0 END, m.metadata_json
             {FROM_SQL}{page_sql}
             ORDER BY sort_key {order}, p.id ASC
             LIMIT {fetch}"
        );
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| LibraryQueryError::invalid(error.to_string()))?;
        let mapped = statement
            .query_map(
                params_from_iter(bind.iter().map(String::as_str)),
                row_from_page,
            )
            .map_err(|error| LibraryQueryError::invalid(error.to_string()))?
            .map(|row| row.map_err(|error| LibraryQueryError::invalid(error.to_string())))
            .collect::<Result<Vec<PageRow>, LibraryQueryError>>()?;
        mapped
    };

    let has_more = rows.len() as i64 > page_size;
    let rows: Vec<PageRow> = rows.into_iter().take(page_size as usize).collect();
    let keys: Vec<(String, String)> = rows
        .iter()
        .map(|row| (row.paper.id.clone(), row.paper.revision_id.clone()))
        .collect();
    let paper_ids: Vec<&str> = keys.iter().map(|(paper_id, _)| paper_id.as_str()).collect();
    let (mut extras, extras_stats) =
        load_paper_card_extras(connection, &keys).map_err(LibraryQueryError::unavailable)?;
    stats.statement_count += extras_stats.statement_count;
    let (mut tags, tag_stats) = tags_by_paper(connection, &paper_ids)?;
    stats.statement_count += tag_stats.statement_count;
    let revisions = library_revisions(connection).map_err(LibraryQueryError::unavailable)?;

    let papers = rows
        .iter()
        .map(|row| {
            let extra = extras
                .remove(&row.paper.id)
                .unwrap_or_else(crate::PaperCardExtras::unavailable);
            HubPaperCard {
                tags: tags.remove(&row.paper.id).unwrap_or_default(),
                has_ocr: extra.has_ocr,
                brief_status: extra.brief_status,
                brief_takeaway: extra.brief_takeaway,
                keywords: extra.keywords,
                chapter_number: extra.chapter_number,
                sort_key: row.sort_key.clone(),
                ..row.paper.clone()
            }
        })
        .collect::<Vec<_>>();
    let next_cursor = if has_more {
        papers.last().map(|card| {
            encode_cursor(&CursorToken {
                digest: digest.clone(),
                key: card.sort_key.clone(),
                id: card.id.clone(),
            })
        })
    } else {
        None
    };

    Ok((
        HubPageResult {
            protocol_version: LIBRARY_PROTOCOL_VERSION,
            revision: revisions.global,
            dependencies: revisions.domain_revisions(&HUB_PAGE_DEPENDENCIES),
            dependency_revision: revisions.dependency_vector(&HUB_PAGE_DEPENDENCIES),
            query_digest: digest,
            sort: query.sort,
            page_size,
            total_count,
            papers,
            next_cursor,
            has_more,
            selection_digest: selection_digest(&query.filters),
            selection_dependencies: revisions
                .domain_revisions(&selection_dependencies(&query.filters)),
            evaluation_anchor,
            evaluation_timezone: EVALUATION_TIMEZONE.to_string(),
        },
        stats,
    ))
}

/// 先取 Paper 本体，再用同一次连接的聚合读取补派生字段。
#[derive(Debug, Clone)]
struct PageRow {
    paper: HubPaperCard,
    sort_key: String,
}

fn row_from_page(row: &rusqlite::Row<'_>) -> rusqlite::Result<PageRow> {
    let authors_json: Option<String> = row.get(3)?;
    let authors = authors_json
        .as_deref()
        .and_then(|text| serde_json::from_str::<Vec<String>>(text).ok())
        .unwrap_or_default();
    let relative_path: String = row.get(7)?;
    let kind = library_paths::document_kind_from_relative(&relative_path)
        .unwrap_or(DocumentKind::Paper)
        .as_str()
        .to_string();
    let sort_key: String = row.get(12)?;
    let paper = HubPaperCard {
        id: row.get(0)?,
        revision_id: row.get(1)?,
        title: row.get(2)?,
        authors,
        publication_year: row.get(4)?,
        display_metadata: row
            .get::<_, Option<String>>(22)?
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|v| v.get("_display").cloned()),
        page_count: row.get(5)?,
        collection_path: row.get(6)?,
        relative_path,
        file_name: row.get(8)?,
        kind,
        sha256: row.get(9)?,
        byte_size: row.get(10)?,
        imported_at: row.get(11)?,
        tags: Vec::new(),
        has_ocr: false,
        brief_status: "missing".to_string(),
        brief_takeaway: None,
        keywords: Vec::new(),
        chapter_number: None,
        sort_key: sort_key.clone(),
        lifecycle_status: row.get(13)?,
        favorite: row.get::<_, i64>(14)? != 0,
        priority: row.get(15)?,
        read_later: row.get::<_, i64>(16)? != 0,
        review_at: row.get(17)?,
        lifecycle_version: row.get(18)?,
        furthest_page: row.get(19)?,
        last_opened_at: row.get(20)?,
        ocr_failed: row.get::<_, i64>(21)? != 0,
    };
    Ok(PageRow { paper, sort_key })
}

/// 标签是卡片上唯一还需要额外表的字段，按 id 分块一次读满。
fn tags_by_paper(
    connection: &Connection,
    paper_ids: &[&str],
) -> LibraryQueryResult<(HashMap<String, Vec<String>>, crate::HubProjectionStats)> {
    let mut stats = crate::HubProjectionStats::default();
    let mut grouped: HashMap<String, Vec<String>> = HashMap::new();
    if paper_ids.is_empty() {
        return Ok((grouped, stats));
    }
    for chunk in paper_ids.chunks(HUB_PROJECTION_CHUNK) {
        stats.statement_count += 1;
        let sql = format!(
            "SELECT pt.paper_id, t.name FROM paper_tags pt
             JOIN tags t ON t.id = pt.tag_id
             WHERE pt.paper_id IN ({}) ORDER BY pt.paper_id, lower(t.name)",
            in_placeholders(chunk.len())
        );
        let mut statement = connection
            .prepare(&sql)
            .map_err(|error| LibraryQueryError::unavailable(error.to_string()))?;
        let rows = statement
            .query_map(params_from_iter(chunk.iter().copied()), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| LibraryQueryError::unavailable(error.to_string()))?;
        for row in rows {
            let (paper_id, name) =
                row.map_err(|error| LibraryQueryError::unavailable(error.to_string()))?;
            grouped.entry(paper_id).or_default().push(name);
        }
    }
    Ok((grouped, stats))
}

fn resolve_page_size(limit: Option<i64>) -> LibraryQueryResult<i64> {
    let page_size = limit.unwrap_or(HUB_DEFAULT_PAGE_SIZE);
    if !(1..=HUB_MAX_PAGE_SIZE).contains(&page_size) {
        return Err(LibraryQueryError::invalid(format!(
            "limit must be between 1 and {HUB_MAX_PAGE_SIZE}, default {HUB_DEFAULT_PAGE_SIZE}"
        )));
    }
    Ok(page_size)
}

fn validate_query(query: &HubPageQuery) -> LibraryQueryResult<()> {
    if matches!(query.sort, HubSort::Manual | HubSort::Chapter)
        && !has_exact_collection_filter(query)
    {
        return Err(LibraryQueryError::invalid(
            "manual and chapter order are only defined inside one collection; pass an exact collection filter",
        ));
    }
    validate_filters(&query.filters)
}

/// 给 Smart Collection 保存路径复用：非法谓词必须在写入前失败，不能把坏 AST 存进表。
pub(crate) fn validate_filters(filters: &[QueryFilter]) -> LibraryQueryResult<()> {
    if filters.len() > MAX_FILTERS {
        return Err(LibraryQueryError::invalid(format!(
            "a library query accepts at most {MAX_FILTERS} filters"
        )));
    }
    for filter in filters {
        let _ = filter_predicate(filter, "2026-01-05T00:00:00Z")?;
    }
    Ok(())
}

fn has_exact_collection_filter(query: &HubPageQuery) -> bool {
    query.filters.iter().any(|filter| {
        matches!(
            filter,
            QueryFilter::Collection {
                recursive: false,
                ..
            }
        )
    })
}

/// 白名单展开：每个变体只产出一条常量 SQL 片段，占位符数量与参数数量一致，用户输入
/// 永远走绑定变量。
fn filter_predicate(
    filter: &QueryFilter,
    evaluation_anchor: &str,
) -> LibraryQueryResult<(String, Vec<String>)> {
    match filter {
        QueryFilter::Collection { path, recursive } => {
            let normalized = normalize_collection_path(path)?;
            if *recursive {
                let prefix = escape_like(&normalized);
                Ok((
                    "(c.relative_path = ? OR c.relative_path LIKE ? ESCAPE '\\')".to_string(),
                    vec![normalized, format!("{prefix}/%")],
                ))
            } else {
                Ok(("c.relative_path = ?".to_string(), vec![normalized]))
            }
        }
        QueryFilter::Text { term } => {
            let trimmed = term.trim();
            if trimmed.is_empty() {
                return Err(LibraryQueryError::invalid("text term cannot be empty"));
            }
            if trimmed.chars().count() > MAX_TEXT_TERM {
                return Err(LibraryQueryError::invalid(format!(
                    "text term is limited to {MAX_TEXT_TERM} characters"
                )));
            }
            let pattern = format!("%{}%", escape_like(&trimmed.to_lowercase()));
            Ok((
                "(lower(COALESCE(m.title, '')) LIKE ? ESCAPE '\\'
                      OR lower(p.file_name) LIKE ? ESCAPE '\\'
                      OR lower(COALESCE(m.authors_json, '')) LIKE ? ESCAPE '\\')"
                    .to_string(),
                vec![pattern.clone(), pattern.clone(), pattern],
            ))
        }
        QueryFilter::Tag { tag } => {
            let trimmed = tag.trim();
            if trimmed.is_empty() {
                return Err(LibraryQueryError::invalid("tag cannot be empty"));
            }
            if trimmed.chars().count() > MAX_TAG_NAME {
                return Err(LibraryQueryError::invalid(format!(
                    "tag is limited to {MAX_TAG_NAME} characters"
                )));
            }
            Ok((
                "EXISTS (SELECT 1 FROM paper_tags pt JOIN tags t ON t.id = pt.tag_id
                 WHERE pt.paper_id = p.id AND lower(t.name) = ?)"
                    .to_string(),
                vec![trimmed.to_lowercase()],
            ))
        }
        QueryFilter::Kind { value } => Ok((
            "(p.relative_path = ? OR p.relative_path LIKE ? ESCAPE '\\')".to_string(),
            vec![
                value.dir_name().to_string(),
                format!("{}/%", value.dir_name()),
            ],
        )),
        QueryFilter::Status { value } => {
            let status = match value.as_str() {
                "unread" | "reading" | "read" => value.clone(),
                "done" | "completed" => "read".to_string(),
                _ => {
                    return Err(LibraryQueryError::invalid(
                        "status must be unread, reading, or read",
                    ))
                }
            };
            Ok((
                "COALESCE(lc.status, 'unread') = ?".to_string(),
                vec![status],
            ))
        }
        QueryFilter::Favorite { value } => Ok((
            "COALESCE(lc.favorite, 0) = ?".to_string(),
            vec![if *value { "1".into() } else { "0".into() }],
        )),
        QueryFilter::ReadLater { value } => Ok((
            "COALESCE(lc.read_later, 0) = ?".to_string(),
            vec![if *value { "1".into() } else { "0".into() }],
        )),
        QueryFilter::ReviewDue => Ok((
            "(lc.review_at IS NOT NULL AND lc.review_at <= ?)".to_string(),
            vec![evaluation_anchor.to_string()],
        )),
        QueryFilter::Imported { relative } => {
            if relative != "this_week" {
                return Err(LibraryQueryError::invalid(
                    "imported.relative only accepts this_week",
                ));
            }
            Ok((
                "p.created_at >= ?".to_string(),
                vec![crate::library_lifecycle::this_week_start(evaluation_anchor)],
            ))
        }
        QueryFilter::OcrFailed => Ok((
            "(NOT EXISTS (SELECT 1 FROM ocr_revisions o WHERE o.revision_id = h.revision_id)
              AND (
                SELECT j.state FROM jobs j
                WHERE j.revision_id = h.revision_id AND j.kind = 'ocr'
                ORDER BY j.updated_at DESC, j.id DESC LIMIT 1
              ) IN ('failed', 'interrupted_unknown'))"
                .to_string(),
            vec![],
        )),
        QueryFilter::Opened => Ok(("eng.last_opened_at IS NOT NULL".to_string(), vec![])),
    }
}

/// 目录筛选路径：接受 `Papers` / `Textbooks` 根本身（§10.1 的 collection 筛选和
/// 导入参数的历史规则不同，那里把裸 `Papers` 当作嵌套文件夹）。
fn normalize_collection_path(path: &str) -> LibraryQueryResult<String> {
    let normalized = library_paths::normalize_slashes(path);
    let segments: Vec<&str> = normalized
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let Some(first) = segments.first() else {
        return Err(LibraryQueryError::invalid(
            "collection path cannot be empty; use a kind filter for the whole library",
        ));
    };
    let canonical = match DocumentKind::from_dir_name(first) {
        Some(kind) => {
            let mut parts = vec![kind.dir_name().to_string()];
            parts.extend(segments[1..].iter().map(|part| part.to_string()));
            parts.join("/")
        }
        None => format!("{PAPERS_DIR}/{}", segments.join("/")),
    };
    library_paths::safe_library_relative(Path::new(&canonical))
        .map_err(LibraryQueryError::invalid)?;
    Ok(canonical)
}

/// LIKE 的字面量转义：`%`、`_` 和转义符本身都不允许变成通配。
fn escape_like(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len() + 8);
    for character in value.chars() {
        if matches!(character, '%' | '_' | '\\') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

/// `serde_json` 的 Map 是 `BTreeMap`（未启用 `preserve_order`），所以 `to_string`
/// 已经是键有序的规范 JSON，可直接作为 digest 输入。cursor 刻意不参与：翻页不是
/// 「另一个查询」，遍历整页期间 Selection Snapshot 必须保持同一个 digest。
fn query_digest(query: &HubPageQuery, page_size: i64) -> String {
    let mut normalized = serde_json::json!({
        "protocolVersion": LIBRARY_PROTOCOL_VERSION,
        "sort": query.sort,
        "limit": page_size,
        "filters": query.filters,
    });
    if let Some(direction) = query.direction {
        normalized["direction"] = serde_json::json!(direction);
    }
    let canonical = serde_json::to_string(&normalized).unwrap_or_else(|_| String::new());
    sha256_hex(canonical.as_bytes())
}

/// §5.1 Selection Snapshot 的**成员**摘要：只覆盖 filters。
///
/// 与 [`query_digest`] 的关系是刻意的：`queryDigest` 标识一次分页查询（含 sort 与
/// limit），而 `all_matching` 要冻结的是成员集合。换排序不该让一个已经确认过的
/// 批量动作失效，所以两者必须是不同的摘要。
pub(crate) fn selection_digest(filters: &[QueryFilter]) -> String {
    let canonical = serde_json::to_string(&serde_json::json!({
        "protocolVersion": LIBRARY_PROTOCOL_VERSION,
        "filters": filters,
    }))
    .unwrap_or_else(|_| String::new());
    sha256_hex(canonical.as_bytes())
}

/// 成员集合真正依赖的域。`tags` 只在筛选里出现标签时才声明：给纯目录选择挂上
/// tags 依赖，等于让一次无关的打标把选择判成漂移。
pub(crate) fn selection_dependencies(filters: &[QueryFilter]) -> Vec<LibraryDomain> {
    let mut domains = vec![LibraryDomain::Structure];
    if filters
        .iter()
        .any(|filter| matches!(filter, QueryFilter::Tag { .. }))
    {
        domains.push(LibraryDomain::Tags);
    }
    if filters.iter().any(|filter| {
        matches!(
            filter,
            QueryFilter::Status { .. }
                | QueryFilter::Favorite { .. }
                | QueryFilter::ReadLater { .. }
                | QueryFilter::ReviewDue
        )
    }) {
        domains.push(LibraryDomain::Lifecycle);
    }
    if filters
        .iter()
        .any(|filter| matches!(filter, QueryFilter::Opened))
    {
        domains.push(LibraryDomain::Engagement);
    }
    if filters
        .iter()
        .any(|filter| matches!(filter, QueryFilter::OcrFailed))
    {
        domains.push(LibraryDomain::Artifacts);
        domains.push(LibraryDomain::Jobs);
    }
    domains
}

/// 一个待冻结的选择成员：Paper 与其当前 head revision。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectionMember {
    pub(crate) paper_id: String,
    pub(crate) revision_id: String,
}

/// filters → `(WHERE 片段, 绑定参数)`。片段全部由常量列名拼成，输入只走变量。
fn selection_clause(
    filters: &[QueryFilter],
    evaluation_anchor: &str,
) -> LibraryQueryResult<(String, Vec<String>)> {
    if filters.len() > MAX_FILTERS {
        return Err(LibraryQueryError::invalid(format!(
            "a library query accepts at most {MAX_FILTERS} filters"
        )));
    }
    let mut sql = String::from(" WHERE p.deleted_at IS NULL");
    let mut bind: Vec<String> = Vec::new();
    for filter in filters {
        let (fragment, params) = filter_predicate(filter, evaluation_anchor)?;
        sql.push_str(" AND ");
        sql.push_str(&fragment);
        bind.extend(params);
    }
    Ok((sql, bind))
}

/// 解析整个 query 的成员，供 `plan_batch` 冻结 Selection Snapshot。
///
/// 返回 `None` 表示成员数超过 `cap`：调用方必须把它当成「批次太大」而不是「刚好
/// 这么多」，静默截断会悄悄少改 Paper。顺序按 `p.id`，与分页排序无关。
#[allow(dead_code)]
pub(crate) fn resolve_selection(
    connection: &Connection,
    filters: &[QueryFilter],
    cap: usize,
) -> LibraryQueryResult<Option<Vec<SelectionMember>>> {
    resolve_selection_at(connection, filters, cap, &crate::now())
}

pub(crate) fn resolve_selection_at(
    connection: &Connection,
    filters: &[QueryFilter],
    cap: usize,
    evaluation_anchor: &str,
) -> LibraryQueryResult<Option<Vec<SelectionMember>>> {
    let (where_sql, bind) = selection_clause(filters, evaluation_anchor)?;
    let limit = cap as i64 + 1;
    let sql = format!(
        "SELECT p.id, h.revision_id FROM papers p
         JOIN paper_heads h ON h.paper_id = p.id
         JOIN collections c ON c.id = p.collection_id
         LEFT JOIN paper_metadata m ON m.revision_id = h.revision_id
         LEFT JOIN paper_lifecycle lc ON lc.paper_id = p.id
         LEFT JOIN reading_engagement eng ON eng.paper_id = p.id AND eng.revision_id = h.revision_id
         {where_sql} ORDER BY p.id ASC LIMIT {limit}"
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| LibraryQueryError::unavailable(error.to_string()))?;
    let rows = statement
        .query_map(params_from_iter(bind.iter().map(String::as_str)), |row| {
            Ok(SelectionMember {
                paper_id: row.get(0)?,
                revision_id: row.get(1)?,
            })
        })
        .map_err(|error| LibraryQueryError::unavailable(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| LibraryQueryError::unavailable(error.to_string()))?;
    if rows.len() > cap {
        return Ok(None);
    }
    Ok(Some(rows))
}

pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn encode_cursor(token: &CursorToken) -> String {
    let json = serde_json::to_string(token).unwrap_or_else(|_| String::new());
    URL_SAFE_NO_PAD.encode(json.as_bytes())
}

fn decode_cursor(cursor: &str, expected_digest: &str) -> LibraryQueryResult<CursorToken> {
    let decoded = URL_SAFE_NO_PAD
        .decode(cursor)
        .map_err(|_| LibraryQueryError::invalid("cursor is not a valid token"))?;
    let text = String::from_utf8(decoded)
        .map_err(|_| LibraryQueryError::invalid("cursor is not a valid token"))?;
    let token: CursorToken = serde_json::from_str(&text)
        .map_err(|_| LibraryQueryError::invalid("cursor is not a valid token"))?;
    if token.digest != expected_digest {
        return Err(LibraryQueryError::invalid(
            "cursor was produced by a different query; re-read from the first page",
        ));
    }
    if token.id.is_empty() {
        return Err(LibraryQueryError::invalid(
            "cursor is missing its tie-breaker id",
        ));
    }
    Ok(token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::v2_workspace::WorkspaceModule;
    use rusqlite::params;

    struct Fixture {
        root: tempfile::TempDir,
        connection: Connection,
    }

    fn fixture() -> Fixture {
        let root = tempfile::tempdir().expect("workspace directory");
        WorkspaceModule::new()
            .open(root.path())
            .expect("initialize workspace");
        let connection =
            db::open(root.path().join(".read-desktop/workspace.sqlite3")).expect("open database");
        Fixture { root, connection }
    }

    /// 幂等取得 collection id：根本身由 migration 建好，id 不是 `coll-…` 形状，所以
    /// 必须由这里回查，不能让调用方猜。
    fn ensure_collection(connection: &Connection, relative_path: &str) -> String {
        let name = relative_path.rsplit('/').next().unwrap_or(relative_path);
        let parent = relative_path
            .rsplit_once('/')
            .map(|(prefix, _)| prefix.to_string())
            .unwrap_or_default();
        let parent_id: Option<String> = if parent.is_empty() {
            None
        } else {
            connection
                .query_row(
                    "SELECT id FROM collections WHERE relative_path = ?1",
                    params![parent],
                    |row| row.get(0),
                )
                .ok()
        };
        connection
            .execute(
                "INSERT INTO collections(id, parent_id, name, relative_path, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')
                 ON CONFLICT(relative_path) DO NOTHING",
                params![
                    format!("coll-{relative_path}"),
                    parent_id,
                    name,
                    relative_path
                ],
            )
            .expect("collection");
        connection
            .query_row(
                "SELECT id FROM collections WHERE relative_path = ?1",
                params![relative_path],
                |row| row.get(0),
            )
            .expect("collection id")
    }

    /// 直接写库来构造查询测试的数据：这些测试关心 SQL 语义，不关心文件复制。
    fn seed_paper(
        connection: &Connection,
        relative_path: &str,
        title: &str,
        year: Option<i64>,
        created_at: &str,
        tags: &[&str],
    ) -> String {
        let collection_path = relative_path
            .rsplit_once('/')
            .map(|(prefix, _)| prefix.to_string())
            .expect("paper path lives in a collection");
        let collection_id = ensure_collection(connection, &collection_path);
        let paper_id = format!("paper-{relative_path}");
        let revision_id = format!("rev-{relative_path}");
        connection
            .execute(
                "INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5, NULL)",
                params![
                    paper_id,
                    collection_id,
                    relative_path.rsplit_once('/').map(|(_, f)| f).unwrap_or(relative_path),
                    relative_path,
                    created_at
                ],
            )
            .expect("paper");
        connection
            .execute(
                "INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                 VALUES (?1, ?2, ?3, 1024, 12, ?4, ?5)",
                params![revision_id, paper_id, format!("{:0>64x}", 1), relative_path, created_at],
            )
            .expect("revision");
        connection
            .execute(
                "INSERT INTO paper_heads(paper_id, revision_id, updated_at) VALUES (?1, ?2, ?3)",
                params![paper_id, revision_id, created_at],
            )
            .expect("head");
        connection
            .execute(
                "INSERT INTO paper_metadata(revision_id, title, authors_json, publication_year, source, created_at)
                 VALUES (?1, ?2, '[]', ?3, 'user', ?4)",
                params![revision_id, title, year, created_at],
            )
            .expect("metadata");
        for tag in tags {
            connection
                .execute(
                    "INSERT INTO tags(id, name, created_at) VALUES (?1, ?2, '2026-08-01T00:00:00Z')
                     ON CONFLICT(name) DO NOTHING",
                    params![format!("tag-{tag}"), tag],
                )
                .expect("tag");
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
        paper_id
    }

    fn seed_library(connection: &Connection, count: usize) -> Vec<String> {
        (0..count)
            .map(|index| {
                let path = format!("Papers/Inbox/paper-{index:04}.pdf");
                seed_paper(
                    connection,
                    &path,
                    &format!("Paper {index}"),
                    Some(2000 + (index % 25) as i64),
                    &format!("2026-08-01T{index:06}Z"),
                    &[],
                )
            })
            .collect()
    }

    fn page(fixture: &Fixture, query: &HubPageQuery) -> HubPageResult {
        hub_page_on_connection(&fixture.connection, query)
            .expect("hub page")
            .0
    }

    #[test]
    fn effective_year_and_unknown_values_sort_consistently_in_both_directions() {
        let fixture = fixture();
        let ids = seed_library(&fixture.connection, 3);
        for (id, year) in ids.iter().zip([Some(2024), Some(2025), None]) {
            let value = serde_json::json!({"_format":"auxiliary-v2","_display":{"year":year,"yearLabel":"发布"}});
            fixture.connection.execute("UPDATE paper_metadata SET publication_year=1990,metadata_json=?1 WHERE revision_id=(SELECT revision_id FROM paper_heads WHERE paper_id=?2)",rusqlite::params![value.to_string(),id]).unwrap();
        }
        for (direction, expected) in [
            (
                SortDirection::Asc,
                vec![ids[0].clone(), ids[1].clone(), ids[2].clone()],
            ),
            (
                SortDirection::Desc,
                vec![ids[1].clone(), ids[0].clone(), ids[2].clone()],
            ),
        ] {
            let result = page(
                &fixture,
                &HubPageQuery {
                    sort: HubSort::Year,
                    direction: Some(direction),
                    ..Default::default()
                },
            );
            assert_eq!(
                result
                    .papers
                    .iter()
                    .map(|p| p.id.clone())
                    .collect::<Vec<_>>(),
                expected
            );
            assert_eq!(result.papers[0].publication_year, Some(1990));
            assert!(result.papers[0].display_metadata.is_some());
        }
    }
    #[test]
    fn hub_page_caps_the_default_page_and_walks_the_keyset_without_gaps() {
        let fixture = fixture();
        let seeded = seed_library(&fixture.connection, 250);
        let first = page(&fixture, &HubPageQuery::default());
        assert_eq!(first.page_size, HUB_DEFAULT_PAGE_SIZE);
        assert_eq!(first.total_count, 250);
        assert_eq!(first.papers.len() as i64, HUB_DEFAULT_PAGE_SIZE);
        assert!(first.has_more);
        let cursor = first.next_cursor.clone().expect("next cursor");

        let mut seen: Vec<String> = first
            .papers
            .iter()
            .map(|card| card.id.clone())
            .chain(
                page(
                    &fixture,
                    &HubPageQuery {
                        cursor: Some(cursor.clone()),
                        ..HubPageQuery::default()
                    },
                )
                .papers
                .iter()
                .map(|card| card.id.clone()),
            )
            .collect();
        let second = page(
            &fixture,
            &HubPageQuery {
                cursor: Some(cursor),
                ..HubPageQuery::default()
            },
        );
        assert_eq!(second.total_count, 250, "totalCount follows the query");
        assert!(second.has_more);
        let third = page(
            &fixture,
            &HubPageQuery {
                cursor: Some(second.next_cursor.expect("third cursor")),
                ..HubPageQuery::default()
            },
        );
        assert!(!third.has_more);
        assert_eq!(third.next_cursor, None);
        seen.extend(third.papers.iter().map(|card| card.id.clone()));
        seen.sort();
        let mut expected = seeded;
        expected.sort();
        assert_eq!(
            seen, expected,
            "the keyset must cover every paper exactly once"
        );
    }

    #[test]
    fn equal_sort_keys_still_produce_a_total_order() {
        let fixture = fixture();
        // 同一 created_at 的两条记录只能靠 id tie-break 决定顺序，否则翻页会漏或重。
        for name in ["a", "b", "c"] {
            seed_paper(
                &fixture.connection,
                &format!("Papers/Tie/{name}.pdf"),
                name,
                None,
                "2026-08-02T00:00:00Z",
                &[],
            );
        }
        let first = page(
            &fixture,
            &HubPageQuery {
                limit: Some(1),
                ..HubPageQuery::default()
            },
        );
        let second = page(
            &fixture,
            &HubPageQuery {
                limit: Some(1),
                cursor: first.next_cursor.clone(),
                ..HubPageQuery::default()
            },
        );
        assert_eq!(first.papers.len(), 1);
        assert_eq!(first.papers[0].sort_key, second.papers[0].sort_key);
        assert_ne!(first.papers[0].id, second.papers[0].id);
    }

    #[test]
    fn manual_order_pages_by_persisted_position() {
        let fixture = fixture();
        let seeded: Vec<String> = ["a", "b", "c", "d"]
            .iter()
            .map(|name| {
                seed_paper(
                    &fixture.connection,
                    &format!("Papers/Leaf/{name}.pdf"),
                    &name.to_uppercase(),
                    Some(2020),
                    &format!("2026-08-03T00:00:0{name}Z"),
                    &[],
                )
            })
            .collect();
        let collection_id = ensure_collection(&fixture.connection, "Papers/Leaf");
        // 故意用与 created_at / 文件名都无关的顺序登记，并留一条没有 position。
        let ordered = [&seeded[2], &seeded[0], &seeded[3]];
        for (position, id) in ordered.iter().copied().enumerate() {
            fixture
                .connection
                .execute(
                    "INSERT INTO collection_paper_order(collection_id, paper_id, position) VALUES (?1, ?2, ?3)",
                    params![collection_id, id, position as i64],
                )
                .expect("order row");
        }
        let query = HubPageQuery {
            filters: vec![QueryFilter::Collection {
                path: "Papers/Leaf".to_string(),
                recursive: false,
            }],
            sort: HubSort::Manual,
            direction: None,
            limit: Some(2),
            cursor: None,
        };
        let first = page(&fixture, &query);
        assert_eq!(
            first
                .papers
                .iter()
                .map(|c| c.id.clone())
                .collect::<Vec<_>>(),
            vec![seeded[2].clone(), seeded[0].clone()]
        );
        let second = page(
            &fixture,
            &HubPageQuery {
                cursor: first.next_cursor.clone(),
                ..query.clone()
            },
        );
        assert_eq!(
            second
                .papers
                .iter()
                .map(|c| c.id.clone())
                .collect::<Vec<_>>(),
            vec![seeded[3].clone(), seeded[1].clone()],
            "没有 position 的卡片排在已登记顺序之后"
        );
        assert!(!second.has_more);
    }

    #[test]
    fn query_digest_is_shared_across_pages_and_changes_with_the_ast() {
        let fixture = fixture();
        seed_library(&fixture.connection, 5);
        let first = page(
            &fixture,
            &HubPageQuery {
                limit: Some(2),
                ..HubPageQuery::default()
            },
        );
        let second = page(
            &fixture,
            &HubPageQuery {
                limit: Some(2),
                cursor: first.next_cursor.clone(),
                ..HubPageQuery::default()
            },
        );
        assert_eq!(first.query_digest, second.query_digest);
        // 默认页与显式同值页是同一个查询；limit 参与 digest 所以不同值必须不同。
        let default_page = page(&fixture, &HubPageQuery::default());
        assert_ne!(default_page.query_digest, first.query_digest);
        assert_eq!(
            default_page.query_digest,
            page(
                &fixture,
                &HubPageQuery {
                    limit: Some(HUB_DEFAULT_PAGE_SIZE),
                    ..HubPageQuery::default()
                }
            )
            .query_digest
        );
        for variant in [
            HubPageQuery {
                sort: HubSort::Title,
                ..HubPageQuery::default()
            },
            HubPageQuery {
                filters: vec![QueryFilter::Kind {
                    value: DocumentKind::Paper,
                }],
                ..HubPageQuery::default()
            },
        ] {
            assert_ne!(
                page(&fixture, &variant).query_digest,
                default_page.query_digest
            );
        }
    }

    #[test]
    fn hub_page_reports_the_global_revision_and_only_declared_dependencies() {
        let fixture = fixture();
        seed_library(&fixture.connection, 3);
        let before = page(&fixture, &HubPageQuery::default());
        assert_eq!(before.revision, 0);
        assert_eq!(
            before.dependency_revision,
            "artifacts=0;engagement=0;jobs=0;lifecycle=0;sort=0;structure=0;tags=0"
        );
        assert_eq!(
            before
                .dependencies
                .iter()
                .map(|entry| entry.domain.as_str())
                .collect::<Vec<_>>(),
            vec![
                "artifacts",
                "engagement",
                "jobs",
                "lifecycle",
                "sort",
                "structure",
                "tags"
            ]
        );

        crate::library_workflow::bump_library_revisions(
            &fixture.connection,
            &[LibraryDomain::Sort],
        )
        .expect("bump sort");
        let after = page(&fixture, &HubPageQuery::default());
        assert!(after.revision > before.revision);
        assert_eq!(
            after.dependency_revision,
            "artifacts=0;engagement=0;jobs=0;lifecycle=0;sort=1;structure=0;tags=0"
        );
        crate::library_workflow::bump_library_revisions(
            &fixture.connection,
            &[LibraryDomain::SmartCollections],
        )
        .expect("bump an undeclared domain");
        let after_unrelated = page(&fixture, &HubPageQuery::default());
        assert!(!after_unrelated
            .dependency_revision
            .contains("smart_collections"));
        assert!(after_unrelated.revision > after.revision);
    }

    /// 读侧证据：同数量替换后 `queryDigest` 与 `totalCount` 都不变，只有 revision 会变，
    /// 所以页面的缓存键只能是 revision。（写侧「替换必然 bump」由
    /// `paper_module::the_hub_cursor_is_the_global_revision_not_a_count_composition` 锁定。）
    #[test]
    fn the_page_cache_key_is_the_revision_not_the_digest_or_the_count() {
        let fixture = fixture();
        let original = seed_library(&fixture.connection, 40);
        let query = HubPageQuery {
            limit: Some(10),
            ..HubPageQuery::default()
        };
        let before = page(&fixture, &query);
        assert_eq!(before.total_count, 40);
        assert!(before.has_more);

        for id in &original {
            fixture
                .connection
                .execute("DELETE FROM papers WHERE id = ?1", params![id])
                .expect("drop the original paper");
        }
        for index in 0..40 {
            seed_paper(
                &fixture.connection,
                &format!("Papers/Inbox/swap-{index:04}.pdf"),
                &format!("Swap {index}"),
                Some(2000 + index),
                &format!("2026-08-02T{index:06}Z"),
                &["swapped"],
            );
        }
        // 生产写路径在同一事务里做的事：结构变了就 bump 对应域。
        crate::library_workflow::bump_library_revisions(
            &fixture.connection,
            &[LibraryDomain::Structure, LibraryDomain::Tags],
        )
        .expect("bump the replacement");

        let after = page(&fixture, &query);
        assert_eq!(
            after.total_count, before.total_count,
            "same-size replacement: the count cannot detect it"
        );
        assert_eq!(
            after.query_digest, before.query_digest,
            "the query AST never changed, so the digest cannot be the cache key"
        );
        assert!(after.revision > before.revision);
        assert_eq!(
            after.dependency_revision,
            "artifacts=0;engagement=0;jobs=0;lifecycle=0;sort=0;structure=1;tags=1"
        );
        assert_ne!(after.papers[0].id, before.papers[0].id);
        assert_eq!(after.papers[0].file_name, "swap-0039.pdf");
    }

    #[test]
    fn filters_bind_as_parameters_and_never_as_sql() {
        let fixture = fixture();
        seed_paper(
            &fixture.connection,
            "Papers/Inbox/attention.pdf",
            "Attention Is All You Need",
            Some(2017),
            "2026-08-03T00:00:00Z",
            &["RAG"],
        );
        seed_paper(
            &fixture.connection,
            "Papers/Inbox/other.pdf",
            "Something Else",
            Some(2019),
            "2026-08-03T00:00:01Z",
            &["%wild%"],
        );
        // 注入尝试必须变成一次普通的字符串比较。
        let injected = page(
            &fixture,
            &HubPageQuery {
                filters: vec![QueryFilter::Text {
                    term: "' OR 1=1 --".to_string(),
                }],
                ..HubPageQuery::default()
            },
        );
        assert_eq!(injected.total_count, 0);

        let matched = page(
            &fixture,
            &HubPageQuery {
                filters: vec![QueryFilter::Text {
                    term: "attention".to_string(),
                }],
                ..HubPageQuery::default()
            },
        );
        assert_eq!(matched.total_count, 1);
        assert_eq!(matched.papers[0].tags, vec!["RAG".to_string()]);

        let wildcard = page(
            &fixture,
            &HubPageQuery {
                filters: vec![QueryFilter::Tag {
                    tag: "%wild%".to_string(),
                }],
                ..HubPageQuery::default()
            },
        );
        assert_eq!(wildcard.total_count, 1, "LIKE metacharacters stay literal");

        let textbook = page(
            &fixture,
            &HubPageQuery {
                filters: vec![QueryFilter::Kind {
                    value: DocumentKind::Textbook,
                }],
                ..HubPageQuery::default()
            },
        );
        assert_eq!(textbook.total_count, 0);
        let recursive = page(
            &fixture,
            &HubPageQuery {
                filters: vec![QueryFilter::Collection {
                    path: "Papers".to_string(),
                    recursive: true,
                }],
                ..HubPageQuery::default()
            },
        );
        assert_eq!(recursive.total_count, 2);
        let exact = page(
            &fixture,
            &HubPageQuery {
                filters: vec![QueryFilter::Collection {
                    path: "Papers/Inbox".to_string(),
                    recursive: false,
                }],
                ..HubPageQuery::default()
            },
        );
        assert_eq!(exact.total_count, 2);
    }

    #[test]
    fn lifecycle_filters_relative_dates_and_ocr_failed_use_the_current_revision() {
        let fixture = fixture();
        let unread = seed_paper(
            &fixture.connection,
            "Papers/Inbox/unread.pdf",
            "Unread",
            Some(2026),
            "2026-09-01T12:00:00Z",
            &[],
        );
        let reading = seed_paper(
            &fixture.connection,
            "Papers/Inbox/reading.pdf",
            "Reading",
            Some(2026),
            "2026-08-01T00:00:00Z",
            &[],
        );
        let failed = seed_paper(
            &fixture.connection,
            "Papers/Inbox/failed.pdf",
            "Failed OCR",
            Some(2026),
            "2026-08-01T00:00:00Z",
            &[],
        );
        let recovered = seed_paper(
            &fixture.connection,
            "Papers/Inbox/recovered.pdf",
            "Recovered OCR",
            Some(2026),
            "2026-08-01T00:00:00Z",
            &[],
        );
        fixture
            .connection
            .execute(
                "INSERT INTO paper_lifecycle(
                    paper_id, status, favorite, priority, read_later, review_at,
                    status_changed_at, completed_at, version, updated_at
                 ) VALUES (?1, 'reading', 0, 0, 0, NULL, '2026-08-31T00:00:00Z', NULL, 1, '2026-08-31T00:00:00Z')",
                params![reading],
            )
            .expect("reading");
        fixture
            .connection
            .execute(
                "INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                 VALUES ('rev-old', ?1, 'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 1024, 12, 'Papers/Inbox/recovered.pdf', '2026-08-30T00:00:00Z')",
                params![recovered],
            )
            .expect("old revision");
        fixture
            .connection
            .execute(
                "INSERT INTO jobs(
                    id, kind, provider, paper_id, revision_id, root_key, artifact_key, dedupe_key,
                    state, stage, provider_committed, priority, payload_json, last_error, created_at, updated_at
                 ) VALUES
                 ('job-failed', 'ocr', 'mistral', ?1, ?2, '', '', 'ocr-failed',
                  'failed', 'done', 0, 0, '{}', NULL, '2026-08-31T00:00:00Z', '2026-08-31T00:00:00Z'),
                 ('job-old', 'ocr', 'mistral', ?3, 'rev-old', '', '', 'ocr-old',
                  'failed', 'done', 0, 0, '{}', NULL, '2026-08-30T00:00:00Z', '2026-08-30T00:00:00Z'),
                 ('job-recovered', 'ocr', 'mistral', ?3, ?4, '', '', 'ocr-recovered',
                  'failed', 'done', 0, 0, '{}', NULL, '2026-08-31T00:00:00Z', '2026-08-31T00:00:00Z')",
                params![
                    failed,
                    format!("rev-Papers/Inbox/failed.pdf"),
                    recovered,
                    format!("rev-Papers/Inbox/recovered.pdf")
                ],
            )
            .expect("jobs");
        fixture
            .connection
            .execute(
                "INSERT INTO ocr_revisions(id, revision_id, status, provider, model, created_at)
                 VALUES ('ocr-recovered', ?1, 'ready', 'mistral', 'mistral-ocr-latest', '2026-08-31T01:00:00Z')",
                params![format!("rev-Papers/Inbox/recovered.pdf")],
            )
            .expect("ready ocr");

        let unread_page = page(
            &fixture,
            &HubPageQuery {
                filters: vec![QueryFilter::Status {
                    value: "unread".to_string(),
                }],
                ..HubPageQuery::default()
            },
        );
        assert!(unread_page.papers.iter().any(|card| card.id == unread));
        assert!(!unread_page.papers.iter().any(|card| card.id == reading));

        let this_week = resolve_selection_at(
            &fixture.connection,
            &[
                QueryFilter::Imported {
                    relative: "this_week".to_string(),
                },
                QueryFilter::Status {
                    value: "unread".to_string(),
                },
            ],
            50,
            "2026-09-02T15:00:00Z",
        )
        .expect("this week")
        .expect("under cap");
        assert_eq!(
            this_week
                .iter()
                .map(|member| member.paper_id.as_str())
                .collect::<Vec<_>>(),
            vec![unread.as_str()]
        );

        let ocr_failed = page(
            &fixture,
            &HubPageQuery {
                filters: vec![QueryFilter::OcrFailed],
                ..HubPageQuery::default()
            },
        );
        assert_eq!(ocr_failed.total_count, 1);
        assert_eq!(ocr_failed.papers[0].id, failed);
        assert!(ocr_failed.papers[0].ocr_failed);
        assert!(!ocr_failed.papers.iter().any(|card| card.id == recovered));

        assert_eq!(
            selection_dependencies(&[QueryFilter::Status {
                value: "reading".to_string()
            }]),
            vec![LibraryDomain::Structure, LibraryDomain::Lifecycle]
        );
        assert_eq!(
            selection_dependencies(&[QueryFilter::OcrFailed]),
            vec![
                LibraryDomain::Structure,
                LibraryDomain::Artifacts,
                LibraryDomain::Jobs
            ]
        );
        assert_eq!(
            selection_dependencies(&[QueryFilter::Opened]),
            vec![LibraryDomain::Structure, LibraryDomain::Engagement]
        );
    }

    #[test]
    fn last_opened_sort_and_smart_collections_are_readable() {
        let fixture = fixture();
        let older = seed_paper(
            &fixture.connection,
            "Papers/Inbox/older.pdf",
            "Older",
            Some(2020),
            "2026-08-01T00:00:00Z",
            &[],
        );
        let newer = seed_paper(
            &fixture.connection,
            "Papers/Inbox/newer.pdf",
            "Newer",
            Some(2021),
            "2026-08-01T00:00:01Z",
            &[],
        );
        fixture
            .connection
            .execute(
                "INSERT INTO reading_engagement(
                    paper_id, revision_id, furthest_page, page_count_snapshot,
                    first_opened_at, last_opened_at, updated_at
                 ) VALUES
                 (?1, ?2, 3, 12, '2026-08-31T01:00:00Z', '2026-08-31T01:00:00Z', '2026-08-31T01:00:00Z'),
                 (?3, ?4, 4, 12, '2026-08-31T02:00:00Z', '2026-08-31T03:00:00Z', '2026-08-31T03:00:00Z')",
                params![
                    older,
                    format!("rev-Papers/Inbox/older.pdf"),
                    newer,
                    format!("rev-Papers/Inbox/newer.pdf")
                ],
            )
            .expect("engagement");
        let opened = page(
            &fixture,
            &HubPageQuery {
                filters: vec![QueryFilter::Opened],
                sort: HubSort::LastOpened,
                ..HubPageQuery::default()
            },
        );
        assert_eq!(opened.total_count, 2);
        assert_eq!(opened.papers[0].id, newer);
        assert_eq!(opened.papers[1].id, older);

        let LibraryReadResult::SmartCollections { collections } = read(
            fixture.root.path(),
            &LibraryReadRequest::SmartCollections {
                protocol_version: LIBRARY_PROTOCOL_VERSION,
            },
        )
        .expect("smart collections") else {
            panic!("expected smart collections");
        };
        assert_eq!(collections.len(), 6);
        assert!(collections.iter().all(|item| item.builtin));
        assert_eq!(
            collections[0].id,
            crate::library_lifecycle::BUILTIN_UNREAD_THIS_WEEK
        );
    }

    #[test]
    fn invalid_requests_fail_closed_before_any_read() {
        let fixture = fixture();
        seed_library(&fixture.connection, 3);
        let cases: Vec<(String, HubPageQuery)> = vec![
            (
                "limit above the cap".to_string(),
                HubPageQuery {
                    limit: Some(HUB_MAX_PAGE_SIZE + 1),
                    ..HubPageQuery::default()
                },
            ),
            (
                "zero limit".to_string(),
                HubPageQuery {
                    limit: Some(0),
                    ..HubPageQuery::default()
                },
            ),
            (
                "manual sort without a collection".to_string(),
                HubPageQuery {
                    sort: HubSort::Manual,
                    ..HubPageQuery::default()
                },
            ),
            (
                "manual sort over a recursive scope".to_string(),
                HubPageQuery {
                    sort: HubSort::Manual,
                    filters: vec![QueryFilter::Collection {
                        path: "Papers".to_string(),
                        recursive: true,
                    }],
                    ..HubPageQuery::default()
                },
            ),
            (
                "escaping path".to_string(),
                HubPageQuery {
                    filters: vec![QueryFilter::Collection {
                        path: "../Papers".to_string(),
                        recursive: false,
                    }],
                    ..HubPageQuery::default()
                },
            ),
            (
                "escaping after the root".to_string(),
                HubPageQuery {
                    filters: vec![QueryFilter::Collection {
                        path: "Papers/../Textbooks".to_string(),
                        recursive: false,
                    }],
                    ..HubPageQuery::default()
                },
            ),
            (
                "blank collection path".to_string(),
                HubPageQuery {
                    filters: vec![QueryFilter::Collection {
                        path: "   ".to_string(),
                        recursive: false,
                    }],
                    ..HubPageQuery::default()
                },
            ),
            (
                "empty text term".to_string(),
                HubPageQuery {
                    filters: vec![QueryFilter::Text {
                        term: "   ".to_string(),
                    }],
                    ..HubPageQuery::default()
                },
            ),
            (
                "too many filters".to_string(),
                HubPageQuery {
                    filters: vec![
                        QueryFilter::Kind {
                            value: DocumentKind::Paper
                        };
                        MAX_FILTERS + 1
                    ],
                    ..HubPageQuery::default()
                },
            ),
            (
                "tampered cursor".to_string(),
                HubPageQuery {
                    cursor: Some("not-a-token".to_string()),
                    ..HubPageQuery::default()
                },
            ),
        ];
        for (label, query) in cases {
            let error = hub_page_on_connection(&fixture.connection, &query)
                .err()
                .unwrap_or_else(|| panic!("{label} must be rejected"));
            assert_eq!(error.code, LibraryQueryErrorCode::InvalidQuery, "{label}");
        }

        // 别的查询的 cursor 不能复用，即使 token 本身格式正确。
        let titles = page(
            &fixture,
            &HubPageQuery {
                limit: Some(1),
                sort: HubSort::Title,
                ..HubPageQuery::default()
            },
        );
        let foreign = hub_page_on_connection(
            &fixture.connection,
            &HubPageQuery {
                limit: Some(1),
                cursor: titles.next_cursor,
                ..HubPageQuery::default()
            },
        );
        let error = foreign.expect_err("a foreign cursor is not this query's");
        assert_eq!(error.code, LibraryQueryErrorCode::InvalidQuery);
        assert!(error.message.contains("different query"), "{error:?}");
    }

    #[test]
    fn unknown_fields_and_protocol_versions_are_rejected() {
        let fixture = fixture();
        seed_library(&fixture.connection, 1);
        let request: serde_json::Value = serde_json::from_str(
            r#"{"kind":"hub_page","protocolVersion":1,"page":{"sort":"recent","limit":10,"filter":[{"kind":"tag","tag":"x"}]}}"#,
        )
        .expect("json");
        let error = serde_json::from_value::<LibraryReadRequest>(request.clone())
            .expect_err("a misspelled field must fail closed, not silently mean 'no filters'");
        let text = error.to_string();
        assert!(text.contains("filter"), "{text}");

        let legacy = serde_json::from_value::<LibraryReadRequest>(
            serde_json::json!({"kind":"hub_page","protocolVersion":0,"page":{}}),
        )
        .expect("valid shape");
        let error = read(fixture.root.path(), &legacy).expect_err("v0 rejected");
        assert_eq!(error.code, LibraryQueryErrorCode::InvalidQuery);
    }

    #[test]
    fn statement_count_is_bounded_by_the_page_not_the_library() {
        let small = fixture();
        seed_library(&small.connection, 200);
        let large = fixture();
        seed_library(&large.connection, 1_000);
        let query = HubPageQuery {
            limit: Some(200),
            ..HubPageQuery::default()
        };
        let (small_stats, large_stats) = (
            hub_page_on_connection(&small.connection, &query)
                .expect("small page")
                .1,
            hub_page_on_connection(&large.connection, &query)
                .expect("large page")
                .1,
        );
        assert_eq!(
            small_stats.statement_count, large_stats.statement_count,
            "a five-times larger library must not add statements to one Hub read"
        );
        // 一条 COUNT、一条整页、聚合投影 8 条（ocr / queued / failed / latest ocr job /
        // brief / metadata / lifecycle / engagement）、一条标签批读。库大小不进这个常数。
        assert_eq!(small_stats.statement_count, 11);
    }

    #[test]
    fn ten_thousand_papers_still_use_the_same_number_of_statements() {
        let large = fixture();
        large.connection.execute_batch("BEGIN").expect("begin");
        seed_library(&large.connection, 10_000);
        large.connection.execute_batch("COMMIT").expect("commit");
        let query = HubPageQuery {
            limit: Some(100),
            ..HubPageQuery::default()
        };
        let started = std::time::Instant::now();
        let (page, stats) = hub_page_on_connection(&large.connection, &query).expect("10k page");
        let elapsed = started.elapsed();
        assert_eq!(page.total_count, 10_000);
        assert_eq!(page.papers.len(), 100);
        assert_eq!(stats.statement_count, 11);
        assert!(
            elapsed.as_secs() < 8,
            "10,000 Paper hub_page took {elapsed:?} in debug; expected a bounded page read"
        );
    }

    #[test]
    fn chapter_sort_orders_segmented_numbers_and_needs_an_exact_collection() {
        let fixture = fixture();
        let early = seed_paper(
            &fixture.connection,
            "Textbooks/Book/early.pdf",
            "Early",
            None,
            "2026-08-01T00:00:00Z",
            &[],
        );
        let late = seed_paper(
            &fixture.connection,
            "Textbooks/Book/late.pdf",
            "Late",
            None,
            "2026-08-01T00:00:01Z",
            &[],
        );
        attach_chapter(&fixture.connection, &early, "1.10");
        attach_chapter(&fixture.connection, &late, "1.2");
        let error = hub_page_on_connection(
            &fixture.connection,
            &HubPageQuery {
                sort: HubSort::Chapter,
                ..HubPageQuery::default()
            },
        )
        .expect_err("chapter without exact collection");
        assert_eq!(error.code, LibraryQueryErrorCode::InvalidQuery);
        let page = page(
            &fixture,
            &HubPageQuery {
                filters: vec![QueryFilter::Collection {
                    path: "Textbooks/Book".to_string(),
                    recursive: false,
                }],
                sort: HubSort::Chapter,
                ..HubPageQuery::default()
            },
        );
        assert_eq!(page.papers[0].id, late);
        assert_eq!(page.papers[1].id, early);
        assert_eq!(page.papers[0].chapter_number.as_deref(), Some("1.2"));
        assert_eq!(page.papers[1].chapter_number.as_deref(), Some("1.10"));
    }

    #[test]
    fn collection_layer_returns_every_live_id_in_manual_order() {
        let fixture = fixture();
        let first = seed_paper(
            &fixture.connection,
            "Papers/Inbox/a.pdf",
            "A",
            None,
            "2026-08-01T00:00:00Z",
            &[],
        );
        let second = seed_paper(
            &fixture.connection,
            "Papers/Inbox/b.pdf",
            "B",
            None,
            "2026-08-01T00:00:01Z",
            &[],
        );
        let collection_id: String = fixture
            .connection
            .query_row(
                "SELECT collection_id FROM papers WHERE id = ?1",
                rusqlite::params![first],
                |row| row.get(0),
            )
            .expect("collection");
        fixture
            .connection
            .execute(
                "INSERT INTO collection_paper_order(collection_id, paper_id, position)
                 VALUES (?1, ?2, 0), (?1, ?3, 1)",
                rusqlite::params![collection_id, second, first],
            )
            .expect("order");
        let LibraryReadResult::CollectionLayer {
            collection_id: returned_id,
            paper_ids,
        } = read(
            fixture.root.path(),
            &LibraryReadRequest::CollectionLayer {
                protocol_version: LIBRARY_PROTOCOL_VERSION,
                collection_path: "Papers/Inbox".to_string(),
            },
        )
        .expect("layer")
        else {
            panic!("expected collection_layer");
        };
        assert_eq!(returned_id.as_deref(), Some(collection_id.as_str()));
        assert_eq!(paper_ids, vec![second, first]);
    }

    fn attach_chapter(connection: &Connection, paper_id: &str, chapter: &str) {
        let artifact_id = format!("meta-{paper_id}");
        connection
            .execute(
                "INSERT INTO artifacts(id, paper_id, revision_id, kind, version, status, content_json, created_at)
                 VALUES (?1, ?2, (SELECT id FROM document_revisions WHERE paper_id = ?2), 'metadata', 1, 'ready', ?3, '2026-08-01T00:00:00Z')",
                rusqlite::params![
                    artifact_id,
                    paper_id,
                    format!("{{\"chapterNumber\":\"{chapter}\"}}")
                ],
            )
            .expect("artifact");
        connection
            .execute(
                "INSERT INTO artifact_heads(paper_id, kind, object_key, artifact_id, updated_at)
                 VALUES (?1, 'metadata', '', ?2, '2026-08-01T00:00:00Z')",
                rusqlite::params![paper_id, artifact_id],
            )
            .expect("head");
    }

    #[test]
    fn watch_handshake_only_trusts_a_fresh_revision() {
        let fixture = fixture();
        let current = read(
            fixture.root.path(),
            &LibraryReadRequest::WatchHandshake {
                protocol_version: LIBRARY_PROTOCOL_VERSION,
                after: None,
            },
        )
        .expect("handshake");
        let LibraryReadResult::WatchHandshake {
            current_revision,
            invalidated,
        } = current
        else {
            panic!("expected a handshake result");
        };
        assert_eq!(current_revision, 0);
        assert!(invalidated, "a first load has no snapshot to keep");

        crate::library_workflow::bump_library_revisions(
            &fixture.connection,
            &[LibraryDomain::Structure],
        )
        .expect("bump");
        let stale = read(
            fixture.root.path(),
            &LibraryReadRequest::WatchHandshake {
                protocol_version: LIBRARY_PROTOCOL_VERSION,
                after: Some(current_revision),
            },
        )
        .expect("stale handshake");
        let LibraryReadResult::WatchHandshake {
            current_revision,
            invalidated,
        } = stale
        else {
            panic!("expected a handshake result");
        };
        assert_eq!(current_revision, 1);
        assert!(invalidated, "after < current must trigger a re-read");

        let fresh = read(
            fixture.root.path(),
            &LibraryReadRequest::WatchHandshake {
                protocol_version: LIBRARY_PROTOCOL_VERSION,
                after: Some(current_revision),
            },
        )
        .expect("fresh handshake");
        let LibraryReadResult::WatchHandshake { invalidated, .. } = fresh else {
            panic!("expected a handshake result");
        };
        assert!(!invalidated);
    }

    /// revision 计数器读不到是 Workspace 状态问题。错误码必须是
    /// `workspace_unavailable`：报成 `invalid_query` 会让调用方去改一个本来正确的
    /// 请求，而把它当成「没有结果」更会直接把文库显示成空。
    #[test]
    fn a_library_without_a_revision_counter_fails_closed_as_unavailable() {
        let fixture = fixture();
        seed_library(&fixture.connection, 2);
        let before = page(&fixture, &HubPageQuery::default());
        assert_eq!(before.total_count, 2);

        fixture
            .connection
            .execute("DELETE FROM library_change_seq", [])
            .expect("remove the singleton counter row");

        let page_error = hub_page_on_connection(&fixture.connection, &HubPageQuery::default())
            .expect_err("a page must not be served without a revision");
        assert_eq!(page_error.code, LibraryQueryErrorCode::WorkspaceUnavailable);
        let handshake_error = read(
            fixture.root.path(),
            &LibraryReadRequest::WatchHandshake {
                protocol_version: LIBRARY_PROTOCOL_VERSION,
                after: Some(before.revision),
            },
        )
        .expect_err("the handshake reads the same counter");
        assert_eq!(
            handshake_error.code,
            LibraryQueryErrorCode::WorkspaceUnavailable
        );
    }

    #[test]
    fn hub_page_json_shape_is_camel_case_and_free_of_absolute_paths() {
        let fixture = fixture();
        seed_paper(
            &fixture.connection,
            "Papers/Inbox/shape.pdf",
            "Shape Check",
            Some(2020),
            "2026-08-04T00:00:00Z",
            &["read"],
        );
        let value =
            serde_json::to_value(page(&fixture, &HubPageQuery::default())).expect("serialize");
        for key in [
            "protocolVersion",
            "queryDigest",
            "dependencyRevision",
            "totalCount",
            "pageSize",
            "nextCursor",
            "hasMore",
        ] {
            assert!(value.get(key).is_some(), "missing {key}");
        }
        let card = &value["papers"][0];
        for key in [
            "revisionId",
            "collectionPath",
            "relativePath",
            "publicationYear",
            "briefStatus",
            "hasOcr",
            "sortKey",
            "tags",
            "lifecycleStatus",
            "favorite",
            "ocrFailed",
        ] {
            assert!(card.get(key).is_some(), "missing {key}");
        }
        // §10.2：投影不得携带绝对路径。
        let root_text = fixture.root.path().to_string_lossy().replace('\\', "/");
        let serialized = value.to_string();
        assert!(!serialized.contains(&root_text), "absolute path leaked");
        assert_eq!(value["dependencies"][0]["domain"], "artifacts");
    }

    #[test]
    fn page_through_read_matches_the_direct_reader() {
        let fixture = fixture();
        seed_library(&fixture.connection, 3);
        crate::library_workflow::bump_library_revisions(
            &fixture.connection,
            &[LibraryDomain::Artifacts, LibraryDomain::Tags],
        )
        .expect("bump");
        let LibraryReadResult::HubPage { page } = read(
            fixture.root.path(),
            &LibraryReadRequest::HubPage {
                protocol_version: LIBRARY_PROTOCOL_VERSION,
                page: HubPageQuery::default(),
            },
        )
        .expect("read") else {
            panic!("expected a hub page");
        };
        assert_eq!(
            page.dependency_revision,
            hub_page_on_connection(&fixture.connection, &HubPageQuery::default())
                .expect("hub page")
                .0
                .dependency_revision
        );
        assert_eq!(
            page.dependencies
                .iter()
                .map(|entry| format!("{}={}", entry.domain, entry.value))
                .collect::<Vec<_>>()
                .join(";"),
            "artifacts=1;engagement=0;jobs=0;lifecycle=0;sort=0;structure=0;tags=1"
        );
    }

    /// 跨语言合同的锚点：`src/library/__fixtures__/library_read_v1.json` 同时被
    /// `libraryWorkspaceContract.test.ts` 读取。Rust 侧要能把它每条 `request` 反序列化回
    /// `LibraryReadRequest` 并跑出逐字相同的 `result`，所以任何一侧改字段、改顺序、改
    /// cursor 编码都会同时打破两侧测试。重新生成：`LIBRARY_FIXTURE=update cargo test`。
    #[test]
    fn the_shared_fixture_is_reproducible_from_the_rust_reader() {
        let path = fixture_path();
        let generated = build_library_fixture();
        let text = format!(
            "{}\n",
            serde_json::to_string_pretty(&generated).expect("pretty json")
        );
        if std::env::var("LIBRARY_FIXTURE").as_deref() == Ok("update") {
            std::fs::create_dir_all(path.parent().expect("fixture directory"))
                .expect("create fixture directory");
            std::fs::write(&path, &text).expect("write fixture");
        }
        let checked_in = std::fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("{} must be checked in ({error})", path.display()));
        assert_eq!(
            checked_in, text,
            "fixture drift: rerun with LIBRARY_FIXTURE=update"
        );

        // 逐条回放：请求必须能被本模块解析，且结果与文件一致。
        let fixture = fixture();
        seed_fixture_library(&fixture.connection);
        for case in generated["cases"].as_array().expect("cases") {
            let request: LibraryReadRequest = serde_json::from_value(case["request"].clone())
                .unwrap_or_else(|error| panic!("{}: {error}", case["name"]));
            let result = read(fixture.root.path(), &request)
                .unwrap_or_else(|error| panic!("{} replay failed: {error:?}", case["name"]));
            let mut live = serde_json::to_value(result).expect("serialize");
            freeze_evaluation_anchor(&mut live);
            assert_eq!(
                live, case["result"],
                "{} drifted from the checked-in result",
                case["name"]
            );
        }
    }

    fn fixture_path() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("repository root")
            .join("src/library/__fixtures__/library_read_v1.json")
    }

    fn seed_fixture_library(connection: &Connection) {
        seed_paper(
            connection,
            "Papers/Inbox/alpha.pdf",
            "Alpha Paper",
            Some(2021),
            "2026-08-01T00:00:03Z",
            &["ml"],
        );
        seed_paper(
            connection,
            "Papers/Inbox/beta.pdf",
            "beta paper",
            Some(2019),
            "2026-08-01T00:00:01Z",
            &[],
        );
        seed_paper(
            connection,
            "Textbooks/gamma.pdf",
            "Gamma Chapter",
            None,
            "2026-08-01T00:00:02Z",
            &["ml"],
        );
        connection
            .execute(
                "INSERT INTO paper_lifecycle(
                    paper_id, status, favorite, priority, read_later, review_at,
                    status_changed_at, completed_at, version, updated_at
                 ) VALUES (
                    'paper-Papers/Inbox/alpha.pdf', 'unread', 0, 0, 0, NULL,
                    '2026-08-01T00:00:00Z', NULL, 0, '2026-08-01T00:00:00Z'
                 )",
                [],
            )
            .expect("alpha lifecycle");
    }

    fn hub_page_case(name: &str, page: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "request": { "kind": "hub_page", "protocolVersion": LIBRARY_PROTOCOL_VERSION, "page": page },
        })
    }

    fn watch_case(name: &str, after: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "request": { "kind": "watch_handshake", "protocolVersion": LIBRARY_PROTOCOL_VERSION, "after": after },
        })
    }

    /// 契约夹具不能编码挂钟：`evaluationAnchor` 每次读都不同，所以文件里存的是
    /// 哨兵，两侧逐字比较前都先把实时值换成它。字段真的消失时键集合会先打破。
    const EVALUATION_ANCHOR_PLACEHOLDER: &str = "<evaluation-anchor>";

    fn freeze_evaluation_anchor(result: &mut serde_json::Value) {
        let live = result["page"]["evaluationAnchor"]
            .as_str()
            .map(str::to_string);
        let Some(anchor) = live else { return };
        chrono::DateTime::parse_from_rfc3339(&anchor)
            .expect("evaluationAnchor must be an RFC3339 timestamp");
        result["page"]["evaluationAnchor"] =
            serde_json::Value::String(EVALUATION_ANCHOR_PLACEHOLDER.to_string());
    }

    fn build_library_fixture() -> serde_json::Value {
        let fixture = fixture();
        seed_fixture_library(&fixture.connection);
        let run = |case: serde_json::Value| -> serde_json::Value {
            let request: LibraryReadRequest =
                serde_json::from_value(case["request"].clone()).expect("request");
            let result = read(fixture.root.path(), &request).expect("read");
            let mut case = case;
            case["result"] = serde_json::to_value(result).expect("result");
            freeze_evaluation_anchor(&mut case["result"]);
            case
        };

        let default = run(hub_page_case(
            "hub_page_default",
            serde_json::json!({ "filters": [], "sort": "recent", "limit": null, "cursor": null }),
        ));
        // 标题排序 + limit 1：`nextCursor` 必须是真令牌，才能验到 cursor 线格式。
        let title_first = run(hub_page_case(
            "hub_page_title_first",
            serde_json::json!({ "filters": [], "sort": "title", "limit": 1, "cursor": null }),
        ));
        let cursor = title_first["result"]["page"]["nextCursor"]
            .as_str()
            .expect("a limited page has a cursor")
            .to_string();
        let title_second = run(hub_page_case(
            "hub_page_title_second",
            serde_json::json!({ "filters": [], "sort": "title", "limit": 1, "cursor": cursor }),
        ));
        // 走完整个 keyset：最后一页必须显式报告 `hasMore: false`。
        let last_cursor = title_second["result"]["page"]["nextCursor"]
            .as_str()
            .expect("the second page still has a successor")
            .to_string();
        let title_third = run(hub_page_case(
            "hub_page_title_third",
            serde_json::json!({ "filters": [], "sort": "title", "limit": 1, "cursor": last_cursor }),
        ));
        let filtered = run(hub_page_case(
            "hub_page_filtered",
            serde_json::json!({
                "filters": [
                    { "kind": "collection", "path": "Papers", "recursive": true },
                    { "kind": "tag", "tag": "ML" },
                ],
                "sort": "year",
                "limit": 2,
                "cursor": null,
            }),
        ));
        // §5.1：只有标签谓词才让成员集合依赖 tags 域，合同要在字节里看得见。
        let tag_only = run(hub_page_case(
            "hub_page_tag_only",
            serde_json::json!({
                "filters": [{ "kind": "tag", "tag": "ml" }],
                "sort": "recent",
                "limit": null,
                "cursor": null,
            }),
        ));
        // collection path 归一化在 Rust 与 TS 两侧各有一份实现，必须给出同一页。
        // 只有根段是宽松匹配的（`papers` / 省略根段），叶子段按 BINARY collation 逐字比较。
        let collection_page = |path: &str| {
            serde_json::json!({
                "filters": [{ "kind": "collection", "path": path, "recursive": false }],
                "sort": "recent",
                "limit": null,
                "cursor": null,
            })
        };
        let canonical = run(hub_page_case(
            "hub_page_collection_canonical",
            collection_page("Papers/Inbox"),
        ));
        let lowercase_root = run(hub_page_case(
            "hub_page_collection_lowercase_root",
            collection_page("papers/Inbox"),
        ));
        let bare_collection = run(hub_page_case(
            "hub_page_collection_bare_name",
            collection_page("Inbox"),
        ));
        let trailing_slash = run(hub_page_case(
            "hub_page_collection_trailing_slash",
            collection_page("Papers/Inbox/"),
        ));
        // 宽松只到根段为止：叶子段仍是 BINARY 比较，写错大小写就是空页而不是「顺手匹配」。
        let leaf_case_matters = run(hub_page_case(
            "hub_page_collection_leaf_case_matters",
            collection_page("Papers/inbox"),
        ));
        let revision = default["result"]["page"]["revision"]
            .as_i64()
            .expect("revision");
        let stale = run(watch_case("watch_handshake_stale", serde_json::Value::Null));
        let current = run(watch_case(
            "watch_handshake_current",
            serde_json::json!(revision),
        ));
        let smart = run(serde_json::json!({
            "name": "smart_collections",
            "request": { "kind": "smart_collections", "protocolVersion": LIBRARY_PROTOCOL_VERSION },
        }));
        let reading_context = run(serde_json::json!({
            "name": "reading_context_alpha",
            "request": {
                "kind": "reading_context",
                "protocolVersion": LIBRARY_PROTOCOL_VERSION,
                "paperId": "paper-Papers/Inbox/alpha.pdf",
                "revisionId": "rev-Papers/Inbox/alpha.pdf",
            },
        }));
        let collection_layer = run(serde_json::json!({
            "name": "collection_layer_inbox",
            "request": {
                "kind": "collection_layer",
                "protocolVersion": LIBRARY_PROTOCOL_VERSION,
                "collectionPath": "Papers/Inbox",
            },
        }));
        serde_json::json!({
            "protocolVersion": LIBRARY_PROTOCOL_VERSION,
            "cases": [
                default,
                title_first,
                title_second,
                title_third,
                filtered,
                tag_only,
                canonical,
                lowercase_root,
                bare_collection,
                trailing_slash,
                leaf_case_matters,
                stale,
                current,
                smart,
                reading_context,
                collection_layer,
            ],
        })
    }
}
