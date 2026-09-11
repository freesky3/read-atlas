use crate::db;
use crate::library_paths::{
    canonicalize_collection_argument, document_kind_from_relative, ensure_inside_library,
    join_workspace_relative, library_relative_from_absolute, safe_library_relative,
    unmanaged_pdf_notice, PAPERS_DIR, PAPERS_ROOT_COLLECTION_ID, TEXTBOOKS_DIR,
    TEXTBOOKS_ROOT_COLLECTION_ID,
};
use crate::library_workflow::{bump_library_revisions, LibraryDomain};
use crate::reader_context;
use chrono::{Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

pub type PaperResult<T> = Result<T, String>;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionProjection {
    pub id: String,
    pub parent_id: Option<String>,
    pub name: String,
    pub relative_path: String,
    #[serde(default = "default_sort_mode")]
    pub sort_mode: String,
    #[serde(default)]
    pub paper_order: Vec<String>,
}

#[allow(dead_code)]
fn default_sort_mode() -> String {
    "recent".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperProjection {
    pub id: String,
    pub revision_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub publication_year: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_metadata: Option<serde_json::Value>,
    pub page_count: Option<i64>,
    pub collection_id: String,
    pub collection_path: String,
    pub file_name: String,
    pub relative_path: String,
    pub sha256: String,
    pub byte_size: i64,
    pub imported_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictProjection {
    pub id: String,
    pub kind: String,
    pub relative_path: Option<String>,
    pub paper_id: Option<String>,
    pub details: serde_json::Value,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryProjection {
    pub cursor: String,
    pub collections: Vec<CollectionProjection>,
    pub papers: Vec<PaperProjection>,
    pub conflicts: Vec<ConflictProjection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unmanaged_notice: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kind_change_notices: Vec<String>,
}

/// 一次导入尝试到底发生了什么。批量导入要把它逐项记进批次，所以这个结论必须
/// 由 `PaperModule` 给出：hash 与回收站的规则只有它一份，调用方复制一遍就会漂移。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportOutcome {
    /// 库里新建了 Paper，外部源还把文件复制进了库。
    CreatedNew,
    /// 同一份内容已经在库里的目标位置上：没有新 Paper，也没有新文件。
    ReusedExisting,
    /// 回收站里找到了同一份内容：走的是 restore，不是新建。
    RestoredExisting,
    /// `duplicate_hash` 或 `target_path_conflict`：已经记档，等用户在 Hub 处理。
    Conflict,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportProjection {
    pub paper: Option<PaperProjection>,
    pub conflict: Option<ConflictProjection>,
    pub copied: bool,
    pub outcome: ImportOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingState {
    pub paper_id: String,
    pub revision_id: String,
    pub page_number: i64,
    pub page_offset: f64,
    pub zoom: f64,
    pub rotation: i64,
    pub right_tab: String,
    pub active_artifact_id: Option<String>,
    pub active_discussion_id: Option<String>,
    pub discussion_draft: String,
    pub quote_basket: serde_json::Value,
    pub workspace_layout: String,
    pub active_outline_node_id: Option<String>,
    pub outline_view: String,
    pub outline_inspector_width: i64,
    #[serde(default = "default_guide_layer_visible")]
    pub guide_layer_visible: bool,
    #[serde(default)]
    pub long_pdf_warning_acked: bool,
    pub updated_at: String,
}

fn default_guide_layer_visible() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageReport {
    pub workspace_bytes: u64,
    pub papers_bytes: u64,
    pub internal_bytes: u64,
    pub paper_logical_bytes: Vec<PaperStorage>,
    pub artifact_logical_bytes: Vec<ArtifactStorage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperStorage {
    pub paper_id: String,
    pub source_bytes: u64,
    pub logical_database_bytes: u64,
    pub artifact_bytes: u64,
    pub discussion_bytes: u64,
    pub ocr_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactStorage {
    pub artifact_id: String,
    pub paper_id: String,
    pub kind: String,
    pub object_key: String,
    pub version: i64,
    pub logical_bytes: u64,
}

fn default_trash_kind() -> String {
    "paper".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrashProjection {
    pub id: String,
    pub paper_id: String,
    pub title: String,
    pub file_name: String,
    pub original_relative_path: String,
    pub source_bytes: u64,
    pub deleted_at: String,
    pub purge_after: String,
    #[serde(default = "default_trash_kind")]
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct PaperModule {
    root: PathBuf,
    papers_path: PathBuf,
    textbooks_path: PathBuf,
    database_path: PathBuf,
}

impl PaperModule {
    pub fn workspace_root(&self) -> &Path {
        &self.root
    }

    pub fn open(root: impl AsRef<Path>) -> PaperResult<Self> {
        let root = fs::canonicalize(root.as_ref())
            .map_err(|error| format!("Unable to resolve Workspace: {error}"))?;
        let papers_path = root.join(PAPERS_DIR);
        let textbooks_path = root.join(TEXTBOOKS_DIR);
        let database_path = root.join(".read-desktop").join("workspace.sqlite3");
        if !papers_path.is_dir() || !database_path.is_file() {
            return Err("Workspace V2 is not initialized".to_string());
        }
        fs::create_dir_all(&textbooks_path)
            .map_err(|error| format!("Unable to create Textbooks directory: {error}"))?;
        let module = Self {
            root,
            papers_path,
            textbooks_path,
            database_path,
        };
        let connection = module.connect()?;
        crate::v2_workspace::ensure_hub_sort_tables(&connection)?;
        crate::annotation_module::ensure_tables(&connection)?;
        module.ensure_root_collections(&connection)?;
        module.cleanup_expired_trash(&connection)?;
        module.cleanup_orphan_staging(&connection)?;
        Ok(module)
    }

    pub fn list_library(&self) -> PaperResult<LibraryProjection> {
        let connection = self.connect()?;
        let collections = self.list_collections(&connection)?;
        let papers = self.list_papers(&connection)?;
        let conflicts = self.list_conflicts(&connection)?;
        // A count-composed cursor hides same-size replacements; the global
        // revision moves on every Hub-visible write instead.
        let cursor = crate::library_workflow::library_revision(&connection)?.to_string();
        Ok(LibraryProjection {
            cursor,
            collections,
            papers,
            conflicts,
            unmanaged_notice: unmanaged_pdf_notice(&self.root),
            kind_change_notices: Vec::new(),
        })
    }

    /// 一个在册 Paper 的当前事实，与 `list_library` 同一份投影。
    /// 逐项动作（导出的每一项）要的就是「一个 Paper」——用 `list_library()`
    /// 会把全表扫一遍，500 项就是 500 次全表。已在回收站的行读不出来。
    pub fn live_paper(&self, paper_id: &str) -> PaperResult<Option<PaperProjection>> {
        let connection = self.connect()?;
        self.query_paper(&connection, "p.id = ?1", paper_id)
    }

    pub fn import_pdf(
        &self,
        source: impl AsRef<Path>,
        collection_path: Option<&str>,
    ) -> PaperResult<ImportProjection> {
        let source = fs::canonicalize(source.as_ref())
            .map_err(|error| format!("Unable to resolve PDF: {error}"))?;
        let source_inside_library = library_relative_from_absolute(&self.root, &source).is_ok();
        let target = if source_inside_library {
            source.clone()
        } else {
            let collection = canonicalize_collection_argument(collection_path)?;
            let file_name = source
                .file_name()
                .ok_or_else(|| "PDF file name is missing".to_string())?;
            self.root.join(collection).join(file_name)
        };
        ensure_inside_library(&self.root, &target)?;
        let connection = self.connect()?;
        let operation_id = Uuid::new_v4().to_string();
        let copied = !source_inside_library;
        let (sha256, byte_size, page_count, staged_path) = if copied {
            let parent = target
                .parent()
                .ok_or_else(|| "Import target has no parent".to_string())?;
            fs::create_dir_all(parent)
                .map_err(|error| format!("Unable to create Collection directory: {error}"))?;
            let staging = target.with_extension(format!("pdf.{}.part", operation_id));
            let inspected = copy_and_inspect_pdf(&source, &staging);
            let inspected = match inspected {
                Ok(value) => value,
                Err(error) => {
                    let _ = fs::remove_file(&staging);
                    return Err(format!("Unable to stage PDF: {error}"));
                }
            };
            (inspected.0, inspected.1, inspected.2, Some(staging))
        } else {
            let inspected = inspect_pdf(&source)?;
            (inspected.0, inspected.1, inspected.2, None)
        };

        let target_relative =
            library_relative_from_absolute(&self.root, &target).unwrap_or_else(|_| {
                target
                    .strip_prefix(&self.root)
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|_| target.clone())
            });
        let target_relative_text = path_text(&target_relative);

        // 1. If an active paper in library has this hash
        if let Some(existing) = self.paper_by_hash(&connection, &sha256)? {
            let existing_path = self.absolute_paper_path(&existing.relative_path)?;
            if (!copied && existing_path == source)
                || existing.relative_path == target_relative_text
            {
                if let Some(staging) = &staged_path {
                    let _ = fs::remove_file(staging);
                }
                return Ok(ImportProjection {
                    paper: Some(existing),
                    conflict: None,
                    copied: false,
                    outcome: ImportOutcome::ReusedExisting,
                });
            }
            if let Some(staging) = &staged_path {
                let _ = fs::remove_file(staging);
            }
            let conflict = self.create_conflict(
                &connection,
                "duplicate_hash",
                library_relative_from_absolute(&self.root, &source)
                    .ok()
                    .as_deref(),
                Some(&existing.id),
                serde_json::json!({
                    "sha256": sha256,
                    "existingRelativePath": existing.relative_path,
                    "incomingPathHint": source.file_name().and_then(|name| name.to_str())
                }),
            )?;
            return Ok(ImportProjection {
                paper: None,
                conflict: Some(conflict),
                copied: false,
                outcome: ImportOutcome::Conflict,
            });
        }

        // 2. If a paper in TRASH matches this hash or relative path, restore and return it
        let trashed_id: Option<String> = connection
            .query_row(
                "SELECT p.id FROM papers p
                 JOIN document_revisions r ON r.paper_id = p.id
                 WHERE p.deleted_at IS NOT NULL AND (r.sha256 = ?1 OR p.relative_path = ?2)
                 ORDER BY p.deleted_at DESC LIMIT 1",
                params![sha256, target_relative_text],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(trashed_id) = trashed_id {
            if let Some(staging) = &staged_path {
                let _ = fs::remove_file(staging);
            }
            let restored = self.restore_paper(&trashed_id)?;
            return Ok(ImportProjection {
                paper: Some(restored),
                conflict: None,
                copied: false,
                outcome: ImportOutcome::RestoredExisting,
            });
        }

        // 3. If target file exists and is already an active paper
        if copied && target.exists() {
            let occupant =
                self.query_paper(&connection, "p.relative_path = ?1", &target_relative_text)?;
            // 只有「同一份内容已经在这个路径上」才算复用。同名的另一份 PDF 必须报
            // 冲突：把 B 报成 A，等于让用户以为第二个文件已经进了库——批量导入里
            // 两个同名源文件是常态，不能靠「目标位置有 Paper」推断身份。
            let reused = occupant.filter(|paper| paper.sha256 == sha256);
            if let Some(staging) = &staged_path {
                let _ = fs::remove_file(staging);
            }
            if let Some(paper) = reused {
                return Ok(ImportProjection {
                    paper: Some(paper),
                    conflict: None,
                    copied: false,
                    outcome: ImportOutcome::ReusedExisting,
                });
            }
            let conflict = self.create_conflict(
                &connection,
                "target_path_conflict",
                library_relative_from_absolute(&self.root, &target)
                    .ok()
                    .as_deref(),
                None,
                serde_json::json!({
                    "incomingSha256": sha256,
                    "targetRelativePath": target_relative_text
                }),
            )?;
            return Ok(ImportProjection {
                paper: None,
                conflict: Some(conflict),
                copied: false,
                outcome: ImportOutcome::Conflict,
            });
        }

        let timestamp = now();
        connection
            .execute(
                "INSERT INTO operation_journal(
                   id, operation_kind, entity_kind, entity_id, source_path, target_path,
                   state, payload_json, created_at
                 ) VALUES (?1, 'import_pdf', 'paper', ?2, ?3, ?4, 'prepared', '{}', ?5)",
                params![
                    operation_id,
                    operation_id,
                    path_text(&source),
                    path_text(&target),
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;

        if let Some(staging) = staged_path {
            if let Err(error) = fs::rename(&staging, &target) {
                let _ = fs::remove_file(&staging);
                return Err(format!("Unable to publish PDF: {error}"));
            }
        }

        let relative_path = library_relative_from_absolute(&self.root, &target)?;
        let paper = match self.register_new_paper(
            &connection,
            &relative_path,
            &sha256,
            byte_size,
            page_count,
        ) {
            Ok(paper) => paper,
            Err(error) => {
                if copied {
                    let _ = fs::remove_file(&target);
                }
                return Err(error);
            }
        };
        connection
            .execute(
                "UPDATE operation_journal
                 SET entity_id = ?1, state = 'committed', committed_at = ?2
                 WHERE id = ?3",
                params![paper.id, now(), operation_id],
            )
            .map_err(|error| error.to_string())?;
        Ok(ImportProjection {
            paper: Some(paper),
            conflict: None,
            copied,
            outcome: ImportOutcome::CreatedNew,
        })
    }

    pub fn move_paper(
        &self,
        paper_id: &str,
        collection_path: &str,
        new_file_name: &str,
        confirm_kind_change: bool,
    ) -> PaperResult<PaperProjection> {
        validate_pdf_file_name(new_file_name)?;
        let collection_path = canonicalize_collection_argument(Some(collection_path))?;
        let connection = self.connect()?;
        let paper = self.paper_by_id(&connection, paper_id)?;
        let source = self.absolute_paper_path(&paper.relative_path)?;
        let target = self.root.join(&collection_path).join(new_file_name);
        ensure_inside_library(&self.root, &target)?;
        if source == target {
            return Ok(paper);
        }
        let target_relative = library_relative_from_absolute(&self.root, &target)?;
        let kind_changed = document_kind_from_relative(&paper.relative_path)
            != document_kind_from_relative(&target_relative.to_string_lossy());
        if kind_changed && !confirm_kind_change {
            return Err("kind_change_confirmation_required".to_string());
        }
        if target.exists() {
            self.create_conflict(
                &connection,
                "target_path_conflict",
                library_relative_from_absolute(&self.root, &target)
                    .ok()
                    .as_deref(),
                Some(paper_id),
                serde_json::json!({"operation": "move"}),
            )?;
            return Err("Target PDF already exists; a conflict was recorded".to_string());
        }
        let operation_id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO operation_journal(
                   id, operation_kind, entity_kind, entity_id, source_path, target_path,
                   state, payload_json, created_at
                 ) VALUES (?1, 'move_paper', 'paper', ?2, ?3, ?4, 'prepared', '{}', ?5)",
                params![
                    operation_id,
                    paper_id,
                    paper.relative_path,
                    path_text(
                        &library_relative_from_absolute(&self.root, &target).unwrap_or_else(|_| {
                            target
                                .strip_prefix(&self.root)
                                .map(Path::to_path_buf)
                                .unwrap_or_else(|_| target.clone())
                        }),
                    ),
                    now()
                ],
            )
            .map_err(|error| error.to_string())?;
        fs::create_dir_all(target.parent().ok_or("Move target has no parent")?)
            .map_err(|error| format!("Unable to create Collection directory: {error}"))?;
        fs::rename(&source, &target).map_err(|error| format!("Unable to move PDF: {error}"))?;

        let relative = library_relative_from_absolute(&self.root, &target)?;
        let new_relative_text = path_text(&relative);
        let sidecar_result = reader_context::relocate_pdf_sidecar(
            &self.root,
            &paper.relative_path,
            &new_relative_text,
        );
        if let Err(error) = &sidecar_result {
            if error != "sidecar_target_conflict" {
                let _ = fs::rename(&target, &source);
                return Err(error.clone());
            }
        }
        let prev_collection_id = paper.collection_id.clone();
        let result = (|| -> PaperResult<()> {
            let mut connection = self.connect()?;
            let transaction = connection
                .transaction()
                .map_err(|error| error.to_string())?;
            let collection_id = self.ensure_collection(&transaction, relative.parent())?;
            transaction
                .execute(
                    "UPDATE papers
                     SET collection_id = ?1, file_name = ?2, relative_path = ?3, updated_at = ?4
                     WHERE id = ?5 AND deleted_at IS NULL",
                    params![
                        collection_id,
                        new_file_name,
                        path_text(&relative),
                        now(),
                        paper_id
                    ],
                )
                .map_err(|error| error.to_string())?;
            // Manual order is keyed on (collection_id, paper_id). A same-folder
            // rename keeps the paper on the same collection_id, so the row
            // must stay put. Only drop it when the paper actually migrates.
            if prev_collection_id != collection_id {
                Self::remove_paper_from_manual_order(&transaction, paper_id)?;
            }
            transaction
                .execute(
                    "UPDATE operation_journal
                     SET state = 'committed', committed_at = ?1 WHERE id = ?2",
                    params![now(), operation_id],
                )
                .map_err(|error| error.to_string())?;
            let mut domains = vec![LibraryDomain::Structure];
            if prev_collection_id != collection_id {
                domains.push(LibraryDomain::Sort);
            }
            bump_library_revisions(&transaction, &domains)?;
            transaction.commit().map_err(|error| error.to_string())
        })();
        if let Err(error) = result {
            let _ = fs::rename(&target, &source);
            if sidecar_result.is_ok() {
                let _ = reader_context::relocate_pdf_sidecar(
                    &self.root,
                    &new_relative_text,
                    &paper.relative_path,
                );
            }
            return Err(error);
        }
        if sidecar_result.as_ref().err().map(String::as_str) == Some("sidecar_target_conflict") {
            let _ = self.create_conflict(
                &connection,
                "sidecar_target_conflict",
                Some(Path::new(&new_relative_text)),
                Some(paper_id),
                serde_json::json!({"operation": "move_sidecar"}),
            );
        }
        if kind_changed {
            self.apply_kind_change(paper_id)?;
        }
        self.paper_by_id(&connection, paper_id)
    }

    pub fn list_collections_public(&self) -> PaperResult<Vec<CollectionProjection>> {
        let connection = self.connect()?;
        self.list_collections(&connection)
    }

    pub fn create_collection(
        &self,
        parent_path: &str,
        name: Option<&str>,
    ) -> PaperResult<CollectionProjection> {
        let parent = if parent_path.trim().is_empty() {
            return Err("不能在 Workspace 根下创建目录".to_string());
        } else {
            safe_library_relative(Path::new(&crate::library_paths::normalize_slashes(
                parent_path,
            )))?
        };
        // forbid export and internal already handled by safe_library_relative
        let mut connection = self.connect()?;
        // parent must already be a collection
        let parent_text = path_text(&parent);
        let parent_row: Option<(String, String)> = connection
            .query_row(
                "SELECT id, relative_path FROM collections WHERE relative_path = ?1",
                params![parent_text],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let (parent_id, parent_relative) = parent_row.ok_or_else(|| "父目录不存在".to_string())?;

        let raw_name = name
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
            .unwrap_or("新建文件夹");
        validate_collection_segment(raw_name)?;
        // generate unique name
        let mut candidate = raw_name.to_string();
        let mut counter = 2;
        loop {
            let candidate_path = PathBuf::from(&parent_relative).join(&candidate);
            let candidate_text = path_text(&candidate_path);
            let exists_db: bool = connection
                .query_row(
                    "SELECT 1 FROM collections WHERE relative_path = ?1",
                    params![candidate_text],
                    |_| Ok(true),
                )
                .optional()
                .map_err(|e| e.to_string())?
                .unwrap_or(false);
            let disk_path = self.root.join(&candidate_path);
            if !exists_db && !disk_path.exists() {
                break;
            }
            candidate = format!("{} ({})", raw_name, counter);
            counter += 1;
            if counter > 1000 {
                return Err("无法生成唯一目录名".to_string());
            }
        }
        let new_relative = PathBuf::from(&parent_relative).join(&candidate);
        let new_relative_text = path_text(&new_relative);
        let disk_path = self.root.join(&new_relative);
        fs::create_dir_all(&disk_path).map_err(|e| format!("Unable to create directory: {e}"))?;
        let id = Uuid::new_v4().to_string();
        let ts = now();
        let transaction = connection
            .transaction()
            .map_err(|e| format!("Unable to open Collection transaction: {e}"))?;
        transaction
            .execute(
                "INSERT INTO collections(id, parent_id, name, relative_path, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![id, parent_id, candidate, new_relative_text, ts],
            )
            .map_err(|e| e.to_string())?;
        // journal
        let _ = transaction.execute(
            "INSERT INTO operation_journal(id, operation_kind, entity_kind, entity_id, source_path, target_path, state, payload_json, created_at, committed_at) VALUES (?1, 'create_collection', 'collection', ?2, NULL, ?3, 'committed', '{}', ?4, ?4)",
            params![Uuid::new_v4().to_string(), id, new_relative_text, ts],
        );
        bump_library_revisions(&transaction, &[LibraryDomain::Structure])?;
        transaction.commit().map_err(|e| e.to_string())?;
        Ok(CollectionProjection {
            id,
            parent_id: Some(parent_id),
            name: candidate,
            relative_path: new_relative_text,
            sort_mode: "recent".to_string(),
            paper_order: Vec::new(),
        })
    }

    pub fn rename_collection(
        &self,
        relative_path: &str,
        new_name: &str,
    ) -> PaperResult<CollectionProjection> {
        let trimmed = new_name.trim();
        validate_collection_segment(trimmed)?;
        let old_text = crate::library_paths::normalize_slashes(relative_path);
        if old_text == PAPERS_DIR || old_text == TEXTBOOKS_DIR {
            return Err("根目录不能重命名".to_string());
        }
        let old_path = safe_library_relative(Path::new(&old_text))?;
        let old_text = path_text(&old_path);
        let connection = self.connect()?;
        let row: Option<(String, Option<String>, String)> = connection
            .query_row(
                "SELECT id, parent_id, name FROM collections WHERE relative_path = ?1",
                params![old_text],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let (id, parent_id_opt, _old_name) = row.ok_or_else(|| "目录不存在".to_string())?;
        let parent_id = parent_id_opt.ok_or_else(|| "根目录不能重命名".to_string())?;
        let parent_path: String = connection
            .query_row(
                "SELECT relative_path FROM collections WHERE id = ?1",
                params![parent_id],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        let new_relative = PathBuf::from(&parent_path).join(trimmed);
        let new_text = path_text(&new_relative);
        // check cross-root: first segment must stay same
        let old_kind = document_kind_from_relative(&old_text);
        let new_kind = document_kind_from_relative(&new_text);
        if old_kind != new_kind {
            return Err("禁止跨根移动".to_string());
        }
        if new_text == old_text {
            return self.collection_by_id(&connection, &id);
        }
        // target exists?
        let conflict: bool = connection
            .query_row(
                "SELECT 1 FROM collections WHERE relative_path = ?1",
                params![new_text],
                |_| Ok(true),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or(false);
        if conflict || self.root.join(&new_relative).exists() {
            self.create_conflict(&connection, "target_path_conflict", Some(Path::new(&new_text)), None, serde_json::json!({"operation":"rename_collection","source":old_text,"target":new_text}))?;
            return Err("目标已存在，已记录冲突".to_string());
        }
        let old_disk = self.root.join(&old_path);
        let new_disk = self.root.join(&new_relative);
        if !old_disk.is_dir() {
            return Err("目录在磁盘上不存在".to_string());
        }
        reject_symlink_components(&self.root, &old_disk)?;
        if let Some(parent) = new_disk.parent() {
            if parent != self.root && parent.exists() {
                reject_symlink_components(&self.root, parent)?;
            }
        }
        fs::rename(&old_disk, &new_disk).map_err(|e| format!("Unable to rename directory: {e}"))?;
        let db_result: PaperResult<CollectionProjection> = (|| {
            let ts = now();
            let mut conn2 = self.connect()?;
            let tx = conn2.transaction().map_err(|e| e.to_string())?;
            tx.execute("UPDATE collections SET name = ?1, relative_path = ?2, updated_at = ?3 WHERE id = ?4", params![trimmed, new_text, ts, id]).map_err(|e| e.to_string())?;
            let like = format!("{}/%", old_text);
            let mut stmt = tx
                .prepare("SELECT id, relative_path FROM collections WHERE relative_path LIKE ?1")
                .map_err(|e| e.to_string())?;
            let descendants: Vec<(String, String)> = stmt
                .query_map(params![like], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| e.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|e| e.to_string())?;
            drop(stmt);
            for (cid, rel) in descendants {
                let suffix = &rel[old_text.len()..];
                let new_rel = format!("{}{}", new_text, suffix);
                tx.execute(
                    "UPDATE collections SET relative_path = ?1, updated_at = ?2 WHERE id = ?3",
                    params![new_rel, ts, cid],
                )
                .map_err(|e| e.to_string())?;
            }
            let mut papers: Vec<(String, String, String)> = Vec::new();
            {
                let mut ps = tx.prepare("SELECT id, relative_path, collection_id FROM papers WHERE relative_path = ?1 OR relative_path LIKE ?2").map_err(|e| e.to_string())?;
                let like_p = format!("{}/%", old_text);
                let rows = ps
                    .query_map(params![old_text, like_p], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    })
                    .map_err(|e| e.to_string())?;
                for r in rows {
                    papers.push(r.map_err(|e| e.to_string())?);
                }
            }
            for (pid, rel, _old_cid) in papers {
                let new_rel = if rel == old_text {
                    new_text.clone()
                } else {
                    format!("{}{}", new_text, &rel[old_text.len()..])
                };
                let parent_dir = Path::new(&new_rel)
                    .parent()
                    .map(path_text)
                    .unwrap_or_else(|| new_text.clone());
                let new_cid: String = tx
                    .query_row(
                        "SELECT id FROM collections WHERE relative_path = ?1",
                        params![parent_dir],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                tx.execute("UPDATE papers SET relative_path = ?1, collection_id = ?2, updated_at = ?3 WHERE id = ?4", params![new_rel, new_cid, ts, pid]).map_err(|e| e.to_string())?;
            }
            tx.execute("INSERT INTO operation_journal(id, operation_kind, entity_kind, entity_id, source_path, target_path, state, payload_json, created_at, committed_at) VALUES (?1, 'rename_collection', 'collection', ?2, ?3, ?4, 'committed', '{}', ?5, ?5)", params![Uuid::new_v4().to_string(), id, old_text, new_text, ts]).map_err(|e| e.to_string())?;
            bump_library_revisions(&tx, &[LibraryDomain::Structure])?;
            tx.commit().map_err(|e| e.to_string())?;
            let conn = self.connect()?;
            self.collection_by_id(&conn, &id)
        })();
        if db_result.is_err() {
            let _ = fs::rename(&new_disk, &old_disk);
        }
        db_result
    }

    pub fn move_collection(
        &self,
        relative_path: &str,
        dest_parent_path: &str,
    ) -> PaperResult<CollectionProjection> {
        let old_text = crate::library_paths::normalize_slashes(relative_path);
        if old_text == PAPERS_DIR || old_text == TEXTBOOKS_DIR {
            return Err("根目录不能移动".to_string());
        }
        let dest_text = crate::library_paths::normalize_slashes(dest_parent_path);
        let old_path = safe_library_relative(Path::new(&old_text))?;
        let dest_path = safe_library_relative(Path::new(&dest_text))?;
        let old_text = path_text(&old_path);
        let dest_text = path_text(&dest_path);
        // prohibits moving into itself or descendant
        if dest_text == old_text || dest_text.starts_with(&format!("{}/", old_text)) {
            return Err("不能移入自身或子孙".to_string());
        }
        // cross-root prohibition
        let old_kind = document_kind_from_relative(&old_text);
        let dest_kind = document_kind_from_relative(&dest_text);
        if old_kind != dest_kind {
            return Err("禁止跨根移动".to_string());
        }
        let connection = self.connect()?;
        let row: Option<(String, String)> = connection
            .query_row(
                "SELECT id, name FROM collections WHERE relative_path = ?1",
                params![old_text],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let (id, name) = row.ok_or_else(|| "目录不存在".to_string())?;
        let dest_parent_row: Option<String> = connection
            .query_row(
                "SELECT id FROM collections WHERE relative_path = ?1",
                params![dest_text],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let dest_parent_id = dest_parent_row.ok_or_else(|| "目标父目录不存在".to_string())?;
        let new_relative = PathBuf::from(&dest_text).join(&name);
        let new_text = path_text(&new_relative);
        let conflict: bool = connection
            .query_row(
                "SELECT 1 FROM collections WHERE relative_path = ?1",
                params![new_text],
                |_| Ok(true),
            )
            .optional()
            .map_err(|e| e.to_string())?
            .unwrap_or(false);
        if conflict || self.root.join(&new_relative).exists() {
            self.create_conflict(&connection, "target_path_conflict", Some(Path::new(&new_text)), None, serde_json::json!({"operation":"move_collection","source":old_text,"target":new_text}))?;
            return Err("目标已存在，已记录冲突".to_string());
        }
        let old_disk = self.root.join(&old_path);
        let new_disk = self.root.join(&new_relative);
        reject_symlink_components(&self.root, &old_disk)?;
        if let Some(parent) = new_disk.parent() {
            if parent != self.root && parent.exists() {
                reject_symlink_components(&self.root, parent)?;
            }
        }
        fs::rename(&old_disk, &new_disk).map_err(|e| format!("Unable to move directory: {e}"))?;
        let db_result: PaperResult<CollectionProjection> = (|| {
            let ts = now();
            let mut conn2 = self.connect()?;
            let tx = conn2.transaction().map_err(|e| e.to_string())?;
            tx.execute("UPDATE collections SET parent_id = ?1, relative_path = ?2, updated_at = ?3 WHERE id = ?4", params![dest_parent_id, new_text, ts, id]).map_err(|e| e.to_string())?;
            let like = format!("{}/%", old_text);
            let mut stmt = tx
                .prepare("SELECT id, relative_path FROM collections WHERE relative_path LIKE ?1")
                .map_err(|e| e.to_string())?;
            let descendants: Vec<(String, String)> = stmt
                .query_map(params![like], |r| Ok((r.get(0)?, r.get(1)?)))
                .map_err(|e| e.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|e| e.to_string())?;
            drop(stmt);
            for (cid, rel) in descendants {
                let suffix = &rel[old_text.len()..];
                let new_rel = format!("{}{}", new_text, suffix);
                tx.execute(
                    "UPDATE collections SET relative_path = ?1, updated_at = ?2 WHERE id = ?3",
                    params![new_rel, ts, cid],
                )
                .map_err(|e| e.to_string())?;
            }
            let mut papers: Vec<(String, String)> = Vec::new();
            {
                let mut ps = tx.prepare("SELECT id, relative_path FROM papers WHERE relative_path = ?1 OR relative_path LIKE ?2").map_err(|e| e.to_string())?;
                let like_p = format!("{}/%", old_text);
                let rows = ps
                    .query_map(params![old_text, like_p], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })
                    .map_err(|e| e.to_string())?;
                for r in rows {
                    papers.push(r.map_err(|e| e.to_string())?);
                }
            }
            for (pid, rel) in papers {
                let new_rel = if rel == old_text {
                    new_text.clone()
                } else {
                    format!("{}{}", new_text, &rel[old_text.len()..])
                };
                let parent_dir = Path::new(&new_rel)
                    .parent()
                    .map(path_text)
                    .unwrap_or_else(|| new_text.clone());
                let new_cid: String = tx
                    .query_row(
                        "SELECT id FROM collections WHERE relative_path = ?1",
                        params![parent_dir],
                        |r| r.get(0),
                    )
                    .map_err(|e| e.to_string())?;
                tx.execute("UPDATE papers SET relative_path = ?1, collection_id = ?2, updated_at = ?3 WHERE id = ?4", params![new_rel, new_cid, ts, pid]).map_err(|e| e.to_string())?;
            }
            tx.execute("INSERT INTO operation_journal(id, operation_kind, entity_kind, entity_id, source_path, target_path, state, payload_json, created_at, committed_at) VALUES (?1, 'move_collection', 'collection', ?2, ?3, ?4, 'committed', '{}', ?5, ?5)", params![Uuid::new_v4().to_string(), id, old_text, new_text, ts]).map_err(|e| e.to_string())?;
            bump_library_revisions(&tx, &[LibraryDomain::Structure])?;
            tx.commit().map_err(|e| e.to_string())?;
            let conn = self.connect()?;
            self.collection_by_id(&conn, &id)
        })();
        if db_result.is_err() {
            let _ = fs::rename(&new_disk, &old_disk);
        }
        db_result
    }

    pub fn trash_collection(&self, relative_path: &str) -> PaperResult<(Vec<String>, Vec<String>)> {
        let rel_text = crate::library_paths::normalize_slashes(relative_path);
        if rel_text == PAPERS_DIR || rel_text == TEXTBOOKS_DIR {
            return Err("根目录不能删除".to_string());
        }
        let safe = safe_library_relative(Path::new(&rel_text))?;
        let rel_text = path_text(&safe);
        let connection = self.connect()?;
        let exists: Option<String> = connection
            .query_row(
                "SELECT id FROM collections WHERE relative_path = ?1",
                params![rel_text],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if exists.is_none() {
            return Err("目录不存在".to_string());
        }
        // collect active papers under subtree
        let like = format!("{}/%", rel_text);
        let mut stmt = connection.prepare("SELECT id, relative_path FROM papers WHERE deleted_at IS NULL AND (relative_path = ?1 OR relative_path LIKE ?2)").map_err(|e| e.to_string())?;
        let papers: Vec<(String, String)> = stmt
            .query_map(params![rel_text, like], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        drop(stmt);
        let mut trashed_ids = Vec::new();
        let mut failed: Option<String> = None;
        for (pid, _) in papers {
            match self.trash_paper(&pid) {
                Ok(_) => trashed_ids.push(pid),
                Err(e) => {
                    failed = Some(e);
                    continue;
                }
            }
        }
        // delete empty dirs on disk and collections
        let colls: Vec<String> = connection.prepare("SELECT relative_path FROM collections WHERE relative_path = ?1 OR relative_path LIKE ?2 ORDER BY length(relative_path) DESC").map_err(|e| e.to_string())?.query_map(params![rel_text, like], |r| r.get(0)).map_err(|e| e.to_string())?.collect::<rusqlite::Result<Vec<_>>>().map_err(|e| e.to_string())?;
        let mut deleted_dirs = Vec::new();
        for cpath in colls {
            let cnt: i64 = connection.query_row("SELECT COUNT(*) FROM papers WHERE deleted_at IS NULL AND (relative_path = ?1 OR relative_path LIKE ?2)", params![cpath, format!("{}/%", cpath)], |r| r.get(0)).map_err(|e| e.to_string())?;
            if cnt == 0 {
                let _ = reader_context::stash_folder_reader_file(&self.root, &cpath);
                let disk = self.root.join(&cpath);
                if disk.is_dir() {
                    if reject_symlink_components(&self.root, &disk).is_ok() {
                        let has_file = walk_has_file(&disk);
                        if !has_file {
                            let _ = fs::remove_dir_all(&disk);
                        }
                    }
                }
                // handle FK: move any papers (including deleted) that still reference this collection to root before delete
                let coll_id: Option<String> = connection
                    .query_row(
                        "SELECT id FROM collections WHERE relative_path = ?1",
                        params![cpath],
                        |r| r.get(0),
                    )
                    .optional()
                    .map_err(|e| e.to_string())?;
                if let Some(cid) = coll_id {
                    let is_textbook =
                        cpath.starts_with(&format!("{}/", TEXTBOOKS_DIR)) || cpath == TEXTBOOKS_DIR;
                    let root_id = if is_textbook {
                        TEXTBOOKS_ROOT_COLLECTION_ID
                    } else {
                        PAPERS_ROOT_COLLECTION_ID
                    };
                    let _ = connection.execute(
                        "UPDATE papers SET collection_id = ?1 WHERE collection_id = ?2",
                        params![root_id, cid],
                    );
                }
                match connection.execute(
                    "DELETE FROM collections WHERE relative_path = ?1",
                    params![cpath],
                ) {
                    Ok(_) => deleted_dirs.push(cpath),
                    Err(e) => {
                        if failed.is_none() {
                            failed = Some(e.to_string());
                        }
                    }
                }
            }
        }
        if let Some(err) = failed {
            return Err(err);
        }
        if !deleted_dirs.is_empty() {
            bump_library_revisions(&connection, &[LibraryDomain::Structure])?;
        }
        Ok((trashed_ids, deleted_dirs))
    }

    #[allow(dead_code)]
    fn queue_remote_cleanup_for_paper(&self, paper_id: &str) -> PaperResult<()> {
        // best effort: record tombstones for remote files
        let connection = self.connect()?;
        // reuse logic from lib.rs queue_paper_remote_cleanup but simplified: insert into remote_tombstones if needed
        // For now just rely on lib's trash_paper remote handling via crate::... but we can't call that here.
        // We'll attempt to insert tombstones for any context roots.
        let _ = connection;
        let _ = paper_id;
        Ok(())
    }

    pub fn rename_paper(&self, paper_id: &str, new_stem: &str) -> PaperResult<PaperProjection> {
        let stem = new_stem.trim();
        validate_pdf_stem(stem)?;
        let new_file_name = format!("{}.pdf", stem);
        validate_pdf_file_name(&new_file_name)?;
        let connection = self.connect()?;
        let paper = self.paper_by_id(&connection, paper_id)?;
        let moved = self.move_paper(paper_id, &paper.collection_path, &new_file_name, false)?;
        // update title to stem in metadata (document_revisions has no title column)
        let conn2 = self.connect()?;
        conn2
            .execute(
                "UPDATE paper_metadata SET title = ?1 WHERE revision_id = ?2",
                params![stem, moved.revision_id],
            )
            .map_err(|e| e.to_string())?;
        bump_library_revisions(&conn2, &[LibraryDomain::Structure])?;
        // also update title in memory projection
        let mut proj = moved;
        proj.title = stem.to_string();
        Ok(proj)
    }

    fn collection_by_id(
        &self,
        connection: &Connection,
        id: &str,
    ) -> PaperResult<CollectionProjection> {
        let mut proj: CollectionProjection = connection
            .query_row(
                "SELECT id, parent_id, name, relative_path FROM collections WHERE id = ?1",
                params![id],
                |r| {
                    Ok(CollectionProjection {
                        id: r.get(0)?,
                        parent_id: r.get(1)?,
                        name: r.get(2)?,
                        relative_path: r.get(3)?,
                        sort_mode: "recent".to_string(),
                        paper_order: Vec::new(),
                    })
                },
            )
            .map_err(|e| e.to_string())?;
        crate::v2_workspace::ensure_hub_sort_tables(connection).map_err(|e| e.to_string())?;
        let mode: Option<String> = connection
            .query_row(
                "SELECT sort_mode FROM collection_sort_prefs WHERE collection_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if let Some(mode) = mode {
            proj.sort_mode = mode;
        }
        let mut stmt = connection
            .prepare(
                "SELECT paper_id FROM collection_paper_order WHERE collection_id = ?1 ORDER BY position",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map(params![id], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        proj.paper_order = rows
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        Ok(proj)
    }

    pub fn reconcile(&self) -> PaperResult<LibraryProjection> {
        let mut kind_change_notices: Vec<String> = Vec::new();
        let connection = self.connect()?;
        // Empty directory reconciliation
        self.sync_empty_collections(&connection)?;
        let current = self.list_papers(&connection)?;
        let mut disk_files = collect_pdf_files(&self.papers_path)?;
        if self.textbooks_path.is_dir() {
            disk_files.extend(collect_pdf_files(&self.textbooks_path)?);
        }
        disk_files.sort();
        let mut disk_by_relative = HashMap::<String, (String, u64, i64)>::new();
        let mut disk_hashes = HashMap::<String, Vec<PathBuf>>::new();
        for file in &disk_files {
            let relative = library_relative_from_absolute(&self.root, file)?;
            let (hash, byte_size, page_count) = inspect_pdf(file)?;
            disk_by_relative.insert(path_text(&relative), (hash.clone(), byte_size, page_count));
            disk_hashes.entry(hash).or_default().push(relative);
        }
        let mut claimed = HashSet::new();

        for paper in &current {
            if let Some((hash, _, _)) = disk_by_relative.get(&paper.relative_path) {
                claimed.insert(paper.relative_path.clone());
                if hash != &paper.sha256 {
                    let (byte_size, page_count) = disk_by_relative
                        .get(&paper.relative_path)
                        .map(|(_, byte_size, page_count)| (*byte_size, *page_count))
                        .ok_or_else(|| "Discovered PDF metadata is unavailable".to_string())?;
                    self.publish_replacement_revision(
                        &connection,
                        paper,
                        hash,
                        byte_size,
                        page_count,
                    )?;
                }
                continue;
            }
            let candidates = disk_hashes
                .get(&paper.sha256)
                .map(|items| {
                    items
                        .iter()
                        .filter(|path| !claimed.contains(&path_text(path)))
                        .cloned()
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if candidates.len() == 1 {
                let relative = &candidates[0];
                let mut connection = self.connect()?;
                let transaction = connection
                    .transaction()
                    .map_err(|error| error.to_string())?;
                let collection_id = self.ensure_collection(&transaction, relative.parent())?;
                let file_name = relative
                    .file_name()
                    .and_then(|value| value.to_str())
                    .ok_or_else(|| "Moved PDF file name is invalid".to_string())?;
                let prev_collection_id = paper.collection_id.clone();
                transaction
                    .execute(
                        "UPDATE papers
                         SET collection_id = ?1, file_name = ?2, relative_path = ?3, updated_at = ?4
                         WHERE id = ?5",
                        params![
                            collection_id,
                            file_name,
                            path_text(relative),
                            now(),
                            paper.id
                        ],
                    )
                    .map_err(|error| error.to_string())?;
                if prev_collection_id != collection_id {
                    Self::remove_paper_from_manual_order(&transaction, &paper.id)?;
                }
                let mut domains = vec![LibraryDomain::Structure];
                if prev_collection_id != collection_id {
                    domains.push(LibraryDomain::Sort);
                }
                bump_library_revisions(&transaction, &domains)?;
                transaction.commit().map_err(|error| error.to_string())?;
                claimed.insert(path_text(relative));
                if document_kind_from_relative(&paper.relative_path)
                    != document_kind_from_relative(&path_text(relative))
                {
                    kind_change_notices.push(self.apply_kind_change(&paper.id)?);
                }
            } else {
                self.create_conflict(
                    &connection,
                    "missing_or_ambiguous_identity",
                    Some(Path::new(&paper.relative_path)),
                    Some(&paper.id),
                    serde_json::json!({"candidateCount": candidates.len()}),
                )?;
            }
        }

        for file in disk_files {
            let relative = library_relative_from_absolute(&self.root, &file)?;
            let relative_text = path_text(&relative);
            if claimed.contains(&relative_text) {
                continue;
            }
            let (hash, byte_size, page_count) = disk_by_relative
                .get(&relative_text)
                .cloned()
                .ok_or_else(|| "Discovered PDF metadata is unavailable".to_string())?;
            if let Some(existing) = self.paper_by_hash(&connection, &hash)? {
                self.create_conflict(
                    &connection,
                    "duplicate_hash",
                    Some(&relative),
                    Some(&existing.id),
                    serde_json::json!({"sha256": hash}),
                )?;
            } else {
                self.register_new_paper(&connection, &relative, &hash, byte_size, page_count)?;
            }
        }
        let mut projection = self.list_library()?;
        projection.kind_change_notices = kind_change_notices;
        Ok(projection)
    }

    pub fn save_reading_state(&self, state: &ReadingState) -> PaperResult<ReadingState> {
        // Reader viewport saves never touch the revision counters: they fire on
        // every scroll and the Hub projects none of these columns.
        if state.page_number < 1
            || state.page_offset < 0.0
            || state.page_offset > 1.0
            || state.zoom <= 0.0
            || !matches!(state.rotation, 0 | 90 | 180 | 270)
            || !matches!(state.right_tab.as_str(), "discussion" | "artifacts")
            || !matches!(
                state.workspace_layout.as_str(),
                "pdf_discussion" | "pdf_outline" | "outline_only"
            )
            || !matches!(state.outline_view.as_str(), "overview" | "deep_dive")
            || !(200..=420).contains(&state.outline_inspector_width)
        {
            return Err("Reading state is outside the supported range".to_string());
        }
        let connection = self.connect()?;
        connection
            .execute(
                "INSERT INTO reading_states(
                   paper_id, revision_id, page_number, page_offset, zoom, rotation,
                   right_tab, active_artifact_id, active_discussion_id, discussion_draft,
                   quote_basket_json, workspace_layout, active_outline_node_id, outline_view,
                   outline_inspector_width, guide_layer_visible, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
                 ON CONFLICT(paper_id) DO UPDATE SET
                   revision_id = excluded.revision_id,
                   page_number = excluded.page_number,
                   page_offset = excluded.page_offset,
                   zoom = excluded.zoom,
                   rotation = excluded.rotation,
                   right_tab = excluded.right_tab,
                   active_artifact_id = excluded.active_artifact_id,
                   active_discussion_id = excluded.active_discussion_id,
                   discussion_draft = excluded.discussion_draft,
                   quote_basket_json = excluded.quote_basket_json,
                   workspace_layout = excluded.workspace_layout,
                   active_outline_node_id = excluded.active_outline_node_id,
                   outline_view = excluded.outline_view,
                   outline_inspector_width = excluded.outline_inspector_width,
                   guide_layer_visible = excluded.guide_layer_visible,
                   updated_at = excluded.updated_at",
                params![
                    state.paper_id,
                    state.revision_id,
                    state.page_number,
                    state.page_offset,
                    state.zoom,
                    state.rotation,
                    state.right_tab,
                    state.active_artifact_id,
                    state.active_discussion_id,
                    state.discussion_draft,
                    state.quote_basket.to_string(),
                    state.workspace_layout,
                    state.active_outline_node_id,
                    state.outline_view,
                    state.outline_inspector_width,
                    if state.guide_layer_visible { 1 } else { 0 },
                    now()
                ],
            )
            .map_err(|error| error.to_string())?;
        self.reading_state(&state.paper_id)?
            .ok_or_else(|| "Reading state was not published".to_string())
    }

    pub fn ack_long_pdf_warning(&self, paper_id: &str, revision_id: &str) -> PaperResult<()> {
        let connection = self.connect()?;
        let timestamp = now();
        let changed = connection
            .execute(
                "UPDATE reading_states
                 SET long_pdf_warning_acked = 1, revision_id = ?2, updated_at = ?3
                 WHERE paper_id = ?1",
                params![paper_id, revision_id, timestamp],
            )
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            connection
                .execute(
                    "INSERT INTO reading_states(
                       paper_id, revision_id, page_number, page_offset, zoom, rotation,
                       right_tab, discussion_draft, quote_basket_json, workspace_layout,
                       outline_view, outline_inspector_width, guide_layer_visible,
                       long_pdf_warning_acked, updated_at
                     ) VALUES (?1, ?2, 1, 0, 1, 0, 'discussion', '', '[]', 'pdf_discussion',
                               'overview', 280, 1, 1, ?3)",
                    params![paper_id, revision_id, timestamp],
                )
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn long_pdf_warning_acked(&self, paper_id: &str) -> PaperResult<bool> {
        Ok(self
            .reading_state(paper_id)?
            .is_some_and(|state| state.long_pdf_warning_acked))
    }

    pub fn apply_kind_change(&self, paper_id: &str) -> PaperResult<String> {
        let mut connection = self.connect()?;
        let paper = self.paper_by_id(&connection, paper_id)?;
        let kind = document_kind_from_relative(&paper.relative_path)
            .unwrap_or(crate::library_paths::DocumentKind::Paper);
        let timestamp = now();
        // Kind sensitive heads, outline/guide state and running jobs all feed
        // the Hub card, so they drop together with the revision bump or not at
        // all.
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM artifact_heads
                 WHERE paper_id = ?1 AND kind IN (
                   'brief', 'glossary', 'symbol_table', 'metadata', 'reading_roadmap'
                 )",
                params![paper_id],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM outline_deep_dive_heads
                 WHERE overview_revision_id IN (
                   SELECT id FROM outline_revisions WHERE revision_id = ?1
                 )",
                params![paper.revision_id],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM outline_heads WHERE revision_id = ?1",
                params![paper.revision_id],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM outline_plans WHERE revision_id = ?1",
                params![paper.revision_id],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM reading_guide_heads WHERE revision_id = ?1",
                params![paper.revision_id],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "DELETE FROM reading_guide_plans WHERE revision_id = ?1",
                params![paper.revision_id],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "UPDATE context_roots
                 SET invalidated_at = ?2
                 WHERE revision_id = ?1 AND invalidated_at IS NULL",
                params![paper.revision_id, timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "UPDATE jobs
                 SET state = 'cancelled', updated_at = ?2
                 WHERE paper_id = ?1
                   AND state IN ('queued', 'running', 'paused')
                   AND kind IN (
                     'orientation_pack', 'document_artifact', 'outline_overview', 'outline_deep_dive',
                     'reading_roadmap', 'reading_guide'
                   )",
                params![paper_id, timestamp],
            )
            .map_err(|error| error.to_string())?;
        bump_library_revisions(
            &transaction,
            &[LibraryDomain::Artifacts, LibraryDomain::Jobs],
        )?;
        transaction.commit().map_err(|error| error.to_string())?;
        let label = match kind {
            crate::library_paths::DocumentKind::Paper => "论文",
            crate::library_paths::DocumentKind::Textbook => "教材",
        };
        Ok(format!(
            "「{}」现在是{}。Brief、地图、精读、旁批需按新种类重新生成。OCR、Lens 与讨论仍保留。",
            paper.title, label
        ))
    }

    pub fn reading_state(&self, paper_id: &str) -> PaperResult<Option<ReadingState>> {
        let connection = self.connect()?;
        connection
            .query_row(
                "SELECT paper_id, revision_id, page_number, page_offset, zoom, rotation,
                        right_tab, active_artifact_id, active_discussion_id, discussion_draft,
                        quote_basket_json, workspace_layout, active_outline_node_id, outline_view,
                        outline_inspector_width, guide_layer_visible, long_pdf_warning_acked, updated_at
                 FROM reading_states WHERE paper_id = ?1",
                params![paper_id],
                |row| {
                    let basket: String = row.get(10)?;
                    let visible: i64 = row.get(15)?;
                    let acked: i64 = row.get(16)?;
                    Ok(ReadingState {
                        paper_id: row.get(0)?,
                        revision_id: row.get(1)?,
                        page_number: row.get(2)?,
                        page_offset: row.get(3)?,
                        zoom: row.get(4)?,
                        rotation: row.get(5)?,
                        right_tab: row.get(6)?,
                        active_artifact_id: row.get(7)?,
                        active_discussion_id: row.get(8)?,
                        discussion_draft: row.get(9)?,
                        quote_basket: serde_json::from_str(&basket)
                            .unwrap_or_else(|_| serde_json::json!([])),
                        workspace_layout: row.get(11)?,
                        active_outline_node_id: row.get(12)?,
                        outline_view: row.get(13)?,
                        outline_inspector_width: row.get(14)?,
                        guide_layer_visible: visible != 0,
                        long_pdf_warning_acked: acked != 0,
                        updated_at: row.get(17)?,
                    })
                },
            )
            .optional()
            .map_err(|error| error.to_string())
    }

    pub fn list_annotations(
        &self,
        paper_id: &str,
    ) -> PaperResult<Vec<crate::annotation_module::UserAnnotation>> {
        let connection = self.connect()?;
        crate::annotation_module::list(&connection, paper_id)
    }

    pub fn create_annotation(
        &self,
        input: &crate::annotation_module::UserAnnotationInput,
    ) -> PaperResult<crate::annotation_module::UserAnnotation> {
        let connection = self.connect()?;
        crate::annotation_module::create(&connection, input)
    }

    pub fn update_annotation(
        &self,
        request: &crate::annotation_module::UserAnnotationUpdateRequest,
    ) -> PaperResult<Option<crate::annotation_module::UserAnnotation>> {
        let connection = self.connect()?;
        crate::annotation_module::update(&connection, request)
    }

    pub fn delete_annotation(&self, id: &str) -> PaperResult<bool> {
        let connection = self.connect()?;
        crate::annotation_module::delete(&connection, id)
    }

    pub fn list_annotation_links(
        &self,
        paper_id: &str,
    ) -> PaperResult<Vec<crate::annotation_module::UserAnnotationLink>> {
        let connection = self.connect()?;
        crate::annotation_module::list_links(&connection, paper_id)
    }

    pub fn create_annotation_link(
        &self,
        input: &crate::annotation_module::UserAnnotationLinkInput,
    ) -> PaperResult<crate::annotation_module::UserAnnotationLink> {
        let connection = self.connect()?;
        crate::annotation_module::create_link(&connection, input)
    }

    pub fn delete_annotation_link(&self, id: &str) -> PaperResult<bool> {
        let connection = self.connect()?;
        crate::annotation_module::delete_link(&connection, id)
    }

    pub fn trash_paper(&self, paper_id: &str) -> PaperResult<bool> {
        let paper = self.paper_by_id(&self.connect()?, paper_id)?;
        let source = self.absolute_paper_path(&paper.relative_path)?;
        let trash_id = Uuid::new_v4().to_string();
        let trash_relative = PathBuf::from("trash")
            .join(&trash_id)
            .join(&paper.file_name);
        let target = self.root.join(".read-desktop").join(&trash_relative);
        fs::create_dir_all(target.parent().ok_or("Trash target has no parent")?)
            .map_err(|error| format!("Unable to create Trash directory: {error}"))?;
        fs::rename(&source, &target).map_err(|error| format!("Unable to trash PDF: {error}"))?;
        if let Some(trash_dir) = target.parent() {
            if let Err(error) = reader_context::copy_pdf_sidecar_into_dir(
                &self.root,
                &paper.relative_path,
                trash_dir,
            ) {
                let _ = fs::rename(&target, &source);
                return Err(error);
            }
        }

        let result = (|| -> PaperResult<()> {
            let mut connection = self.connect()?;
            let transaction = connection
                .transaction()
                .map_err(|error| error.to_string())?;
            let timestamp = now();
            transaction
                .execute(
                    "UPDATE papers SET deleted_at = ?1, updated_at = ?1 WHERE id = ?2",
                    params![timestamp, paper_id],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "UPDATE jobs
                     SET state = CASE WHEN provider_committed = 1
                                      THEN 'interrupted_unknown' ELSE 'cancelled' END,
                         stage = CASE WHEN provider_committed = 1
                                      THEN 'interrupted_unknown' ELSE 'cancelled' END,
                         updated_at = ?1
                     WHERE paper_id = ?2 AND state IN ('queued', 'running', 'paused')",
                    params![timestamp, paper_id],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "INSERT INTO trash_entries(
                       id, paper_id, original_relative_path, trash_relative_path,
                       deleted_at, purge_after
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        trash_id,
                        paper_id,
                        paper.relative_path,
                        path_text(&trash_relative),
                        timestamp,
                        (Utc::now() + Duration::days(30)).to_rfc3339()
                    ],
                )
                .map_err(|error| error.to_string())?;
            Self::remove_paper_from_manual_order(&transaction, paper_id)?;
            bump_library_revisions(
                &transaction,
                &[
                    LibraryDomain::Structure,
                    LibraryDomain::Sort,
                    LibraryDomain::Jobs,
                ],
            )?;
            transaction.commit().map_err(|error| error.to_string())
        })();
        if let Err(error) = result {
            let _ = fs::rename(&target, &source);
            if let Some(trash_dir) = target.parent() {
                let _ = reader_context::restore_pdf_sidecar_from_dir(
                    &self.root,
                    &paper.relative_path,
                    trash_dir,
                );
            }
            return Err(error);
        }
        Ok(true)
    }

    pub fn restore_paper(&self, paper_id: &str) -> PaperResult<PaperProjection> {
        let connection = self.connect()?;
        let entry = connection
            .query_row(
                "SELECT id, original_relative_path, trash_relative_path
                 FROM trash_entries
                 WHERE paper_id = ?1 AND restored_at IS NULL
                 ORDER BY deleted_at DESC LIMIT 1",
                params![paper_id],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Paper is not in Trash".to_string())?;
        let source = self.trash_absolute_path(&entry.2)?;
        let target = self.absolute_paper_path(&entry.1)?;
        if target.exists() {
            self.create_conflict(
                &connection,
                "restore_path_conflict",
                Some(Path::new(&entry.1)),
                Some(paper_id),
                serde_json::json!({"trashEntryId": entry.0}),
            )?;
            return Err("Restore target already exists; a conflict was recorded".to_string());
        }
        fs::create_dir_all(target.parent().ok_or("Restore target has no parent")?)
            .map_err(|error| format!("Unable to create restore directory: {error}"))?;
        fs::rename(&source, &target).map_err(|error| format!("Unable to restore PDF: {error}"))?;
        if let Some(trash_dir) = source.parent() {
            if let Err(error) =
                reader_context::restore_pdf_sidecar_from_dir(&self.root, &entry.1, trash_dir)
            {
                let _ = fs::rename(&target, &source);
                return Err(error);
            }
        }
        let rollback_restore = |root: &Path, original: &str, dest: &Path, trash: &Path| {
            let _ = fs::rename(dest, trash);
            if let Some(trash_dir) = trash.parent() {
                let _ = reader_context::copy_pdf_sidecar_into_dir(root, original, trash_dir);
            }
        };
        let result = connection.execute_batch("BEGIN IMMEDIATE;");
        if let Err(error) = result {
            rollback_restore(&self.root, &entry.1, &target, &source);
            return Err(error.to_string());
        }
        let updated = connection
            .execute(
                "UPDATE papers SET deleted_at = NULL, updated_at = ?1 WHERE id = ?2",
                params![now(), paper_id],
            )
            .and_then(|_| {
                connection.execute(
                    "UPDATE trash_entries SET restored_at = ?1 WHERE id = ?2",
                    params![now(), entry.0],
                )
            });
        if let Err(error) = updated {
            let _ = connection.execute_batch("ROLLBACK;");
            rollback_restore(&self.root, &entry.1, &target, &source);
            return Err(error.to_string());
        }
        if let Err(error) = bump_library_revisions(&connection, &[LibraryDomain::Structure]) {
            let _ = connection.execute_batch("ROLLBACK;");
            rollback_restore(&self.root, &entry.1, &target, &source);
            return Err(error);
        }
        connection
            .execute_batch("COMMIT;")
            .map_err(|error| error.to_string())?;
        self.paper_by_id(&connection, paper_id)
    }

    pub fn list_trash(&self) -> PaperResult<Vec<TrashProjection>> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT t.id, t.paper_id, COALESCE(m.title, p.file_name), p.file_name,
                        t.original_relative_path, t.trash_relative_path,
                        t.deleted_at, t.purge_after
                 FROM trash_entries t
                 JOIN papers p ON p.id = t.paper_id
                 LEFT JOIN paper_heads h ON h.paper_id = p.id
                 LEFT JOIN paper_metadata m ON m.revision_id = h.revision_id
                 WHERE t.restored_at IS NULL
                 ORDER BY t.deleted_at DESC",
            )
            .map_err(|error| error.to_string())?;
        let entries = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        Ok(entries
            .into_iter()
            .map(
                |(
                    id,
                    paper_id,
                    title,
                    file_name,
                    original_relative_path,
                    trash_relative_path,
                    deleted_at,
                    purge_after,
                )| TrashProjection {
                    id,
                    paper_id,
                    title,
                    file_name,
                    original_relative_path,
                    source_bytes: self
                        .trash_absolute_path(&trash_relative_path)
                        .ok()
                        .and_then(|path| fs::metadata(path).ok())
                        .map(|metadata| metadata.len())
                        .unwrap_or(0),
                    deleted_at,
                    purge_after,
                    kind: default_trash_kind(),
                },
            )
            .collect())
    }

    pub fn storage_report(&self) -> PaperResult<StorageReport> {
        let connection = self.connect()?;
        let papers = self.list_papers(&connection)?;
        let mut logical_by_paper = HashMap::<String, (u64, u64, u64)>::new();
        let mut logical_statement = connection
            .prepare(
                "SELECT p.id,
                        COALESCE(a.bytes, 0),
                        COALESCE(d.bytes, 0),
                        COALESCE(o.bytes, 0)
                 FROM papers p
                 LEFT JOIN (
                   SELECT paper_id,
                          SUM(LENGTH(content_json) + LENGTH(evidence_json)
                              + LENGTH(dependency_snapshot_json)) AS bytes
                   FROM artifacts GROUP BY paper_id
                 ) a ON a.paper_id = p.id
                 LEFT JOIN (
                   SELECT d.paper_id,
                          SUM(LENGTH(m.content) + LENGTH(m.citations_json)) AS bytes
                   FROM messages m
                   JOIN discussions d ON d.id = m.discussion_id
                   GROUP BY d.paper_id
                 ) d ON d.paper_id = p.id
                 LEFT JOIN (
                   SELECT r.paper_id,
                          SUM(LENGTH(b.text_content) + LENGTH(b.content_digest)) AS bytes
                   FROM ocr_blocks b
                   JOIN ocr_pages pg ON pg.id = b.ocr_page_id
                   JOIN ocr_revisions ocr ON ocr.id = pg.ocr_revision_id
                   JOIN document_revisions r ON r.id = ocr.revision_id
                   GROUP BY r.paper_id
                 ) o ON o.paper_id = p.id
                 WHERE p.deleted_at IS NULL",
            )
            .map_err(|error| error.to_string())?;
        let logical_rows = logical_statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?.max(0) as u64,
                    row.get::<_, i64>(2)?.max(0) as u64,
                    row.get::<_, i64>(3)?.max(0) as u64,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        for (paper_id, artifact, discussion, ocr) in logical_rows {
            logical_by_paper.insert(paper_id, (artifact, discussion, ocr));
        }
        let mut paper_logical_bytes = Vec::new();
        let mut papers_bytes = 0u64;
        for paper in papers {
            let source_bytes = fs::metadata(self.absolute_paper_path(&paper.relative_path)?)
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            papers_bytes += source_bytes;
            let (artifact_bytes, discussion_bytes, ocr_bytes) =
                logical_by_paper.remove(&paper.id).unwrap_or_default();
            paper_logical_bytes.push(PaperStorage {
                paper_id: paper.id,
                source_bytes,
                logical_database_bytes: artifact_bytes + discussion_bytes + ocr_bytes,
                artifact_bytes,
                discussion_bytes,
                ocr_bytes,
            });
        }
        let artifact_logical_bytes = {
            let mut statement = connection
                .prepare(
                    "SELECT a.id, a.paper_id, a.kind, a.object_key, a.version,
                            LENGTH(a.content_json) + LENGTH(a.evidence_json)
                              + LENGTH(a.dependency_snapshot_json)
                     FROM artifacts a
                     JOIN papers p ON p.id = a.paper_id
                     WHERE p.deleted_at IS NULL
                     ORDER BY a.paper_id, a.kind, a.object_key, a.version DESC",
                )
                .map_err(|error| error.to_string())?;
            let values = statement
                .query_map([], |row| {
                    Ok(ArtifactStorage {
                        artifact_id: row.get(0)?,
                        paper_id: row.get(1)?,
                        kind: row.get(2)?,
                        object_key: row.get(3)?,
                        version: row.get(4)?,
                        logical_bytes: row.get::<_, i64>(5)?.max(0) as u64,
                    })
                })
                .map_err(|error| error.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|error| error.to_string())?;
            values
        };
        let internal_bytes = directory_size(&self.root.join(".read-desktop"))?;
        Ok(StorageReport {
            workspace_bytes: papers_bytes + internal_bytes,
            papers_bytes,
            internal_bytes,
            paper_logical_bytes,
            artifact_logical_bytes,
        })
    }
    fn connect(&self) -> PaperResult<Connection> {
        db::open(&self.database_path)
            .map_err(|error| format!("Unable to open Workspace database: {error}"))
    }

    fn ensure_root_collections(&self, connection: &Connection) -> PaperResult<()> {
        let timestamp = now();
        connection
            .execute(
                "INSERT OR IGNORE INTO collections(
                   id, parent_id, name, relative_path, created_at, updated_at
                 ) VALUES (?1, NULL, ?2, ?2, ?3, ?3)",
                params![PAPERS_ROOT_COLLECTION_ID, PAPERS_DIR, timestamp],
            )
            .map_err(|error| error.to_string())?;
        connection
            .execute(
                "INSERT OR IGNORE INTO collections(
                   id, parent_id, name, relative_path, created_at, updated_at
                 ) VALUES (?1, NULL, ?2, ?2, ?3, ?3)",
                params![TEXTBOOKS_ROOT_COLLECTION_ID, TEXTBOOKS_DIR, timestamp],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn cleanup_expired_trash(&self, connection: &Connection) -> PaperResult<()> {
        let cutoff = now();
        let mut statement = connection
            .prepare(
                "SELECT id, trash_relative_path FROM trash_entries
                 WHERE restored_at IS NULL AND purge_after <= ?1",
            )
            .map_err(|error| error.to_string())?;
        let entries = statement
            .query_map(params![cutoff], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        drop(statement);
        for (id, relative) in entries {
            let path = self.trash_absolute_path(&relative)?;
            let purge_path = path.with_file_name(format!(".{id}.purging"));
            let mut moved = false;
            if path.exists() {
                fs::rename(&path, &purge_path).map_err(|error| {
                    format!("Unable to stage expired Trash entry for deletion: {error}")
                })?;
                moved = true;
            }

            let transaction = connection
                .unchecked_transaction()
                .map_err(|error| error.to_string())?;
            let deleted = transaction
                .execute(
                    "DELETE FROM trash_entries
                     WHERE id = ?1 AND restored_at IS NULL AND purge_after <= ?2",
                    params![id, cutoff],
                )
                .map_err(|error| error.to_string());
            match deleted {
                Ok(1) => transaction.commit().map_err(|error| error.to_string())?,
                Ok(_) => {
                    let _ = transaction.rollback();
                    if moved {
                        let _ = fs::rename(&purge_path, &path);
                    }
                    continue;
                }
                Err(error) => {
                    let _ = transaction.rollback();
                    if moved {
                        let _ = fs::rename(&purge_path, &path);
                    }
                    return Err(error);
                }
            }
            if moved {
                if purge_path.is_dir() {
                    fs::remove_dir_all(&purge_path).map_err(|error| error.to_string())?;
                } else if purge_path.exists() {
                    fs::remove_file(&purge_path).map_err(|error| error.to_string())?;
                }
                remove_empty_trash_parents(&self.root, &path);
            }
        }
        Ok(())
    }

    fn cleanup_orphan_staging(&self, connection: &Connection) -> PaperResult<()> {
        let mut staging = Vec::new();
        collect_part_files(&self.papers_path, &mut staging)?;
        if self.textbooks_path.is_dir() {
            collect_part_files(&self.textbooks_path, &mut staging)?;
        }
        for path in staging {
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            let operation_id = name
                .strip_suffix(".part")
                .and_then(|value| value.rsplit('.').next())
                .unwrap_or_default();
            let prepared = connection
                .query_row(
                    "SELECT 1 FROM operation_journal WHERE id = ?1 AND state = 'prepared'",
                    params![operation_id],
                    |_| Ok(true),
                )
                .optional()
                .map_err(|error| error.to_string())?
                .unwrap_or(false);
            if !prepared {
                let _ = fs::remove_file(path);
            }
        }
        Ok(())
    }

    fn ensure_collection(
        &self,
        transaction: &Transaction<'_>,
        relative: Option<&Path>,
    ) -> PaperResult<String> {
        let Some(relative) = relative else {
            return Err("PDF must live in Papers or Textbooks".to_string());
        };
        if relative.as_os_str().is_empty() {
            return Err("PDF must live in Papers or Textbooks".to_string());
        }
        let safe = safe_library_relative(relative)?;
        let mut components = safe.components();
        let first = match components.next() {
            Some(Component::Normal(name)) => name,
            _ => return Err("Collection path contains an unsafe component".to_string()),
        };
        let first_name = first.to_string_lossy();
        let mut parent_id = if first_name.eq_ignore_ascii_case(PAPERS_DIR) {
            PAPERS_ROOT_COLLECTION_ID.to_string()
        } else if first_name.eq_ignore_ascii_case(TEXTBOOKS_DIR) {
            TEXTBOOKS_ROOT_COLLECTION_ID.to_string()
        } else {
            return Err("Collection must be under Papers or Textbooks".to_string());
        };
        let mut accumulated = PathBuf::from(if first_name.eq_ignore_ascii_case(TEXTBOOKS_DIR) {
            TEXTBOOKS_DIR
        } else {
            PAPERS_DIR
        });
        for component in components {
            let Component::Normal(name) = component else {
                return Err("Collection path contains an unsafe component".to_string());
            };
            accumulated.push(name);
            let relative_text = path_text(&accumulated);
            if let Some(existing) = transaction
                .query_row(
                    "SELECT id FROM collections WHERE relative_path = ?1",
                    params![relative_text],
                    |row| row.get::<_, String>(0),
                )
                .optional()
                .map_err(|error| error.to_string())?
            {
                parent_id = existing;
                continue;
            }
            let id = Uuid::new_v4().to_string();
            transaction
                .execute(
                    "INSERT INTO collections(
                       id, parent_id, name, relative_path, created_at, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                    params![id, parent_id, name.to_string_lossy(), relative_text, now()],
                )
                .map_err(|error| error.to_string())?;
            parent_id = id;
        }
        Ok(parent_id)
    }

    fn sync_empty_collections(&self, connection: &Connection) -> PaperResult<()> {
        // collect dirs from disk
        let mut disk_dirs: Vec<PathBuf> = Vec::new();
        collect_dirs_into(&self.papers_path, &mut disk_dirs)?;
        if self.textbooks_path.is_dir() {
            collect_dirs_into(&self.textbooks_path, &mut disk_dirs)?;
        }
        let mut disk_set = HashSet::new();
        // single transaction for all ensure_collection to avoid N+1
        {
            let mut conn = self.connect()?;
            let tx = conn.transaction().map_err(|e| e.to_string())?;
            let before: i64 = tx
                .query_row("SELECT COUNT(*) FROM collections", [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            for dir in &disk_dirs {
                if let Ok(rel) = library_relative_from_absolute(&self.root, dir) {
                    let text = path_text(&rel);
                    let _ = self.ensure_collection(&tx, Some(&rel));
                    disk_set.insert(text);
                }
            }
            let after: i64 = tx
                .query_row("SELECT COUNT(*) FROM collections", [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            if after != before {
                bump_library_revisions(&tx, &[LibraryDomain::Structure])?;
            }
            tx.commit().map_err(|e| e.to_string())?;
        }
        // also insert any dirs not yet in set due to ensure_collection failure? Ensure set still populated
        for dir in &disk_dirs {
            if let Ok(rel) = library_relative_from_absolute(&self.root, dir) {
                disk_set.insert(path_text(&rel));
            }
        }
        // single query for active counts to avoid N+1
        let mut del_stmt = connection.prepare("SELECT c.id, c.relative_path, (SELECT COUNT(*) FROM papers p WHERE p.deleted_at IS NULL AND (p.relative_path = c.relative_path OR p.relative_path LIKE c.relative_path || '/%')) as cnt FROM collections c") .map_err(|e| e.to_string())?;
        let rows: Vec<(String, String, i64)> = del_stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map_err(|e| e.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| e.to_string())?;
        drop(del_stmt);
        let mut removed_collections = 0usize;
        for (cid, rel, cnt) in rows {
            if rel == PAPERS_DIR || rel == TEXTBOOKS_DIR {
                continue;
            }
            if disk_set.contains(&rel) {
                continue;
            }
            if cnt != 0 {
                continue;
            }
            removed_collections += connection
                .execute("DELETE FROM collections WHERE id = ?1", params![cid])
                .map_err(|e| e.to_string())?;
        }
        if removed_collections > 0 {
            bump_library_revisions(connection, &[LibraryDomain::Structure])?;
        }
        Ok(())
    }

    fn register_new_paper(
        &self,
        connection: &Connection,
        relative_path: &Path,
        sha256: &str,
        byte_size: u64,
        page_count: i64,
    ) -> PaperResult<PaperProjection> {
        let relative_path = safe_library_relative(relative_path)?;
        let file_name = relative_path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| "PDF file name is invalid".to_string())?;
        validate_pdf_file_name(file_name)?;
        let transaction = connection
            .unchecked_transaction()
            .map_err(|error| error.to_string())?;
        let collection_id = self.ensure_collection(&transaction, relative_path.parent())?;
        let paper_id = Uuid::new_v4().to_string();
        let revision_id = Uuid::new_v4().to_string();
        let timestamp = now();
        let title = relative_path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("Untitled paper");
        let rel_text = path_text(&relative_path);
        transaction
            .execute(
                "UPDATE papers SET relative_path = relative_path || '.deleted.' || id
                 WHERE relative_path = ?1 AND deleted_at IS NOT NULL",
                params![rel_text],
            )
            .map_err(|error| error.to_string())?;
        bump_library_revisions(&transaction, &[LibraryDomain::Structure])?;
        transaction
            .execute(
                "INSERT INTO papers(
                   id, collection_id, file_name, relative_path, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                params![paper_id, collection_id, file_name, rel_text, timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO document_revisions(
                   id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    revision_id,
                    paper_id,
                    sha256,
                    byte_size as i64,
                    page_count,
                    path_text(&relative_path),
                    timestamp
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO paper_heads(paper_id, revision_id, updated_at)
                 VALUES (?1, ?2, ?3)",
                params![paper_id, revision_id, timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO paper_metadata(
                   revision_id, title, authors_json, metadata_json, source, created_at
                 ) VALUES (?1, ?2, '[]', '{}', 'filename', ?3)",
                params![revision_id, title, timestamp],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        self.paper_by_id(connection, &paper_id)
    }

    fn publish_replacement_revision(
        &self,
        connection: &Connection,
        paper: &PaperProjection,
        sha256: &str,
        byte_size: u64,
        page_count: i64,
    ) -> PaperResult<()> {
        let existing_revision = connection
            .query_row(
                "SELECT id FROM document_revisions WHERE paper_id = ?1 AND sha256 = ?2",
                params![paper.id, sha256],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let revision_id = existing_revision
            .clone()
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let transaction = connection
            .unchecked_transaction()
            .map_err(|error| error.to_string())?;
        if existing_revision.is_none() {
            transaction
                .execute(
                    "INSERT INTO document_revisions(
                       id, paper_id, sha256, byte_size, page_count,
                       source_relative_path, created_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![
                        revision_id,
                        paper.id,
                        sha256,
                        byte_size as i64,
                        page_count,
                        paper.relative_path,
                        now()
                    ],
                )
                .map_err(|error| error.to_string())?;
            transaction
                .execute(
                    "INSERT INTO paper_metadata(
                       revision_id, title, authors_json, publication_year,
                       venue, doi, abstract_text, metadata_json, source, created_at
                     )
                     SELECT ?1, title, authors_json, publication_year, venue, doi,
                            abstract_text, json_set(metadata_json, '$._inheritedFromRevision', revision_id, '$._display.titleSource', '旧 PDF 修订', '$._display.authorsSource', '旧 PDF 修订', '$._display.yearLabel', '旧 PDF 修订·保留年份', '$._display.year', CASE WHEN json_extract(metadata_json,'$._format')='auxiliary-v2' THEN json_extract(metadata_json,'$._display.year') ELSE publication_year END), 'inherited', ?2
                     FROM paper_metadata WHERE revision_id = ?3",
                    params![revision_id, now(), paper.revision_id],
                )
                .map_err(|error| error.to_string())?;
        }
        transaction
            .execute(
                "UPDATE paper_heads SET revision_id = ?1, updated_at = ?2 WHERE paper_id = ?3",
                params![revision_id, now(), paper.id],
            )
            .map_err(|error| error.to_string())?;
        bump_library_revisions(&transaction, &[LibraryDomain::Structure])?;
        transaction.commit().map_err(|error| error.to_string())
    }

    fn paper_by_id(&self, connection: &Connection, paper_id: &str) -> PaperResult<PaperProjection> {
        self.query_paper(connection, "p.id = ?1", paper_id)?
            .ok_or_else(|| "Paper was not found".to_string())
    }

    fn paper_by_hash(
        &self,
        connection: &Connection,
        sha256: &str,
    ) -> PaperResult<Option<PaperProjection>> {
        self.query_paper(connection, "r.sha256 = ?1", sha256)
    }

    fn query_paper(
        &self,
        connection: &Connection,
        predicate: &str,
        value: &str,
    ) -> PaperResult<Option<PaperProjection>> {
        let sql = format!(
            "SELECT p.id, r.id, COALESCE(NULLIF(m.title, ''), p.file_name), m.authors_json, m.publication_year,
                    r.page_count, p.collection_id, c.relative_path, p.file_name,
                    p.relative_path, r.sha256, r.byte_size, r.created_at, m.metadata_json
             FROM papers p
             JOIN paper_heads h ON h.paper_id = p.id
             JOIN document_revisions r ON r.id = h.revision_id
             JOIN collections c ON c.id = p.collection_id
             LEFT JOIN paper_metadata m ON m.revision_id = r.id
             WHERE p.deleted_at IS NULL AND {predicate}
             LIMIT 1"
        );
        connection
            .query_row(&sql, params![value], paper_from_row)
            .optional()
            .map_err(|error| error.to_string())
    }

    fn list_papers(&self, connection: &Connection) -> PaperResult<Vec<PaperProjection>> {
        let mut statement = connection
            .prepare(
                "SELECT p.id, r.id, COALESCE(NULLIF(m.title, ''), p.file_name), m.authors_json, m.publication_year,
                        r.page_count, p.collection_id, c.relative_path, p.file_name,
                        p.relative_path, r.sha256, r.byte_size, r.created_at, m.metadata_json
                 FROM papers p
                 JOIN paper_heads h ON h.paper_id = p.id
                 JOIN document_revisions r ON r.id = h.revision_id
                 JOIN collections c ON c.id = p.collection_id
                 LEFT JOIN paper_metadata m ON m.revision_id = r.id
                 WHERE p.deleted_at IS NULL
                 ORDER BY c.relative_path, p.file_name",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], paper_from_row)
            .map_err(|error| error.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())
    }

    fn list_collections(&self, connection: &Connection) -> PaperResult<Vec<CollectionProjection>> {
        let mut statement = connection
            .prepare(
                "SELECT id, parent_id, name, relative_path
                 FROM collections ORDER BY relative_path",
            )
            .map_err(|error| error.to_string())?;
        let base: Vec<CollectionProjection> = statement
            .query_map([], |row| {
                Ok(CollectionProjection {
                    id: row.get(0)?,
                    parent_id: row.get(1)?,
                    name: row.get(2)?,
                    relative_path: row.get(3)?,
                    sort_mode: "recent".to_string(),
                    paper_order: Vec::new(),
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        drop(statement);
        crate::v2_workspace::ensure_hub_sort_tables(connection).map_err(|e| e.to_string())?;
        let mut prefs_stmt = connection
            .prepare("SELECT collection_id, sort_mode FROM collection_sort_prefs")
            .map_err(|error| error.to_string())?;
        let prefs_rows = prefs_stmt
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        drop(prefs_stmt);
        let prefs: HashMap<String, String> = prefs_rows.into_iter().collect();
        let mut order_stmt = connection
            .prepare("SELECT collection_id, paper_id, position FROM collection_paper_order ORDER BY collection_id, position")
            .map_err(|error| error.to_string())?;
        let order_rows = order_stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        drop(order_stmt);
        let mut order_map: HashMap<String, Vec<(i64, String)>> = HashMap::new();
        for (cid, pid, pos) in order_rows {
            order_map.entry(cid).or_default().push((pos, pid));
        }
        let mut result = Vec::with_capacity(base.len());
        for mut col in base {
            if let Some(mode) = prefs.get(&col.id) {
                col.sort_mode = mode.clone();
            }
            if let Some(ordered) = order_map.get(&col.id) {
                let mut sorted = ordered.clone();
                sorted.sort_by_key(|(pos, _)| *pos);
                col.paper_order = sorted.into_iter().map(|(_, pid)| pid).collect();
            }
            result.push(col);
        }
        Ok(result)
    }

    pub fn reorder_collection_papers(
        &self,
        collection_id: &str,
        paper_ids: &[String],
    ) -> PaperResult<()> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        crate::v2_workspace::ensure_hub_sort_tables(&transaction).map_err(|e| e.to_string())?;
        let exists: Option<String> = transaction
            .query_row(
                "SELECT id FROM collections WHERE id = ?1",
                params![collection_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if exists.is_none() {
            return Err("Collection does not exist".to_string());
        }
        let mut stmt = transaction
            .prepare("SELECT id FROM papers WHERE collection_id = ?1 AND deleted_at IS NULL")
            .map_err(|error| error.to_string())?;
        let live: Vec<String> = stmt
            .query_map(params![collection_id], |row| row.get(0))
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        drop(stmt);
        let live_set: HashSet<String> = live.iter().cloned().collect();
        let input_set: HashSet<String> = paper_ids.iter().cloned().collect();
        if live_set != input_set {
            return Err(
                "paperIds must be exact permutation of live papers in collection".to_string(),
            );
        }
        if paper_ids.len() != live.len() {
            return Err(
                "paperIds must be exact permutation of live papers in collection".to_string(),
            );
        }
        // No-op guard: if the live set was already saved in this exact order,
        // skip the rewrite to spare WAL pressure and avoid spurious change rows.
        let mut existing_stmt = transaction
            .prepare("SELECT paper_id FROM collection_paper_order WHERE collection_id = ?1 ORDER BY position")
            .map_err(|error| error.to_string())?;
        let existing: Vec<String> = existing_stmt
            .query_map(params![collection_id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        drop(existing_stmt);
        if existing == paper_ids {
            transaction.commit().map_err(|error| error.to_string())?;
            return Ok(());
        }
        transaction
            .execute(
                "DELETE FROM collection_paper_order WHERE collection_id = ?1",
                params![collection_id],
            )
            .map_err(|error| error.to_string())?;
        for (pos, pid) in paper_ids.iter().enumerate() {
            transaction
                .execute(
                    "INSERT INTO collection_paper_order(collection_id, paper_id, position) VALUES (?1, ?2, ?3)",
                    params![collection_id, pid, pos as i64],
                )
                .map_err(|error| error.to_string())?;
        }
        bump_library_revisions(&transaction, &[LibraryDomain::Sort])?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(())
    }

    pub fn set_collection_sort_mode(
        &self,
        collection_id: &str,
        sort_mode: &str,
    ) -> PaperResult<()> {
        let allowed = ["recent", "year", "title", "manual", "chapter"];
        if !allowed.contains(&sort_mode) {
            return Err("Invalid sort mode".to_string());
        }
        let mut connection = self.connect()?;
        crate::v2_workspace::ensure_hub_sort_tables(&connection).map_err(|e| e.to_string())?;
        let exists: Option<String> = connection
            .query_row(
                "SELECT id FROM collections WHERE id = ?1",
                params![collection_id],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if exists.is_none() {
            return Err("Collection does not exist".to_string());
        }
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        transaction
            .execute(
                "INSERT INTO collection_sort_prefs(collection_id, sort_mode, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT(collection_id) DO UPDATE SET sort_mode = excluded.sort_mode, updated_at = excluded.updated_at",
                params![collection_id, sort_mode, now()],
            )
            .map_err(|error| error.to_string())?;
        bump_library_revisions(&transaction, &[LibraryDomain::Sort])?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(())
    }

    fn remove_paper_from_manual_order(
        transaction: &Transaction,
        paper_id: &str,
    ) -> PaperResult<()> {
        transaction
            .execute(
                "DELETE FROM collection_paper_order WHERE paper_id = ?1",
                params![paper_id],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn list_conflicts(&self, connection: &Connection) -> PaperResult<Vec<ConflictProjection>> {
        let mut statement = connection
            .prepare(
                "SELECT id, conflict_kind, relative_path, paper_id, details_json, created_at
                 FROM reconciliation_conflicts
                 WHERE status = 'open'
                 ORDER BY created_at DESC",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                let details: String = row.get(4)?;
                Ok(ConflictProjection {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    relative_path: row.get(2)?,
                    paper_id: row.get(3)?,
                    details: serde_json::from_str(&details)
                        .unwrap_or_else(|_| serde_json::json!({})),
                    created_at: row.get(5)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())
    }

    fn create_conflict(
        &self,
        connection: &Connection,
        kind: &str,
        relative_path: Option<&Path>,
        paper_id: Option<&str>,
        details: serde_json::Value,
    ) -> PaperResult<ConflictProjection> {
        let relative_text = relative_path.map(path_text);
        if let Some(existing) = connection
            .query_row(
                "SELECT id FROM reconciliation_conflicts
                 WHERE conflict_kind = ?1
                   AND COALESCE(relative_path, '') = COALESCE(?2, '')
                   AND COALESCE(paper_id, '') = COALESCE(?3, '')
                   AND status = 'open'
                 LIMIT 1",
                params![kind, relative_text, paper_id],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| error.to_string())?
        {
            return self
                .list_conflicts(connection)?
                .into_iter()
                .find(|conflict| conflict.id == existing)
                .ok_or_else(|| "Conflict projection is unavailable".to_string());
        }
        let id = Uuid::new_v4().to_string();
        let created_at = now();
        connection
            .execute(
                "INSERT INTO reconciliation_conflicts(
                   id, conflict_kind, relative_path, paper_id,
                   details_json, status, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'open', ?6)",
                params![
                    id,
                    kind,
                    relative_text,
                    paper_id,
                    details.to_string(),
                    created_at
                ],
            )
            .map_err(|error| error.to_string())?;
        bump_library_revisions(connection, &[LibraryDomain::Structure])?;
        Ok(ConflictProjection {
            id,
            kind: kind.to_string(),
            relative_path: relative_text,
            paper_id: paper_id.map(str::to_string),
            details,
            created_at,
        })
    }

    fn absolute_paper_path(&self, relative_path: &str) -> PaperResult<PathBuf> {
        join_workspace_relative(&self.root, relative_path)
    }

    /// Resolve a database Trash path without ever following a path outside
    /// `.read-desktop/trash`. Corrupt or manually edited rows must not turn
    /// Workspace activation/restoration into an arbitrary file operation.
    fn trash_absolute_path(&self, relative_path: &str) -> PaperResult<PathBuf> {
        let relative = safe_relative_path(Path::new(relative_path))?;
        let first = relative.components().next();
        if !matches!(first, Some(Component::Normal(name)) if name == "trash") {
            return Err("Trash path is outside the Workspace Trash directory".to_string());
        }
        let trash_root = self.root.join(".read-desktop").join("trash");
        let target = self.root.join(".read-desktop").join(&relative);
        ensure_inside(&trash_root, &target)
            .map_err(|_| "Trash path escaped the Workspace Trash directory".to_string())?;
        reject_symlink_components(&trash_root, &target)?;
        Ok(target)
    }
}

fn paper_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<PaperProjection> {
    let authors_json: Option<String> = row.get(3)?;
    let authors = authors_json
        .and_then(|value| serde_json::from_str::<Vec<String>>(&value).ok())
        .unwrap_or_default();
    Ok(PaperProjection {
        id: row.get(0)?,
        revision_id: row.get(1)?,
        title: row
            .get::<_, Option<String>>(2)?
            .unwrap_or_else(|| "Untitled paper".to_string()),
        authors,
        publication_year: row.get(4)?,
        display_metadata: row
            .get::<_, Option<String>>(13)?
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
            .and_then(|v| v.get("_display").cloned()),
        page_count: row.get(5)?,
        collection_id: row.get(6)?,
        collection_path: row.get(7)?,
        file_name: row.get(8)?,
        relative_path: row.get(9)?,
        sha256: row.get(10)?,
        byte_size: row.get(11)?,
        imported_at: row.get(12)?,
    })
}

fn safe_relative_path(path: &Path) -> PaperResult<PathBuf> {
    if path.is_absolute() {
        return Err("Path must be relative to Papers".to_string());
    }
    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => safe.push(value),
            _ => return Err("Path contains an unsafe component".to_string()),
        }
    }
    Ok(safe)
}

fn ensure_inside(root: &Path, path: &Path) -> PaperResult<()> {
    if !path.starts_with(root) {
        return Err("Path escaped the Workspace Papers directory".to_string());
    }
    Ok(())
}

fn reject_symlink_components(root: &Path, path: &Path) -> PaperResult<()> {
    if fs::symlink_metadata(root)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("Path root is a symbolic link".to_string());
    }
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "Path escaped its allowed root".to_string())?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err("Path contains an unsafe component".to_string());
        };
        current.push(name);
        if fs::symlink_metadata(&current)
            .map(|metadata| metadata.file_type().is_symlink())
            .unwrap_or(false)
        {
            return Err("Path contains a symbolic link".to_string());
        }
    }
    Ok(())
}

fn remove_empty_trash_parents(workspace_root: &Path, path: &Path) {
    let trash_root = workspace_root.join(".read-desktop").join("trash");
    let mut current = path.parent().map(Path::to_path_buf);
    while let Some(directory) = current {
        if directory == trash_root || !directory.starts_with(&trash_root) {
            break;
        }
        match fs::remove_dir(&directory) {
            Ok(()) => current = directory.parent().map(Path::to_path_buf),
            Err(_) => break,
        }
    }
}

fn validate_pdf_path(path: &Path) -> PaperResult<()> {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("pdf") {
        return Err("Only native PDF files are supported".to_string());
    }
    Ok(())
}

fn is_windows_reserved(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    let base = upper.split('.').next().unwrap_or(&upper);
    matches!(base, "CON" | "PRN" | "AUX" | "NUL")
        || (base.starts_with("COM") && base.len() == 4 && base.as_bytes()[3].is_ascii_digit())
        || (base.starts_with("LPT") && base.len() == 4 && base.as_bytes()[3].is_ascii_digit())
}

fn validate_collection_segment(name: &str) -> PaperResult<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.contains(':')
        || name.contains('*')
        || name.contains('?')
        || name.contains('"')
        || name.contains('<')
        || name.contains('>')
        || name.contains('|')
    {
        return Err(format!("目录名不合法: {}", name));
    }
    if name.ends_with(' ') || name.ends_with('.') {
        return Err(format!("目录名不能以空格或点结尾: {}", name));
    }
    if is_windows_reserved(name) {
        return Err(format!("目录名是 Windows 保留名: {}", name));
    }
    Ok(())
}

fn validate_pdf_file_name(file_name: &str) -> PaperResult<()> {
    if file_name.is_empty()
        || file_name.contains('/')
        || file_name.contains('\\')
        || file_name == "."
        || file_name == ".."
    {
        return Err("PDF file name is unsafe".to_string());
    }
    // segment rules for stem
    let path = Path::new(file_name);
    let ext = path.extension().and_then(|v| v.to_str()).unwrap_or("");
    if !ext.eq_ignore_ascii_case("pdf") {
        return Err("Only native PDF files are supported".to_string());
    }
    let stem = path.file_stem().and_then(|v| v.to_str()).unwrap_or("");
    if stem.is_empty() {
        return Err("PDF file name is unsafe".to_string());
    }
    if stem.contains('/')
        || stem.contains('\\')
        || stem.contains(':')
        || stem.contains('*')
        || stem.contains('?')
        || stem.contains('"')
        || stem.contains('<')
        || stem.contains('>')
        || stem.contains('|')
    {
        return Err(format!("文件名不合法: {}", file_name));
    }
    if stem == "." || stem == ".." {
        return Err("PDF file name is unsafe".to_string());
    }
    if stem.ends_with(' ') || stem.ends_with('.') {
        return Err(format!("文件名不能以空格或点结尾: {}", file_name));
    }
    if is_windows_reserved(stem) {
        return Err(format!("文件名是 Windows 保留名: {}", file_name));
    }
    Ok(())
}

fn validate_pdf_stem(stem: &str) -> PaperResult<()> {
    if stem.is_empty() {
        return Err("文件名不能为空".to_string());
    }
    validate_collection_segment(stem)?;
    // already validated via segment, but need to ensure no .pdf inside - stem shouldn't contain dot? Actually dots allowed except trailing
    Ok(())
}

fn inspect_pdf(path: &Path) -> PaperResult<(String, u64, i64)> {
    validate_pdf_path(path)?;
    let file = fs::File::open(path).map_err(|error| format!("Unable to read PDF: {error}"))?;
    let size = file
        .metadata()
        .map_err(|error| format!("Unable to stat PDF: {error}"))?
        .len();
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut hasher = Sha256::new();
    let mut header = Vec::with_capacity(5);
    let mut tail = Vec::new();
    let mut pages = 0_i64;
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("Unable to read PDF: {error}"))?;
        if read == 0 {
            break;
        }
        let chunk = &buffer[..read];
        if header.len() < 5 {
            let needed = 5 - header.len();
            header.extend_from_slice(&chunk[..needed.min(chunk.len())]);
        }
        hasher.update(chunk);
        pages += count_page_markers_in_chunks(&mut tail, chunk);
    }
    pages += flush_page_marker_tail(&mut tail);
    if header.as_slice() != b"%PDF-" {
        return Err("The selected file is not a PDF".to_string());
    }
    Ok((format!("{:x}", hasher.finalize()), size, pages.max(1)))
}

/// Copy a PDF to a staging path while computing its digest and page estimate
/// from the same byte stream. This keeps the imported bytes and the metadata
/// atomically tied together even if the source is modified during import.
fn copy_and_inspect_pdf(source: &Path, target: &Path) -> PaperResult<(String, u64, i64)> {
    validate_pdf_path(source)?;
    let input = fs::File::open(source).map_err(|error| format!("Unable to read PDF: {error}"))?;
    let output =
        fs::File::create(target).map_err(|error| format!("Unable to stage PDF: {error}"))?;
    let mut reader = BufReader::with_capacity(1024 * 1024, input);
    let mut writer = BufWriter::with_capacity(1024 * 1024, output);
    let mut hasher = Sha256::new();
    let mut header = Vec::with_capacity(5);
    let mut tail = Vec::new();
    let mut pages = 0_i64;
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| format!("Unable to read PDF: {error}"))?;
        if read == 0 {
            break;
        }
        let chunk = &buffer[..read];
        if header.len() < 5 {
            let needed = 5 - header.len();
            header.extend_from_slice(&chunk[..needed.min(chunk.len())]);
        }
        writer
            .write_all(chunk)
            .map_err(|error| format!("Unable to stage PDF: {error}"))?;
        hasher.update(chunk);
        bytes = bytes.saturating_add(read as u64);
        pages += count_page_markers_in_chunks(&mut tail, chunk);
    }
    writer
        .flush()
        .map_err(|error| format!("Unable to flush staged PDF: {error}"))?;
    pages += flush_page_marker_tail(&mut tail);
    if header.as_slice() != b"%PDF-" {
        return Err("The selected file is not a PDF".to_string());
    }
    Ok((format!("{:x}", hasher.finalize()), bytes, pages.max(1)))
}

#[cfg(test)]
fn copy_file_streaming(source: &Path, target: &Path) -> PaperResult<()> {
    let input = fs::File::open(source).map_err(|error| error.to_string())?;
    let output = fs::File::create(target).map_err(|error| error.to_string())?;
    let mut reader = BufReader::with_capacity(1024 * 1024, input);
    let mut writer = BufWriter::with_capacity(1024 * 1024, output);
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if count == 0 {
            break;
        }
        writer
            .write_all(&buffer[..count])
            .map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

fn count_page_markers_in_chunks(tail: &mut Vec<u8>, chunk: &[u8]) -> i64 {
    const NEEDLE: &[u8] = b"/Type /Page";
    let mut window = Vec::with_capacity(tail.len() + chunk.len());
    window.extend_from_slice(tail);
    window.extend_from_slice(chunk);
    // Keep a complete marker at the end of a chunk uncounted until the next
    // byte is available. This avoids mistaking `/Type /Pages` for a page when
    // the trailing `s` arrives in the next read.
    let scan_until = window.len().saturating_sub(NEEDLE.len());
    let count = window
        .windows(NEEDLE.len())
        .enumerate()
        .take_while(|(index, _)| *index < scan_until)
        .filter(|(index, candidate)| {
            *candidate == NEEDLE
                && window
                    .get(index + NEEDLE.len())
                    .is_none_or(|next| *next != b's')
        })
        .count() as i64;
    let keep = NEEDLE.len();
    tail.clear();
    tail.extend_from_slice(&window[window.len().saturating_sub(keep)..]);
    count
}

/// Count a marker retained at EOF, where the absence of a following byte
/// proves that it is not the `/Type /Pages` plural token.
fn flush_page_marker_tail(tail: &mut Vec<u8>) -> i64 {
    const NEEDLE: &[u8] = b"/Type /Page";
    let count = tail
        .windows(NEEDLE.len())
        .enumerate()
        .filter(|(index, candidate)| {
            *candidate == NEEDLE
                && tail
                    .get(index + NEEDLE.len())
                    .is_none_or(|next| *next != b's')
        })
        .count() as i64;
    tail.clear();
    count
}

#[cfg(test)]
#[cfg(test)]
fn sha256_file(path: &Path) -> PaperResult<String> {
    let file = fs::File::open(path).map_err(|error| format!("Unable to hash PDF: {error}"))?;
    let mut reader = BufReader::with_capacity(1024 * 1024, file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = reader
            .read(&mut buffer)
            .map_err(|error| format!("Unable to hash PDF: {error}"))?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn collect_dirs_into(root: &Path, dirs: &mut Vec<PathBuf>) -> PaperResult<()> {
    for entry in fs::read_dir(root).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        let ft = entry.file_type().map_err(|e| e.to_string())?;
        if ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            dirs.push(path.clone());
            collect_dirs_into(&path, dirs)?;
        }
    }
    Ok(())
}

fn walk_has_file(dir: &Path) -> bool {
    if let Ok(entries) = fs::read_dir(dir) {
        for e in entries.flatten() {
            if let Ok(ft) = e.file_type() {
                if ft.is_symlink() {
                    continue;
                }
                if ft.is_file() {
                    return true;
                }
                if ft.is_dir() && walk_has_file(&e.path()) {
                    return true;
                }
            }
        }
    }
    false
}

fn collect_pdf_files(root: &Path) -> PaperResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_pdf_files_into(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_pdf_files_into(root: &Path, files: &mut Vec<PathBuf>) -> PaperResult<()> {
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_pdf_files_into(&path, files)?;
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("pdf"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn collect_part_files(root: &Path, files: &mut Vec<PathBuf>) -> PaperResult<()> {
    for entry in fs::read_dir(root).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            collect_part_files(&path, files)?;
        } else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("part"))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn directory_size(path: &Path) -> PaperResult<u64> {
    if !path.exists() {
        return Ok(0);
    }
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if metadata.file_type().is_symlink() {
        return Ok(0);
    }
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    let mut total = 0;
    for entry in fs::read_dir(path).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        total += directory_size(&entry.path())?;
    }
    Ok(total)
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::library_paths::document_kind_from_relative;
    use crate::v2_workspace::WorkspaceModule;
    use sha2::{Digest, Sha256};
    use tempfile::tempdir;

    fn pdf(seed: &str) -> Vec<u8> {
        format!("%PDF-1.4\n{seed}\n1 0 obj <</Type /Page>>\n%%EOF").into_bytes()
    }

    fn ready_module() -> (tempfile::TempDir, WorkspaceModule, PaperModule) {
        let root = tempdir().expect("temporary Workspace");
        let workspace = WorkspaceModule::new();
        workspace.open(root.path()).expect("initialize Workspace");
        let paper = PaperModule::open(root.path()).expect("open PaperModule");
        (root, workspace, paper)
    }

    #[test]
    fn imports_into_nested_collection_and_publishes_one_head() {
        let (root, _workspace, module) = ready_module();
        let source_root = tempdir().expect("source directory");
        let source = source_root.path().join("paper.pdf");
        fs::write(&source, pdf("first")).expect("source PDF");

        let imported = module
            .import_pdf(&source, Some("ML/Vision/Segmentation"))
            .expect("import");
        let paper = imported.paper.expect("published paper");

        assert!(imported.copied);
        assert_eq!(paper.collection_path, "Papers/ML/Vision/Segmentation");
        assert_eq!(
            fs::read(root.path().join("Papers/ML/Vision/Segmentation/paper.pdf"))
                .expect("copied PDF"),
            pdf("first")
        );
        let library = module.list_library().expect("library");
        assert!(library
            .collections
            .iter()
            .any(|collection| collection.relative_path == "Papers/ML/Vision/Segmentation"));
        assert_eq!(library.papers.len(), 1);
    }

    #[test]
    fn registers_a_pdf_already_inside_papers_without_copying_it() {
        let (root, _workspace, module) = ready_module();
        let source = root.path().join("Papers/Inbox/inside.pdf");
        fs::create_dir_all(source.parent().unwrap()).expect("Inbox");
        fs::write(&source, pdf("inside")).expect("source PDF");

        let imported = module.import_pdf(&source, None).expect("register");

        assert!(!imported.copied);
        assert_eq!(
            imported.paper.expect("paper").relative_path,
            "Papers/Inbox/inside.pdf"
        );
    }

    #[test]
    fn external_rename_preserves_identity_and_content_change_publishes_revision() {
        let (root, _workspace, module) = ready_module();
        let source_root = tempdir().expect("source directory");
        let source = source_root.path().join("paper.pdf");
        fs::write(&source, pdf("first")).expect("source PDF");
        let original = module
            .import_pdf(&source, Some("Inbox"))
            .expect("import")
            .paper
            .expect("paper");

        let old_path = root.path().join("Papers/Inbox/paper.pdf");
        let new_path = root.path().join("Papers/Archive/Renamed.pdf");
        fs::create_dir_all(new_path.parent().unwrap()).expect("Archive");
        fs::rename(&old_path, &new_path).expect("Explorer rename");
        let renamed = module
            .reconcile()
            .expect("reconcile rename")
            .papers
            .into_iter()
            .next()
            .expect("renamed paper");
        assert_eq!(renamed.id, original.id);
        assert_eq!(renamed.relative_path, "Papers/Archive/Renamed.pdf");
        assert_eq!(renamed.revision_id, original.revision_id);

        fs::write(&new_path, pdf("replacement")).expect("replace content");
        let replaced = module
            .reconcile()
            .expect("reconcile replacement")
            .papers
            .into_iter()
            .next()
            .expect("replacement");
        assert_eq!(replaced.id, original.id);
        assert_ne!(replaced.revision_id, original.revision_id);
    }

    #[test]
    fn cross_root_reconcile_detaches_kind_sensitive_heads_and_keeps_ocr_and_discussion() {
        let (root, _workspace, module) = ready_module();
        let source_root = tempdir().expect("source directory");
        let source = source_root.path().join("paper.pdf");
        fs::write(&source, pdf("cross-root")).expect("source PDF");
        let paper = module
            .import_pdf(&source, Some("Inbox"))
            .expect("import")
            .paper
            .expect("paper");
        let connection = module.connect().expect("connect");

        let artifact_id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO artifacts(id, paper_id, revision_id, kind, version, status, content_json, created_at)
                 VALUES (?1, ?2, ?3, 'brief', 1, 'ready', '{}', ?4)",
                params![artifact_id, paper.id, paper.revision_id, now()],
            )
            .expect("artifact");
        connection
            .execute(
                "INSERT INTO artifact_heads(paper_id, kind, object_key, artifact_id, updated_at)
                 VALUES (?1, 'brief', '', ?2, ?3)",
                params![paper.id, artifact_id, now()],
            )
            .expect("brief head");

        let ocr_id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO ocr_revisions(id, revision_id, status, provider, model, created_at)
                 VALUES (?1, ?2, 'ready', 'mistral', 'ocr-latest', ?3)",
                params![ocr_id, paper.revision_id, now()],
            )
            .expect("ocr revision");
        let root_id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO context_roots(id, revision_id, provider, model, context_epoch, provider_file_id, state, created_at)
                 VALUES (?1, ?2, 'gemini', 'gemini-2.5-pro', 'epoch-1', 'file-1', 'ready', ?3)",
                params![root_id, paper.revision_id, now()],
            )
            .expect("context root");
        let discussion_id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO discussions(id, paper_id, revision_id, title, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, 'Q&A', 'active', ?4, ?4)",
                params![discussion_id, paper.id, paper.revision_id, now()],
            )
            .expect("discussion");
        let job_id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO jobs(id, kind, provider, paper_id, revision_id, dedupe_key, state, stage, priority, payload_json, created_at, updated_at)
                 VALUES (?1, 'orientation_pack', 'gemini', ?2, ?3, 'orientation:x', 'queued', 'queued', 90, '{}', ?4, ?4)",
                params![job_id, paper.id, paper.revision_id, now()],
            )
            .expect("orientation job");
        drop(connection);

        let old_path = root.path().join("Papers/Inbox/paper.pdf");
        let new_path = root.path().join("Textbooks/CLRS/paper.pdf");
        fs::create_dir_all(new_path.parent().expect("parent")).expect("Textbooks/CLRS");
        fs::rename(&old_path, &new_path).expect("Explorer move");

        let library = module.reconcile().expect("reconcile cross-root");
        assert!(
            library
                .kind_change_notices
                .iter()
                .any(|notice| notice.contains("教材")),
            "expected a kind change notice, got {:?}",
            library.kind_change_notices
        );
        let moved = library
            .papers
            .iter()
            .find(|candidate| candidate.id == paper.id)
            .expect("moved paper");
        assert_eq!(moved.relative_path, "Textbooks/CLRS/paper.pdf");

        let connection = module.connect().expect("connect");
        let brief_head: Option<String> = connection
            .query_row(
                "SELECT artifact_id FROM artifact_heads WHERE paper_id = ?1 AND kind = 'brief'",
                params![paper.id],
                |row| row.get(0),
            )
            .optional()
            .expect("brief query")
            .flatten();
        assert!(brief_head.is_none(), "brief head must be detached");
        let ocr_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM ocr_revisions WHERE revision_id = ?1",
                params![paper.revision_id],
                |row| row.get(0),
            )
            .expect("ocr count");
        assert_eq!(ocr_count, 1, "OCR must be kept");
        let discussion_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM discussions WHERE paper_id = ?1",
                params![paper.id],
                |row| row.get(0),
            )
            .expect("discussion count");
        assert_eq!(discussion_count, 1, "discussion must be kept");
        let invalidated: Option<String> = connection
            .query_row(
                "SELECT invalidated_at FROM context_roots WHERE id = ?1",
                params![root_id],
                |row| row.get(0),
            )
            .optional()
            .expect("root query")
            .flatten();
        assert!(invalidated.is_some(), "context root must be invalidated");
        let job_state: String = connection
            .query_row(
                "SELECT state FROM jobs WHERE id = ?1",
                params![job_id],
                |row| row.get(0),
            )
            .expect("job state");
        assert_eq!(job_state, "cancelled", "orientation job must be cancelled");
    }

    #[test]
    fn same_root_rename_keeps_kind_sensitive_heads() {
        let (root, _workspace, module) = ready_module();
        let source_root = tempdir().expect("source directory");
        let source = source_root.path().join("paper.pdf");
        fs::write(&source, pdf("rename")).expect("source PDF");
        let paper = module
            .import_pdf(&source, Some("Inbox"))
            .expect("import")
            .paper
            .expect("paper");
        let connection = module.connect().expect("connect");
        let artifact_id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO artifacts(id, paper_id, revision_id, kind, version, status, content_json, created_at)
                 VALUES (?1, ?2, ?3, 'brief', 1, 'ready', '{}', ?4)",
                params![artifact_id, paper.id, paper.revision_id, now()],
            )
            .expect("artifact");
        connection
            .execute(
                "INSERT INTO artifact_heads(paper_id, kind, object_key, artifact_id, updated_at)
                 VALUES (?1, 'brief', '', ?2, ?3)",
                params![paper.id, artifact_id, now()],
            )
            .expect("brief head");
        drop(connection);

        let old_path = root.path().join("Papers/Inbox/paper.pdf");
        let new_path = root.path().join("Papers/Inbox/Renamed.pdf");
        fs::rename(&old_path, &new_path).expect("rename in place");

        let library = module.reconcile().expect("reconcile rename");
        assert!(
            library.kind_change_notices.is_empty(),
            "same-root rename must not detach heads"
        );
        let connection = module.connect().expect("connect");
        let brief_head: Option<String> = connection
            .query_row(
                "SELECT artifact_id FROM artifact_heads WHERE paper_id = ?1 AND kind = 'brief'",
                params![paper.id],
                |row| row.get(0),
            )
            .optional()
            .expect("brief query")
            .flatten();
        assert!(
            brief_head.is_some(),
            "brief head must survive same-root rename"
        );
    }

    #[test]
    fn move_paper_requires_confirmation_across_roots() {
        let (_root, _workspace, module) = ready_module();
        let source_root = tempdir().expect("source directory");
        let source = source_root.path().join("paper.pdf");
        fs::write(&source, pdf("confirm")).expect("source PDF");
        let paper = module
            .import_pdf(&source, Some("Inbox"))
            .expect("import")
            .paper
            .expect("paper");

        let unconfirmed = module.move_paper(&paper.id, "Textbooks/CLRS", "paper.pdf", false);
        assert_eq!(
            unconfirmed.err().as_deref(),
            Some("kind_change_confirmation_required")
        );
        let confirmed = module
            .move_paper(&paper.id, "Textbooks/CLRS", "paper.pdf", true)
            .expect("confirmed move");
        assert_eq!(confirmed.relative_path, "Textbooks/CLRS/paper.pdf");
    }

    #[test]
    fn move_and_trash_paper_take_reader_sidecar() {
        let (root, _workspace, module) = ready_module();
        let source_root = tempdir().expect("source directory");
        let source = source_root.path().join("alpha.pdf");
        fs::write(&source, pdf("sidecar")).expect("source PDF");
        let paper = module
            .import_pdf(&source, Some("Inbox"))
            .expect("import")
            .paper
            .expect("paper");
        crate::reader_context::write_or_delete(
            &crate::reader_context::pdf_sidecar_path(root.path(), &paper.relative_path).unwrap(),
            "pdf notes",
        )
        .unwrap();
        crate::reader_context::write_or_delete(
            &crate::reader_context::folder_file_path(root.path(), "Papers/Inbox").unwrap(),
            "inbox notes",
        )
        .unwrap();

        let moved = module
            .move_paper(&paper.id, "Papers/Archive", "beta.pdf", false)
            .expect("move");
        assert!(
            !crate::reader_context::pdf_sidecar_path(root.path(), "Papers/Inbox/alpha.pdf")
                .unwrap()
                .exists()
        );
        assert_eq!(
            crate::reader_context::read_trimmed(
                &crate::reader_context::pdf_sidecar_path(root.path(), &moved.relative_path)
                    .unwrap()
            )
            .unwrap()
            .as_deref(),
            Some("pdf notes")
        );

        module.trash_paper(&moved.id).expect("trash");
        assert!(
            !crate::reader_context::pdf_sidecar_path(root.path(), &moved.relative_path)
                .unwrap()
                .exists()
        );
        module.restore_paper(&moved.id).expect("restore");
        assert_eq!(
            crate::reader_context::read_trimmed(
                &crate::reader_context::pdf_sidecar_path(root.path(), "Papers/Archive/beta.pdf")
                    .unwrap()
            )
            .unwrap()
            .as_deref(),
            Some("pdf notes")
        );

        module
            .trash_collection("Papers/Inbox")
            .expect("trash inbox");
        assert!(
            !crate::reader_context::folder_file_path(root.path(), "Papers/Inbox")
                .unwrap()
                .exists()
        );
        let stashed = crate::reader_context::list_folder_trash(root.path()).unwrap();
        assert_eq!(stashed.len(), 1);
        assert_eq!(stashed[0].original_relative_path, "Papers/Inbox");
    }

    #[test]
    fn long_pdf_warning_ack_round_trip() {
        let (_root, _workspace, module) = ready_module();
        let source_root = tempdir().expect("source directory");
        let source = source_root.path().join("paper.pdf");
        fs::write(&source, pdf("ack")).expect("source PDF");
        let paper = module
            .import_pdf(&source, Some("Inbox"))
            .expect("import")
            .paper
            .expect("paper");
        assert!(!module
            .long_pdf_warning_acked(&paper.id)
            .expect("not acked yet"));
        module
            .ack_long_pdf_warning(&paper.id, &paper.revision_id)
            .expect("ack");
        assert!(module.long_pdf_warning_acked(&paper.id).expect("acked"));
    }

    #[test]
    fn duplicate_hash_creates_conflict_without_second_paper() {
        let (_root, _workspace, module) = ready_module();
        let source_root = tempdir().expect("source directory");
        let first = source_root.path().join("first.pdf");
        let second = source_root.path().join("second.pdf");
        fs::write(&first, pdf("same")).expect("first PDF");
        fs::write(&second, pdf("same")).expect("second PDF");
        module
            .import_pdf(&first, Some("Inbox"))
            .expect("first import");

        let duplicate = module
            .import_pdf(&second, Some("Archive"))
            .expect("duplicate result");

        assert!(duplicate.paper.is_none());
        assert_eq!(duplicate.conflict.expect("conflict").kind, "duplicate_hash");
        assert_eq!(module.list_library().expect("library").papers.len(), 1);
    }

    #[test]
    fn reimport_after_trash_restores_paper_without_error() {
        let (_root, _workspace, module) = ready_module();
        let source_root = tempdir().expect("source directory");
        let source = source_root.path().join("attention.pdf");
        fs::write(&source, pdf("attention")).expect("source PDF");

        let imported = module
            .import_pdf(&source, Some("Inbox"))
            .expect("first import")
            .paper
            .expect("first paper");
        assert_eq!(module.list_library().expect("library").papers.len(), 1);

        // Move to trash
        module.trash_paper(&imported.id).expect("trash paper");
        assert_eq!(module.list_library().expect("library").papers.len(), 0);

        // Re-importing the same PDF should seamlessly restore it without unique constraint error
        let reimported = module
            .import_pdf(&source, Some("Inbox"))
            .expect("reimport after trash")
            .paper
            .expect("restored paper");

        assert_eq!(reimported.id, imported.id);
        assert_eq!(module.list_library().expect("library").papers.len(), 1);
    }

    #[test]
    fn reading_state_round_trips_and_trash_restore_never_overwrites() {
        let (root, _workspace, module) = ready_module();
        let source_root = tempdir().expect("source directory");
        let source = source_root.path().join("paper.pdf");
        fs::write(&source, pdf("paper")).expect("source PDF");
        let paper = module
            .import_pdf(&source, Some("Inbox"))
            .expect("import")
            .paper
            .expect("paper");
        let state = ReadingState {
            paper_id: paper.id.clone(),
            revision_id: paper.revision_id.clone(),
            page_number: 7,
            page_offset: 0.42,
            zoom: 1.35,
            rotation: 90,
            right_tab: "artifacts".to_string(),
            active_artifact_id: None,
            active_discussion_id: None,
            discussion_draft: "draft".to_string(),
            quote_basket: serde_json::json!([{"blockId": "b-1"}]),
            workspace_layout: "pdf_discussion".to_string(),
            active_outline_node_id: None,
            outline_view: "overview".to_string(),
            outline_inspector_width: 280,
            guide_layer_visible: true,
            long_pdf_warning_acked: false,
            updated_at: String::new(),
        };
        let saved = module.save_reading_state(&state).expect("save state");
        assert_eq!(saved.page_number, 7);
        assert_eq!(saved.quote_basket, state.quote_basket);
        assert_eq!(saved.outline_inspector_width, 280);
        assert!(saved.guide_layer_visible);

        let mut wide = state.clone();
        wide.outline_inspector_width = 320;
        let saved_wide = module.save_reading_state(&wide).expect("save width");
        assert_eq!(saved_wide.outline_inspector_width, 320);

        let mut invalid = state.clone();
        invalid.outline_inspector_width = 100;
        assert!(module.save_reading_state(&invalid).is_err());

        module.trash_paper(&paper.id).expect("trash paper");
        assert!(module.list_library().expect("library").papers.is_empty());
        let trash = module.list_trash().expect("trash projection");
        assert_eq!(trash.len(), 1);
        assert_eq!(trash[0].paper_id, paper.id);
        assert!(trash[0].source_bytes > 0);

        let restored = module.restore_paper(&paper.id).expect("restore");
        assert_eq!(restored.id, paper.id);
        assert!(module.list_trash().expect("empty trash").is_empty());
        assert!(root.path().join("Papers/Inbox/paper.pdf").is_file());
        let storage = module.storage_report().expect("storage report");
        assert_eq!(storage.paper_logical_bytes.len(), 1);
        assert!(storage.workspace_bytes >= storage.papers_bytes);
    }

    fn import_three_textbook_pdfs(
        _root: &tempfile::TempDir,
        module: &PaperModule,
        collection_path: &str,
        names: &[&str],
    ) -> Vec<PaperProjection> {
        let source_root = tempdir().expect("source directory");
        let mut papers = Vec::new();
        for (idx, name) in names.iter().enumerate() {
            let source = source_root.path().join(format!("{name}.pdf"));
            fs::write(&source, pdf(&format!("seed-{idx}"))).expect("source PDF");
            let imported = module
                .import_pdf(&source, Some(collection_path))
                .expect("import pdf");
            let paper = imported.paper.expect("published paper");
            papers.push(paper);
        }
        papers
    }

    fn lookup_collection_id(connection: &Connection, relative_path: &str) -> String {
        connection
            .query_row(
                "SELECT id FROM collections WHERE relative_path = ?1",
                params![relative_path],
                |row| row.get::<_, String>(0),
            )
            .expect("collection id")
    }

    fn order_position(connection: &Connection, collection_id: &str, paper_id: &str) -> Option<i64> {
        connection
            .query_row(
                "SELECT position FROM collection_paper_order
                 WHERE collection_id = ?1 AND paper_id = ?2",
                params![collection_id, paper_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .expect("order position")
    }

    #[test]
    fn ensure_hub_sort_tables_is_idempotent_and_does_not_bump_user_version() {
        let (root, _workspace, _module) = ready_module();
        let database_path = root.path().join(".read-desktop/workspace.sqlite3");
        // First open already called ensure. Drop the tables to simulate a
        // pre-feature database; Schema 8 validation now refuses to open such a
        // workspace, so the repair path is exercised on the connection itself.
        let connection = db::open(&database_path).expect("open");
        let user_version_before: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version before drop");
        connection
            .execute_batch("DROP TABLE collection_paper_order; DROP TABLE collection_sort_prefs;")
            .expect("drop");
        crate::v2_workspace::ensure_hub_sort_tables(&connection).expect("first ensure");
        let collection_id: String = connection
            .query_row(
                "SELECT id FROM collections ORDER BY id LIMIT 1",
                [],
                |row| row.get(0),
            )
            .expect("a root collection exists");
        connection
            .execute(
                "INSERT INTO collection_sort_prefs(collection_id, sort_mode, updated_at)
                 VALUES (?1, 'title', '2026-08-31T00:00:00Z')",
                params![collection_id],
            )
            .expect("seed sort preference");
        crate::v2_workspace::ensure_hub_sort_tables(&connection).expect("second ensure");
        let tables: Vec<String> = {
            let mut tables_stmt = connection
                .prepare(
                    "SELECT name FROM sqlite_master WHERE type='table'
                     AND name IN ('collection_paper_order', 'collection_sort_prefs')
                     ORDER BY name",
                )
                .unwrap();
            tables_stmt
                .query_map([], |row| row.get::<_, String>(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };
        assert_eq!(
            tables,
            vec![
                "collection_paper_order".to_string(),
                "collection_sort_prefs".to_string(),
            ]
        );
        let user_version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user_version");
        assert_eq!(
            user_version, user_version_before,
            "ensure must not bump schema_version"
        );
        let mode: String = connection
            .query_row(
                "SELECT sort_mode FROM collection_sort_prefs WHERE collection_id = ?1",
                params![collection_id],
                |row| row.get(0),
            )
            .expect("seeded sort mode survives the second ensure");
        assert_eq!(mode, "title");
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM collection_paper_order", [], |row| {
                row.get(0)
            })
            .expect("count");
        assert_eq!(count, 0);
    }

    fn revision_snapshot(root: &tempfile::TempDir) -> crate::library_workflow::LibraryRevisions {
        let connection =
            db::open(root.path().join(".read-desktop/workspace.sqlite3")).expect("open database");
        crate::library_workflow::library_revisions(&connection).expect("revisions")
    }

    fn collection_id(module: &PaperModule, relative_path: &str) -> String {
        let connection = module.connect().expect("connect");
        lookup_collection_id(&connection, relative_path)
    }

    fn import_one(
        module: &PaperModule,
        sources: &Path,
        collection: &str,
        name: &str,
    ) -> PaperProjection {
        let source = sources.join(format!("{name}.pdf"));
        fs::write(&source, pdf(name)).expect("source PDF");
        module
            .import_pdf(&source, Some(collection))
            .expect("import")
            .paper
            .expect("paper")
    }

    #[test]
    fn every_hub_visible_write_advances_the_global_revision() {
        let (root, _workspace, module) = ready_module();
        let sources = tempdir().expect("source directory");
        let first = import_one(&module, sources.path(), "Inbox", "first");
        let second = import_one(&module, sources.path(), "Inbox", "second");
        let first_id = first.id.clone();
        let second_id = second.id.clone();

        type WriteCase<'a> = (&'static str, Box<dyn Fn(&PaperModule) + 'a>);

        let cases: Vec<WriteCase<'_>> = vec![
            (
                "move_paper",
                Box::new(|module| {
                    module
                        .move_paper(&first_id, "Papers/Moved", "first.pdf", false)
                        .expect("move paper");
                }),
            ),
            (
                "move second paper",
                Box::new(|module| {
                    module
                        .move_paper(&second_id, "Papers/Moved", "second.pdf", false)
                        .expect("move second paper");
                }),
            ),
            (
                "rename_paper",
                Box::new(|module| {
                    module.rename_paper(&first_id, "renamed").expect("rename");
                }),
            ),
            (
                "reorder_collection_papers",
                Box::new(|module| {
                    let moved_id = collection_id(module, "Papers/Moved");
                    module
                        .reorder_collection_papers(
                            &moved_id,
                            &[second_id.clone(), first_id.clone()],
                        )
                        .expect("reorder");
                }),
            ),
            (
                "set_collection_sort_mode",
                Box::new(|module| {
                    let moved_id = collection_id(module, "Papers/Moved");
                    module
                        .set_collection_sort_mode(&moved_id, "manual")
                        .expect("sort mode");
                }),
            ),
            (
                "create_collection",
                Box::new(|module| {
                    module
                        .create_collection("Papers", Some("Nested"))
                        .expect("create collection");
                }),
            ),
            (
                "rename_collection",
                Box::new(|module| {
                    module
                        .rename_collection("Papers/Nested", "Renamed")
                        .expect("rename collection");
                }),
            ),
            (
                "move_collection",
                Box::new(|module| {
                    module
                        .move_collection("Papers/Renamed", "Papers/Moved")
                        .expect("move collection");
                }),
            ),
            (
                "trash_collection",
                Box::new(|module| {
                    module
                        .trash_collection("Papers/Moved/Renamed")
                        .expect("trash collection");
                }),
            ),
            (
                "trash_paper",
                Box::new(|module| {
                    module.trash_paper(&second_id).expect("trash paper");
                }),
            ),
            (
                "restore_paper",
                Box::new(|module| {
                    module.restore_paper(&second_id).expect("restore paper");
                }),
            ),
            (
                "apply_kind_change",
                Box::new(|module| {
                    module
                        .apply_kind_change(&first_id)
                        .expect("apply kind change");
                }),
            ),
            (
                "reconcile registers a dropped PDF",
                Box::new(|module| {
                    fs::write(module.root.join("Papers/Inbox/dropped.pdf"), pdf("dropped"))
                        .expect("drop pdf on disk");
                    module.reconcile().expect("reconcile");
                }),
            ),
            (
                "reconcile records a conflict",
                Box::new(|module| {
                    // The same bytes under a second path must surface as an open
                    // conflict, which is Hub-visible without a new paper.
                    fs::write(
                        module.root.join("Papers/Inbox/duplicate.pdf"),
                        pdf("dropped"),
                    )
                    .expect("duplicate pdf on disk");
                    module.reconcile().expect("reconcile conflict");
                }),
            ),
        ];

        for (label, run) in cases {
            let before = revision_snapshot(&root);
            run(&module);
            let after = revision_snapshot(&root);
            assert!(
                after.global > before.global,
                "{label} must advance the global library revision"
            );
        }
    }

    #[test]
    fn the_hub_cursor_is_the_global_revision_not_a_count_composition() {
        let (root, _workspace, module) = ready_module();
        let sources = tempdir().expect("source directory");
        let paper = import_one(&module, sources.path(), "Inbox", "paper");
        let before = module.list_library().expect("library before");
        assert_eq!(
            before.cursor,
            revision_snapshot(&root).global.to_string(),
            "the cursor must report the global revision"
        );

        // Replace the PDF bytes in place: papers, collections and conflicts all
        // keep their exact counts, which is what the old cursor could not see.
        fs::write(root.path().join(&paper.relative_path), pdf("replacement"))
            .expect("rewrite paper");
        let after = module.reconcile().expect("reconcile");

        assert_eq!(after.collections.len(), before.collections.len());
        assert_eq!(after.papers.len(), before.papers.len());
        assert_eq!(after.conflicts.len(), before.conflicts.len());
        assert_ne!(
            after.cursor, before.cursor,
            "a same-size replacement must move the Hub cursor"
        );
        assert_eq!(after.cursor, revision_snapshot(&root).global.to_string());
    }

    #[test]
    fn rejected_writes_and_viewport_saves_leave_the_revision_untouched() {
        let (root, _workspace, module) = ready_module();
        let sources = tempdir().expect("source directory");
        let paper = import_one(&module, sources.path(), "Inbox", "paper");
        let inbox_id = collection_id(&module, "Papers/Inbox");
        let before = revision_snapshot(&root);

        let rejected = module
            .reorder_collection_papers(&inbox_id, &["not-a-live-paper".to_string()])
            .expect_err("non-permutation must be rejected");
        assert!(rejected.contains("exact permutation"), "{rejected}");

        // Moving a paper onto its own path is a no-op before any write.
        module
            .move_paper(&paper.id, "Papers/Inbox", "paper.pdf", false)
            .expect("same path move");

        let mut state = ReadingState {
            paper_id: paper.id.clone(),
            revision_id: paper.revision_id.clone(),
            page_number: 3,
            page_offset: 0.5,
            zoom: 1.0,
            rotation: 0,
            right_tab: "discussion".to_string(),
            active_artifact_id: None,
            active_discussion_id: None,
            discussion_draft: String::new(),
            quote_basket: serde_json::json!([]),
            workspace_layout: "pdf_discussion".to_string(),
            active_outline_node_id: None,
            outline_view: "overview".to_string(),
            outline_inspector_width: 280,
            guide_layer_visible: true,
            long_pdf_warning_acked: false,
            updated_at: String::new(),
        };
        module.save_reading_state(&state).expect("save state");
        state.page_number = 9;
        module.save_reading_state(&state).expect("save again");

        let after = revision_snapshot(&root);
        assert_eq!(after.global, before.global);
        assert_eq!(after.domains, before.domains);
    }

    #[test]
    fn a_sort_write_bumps_only_the_sort_domain() {
        let (root, _workspace, module) = ready_module();
        let sources = tempdir().expect("source directory");
        let first = import_one(&module, sources.path(), "Inbox", "first");
        let second = import_one(&module, sources.path(), "Inbox", "second");
        let inbox_id = collection_id(&module, "Papers/Inbox");

        let before = revision_snapshot(&root);
        module
            .set_collection_sort_mode(&inbox_id, "manual")
            .expect("sort mode");
        let after_mode = revision_snapshot(&root);
        assert_eq!(
            after_mode.domain(LibraryDomain::Sort),
            before.domain(LibraryDomain::Sort) + 1
        );
        assert_eq!(
            after_mode.domain(LibraryDomain::Structure),
            before.domain(LibraryDomain::Structure),
            "a sort preference must not invalidate structure"
        );
        assert_eq!(after_mode.global, before.global + 1);

        module
            .reorder_collection_papers(&inbox_id, &[second.id.clone(), first.id.clone()])
            .expect("reorder");
        let after_reorder = revision_snapshot(&root);
        assert_eq!(
            after_reorder.domain(LibraryDomain::Sort),
            after_mode.domain(LibraryDomain::Sort) + 1
        );
        assert_eq!(
            after_reorder.domain(LibraryDomain::Structure),
            after_mode.domain(LibraryDomain::Structure)
        );

        // Re-submitting the identical permutation is the documented no-op guard,
        // so it must not pretend the Hub changed.
        module
            .reorder_collection_papers(&inbox_id, &[second.id, first.id])
            .expect("reorder again");
        assert_eq!(revision_snapshot(&root).global, after_reorder.global);
    }

    #[test]
    fn kind_change_bumps_artifacts_and_jobs_but_not_structure() {
        let (root, _workspace, module) = ready_module();
        let sources = tempdir().expect("source directory");
        let paper = import_one(&module, sources.path(), "Inbox", "paper");
        let before = revision_snapshot(&root);
        module.apply_kind_change(&paper.id).expect("kind change");
        let after = revision_snapshot(&root);
        assert_eq!(
            after.domain(LibraryDomain::Artifacts),
            before.domain(LibraryDomain::Artifacts) + 1
        );
        assert_eq!(
            after.domain(LibraryDomain::Jobs),
            before.domain(LibraryDomain::Jobs) + 1
        );
        assert_eq!(
            after.domain(LibraryDomain::Structure),
            before.domain(LibraryDomain::Structure)
        );
    }

    #[test]
    fn reorder_requires_exact_permutation_of_live_papers() {
        let (root, _workspace, module) = ready_module();
        let papers = import_three_textbook_pdfs(&root, &module, "Textbooks/ML", &["a", "b", "c"]);
        let connection = db::open(root.path().join(".read-desktop/workspace.sqlite3")).expect("db");
        let collection_id = lookup_collection_id(&connection, "Textbooks/ML");

        // Baseline: write the manual order b → a → c.
        let reordered = vec![
            papers[1].id.clone(),
            papers[0].id.clone(),
            papers[2].id.clone(),
        ];
        module
            .reorder_collection_papers(&collection_id, &reordered)
            .expect("reorder");
        let listed = module.list_collections_public().expect("list");
        let target = listed
            .iter()
            .find(|c| c.relative_path == "Textbooks/ML")
            .expect("collection");
        assert_eq!(target.sort_mode, "recent");
        assert_eq!(target.paper_order, reordered);
        assert_eq!(
            order_position(&connection, &collection_id, &papers[1].id),
            Some(0)
        );

        // Same order again is a successful no-op (no error).
        module
            .reorder_collection_papers(&collection_id, &reordered)
            .expect("no-op reorder");

        // Wrong cardinality is rejected.
        let err = module
            .reorder_collection_papers(&collection_id, &vec![papers[0].id.clone()])
            .expect_err("must reject missing");
        assert!(err.contains("paperIds must be exact permutation"));

        // Unknown id is rejected.
        let err = module
            .reorder_collection_papers(
                &collection_id,
                &vec![
                    papers[0].id.clone(),
                    papers[1].id.clone(),
                    "ghost".to_string(),
                ],
            )
            .expect_err("must reject foreign id");
        assert!(err.contains("paperIds must be exact permutation"));

        // Unknown collection is rejected.
        let err = module
            .reorder_collection_papers("ghost-collection", &reordered)
            .expect_err("must reject unknown collection");
        assert!(err.contains("Collection does not exist"));

        // Existing order is still intact after rejection.
        let listed = module.list_collections_public().expect("list");
        let target = listed
            .iter()
            .find(|c| c.relative_path == "Textbooks/ML")
            .expect("collection");
        assert_eq!(target.paper_order, reordered);
    }

    #[test]
    fn set_collection_sort_mode_persists_and_rejects_invalid_modes() {
        let (root, _workspace, module) = ready_module();
        let _papers = import_three_textbook_pdfs(&root, &module, "Textbooks/ML", &["a", "b", "c"]);
        let connection = db::open(root.path().join(".read-desktop/workspace.sqlite3")).expect("db");
        let collection_id = lookup_collection_id(&connection, "Textbooks/ML");

        module
            .set_collection_sort_mode(&collection_id, "chapter")
            .expect("set chapter");
        let listed = module.list_collections_public().expect("list");
        let target = listed
            .iter()
            .find(|c| c.relative_path == "Textbooks/ML")
            .expect("collection");
        assert_eq!(target.sort_mode, "chapter");

        // Upsert overwrites prior value.
        module
            .set_collection_sort_mode(&collection_id, "manual")
            .expect("set manual");
        let listed = module.list_collections_public().expect("list");
        let target = listed
            .iter()
            .find(|c| c.relative_path == "Textbooks/ML")
            .expect("collection");
        assert_eq!(target.sort_mode, "manual");

        let err = module
            .set_collection_sort_mode(&collection_id, "alphabetical")
            .expect_err("invalid mode");
        assert!(err.contains("Invalid sort mode"));

        let err = module
            .set_collection_sort_mode("ghost", "recent")
            .expect_err("unknown collection");
        assert!(err.contains("Collection does not exist"));
    }

    #[test]
    fn same_folder_rename_preserves_manual_order_row() {
        // This is the contract that the original implementation broke:
        // renaming a paper (F2) inside its current collection must NOT
        // drop the manual-order row.
        let (root, _workspace, module) = ready_module();
        let papers = import_three_textbook_pdfs(&root, &module, "Textbooks/ML", &["a", "b", "c"]);
        let connection = db::open(root.path().join(".read-desktop/workspace.sqlite3")).expect("db");
        let collection_id = lookup_collection_id(&connection, "Textbooks/ML");
        let reordered = vec![
            papers[2].id.clone(),
            papers[0].id.clone(),
            papers[1].id.clone(),
        ];
        module
            .reorder_collection_papers(&collection_id, &reordered)
            .expect("seed manual order");
        let positions_before = reordered
            .iter()
            .map(|id| order_position(&connection, &collection_id, id))
            .collect::<Vec<_>>();
        assert!(positions_before.iter().all(|p| p.is_some()));

        // Same-collection rename: stem change without collection change.
        module
            .rename_paper(&papers[1].id, "renamed-b")
            .expect("rename");

        let positions_after = reordered
            .iter()
            .map(|id| order_position(&connection, &collection_id, id))
            .collect::<Vec<_>>();
        assert_eq!(
            positions_after, positions_before,
            "same-folder rename must not touch manual order rows"
        );

        let listed = module.list_collections_public().expect("list");
        let target = listed
            .iter()
            .find(|c| c.relative_path == "Textbooks/ML")
            .expect("collection");
        assert_eq!(target.paper_order, reordered);
    }

    #[test]
    fn cross_folder_move_drops_manual_order_row_but_same_folder_keeps_it() {
        let (root, _workspace, module) = ready_module();
        let papers = import_three_textbook_pdfs(&root, &module, "Textbooks/ML", &["a", "b", "c"]);
        let connection = db::open(root.path().join(".read-desktop/workspace.sqlite3")).expect("db");
        let collection_id = lookup_collection_id(&connection, "Textbooks/ML");
        let reordered = vec![
            papers[0].id.clone(),
            papers[2].id.clone(),
            papers[1].id.clone(),
        ];
        module
            .reorder_collection_papers(&collection_id, &reordered)
            .expect("seed manual order");

        // Same-folder rename keeps the row.
        module
            .rename_paper(&papers[1].id, "still-b")
            .expect("rename in place");
        assert_eq!(
            order_position(&connection, &collection_id, &papers[1].id),
            Some(2)
        );

        // Move to another folder removes it (it now belongs to the new
        // collection; the destination has no manual order yet).
        // Cross-root (Textbooks → Papers) requires `confirm_kind_change`
        // because kind-sensitive artifact heads detach.
        module
            .move_paper(&papers[1].id, "Papers/Inbox", "moved.pdf", true)
            .expect("move cross folder");
        let connection = db::open(root.path().join(".read-desktop/workspace.sqlite3")).expect("db");
        assert_eq!(
            order_position(&connection, &collection_id, &papers[1].id),
            None
        );
    }

    #[test]
    fn trash_paper_removes_manual_order_row_and_restore_does_not_resurrect_it() {
        let (root, _workspace, module) = ready_module();
        let papers = import_three_textbook_pdfs(&root, &module, "Textbooks/ML", &["a", "b", "c"]);
        let connection = db::open(root.path().join(".read-desktop/workspace.sqlite3")).expect("db");
        let collection_id = lookup_collection_id(&connection, "Textbooks/ML");
        let reordered = vec![
            papers[2].id.clone(),
            papers[1].id.clone(),
            papers[0].id.clone(),
        ];
        module
            .reorder_collection_papers(&collection_id, &reordered)
            .expect("seed manual order");
        assert_eq!(
            order_position(&connection, &collection_id, &papers[1].id),
            Some(1)
        );

        module.trash_paper(&papers[1].id).expect("trash");
        let connection = db::open(root.path().join(".read-desktop/workspace.sqlite3")).expect("db");
        assert_eq!(
            order_position(&connection, &collection_id, &papers[1].id),
            None
        );

        module.restore_paper(&papers[1].id).expect("restore");
        let connection = db::open(root.path().join(".read-desktop/workspace.sqlite3")).expect("db");
        assert_eq!(
            order_position(&connection, &collection_id, &papers[1].id),
            None,
            "restore must not silently reattach a stale order row"
        );
    }

    #[test]
    fn streaming_hash_and_copy_preserve_large_pdf_bytes() {
        let source_root = tempdir().expect("source directory");
        let target_root = tempdir().expect("target directory");
        let source = source_root.path().join("large.pdf");
        let target = target_root.path().join("copied.pdf");

        let mut bytes = b"%PDF-1.7\n".to_vec();
        bytes.extend((0..(2 * 1024 * 1024 + 137)).map(|index| (index % 251) as u8));
        bytes.extend_from_slice(b"\n%%EOF");
        fs::write(&source, &bytes).expect("write source PDF");

        copy_file_streaming(&source, &target).expect("streaming copy");
        assert_eq!(fs::read(&target).expect("read copied PDF"), bytes);

        let expected = format!("{:x}", Sha256::digest(&bytes));
        assert_eq!(sha256_file(&source).expect("streaming hash"), expected);
        let (digest, size, pages) = inspect_pdf(&source).expect("streaming inspect");
        assert_eq!(digest, expected);
        assert_eq!(size, bytes.len() as u64);
        assert_eq!(pages, 1, "a PDF without page markers still has one page");
    }

    #[test]
    fn page_marker_scanner_handles_chunk_boundaries_and_pages_plural() {
        let mut tail = Vec::new();
        assert_eq!(count_page_markers_in_chunks(&mut tail, b"/Type /Page"), 0);
        assert_eq!(count_page_markers_in_chunks(&mut tail, b"s"), 0);
        assert_eq!(flush_page_marker_tail(&mut tail), 0);

        let mut tail = Vec::new();
        assert_eq!(count_page_markers_in_chunks(&mut tail, b"/Type /Page"), 0);
        assert_eq!(count_page_markers_in_chunks(&mut tail, b" "), 1);
        assert_eq!(flush_page_marker_tail(&mut tail), 0);

        let mut tail = Vec::new();
        assert_eq!(
            count_page_markers_in_chunks(&mut tail, b"prefix /Type /Page"),
            0
        );
        assert_eq!(flush_page_marker_tail(&mut tail), 1);
    }

    #[test]
    fn workspace_activation_purges_only_expired_trash_and_orphan_parts() {
        let root = tempdir().expect("temporary Workspace");
        let workspace = WorkspaceModule::new();
        workspace.open(root.path()).expect("initialize Workspace");
        let source_root = tempdir().expect("source directory");
        let source = source_root.path().join("paper.pdf");
        fs::write(&source, pdf("cleanup")).expect("source PDF");
        let module = PaperModule::open(root.path()).expect("open PaperModule");
        let paper = module
            .import_pdf(&source, Some("Inbox"))
            .expect("import")
            .paper
            .expect("paper");
        module.trash_paper(&paper.id).expect("trash paper");

        let connection = db::open(root.path().join(".read-desktop/workspace.sqlite3"))
            .expect("database connection");
        let (trash_id, trash_relative): (String, String) = connection
            .query_row(
                "SELECT id, trash_relative_path FROM trash_entries WHERE paper_id = ?1",
                rusqlite::params![paper.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("trash row");
        connection
            .execute(
                "UPDATE trash_entries SET purge_after = '2000-01-01T00:00:00Z' WHERE id = ?1",
                rusqlite::params![trash_id],
            )
            .expect("expire trash row");

        let live_id = "live-part-operation";
        connection
            .execute(
                "INSERT INTO operation_journal(
                   id, operation_kind, entity_kind, entity_id, state, payload_json, created_at
                 ) VALUES (?1, 'import', 'paper', ?1, 'prepared', '{}', '2026-08-18T00:00:00Z')",
                rusqlite::params![live_id],
            )
            .expect("prepared operation");
        drop(connection);

        let orphan = root
            .path()
            .join("Papers/Inbox/orphan.pdf.dead-operation.part");
        let live = root
            .path()
            .join(format!("Papers/Inbox/live.pdf.{live_id}.part"));
        fs::write(&orphan, b"orphan").expect("orphan staging");
        fs::write(&live, b"live").expect("prepared staging");
        let trash_path = root.path().join(".read-desktop").join(&trash_relative);
        assert!(trash_path.is_file());

        drop(module);
        let reopened = PaperModule::open(root.path()).expect("reopen PaperModule");
        assert!(reopened.list_trash().expect("list trash").is_empty());
        assert!(!trash_path.exists(), "expired trash file should be purged");
        assert!(!orphan.exists(), "unconfirmed staging should be removed");
        assert!(live.exists(), "prepared staging must be retained");
    }

    #[test]
    fn imports_textbook_pdf_under_textbooks_root() {
        let (root, _workspace, module) = ready_module();
        assert!(root.path().join("Textbooks").is_dir());
        let source_root = tempdir().expect("source directory");
        let source = source_root.path().join("ch3.pdf");
        fs::write(&source, pdf("chapter")).expect("source PDF");

        let imported = module
            .import_pdf(&source, Some("Textbooks/CLRS"))
            .expect("import textbook");
        let paper = imported.paper.expect("published textbook");
        assert_eq!(paper.collection_path, "Textbooks/CLRS");
        assert_eq!(paper.relative_path, "Textbooks/CLRS/ch3.pdf");
        assert!(root.path().join("Textbooks/CLRS/ch3.pdf").is_file());
        assert_eq!(
            document_kind_from_relative(&paper.relative_path)
                .expect("kind")
                .as_str(),
            "textbook"
        );
    }

    #[test]
    fn normalizes_legacy_relative_paths_during_schema_initialization() {
        let root = tempdir().expect("temporary Workspace");
        {
            let workspace = WorkspaceModule::new();
            workspace.open(root.path()).expect("initialize Workspace");
        }
        let database = root.path().join(".read-desktop/workspace.sqlite3");
        let connection = db::open(&database).expect("database");
        let timestamp = "2026-08-17T00:00:00Z";
        connection
            .execute(
                "UPDATE collections SET relative_path = '' WHERE id = ?1",
                rusqlite::params![PAPERS_ROOT_COLLECTION_ID],
            )
            .expect("legacy root path");
        connection
            .execute(
                "INSERT INTO collections(id, parent_id, name, relative_path, created_at, updated_at)
                 VALUES ('c-inbox', ?1, 'Inbox', 'Inbox', ?2, ?2)",
                rusqlite::params![PAPERS_ROOT_COLLECTION_ID, timestamp],
            )
            .expect("legacy inbox collection");
        connection
            .execute(
                "INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                 VALUES ('p1', 'c-inbox', 'paper.pdf', 'Inbox/paper.pdf', ?1, ?1)",
                rusqlite::params![timestamp],
            )
            .expect("legacy paper");
        connection
            .execute(
                "INSERT INTO document_revisions(
                   id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at
                 ) VALUES ('r1', 'p1', 'abc', 12, 1, 'Inbox/paper.pdf', ?1)",
                rusqlite::params![timestamp],
            )
            .expect("legacy revision");
        crate::v2_workspace::migrate_library_roots(&connection).expect("normalize legacy paths");
        let paper_path: String = connection
            .query_row(
                "SELECT relative_path FROM papers WHERE id = 'p1'",
                [],
                |row| row.get(0),
            )
            .expect("paper path");
        assert_eq!(paper_path, "Papers/Inbox/paper.pdf");
        let inbox_path: String = connection
            .query_row(
                "SELECT relative_path FROM collections WHERE id = 'c-inbox'",
                [],
                |row| row.get(0),
            )
            .expect("inbox path");
        assert_eq!(inbox_path, "Papers/Inbox");
        let revision_path: String = connection
            .query_row(
                "SELECT source_relative_path FROM document_revisions WHERE id = 'r1'",
                [],
                |row| row.get(0),
            )
            .expect("revision path");
        assert_eq!(revision_path, "Papers/Inbox/paper.pdf");
        let textbooks: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM collections WHERE id = ?1",
                rusqlite::params![TEXTBOOKS_ROOT_COLLECTION_ID],
                |row| row.get(0),
            )
            .expect("textbooks root");
        assert_eq!(textbooks, 1);
    }
}
