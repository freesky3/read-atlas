//! D-063 PR 4：Reading Lifecycle、Engagement 与 Smart Collection。
//!
//! 三件事必须分开：Lifecycle 是 Paper 级意图，Engagement 是某个 revision 的
//! 进度，`reading_states` 继续只负责视口恢复。无 Lifecycle 行就是默认值，
//! 旧库不回填。Smart Collection 只存版本化 query AST，不复制成员。

use crate::library_query::QueryFilter;
use crate::library_workflow::{bump_library_revisions, LibraryDomain};
use crate::now;
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub(crate) type LifecycleResult<T> = Result<T, LifecycleError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LifecycleError {
    pub code: &'static str,
    pub message: String,
}

impl LifecycleError {
    fn of(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn invalid(message: impl Into<String>) -> Self {
        Self::of("invalid_query", message)
    }

    fn stale() -> Self {
        Self::of(
            "stale_library_snapshot",
            "the lifecycle version changed; reload before writing",
        )
    }

    fn missing_paper(paper_id: &str) -> Self {
        Self::of(
            "paper_not_found",
            format!("paper {paper_id} is not in the library"),
        )
    }

    fn unavailable(message: impl Into<String>) -> Self {
        Self::of("workspace_unavailable", message)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LifecycleStatus {
    Unread,
    Reading,
    Read,
}

impl LifecycleStatus {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Unread => "unread",
            Self::Reading => "reading",
            Self::Read => "read",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "unread" => Some(Self::Unread),
            "reading" => Some(Self::Reading),
            "read" | "done" | "completed" => Some(Self::Read),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LifecycleProjection {
    pub paper_id: String,
    pub status: LifecycleStatus,
    pub favorite: bool,
    pub priority: i64,
    pub read_later: bool,
    pub review_at: Option<String>,
    pub status_changed_at: String,
    pub completed_at: Option<String>,
    pub version: i64,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct EngagementProjection {
    pub paper_id: String,
    pub revision_id: String,
    pub furthest_page: i64,
    pub page_count_snapshot: i64,
    pub first_opened_at: String,
    pub last_opened_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadingContextProjection {
    pub lifecycle: LifecycleProjection,
    pub engagement: Option<EngagementProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct LifecyclePatch {
    pub status: Option<LifecycleStatus>,
    pub favorite: Option<bool>,
    pub priority: Option<i64>,
    pub read_later: Option<bool>,
    /// `Some(None)` 清掉复习日期；缺省表示不动。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_at: Option<Option<String>>,
}

impl LifecyclePatch {
    fn is_empty(&self) -> bool {
        self.status.is_none()
            && self.favorite.is_none()
            && self.priority.is_none()
            && self.read_later.is_none()
            && self.review_at.is_none()
    }

    pub(crate) fn validate(&self) -> LifecycleResult<()> {
        if self.is_empty() {
            return Err(LifecycleError::invalid(
                "a lifecycle patch must change at least one field",
            ));
        }
        if let Some(priority) = self.priority {
            if !(0..=3).contains(&priority) {
                return Err(LifecycleError::invalid("priority must be between 0 and 3"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReaderActivity {
    pub paper_id: String,
    pub revision_id: String,
    pub page_number: i64,
    pub page_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SmartQuery {
    pub query_version: i32,
    pub filters: Vec<QueryFilter>,
    pub sort: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SmartCollectionProjection {
    pub id: String,
    pub name: String,
    pub builtin: bool,
    pub query: SmartQuery,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

pub(crate) const QUERY_VERSION: i32 = 1;
pub(crate) const BUILTIN_UNREAD_THIS_WEEK: &str = "unread_this_week";
pub(crate) const BUILTIN_READING: &str = "reading";
pub(crate) const BUILTIN_READ_LATER: &str = "read_later";
pub(crate) const BUILTIN_REVIEW_DUE: &str = "review_due";
pub(crate) const BUILTIN_OCR_FAILED: &str = "ocr_failed";
pub(crate) const BUILTIN_RECENTLY_OPENED: &str = "recently_opened";

pub(crate) fn default_lifecycle(paper_id: &str, timestamp: &str) -> LifecycleProjection {
    LifecycleProjection {
        paper_id: paper_id.to_string(),
        status: LifecycleStatus::Unread,
        favorite: false,
        priority: 0,
        read_later: false,
        review_at: None,
        status_changed_at: timestamp.to_string(),
        completed_at: None,
        version: 0,
        updated_at: timestamp.to_string(),
    }
}

pub(crate) fn load_lifecycle(
    connection: &Connection,
    paper_id: &str,
) -> LifecycleResult<LifecycleProjection> {
    let row = connection
        .query_row(
            "SELECT status, favorite, priority, read_later, review_at,
                    status_changed_at, completed_at, version, updated_at
             FROM paper_lifecycle WHERE paper_id = ?1",
            params![paper_id],
            |row| {
                Ok(LifecycleProjection {
                    paper_id: paper_id.to_string(),
                    status: LifecycleStatus::parse(&row.get::<_, String>(0)?)
                        .unwrap_or(LifecycleStatus::Unread),
                    favorite: row.get::<_, i64>(1)? != 0,
                    priority: row.get(2)?,
                    read_later: row.get::<_, i64>(3)? != 0,
                    review_at: row.get(4)?,
                    status_changed_at: row.get(5)?,
                    completed_at: row.get(6)?,
                    version: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            },
        )
        .optional()
        .map_err(|error| LifecycleError::unavailable(error.to_string()))?;
    Ok(row.unwrap_or_else(|| default_lifecycle(paper_id, &now())))
}

pub(crate) fn load_engagement(
    connection: &Connection,
    paper_id: &str,
    revision_id: &str,
) -> LifecycleResult<Option<EngagementProjection>> {
    connection
        .query_row(
            "SELECT furthest_page, page_count_snapshot, first_opened_at, last_opened_at, updated_at
             FROM reading_engagement WHERE paper_id = ?1 AND revision_id = ?2",
            params![paper_id, revision_id],
            |row| {
                Ok(EngagementProjection {
                    paper_id: paper_id.to_string(),
                    revision_id: revision_id.to_string(),
                    furthest_page: row.get(0)?,
                    page_count_snapshot: row.get(1)?,
                    first_opened_at: row.get(2)?,
                    last_opened_at: row.get(3)?,
                    updated_at: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(|error| LifecycleError::unavailable(error.to_string()))
}

pub(crate) fn reading_context(
    connection: &Connection,
    paper_id: &str,
    revision_id: &str,
) -> LifecycleResult<ReadingContextProjection> {
    paper_exists(connection, paper_id)?;
    Ok(ReadingContextProjection {
        lifecycle: load_lifecycle(connection, paper_id)?,
        engagement: load_engagement(connection, paper_id, revision_id)?,
    })
}

fn paper_exists(connection: &Connection, paper_id: &str) -> LifecycleResult<()> {
    let found: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM papers WHERE id = ?1 AND deleted_at IS NULL",
            params![paper_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| LifecycleError::unavailable(error.to_string()))?;
    if found.is_none() {
        return Err(LifecycleError::missing_paper(paper_id));
    }
    Ok(())
}

fn revision_belongs(
    connection: &Connection,
    paper_id: &str,
    revision_id: &str,
) -> LifecycleResult<()> {
    let found: Option<i64> = connection
        .query_row(
            "SELECT 1 FROM document_revisions WHERE id = ?1 AND paper_id = ?2",
            params![revision_id, paper_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| LifecycleError::unavailable(error.to_string()))?;
    if found.is_none() {
        return Err(LifecycleError::invalid(
            "revisionId does not belong to this Paper",
        ));
    }
    Ok(())
}

/// 单项收藏 / 状态 / 稍后阅读。`expected_version = 0` 表示库里还没有行。
pub(crate) fn apply_lifecycle_patch(
    connection: &Connection,
    paper_id: &str,
    expected_version: i64,
    patch: &LifecyclePatch,
    timestamp: &str,
) -> LifecycleResult<(LifecycleProjection, bool)> {
    paper_exists(connection, paper_id)?;
    patch.validate()?;
    let current = load_lifecycle(connection, paper_id)?;
    if current.version != expected_version {
        return Err(LifecycleError::stale());
    }
    let mut next = current.clone();
    if let Some(status) = patch.status {
        if next.status != status {
            next.status = status;
            next.status_changed_at = timestamp.to_string();
            next.completed_at = match status {
                LifecycleStatus::Read => Some(timestamp.to_string()),
                _ => None,
            };
        }
    }
    if let Some(favorite) = patch.favorite {
        next.favorite = favorite;
    }
    if let Some(priority) = patch.priority {
        next.priority = priority;
    }
    if let Some(read_later) = patch.read_later {
        next.read_later = read_later;
    }
    if let Some(review_at) = &patch.review_at {
        next.review_at = review_at.clone();
    }
    if next == current {
        return Ok((current, false));
    }
    next.version = current.version + 1;
    next.updated_at = timestamp.to_string();
    upsert_lifecycle(connection, &next)?;
    Ok((next, true))
}

fn upsert_lifecycle(connection: &Connection, row: &LifecycleProjection) -> LifecycleResult<()> {
    connection
        .execute(
            "INSERT INTO paper_lifecycle(
                paper_id, status, favorite, priority, read_later, review_at,
                status_changed_at, completed_at, version, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(paper_id) DO UPDATE SET
                status = excluded.status,
                favorite = excluded.favorite,
                priority = excluded.priority,
                read_later = excluded.read_later,
                review_at = excluded.review_at,
                status_changed_at = excluded.status_changed_at,
                completed_at = excluded.completed_at,
                version = excluded.version,
                updated_at = excluded.updated_at",
            params![
                row.paper_id,
                row.status.as_str(),
                if row.favorite { 1 } else { 0 },
                row.priority,
                if row.read_later { 1 } else { 0 },
                row.review_at,
                row.status_changed_at,
                row.completed_at,
                row.version,
                row.updated_at,
            ],
        )
        .map_err(|error| LifecycleError::unavailable(error.to_string()))?;
    Ok(())
}

/// 第一次真正打开 Reader：unread → reading。同 revision 的 furthest_page 单调不减。
/// 显式已读不会因此降级；新 revision 另开一行，旧 engagement 不删。
pub(crate) fn record_activity(
    connection: &Connection,
    activity: &ReaderActivity,
    timestamp: &str,
) -> LifecycleResult<ReadingContextProjection> {
    paper_exists(connection, &activity.paper_id)?;
    revision_belongs(connection, &activity.paper_id, &activity.revision_id)?;
    let page = activity.page_number.max(1);
    let page_count = activity.page_count.max(1);
    let previous = load_engagement(connection, &activity.paper_id, &activity.revision_id)?;
    let engagement = EngagementProjection {
        paper_id: activity.paper_id.clone(),
        revision_id: activity.revision_id.clone(),
        furthest_page: previous
            .as_ref()
            .map(|row| row.furthest_page.max(page))
            .unwrap_or(page),
        page_count_snapshot: previous
            .as_ref()
            .map(|row| row.page_count_snapshot.max(page_count))
            .unwrap_or(page_count),
        first_opened_at: previous
            .as_ref()
            .map(|row| row.first_opened_at.clone())
            .unwrap_or_else(|| timestamp.to_string()),
        last_opened_at: timestamp.to_string(),
        updated_at: timestamp.to_string(),
    };
    connection
        .execute(
            "INSERT INTO reading_engagement(
                paper_id, revision_id, furthest_page, page_count_snapshot,
                first_opened_at, last_opened_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(paper_id, revision_id) DO UPDATE SET
                furthest_page = MAX(reading_engagement.furthest_page, excluded.furthest_page),
                page_count_snapshot = MAX(reading_engagement.page_count_snapshot, excluded.page_count_snapshot),
                last_opened_at = excluded.last_opened_at,
                updated_at = excluded.updated_at",
            params![
                engagement.paper_id,
                engagement.revision_id,
                engagement.furthest_page,
                engagement.page_count_snapshot,
                engagement.first_opened_at,
                engagement.last_opened_at,
                engagement.updated_at,
            ],
        )
        .map_err(|error| LifecycleError::unavailable(error.to_string()))?;

    let mut lifecycle = load_lifecycle(connection, &activity.paper_id)?;
    let mut bumped = vec![LibraryDomain::Engagement];
    if lifecycle.status == LifecycleStatus::Unread {
        lifecycle.status = LifecycleStatus::Reading;
        lifecycle.status_changed_at = timestamp.to_string();
        lifecycle.version += 1;
        lifecycle.updated_at = timestamp.to_string();
        upsert_lifecycle(connection, &lifecycle)?;
        bumped.push(LibraryDomain::Lifecycle);
    }
    bump_library_revisions(connection, &bumped).map_err(LifecycleError::unavailable)?;
    Ok(ReadingContextProjection {
        lifecycle,
        engagement: Some(engagement),
    })
}

pub(crate) fn builtin_collections() -> Vec<SmartCollectionProjection> {
    vec![
        builtin(
            BUILTIN_UNREAD_THIS_WEEK,
            "本周导入但未读",
            vec![
                QueryFilter::Imported {
                    relative: "this_week".to_string(),
                },
                QueryFilter::Status {
                    value: "unread".to_string(),
                },
            ],
            "recent",
        ),
        builtin(
            BUILTIN_READING,
            "阅读中",
            vec![QueryFilter::Status {
                value: "reading".to_string(),
            }],
            "recent",
        ),
        builtin(
            BUILTIN_READ_LATER,
            "稍后阅读",
            vec![QueryFilter::ReadLater { value: true }],
            "recent",
        ),
        builtin(
            BUILTIN_REVIEW_DUE,
            "需要复习",
            vec![QueryFilter::ReviewDue],
            "recent",
        ),
        builtin(
            BUILTIN_OCR_FAILED,
            "OCR 失败",
            vec![QueryFilter::OcrFailed],
            "recent",
        ),
        builtin(
            BUILTIN_RECENTLY_OPENED,
            "最近打开",
            vec![QueryFilter::Opened],
            "last_opened",
        ),
    ]
}

fn builtin(
    id: &str,
    name: &str,
    filters: Vec<QueryFilter>,
    sort: &str,
) -> SmartCollectionProjection {
    SmartCollectionProjection {
        id: id.to_string(),
        name: name.to_string(),
        builtin: true,
        query: SmartQuery {
            query_version: QUERY_VERSION,
            filters,
            sort: sort.to_string(),
        },
        created_at: None,
        updated_at: None,
    }
}

pub(crate) fn list_smart_collections(
    connection: &Connection,
) -> LifecycleResult<Vec<SmartCollectionProjection>> {
    let mut listed = builtin_collections();
    let mut statement = connection
        .prepare(
            "SELECT id, name, query_version, query_json, created_at, updated_at
             FROM smart_collections ORDER BY updated_at DESC, id ASC",
        )
        .map_err(|error| LifecycleError::unavailable(error.to_string()))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i32>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        })
        .map_err(|error| LifecycleError::unavailable(error.to_string()))?;
    for row in rows {
        let (id, name, query_version, query_json, created_at, updated_at) =
            row.map_err(|error| LifecycleError::unavailable(error.to_string()))?;
        let filters: Vec<QueryFilter> = serde_json::from_str(&query_json)
            .map_err(|error| LifecycleError::unavailable(error.to_string()))?;
        listed.push(SmartCollectionProjection {
            id,
            name,
            builtin: false,
            query: SmartQuery {
                query_version,
                filters,
                sort: "recent".to_string(),
            },
            created_at: Some(created_at),
            updated_at: Some(updated_at),
        });
    }
    Ok(listed)
}

pub(crate) fn create_smart_collection(
    connection: &Connection,
    name: &str,
    filters: &[QueryFilter],
    timestamp: &str,
) -> LifecycleResult<SmartCollectionProjection> {
    let name = normalize_name(name)?;
    validate_user_filters(filters)?;
    let id = uuid::Uuid::new_v4().to_string();
    let query_json = serde_json::to_string(filters).unwrap_or_else(|_| "[]".to_string());
    connection
        .execute(
            "INSERT INTO smart_collections(id, name, query_version, query_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
            params![id, name, QUERY_VERSION, query_json, timestamp],
        )
        .map_err(|error| LifecycleError::unavailable(error.to_string()))?;
    bump_library_revisions(connection, &[LibraryDomain::SmartCollections])
        .map_err(LifecycleError::unavailable)?;
    Ok(SmartCollectionProjection {
        id,
        name,
        builtin: false,
        query: SmartQuery {
            query_version: QUERY_VERSION,
            filters: filters.to_vec(),
            sort: "recent".to_string(),
        },
        created_at: Some(timestamp.to_string()),
        updated_at: Some(timestamp.to_string()),
    })
}

pub(crate) fn rename_smart_collection(
    connection: &Connection,
    id: &str,
    name: &str,
    timestamp: &str,
) -> LifecycleResult<SmartCollectionProjection> {
    if is_builtin(id) {
        return Err(LifecycleError::invalid(
            "built-in smart collections cannot be renamed",
        ));
    }
    let name = normalize_name(name)?;
    let changed = connection
        .execute(
            "UPDATE smart_collections SET name = ?2, updated_at = ?3 WHERE id = ?1",
            params![id, name, timestamp],
        )
        .map_err(|error| LifecycleError::unavailable(error.to_string()))?;
    if changed == 0 {
        return Err(LifecycleError::invalid("smart collection does not exist"));
    }
    bump_library_revisions(connection, &[LibraryDomain::SmartCollections])
        .map_err(LifecycleError::unavailable)?;
    list_smart_collections(connection)?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| LifecycleError::invalid("smart collection does not exist"))
}

pub(crate) fn delete_smart_collection(connection: &Connection, id: &str) -> LifecycleResult<()> {
    if is_builtin(id) {
        return Err(LifecycleError::invalid(
            "built-in smart collections cannot be deleted",
        ));
    }
    let changed = connection
        .execute("DELETE FROM smart_collections WHERE id = ?1", params![id])
        .map_err(|error| LifecycleError::unavailable(error.to_string()))?;
    if changed == 0 {
        return Err(LifecycleError::invalid("smart collection does not exist"));
    }
    bump_library_revisions(connection, &[LibraryDomain::SmartCollections])
        .map_err(LifecycleError::unavailable)?;
    Ok(())
}

pub(crate) fn is_builtin(id: &str) -> bool {
    builtin_collections().iter().any(|item| item.id == id)
}

fn normalize_name(name: &str) -> LifecycleResult<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 80 {
        return Err(LifecycleError::invalid(
            "smart collection name must be 1–80 characters",
        ));
    }
    Ok(trimmed.to_string())
}

fn validate_user_filters(filters: &[QueryFilter]) -> LifecycleResult<()> {
    if filters.is_empty() {
        return Err(LifecycleError::invalid(
            "a saved smart collection needs at least one filter",
        ));
    }
    crate::library_query::validate_filters(filters)
        .map_err(|error| LifecycleError::invalid(error.message))?;
    Ok(())
}

/// 「本周」按 evaluation anchor 所在周的周一 00:00 UTC，不在保存时固化成某一天。
pub(crate) fn this_week_start(anchor: &str) -> String {
    let parsed = DateTime::parse_from_rfc3339(anchor)
        .map(|value| value.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now());
    let days = parsed.weekday().num_days_from_monday() as i64;
    let date = parsed.date_naive() - Duration::days(days);
    let midnight = date.and_hms_opt(0, 0, 0).unwrap_or(parsed.naive_utc());
    Utc.from_utc_datetime(&midnight).to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2_workspace::WorkspaceModule;
    use rusqlite::params;

    struct Fixture {
        #[allow(dead_code)]
        root: tempfile::TempDir,
        connection: Connection,
    }

    fn fixture() -> Fixture {
        let root = tempfile::tempdir().expect("workspace");
        WorkspaceModule::new()
            .open(root.path())
            .expect("initialize");
        let connection =
            crate::db::open(&root.path().join(".read-desktop/workspace.sqlite3")).expect("open db");
        Fixture { root, connection }
    }

    fn seed_paper(connection: &Connection, label: &str) -> (String, String) {
        let paper_id = format!("paper-{label}");
        let revision_id = format!("rev-{label}");
        connection
            .execute(
                "INSERT INTO collections(id, parent_id, name, relative_path, created_at, updated_at)
                 VALUES ('coll-inbox', NULL, 'Inbox', 'Papers/Inbox', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')
                 ON CONFLICT(relative_path) DO NOTHING",
                [],
            )
            .expect("collection");
        connection
            .execute(
                "INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at, deleted_at)
                 VALUES (?1, (SELECT id FROM collections WHERE relative_path = 'Papers/Inbox'), ?2, ?3, '2026-08-30T00:00:00Z', '2026-08-30T00:00:00Z', NULL)",
                params![paper_id, format!("{label}.pdf"), format!("Papers/Inbox/{label}.pdf")],
            )
            .expect("paper");
        connection
            .execute(
                "INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                 VALUES (?1, ?2, ?3, 1024, 12, ?4, '2026-08-30T00:00:00Z')",
                params![revision_id, paper_id, format!("{:0>64}", label), format!("Papers/Inbox/{label}.pdf")],
            )
            .expect("revision");
        connection
            .execute(
                "INSERT INTO paper_heads(paper_id, revision_id, updated_at) VALUES (?1, ?2, '2026-08-30T00:00:00Z')",
                params![paper_id, revision_id],
            )
            .expect("head");
        (paper_id, revision_id)
    }

    #[test]
    fn missing_lifecycle_row_is_unread_defaults() {
        let fixture = fixture();
        let (paper_id, revision_id) = seed_paper(&fixture.connection, "alpha");
        let context = reading_context(&fixture.connection, &paper_id, &revision_id).expect("ctx");
        assert_eq!(context.lifecycle.status, LifecycleStatus::Unread);
        assert_eq!(context.lifecycle.version, 0);
        assert!(!context.lifecycle.favorite);
        assert!(context.engagement.is_none());
    }

    #[test]
    fn first_open_moves_unread_to_reading_and_furthest_page_never_drops() {
        let fixture = fixture();
        let (paper_id, revision_id) = seed_paper(&fixture.connection, "alpha");
        let first = record_activity(
            &fixture.connection,
            &ReaderActivity {
                paper_id: paper_id.clone(),
                revision_id: revision_id.clone(),
                page_number: 4,
                page_count: 12,
            },
            "2026-08-31T01:00:00Z",
        )
        .expect("open");
        assert_eq!(first.lifecycle.status, LifecycleStatus::Reading);
        assert_eq!(first.engagement.as_ref().unwrap().furthest_page, 4);
        let later = record_activity(
            &fixture.connection,
            &ReaderActivity {
                paper_id: paper_id.clone(),
                revision_id: revision_id.clone(),
                page_number: 2,
                page_count: 12,
            },
            "2026-08-31T02:00:00Z",
        )
        .expect("back");
        assert_eq!(later.engagement.as_ref().unwrap().furthest_page, 4);
        assert_eq!(later.lifecycle.status, LifecycleStatus::Reading);
    }

    #[test]
    fn explicit_read_survives_reopening_page_one() {
        let fixture = fixture();
        let (paper_id, revision_id) = seed_paper(&fixture.connection, "alpha");
        record_activity(
            &fixture.connection,
            &ReaderActivity {
                paper_id: paper_id.clone(),
                revision_id: revision_id.clone(),
                page_number: 12,
                page_count: 12,
            },
            "2026-08-31T01:00:00Z",
        )
        .expect("open");
        apply_lifecycle_patch(
            &fixture.connection,
            &paper_id,
            1,
            &LifecyclePatch {
                status: Some(LifecycleStatus::Read),
                ..LifecyclePatch::default()
            },
            "2026-08-31T02:00:00Z",
        )
        .expect("mark read");
        let again = record_activity(
            &fixture.connection,
            &ReaderActivity {
                paper_id: paper_id.clone(),
                revision_id,
                page_number: 1,
                page_count: 12,
            },
            "2026-08-31T03:00:00Z",
        )
        .expect("reopen");
        assert_eq!(again.lifecycle.status, LifecycleStatus::Read);
        assert_eq!(again.engagement.as_ref().unwrap().furthest_page, 12);
    }

    #[test]
    fn a_new_revision_starts_progress_unknown_and_keeps_old_engagement() {
        let fixture = fixture();
        let (paper_id, first_revision) = seed_paper(&fixture.connection, "alpha");
        record_activity(
            &fixture.connection,
            &ReaderActivity {
                paper_id: paper_id.clone(),
                revision_id: first_revision.clone(),
                page_number: 9,
                page_count: 12,
            },
            "2026-08-31T01:00:00Z",
        )
        .expect("r1");
        fixture
            .connection
            .execute(
                "INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                 VALUES ('rev-alpha-2', ?1, 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 2048, 20, 'Papers/Inbox/alpha.pdf', '2026-08-31T04:00:00Z')",
                params![paper_id],
            )
            .expect("r2");
        let second = record_activity(
            &fixture.connection,
            &ReaderActivity {
                paper_id: paper_id.clone(),
                revision_id: "rev-alpha-2".to_string(),
                page_number: 2,
                page_count: 20,
            },
            "2026-08-31T05:00:00Z",
        )
        .expect("open r2");
        assert_eq!(second.engagement.as_ref().unwrap().furthest_page, 2);
        let old = load_engagement(&fixture.connection, &paper_id, &first_revision)
            .expect("old")
            .expect("kept");
        assert_eq!(old.furthest_page, 9);
    }

    #[test]
    fn stale_version_is_rejected_and_unread_does_not_clear_engagement() {
        let fixture = fixture();
        let (paper_id, revision_id) = seed_paper(&fixture.connection, "alpha");
        record_activity(
            &fixture.connection,
            &ReaderActivity {
                paper_id: paper_id.clone(),
                revision_id: revision_id.clone(),
                page_number: 5,
                page_count: 12,
            },
            "2026-08-31T01:00:00Z",
        )
        .expect("open");
        let error = apply_lifecycle_patch(
            &fixture.connection,
            &paper_id,
            0,
            &LifecyclePatch {
                favorite: Some(true),
                ..LifecyclePatch::default()
            },
            "2026-08-31T02:00:00Z",
        )
        .expect_err("stale");
        assert_eq!(error.code, "stale_library_snapshot");
        apply_lifecycle_patch(
            &fixture.connection,
            &paper_id,
            1,
            &LifecyclePatch {
                status: Some(LifecycleStatus::Unread),
                ..LifecyclePatch::default()
            },
            "2026-08-31T03:00:00Z",
        )
        .expect("mark unread");
        let engagement = load_engagement(&fixture.connection, &paper_id, &revision_id)
            .expect("eng")
            .expect("kept");
        assert_eq!(engagement.furthest_page, 5);
    }

    #[test]
    fn this_week_is_monday_of_the_anchor_not_a_frozen_calendar_day() {
        assert_eq!(
            this_week_start("2026-09-02T15:00:00+00:00"),
            "2026-08-31T00:00:00+00:00"
        );
    }

    #[test]
    fn builtins_are_not_stored_and_user_collections_round_trip() {
        let fixture = fixture();
        let listed = list_smart_collections(&fixture.connection).expect("list");
        assert_eq!(listed.len(), 6);
        assert!(listed.iter().all(|item| item.builtin));
        let created = create_smart_collection(
            &fixture.connection,
            "  精读候选  ",
            &[QueryFilter::Favorite { value: true }],
            "2026-08-31T04:00:00Z",
        )
        .expect("create");
        assert!(!created.builtin);
        assert_eq!(created.name, "精读候选");
        let renamed = rename_smart_collection(
            &fixture.connection,
            &created.id,
            "周末复盘",
            "2026-08-31T05:00:00Z",
        )
        .expect("rename");
        assert_eq!(renamed.name, "周末复盘");
        assert!(rename_smart_collection(
            &fixture.connection,
            BUILTIN_READING,
            "nope",
            "2026-08-31T06:00:00Z",
        )
        .is_err());
        delete_smart_collection(&fixture.connection, &created.id).expect("delete");
        let after = list_smart_collections(&fixture.connection).expect("after");
        assert_eq!(after.len(), 6);
    }
}
