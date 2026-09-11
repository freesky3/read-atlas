//! Durable user-owned annotations for the PDF reader.
//!
//! An annotation stores both a live OCR locator and the source snapshot.  OCR
//! revisions may be deleted or replaced, but the user's note/highlight remains
//! listable and can be marked orphaned or migrated by the client.
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type AnnotationResult<T> = Result<T, String>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationLocator {
    #[serde(default)]
    pub document_revision_id: Option<String>,
    #[serde(default)]
    pub ocr_revision_id: Option<String>,
    #[serde(default)]
    pub block_id: Option<String>,
    #[serde(default)]
    pub block_index: Option<i64>,
    #[serde(default)]
    pub block_type: Option<String>,
    #[serde(default)]
    pub content_digest: Option<String>,
    #[serde(default)]
    pub excerpt: Option<String>,
    pub page_number: i64,
    #[serde(default)]
    pub bbox: Option<[f64; 4]>,
    #[serde(default)]
    pub page_width: Option<f64>,
    #[serde(default)]
    pub page_height: Option<f64>,
    #[serde(default)]
    pub coordinate_space: Option<String>,
    #[serde(default)]
    pub text_range: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAnnotation {
    pub id: String,
    pub paper_id: String,
    pub kind: String,
    pub title: Option<String>,
    pub body: Option<String>,
    pub color: Option<String>,
    pub locator: AnnotationLocator,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAnnotationInput {
    pub paper_id: String,
    pub kind: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    pub locator: AnnotationLocator,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAnnotationUpdateRequest {
    #[serde(alias = "annotationId")]
    pub id: String,
    #[serde(default)]
    pub title: Option<Option<String>>,
    #[serde(default)]
    pub body: Option<Option<String>>,
    #[serde(default)]
    pub color: Option<Option<String>>,
    #[serde(default)]
    pub locator: Option<AnnotationLocator>,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAnnotationLink {
    pub id: String,
    pub source_annotation_id: String,
    pub target_annotation_id: String,
    pub link_kind: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserAnnotationLinkInput {
    pub source_annotation_id: String,
    pub target_annotation_id: String,
    pub link_kind: String,
}

const KINDS: [&str; 3] = ["highlight", "bookmark", "note"];
const STATUSES: [&str; 5] = ["active", "orphan", "orphaned", "migrated", "deleted"];
const LINK_KINDS: [&str; 4] = ["related", "supports", "contradicts", "question"];

pub(crate) fn ensure_tables(connection: &Connection) -> AnnotationResult<()> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS user_annotations (
              id TEXT PRIMARY KEY,
              paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
              kind TEXT NOT NULL CHECK (kind IN ('highlight','bookmark','note')),
              title TEXT,
              body TEXT,
              color TEXT,
              status TEXT NOT NULL CHECK (status IN ('active','orphan','orphaned','migrated','deleted')),
              document_revision_id TEXT REFERENCES document_revisions(id) ON DELETE SET NULL,
              ocr_revision_id TEXT REFERENCES ocr_revisions(id) ON DELETE SET NULL,
              block_id TEXT,
              block_index INTEGER,
              block_type TEXT,
              content_digest TEXT,
              excerpt TEXT,
              page_number INTEGER NOT NULL CHECK (page_number >= 1),
              bbox_x0 REAL,
              bbox_y0 REAL,
              bbox_x1 REAL,
              bbox_y1 REAL,
              page_width REAL,
              page_height REAL,
              coordinate_space TEXT,
              text_range_json TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              CHECK ((bbox_x0 IS NULL AND bbox_y0 IS NULL AND bbox_x1 IS NULL AND bbox_y1 IS NULL)
                     OR (bbox_x0 <= bbox_x1 AND bbox_y0 <= bbox_y1))
            );
            CREATE INDEX IF NOT EXISTS idx_user_annotations_paper_page
              ON user_annotations(paper_id, page_number, created_at, id);
            CREATE INDEX IF NOT EXISTS idx_user_annotations_paper_status
              ON user_annotations(paper_id, status, updated_at);
            CREATE INDEX IF NOT EXISTS idx_user_annotations_block
              ON user_annotations(block_id) WHERE block_id IS NOT NULL;
            CREATE TABLE IF NOT EXISTS user_annotation_links (
              id TEXT PRIMARY KEY,
              source_annotation_id TEXT NOT NULL REFERENCES user_annotations(id) ON DELETE CASCADE,
              target_annotation_id TEXT NOT NULL REFERENCES user_annotations(id) ON DELETE CASCADE,
              link_kind TEXT NOT NULL CHECK (link_kind IN ('related','supports','contradicts','question')),
              created_at TEXT NOT NULL,
              UNIQUE(source_annotation_id, target_annotation_id, link_kind),
              CHECK (source_annotation_id <> target_annotation_id)
            );
            CREATE INDEX IF NOT EXISTS idx_user_annotation_links_source
              ON user_annotation_links(source_annotation_id);
            CREATE INDEX IF NOT EXISTS idx_user_annotation_links_target
              ON user_annotation_links(target_annotation_id);
            "#,
        )
        .map_err(|error| error.to_string())
}

fn validate_locator(locator: &AnnotationLocator) -> AnnotationResult<()> {
    if locator.page_number < 1 {
        return Err("Annotation pageNumber must be at least 1".to_string());
    }
    if let Some(index) = locator.block_index {
        if index < 0 {
            return Err("Annotation blockIndex must be non-negative".to_string());
        }
    }
    if let Some(bbox) = locator.bbox {
        if bbox.iter().any(|value| !value.is_finite()) || bbox[0] > bbox[2] || bbox[1] > bbox[3] {
            return Err("Annotation bbox is invalid".to_string());
        }
    }
    if locator
        .page_width
        .is_some_and(|value| !value.is_finite() || value <= 0.0)
        || locator
            .page_height
            .is_some_and(|value| !value.is_finite() || value <= 0.0)
    {
        return Err("Annotation page dimensions are invalid".to_string());
    }
    if let Some(range) = locator.text_range.as_ref() {
        let valid = range
            .get("start")
            .and_then(serde_json::Value::as_i64)
            .zip(range.get("end").and_then(serde_json::Value::as_i64))
            .is_some_and(|(start, end)| start >= 0 && end >= start);
        if !valid {
            return Err("Annotation textRange is invalid".to_string());
        }
    }
    Ok(())
}

fn validate_status(value: &str) -> AnnotationResult<()> {
    if STATUSES.contains(&value) {
        Ok(())
    } else {
        Err(format!("Unsupported annotation status: {value}"))
    }
}

fn validate_kind(value: &str) -> AnnotationResult<()> {
    if KINDS.contains(&value) {
        Ok(())
    } else {
        Err(format!("Unsupported annotation kind: {value}"))
    }
}

fn validate_link_kind(value: &str) -> AnnotationResult<()> {
    if LINK_KINDS.contains(&value) {
        Ok(())
    } else {
        Err(format!("Unsupported annotation link kind: {value}"))
    }
}

fn bbox_values(value: Option<[f64; 4]>) -> [Option<f64>; 4] {
    value.map_or([None, None, None, None], |bbox| {
        [Some(bbox[0]), Some(bbox[1]), Some(bbox[2]), Some(bbox[3])]
    })
}

fn text_range_value(value: &Option<serde_json::Value>) -> Option<String> {
    value.as_ref().map(serde_json::Value::to_string)
}

fn row_to_annotation(row: &rusqlite::Row<'_>) -> rusqlite::Result<UserAnnotation> {
    let bbox_values: [Option<f64>; 4] = [row.get(15)?, row.get(16)?, row.get(17)?, row.get(18)?];
    let bbox = match bbox_values {
        [Some(x0), Some(y0), Some(x1), Some(y1)] => Some([x0, y0, x1, y1]),
        _ => None,
    };
    let text_range_json: Option<String> = row.get(22)?;
    let text_range = text_range_json.and_then(|value| serde_json::from_str(&value).ok());
    Ok(UserAnnotation {
        id: row.get(0)?,
        paper_id: row.get(1)?,
        kind: row.get(2)?,
        title: row.get(3)?,
        body: row.get(4)?,
        color: row.get(5)?,
        status: row.get(6)?,
        locator: AnnotationLocator {
            document_revision_id: row.get(7)?,
            ocr_revision_id: row.get(8)?,
            block_id: row.get(9)?,
            block_index: row.get(10)?,
            block_type: row.get(11)?,
            content_digest: row.get(12)?,
            excerpt: row.get(13)?,
            page_number: row.get(14)?,
            bbox,
            page_width: row.get(19)?,
            page_height: row.get(20)?,
            coordinate_space: row.get(21)?,
            text_range,
        },
        created_at: row.get(23)?,
        updated_at: row.get(24)?,
    })
}

const SELECT_SQL: &str = "SELECT id,paper_id,kind,title,body,color,status,document_revision_id,ocr_revision_id,block_id,block_index,block_type,content_digest,excerpt,page_number,bbox_x0,bbox_y0,bbox_x1,bbox_y1,page_width,page_height,coordinate_space,text_range_json,created_at,updated_at FROM user_annotations";

pub(crate) fn list(
    connection: &Connection,
    paper_id: &str,
) -> AnnotationResult<Vec<UserAnnotation>> {
    ensure_tables(connection)?;
    let mut statement = connection
        .prepare(&format!("{SELECT_SQL} WHERE paper_id=?1 AND status!='deleted' ORDER BY page_number,created_at,id"))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![paper_id], row_to_annotation)
        .map_err(|error| error.to_string())?;
    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

pub(crate) fn get(connection: &Connection, id: &str) -> AnnotationResult<Option<UserAnnotation>> {
    ensure_tables(connection)?;
    connection
        .query_row(
            &format!("{SELECT_SQL} WHERE id=?1"),
            params![id],
            row_to_annotation,
        )
        .optional()
        .map_err(|error| error.to_string())
}

pub(crate) fn create(
    connection: &Connection,
    input: &UserAnnotationInput,
) -> AnnotationResult<UserAnnotation> {
    ensure_tables(connection)?;
    if input.paper_id.trim().is_empty() {
        return Err("Annotation paperId is required".to_string());
    }
    validate_kind(&input.kind)?;
    let active: i64 = connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM papers WHERE id=?1 AND deleted_at IS NULL)",
            params![input.paper_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active == 0 {
        return Err("Annotation paper was not found in the active library".to_string());
    }
    validate_locator(&input.locator)?;
    let status = input.status.as_deref().unwrap_or("active");
    validate_status(status)?;
    let id = Uuid::new_v4().to_string();
    let timestamp = crate::now();
    let bbox = bbox_values(input.locator.bbox);
    connection
        .execute(
            "INSERT INTO user_annotations(id,paper_id,kind,title,body,color,status,document_revision_id,ocr_revision_id,block_id,block_index,block_type,content_digest,excerpt,page_number,bbox_x0,bbox_y0,bbox_x1,bbox_y1,page_width,page_height,coordinate_space,text_range_json,created_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25)",
            params![
                id,
                input.paper_id,
                input.kind,
                input.title,
                input.body,
                input.color,
                status,
                input.locator.document_revision_id,
                input.locator.ocr_revision_id,
                input.locator.block_id,
                input.locator.block_index,
                input.locator.block_type,
                input.locator.content_digest,
                input.locator.excerpt,
                input.locator.page_number,
                bbox[0],
                bbox[1],
                bbox[2],
                bbox[3],
                input.locator.page_width,
                input.locator.page_height,
                input.locator.coordinate_space,
                text_range_value(&input.locator.text_range),
                timestamp,
                timestamp,
            ],
        )
        .map_err(|error| error.to_string())?;
    get(connection, &id)?.ok_or_else(|| "Annotation was not published".to_string())
}

pub(crate) fn update(
    connection: &Connection,
    request: &UserAnnotationUpdateRequest,
) -> AnnotationResult<Option<UserAnnotation>> {
    ensure_tables(connection)?;
    let Some(current) = get(connection, &request.id)? else {
        return Ok(None);
    };
    let title = request
        .title
        .as_ref()
        .map_or(current.title.clone(), Clone::clone);
    let body = request
        .body
        .as_ref()
        .map_or(current.body.clone(), Clone::clone);
    let color = request
        .color
        .as_ref()
        .map_or(current.color.clone(), Clone::clone);
    let locator = request.locator.clone().unwrap_or(current.locator);
    validate_locator(&locator)?;
    let status = request.status.as_deref().unwrap_or(&current.status);
    validate_status(status)?;
    let bbox = bbox_values(locator.bbox);
    let timestamp = crate::now();
    connection
        .execute(
            "UPDATE user_annotations SET title=?2,body=?3,color=?4,status=?5,document_revision_id=?6,ocr_revision_id=?7,block_id=?8,block_index=?9,block_type=?10,content_digest=?11,excerpt=?12,page_number=?13,bbox_x0=?14,bbox_y0=?15,bbox_x1=?16,bbox_y1=?17,page_width=?18,page_height=?19,coordinate_space=?20,text_range_json=?21,updated_at=?22 WHERE id=?1",
            params![
                request.id,
                title,
                body,
                color,
                status,
                locator.document_revision_id,
                locator.ocr_revision_id,
                locator.block_id,
                locator.block_index,
                locator.block_type,
                locator.content_digest,
                locator.excerpt,
                locator.page_number,
                bbox[0],
                bbox[1],
                bbox[2],
                bbox[3],
                locator.page_width,
                locator.page_height,
                locator.coordinate_space,
                text_range_value(&locator.text_range),
                timestamp,
            ],
        )
        .map_err(|error| error.to_string())?;
    get(connection, &request.id)
}

pub(crate) fn delete(connection: &Connection, id: &str) -> AnnotationResult<bool> {
    ensure_tables(connection)?;
    let changed = connection
        .execute(
            "UPDATE user_annotations SET status='deleted',updated_at=?2 WHERE id=?1 AND status!='deleted'",
            params![id, crate::now()],
        )
        .map_err(|error| error.to_string())?;
    Ok(changed != 0)
}

pub(crate) fn list_links(
    connection: &Connection,
    paper_id: &str,
) -> AnnotationResult<Vec<UserAnnotationLink>> {
    ensure_tables(connection)?;
    let mut statement = connection
        .prepare("SELECT l.id,l.source_annotation_id,l.target_annotation_id,l.link_kind,l.created_at FROM user_annotation_links l JOIN user_annotations s ON s.id=l.source_annotation_id JOIN user_annotations t ON t.id=l.target_annotation_id WHERE s.paper_id=?1 AND t.paper_id=?1 AND s.status!='deleted' AND t.status!='deleted' ORDER BY l.created_at,l.id")
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![paper_id], |row| {
            Ok(UserAnnotationLink {
                id: row.get(0)?,
                source_annotation_id: row.get(1)?,
                target_annotation_id: row.get(2)?,
                link_kind: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.map(|row| row.map_err(|error| error.to_string()))
        .collect()
}

pub(crate) fn create_link(
    connection: &Connection,
    input: &UserAnnotationLinkInput,
) -> AnnotationResult<UserAnnotationLink> {
    ensure_tables(connection)?;
    if input.source_annotation_id == input.target_annotation_id {
        return Err("An annotation cannot link to itself".to_string());
    }
    validate_link_kind(&input.link_kind)?;
    let same_paper: Option<String> = connection
        .query_row(
            "SELECT s.paper_id FROM user_annotations s JOIN user_annotations t ON t.id=?2 WHERE s.id=?1 AND s.paper_id=t.paper_id AND s.status!='deleted' AND t.status!='deleted'",
            params![input.source_annotation_id, input.target_annotation_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if same_paper.is_none() {
        return Err(
            "Annotation link endpoints must be active annotations in the same paper".to_string(),
        );
    }
    let id = Uuid::new_v4().to_string();
    connection
        .execute(
            "INSERT OR IGNORE INTO user_annotation_links(id,source_annotation_id,target_annotation_id,link_kind,created_at) VALUES(?1,?2,?3,?4,?5)",
            params![id, input.source_annotation_id, input.target_annotation_id, input.link_kind, crate::now()],
        )
        .map_err(|error| error.to_string())?;
    connection
        .query_row(
            "SELECT id,source_annotation_id,target_annotation_id,link_kind,created_at FROM user_annotation_links WHERE source_annotation_id=?1 AND target_annotation_id=?2 AND link_kind=?3",
            params![input.source_annotation_id, input.target_annotation_id, input.link_kind],
            |row| {
                Ok(UserAnnotationLink {
                    id: row.get(0)?,
                    source_annotation_id: row.get(1)?,
                    target_annotation_id: row.get(2)?,
                    link_kind: row.get(3)?,
                    created_at: row.get(4)?,
                })
            },
        )
        .map_err(|error| error.to_string())
}

pub(crate) fn delete_link(connection: &Connection, id: &str) -> AnnotationResult<bool> {
    ensure_tables(connection)?;
    let changed = connection
        .execute("DELETE FROM user_annotation_links WHERE id=?1", params![id])
        .map_err(|error| error.to_string())?;
    Ok(changed != 0)
}
