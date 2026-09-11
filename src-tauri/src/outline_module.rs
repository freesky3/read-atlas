use crate::db;
use crate::outline_catalog::{build_outline_catalog, CatalogSourceBlock, OutlineCatalog};
use crate::outline_map::{OutlineGraphV4, MAP_PROTOCOL};
use crate::outline_protocol::{OutlineUnit, COMPOSE_PROTOCOL, EXTRACT_PROTOCOL};
use crate::outline_validate::OutlineGraphOutcome;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use uuid::Uuid;
pub type OutlineResult<T> = Result<T, String>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum OutlineStatus {
    MissingOcr,
    ReadyToPlan,
    #[allow(dead_code)]
    Planned,
    Generating,
    Partial,
    Published,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineHeadProjection {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub ocr_revision_id: String,
    pub catalog_digest: String,
    pub protocol_version: String,
    pub coverage_warnings: Vec<String>,
    pub graph: Option<Value>,
    pub units: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_status: Option<String>,
    pub review_notes: Vec<String>,
    pub gaps: Vec<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineProjection {
    pub status: OutlineStatus,
    pub revision_id: String,
    pub ocr_revision_id: Option<String>,
    pub has_paper_root: bool,
    pub catalog: Option<OutlineCatalog>,
    pub head: Option<OutlineHeadProjection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_attempt: Option<OutlineHeadProjection>,
    pub active_job_id: Option<String>,
    pub coverage_warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlinePlan {
    pub revision_id: String,
    pub ocr_revision_id: String,
    pub catalog_digest: String,
    pub model: String,
    pub extract_calls: i64,
    pub compose_calls: i64,
    pub max_repair_calls: i64,
    pub page_count: i64,
    pub catalog_token_estimate: i64,
    pub pdf_token_estimate: i64,
    pub estimated_cost: Option<String>,
    pub has_paper_root: bool,
    pub supports_native_pdf: bool,
    pub root_calls: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_digest: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publish_id: Option<String>,
    #[serde(default)]
    pub protocol_version: String,
    #[serde(default)]
    pub workflow: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_head_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OutlineModule {
    database_path: PathBuf,
}

impl OutlineModule {
    pub fn open(database_path: impl AsRef<Path>) -> OutlineResult<Self> {
        let database_path = database_path.as_ref().to_path_buf();
        if !database_path.is_file() {
            return Err("Workspace database is unavailable".to_string());
        }
        Ok(Self { database_path })
    }

    pub fn project(
        &self,
        revision_id: &str,
        paper_model: Option<&str>,
    ) -> OutlineResult<OutlineProjection> {
        self.project_for(revision_id, "gemini", paper_model)
    }

    pub fn project_for(
        &self,
        revision_id: &str,
        provider: &str,
        paper_model: Option<&str>,
    ) -> OutlineResult<OutlineProjection> {
        let connection = self.connect()?;
        if !revision_exists(&connection, revision_id)? {
            return Err("Document revision was not found".to_string());
        }
        let ocr_revision_id = latest_ready_ocr(&connection, revision_id)?;
        let has_paper_root = match paper_model {
            Some(model) => has_active_paper_root(&connection, revision_id, provider, model)?,
            None => false,
        };
        let Some(ocr_revision_id) = ocr_revision_id else {
            return Ok(OutlineProjection {
                status: OutlineStatus::MissingOcr,
                revision_id: revision_id.to_string(),
                ocr_revision_id: None,
                has_paper_root,
                catalog: None,
                head: None,
                latest_attempt: None,
                active_job_id: None,
                coverage_warnings: Vec::new(),
            });
        };

        let catalog = Some(self.build_catalog_for_ocr(&ocr_revision_id)?);
        let active_job_id = active_outline_job(&connection, revision_id)?;
        let head = load_head(&connection, revision_id)?;

        let status = if active_job_id.is_some() {
            OutlineStatus::Generating
        } else if let Some(head) = &head {
            if head.ocr_revision_id != ocr_revision_id {
                OutlineStatus::Stale
            } else if head.status == "partial" {
                OutlineStatus::Partial
            } else {
                OutlineStatus::Published
            }
        } else {
            OutlineStatus::ReadyToPlan
        };

        let coverage_warnings = head
            .as_ref()
            .map(|head| head.coverage_warnings.clone())
            .unwrap_or_default();

        Ok(OutlineProjection {
            status,
            revision_id: revision_id.to_string(),
            ocr_revision_id: Some(ocr_revision_id),
            has_paper_root,
            catalog,
            head,
            latest_attempt: load_latest_attempt(&connection, revision_id)?,
            active_job_id,
            coverage_warnings,
        })
    }

    /// Route-aware read projection. It uses only persisted, non-secret route
    /// identity, so a missing credential never hides an already published head.
    #[allow(dead_code)]
    pub(crate) fn project_for_route(
        &self,
        revision_id: &str,
        route_id: &str,
        paper_model: Option<&str>,
    ) -> OutlineResult<OutlineProjection> {
        if route_id.trim().is_empty() {
            return Err("Provider route is required".to_string());
        }
        let connection = self.connect()?;
        if !revision_exists(&connection, revision_id)? {
            return Err("Document revision was not found".to_string());
        }
        let ocr_revision_id = latest_ready_ocr(&connection, revision_id)?;
        let has_paper_root = match paper_model {
            Some(model) => {
                has_active_paper_root_for_route(&connection, revision_id, route_id, model)?
            }
            None => false,
        };
        let Some(ocr_revision_id) = ocr_revision_id else {
            return Ok(OutlineProjection {
                status: OutlineStatus::MissingOcr,
                revision_id: revision_id.to_string(),
                ocr_revision_id: None,
                has_paper_root,
                catalog: None,
                head: None,
                latest_attempt: None,
                active_job_id: None,
                coverage_warnings: Vec::new(),
            });
        };

        let catalog = Some(self.build_catalog_for_ocr(&ocr_revision_id)?);
        let active_job_id = active_outline_job_for_route(&connection, revision_id, route_id)?;
        let head = load_head(&connection, revision_id)?;
        let status = if active_job_id.is_some() {
            OutlineStatus::Generating
        } else if let Some(head) = &head {
            if head.ocr_revision_id != ocr_revision_id {
                OutlineStatus::Stale
            } else if head.status == "partial" {
                OutlineStatus::Partial
            } else {
                OutlineStatus::Published
            }
        } else {
            OutlineStatus::ReadyToPlan
        };
        let coverage_warnings = head
            .as_ref()
            .map(|head| head.coverage_warnings.clone())
            .unwrap_or_default();

        Ok(OutlineProjection {
            status,
            revision_id: revision_id.to_string(),
            ocr_revision_id: Some(ocr_revision_id),
            has_paper_root,
            catalog,
            head,
            latest_attempt: load_latest_attempt(&connection, revision_id)?,
            active_job_id,
            coverage_warnings,
        })
    }

    /// Safe local projection used when an exact Provider route cannot be
    /// captured. It retains local published data but never attributes a root
    /// or active work item from another route to the current UI.
    pub(crate) fn project_without_route(
        &self,
        revision_id: &str,
    ) -> OutlineResult<OutlineProjection> {
        let connection = self.connect()?;
        if !revision_exists(&connection, revision_id)? {
            return Err("Document revision was not found".to_string());
        }
        let ocr_revision_id = latest_ready_ocr(&connection, revision_id)?;
        let Some(ocr_revision_id) = ocr_revision_id else {
            return Ok(OutlineProjection {
                status: OutlineStatus::MissingOcr,
                revision_id: revision_id.to_string(),
                ocr_revision_id: None,
                has_paper_root: false,
                catalog: None,
                head: None,
                latest_attempt: None,
                active_job_id: None,
                coverage_warnings: Vec::new(),
            });
        };
        let catalog = Some(self.build_catalog_for_ocr(&ocr_revision_id)?);
        let head = load_head(&connection, revision_id)?;
        let status = if let Some(head) = &head {
            if head.ocr_revision_id != ocr_revision_id {
                OutlineStatus::Stale
            } else if head.status == "partial" {
                OutlineStatus::Partial
            } else {
                OutlineStatus::Published
            }
        } else {
            OutlineStatus::ReadyToPlan
        };
        let coverage_warnings = head
            .as_ref()
            .map(|head| head.coverage_warnings.clone())
            .unwrap_or_default();
        Ok(OutlineProjection {
            status,
            revision_id: revision_id.to_string(),
            ocr_revision_id: Some(ocr_revision_id),
            has_paper_root: false,
            catalog,
            head,
            latest_attempt: load_latest_attempt(&connection, revision_id)?,
            active_job_id: None,
            coverage_warnings,
        })
    }

    pub fn catalog_for_revision(
        &self,
        revision_id: &str,
    ) -> OutlineResult<(String, OutlineCatalog)> {
        let connection = self.connect()?;
        let ocr_revision_id = latest_ready_ocr(&connection, revision_id)?
            .ok_or_else(|| "Outline requires a published OCR revision".to_string())?;
        let catalog = self.build_catalog_for_ocr(&ocr_revision_id)?;
        Ok((ocr_revision_id, catalog))
    }

    pub fn revision_page_count(&self, revision_id: &str) -> OutlineResult<i64> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT COALESCE(page_count, 0) FROM document_revisions WHERE id = ?1",
                params![revision_id],
                |row| row.get(0),
            )
            .map_err(|_| "Document revision was not found".to_string())
    }

    pub fn has_paper_root(
        &self,
        revision_id: &str,
        provider: &str,
        model: &str,
    ) -> OutlineResult<bool> {
        has_active_paper_root(&self.connect()?, revision_id, provider, model)
    }

    pub(crate) fn has_paper_root_for_route(
        &self,
        revision_id: &str,
        route_id: &str,
        model: &str,
    ) -> OutlineResult<bool> {
        if route_id.trim().is_empty() {
            return Err("Provider route is required".to_string());
        }
        has_active_paper_root_for_route(&self.connect()?, revision_id, route_id, model)
    }

    pub fn paper_id_for_revision(&self, revision_id: &str) -> OutlineResult<String> {
        self.connect()?
            .query_row(
                "SELECT paper_id FROM document_revisions WHERE id = ?1",
                params![revision_id],
                |row| row.get(0),
            )
            .map_err(|_| "Document revision was not found".to_string())
    }

    pub fn revision_sha256(&self, revision_id: &str) -> OutlineResult<String> {
        self.connect()?
            .query_row(
                "SELECT sha256 FROM document_revisions WHERE id = ?1",
                params![revision_id],
                |row| row.get(0),
            )
            .map_err(|_| "Document revision was not found".to_string())
    }

    pub fn current_overview_id(&self, revision_id: &str) -> OutlineResult<Option<String>> {
        Ok(load_head(&self.connect()?, revision_id)?.map(|head| head.id))
    }

    pub fn current_overview_head(
        &self,
        revision_id: &str,
    ) -> OutlineResult<Option<OutlineHeadProjection>> {
        load_head(&self.connect()?, revision_id)
    }

    pub fn has_source_root(
        &self,
        revision_id: &str,
        route: &str,
        model: &str,
        prompt: &str,
    ) -> OutlineResult<bool> {
        let root = self
            .database_path
            .parent()
            .and_then(Path::parent)
            .ok_or("Workspace path unavailable")?;
        let source = crate::reading_artifact_module::ReadingArtifactModule::open(root)?;
        let facts = crate::reading_artifact_module::DocumentFacts {
            paper_id: self.paper_id_for_revision(revision_id)?,
            revision_id: revision_id.into(),
            revision_sha256: self.revision_sha256(revision_id)?,
            title: String::new(),
            pdf_path: PathBuf::new(),
        };
        source
            .lookup_root_for_route_with_instruction(&facts, route, model, Some(prompt))
            .map(|r| r.is_some())
            .map_err(|e| e.to_string())
    }

    pub fn check_frozen_inputs(&self, payload: &Value) -> OutlineResult<()> {
        self.load_plan(required_plan_text(payload, "planId")?)?;
        let revision = required_plan_text(payload, "revisionId")?;
        let ocr = required_plan_text(payload, "ocrRevisionId")?;
        let sha = required_plan_text(payload, "pdfHash")?;
        let (current_ocr, catalog) = self.catalog_for_revision(revision)?;
        if current_ocr != ocr
            || catalog.digest != required_plan_text(payload, "catalogDigest")?
            || self.revision_sha256(revision)? != sha
        {
            return Err("PDF、OCR 或目录已变化，请重新计划。".into());
        }
        check_document_snapshot(&self.connect()?, revision, ocr, sha)?;
        let current = self.current_overview_id(revision)?;
        let expected = if payload.get("nodeId").is_some() {
            payload["overviewRevisionId"].as_str()
        } else {
            payload["expectedHeadId"].as_str()
        };
        if current.as_deref() != expected {
            return Err("地图父版本已变化，请重新计划。".into());
        }
        if let Some(node) = payload["nodeId"].as_str() {
            let local = self.deep_dive_for_node(revision, node)?.map(|head| head.id);
            if local.as_deref() != payload["expectedLocalHeadId"].as_str() {
                return Err("局部图已更新或删除，请重新计划。".into());
            }
        }
        Ok(())
    }

    pub fn job_for_plan(&self, revision: &str, plan_id: &str) -> OutlineResult<Option<String>> {
        self.connect()?.query_row("SELECT id FROM jobs WHERE revision_id=?1 AND json_extract(payload_json,'$.planId')=?2 ORDER BY created_at LIMIT 1",params![revision,plan_id],|r|r.get(0)).optional().map_err(|e|e.to_string())
    }

    pub fn published_plan(&self, publish_id: &str) -> OutlineResult<bool> {
        self.connect()?.query_row("SELECT EXISTS(SELECT 1 FROM outline_plans WHERE json_extract(payload_json,'$.publishId')=?1 AND json_extract(payload_json,'$.publishedRevisionId') IS NOT NULL)",params![publish_id],|r|r.get(0)).map_err(|e|e.to_string())
    }

    pub fn save_plan(
        &self,
        plan_id: &str,
        revision_id: &str,
        ocr_revision_id: &str,
        catalog_digest: &str,
        model: &str,
        payload: &Value,
    ) -> OutlineResult<()> {
        let created_at = chrono::Utc::now().to_rfc3339();
        self.connect()?
            .execute(
                "INSERT INTO outline_plans(
                   id, revision_id, ocr_revision_id, catalog_digest, model, payload_json, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    plan_id,
                    revision_id,
                    ocr_revision_id,
                    catalog_digest,
                    model,
                    payload.to_string(),
                    created_at
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn load_plan(&self, plan_id: &str) -> OutlineResult<Value> {
        let raw: String = self
            .connect()?
            .query_row(
                "SELECT payload_json FROM outline_plans WHERE id = ?1",
                params![plan_id],
                |row| row.get(0),
            )
            .map_err(|_| "地图计划不存在或已过期，请重新计划。".to_string())?;
        let payload: Value = serde_json::from_str(&raw).map_err(|error| error.to_string())?;
        if payload["invalidated"].as_bool() == Some(true) {
            return Err("地图已删除，此计划已失效，请重新计划。".into());
        }
        Ok(payload)
    }

    pub fn save_candidate_overview(
        &self,
        revision_id: &str,
        ocr_revision_id: &str,
        catalog_digest: &str,
        graph: &OutlineGraphV4,
        publish_id: &str,
    ) -> OutlineResult<String> {
        let paper_id = self.paper_id_for_revision(revision_id)?;
        let id = format!("candidate:{publish_id}");
        let created_at = chrono::Utc::now().to_rfc3339();
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT OR IGNORE INTO outline_revisions(
                   id, paper_id, revision_id, ocr_revision_id, kind, status, protocol_version,
                   catalog_digest, units_json, graph_json, coverage_json, dependency_snapshot_json,
                   created_at
                 ) VALUES (?1, ?2, ?3, ?4, 'overview', 'partial', ?5, ?6, '[]', ?7, ?8, ?9, ?10)",
                params![
                    id,
                    paper_id,
                    revision_id,
                    ocr_revision_id,
                    MAP_PROTOCOL,
                    catalog_digest,
                    serde_json::to_string(graph).map_err(|error| error.to_string())?,
                    serde_json::json!({
                        "reviewStatus": "unchecked",
                        "warnings": []
                    })
                    .to_string(),
                    serde_json::json!({
                        "publishId": publish_id,
                        "generationStatus": "candidate"
                    })
                    .to_string(),
                    created_at
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(id)
    }

    pub fn publish_overview_v4(
        &self,
        revision_id: &str,
        ocr_revision_id: &str,
        catalog_digest: &str,
        graph: &OutlineGraphV4,
        review_notes: &[String],
        expected_head_id: Option<&str>,
        publish_id: &str,
    ) -> OutlineResult<OutlineHeadProjection> {
        if graph.nodes.is_empty() {
            return Err("没有有效节点，不能发布正式地图。".to_string());
        }
        let paper_id = self.paper_id_for_revision(revision_id)?;
        let mut connection = self.connect()?;
        let tx = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let existing: Option<String> = tx
            .query_row(
                "SELECT id FROM outline_revisions
                 WHERE json_extract(dependency_snapshot_json, '$.publishId') = ?1
                   AND status = 'published'
                 LIMIT 1",
                params![publish_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if existing.is_some() || plan_was_published(&tx, publish_id)? {
            tx.commit().map_err(|error| error.to_string())?;
            return load_head(&self.connect()?, revision_id)?
                .ok_or_else(|| "地图已发布".to_string());
        }
        let payload = publication_plan(&tx, publish_id)?;
        verify_publish_inputs(&tx, &payload, revision_id, ocr_revision_id, catalog_digest)?;
        let current: Option<String> = tx
            .query_row(
                "SELECT overview_revision_id FROM outline_heads WHERE revision_id = ?1",
                params![revision_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if current.as_deref() != expected_head_id {
            return Err("当前地图已被更新或删除，旧任务不能覆盖新结果。".to_string());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        let gaps = serde_json::to_value(&graph.gaps).unwrap_or(Value::Array(vec![]));
        let review_status = if graph.gaps.is_empty() {
            "reviewed"
        } else {
            "reviewed_with_gaps"
        };
        tx.execute(
            "INSERT INTO outline_revisions(
               id, paper_id, revision_id, ocr_revision_id, kind, status, protocol_version,
               catalog_digest, units_json, graph_json, coverage_json, dependency_snapshot_json,
               created_at
             ) VALUES (?1, ?2, ?3, ?4, 'overview', 'published', ?5, ?6, '[]', ?7, ?8, ?9, ?10)",
            params![
                id,
                paper_id,
                revision_id,
                ocr_revision_id,
                MAP_PROTOCOL,
                catalog_digest,
                serde_json::to_string(graph).map_err(|error| error.to_string())?,
                serde_json::json!({
                    "reviewStatus": review_status,
                    "reviewNotes": review_notes,
                    "gaps": gaps,
                    "warnings": []
                })
                .to_string(),
                serde_json::json!({
                    "publishId": publish_id,
                    "expectedHeadId": expected_head_id,
                    "generationStatus": "published"
                })
                .to_string(),
                created_at
            ],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT INTO outline_heads(revision_id, overview_revision_id, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(revision_id) DO UPDATE SET
               overview_revision_id = excluded.overview_revision_id,
               updated_at = excluded.updated_at",
            params![revision_id, id, created_at],
        )
        .map_err(|error| error.to_string())?;
        if let Some(previous_id) = current {
            if previous_id != id {
                tx.execute(
                    "DELETE FROM outline_revisions WHERE id = ?1",
                    params![previous_id],
                )
                .map_err(|error| error.to_string())?;
            }
        }
        mark_plan_published(&tx, publish_id, &id)?;
        tx.execute(
            "DELETE FROM outline_revisions WHERE id=?1 AND status='partial'",
            params![format!("candidate:{publish_id}")],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|error| error.to_string())?;
        load_head(&self.connect()?, revision_id)?
            .ok_or_else(|| "Outline head was not published".to_string())
    }

    pub fn publish_deep_dive_v4(
        &self,
        revision_id: &str,
        node_id: &str,
        overview_revision_id: &str,
        ocr_revision_id: &str,
        catalog_digest: &str,
        graph: &OutlineGraphV4,
        publish_id: &str,
    ) -> OutlineResult<OutlineHeadProjection> {
        let paper_id = self.paper_id_for_revision(revision_id)?;
        let mut connection = self.connect()?;
        let tx = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        if plan_was_published(&tx, publish_id)? {
            tx.commit().map_err(|e| e.to_string())?;
            return self
                .deep_dive_for_node(revision_id, node_id)?
                .ok_or_else(|| "已发布的局部图已被删除。".into());
        }
        let payload = publication_plan(&tx, publish_id)?;
        verify_publish_inputs(&tx, &payload, revision_id, ocr_revision_id, catalog_digest)?;
        let current_overview: String = tx
            .query_row(
                "SELECT overview_revision_id FROM outline_heads WHERE revision_id = ?1",
                params![revision_id],
                |row| row.get(0),
            )
            .map_err(|_| "Generate an Overview before a Deep dive".to_string())?;
        if current_overview != overview_revision_id {
            return Err("父图已更新，旧局部图任务不能挂到新节点上。".to_string());
        }
        let previous_id: Option<String> = tx
            .query_row(
                "SELECT deep_dive_revision_id FROM outline_deep_dive_heads
                 WHERE overview_revision_id = ?1 AND node_id = ?2",
                params![overview_revision_id, node_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if previous_id.as_deref() != payload["expectedLocalHeadId"].as_str() {
            return Err("当前局部图已更新或删除，旧任务不能覆盖新结果。".into());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        tx.execute(
            "INSERT INTO outline_revisions(
               id, paper_id, revision_id, ocr_revision_id, kind, parent_overview_id, parent_node_id,
               status, protocol_version, catalog_digest, units_json, graph_json, coverage_json,
               dependency_snapshot_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, 'deep_dive', ?5, ?6, 'published', ?7, ?8, '[]', ?9, ?10, ?11, ?12)",
            params![
                id,
                paper_id,
                revision_id,
                ocr_revision_id,
                overview_revision_id,
                node_id,
                crate::outline_map::DEEP_DIVE_PROTOCOL_V4,
                catalog_digest,
                serde_json::to_string(graph).map_err(|error| error.to_string())?,
                serde_json::json!({"warnings": [], "reviewStatus": "self_checked", "gaps": graph.gaps}).to_string(),
                serde_json::json!({
                    "nodeId": node_id,
                    "overviewRevisionId": overview_revision_id,
                    "publishId": publish_id
                })
                .to_string(),
                created_at
            ],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT INTO outline_deep_dive_heads(overview_revision_id, node_id, deep_dive_revision_id, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(overview_revision_id, node_id) DO UPDATE SET
               deep_dive_revision_id = excluded.deep_dive_revision_id,
               updated_at = excluded.updated_at",
            params![overview_revision_id, node_id, id, created_at],
        )
        .map_err(|error| error.to_string())?;
        if let Some(previous_id) = previous_id {
            if previous_id != id {
                tx.execute(
                    "DELETE FROM outline_revisions WHERE id = ?1",
                    params![previous_id],
                )
                .map_err(|error| error.to_string())?;
            }
        }
        mark_plan_published(&tx, publish_id, &id)?;
        tx.commit().map_err(|error| error.to_string())?;
        self.deep_dive_for_node(revision_id, node_id)?
            .ok_or_else(|| "Deep dive was not published".to_string())
    }

    pub fn orientation_context(&self, revision_id: &str) -> OutlineResult<Value> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT a.kind, a.content_json
                 FROM artifact_heads h
                 JOIN artifacts a ON a.id = h.artifact_id
                 WHERE a.revision_id = ?1 AND a.kind IN ('brief', 'glossary', 'symbol_table')",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![revision_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        let mut pack = serde_json::Map::new();
        for (kind, content) in rows {
            if let Ok(value) = serde_json::from_str::<Value>(&content) {
                pack.insert(kind, value);
            }
        }
        Ok(Value::Object(pack))
    }

    pub fn publish_overview(
        &self,
        revision_id: &str,
        ocr_revision_id: &str,
        catalog_digest: &str,
        units: &[OutlineUnit],
        outcome: &OutlineGraphOutcome,
    ) -> OutlineResult<OutlineHeadProjection> {
        let paper_id = self.paper_id_for_revision(revision_id)?;
        let previous_id = load_head(&self.connect()?, revision_id)?.map(|head| head.id);
        let id = Uuid::new_v4().to_string();
        let (status, graph_json, coverage_json) = match outcome {
            OutlineGraphOutcome::Valid { graph, warnings } => (
                "published",
                Some(serde_json::to_string(graph).map_err(|error| error.to_string())?),
                serde_json::to_string(&serde_json::json!({ "warnings": warnings }))
                    .map_err(|error| error.to_string())?,
            ),
            OutlineGraphOutcome::Partial { issues, .. } => (
                "partial",
                None,
                serde_json::to_string(&serde_json::json!({
                    "warnings": issues.iter().map(|issue| issue.message.clone()).collect::<Vec<_>>(),
                    "issues": issues.iter().map(|issue| issue.code.clone()).collect::<Vec<_>>()
                }))
                .map_err(|error| error.to_string())?,
            ),
        };
        let units_json = serde_json::to_string(units).map_err(|error| error.to_string())?;
        let created_at = chrono::Utc::now().to_rfc3339();
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO outline_revisions(
                   id, paper_id, revision_id, ocr_revision_id, kind, status, protocol_version,
                   catalog_digest, units_json, graph_json, coverage_json, dependency_snapshot_json,
                   created_at
                 ) VALUES (?1, ?2, ?3, ?4, 'overview', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    id,
                    paper_id,
                    revision_id,
                    ocr_revision_id,
                    status,
                    format!("{EXTRACT_PROTOCOL}+{COMPOSE_PROTOCOL}"),
                    catalog_digest,
                    units_json,
                    graph_json,
                    coverage_json,
                    serde_json::json!({
                        "ocrRevisionId": ocr_revision_id,
                        "catalogDigest": catalog_digest
                    })
                    .to_string(),
                    created_at
                ],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "INSERT INTO outline_heads(revision_id, overview_revision_id, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(revision_id) DO UPDATE SET
                   overview_revision_id = excluded.overview_revision_id,
                   updated_at = excluded.updated_at",
                params![revision_id, id, created_at],
            )
            .map_err(|error| error.to_string())?;
        if let Some(previous_id) = previous_id {
            if previous_id != id {
                connection
                    .execute(
                        "DELETE FROM outline_revisions WHERE id = ?1",
                        params![previous_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
        load_head(&connection, revision_id)?
            .ok_or_else(|| "Outline head was not published".to_string())
    }

    pub fn delete_overview(&self, revision_id: &str) -> OutlineResult<()> {
        let mut connection = self.connect()?;
        let tx = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        tx.execute("UPDATE outline_plans SET payload_json=json_set(payload_json,'$.invalidated',json('true')) WHERE revision_id=?1",params![revision_id]).map_err(|e|e.to_string())?;
        tx.execute(
            "DELETE FROM outline_revisions WHERE revision_id=?1",
            params![revision_id],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())
    }

    pub fn delete_deep_dive(&self, revision_id: &str, node_id: &str) -> OutlineResult<()> {
        let mut conn = self.connect()?;
        let connection = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        let overview = load_head(&connection, revision_id)?
            .ok_or_else(|| "Generate an Overview before a Deep dive".to_string())?;
        let deep_dive_id: Option<String> = connection
            .query_row(
                "SELECT deep_dive_revision_id FROM outline_deep_dive_heads
                 WHERE overview_revision_id = ?1 AND node_id = ?2",
                params![overview.id, node_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        connection.execute("UPDATE outline_plans SET payload_json=json_set(payload_json,'$.invalidated',json('true')) WHERE revision_id=?1 AND json_extract(payload_json,'$.overviewRevisionId')=?2 AND json_extract(payload_json,'$.nodeId')=?3",params![revision_id,overview.id,node_id]).map_err(|e|e.to_string())?;
        if let Some(deep_dive_id) = deep_dive_id {
            connection
                .execute(
                    "DELETE FROM outline_revisions WHERE id = ?1",
                    params![deep_dive_id],
                )
                .map_err(|error| error.to_string())?;
        }
        connection.commit().map_err(|e| e.to_string())
    }

    pub fn current_graph(&self, revision_id: &str) -> OutlineResult<Option<Value>> {
        Ok(load_head(&self.connect()?, revision_id)?.and_then(|head| head.graph))
    }

    pub fn publish_deep_dive(
        &self,
        revision_id: &str,
        node_id: &str,
        ocr_revision_id: &str,
        catalog_digest: &str,
        units: &[OutlineUnit],
        outcome: &OutlineGraphOutcome,
    ) -> OutlineResult<OutlineHeadProjection> {
        let paper_id = self.paper_id_for_revision(revision_id)?;
        let overview = load_head(&self.connect()?, revision_id)?
            .ok_or_else(|| "Generate an Overview before a Deep dive".to_string())?;
        if overview.status != "published" || overview.graph.is_none() {
            return Err("Deep dive requires a published Overview graph".to_string());
        }
        let previous_id: Option<String> = self
            .connect()?
            .query_row(
                "SELECT deep_dive_revision_id FROM outline_deep_dive_heads
                 WHERE overview_revision_id = ?1 AND node_id = ?2",
                params![overview.id, node_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let id = Uuid::new_v4().to_string();
        let (status, graph_json, coverage_json) = match outcome {
            OutlineGraphOutcome::Valid { graph, warnings } => (
                "published",
                Some(serde_json::to_string(graph).map_err(|error| error.to_string())?),
                serde_json::to_string(&serde_json::json!({ "warnings": warnings }))
                    .map_err(|error| error.to_string())?,
            ),
            OutlineGraphOutcome::Partial { issues, .. } => (
                "partial",
                None,
                serde_json::to_string(&serde_json::json!({
                    "warnings": issues.iter().map(|issue| issue.message.clone()).collect::<Vec<_>>()
                }))
                .map_err(|error| error.to_string())?,
            ),
        };
        let created_at = chrono::Utc::now().to_rfc3339();
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO outline_revisions(
                   id, paper_id, revision_id, ocr_revision_id, kind, parent_overview_id, parent_node_id,
                   status, protocol_version, catalog_digest, units_json, graph_json, coverage_json,
                   dependency_snapshot_json, created_at
                 ) VALUES (?1, ?2, ?3, ?4, 'deep_dive', ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    id,
                    paper_id,
                    revision_id,
                    ocr_revision_id,
                    overview.id,
                    node_id,
                    status,
                    crate::outline_protocol::DEEP_DIVE_PROTOCOL,
                    catalog_digest,
                    serde_json::to_string(units).map_err(|error| error.to_string())?,
                    graph_json,
                    coverage_json,
                    serde_json::json!({"nodeId": node_id, "overviewRevisionId": overview.id}).to_string(),
                    created_at
                ],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "INSERT INTO outline_deep_dive_heads(overview_revision_id, node_id, deep_dive_revision_id, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(overview_revision_id, node_id) DO UPDATE SET
                   deep_dive_revision_id = excluded.deep_dive_revision_id,
                   updated_at = excluded.updated_at",
                params![overview.id, node_id, id, created_at],
            )
            .map_err(|error| error.to_string())?;
        if let Some(previous_id) = previous_id {
            if previous_id != id {
                connection
                    .execute(
                        "DELETE FROM outline_revisions WHERE id = ?1",
                        params![previous_id],
                    )
                    .map_err(|error| error.to_string())?;
            }
        }
        Ok(OutlineHeadProjection {
            id,
            kind: "deep_dive".to_string(),
            status: status.to_string(),
            ocr_revision_id: ocr_revision_id.to_string(),
            catalog_digest: catalog_digest.to_string(),
            protocol_version: crate::outline_protocol::DEEP_DIVE_PROTOCOL.to_string(),
            coverage_warnings: Vec::new(),
            graph: graph_json.and_then(|text| serde_json::from_str(&text).ok()),
            units: serde_json::to_value(units).unwrap_or(Value::Array(vec![])),
            review_status: None,
            review_notes: Vec::new(),
            gaps: Vec::new(),
        })
    }

    pub fn deep_dive_for_node(
        &self,
        revision_id: &str,
        node_id: &str,
    ) -> OutlineResult<Option<OutlineHeadProjection>> {
        let connection = self.connect()?;
        let overview = match load_head(&connection, revision_id)? {
            Some(head) => head,
            None => return Ok(None),
        };
        connection
            .query_row(
                "SELECT r.id, r.kind, r.status, r.ocr_revision_id, r.catalog_digest,
                        r.protocol_version, r.coverage_json, r.graph_json, r.units_json
                 FROM outline_deep_dive_heads h
                 JOIN outline_revisions r ON r.id = h.deep_dive_revision_id
                 WHERE h.overview_revision_id = ?1 AND h.node_id = ?2",
                params![overview.id, node_id],
                |row| {
                    let coverage_json: String = row.get(6)?;
                    let graph_json: Option<String> = row.get(7)?;
                    let units_json: String = row.get(8)?;
                    Ok(OutlineHeadProjection {
                        id: row.get(0)?,
                        kind: row.get(1)?,
                        status: row.get(2)?,
                        ocr_revision_id: row.get(3)?,
                        catalog_digest: row.get(4)?,
                        protocol_version: row.get(5)?,
                        review_status: coverage_field(&coverage_json, "reviewStatus")
                            .as_str()
                            .map(str::to_string),
                        review_notes: serde_json::from_value(coverage_field(
                            &coverage_json,
                            "reviewNotes",
                        ))
                        .unwrap_or_default(),
                        gaps: serde_json::from_value(coverage_field(&coverage_json, "gaps"))
                            .unwrap_or_default(),
                        coverage_warnings: coverage_warnings_from_json(&coverage_json),
                        graph: graph_json.and_then(|text| serde_json::from_str(&text).ok()),
                        units: serde_json::from_str(&units_json).unwrap_or(Value::Array(vec![])),
                    })
                },
            )
            .optional()
            .map_err(|error| error.to_string())
    }

    fn build_catalog_for_ocr(&self, ocr_revision_id: &str) -> OutlineResult<OutlineCatalog> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT b.id, p.page_number, b.block_index, b.block_type, b.text_content,
                        b.x0, b.y0, b.x1, b.y1
                 FROM ocr_blocks b
                 JOIN ocr_pages p ON p.id = b.ocr_page_id
                 WHERE p.ocr_revision_id = ?1
                 ORDER BY p.page_number, b.block_index",
            )
            .map_err(|error| error.to_string())?;
        let blocks = statement
            .query_map(params![ocr_revision_id], |row| {
                Ok(CatalogSourceBlock {
                    id: row.get(0)?,
                    page: row.get(1)?,
                    block_index: row.get(2)?,
                    block_type: row.get(3)?,
                    text_content: row.get(4)?,
                    bbox: [row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?],
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        Ok(build_outline_catalog(&blocks))
    }

    fn connect(&self) -> OutlineResult<Connection> {
        db::open(&self.database_path).map_err(|error| error.to_string())
    }
}

fn revision_exists(connection: &Connection, revision_id: &str) -> OutlineResult<bool> {
    connection
        .query_row(
            "SELECT 1 FROM document_revisions WHERE id = ?1",
            params![revision_id],
            |_| Ok(()),
        )
        .optional()
        .map(|row| row.is_some())
        .map_err(|error| error.to_string())
}

fn latest_ready_ocr(connection: &Connection, revision_id: &str) -> OutlineResult<Option<String>> {
    connection
        .query_row(
            "SELECT id FROM ocr_revisions
             WHERE revision_id = ?1 AND status = 'ready'
             ORDER BY published_at DESC LIMIT 1",
            params![revision_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn has_active_paper_root_for_route(
    connection: &Connection,
    revision_id: &str,
    route_id: &str,
    model: &str,
) -> OutlineResult<bool> {
    connection
        .query_row(
            "SELECT 1 FROM context_roots
             WHERE revision_id = ?1 AND provider_route_id = ?2 AND model = ?3
               AND invalidated_at IS NULL
               AND provider_file_id IS NOT NULL
             LIMIT 1",
            params![revision_id, route_id, model],
            |_| Ok(()),
        )
        .optional()
        .map(|row| row.is_some())
        .map_err(|error| error.to_string())
}

fn has_active_paper_root(
    connection: &Connection,
    revision_id: &str,
    provider: &str,
    model: &str,
) -> OutlineResult<bool> {
    connection
        .query_row(
            "SELECT 1 FROM context_roots
             WHERE revision_id = ?1 AND provider = ?2 AND model = ?3
               AND invalidated_at IS NULL
               AND provider_file_id IS NOT NULL
             LIMIT 1",
            params![revision_id, provider, model],
            |_| Ok(()),
        )
        .optional()
        .map(|row| row.is_some())
        .map_err(|error| error.to_string())
}

fn active_outline_job_for_route(
    connection: &Connection,
    revision_id: &str,
    route_id: &str,
) -> OutlineResult<Option<String>> {
    connection
        .query_row(
            "SELECT id FROM jobs
             WHERE revision_id = ?1 AND provider_route_id = ?2
               AND kind IN ('outline_overview', 'outline_deep_dive')
               AND state IN ('queued', 'running', 'paused')
             ORDER BY updated_at DESC LIMIT 1",
            params![revision_id, route_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}
fn active_outline_job(connection: &Connection, revision_id: &str) -> OutlineResult<Option<String>> {
    connection
        .query_row(
            "SELECT id FROM jobs
             WHERE revision_id = ?1
               AND kind IN ('outline_overview', 'outline_deep_dive')
               AND state IN ('queued', 'running', 'paused')
             ORDER BY updated_at DESC LIMIT 1",
            params![revision_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn required_plan_text<'a>(payload: &'a Value, key: &str) -> OutlineResult<&'a str> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("地图计划缺少 {key}，请重新计划。"))
}
fn check_document_snapshot(
    conn: &Connection,
    revision: &str,
    ocr: &str,
    sha: &str,
) -> OutlineResult<()> {
    let active: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM document_revisions r JOIN papers p ON p.id=r.paper_id WHERE r.id=?1 AND r.sha256=?2 AND p.deleted_at IS NULL)",params![revision,sha],|r|r.get(0)).map_err(|e|e.to_string())?;
    if !active || latest_ready_ocr(conn, revision)?.as_deref() != Some(ocr) {
        return Err("文档已删除或来源已变化，不能继续生成。".into());
    }
    Ok(())
}
fn publication_plan(conn: &Connection, publish_id: &str) -> OutlineResult<Value> {
    let raw: String = conn.query_row("SELECT payload_json FROM outline_plans WHERE json_extract(payload_json,'$.publishId')=?1",params![publish_id],|r|r.get(0)).map_err(|_|"发布缺少冻结计划，请重新计划。".to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())
}
fn plan_was_published(conn: &Connection, publish_id: &str) -> OutlineResult<bool> {
    Ok(publication_plan(conn, publish_id)?
        .get("publishedRevisionId")
        .is_some())
}
fn mark_plan_published(conn: &Connection, publish_id: &str, revision: &str) -> OutlineResult<()> {
    conn.execute("UPDATE outline_plans SET payload_json=json_set(payload_json,'$.publishedRevisionId',?2) WHERE json_extract(payload_json,'$.publishId')=?1",params![publish_id,revision]).map_err(|e|e.to_string())?;
    Ok(())
}
fn verify_publish_inputs(
    conn: &Connection,
    payload: &Value,
    revision: &str,
    ocr: &str,
    digest: &str,
) -> OutlineResult<()> {
    if required_plan_text(payload, "revisionId")? != revision
        || required_plan_text(payload, "ocrRevisionId")? != ocr
        || required_plan_text(payload, "catalogDigest")? != digest
    {
        return Err("发布输入与计划不符。".into());
    }
    if payload["invalidated"].as_bool() == Some(true) {
        return Err("地图计划已因删除失效。".into());
    }
    check_document_snapshot(conn, revision, ocr, required_plan_text(payload, "pdfHash")?)?;
    let cancelled: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM jobs WHERE json_extract(payload_json,'$.publishId')=?1 AND state IN ('cancelled','paused','failed','interrupted_unknown'))",params![payload["publishId"].as_str()],|r|r.get(0)).map_err(|e|e.to_string())?;
    if cancelled {
        return Err("地图任务已经停止，不能发布。".into());
    }
    Ok(())
}

fn coverage_field(raw: &str, key: &str) -> Value {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| v.get(key).cloned())
        .unwrap_or(Value::Null)
}
fn load_latest_attempt(
    conn: &Connection,
    revision: &str,
) -> OutlineResult<Option<OutlineHeadProjection>> {
    conn.query_row("SELECT r.id,r.kind,r.status,r.ocr_revision_id,r.catalog_digest,r.protocol_version,r.coverage_json,r.graph_json,r.units_json FROM outline_revisions r WHERE r.revision_id=?1 AND r.kind='overview' AND r.protocol_version='outline-map-v4' AND r.status='partial' AND r.created_at > COALESCE((SELECT published.created_at FROM outline_heads h JOIN outline_revisions published ON published.id=h.overview_revision_id WHERE h.revision_id=r.revision_id),'') AND NOT EXISTS(SELECT 1 FROM outline_plans p WHERE json_extract(p.payload_json,'$.publishId')=json_extract(r.dependency_snapshot_json,'$.publishId') AND json_extract(p.payload_json,'$.publishedRevisionId') IS NOT NULL) ORDER BY r.created_at DESC LIMIT 1",params![revision],|row| {
        let coverage: String=row.get(6)?;
        let graph: Option<String>=row.get(7)?;
        Ok(OutlineHeadProjection { id:row.get(0)?,kind:row.get(1)?,status:row.get(2)?,ocr_revision_id:row.get(3)?,catalog_digest:row.get(4)?,protocol_version:row.get(5)?,coverage_warnings:coverage_warnings_from_json(&coverage),graph:graph.and_then(|s|serde_json::from_str(&s).ok()),units:serde_json::from_str(&row.get::<_,String>(8)?).unwrap_or_default(),review_status:Some("unchecked".into()),review_notes:Vec::new(),gaps:Vec::new() })
    }).optional().map_err(|e|e.to_string())
}

fn load_head(
    connection: &Connection,
    revision_id: &str,
) -> OutlineResult<Option<OutlineHeadProjection>> {
    connection
        .query_row(
            "SELECT r.id, r.kind, r.status, r.ocr_revision_id, r.catalog_digest,
                    r.protocol_version, r.coverage_json, r.graph_json, r.units_json
             FROM outline_heads h
             JOIN outline_revisions r ON r.id = h.overview_revision_id
             WHERE h.revision_id = ?1",
            params![revision_id],
            |row| {
                let coverage_json: String = row.get(6)?;
                let graph_json: Option<String> = row.get(7)?;
                let units_json: String = row.get(8)?;
                Ok(OutlineHeadProjection {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    status: row.get(2)?,
                    ocr_revision_id: row.get(3)?,
                    catalog_digest: row.get(4)?,
                    protocol_version: row.get(5)?,
                    review_status: coverage_field(&coverage_json, "reviewStatus")
                        .as_str()
                        .map(str::to_string),
                    review_notes: serde_json::from_value(coverage_field(
                        &coverage_json,
                        "reviewNotes",
                    ))
                    .unwrap_or_default(),
                    gaps: serde_json::from_value(coverage_field(&coverage_json, "gaps"))
                        .unwrap_or_default(),
                    coverage_warnings: coverage_warnings_from_json(&coverage_json),
                    graph: graph_json.and_then(|text| serde_json::from_str(&text).ok()),
                    units: serde_json::from_str(&units_json).unwrap_or(Value::Array(vec![])),
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn coverage_warnings_from_json(raw: &str) -> Vec<String> {
    let parsed: Value = serde_json::from_str(raw).unwrap_or(Value::Null);
    parsed
        .get("warnings")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2_workspace::WorkspaceModule;
    use tempfile::tempdir;

    fn ready_module() -> (tempfile::TempDir, OutlineModule) {
        let root = tempdir().expect("workspace");
        let projection = WorkspaceModule::new()
            .open(root.path())
            .expect("initialize workspace");
        let module = OutlineModule::open(&projection.database_path).expect("open outline");
        (root, module)
    }

    fn insert_paper(connection: &Connection, with_ocr: bool) {
        connection
            .execute_batch(
                r#"
                INSERT INTO collections(id, name, relative_path, created_at, updated_at)
                VALUES ('col-1', 'Inbox', 'Inbox', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                VALUES ('paper-1', 'col-1', 'paper.pdf', 'Inbox/paper.pdf', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                VALUES ('rev-1', 'paper-1', 'digest-1', 12, 8, 'Inbox/paper.pdf', '2026-08-17T00:00:00Z');
                "#,
            )
            .expect("paper fixture");
        if !with_ocr {
            return;
        }
        connection
            .execute_batch(
                r#"
                INSERT INTO ocr_revisions(id, revision_id, status, provider, model, created_at, published_at)
                VALUES ('ocr-1', 'rev-1', 'ready', 'mistral', 'mistral-ocr-latest', '2026-08-17T00:00:01Z', '2026-08-17T00:00:02Z');
                INSERT INTO ocr_pages(id, ocr_revision_id, page_number) VALUES ('page-1', 'ocr-1', 1);
                INSERT INTO ocr_blocks(id, ocr_page_id, block_index, block_type, text_content, content_digest, x0, y0, x1, y1)
                VALUES
                  ('header-1', 'page-1', 0, 'header', 'Running title', 'd-h', 10, 10, 900, 40),
                  ('p-1', 'page-1', 1, 'paragraph', 'The paper trains an RNN simulator.', 'd-p', 40, 80, 900, 160),
                  ('fig-1', 'page-1', 2, 'figure', 'pixel dump', 'd-f', 80, 180, 720, 520),
                  ('cap-1', 'page-1', 3, 'caption', 'Figure 1. RNN architecture.', 'd-c', 90, 530, 700, 570);
                "#,
            )
            .expect("ocr fixture");
    }

    #[test]
    fn projection_is_missing_ocr_when_no_ready_revision_exists() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, false);
        let projection = module
            .project("rev-1", Some("gemini-2.5-flash"))
            .expect("project");
        assert_eq!(projection.status, OutlineStatus::MissingOcr);
        assert!(projection.catalog.is_none());
        assert!(projection.head.is_none());
        assert!(!projection.has_paper_root);
    }

    #[test]
    fn projection_is_ready_to_plan_with_a_stable_catalog() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, true);
        let projection = module
            .project("rev-1", Some("gemini-2.5-flash"))
            .expect("project");
        assert_eq!(projection.status, OutlineStatus::ReadyToPlan);
        assert_eq!(projection.ocr_revision_id.as_deref(), Some("ocr-1"));
        let catalog = projection.catalog.expect("catalog");
        let ids = catalog
            .entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["p-1", "fig-1", "cap-1"]);
        let figure = catalog
            .entries
            .iter()
            .find(|entry| entry.id == "fig-1")
            .expect("figure");
        assert!(figure.excerpt.is_none());
        assert_eq!(figure.nearby_caption_block_id.as_deref(), Some("cap-1"));
        assert_eq!(catalog.digest.len(), 64);
    }

    #[test]
    fn paper_root_lookup_does_not_hit_a_different_provider() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, true);
        connection
            .execute(
                "INSERT INTO context_roots(
                   id, revision_id, provider, model, context_epoch,
                   provider_file_id, provider_node_id, state, created_at
                 ) VALUES (
                   'root-openai', 'rev-1', 'openai_compatible', 'gpt-4.1',
                   'epoch', 'file-openai', 'node-openai', 'active', '2026-08-18T00:00:00Z'
                 )",
                [],
            )
            .expect("openai-compatible root");
        assert!(
            has_active_paper_root(&connection, "rev-1", "openai_compatible", "gpt-4.1")
                .expect("openai lookup")
        );
        assert!(
            !has_active_paper_root(&connection, "rev-1", "gemini", "gpt-4.1")
                .expect("gemini lookup")
        );
        assert!(!module
            .has_paper_root("rev-1", "gemini", "gpt-4.1")
            .expect("module gemini lookup"));
    }

    #[test]
    fn newer_ocr_marks_published_overview_stale() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, true);
        connection
            .execute_batch(
                r#"
                INSERT INTO outline_revisions(
                  id, paper_id, revision_id, ocr_revision_id, kind, status, protocol_version,
                  catalog_digest, units_json, graph_json, coverage_json, dependency_snapshot_json, created_at
                ) VALUES (
                  'outline-1', 'paper-1', 'rev-1', 'ocr-1', 'overview', 'published', 'outline-overview-v1',
                  'abc', '[]', '{"title":"t","summary":"s","nodes":[],"edges":[]}', '{}', '{}', '2026-08-17T00:00:03Z'
                );
                INSERT INTO outline_heads(revision_id, overview_revision_id, updated_at)
                VALUES ('rev-1', 'outline-1', '2026-08-17T00:00:03Z');
                INSERT INTO ocr_revisions(id, revision_id, status, provider, model, created_at, published_at)
                VALUES ('ocr-2', 'rev-1', 'ready', 'mistral', 'mistral-ocr-latest', '2026-08-17T00:00:05Z', '2026-08-17T00:00:06Z');
                INSERT INTO ocr_pages(id, ocr_revision_id, page_number) VALUES ('page-2', 'ocr-2', 1);
                INSERT INTO ocr_blocks(id, ocr_page_id, block_index, block_type, text_content, content_digest, x0, y0, x1, y1)
                VALUES ('p-2', 'page-2', 0, 'paragraph', 'New OCR text', 'd-n', 40, 80, 900, 160);
                "#,
            )
            .expect("stale fixture");
        let projection = module
            .project("rev-1", Some("gemini-2.5-flash"))
            .expect("project");
        assert_eq!(projection.status, OutlineStatus::Stale);
        assert_eq!(projection.ocr_revision_id.as_deref(), Some("ocr-2"));
        assert_eq!(
            projection.head.as_ref().map(|head| head.id.as_str()),
            Some("outline-1")
        );
    }

    #[test]
    fn deleting_ocr_cascades_outline_rows() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, true);
        connection
            .execute_batch(
                r#"
                INSERT INTO outline_revisions(
                  id, paper_id, revision_id, ocr_revision_id, kind, status, protocol_version,
                  catalog_digest, units_json, coverage_json, dependency_snapshot_json, created_at
                ) VALUES (
                  'outline-1', 'paper-1', 'rev-1', 'ocr-1', 'overview', 'published', 'outline-overview-v1',
                  'abc', '[]', '{}', '{}', '2026-08-17T00:00:03Z'
                );
                INSERT INTO outline_heads(revision_id, overview_revision_id, updated_at)
                VALUES ('rev-1', 'outline-1', '2026-08-17T00:00:03Z');
                "#,
            )
            .expect("outline rows");
        connection
            .execute("DELETE FROM ocr_revisions WHERE id = 'ocr-1'", [])
            .expect("delete ocr");
        let remaining: i64 = connection
            .query_row("SELECT COUNT(*) FROM outline_revisions", [], |row| {
                row.get(0)
            })
            .expect("count");
        let heads: i64 = connection
            .query_row("SELECT COUNT(*) FROM outline_heads", [], |row| row.get(0))
            .expect("heads");
        assert_eq!(remaining, 0);
        assert_eq!(heads, 0);
    }

    fn published_graph() -> OutlineGraphOutcome {
        OutlineGraphOutcome::Valid {
            graph: crate::outline_protocol::OutlineGraph {
                title: "Map".to_string(),
                summary: "s".to_string(),
                nodes: Vec::new(),
                edges: Vec::new(),
            },
            warnings: Vec::new(),
        }
    }

    #[test]
    fn delete_overview_removes_head_and_deep_dives() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, true);
        connection
            .execute_batch(
                r#"
                INSERT INTO outline_revisions(
                  id, paper_id, revision_id, ocr_revision_id, kind, status, protocol_version,
                  catalog_digest, units_json, graph_json, coverage_json, dependency_snapshot_json, created_at
                ) VALUES (
                  'outline-1', 'paper-1', 'rev-1', 'ocr-1', 'overview', 'published', 'outline-overview-v1',
                  'abc', '[]', '{"title":"t","summary":"s","nodes":[],"edges":[]}', '{}', '{}', '2026-08-17T00:00:03Z'
                );
                INSERT INTO outline_heads(revision_id, overview_revision_id, updated_at)
                VALUES ('rev-1', 'outline-1', '2026-08-17T00:00:03Z');
                INSERT INTO outline_revisions(
                  id, paper_id, revision_id, ocr_revision_id, kind, parent_overview_id, parent_node_id,
                  status, protocol_version, catalog_digest, units_json, graph_json, coverage_json,
                  dependency_snapshot_json, created_at
                ) VALUES (
                  'dive-1', 'paper-1', 'rev-1', 'ocr-1', 'deep_dive', 'outline-1', 'n1',
                  'published', 'outline-deep-dive-v1', 'abc', '[]',
                  '{"title":"d","summary":"s","nodes":[],"edges":[]}', '{}', '{}', '2026-08-17T00:00:04Z'
                );
                INSERT INTO outline_deep_dive_heads(overview_revision_id, node_id, deep_dive_revision_id, updated_at)
                VALUES ('outline-1', 'n1', 'dive-1', '2026-08-17T00:00:04Z');
                "#,
            )
            .expect("fixture");
        module.delete_overview("rev-1").expect("delete");
        let remaining: i64 = connection
            .query_row("SELECT COUNT(*) FROM outline_revisions", [], |row| {
                row.get(0)
            })
            .expect("count");
        let heads: i64 = connection
            .query_row("SELECT COUNT(*) FROM outline_heads", [], |row| row.get(0))
            .expect("heads");
        let dives: i64 = connection
            .query_row("SELECT COUNT(*) FROM outline_deep_dive_heads", [], |row| {
                row.get(0)
            })
            .expect("dives");
        assert_eq!(remaining, 0);
        assert_eq!(heads, 0);
        assert_eq!(dives, 0);
    }

    #[test]
    fn publish_overview_deletes_previous_revision() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, true);
        module
            .publish_overview("rev-1", "ocr-1", "digest-a", &[], &published_graph())
            .expect("first");
        module
            .publish_overview("rev-1", "ocr-1", "digest-b", &[], &published_graph())
            .expect("second");
        let count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM outline_revisions WHERE kind = 'overview'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(count, 1);
    }

    #[test]
    fn delete_deep_dive_keeps_overview() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, true);
        connection
            .execute_batch(
                r#"
                INSERT INTO outline_revisions(
                  id, paper_id, revision_id, ocr_revision_id, kind, status, protocol_version,
                  catalog_digest, units_json, graph_json, coverage_json, dependency_snapshot_json, created_at
                ) VALUES (
                  'outline-1', 'paper-1', 'rev-1', 'ocr-1', 'overview', 'published', 'outline-overview-v1',
                  'abc', '[]', '{"title":"t","summary":"s","nodes":[],"edges":[]}', '{}', '{}', '2026-08-17T00:00:03Z'
                );
                INSERT INTO outline_heads(revision_id, overview_revision_id, updated_at)
                VALUES ('rev-1', 'outline-1', '2026-08-17T00:00:03Z');
                INSERT INTO outline_revisions(
                  id, paper_id, revision_id, ocr_revision_id, kind, parent_overview_id, parent_node_id,
                  status, protocol_version, catalog_digest, units_json, graph_json, coverage_json,
                  dependency_snapshot_json, created_at
                ) VALUES (
                  'dive-1', 'paper-1', 'rev-1', 'ocr-1', 'deep_dive', 'outline-1', 'n1',
                  'published', 'outline-deep-dive-v1', 'abc', '[]',
                  '{"title":"d","summary":"s","nodes":[],"edges":[]}', '{}', '{}', '2026-08-17T00:00:04Z'
                );
                INSERT INTO outline_deep_dive_heads(overview_revision_id, node_id, deep_dive_revision_id, updated_at)
                VALUES ('outline-1', 'n1', 'dive-1', '2026-08-17T00:00:04Z');
                "#,
            )
            .expect("fixture");
        module.delete_deep_dive("rev-1", "n1").expect("delete dive");
        let overviews: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM outline_revisions WHERE kind = 'overview'",
                [],
                |row| row.get(0),
            )
            .expect("overview");
        let dives: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM outline_revisions WHERE kind = 'deep_dive'",
                [],
                |row| row.get(0),
            )
            .expect("dives");
        assert_eq!(overviews, 1);
        assert_eq!(dives, 0);
    }
}
