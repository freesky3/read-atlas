use crate::db;
use crate::guide_batching::{plan_guide_batches, GuidePageBlocks};
use crate::guide_protocol::{
    GUIDE_JOB_KIND, GUIDE_LANGUAGE, GUIDE_PROMPT_VERSION, GUIDE_PROTOCOL_VERSION,
};
use crate::guide_validate::{GuideCatalogBlock, GuideInk};
use crate::outline_catalog::{is_chrome_block, OutlineCatalog};
use crate::outline_module::OutlineModule;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub type GuideResult<T> = Result<T, String>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GuideStatus {
    MissingOcr,
    ReadyToPlan,
    Generating,
    Partial,
    Published,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideHeadProjection {
    pub id: String,
    pub status: String,
    pub ocr_revision_id: String,
    pub protocol_version: String,
    pub prompt_version: String,
    pub model: String,
    pub language: String,
    pub reused_outline_revision_id: Option<String>,
    pub coverage: Value,
    pub warnings: Vec<String>,
    pub context: Value,
    pub inks: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cast_snapshot: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideProjection {
    pub status: GuideStatus,
    pub revision_id: String,
    pub ocr_revision_id: Option<String>,
    pub has_paper_root: bool,
    pub head: Option<GuideHeadProjection>,
    pub active_job_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred_character_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuidePlan {
    pub revision_id: String,
    pub ocr_revision_id: String,
    pub catalog_digest: String,
    pub model: String,
    pub page_count: i64,
    pub batch_count: i64,
    pub understand_calls: i64,
    #[serde(default)]
    pub root_calls: i64,
    pub annotation_calls: i64,
    pub repair_calls: i64,
    pub reused_outline: bool,
    pub has_paper_root: bool,
    pub supports_native_pdf: bool,
    pub estimated_cost: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guide_protocol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub character_ids: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct GuideModule {
    database_path: PathBuf,
}

impl GuideModule {
    pub fn open(database_path: impl AsRef<Path>) -> GuideResult<Self> {
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
    ) -> GuideResult<GuideProjection> {
        self.project_for(revision_id, "gemini", paper_model)
    }

    pub fn project_for(
        &self,
        revision_id: &str,
        provider: &str,
        paper_model: Option<&str>,
    ) -> GuideResult<GuideProjection> {
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
            return Ok(GuideProjection {
                status: GuideStatus::MissingOcr,
                revision_id: revision_id.to_string(),
                ocr_revision_id: None,
                has_paper_root,
                head: None,
                active_job_id: None,
                preferred_character_ids: self.preferred_cast(revision_id),
            });
        };

        let active_job_id = active_guide_job(&connection, revision_id)?;
        let head = load_head(&connection, revision_id)?;
        let status = if active_job_id.is_some() {
            GuideStatus::Generating
        } else if let Some(head) = &head {
            if head.ocr_revision_id != ocr_revision_id {
                GuideStatus::Stale
            } else if head.status == "partial" {
                GuideStatus::Partial
            } else {
                GuideStatus::Published
            }
        } else {
            GuideStatus::ReadyToPlan
        };

        Ok(GuideProjection {
            status,
            revision_id: revision_id.to_string(),
            ocr_revision_id: Some(ocr_revision_id),
            has_paper_root,
            head,
            active_job_id,
            preferred_character_ids: self.preferred_cast(revision_id),
        })
    }

    /// Route-aware read projection. It never resolves a credential, allowing
    /// published local heads to remain visible while a route needs attention.
    #[allow(dead_code)]
    pub(crate) fn project_for_route(
        &self,
        revision_id: &str,
        route_id: &str,
        paper_model: Option<&str>,
    ) -> GuideResult<GuideProjection> {
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
            return Ok(GuideProjection {
                status: GuideStatus::MissingOcr,
                revision_id: revision_id.to_string(),
                ocr_revision_id: None,
                has_paper_root,
                head: None,
                active_job_id: None,
                preferred_character_ids: self.preferred_cast(revision_id),
            });
        };

        let active_job_id = active_guide_job_for_route(&connection, revision_id, route_id)?;
        let head = load_head(&connection, revision_id)?;
        let status = if active_job_id.is_some() {
            GuideStatus::Generating
        } else if let Some(head) = &head {
            if head.ocr_revision_id != ocr_revision_id {
                GuideStatus::Stale
            } else if head.status == "partial" {
                GuideStatus::Partial
            } else {
                GuideStatus::Published
            }
        } else {
            GuideStatus::ReadyToPlan
        };

        Ok(GuideProjection {
            status,
            revision_id: revision_id.to_string(),
            ocr_revision_id: Some(ocr_revision_id),
            has_paper_root,
            head,
            active_job_id,
            preferred_character_ids: self.preferred_cast(revision_id),
        })
    }

    /// Safe local projection used when no exact Provider route is currently
    /// executable. It deliberately exposes neither a remote root nor work
    /// owned by another route, while keeping an already-published guide visible.
    pub(crate) fn project_without_route(&self, revision_id: &str) -> GuideResult<GuideProjection> {
        let connection = self.connect()?;
        if !revision_exists(&connection, revision_id)? {
            return Err("Document revision was not found".to_string());
        }
        let ocr_revision_id = latest_ready_ocr(&connection, revision_id)?;
        let Some(ocr_revision_id) = ocr_revision_id else {
            return Ok(GuideProjection {
                status: GuideStatus::MissingOcr,
                revision_id: revision_id.to_string(),
                ocr_revision_id: None,
                has_paper_root: false,
                head: None,
                active_job_id: None,
                preferred_character_ids: self.preferred_cast(revision_id),
            });
        };
        let head = load_head(&connection, revision_id)?;
        let status = if let Some(head) = &head {
            if head.ocr_revision_id != ocr_revision_id {
                GuideStatus::Stale
            } else if head.status == "partial" {
                GuideStatus::Partial
            } else {
                GuideStatus::Published
            }
        } else {
            GuideStatus::ReadyToPlan
        };
        Ok(GuideProjection {
            status,
            revision_id: revision_id.to_string(),
            ocr_revision_id: Some(ocr_revision_id),
            has_paper_root: false,
            head,
            active_job_id: None,
            preferred_character_ids: self.preferred_cast(revision_id),
        })
    }

    pub fn delete(&self, revision_id: &str) -> GuideResult<()> {
        let connection = self.connect()?;
        if !revision_exists(&connection, revision_id)? {
            return Err("Document revision was not found".to_string());
        }
        connection
            .execute(
                "DELETE FROM reading_guide_revisions WHERE revision_id = ?1",
                params![revision_id],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn paper_id_for_revision(&self, revision_id: &str) -> GuideResult<String> {
        self.connect()?
            .query_row(
                "SELECT paper_id FROM document_revisions WHERE id = ?1",
                params![revision_id],
                |row| row.get(0),
            )
            .map_err(|_| "Document revision was not found".to_string())
    }

    pub fn plan(
        &self,
        revision_id: &str,
        provider: &str,
        model: &str,
        supports_native_pdf: bool,
    ) -> GuideResult<GuidePlan> {
        if !supports_native_pdf {
            return Err("The current paper model does not support native PDF".to_string());
        }
        let outline = OutlineModule::open(&self.database_path)?;
        let (ocr_revision_id, catalog) = outline.catalog_for_revision(revision_id)?;
        let page_count = outline.revision_page_count(revision_id)?;
        let has_paper_root = outline.has_paper_root(revision_id, provider, model)?;
        let reused_outline = self
            .compatible_outline_context(revision_id, &ocr_revision_id, &catalog)?
            .is_some();
        let pages = locatable_pages(&catalog);
        let section_starts = self
            .compatible_outline_context(revision_id, &ocr_revision_id, &catalog)?
            .map(|context| section_starts_from_context(&context))
            .unwrap_or_default();
        let batches = plan_guide_batches(&pages, &section_starts);
        if batches.is_empty() {
            return Err("Reading guide needs locatable OCR blocks".to_string());
        }
        let batch_count = batches.len() as i64;
        let plan = GuidePlan {
            revision_id: revision_id.to_string(),
            ocr_revision_id: ocr_revision_id.clone(),
            catalog_digest: catalog.digest.clone(),
            model: model.to_string(),
            page_count,
            batch_count,
            understand_calls: if reused_outline { 0 } else { 1 },
            root_calls: 0,
            annotation_calls: batch_count,
            repair_calls: batch_count,
            reused_outline,
            has_paper_root,
            supports_native_pdf,
            estimated_cost: None,
            plan_id: None,
            plan_digest: None,
            guide_protocol: None,
            character_ids: None,
        };
        persist_plan(&self.connect()?, &plan)?;
        Ok(plan)
    }

    #[allow(dead_code)]
    pub(crate) fn plan_for_route(
        &self,
        revision_id: &str,
        route_id: &str,
        model: &str,
        supports_native_pdf: bool,
    ) -> GuideResult<GuidePlan> {
        if route_id.trim().is_empty() {
            return Err("Provider route is required".to_string());
        }
        if !supports_native_pdf {
            return Err("The current paper model does not support native PDF".to_string());
        }
        let outline = OutlineModule::open(&self.database_path)?;
        let (ocr_revision_id, catalog) = outline.catalog_for_revision(revision_id)?;
        let page_count = outline.revision_page_count(revision_id)?;
        let has_paper_root = outline.has_paper_root_for_route(revision_id, route_id, model)?;
        let reused_outline = self
            .compatible_outline_context(revision_id, &ocr_revision_id, &catalog)?
            .is_some();
        let pages = locatable_pages(&catalog);
        let section_starts = self
            .compatible_outline_context(revision_id, &ocr_revision_id, &catalog)?
            .map(|context| section_starts_from_context(&context))
            .unwrap_or_default();
        let batches = plan_guide_batches(&pages, &section_starts);
        if batches.is_empty() {
            return Err("Reading guide needs locatable OCR blocks".to_string());
        }
        let batch_count = batches.len() as i64;
        let plan = GuidePlan {
            revision_id: revision_id.to_string(),
            ocr_revision_id: ocr_revision_id.clone(),
            catalog_digest: catalog.digest.clone(),
            model: model.to_string(),
            page_count,
            batch_count,
            understand_calls: if reused_outline { 0 } else { 1 },
            root_calls: 0,
            annotation_calls: batch_count,
            repair_calls: batch_count,
            reused_outline,
            has_paper_root,
            supports_native_pdf,
            estimated_cost: None,
            plan_id: None,
            plan_digest: None,
            guide_protocol: None,
            character_ids: None,
        };
        persist_plan(&self.connect()?, &plan)?;
        Ok(plan)
    }

    pub fn save_frozen_plan(
        &self,
        plan: &mut GuidePlan,
        payload: &mut Value,
        route_id: &str,
    ) -> GuideResult<()> {
        let digest = crate::guide_memo::digest_text(
            &json!({"inputs":payload,"routeId":route_id,"model":plan.model}).to_string(),
        );
        let id = Uuid::new_v4().to_string();
        plan.plan_id = Some(id.clone());
        plan.plan_digest = Some(digest.clone());
        payload["planDigest"] = json!(digest);
        payload["planId"] = json!(id);
        payload["publishId"] = json!(id);
        self.connect()?.execute(
            "INSERT INTO reading_guide_plans(id,revision_id,ocr_revision_id,catalog_digest,model,payload_json,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![id,plan.revision_id,plan.ocr_revision_id,plan.catalog_digest,plan.model,payload.to_string(),chrono::Utc::now().to_rfc3339()]
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn load_frozen_plan(
        &self,
        revision_id: &str,
        id: &str,
        digest: &str,
        candidate: &Value,
        route_id: &str,
        model: &str,
    ) -> GuideResult<Value> {
        let raw: String = self
            .connect()?
            .query_row(
                "SELECT payload_json FROM reading_guide_plans WHERE id=?1 AND revision_id=?2",
                params![id, revision_id],
                |r| r.get(0),
            )
            .map_err(|_| "旁批计划不存在，请重新计划")?;
        let payload: Value = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        let actual = crate::guide_memo::digest_text(
            &json!({"inputs":candidate,"routeId":route_id,"model":model}).to_string(),
        );
        if digest != actual || payload["planDigest"].as_str() != Some(digest) {
            return Err("角色、提示词、OCR、阅读偏好或模型已变化，请重新确认旁批计划".into());
        }
        Ok(payload)
    }

    pub fn locatable_pages_for(
        &self,
        revision_id: &str,
    ) -> GuideResult<(String, OutlineCatalog, Vec<GuidePageBlocks>, Option<Value>)> {
        let outline = OutlineModule::open(&self.database_path)?;
        let (ocr_revision_id, catalog) = outline.catalog_for_revision(revision_id)?;
        let context = self.compatible_outline_context(revision_id, &ocr_revision_id, &catalog)?;
        let pages = locatable_pages(&catalog);
        Ok((ocr_revision_id, catalog, pages, context))
    }

    pub fn catalog_blocks(
        &self,
        ocr_revision_id: &str,
        catalog: &OutlineCatalog,
    ) -> GuideResult<Vec<GuideCatalogBlock>> {
        let texts = self.block_texts(ocr_revision_id)?;
        Ok(catalog
            .entries
            .iter()
            .filter(|entry| !is_chrome_block(&entry.block_type))
            .map(|entry| {
                let excerpt = texts
                    .get(&entry.id)
                    .map(|text| {
                        crate::outline_catalog::truncate_excerpt(
                            text,
                            crate::guide_protocol::GUIDE_EXCERPT_CHARS,
                        )
                    })
                    .filter(|text| !text.is_empty())
                    .or_else(|| entry.excerpt.clone());
                GuideCatalogBlock {
                    id: entry.id.clone(),
                    page_number: entry.page,
                    block_index: entry.block_index,
                    block_type: entry.block_type.clone(),
                    bbox: entry.bbox,
                    excerpt,
                }
            })
            .collect())
    }

    pub fn catalog_blocks_full(
        &self,
        ocr_revision_id: &str,
        catalog: &OutlineCatalog,
    ) -> GuideResult<Vec<GuideCatalogBlock>> {
        let texts = self.block_texts(ocr_revision_id)?;
        Ok(catalog
            .entries
            .iter()
            .filter(|entry| !is_chrome_block(&entry.block_type))
            .map(|entry| {
                let excerpt = texts
                    .get(&entry.id)
                    .cloned()
                    .filter(|text| !text.trim().is_empty())
                    .or_else(|| entry.excerpt.clone());
                GuideCatalogBlock {
                    id: entry.id.clone(),
                    page_number: entry.page,
                    block_index: entry.block_index,
                    block_type: entry.block_type.clone(),
                    bbox: entry.bbox,
                    excerpt,
                }
            })
            .collect())
    }

    fn block_texts(
        &self,
        ocr_revision_id: &str,
    ) -> GuideResult<std::collections::HashMap<String, String>> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT b.id, b.text_content
                 FROM ocr_blocks b
                 JOIN ocr_pages p ON p.id = b.ocr_page_id
                 WHERE p.ocr_revision_id = ?1",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![ocr_revision_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| error.to_string())?;
        let mut texts = std::collections::HashMap::new();
        for row in rows {
            let (id, text) = row.map_err(|error| error.to_string())?;
            if !text.trim().is_empty() {
                texts.insert(id, text);
            }
        }
        Ok(texts)
    }

    pub fn compatible_outline_context(
        &self,
        revision_id: &str,
        ocr_revision_id: &str,
        catalog: &OutlineCatalog,
    ) -> GuideResult<Option<Value>> {
        let connection = self.connect()?;
        let row: Option<(String, String, String)> = connection
            .query_row(
                "SELECT r.id, r.status, r.units_json
                 FROM outline_heads h
                 JOIN outline_revisions r ON r.id = h.overview_revision_id
                 WHERE h.revision_id = ?1 AND r.ocr_revision_id = ?2",
                params![revision_id, ocr_revision_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let Some((outline_id, status, units_json)) = row else {
            return Ok(None);
        };
        if status != "published" && status != "partial" {
            return Ok(None);
        }
        let units: Value = serde_json::from_str(&units_json).unwrap_or(Value::Array(Vec::new()));
        if units
            .as_array()
            .map(|items| items.is_empty())
            .unwrap_or(true)
        {
            return Ok(None);
        }
        Ok(Some(reading_context_from_units(
            &units,
            catalog,
            Some(outline_id),
        )))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn publish(
        &self,
        revision_id: &str,
        ocr_revision_id: &str,
        model: &str,
        context: &Value,
        inks: &[GuideInk],
        reused_outline_revision_id: Option<&str>,
        coverage: &Value,
        warnings: &[String],
        status: &str,
    ) -> GuideResult<GuideHeadProjection> {
        let paper_id = self.paper_id_for_revision(revision_id)?;
        let previous_id = load_head(&self.connect()?, revision_id)?.map(|head| head.id);
        let id = Uuid::new_v4().to_string();
        let created_at = chrono::Utc::now().to_rfc3339();
        let inks_json =
            serde_json::to_string(&inks.iter().map(GuideInk::to_value).collect::<Vec<_>>())
                .map_err(|error| error.to_string())?;
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO reading_guide_revisions(
                   id, paper_id, revision_id, ocr_revision_id, status, protocol_version,
                   prompt_version, model, language, reused_outline_revision_id, coverage_json,
                   warnings_json, context_json, inks_json, dependency_snapshot_json, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                params![
                    id,
                    paper_id,
                    revision_id,
                    ocr_revision_id,
                    status,
                    GUIDE_PROTOCOL_VERSION,
                    GUIDE_PROMPT_VERSION,
                    model,
                    GUIDE_LANGUAGE,
                    reused_outline_revision_id,
                    coverage.to_string(),
                    serde_json::to_string(warnings).unwrap_or_else(|_| "[]".to_string()),
                    context.to_string(),
                    inks_json,
                    json!({"ocrRevisionId": ocr_revision_id}).to_string(),
                    created_at
                ],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "INSERT INTO reading_guide_heads(revision_id, guide_revision_id, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(revision_id) DO UPDATE SET
                   guide_revision_id = excluded.guide_revision_id,
                   updated_at = excluded.updated_at",
                params![revision_id, id, created_at],
            )
            .map_err(|error| error.to_string())?;
        if let Some(previous_id) = previous_id {
            if previous_id != id {
                let _ = connection.execute(
                    "DELETE FROM reading_guide_revisions WHERE id = ?1",
                    params![previous_id],
                );
            }
        }
        load_head(&connection, revision_id)?
            .ok_or_else(|| "Reading guide head was not published".to_string())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn publish_v2(
        &self,
        revision_id: &str,
        ocr_revision_id: &str,
        model: &str,
        language: &str,
        context: &Value,
        inks: &[GuideInk],
        coverage: &Value,
        warnings: &[String],
        status: &str,
        cast_snapshot: &Value,
        publish_id: Option<&str>,
    ) -> GuideResult<GuideHeadProjection> {
        let paper_id = self.paper_id_for_revision(revision_id)?;
        let mut connection = self.connect()?;
        let tx = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let previous_id = load_head(&tx, revision_id)?.map(|head| head.id);
        let id = publish_id
            .map(str::to_string)
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let already_published:bool=tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM reading_guide_revisions WHERE id=?1) OR EXISTS(SELECT 1 FROM reading_guide_plans WHERE id=?1 AND json_extract(payload_json,'$.publishedGuideId')=?1)",params![id],|r|r.get(0)).map_err(|e|e.to_string())?;
        if already_published {
            tx.commit().map_err(|e| e.to_string())?;
            return load_head(&self.connect()?, revision_id)?
                .ok_or_else(|| "此前已发布的旁批已被用户删除".to_string());
        }
        let created_at = chrono::Utc::now().to_rfc3339();
        let inks_json =
            serde_json::to_string(&inks.iter().map(GuideInk::to_value).collect::<Vec<_>>())
                .map_err(|error| error.to_string())?;
        let dependency = json!({
            "ocrRevisionId": ocr_revision_id,
            "castSnapshot": cast_snapshot
        });
        tx.execute(
            "INSERT INTO reading_guide_revisions(
               id, paper_id, revision_id, ocr_revision_id, status, protocol_version,
               prompt_version, model, language, reused_outline_revision_id, coverage_json,
               warnings_json, context_json, inks_json, dependency_snapshot_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                id,
                paper_id,
                revision_id,
                ocr_revision_id,
                status,
                crate::guide_protocol::GUIDE_PROTOCOL_V2,
                crate::guide_protocol::GUIDE_PROMPT_V2,
                model,
                language,
                Option::<String>::None,
                coverage.to_string(),
                serde_json::to_string(warnings).unwrap_or_else(|_| "[]".to_string()),
                context.to_string(),
                inks_json,
                dependency.to_string(),
                created_at
            ],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            "INSERT INTO reading_guide_heads(revision_id, guide_revision_id, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(revision_id) DO UPDATE SET
               guide_revision_id = excluded.guide_revision_id,
               updated_at = excluded.updated_at",
            params![revision_id, id, created_at],
        )
        .map_err(|error| error.to_string())?;
        if let Some(previous_id) = previous_id {
            if previous_id != id {
                let _ = tx.execute(
                    "DELETE FROM reading_guide_revisions WHERE id = ?1",
                    params![previous_id],
                );
            }
        }
        tx.execute("UPDATE reading_guide_plans SET payload_json=json_set(payload_json,'$.publishedGuideId',?1) WHERE id=?1",params![id]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|error| error.to_string())?;
        load_head(&self.connect()?, revision_id)?
            .ok_or_else(|| "Reading guide head was not published".to_string())
    }

    pub fn save_document_cast(
        &self,
        document_id: &str,
        character_ids: &[String],
    ) -> GuideResult<()> {
        let connection = self.connect()?;
        ensure_extension_tables(&connection)?;
        connection
            .execute(
                "INSERT INTO reading_guide_preferences(document_id, character_ids_json, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(document_id) DO UPDATE SET
                   character_ids_json = excluded.character_ids_json,
                   updated_at = excluded.updated_at",
                params![
                    document_id,
                    serde_json::to_string(character_ids).unwrap_or_else(|_| "[]".to_string()),
                    chrono::Utc::now().to_rfc3339()
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn document_cast(&self, document_id: &str) -> GuideResult<Option<Vec<String>>> {
        let connection = self.connect()?;
        ensure_extension_tables(&connection)?;
        let json: Option<String> = connection
            .query_row(
                "SELECT character_ids_json FROM reading_guide_preferences WHERE document_id = ?1",
                params![document_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        Ok(json.and_then(|value| serde_json::from_str(&value).ok()))
    }

    fn preferred_cast(&self, revision_id: &str) -> Option<Vec<String>> {
        let paper_id = self.paper_id_for_revision(revision_id).ok()?;
        self.document_cast(&paper_id).ok().flatten()
    }

    pub fn load_memo(&self, digest: &str) -> GuideResult<Option<Value>> {
        let connection = self.connect()?;
        ensure_extension_tables(&connection)?;
        let json: Option<String> = connection
            .query_row(
                "SELECT memo_json FROM reading_guide_memos WHERE cache_digest = ?1",
                params![digest],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        Ok(json.and_then(|value| serde_json::from_str(&value).ok()))
    }

    pub fn save_memo(
        &self,
        digest: &str,
        revision_id: &str,
        ocr_revision_id: &str,
        document_kind: &str,
        memo: &Value,
        raw_response: Option<&str>,
    ) -> GuideResult<()> {
        let connection = self.connect()?;
        ensure_extension_tables(&connection)?;
        connection
            .execute(
                "INSERT INTO reading_guide_memos(
                   cache_digest, revision_id, ocr_revision_id, document_kind, memo_json, raw_response, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(cache_digest) DO UPDATE SET
                   memo_json = excluded.memo_json,
                   raw_response = excluded.raw_response",
                params![
                    digest,
                    revision_id,
                    ocr_revision_id,
                    document_kind,
                    memo.to_string(),
                    raw_response,
                    chrono::Utc::now().to_rfc3339()
                ],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn connect(&self) -> GuideResult<Connection> {
        db::open(&self.database_path).map_err(|error| error.to_string())
    }
}

fn locatable_pages(catalog: &OutlineCatalog) -> Vec<GuidePageBlocks> {
    let mut pages: Vec<GuidePageBlocks> = Vec::new();
    for entry in &catalog.entries {
        if is_chrome_block(&entry.block_type) {
            continue;
        }
        if let Some(existing) = pages.iter_mut().find(|page| page.page == entry.page) {
            existing.block_ids.push(entry.id.clone());
        } else {
            pages.push(GuidePageBlocks {
                page: entry.page,
                block_ids: vec![entry.id.clone()],
            });
        }
    }
    pages
}

fn section_starts_from_context(context: &Value) -> Vec<i64> {
    context
        .get("sections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|section| section.get("pageStart").and_then(Value::as_i64))
        .collect()
}

fn reading_context_from_units(
    units: &Value,
    catalog: &OutlineCatalog,
    outline_id: Option<String>,
) -> Value {
    let mut page_by_id = std::collections::HashMap::new();
    for entry in &catalog.entries {
        page_by_id.insert(entry.id.clone(), entry.page);
    }
    let list = units.as_array().cloned().unwrap_or_default();
    let sections: Vec<Value> = list
        .iter()
        .filter_map(|unit| {
            let heading = unit.get("title")?.as_str()?.to_string();
            let summary = unit.get("takeaway")?.as_str()?.to_string();
            let evidence = unit
                .get("evidenceIds")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            let pages: Vec<i64> = evidence
                .iter()
                .filter_map(Value::as_str)
                .filter_map(|id| page_by_id.get(id).copied())
                .collect();
            let page_start = pages.iter().copied().min().unwrap_or(1);
            let page_end = pages.iter().copied().max().unwrap_or(page_start);
            Some(json!({
                "heading": heading,
                "summary": summary,
                "pageStart": page_start,
                "pageEnd": page_end,
                "evidenceIds": evidence
            }))
        })
        .collect();
    json!({
        "thesis": list.first().and_then(|unit| unit.get("takeaway")).cloned().unwrap_or(json!("")),
        "sections": sections,
        "argumentFlow": [],
        "confusingPoints": [],
        "reusedOutlineRevisionId": outline_id
    })
}

fn load_head(
    connection: &Connection,
    revision_id: &str,
) -> GuideResult<Option<GuideHeadProjection>> {
    connection
        .query_row(
            "SELECT r.id, r.status, r.ocr_revision_id, r.protocol_version, r.prompt_version,
                    r.model, r.language, r.reused_outline_revision_id, r.coverage_json,
                    r.warnings_json, r.context_json, r.inks_json, r.dependency_snapshot_json
             FROM reading_guide_heads h
             JOIN reading_guide_revisions r ON r.id = h.guide_revision_id
             WHERE h.revision_id = ?1",
            params![revision_id],
            |row| {
                let coverage: String = row.get(8)?;
                let warnings: String = row.get(9)?;
                let context: String = row.get(10)?;
                let inks: String = row.get(11)?;
                let dependency: String = row.get(12)?;
                let dependency_value: Value = serde_json::from_str(&dependency)
                    .unwrap_or_else(|_| Value::Object(Default::default()));
                let cast_snapshot = dependency_value.get("castSnapshot").cloned();
                Ok(GuideHeadProjection {
                    id: row.get(0)?,
                    status: row.get(1)?,
                    ocr_revision_id: row.get(2)?,
                    protocol_version: row.get(3)?,
                    prompt_version: row.get(4)?,
                    model: row.get(5)?,
                    language: row.get(6)?,
                    reused_outline_revision_id: row.get(7)?,
                    coverage: serde_json::from_str(&coverage)
                        .unwrap_or_else(|_| Value::Object(Default::default())),
                    warnings: serde_json::from_str(&warnings).unwrap_or_default(),
                    context: serde_json::from_str(&context)
                        .unwrap_or_else(|_| Value::Object(Default::default())),
                    inks: serde_json::from_str(&inks).unwrap_or_else(|_| Value::Array(Vec::new())),
                    cast_snapshot,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn revision_exists(connection: &Connection, revision_id: &str) -> GuideResult<bool> {
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

fn latest_ready_ocr(connection: &Connection, revision_id: &str) -> GuideResult<Option<String>> {
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

#[allow(dead_code)]
fn has_active_paper_root_for_route(
    connection: &Connection,
    revision_id: &str,
    route_id: &str,
    model: &str,
) -> GuideResult<bool> {
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
) -> GuideResult<bool> {
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

fn persist_plan(connection: &Connection, plan: &GuidePlan) -> GuideResult<()> {
    let payload = serde_json::to_string(plan).map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT INTO reading_guide_plans(
               id, revision_id, ocr_revision_id, catalog_digest, model, payload_json, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Uuid::new_v4().to_string(),
                plan.revision_id,
                plan.ocr_revision_id,
                plan.catalog_digest,
                plan.model,
                payload,
                chrono::Utc::now().to_rfc3339()
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn active_guide_job_for_route(
    connection: &Connection,
    revision_id: &str,
    route_id: &str,
) -> GuideResult<Option<String>> {
    connection
        .query_row(
            "SELECT id FROM jobs
             WHERE revision_id = ?1 AND provider_route_id = ?2
               AND kind = ?3
               AND state IN ('queued', 'running', 'paused')
             ORDER BY updated_at DESC LIMIT 1",
            params![revision_id, route_id, GUIDE_JOB_KIND],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

fn active_guide_job(connection: &Connection, revision_id: &str) -> GuideResult<Option<String>> {
    connection
        .query_row(
            "SELECT id FROM jobs
             WHERE revision_id = ?1
               AND kind = ?2
               AND state IN ('queued', 'running', 'paused')
             ORDER BY updated_at DESC LIMIT 1",
            params![revision_id, GUIDE_JOB_KIND],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())
}

pub fn ensure_extension_tables(connection: &Connection) -> GuideResult<()> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS reading_guide_preferences (
              document_id TEXT PRIMARY KEY,
              character_ids_json TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS reading_guide_memos (
              cache_digest TEXT PRIMARY KEY,
              revision_id TEXT NOT NULL,
              ocr_revision_id TEXT NOT NULL,
              document_kind TEXT NOT NULL,
              memo_json TEXT NOT NULL,
              raw_response TEXT,
              created_at TEXT NOT NULL
            );
            "#,
        )
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guide_protocol::{GUIDE_PROMPT_VERSION, GUIDE_PROTOCOL_VERSION};
    use crate::v2_workspace::WorkspaceModule;
    use rusqlite::params;
    use tempfile::tempdir;

    fn ready_module() -> (tempfile::TempDir, GuideModule) {
        let root = tempdir().expect("workspace");
        let projection = WorkspaceModule::new()
            .open(root.path())
            .expect("initialize workspace");
        let module = GuideModule::open(&projection.database_path).expect("open guide");
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
                  ('p-1', 'page-1', 1, 'paragraph', 'The paper trains an RNN simulator.', 'd-p', 40, 80, 900, 160);
                "#,
            )
            .expect("ocr fixture");
    }

    fn insert_head(connection: &Connection, ocr_revision_id: &str, status: &str) {
        connection
            .execute(
                "INSERT INTO reading_guide_revisions(
                   id, paper_id, revision_id, ocr_revision_id, status, protocol_version,
                   prompt_version, model, language, coverage_json, warnings_json,
                   context_json, inks_json, dependency_snapshot_json, created_at
                 ) VALUES (
                   'guide-1', 'paper-1', 'rev-1', ?1, ?2, ?3, ?4, 'gemini-2.5-flash', 'en',
                   '{}', '[]', '{}', '[]', '{}', '2026-08-20T00:00:00Z'
                 )",
                params![
                    ocr_revision_id,
                    status,
                    GUIDE_PROTOCOL_VERSION,
                    GUIDE_PROMPT_VERSION
                ],
            )
            .expect("guide revision");
        connection
            .execute(
                "INSERT INTO reading_guide_heads(revision_id, guide_revision_id, updated_at)
                 VALUES ('rev-1', 'guide-1', '2026-08-20T00:00:00Z')",
                [],
            )
            .expect("guide head");
    }

    #[test]
    fn committed_v2_publish_cannot_replace_a_newer_head_on_resume() {
        let (_root, module) = ready_module();
        insert_paper(&module.connect().unwrap(), true);
        let mut plan = module
            .plan_for_route("rev-1", "route", "model", true)
            .unwrap();
        let mut payload = json!({});
        module
            .save_frozen_plan(&mut plan, &mut payload, "route")
            .unwrap();
        let id = plan.plan_id.as_deref().unwrap();
        let first = module
            .publish_v2(
                "rev-1",
                "ocr-1",
                "model",
                "en",
                &json!({}),
                &[],
                &json!({}),
                &[],
                "published",
                &json!({}),
                Some(id),
            )
            .unwrap();
        let newer = module
            .publish_v2(
                "rev-1",
                "ocr-1",
                "model",
                "en",
                &json!({}),
                &[],
                &json!({}),
                &[],
                "published",
                &json!({}),
                Some("newer"),
            )
            .unwrap();
        assert_eq!(first.language, "en");
        assert_eq!(newer.language, "en");
        assert_ne!(first.id, newer.id);
        let resumed = module
            .publish_v2(
                "rev-1",
                "ocr-1",
                "model",
                "en",
                &json!({}),
                &[],
                &json!({}),
                &[],
                "published",
                &json!({}),
                Some(id),
            )
            .unwrap();
        assert_eq!(resumed.id, newer.id);
    }

    #[test]
    fn frozen_plan_rejects_changed_cast_prompt_reader_ocr_and_route() {
        let (_root, module) = ready_module();
        insert_paper(&module.connect().unwrap(), true);
        let mut plan = module
            .plan_for_route("rev-1", "route-one", "model", true)
            .unwrap();
        let candidate = json!({"castSnapshot":{"order":["preset:chitanda"]},"prompts":{"context":"memo"},"readerContext":"reader","ocrRevisionId":"ocr-1"});
        let mut frozen = candidate.clone();
        module
            .save_frozen_plan(&mut plan, &mut frozen, "route-one")
            .unwrap();
        let id = plan.plan_id.as_deref().unwrap();
        let digest = plan.plan_digest.as_deref().unwrap();
        assert_eq!(
            module
                .load_frozen_plan("rev-1", id, digest, &candidate, "route-one", "model")
                .unwrap(),
            frozen
        );
        for key in ["castSnapshot", "prompts", "readerContext", "ocrRevisionId"] {
            let mut changed = candidate.clone();
            changed[key] = json!("changed");
            assert!(module
                .load_frozen_plan("rev-1", id, digest, &changed, "route-one", "model")
                .is_err());
        }
        assert!(module
            .load_frozen_plan("rev-1", id, digest, &candidate, "route-two", "model")
            .is_err());
        assert!(module
            .load_frozen_plan("rev-1", id, digest, &candidate, "route-one", "other-model")
            .is_err());
    }

    #[test]
    fn project_reports_missing_ocr_before_planning() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, false);
        let projection = module.project("rev-1", None).expect("project");
        assert_eq!(projection.status, GuideStatus::MissingOcr);
        assert!(projection.head.is_none());
        assert!(projection.ocr_revision_id.is_none());
    }

    #[test]
    fn project_reports_ready_to_plan_when_ocr_exists() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, true);
        let projection = module.project("rev-1", None).expect("project");
        assert_eq!(projection.status, GuideStatus::ReadyToPlan);
        assert_eq!(projection.ocr_revision_id.as_deref(), Some("ocr-1"));
        assert!(projection.head.is_none());
    }

    #[test]
    fn project_marks_head_stale_when_ocr_changes() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, true);
        insert_head(&connection, "ocr-1", "published");
        connection
            .execute_batch(
                r#"
                INSERT INTO ocr_revisions(id, revision_id, status, provider, model, created_at, published_at)
                VALUES ('ocr-2', 'rev-1', 'ready', 'mistral', 'mistral-ocr-latest', '2026-08-20T01:00:00Z', '2026-08-20T01:00:01Z');
                "#,
            )
            .expect("newer ocr");
        let projection = module.project("rev-1", None).expect("project");
        assert_eq!(projection.status, GuideStatus::Stale);
        assert_eq!(
            projection.head.as_ref().map(|head| head.id.as_str()),
            Some("guide-1")
        );
        assert_eq!(projection.ocr_revision_id.as_deref(), Some("ocr-2"));
    }

    #[test]
    fn delete_removes_head_and_returns_ready_to_plan() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, true);
        insert_head(&connection, "ocr-1", "partial");
        module.delete("rev-1").expect("delete");
        let projection = module.project("rev-1", None).expect("project");
        assert_eq!(projection.status, GuideStatus::ReadyToPlan);
        assert!(projection.head.is_none());
    }

    #[test]
    fn unknown_revision_is_rejected() {
        let (_root, module) = ready_module();
        assert!(module.project("missing", None).is_err());
        assert!(module.delete("missing").is_err());
    }

    #[test]
    fn plan_rejects_models_without_native_pdf() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, true);
        let error = module
            .plan("rev-1", "gemini", "gemini-2.5-flash", false)
            .expect_err("native pdf");
        assert!(error.contains("native PDF"));
    }

    #[test]
    fn catalog_blocks_include_ocr_excerpt() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, true);
        let outline = OutlineModule::open(&module.database_path).expect("outline");
        let (ocr, catalog) = outline.catalog_for_revision("rev-1").expect("catalog");
        let blocks = module.catalog_blocks(&ocr, &catalog).expect("blocks");
        assert!(blocks.iter().any(|block| {
            block
                .excerpt
                .as_deref()
                .is_some_and(|text| text.contains("RNN"))
        }));
    }

    #[test]
    fn plan_counts_locatable_pages_and_persists() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, true);
        let plan = module
            .plan("rev-1", "gemini", "gemini-2.5-flash", true)
            .expect("plan");
        assert_eq!(plan.ocr_revision_id, "ocr-1");
        assert_eq!(plan.batch_count, 1);
        assert_eq!(plan.understand_calls, 1);
        assert!(!plan.reused_outline);
        let stored: i64 = module
            .connect()
            .expect("connect")
            .query_row(
                "SELECT COUNT(*) FROM reading_guide_plans WHERE revision_id = 'rev-1'",
                [],
                |row| row.get(0),
            )
            .expect("count plans");
        assert_eq!(stored, 1);
    }

    #[test]
    fn publish_replaces_the_previous_head() {
        let (_root, module) = ready_module();
        let connection = module.connect().expect("connect");
        insert_paper(&connection, true);
        insert_head(&connection, "ocr-1", "published");
        let published = module
            .publish(
                "rev-1",
                "ocr-1",
                "gemini-2.5-flash",
                &json!({"thesis": "t", "sections": []}),
                &[],
                None,
                &json!({"complete": true}),
                &[],
                "published",
            )
            .expect("publish");
        assert_ne!(published.id, "guide-1");
        let projection = module.project("rev-1", None).expect("project");
        assert_eq!(projection.status, GuideStatus::Published);
        assert_eq!(
            projection.head.as_ref().map(|head| head.id.as_str()),
            Some(published.id.as_str())
        );
    }
}
