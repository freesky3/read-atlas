use crate::db;
use crate::library_workflow::{bump_library_revisions, LibraryDomain};
use chrono::Utc;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub type ArtifactResult<T> = Result<T, String>;

// Keep room below SQLite's variable limit while allowing projection and
// override lookups to scale to large workspaces. Both query paths use the
// same bound so a list/get request never exceeds the connection's bind limit.
const SQLITE_BIND_BATCH_SIZE: usize = 400;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceAnchor {
    pub revision_id: String,
    pub page_number: i64,
    pub block_id: Option<String>,
    pub bbox: Option<[i64; 4]>,
    pub excerpt: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ArtifactDraft {
    pub paper_id: String,
    pub revision_id: String,
    pub ocr_revision_id: Option<String>,
    pub kind: String,
    pub object_key: String,
    pub content: Value,
    pub evidence: Vec<EvidenceAnchor>,
    pub dependency_snapshot: Value,
    pub provider_node_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactProjection {
    pub id: String,
    pub paper_id: String,
    pub revision_id: String,
    pub ocr_revision_id: Option<String>,
    pub kind: String,
    pub object_key: String,
    pub version: i64,
    pub status: String,
    pub content: Value,
    pub overrides: Value,
    pub evidence: Vec<EvidenceAnchor>,
    pub dependency_snapshot: Value,
    pub provider_node_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrBlockInput {
    pub block_index: i64,
    pub block_type: String,
    pub text_content: String,
    pub bbox: [i64; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrPageInput {
    pub page_number: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub markdown: Option<String>,
    pub blocks: Vec<OcrBlockInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrBlockProjection {
    pub id: String,
    pub page_number: i64,
    pub block_index: i64,
    pub block_type: String,
    pub text_content: String,
    pub content_digest: String,
    pub bbox: [i64; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrProjection {
    pub id: String,
    pub revision_id: String,
    pub status: String,
    pub provider: String,
    pub model: String,
    pub blocks: Vec<OcrBlockProjection>,
    pub created_at: String,
    pub published_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LensQaProjection {
    pub id: String,
    pub lens_artifact_id: String,
    pub parent_id: Option<String>,
    pub role: String,
    pub content: String,
    pub status: String,
    pub provider_node_id: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct ArtifactModule {
    database_path: PathBuf,
}

impl ArtifactModule {
    pub fn open(database_path: impl AsRef<Path>) -> ArtifactResult<Self> {
        let database_path = database_path.as_ref().to_path_buf();
        if !database_path.is_file() {
            return Err("Workspace database is unavailable".to_string());
        }
        Ok(Self { database_path })
    }

    pub fn publish(&self, draft: ArtifactDraft) -> ArtifactResult<ArtifactProjection> {
        self.publish_prepared(draft, |_, _| Ok(()))
    }

    /// Prepare effective data and synchronize projections inside the same head transaction.
    pub(crate) fn publish_prepared<F>(
        &self,
        mut draft: ArtifactDraft,
        prepare: F,
    ) -> ArtifactResult<ArtifactProjection>
    where
        F: FnOnce(&Connection, &mut ArtifactDraft) -> ArtifactResult<()>,
    {
        validate_artifact_draft(&draft)?;
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        if let Some(job_id) = draft.dependency_snapshot["jobId"].as_str() {
            let duplicate:Option<String>=transaction.query_row("SELECT id FROM artifacts WHERE paper_id=?1 AND kind=?2 AND json_extract(dependency_snapshot_json,'$.jobId')=?3 AND object_key=?4 AND revision_id=?5 ORDER BY version DESC LIMIT 1",params![draft.paper_id,draft.kind,job_id,draft.object_key,draft.revision_id],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
            if let Some(id) = duplicate {
                return self.get_from_connection(&transaction, &id);
            }
            if let Some(created) = draft.dependency_snapshot["jobCreatedAt"].as_str() {
                let newer:Option<String>=transaction.query_row("SELECT a.id FROM artifact_heads h JOIN artifacts a ON a.id=h.artifact_id WHERE h.paper_id=?1 AND h.kind=?2 AND h.object_key=?3 AND (json_extract(a.dependency_snapshot_json,'$.jobCreatedAt')>?4 OR (json_extract(a.dependency_snapshot_json,'$.jobCreatedAt')=?4 AND json_extract(a.dependency_snapshot_json,'$.jobId')>?5))",params![draft.paper_id,draft.kind,draft.object_key,created,job_id],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
                if let Some(id) = newer {
                    return self.get_from_connection(&transaction, &id);
                }
            }
        }
        if draft.dependency_snapshot.get("jobId").is_some() {
            let current: Option<String> = transaction
                .query_row(
                    "SELECT revision_id FROM paper_heads WHERE paper_id=?1",
                    params![draft.paper_id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            if current.as_deref().is_some_and(|id| id != draft.revision_id) {
                draft.dependency_snapshot["historicalOnly"] = serde_json::json!(true);
            }
        }
        prepare(&transaction, &mut draft)?;
        validate_artifact_draft(&draft)?;
        let version: i64 = transaction
            .query_row(
                "SELECT COALESCE(MAX(version), 0) + 1
                 FROM artifacts
                 WHERE paper_id = ?1 AND kind = ?2 AND object_key = ?3",
                params![draft.paper_id, draft.kind, draft.object_key],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        let previous_head: Option<String> = transaction
            .query_row(
                "SELECT artifact_id FROM artifact_heads
                 WHERE paper_id = ?1 AND kind = ?2 AND object_key = ?3",
                params![draft.paper_id, draft.kind, draft.object_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        transaction
            .execute(
                "INSERT INTO artifacts(
                   id, paper_id, revision_id, ocr_revision_id, kind, object_key,
                   version, status, content_json, evidence_json,
                   dependency_snapshot_json, provider_node_id, created_at
                 ) VALUES (
                   ?1, ?2, ?3, ?4, ?5, ?6, ?7, 'ready', ?8, ?9, ?10, ?11, ?12
                 )",
                params![
                    id,
                    draft.paper_id,
                    draft.revision_id,
                    draft.ocr_revision_id,
                    draft.kind,
                    draft.object_key,
                    version,
                    draft.content.to_string(),
                    serde_json::to_string(&draft.evidence).map_err(|error| error.to_string())?,
                    draft.dependency_snapshot.to_string(),
                    draft.provider_node_id,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        if draft.dependency_snapshot["historicalOnly"] != true {
            transaction
            .execute(
                "INSERT INTO artifact_heads(paper_id, kind, object_key, artifact_id, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(paper_id, kind, object_key) DO UPDATE SET
                   artifact_id = excluded.artifact_id,
                   updated_at = excluded.updated_at",
                params![draft.paper_id, draft.kind, draft.object_key, id, timestamp],
            )
            .map_err(|error| error.to_string())?;
            if let Some(previous) = previous_head {
                transaction
                    .execute(
                        "UPDATE artifacts SET superseded_at = ?1 WHERE id = ?2",
                        params![timestamp, previous],
                    )
                    .map_err(|error| error.to_string())?;
                if draft.kind.starts_with("lens_") {
                    transaction
                        .execute(
                            "DELETE FROM lens_qa WHERE lens_artifact_id = ?1",
                            params![previous],
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
        }
        bump_library_revisions(&transaction, &[LibraryDomain::Artifacts])
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        self.get_from_connection(&connection, &id)
    }

    pub fn publish_batch(
        &self,
        drafts: Vec<ArtifactDraft>,
    ) -> ArtifactResult<Vec<ArtifactProjection>> {
        if drafts.is_empty() {
            return Err("Artifact batch cannot be empty".to_string());
        }
        for draft in &drafts {
            validate_artifact_draft(draft)?;
        }
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let timestamp = now();
        let mut published_ids = Vec::with_capacity(drafts.len());
        for draft in drafts {
            if draft.dependency_snapshot["protocol"] == "orientation-pack-legacy" {
                let current:Option<String>=transaction.query_row("SELECT a.id FROM artifact_heads h JOIN artifacts a ON a.id=h.artifact_id WHERE h.paper_id=?1 AND h.kind=?2 AND h.object_key=?3 AND json_extract(a.dependency_snapshot_json,'$.protocol') LIKE 'document-artifact-%'",params![draft.paper_id,draft.kind,draft.object_key],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
                if let Some(id) = current {
                    published_ids.push(id);
                    continue;
                }
            }

            let version: i64 = transaction
                .query_row(
                    "SELECT COALESCE(MAX(version), 0) + 1
                     FROM artifacts
                     WHERE paper_id = ?1 AND kind = ?2 AND object_key = ?3",
                    params![draft.paper_id, draft.kind, draft.object_key],
                    |row| row.get(0),
                )
                .map_err(|error| error.to_string())?;
            let previous_head: Option<String> = transaction
                .query_row(
                    "SELECT artifact_id FROM artifact_heads
                     WHERE paper_id = ?1 AND kind = ?2 AND object_key = ?3",
                    params![draft.paper_id, draft.kind, draft.object_key],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| error.to_string())?;
            let id = Uuid::new_v4().to_string();
            transaction
                .execute(
                    "INSERT INTO artifacts(
                       id, paper_id, revision_id, ocr_revision_id, kind, object_key,
                       version, status, content_json, evidence_json,
                       dependency_snapshot_json, provider_node_id, created_at
                     ) VALUES (
                       ?1, ?2, ?3, ?4, ?5, ?6, ?7, 'ready', ?8, ?9, ?10, ?11, ?12
                     )",
                    params![
                        id,
                        draft.paper_id,
                        draft.revision_id,
                        draft.ocr_revision_id,
                        draft.kind,
                        draft.object_key,
                        version,
                        draft.content.to_string(),
                        serde_json::to_string(&draft.evidence).map_err(|error| error.to_string())?,
                        draft.dependency_snapshot.to_string(),
                        draft.provider_node_id,
                        timestamp
                    ],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "INSERT INTO artifact_heads(paper_id, kind, object_key, artifact_id, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(paper_id, kind, object_key) DO UPDATE SET
                       artifact_id = excluded.artifact_id,
                       updated_at = excluded.updated_at",
                    params![draft.paper_id, draft.kind, draft.object_key, id, timestamp],
                )
                .map_err(|error| error.to_string())?;
            if let Some(previous) = previous_head {
                transaction
                    .execute(
                        "UPDATE artifacts SET superseded_at = ?1 WHERE id = ?2",
                        params![timestamp, previous],
                    )
                    .map_err(|error| error.to_string())?;
                if draft.kind.starts_with("lens_") {
                    transaction
                        .execute(
                            "DELETE FROM lens_qa WHERE lens_artifact_id = ?1",
                            params![previous],
                        )
                        .map_err(|error| error.to_string())?;
                }
            }
            published_ids.push(id);
        }
        bump_library_revisions(&transaction, &[LibraryDomain::Artifacts])
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        self.get_many_from_connection(&connection, &published_ids)
    }

    pub fn head(
        &self,
        paper_id: &str,
        kind: &str,
        object_key: &str,
    ) -> ArtifactResult<Option<ArtifactProjection>> {
        let connection = self.connect()?;
        let id: Option<String> = connection
            .query_row(
                "SELECT artifact_id FROM artifact_heads
                 WHERE paper_id = ?1 AND kind = ?2 AND object_key = ?3",
                params![paper_id, kind, object_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        id.map(|id| self.get_from_connection(&connection, &id))
            .transpose()
    }

    pub fn head_for_revision(
        &self,
        revision_id: &str,
        kind: &str,
        object_key: &str,
    ) -> ArtifactResult<Option<ArtifactProjection>> {
        let connection = self.connect()?;
        let id: Option<String> = connection
            .query_row(
                "SELECT h.artifact_id
                 FROM artifact_heads h
                 JOIN artifacts a ON a.id = h.artifact_id
                 WHERE a.revision_id = ?1 AND h.kind = ?2 AND h.object_key = ?3",
                params![revision_id, kind, object_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        id.map(|id| self.get_from_connection(&connection, &id))
            .transpose()
    }

    pub fn list(&self, paper_id: &str) -> ArtifactResult<Vec<ArtifactProjection>> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT a.id, a.paper_id, a.revision_id, a.ocr_revision_id,
                        a.kind, a.object_key, a.version, a.status,
                        a.content_json, a.evidence_json,
                        a.dependency_snapshot_json, a.provider_node_id, a.created_at
                 FROM artifacts a
                 JOIN artifact_heads h ON h.artifact_id = a.id
                 WHERE a.paper_id = ?1
                 ORDER BY a.kind, a.object_key",
            )
            .map_err(|error| error.to_string())?;
        let mut artifacts = statement
            .query_map(params![paper_id], artifact_from_row)
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        self.load_overrides_batch(&connection, &mut artifacts)?;
        Ok(artifacts)
    }

    pub fn get(&self, id: &str) -> ArtifactResult<ArtifactProjection> {
        let connection = self.connect()?;
        self.get_from_connection(&connection, id)
    }

    pub fn delete(&self, id: &str) -> ArtifactResult<Option<ArtifactProjection>> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;

        let target: Option<(String, String, String)> = transaction
            .query_row(
                "SELECT paper_id, kind, object_key FROM artifacts WHERE id = ?1",
                params![id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;

        let Some((paper_id, kind, object_key)) = target else {
            return Ok(None);
        };

        let current_head: Option<String> = transaction
            .query_row(
                "SELECT artifact_id FROM artifact_heads WHERE paper_id = ?1 AND kind = ?2 AND object_key = ?3",
                params![paper_id, kind, object_key],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;

        let is_head = current_head.as_deref() == Some(id);

        if is_head {
            let next_head: Option<(String, String)> = transaction
                .query_row(
                    "SELECT id, created_at FROM artifacts
                     WHERE paper_id = ?1 AND kind = ?2 AND object_key = ?3 AND id != ?4
                     ORDER BY created_at DESC, version DESC LIMIT 1",
                    params![paper_id, kind, object_key, id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .optional()
                .map_err(|error| error.to_string())?;

            if let Some((next_id, next_time)) = next_head {
                transaction
                    .execute(
                        "UPDATE artifact_heads SET artifact_id = ?1, updated_at = ?2
                         WHERE paper_id = ?3 AND kind = ?4 AND object_key = ?5",
                        params![next_id, next_time, paper_id, kind, object_key],
                    )
                    .map_err(|error| error.to_string())?;
                transaction
                    .execute(
                        "UPDATE artifacts SET superseded_at = NULL WHERE id = ?1",
                        params![next_id],
                    )
                    .map_err(|error| error.to_string())?;
            } else {
                transaction
                    .execute(
                        "DELETE FROM artifact_heads WHERE paper_id = ?1 AND kind = ?2 AND object_key = ?3",
                        params![paper_id, kind, object_key],
                    )
                    .map_err(|error| error.to_string())?;
            }
        }

        transaction
            .execute(
                "DELETE FROM lens_qa WHERE lens_artifact_id = ?1",
                params![id],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM term_overrides WHERE artifact_id = ?1",
                params![id],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM symbol_overrides WHERE artifact_id = ?1",
                params![id],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute("DELETE FROM artifacts WHERE id = ?1", params![id])
            .map_err(|error| error.to_string())?;

        bump_library_revisions(&transaction, &[LibraryDomain::Artifacts])
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;

        self.head(&paper_id, &kind, &object_key)
    }

    fn get_from_connection(
        &self,
        connection: &Connection,
        id: &str,
    ) -> ArtifactResult<ArtifactProjection> {
        self.get_many_from_connection(connection, &[id.to_string()])?
            .into_iter()
            .next()
            .ok_or_else(|| "Artifact was not found".to_string())
    }

    fn get_many_from_connection(
        &self,
        connection: &Connection,
        ids: &[String],
    ) -> ArtifactResult<Vec<ArtifactProjection>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut by_id = HashMap::with_capacity(ids.len());
        for batch in ids.chunks(SQLITE_BIND_BATCH_SIZE) {
            let placeholders = std::iter::repeat_n("?", batch.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT id, paper_id, revision_id, ocr_revision_id, kind, object_key,
                        version, status, content_json, evidence_json,
                        dependency_snapshot_json, provider_node_id, created_at
                 FROM artifacts WHERE id IN ({placeholders})"
            );
            let mut statement = connection
                .prepare_cached(&sql)
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map(params_from_iter(batch.iter()), artifact_from_row)
                .map_err(|error| error.to_string())?;
            for row in rows {
                let artifact = row.map_err(|error| error.to_string())?;
                by_id.insert(artifact.id.clone(), artifact);
            }
        }
        let mut artifacts = ids
            .iter()
            .filter_map(|id| by_id.remove(id))
            .collect::<Vec<_>>();
        self.load_overrides_batch(connection, &mut artifacts)?;
        Ok(artifacts)
    }

    fn load_overrides_batch(
        &self,
        connection: &Connection,
        artifacts: &mut [ArtifactProjection],
    ) -> ArtifactResult<()> {
        let glossary_ids = artifacts
            .iter()
            .filter(|artifact| artifact.kind == "glossary")
            .map(|artifact| artifact.id.clone())
            .collect::<Vec<_>>();
        let symbol_ids = artifacts
            .iter()
            .filter(|artifact| artifact.kind == "symbol_table")
            .map(|artifact| artifact.id.clone())
            .collect::<Vec<_>>();
        let glossary =
            self.read_override_table(connection, &glossary_ids, "term_overrides", "term_key")?;
        let symbols =
            self.read_override_table(connection, &symbol_ids, "symbol_overrides", "symbol_key")?;
        for artifact in artifacts {
            artifact.overrides = match artifact.kind.as_str() {
                "glossary" => glossary
                    .get(&artifact.id)
                    .cloned()
                    .map(Value::Object)
                    .unwrap_or_else(|| serde_json::json!({})),
                "symbol_table" => symbols
                    .get(&artifact.id)
                    .cloned()
                    .map(Value::Object)
                    .unwrap_or_else(|| serde_json::json!({})),
                _ => serde_json::json!({}),
            };
        }
        Ok(())
    }

    fn read_override_table(
        &self,
        connection: &Connection,
        artifact_ids: &[String],
        table: &str,
        key_column: &str,
    ) -> ArtifactResult<HashMap<String, serde_json::Map<String, Value>>> {
        if artifact_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut values = HashMap::<String, serde_json::Map<String, Value>>::new();
        for batch in artifact_ids.chunks(SQLITE_BIND_BATCH_SIZE) {
            let placeholders = std::iter::repeat_n("?", batch.len())
                .collect::<Vec<_>>()
                .join(",");
            let sql = format!(
                "SELECT artifact_id, {key_column}, value_json FROM {table}
                 WHERE artifact_id IN ({placeholders})"
            );
            let mut statement = connection
                .prepare_cached(&sql)
                .map_err(|error| error.to_string())?;
            let rows = statement
                .query_map(params_from_iter(batch.iter()), |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|error| error.to_string())?;
            for row in rows {
                let (artifact_id, key, raw) = row.map_err(|error| error.to_string())?;
                values.entry(artifact_id).or_default().insert(
                    key,
                    serde_json::from_str(&raw).unwrap_or_else(|_| serde_json::json!({})),
                );
            }
        }
        Ok(values)
    }

    pub fn set_override(
        &self,
        artifact_id: &str,
        kind: &str,
        key: &str,
        value: &Value,
    ) -> ArtifactResult<()> {
        if key.trim().is_empty() || !value.is_object() {
            return Err("Override key and object value are required".to_string());
        }
        let connection = self.connect()?;
        let actual_kind: String = connection
            .query_row(
                "SELECT kind FROM artifacts WHERE id = ?1",
                params![artifact_id],
                |row| row.get(0),
            )
            .map_err(|_| "Artifact for override was not found".to_string())?;
        if actual_kind != kind {
            return Err("Override kind does not match the Artifact".to_string());
        }
        let table = match kind {
            "glossary" => "term_overrides",
            "symbol_table" => "symbol_overrides",
            _ => return Err("Only glossary and symbol table overrides are editable".to_string()),
        };
        let key_column = if table == "term_overrides" {
            "term_key"
        } else {
            "symbol_key"
        };
        let sql = format!(
            "INSERT INTO {table}(artifact_id, {key_column}, value_json, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(artifact_id, {key_column}) DO UPDATE SET
               value_json = excluded.value_json,
               updated_at = excluded.updated_at"
        );
        connection
            .execute(&sql, params![artifact_id, key, value.to_string(), now()])
            .map_err(|error| error.to_string())?;
        Ok(())
    }
    pub fn publish_ocr(
        &self,
        revision_id: &str,
        provider: &str,
        model: &str,
        raw_staging_path: Option<&str>,
        pages: &[OcrPageInput],
    ) -> ArtifactResult<OcrProjection> {
        validate_ocr_pages(pages)?;
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        transaction
            .execute(
                "INSERT INTO ocr_revisions(
                   id, revision_id, status, provider, model, raw_staging_path,
                   created_at, published_at
                 ) VALUES (?1, ?2, 'ready', ?3, ?4, ?5, ?6, ?6)",
                params![
                    id,
                    revision_id,
                    provider,
                    model,
                    raw_staging_path,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        for page in pages {
            let page_id = Uuid::new_v4().to_string();
            transaction
                .execute(
                    "INSERT INTO ocr_pages(
                       id, ocr_revision_id, page_number, width, height, markdown
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        page_id,
                        id,
                        page.page_number,
                        page.width,
                        page.height,
                        page.markdown
                    ],
                )
                .map_err(|error| error.to_string())?;
            for block in &page.blocks {
                let digest = content_digest(&block.text_content);
                transaction
                    .execute(
                        "INSERT INTO ocr_blocks(
                           id, ocr_page_id, block_index, block_type, text_content,
                           content_digest, x0, y0, x1, y1
                         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                        params![
                            Uuid::new_v4().to_string(),
                            page_id,
                            block.block_index,
                            block.block_type,
                            block.text_content,
                            digest,
                            block.bbox[0],
                            block.bbox[1],
                            block.bbox[2],
                            block.bbox[3]
                        ],
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
        bump_library_revisions(&transaction, &[LibraryDomain::Artifacts])
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        self.ocr(&id)
    }

    pub fn latest_ocr(&self, revision_id: &str) -> ArtifactResult<Option<OcrProjection>> {
        let connection = self.connect()?;
        let id = connection
            .query_row(
                "SELECT id FROM ocr_revisions
                 WHERE revision_id = ?1 AND status = 'ready'
                 ORDER BY published_at DESC LIMIT 1",
                params![revision_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        id.map(|id| self.ocr(&id)).transpose()
    }

    pub fn ocr(&self, id: &str) -> ArtifactResult<OcrProjection> {
        let connection = self.connect()?;
        let (revision_id, status, provider, model, created_at, published_at) = connection
            .query_row(
                "SELECT revision_id, status, provider, model, created_at, published_at
                 FROM ocr_revisions WHERE id = ?1",
                params![id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT b.id, p.page_number, b.block_index, b.block_type,
                        b.text_content, b.content_digest, b.x0, b.y0, b.x1, b.y1
                 FROM ocr_blocks b
                 JOIN ocr_pages p ON p.id = b.ocr_page_id
                 WHERE p.ocr_revision_id = ?1
                 ORDER BY p.page_number, b.block_index",
            )
            .map_err(|error| error.to_string())?;
        let blocks = statement
            .query_map(params![id], |row| {
                Ok(OcrBlockProjection {
                    id: row.get(0)?,
                    page_number: row.get(1)?,
                    block_index: row.get(2)?,
                    block_type: row.get(3)?,
                    text_content: row.get(4)?,
                    content_digest: row.get(5)?,
                    bbox: [row.get(6)?, row.get(7)?, row.get(8)?, row.get(9)?],
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        Ok(OcrProjection {
            id: id.to_string(),
            revision_id,
            status,
            provider,
            model,
            blocks,
            created_at,
            published_at,
        })
    }

    pub fn delete_ocr_cascade(&self, revision_id: &str) -> ArtifactResult<bool> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;

        // 1. Delete lens_qa belonging to artifacts for this revision that are OCR-dependent
        transaction
            .execute(
                "DELETE FROM lens_qa WHERE lens_artifact_id IN (
                    SELECT id FROM artifacts WHERE revision_id = ?1 AND (
                        ocr_revision_id IS NOT NULL OR kind IN ('translation', 'explanation', 'lens_formula', 'lens_figure', 'lens_table')
                    )
                )",
                params![revision_id],
            )
            .map_err(|error| error.to_string())?;

        // 2. Delete artifact_heads for OCR-dependent artifacts
        transaction
            .execute(
                "DELETE FROM artifact_heads WHERE artifact_id IN (
                    SELECT id FROM artifacts WHERE revision_id = ?1 AND (
                        ocr_revision_id IS NOT NULL OR kind IN ('translation', 'explanation', 'lens_formula', 'lens_figure', 'lens_table')
                    )
                )",
                params![revision_id],
            )
            .map_err(|error| error.to_string())?;

        // 3. Delete OCR-dependent artifacts (keeping brief, metadata, glossary, symbol_table, context_compaction)
        transaction
            .execute(
                "DELETE FROM artifacts WHERE revision_id = ?1 AND (
                    ocr_revision_id IS NOT NULL OR kind IN ('translation', 'explanation', 'lens_formula', 'lens_figure', 'lens_table')
                )",
                params![revision_id],
            )
            .map_err(|error| error.to_string())?;

        // 4. Delete reading guide heads if table exists
        let _ = transaction.execute(
            "DELETE FROM reading_guide_heads WHERE revision_id = ?1",
            params![revision_id],
        );

        // 5. Delete from ocr_revisions (foreign keys cascade to ocr_pages, ocr_blocks, reading_guide_layers, outline_overviews)
        let deleted = transaction
            .execute(
                "DELETE FROM ocr_revisions WHERE revision_id = ?1",
                params![revision_id],
            )
            .map_err(|error| error.to_string())?;

        if deleted > 0 {
            bump_library_revisions(&transaction, &[LibraryDomain::Artifacts])
                .map_err(|error| error.to_string())?;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(deleted > 0)
    }

    pub fn append_lens_qa(
        &self,
        lens_artifact_id: &str,
        parent_id: Option<&str>,
        role: &str,
        content: &str,
        provider_node_id: Option<&str>,
        status: &str,
    ) -> ArtifactResult<LensQaProjection> {
        if !matches!(role, "user" | "assistant")
            || !matches!(status, "streaming" | "complete" | "cancelled" | "failed")
            || content.trim().is_empty()
        {
            return Err("Lens QA message is invalid".to_string());
        }
        let connection = self.connect()?;
        let lens_kind: Option<String> = connection
            .query_row(
                "SELECT kind FROM artifacts WHERE id = ?1",
                params![lens_artifact_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if !lens_kind.is_some_and(|kind| kind.starts_with("lens_")) {
            return Err("Lens QA can only attach to a Lens Artifact".to_string());
        }
        let id = Uuid::new_v4().to_string();
        let timestamp = now();
        connection
            .execute(
                "INSERT INTO lens_qa(
                   id, lens_artifact_id, parent_id, role, content, status,
                   provider_node_id, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
                params![
                    id,
                    lens_artifact_id,
                    parent_id,
                    role,
                    content,
                    status,
                    provider_node_id,
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(LensQaProjection {
            id,
            lens_artifact_id: lens_artifact_id.to_string(),
            parent_id: parent_id.map(str::to_string),
            role: role.to_string(),
            content: content.to_string(),
            status: status.to_string(),
            provider_node_id: provider_node_id.map(str::to_string),
            created_at: timestamp,
        })
    }

    pub fn list_lens_qa(&self, lens_artifact_id: &str) -> ArtifactResult<Vec<LensQaProjection>> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT id, lens_artifact_id, parent_id, role, content, status,
                        provider_node_id, created_at
                 FROM lens_qa WHERE lens_artifact_id = ?1 ORDER BY created_at",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![lens_artifact_id], |row| {
                Ok(LensQaProjection {
                    id: row.get(0)?,
                    lens_artifact_id: row.get(1)?,
                    parent_id: row.get(2)?,
                    role: row.get(3)?,
                    content: row.get(4)?,
                    status: row.get(5)?,
                    provider_node_id: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())
    }

    fn connect(&self) -> ArtifactResult<Connection> {
        db::open(&self.database_path).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
pub fn remap_block(old: &OcrBlockProjection, candidates: &[OcrBlockProjection]) -> Option<String> {
    let matches = candidates
        .iter()
        .filter(|candidate| {
            candidate.page_number == old.page_number
                && candidate.block_type == old.block_type
                && ((candidate.content_digest == old.content_digest
                    && bbox_iou(old.bbox, candidate.bbox) >= 0.5)
                    || (text_similarity(&old.text_content, &candidate.text_content) >= 0.9
                        && bbox_iou(old.bbox, candidate.bbox) >= 0.8))
        })
        .collect::<Vec<_>>();
    if matches.len() == 1 {
        Some(matches[0].id.clone())
    } else {
        None
    }
}

fn artifact_from_row(row: &Row<'_>) -> rusqlite::Result<ArtifactProjection> {
    let content: String = row.get(8)?;
    let evidence: String = row.get(9)?;
    let dependencies: String = row.get(10)?;
    Ok(ArtifactProjection {
        id: row.get(0)?,
        paper_id: row.get(1)?,
        revision_id: row.get(2)?,
        ocr_revision_id: row.get(3)?,
        kind: row.get(4)?,
        object_key: row.get(5)?,
        version: row.get(6)?,
        status: row.get(7)?,
        content: serde_json::from_str(&content).unwrap_or(Value::Null),
        overrides: serde_json::json!({}),
        evidence: serde_json::from_str(&evidence).unwrap_or_default(),
        dependency_snapshot: serde_json::from_str(&dependencies)
            .unwrap_or_else(|_| serde_json::json!({})),
        provider_node_id: row.get(11)?,
        created_at: row.get(12)?,
    })
}

fn validate_artifact_draft(draft: &ArtifactDraft) -> ArtifactResult<()> {
    if draft.paper_id.trim().is_empty()
        || draft.revision_id.trim().is_empty()
        || draft.kind.trim().is_empty()
        || !draft.content.is_object()
        || !draft.dependency_snapshot.is_object()
    {
        return Err("Artifact draft is incomplete".to_string());
    }
    for evidence in &draft.evidence {
        if evidence.page_number < 1 || evidence.revision_id != draft.revision_id {
            return Err("Artifact evidence does not match its Document Revision".to_string());
        }
        if let Some(bbox) = evidence.bbox {
            validate_bbox(bbox)?;
        }
    }
    Ok(())
}

fn validate_ocr_pages(pages: &[OcrPageInput]) -> ArtifactResult<()> {
    if pages.is_empty() {
        return Err("OCR response contains no pages".to_string());
    }
    let mut previous_page = 0;
    let mut block_count = 0;
    for page in pages {
        if page.page_number < 1 || page.page_number <= previous_page {
            return Err("OCR pages must be unique and ordered".to_string());
        }
        previous_page = page.page_number;
        let mut previous_index = -1;
        for block in &page.blocks {
            if block.block_index <= previous_index
                || block.block_type.trim().is_empty()
                || block.text_content.trim().is_empty()
            {
                return Err("OCR block is empty or out of order".to_string());
            }
            previous_index = block.block_index;
            validate_bbox(block.bbox)?;
            block_count += 1;
        }
    }
    if block_count == 0 {
        return Err("OCR response has Markdown but no locatable text Blocks".to_string());
    }
    Ok(())
}

fn validate_bbox(bbox: [i64; 4]) -> ArtifactResult<()> {
    if bbox.iter().any(|value| !(0..=1000).contains(value))
        || bbox[0] > bbox[2]
        || bbox[1] > bbox[3]
    {
        return Err("BBox must use [x0,y0,x1,y1] in normalized 0-1000 coordinates".to_string());
    }
    Ok(())
}

#[cfg(test)]
fn bbox_iou(left: [i64; 4], right: [i64; 4]) -> f64 {
    let x0 = left[0].max(right[0]);
    let y0 = left[1].max(right[1]);
    let x1 = left[2].min(right[2]);
    let y1 = left[3].min(right[3]);
    let intersection = ((x1 - x0).max(0) * (y1 - y0).max(0)) as f64;
    let left_area = ((left[2] - left[0]).max(0) * (left[3] - left[1]).max(0)) as f64;
    let right_area = ((right[2] - right[0]).max(0) * (right[3] - right[1]).max(0)) as f64;
    let union = left_area + right_area - intersection;
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

#[cfg(test)]
fn text_similarity(left: &str, right: &str) -> f64 {
    let left = left
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let right = right
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let maximum = left.chars().count().max(right.chars().count());
    if maximum == 0 {
        return 1.0;
    }
    1.0 - levenshtein(&left, &right) as f64 / maximum as f64
}

#[cfg(test)]
fn levenshtein(left: &str, right: &str) -> usize {
    let right = right.chars().collect::<Vec<_>>();
    let mut previous = (0..=right.len()).collect::<Vec<_>>();
    for (left_index, left_char) in left.chars().enumerate() {
        let mut current = vec![left_index + 1];
        for (right_index, right_char) in right.iter().enumerate() {
            current.push(
                (previous[right_index + 1] + 1)
                    .min(current[right_index] + 1)
                    .min(previous[right_index] + usize::from(left_char != *right_char)),
            );
        }
        previous = current;
    }
    previous[right.len()]
}

fn content_digest(content: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(content.trim().as_bytes());
    format!("{:x}", hasher.finalize())
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paper_module::PaperModule;
    use crate::v2_workspace::WorkspaceModule;
    use std::fs;
    use tempfile::tempdir;

    struct Fixture {
        _root: tempfile::TempDir,
        _workspace: WorkspaceModule,
        database_path: PathBuf,
        paper_id: String,
        revision_id: String,
    }

    fn fixture() -> Fixture {
        let root = tempdir().expect("Workspace");
        let workspace = WorkspaceModule::new();
        let projection = workspace.open(root.path()).expect("initialize");
        let source_root = tempdir().expect("source");
        let source = source_root.path().join("paper.pdf");
        fs::write(&source, b"%PDF-1.4\n1 0 obj <</Type /Page>>\n%%EOF").expect("PDF");
        let paper = PaperModule::open(root.path())
            .expect("PaperModule")
            .import_pdf(&source, Some("Inbox"))
            .expect("import")
            .paper
            .expect("paper");
        Fixture {
            _root: root,
            _workspace: workspace,
            database_path: projection.database_path,
            paper_id: paper.id,
            revision_id: paper.revision_id,
        }
    }

    fn draft(fixture: &Fixture, kind: &str, object_key: &str, value: i64) -> ArtifactDraft {
        ArtifactDraft {
            paper_id: fixture.paper_id.clone(),
            revision_id: fixture.revision_id.clone(),
            ocr_revision_id: None,
            kind: kind.to_string(),
            object_key: object_key.to_string(),
            content: serde_json::json!({"value": value}),
            evidence: vec![EvidenceAnchor {
                revision_id: fixture.revision_id.clone(),
                page_number: 1,
                block_id: None,
                bbox: None,
                excerpt: Some("evidence".to_string()),
            }],
            dependency_snapshot: serde_json::json!({"revisionId": fixture.revision_id}),
            provider_node_id: None,
        }
    }

    #[test]
    fn prepared_publication_rolls_back_projection_and_head_and_ignores_late_jobs() {
        let fixture = fixture();
        let module = ArtifactModule::open(&fixture.database_path).unwrap();
        let mut first = draft(&fixture, "metadata", "", 1);
        first.dependency_snapshot =
            serde_json::json!({"jobId":"job-new","jobCreatedAt":"2026-09-08T10:00:00Z"});
        let saved = module.publish(first.clone()).unwrap();
        let mut failed = draft(&fixture, "metadata", "", 2);
        failed.dependency_snapshot =
            serde_json::json!({"jobId":"job-failed","jobCreatedAt":"2026-09-08T11:00:00Z"});
        let before: String = module
            .connect()
            .unwrap()
            .query_row(
                "SELECT title FROM paper_metadata WHERE revision_id=?1",
                params![fixture.revision_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(module
            .publish_prepared(failed, |c, _| {
                c.execute(
                    "UPDATE paper_metadata SET title='should roll back' WHERE revision_id=?1",
                    params![fixture.revision_id],
                )
                .unwrap();
                Err("injected failure".into())
            })
            .is_err());
        assert_eq!(
            module
                .head(&fixture.paper_id, "metadata", "")
                .unwrap()
                .unwrap()
                .id,
            saved.id
        );
        assert_eq!(
            module
                .connect()
                .unwrap()
                .query_row(
                    "SELECT title FROM paper_metadata WHERE revision_id=?1",
                    params![fixture.revision_id],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            before
        );
        assert_eq!(
            module
                .publish_prepared(first, |_, _| panic!("retry must not repeat publication"))
                .unwrap()
                .id,
            saved.id
        );
        let mut late = draft(&fixture, "metadata", "", 3);
        late.dependency_snapshot =
            serde_json::json!({"jobId":"job-old","jobCreatedAt":"2026-09-08T09:00:00Z"});
        assert_eq!(
            module
                .publish_prepared(late, |_, _| panic!(
                    "late result must not replace projection"
                ))
                .unwrap()
                .id,
            saved.id
        );
    }
    #[test]
    fn publishes_immutable_versions_and_moves_only_the_head() {
        let fixture = fixture();
        let module = ArtifactModule::open(&fixture.database_path).expect("module");
        let first = module
            .publish(draft(&fixture, "brief", "", 1))
            .expect("first");
        let second = module
            .publish(draft(&fixture, "brief", "", 2))
            .expect("second");

        assert_eq!(first.version, 1);
        assert_eq!(second.version, 2);
        assert_eq!(
            module
                .head(&fixture.paper_id, "brief", "")
                .expect("head")
                .unwrap()
                .id,
            second.id
        );
        assert_eq!(module.get(&first.id).expect("history").content["value"], 1);
    }

    #[test]
    fn batch_validation_failure_publishes_no_partial_heads() {
        let fixture = fixture();
        let module = ArtifactModule::open(&fixture.database_path).expect("module");
        let valid = draft(&fixture, "brief", "", 1);
        let mut invalid = draft(&fixture, "glossary", "", 1);
        invalid.content = Value::Null;

        assert!(module.publish_batch(vec![valid, invalid]).is_err());
        assert!(module
            .head(&fixture.paper_id, "brief", "")
            .expect("brief head")
            .is_none());
        assert!(module
            .head(&fixture.paper_id, "glossary", "")
            .expect("glossary head")
            .is_none());
    }

    #[test]
    fn invalid_ocr_is_rejected_without_partial_rows() {
        let fixture = fixture();
        let module = ArtifactModule::open(&fixture.database_path).expect("module");
        let error = module
            .publish_ocr(
                &fixture.revision_id,
                "mistral",
                "mistral-ocr-latest",
                None,
                &[OcrPageInput {
                    page_number: 1,
                    width: None,
                    height: None,
                    markdown: Some("markdown only".to_string()),
                    blocks: Vec::new(),
                }],
            )
            .expect_err("missing blocks");
        assert!(error.contains("no locatable"));
        assert!(module
            .latest_ocr(&fixture.revision_id)
            .expect("OCR")
            .is_none());
    }

    #[test]
    fn lens_regeneration_deletes_old_qa_only_after_successful_publish() {
        let fixture = fixture();
        let module = ArtifactModule::open(&fixture.database_path).expect("module");
        let first = module
            .publish(draft(&fixture, "lens_figure", "block-1", 1))
            .expect("first Lens");
        module
            .append_lens_qa(&first.id, None, "user", "why?", None, "complete")
            .expect("QA");
        let mut invalid = draft(&fixture, "lens_figure", "block-1", 2);
        invalid.content = Value::Null;
        assert!(module.publish(invalid).is_err());
        assert_eq!(module.list_lens_qa(&first.id).expect("old QA").len(), 1);

        let second = module
            .publish(draft(&fixture, "lens_figure", "block-1", 2))
            .expect("regenerate");
        assert_ne!(first.id, second.id);
        assert!(module
            .list_lens_qa(&first.id)
            .expect("cleared QA")
            .is_empty());
    }

    #[test]
    fn glossary_overrides_are_projected_without_mutating_the_artifact_revision() {
        let fixture = fixture();
        let module = ArtifactModule::open(&fixture.database_path).expect("module");
        let mut glossary = draft(&fixture, "glossary", "", 1);
        glossary.content = serde_json::json!({
            "entries": [{"term": "Cache", "definition": "Original"}]
        });
        let artifact = module.publish(glossary).expect("glossary");

        module
            .set_override(
                &artifact.id,
                "glossary",
                "Cache",
                &serde_json::json!({"definition": "User wording"}),
            )
            .expect("override");
        let projected = module.get(&artifact.id).expect("projected artifact");

        assert_eq!(projected.content["entries"][0]["definition"], "Original");
        assert_eq!(projected.overrides["Cache"]["definition"], "User wording");
        assert!(module
            .set_override(
                &artifact.id,
                "symbol_table",
                "Cache",
                &serde_json::json!({"meaning": "wrong kind"}),
            )
            .is_err());
    }

    #[test]
    fn large_glossary_projection_batches_artifact_and_override_ids() {
        let fixture = fixture();
        let module = ArtifactModule::open(&fixture.database_path).expect("module");
        let count = SQLITE_BIND_BATCH_SIZE + 1;
        let drafts = (0..count)
            .map(|index| {
                draft(
                    &fixture,
                    "glossary",
                    &format!("entry-{index}"),
                    index as i64,
                )
            })
            .collect::<Vec<_>>();
        let published = module.publish_batch(drafts).expect("batch publish");
        assert_eq!(published.len(), count);

        module
            .set_override(
                &published[0].id,
                "glossary",
                "first",
                &serde_json::json!({"definition": "first override"}),
            )
            .expect("first override");
        module
            .set_override(
                &published[count - 1].id,
                "glossary",
                "last",
                &serde_json::json!({"definition": "last override"}),
            )
            .expect("last override");

        let listed = module.list(&fixture.paper_id).expect("list artifacts");
        assert_eq!(listed.len(), count);
        let first = listed
            .iter()
            .find(|artifact| artifact.id == published[0].id)
            .expect("first artifact");
        let last = listed
            .iter()
            .find(|artifact| artifact.id == published[count - 1].id)
            .expect("last artifact");
        assert_eq!(first.overrides["first"]["definition"], "first override");
        assert_eq!(last.overrides["last"]["definition"], "last override");
    }

    #[test]
    fn reocr_remap_requires_one_unique_high_confidence_candidate() {
        let old = OcrBlockProjection {
            id: "old".to_string(),
            page_number: 2,
            block_index: 0,
            block_type: "text".to_string(),
            text_content: "A stable scientific sentence".to_string(),
            content_digest: content_digest("A stable scientific sentence"),
            bbox: [100, 100, 500, 200],
        };
        let candidate = OcrBlockProjection {
            id: "new".to_string(),
            ..old.clone()
        };
        assert_eq!(
            remap_block(&old, std::slice::from_ref(&candidate)),
            Some("new".to_string())
        );
        assert_eq!(remap_block(&old, &[candidate.clone(), candidate]), None);
    }

    #[test]
    fn deletes_artifact_and_updates_heads_or_removes_head() {
        let fixture = fixture();
        let module = ArtifactModule::open(&fixture.database_path).expect("module");
        let v1 = module
            .publish(draft(&fixture, "lens_figure", "block1", 100))
            .expect("v1");
        let v2 = module
            .publish(draft(&fixture, "lens_figure", "block1", 200))
            .expect("v2");
        assert_eq!(v2.version, 2);

        let head = module
            .head(&fixture.paper_id, "lens_figure", "block1")
            .expect("head")
            .expect("found");
        assert_eq!(head.id, v2.id);

        // Deleting v2 should promote v1 back to head
        let remaining_head = module
            .delete(&v2.id)
            .expect("delete v2")
            .expect("remaining head");
        assert_eq!(remaining_head.id, v1.id);

        // Deleting v1 should remove head completely
        let empty_head = module.delete(&v1.id).expect("delete v1");
        assert!(empty_head.is_none());
        assert!(module
            .head(&fixture.paper_id, "lens_figure", "block1")
            .expect("head")
            .is_none());
    }

    #[test]
    fn delete_ocr_cascade_cleans_dependent_artifacts_and_preserves_brief() {
        let fixture = fixture();
        let module = ArtifactModule::open(&fixture.database_path).expect("module");

        // Publish OCR
        module
            .publish_ocr(
                &fixture.revision_id,
                "mistral",
                "mistral-ocr-latest",
                None,
                &[OcrPageInput {
                    page_number: 1,
                    width: Some(1000),
                    height: Some(1000),
                    markdown: Some("markdown".to_string()),
                    blocks: vec![OcrBlockInput {
                        block_index: 0,
                        block_type: "text".to_string(),
                        text_content: "content".to_string(),
                        bbox: [0, 0, 100, 100],
                    }],
                }],
            )
            .expect("publish ocr");

        // Publish Brief (not OCR-dependent)
        let brief = module
            .publish(draft(&fixture, "brief", "", 50))
            .expect("brief");
        // Publish Lens (OCR-dependent)
        let lens = module
            .publish(draft(&fixture, "lens_figure", "block1", 100))
            .expect("lens");

        assert!(module.get(&brief.id).is_ok());
        assert!(module.get(&lens.id).is_ok());
        assert!(module
            .latest_ocr(&fixture.revision_id)
            .expect("ocr")
            .is_some());

        // Perform cascade delete
        let deleted = module
            .delete_ocr_cascade(&fixture.revision_id)
            .expect("delete cascade");
        assert!(deleted);

        // OCR and Lens must be gone
        assert!(module
            .latest_ocr(&fixture.revision_id)
            .expect("ocr")
            .is_none());
        assert!(module.get(&lens.id).is_err());

        // Brief must be safely preserved
        assert!(module.get(&brief.id).is_ok());
    }
}
