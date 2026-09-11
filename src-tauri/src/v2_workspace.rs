use crate::db;
use crate::library_paths::{
    prefix_legacy_relative, PAPERS_DIR, PAPERS_ROOT_COLLECTION_ID, TEXTBOOKS_DIR,
    TEXTBOOKS_ROOT_COLLECTION_ID,
};
use chrono::Utc;
use fs2::FileExt;
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyResetPreview {
    pub root_path: PathBuf,
    pub legacy_database_path: PathBuf,
    pub legacy_database_exists: bool,
    pub legacy_library_path: PathBuf,
    pub legacy_library_exists: bool,
    pub file_count: u64,
    pub pdf_count: u64,
    pub total_bytes: u64,
    pub backup_path: PathBuf,
    pub warnings: Vec<String>,
    pub can_proceed: bool,
    pub preview_digest: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyResetResult {
    pub projection: WorkspaceProjection,
    pub backup_path: PathBuf,
    pub file_count: u64,
    pub pdf_count: u64,
    pub total_bytes: u64,
}

pub const WORKSPACE_FORMAT_VERSION: i64 = 2;
const SQLITE_V6_SCHEMA_VERSION: i64 = 6;
const SQLITE_V7_SCHEMA_VERSION: i64 = 7;
pub(crate) const SQLITE_SCHEMA_VERSION: i64 = 8;
/// D-063 §9：Schema 8 新增的持久化领域，validator 与迁移共用这一份清单。
const V8_DOMAINS: [&str; 8] = [
    "structure",
    "tags",
    "lifecycle",
    "engagement",
    "artifacts",
    "jobs",
    "smart_collections",
    "sort",
];
pub const OUTLINE_EPOCH: i64 = 3;
pub const OUTLINE_EPOCH_KEY: &str = "outline_epoch";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceStatus {
    Ready,
    ResetRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceProjection {
    pub status: WorkspaceStatus,
    pub root_path: PathBuf,
    pub papers_path: PathBuf,
    pub textbooks_path: PathBuf,
    pub database_path: PathBuf,
}

#[derive(Debug)]
struct ActiveWorkspaceLock {
    root: PathBuf,
    // The open handle must stay alive for the lifetime of the active workspace;
    // dropping it would release the OS-level exclusive lock.
    _file: File,
}

#[derive(Debug, Default)]
pub struct WorkspaceModule {
    active_lock: Mutex<Option<ActiveWorkspaceLock>>,
}

impl WorkspaceModule {
    #[cfg(test)]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&self, root: &Path) -> Result<WorkspaceProjection, String> {
        let mut active_lock = self
            .active_lock
            .lock()
            .map_err(|_| "Workspace lock state is unavailable".to_string())?;
        if active_lock
            .as_ref()
            .is_some_and(|active| active.root == root)
        {
            return Ok(WorkspaceProjection {
                status: WorkspaceStatus::Ready,
                root_path: root.to_path_buf(),
                papers_path: root.join(PAPERS_DIR),
                textbooks_path: root.join(TEXTBOOKS_DIR),
                database_path: root.join(".read-desktop").join("workspace.sqlite3"),
            });
        }

        if root.exists() && !root.is_dir() {
            return Err("Workspace path is not a directory".to_string());
        }
        fs::create_dir_all(root).map_err(|error| format!("Unable to create Workspace: {error}"))?;

        let papers_path = root.join(PAPERS_DIR);
        let textbooks_path = root.join(TEXTBOOKS_DIR);
        let internal_path = root.join(".read-desktop");
        let database_path = internal_path.join("workspace.sqlite3");
        let legacy = !database_path.exists()
            && (root.join("workspace.sqlite3").exists() || root.join("library").exists());

        if legacy {
            return Ok(WorkspaceProjection {
                status: WorkspaceStatus::ResetRequired,
                root_path: root.to_path_buf(),
                papers_path,
                textbooks_path,
                database_path,
            });
        }

        fs::create_dir_all(&papers_path)
            .map_err(|error| format!("Unable to create Papers directory: {error}"))?;
        fs::create_dir_all(&textbooks_path)
            .map_err(|error| format!("Unable to create Textbooks directory: {error}"))?;
        for directory in ["artifacts", "cache", "logs", "trash"] {
            fs::create_dir_all(internal_path.join(directory))
                .map_err(|error| format!("Unable to create {directory} directory: {error}"))?;
        }
        let lock_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(internal_path.join("workspace.lock"))
            .map_err(|error| format!("Unable to open Workspace lock: {error}"))?;
        lock_file
            .try_lock_exclusive()
            .map_err(|_| "Workspace is already open in another process".to_string())?;

        initialize_database(&database_path)?;
        *active_lock = Some(ActiveWorkspaceLock {
            root: root.to_path_buf(),
            _file: lock_file,
        });

        Ok(WorkspaceProjection {
            status: WorkspaceStatus::Ready,
            root_path: root.to_path_buf(),
            papers_path,
            textbooks_path,
            database_path,
        })
    }

    pub fn inspect_legacy_reset(&self, root: &Path) -> Result<LegacyResetPreview, String> {
        let root = normalize_workspace_root(root)?;
        validate_workspace_root_safety(&root)?;
        let legacy_database = root.join("workspace.sqlite3");
        let legacy_library = root.join("library");
        let legacy_database_exists = inspect_legacy_file(&legacy_database, "Legacy database")?;
        let legacy_library_exists = inspect_legacy_directory(&legacy_library, "Legacy library")?;
        if !legacy_database_exists && !legacy_library_exists {
            return Ok(LegacyResetPreview {
                root_path: root.clone(),
                legacy_database_path: legacy_database,
                legacy_database_exists: false,
                legacy_library_path: legacy_library,
                legacy_library_exists: false,
                file_count: 0,
                pdf_count: 0,
                total_bytes: 0,
                backup_path: backup_path_for(&root, "no-legacy-data"),
                warnings: vec!["No legacy data found".to_string()],
                can_proceed: false,
                preview_digest: String::new(),
            });
        }
        let inventory = legacy_inventory(
            &root,
            &legacy_database,
            legacy_database_exists,
            &legacy_library,
            legacy_library_exists,
        )?;
        let digest = inventory.digest();
        let backup_path = backup_path_for(&root, &digest);
        Ok(LegacyResetPreview {
            root_path: root.clone(),
            legacy_database_path: legacy_database,
            legacy_database_exists,
            legacy_library_path: legacy_library,
            legacy_library_exists,
            file_count: inventory.file_count,
            pdf_count: inventory.pdf_count,
            total_bytes: inventory.total_bytes,
            backup_path,
            warnings: Vec::new(),
            can_proceed: true,
            preview_digest: digest,
        })
    }

    pub fn execute_legacy_reset(
        &self,
        root: &Path,
        expected_digest: &str,
    ) -> Result<LegacyResetResult, String> {
        let root = normalize_workspace_root(root)?;
        validate_workspace_root_safety(&root)?;
        // Poisoning means the active state is unknown, so reset must fail closed.
        let guard = self.active_lock.lock().map_err(|_| {
            "Workspace lock state is unavailable; reset was not started".to_string()
        })?;
        if guard.is_some() {
            return Err(
                "Workspace is currently open with active tasks; please wait or close before reset"
                    .to_string(),
            );
        }
        drop(guard);
        // Gate: try to ensure workspace.lock is not held by another process
        let internal_lock = root.join(".read-desktop").join("workspace.lock");
        if internal_lock.exists() {
            let metadata = fs::symlink_metadata(&internal_lock)
                .map_err(|error| format!("Unable to inspect Workspace lock: {error}"))?;
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err("Workspace lock path is not a regular file".to_string());
            }
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&internal_lock)
                .map_err(|error| format!("Unable to open Workspace lock: {error}"))?;
            file.try_lock_exclusive()
                .map_err(|_| "Workspace is locked by another process".to_string())?;
            FileExt::unlock(&file)
                .map_err(|error| format!("Unable to release Workspace lock probe: {error}"))?;
        }
        let preview = self.inspect_legacy_reset(&root)?;
        if preview.preview_digest != expected_digest {
            return Err("Legacy data changed after preview; please re-inspect".to_string());
        }
        if !preview.can_proceed {
            return Err("Nothing to reset".to_string());
        }
        let backup_dir = preview.backup_path.clone();
        let backup_root = backup_dir
            .parent()
            .ok_or_else(|| "Backup path has no parent directory".to_string())?;
        fs::create_dir_all(backup_root)
            .map_err(|e| format!("Unable to create backup root: {e}"))?;
        fs::create_dir(&backup_dir)
            .map_err(|e| format!("Unable to reserve exact backup directory: {e}"))?;
        let mut moved: Vec<(PathBuf, PathBuf)> = Vec::new();
        let mut manifest_entries: Vec<serde_json::Value> = Vec::new();
        // Move legacy database set with rollback on partial failure
        if preview.legacy_database_exists {
            let src = preview.legacy_database_path.clone();
            let dst = backup_dir.join("workspace.sqlite3");
            if let Err(e) = move_path(&src, &dst, &mut moved) {
                return Err(rollback_after_failure(&backup_dir, &mut moved, &e));
            }
            manifest_entries.push(serde_json::json!({"kind":"database","from": src.to_string_lossy(), "to": dst.to_string_lossy()}));
            for suffix in ["-wal", "-shm", "-journal"] {
                let s = sqlite_sidecar_path(&src, suffix);
                if s.is_file() {
                    let d = sqlite_sidecar_path(&dst, suffix);
                    if let Err(e) = move_path(&s, &d, &mut moved) {
                        return Err(rollback_after_failure(&backup_dir, &mut moved, &e));
                    }
                    manifest_entries.push(serde_json::json!({"kind":"sidecar","from": s.to_string_lossy(), "to": d.to_string_lossy()}));
                }
            }
        }
        if preview.legacy_library_exists {
            let src = preview.legacy_library_path.clone();
            let dst = backup_dir.join("library");
            if let Err(e) = move_path(&src, &dst, &mut moved) {
                return Err(rollback_after_failure(&backup_dir, &mut moved, &e));
            }
            manifest_entries.push(serde_json::json!({"kind":"library","from": src.to_string_lossy(), "to": dst.to_string_lossy()}));
        }
        // Write manifest
        let manifest_path = backup_dir.join("manifest.json");
        let manifest = serde_json::json!({
            "root": root.to_string_lossy(),
            "createdAt": Utc::now().to_rfc3339(),
            "previewDigest": expected_digest,
            "entries": manifest_entries,
            "fileCount": preview.file_count,
            "pdfCount": preview.pdf_count,
            "totalBytes": preview.total_bytes,
        });
        let manifest_json = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("Unable to serialize reset manifest: {e}"))?;
        if let Err(e) = fs::write(&manifest_path, manifest_json) {
            return Err(rollback_after_failure(
                &backup_dir,
                &mut moved,
                &format!("Unable to write manifest: {e}"),
            ));
        }
        // Try to initialize V2 and verify with quick_check
        match self.open(&root) {
            Ok(projection) => {
                let db_path = root.join(".read-desktop").join("workspace.sqlite3");
                let validation = db::open(&db_path)
                    .map_err(|error| format!("Unable to open V2 database for validation: {error}"))
                    .and_then(|connection| {
                        let quick_check: String = connection
                            .query_row("PRAGMA quick_check", [], |row| row.get(0))
                            .map_err(|error| format!("Unable to run V2 quick_check: {error}"))?;
                        if quick_check.eq_ignore_ascii_case("ok") {
                            Ok(())
                        } else {
                            Err(format!("V2 quick_check failed: {quick_check}"))
                        }
                    });
                if let Err(error) = validation {
                    let release_error = self.release_active_lock(&root).err();
                    let archive_error = archive_failed_v2(&root, &backup_dir).err();
                    let rollback_error = rollback_moves(&mut moved).err();
                    let combined = reset_failure_message(
                        &backup_dir,
                        &error,
                        release_error,
                        archive_error,
                        rollback_error,
                    );
                    write_rollback_record(&backup_dir, &combined);
                    return Err(combined);
                }
                Ok(LegacyResetResult {
                    projection,
                    backup_path: backup_dir,
                    file_count: preview.file_count,
                    pdf_count: preview.pdf_count,
                    total_bytes: preview.total_bytes,
                })
            }
            Err(e) => {
                let archive_error = archive_failed_v2(&root, &backup_dir).err();
                let rollback_error = rollback_moves(&mut moved).err();
                let combined =
                    reset_failure_message(&backup_dir, &e, None, archive_error, rollback_error);
                write_rollback_record(&backup_dir, &combined);
                Err(combined)
            }
        }
    }

    fn release_active_lock(&self, root: &Path) -> Result<(), String> {
        let mut active_lock = self
            .active_lock
            .lock()
            .map_err(|_| "Workspace lock state is unavailable during rollback".to_string())?;
        if active_lock
            .as_ref()
            .is_some_and(|active| active.root == root)
        {
            *active_lock = None;
        }
        Ok(())
    }
}

fn initialize_database(path: &Path) -> Result<(), String> {
    let is_new = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => metadata.len() == 0,
        Ok(_) => return Err("Workspace database path is not a regular file".to_string()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(error) => {
            return Err(format!(
                "Unable to inspect Workspace database before initialization: {error}"
            ));
        }
    };
    if is_new {
        if sqlite_sidecar_exists(path) {
            return Err("Workspace database is missing but SQLite sidecars remain".to_string());
        }
        initialize_v6_database(path)?;
        migrate_database_to_schema_7(path, false)?;
        let connection = db::open(path).map_err(|error| error.to_string())?;
        ensure_hub_sort_tables(&connection)?;
        drop(connection);
        return migrate_database_to_schema_8(path, false);
    }

    match inspect_database_schema_state(path)? {
        ExistingDatabaseSchema::V8 => {
            let connection = db::open(path).map_err(|error| error.to_string())?;
            ensure_hub_sort_tables(&connection)?;
            crate::guide_module::ensure_extension_tables(&connection)?;
            Ok(())
        }
        ExistingDatabaseSchema::V7 => {
            let connection = db::open(path).map_err(|error| error.to_string())?;
            ensure_hub_sort_tables(&connection)?;
            drop(connection);
            migrate_database_to_schema_8(path, true)
        }
        ExistingDatabaseSchema::V6 => {
            migrate_database_to_schema_7(path, true)?;
            let connection = db::open(path).map_err(|error| error.to_string())?;
            ensure_hub_sort_tables(&connection)?;
            drop(connection);
            migrate_database_to_schema_8(path, true)
        }
        ExistingDatabaseSchema::Legacy => {
            initialize_v6_database(path)?;
            let result = match inspect_database_schema_state(path)? {
                ExistingDatabaseSchema::V6 => migrate_database_to_schema_7(path, true),
                state => Err(format!(
                    "Legacy Workspace migration did not reach exact schema 6: {state:?}"
                )),
            };
            result?;
            let connection = db::open(path).map_err(|error| error.to_string())?;
            ensure_hub_sort_tables(&connection)?;
            drop(connection);
            migrate_database_to_schema_8(path, true)
        }
    }
}

fn migrate_database_to_schema_8(path: &Path, backup_existing_v7: bool) -> Result<(), String> {
    if backup_existing_v7 {
        db::checkpoint_close_copy_database(path, "pre-schema-8").map_err(|error| {
            format!("Unable to create the required pre-schema-8 backup: {error}")
        })?;
    }

    let mut connection = db::open(path).map_err(|error| error.to_string())?;
    migrate_v7_to_v8(&mut connection)?;
    crate::guide_module::ensure_extension_tables(&connection)
}

fn migrate_database_to_schema_7(path: &Path, backup_existing_v6: bool) -> Result<(), String> {
    if backup_existing_v6 {
        db::checkpoint_close_copy_database(path, "pre-schema-7").map_err(|error| {
            format!("Unable to create the required pre-schema-7 backup: {error}")
        })?;
    }

    let mut connection = db::open(path).map_err(|error| error.to_string())?;
    migrate_v6_to_v7(&mut connection)
}

fn initialize_v6_database(path: &Path) -> Result<(), String> {
    if path.exists() {
        if !path.is_file() {
            return Err("Workspace database path is not a file".to_string());
        }
        if existing_database_needs_backup(path)? {
            backup_database_files(path)?;
        }
    } else if sqlite_sidecar_exists(path) {
        // A missing main file with a leftover WAL/SHM is not a recoverable V2
        // database. Preserve the sidecars before creating a fresh database.
        backup_database_files(path)?;
    }
    let connection = db::open_wal(path).map_err(|error| error.to_string())?;
    let user_version_before_initialization: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             CREATE TABLE IF NOT EXISTS schema_meta (
               key TEXT PRIMARY KEY,
               value TEXT NOT NULL
             );",
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            r#"
            BEGIN IMMEDIATE;
            CREATE TABLE IF NOT EXISTS collections (
              id TEXT PRIMARY KEY,
              parent_id TEXT REFERENCES collections(id) ON DELETE RESTRICT,
              name TEXT NOT NULL,
              relative_path TEXT NOT NULL UNIQUE,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS papers (
              id TEXT PRIMARY KEY,
              collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE RESTRICT,
              file_name TEXT NOT NULL,
              relative_path TEXT NOT NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              deleted_at TEXT
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_papers_active_relative_path
              ON papers(relative_path) WHERE deleted_at IS NULL;
            CREATE TABLE IF NOT EXISTS document_revisions (
              id TEXT PRIMARY KEY,
              paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
              sha256 TEXT NOT NULL,
              byte_size INTEGER NOT NULL CHECK (byte_size >= 0),
              page_count INTEGER,
              source_relative_path TEXT NOT NULL,
              created_at TEXT NOT NULL,
              UNIQUE(paper_id, sha256)
            );
            CREATE TABLE IF NOT EXISTS paper_heads (
              paper_id TEXT PRIMARY KEY REFERENCES papers(id) ON DELETE CASCADE,
              revision_id TEXT NOT NULL UNIQUE REFERENCES document_revisions(id) ON DELETE RESTRICT,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS paper_metadata (
              revision_id TEXT PRIMARY KEY REFERENCES document_revisions(id) ON DELETE CASCADE,
              title TEXT NOT NULL,
              authors_json TEXT NOT NULL DEFAULT '[]',
              publication_year INTEGER,
              venue TEXT,
              doi TEXT,
              abstract_text TEXT,
              metadata_json TEXT NOT NULL DEFAULT '{}',
              source TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS tags (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL UNIQUE,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS paper_tags (
              paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
              tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
              PRIMARY KEY (paper_id, tag_id)
            );
            CREATE TABLE IF NOT EXISTS reading_states (
              paper_id TEXT PRIMARY KEY REFERENCES papers(id) ON DELETE CASCADE,
              revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
              page_number INTEGER NOT NULL DEFAULT 1 CHECK (page_number >= 1),
              page_offset REAL NOT NULL DEFAULT 0,
              zoom REAL NOT NULL DEFAULT 1 CHECK (zoom > 0),
              rotation INTEGER NOT NULL DEFAULT 0,
              right_tab TEXT NOT NULL DEFAULT 'discussion',
              active_artifact_id TEXT,
              active_discussion_id TEXT,
              discussion_draft TEXT NOT NULL DEFAULT '',
              quote_basket_json TEXT NOT NULL DEFAULT '[]',
              workspace_layout TEXT NOT NULL DEFAULT 'pdf_discussion',
              active_outline_node_id TEXT,
              outline_view TEXT NOT NULL DEFAULT 'overview',
              outline_inspector_width INTEGER NOT NULL DEFAULT 280,
              guide_layer_visible INTEGER NOT NULL DEFAULT 1 CHECK (guide_layer_visible IN (0, 1)),
              long_pdf_warning_acked INTEGER NOT NULL DEFAULT 0 CHECK (long_pdf_warning_acked IN (0, 1)),
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS provider_nodes (
              id TEXT PRIMARY KEY,
              provider TEXT NOT NULL,
              model TEXT NOT NULL,
              context_epoch TEXT NOT NULL,
              provider_node_id TEXT,
              parent_id TEXT REFERENCES provider_nodes(id) ON DELETE SET NULL,
              state TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS context_roots (
              id TEXT PRIMARY KEY,
              revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
              provider TEXT NOT NULL,
              model TEXT NOT NULL,
              context_epoch TEXT NOT NULL,
              provider_file_id TEXT,
              provider_node_id TEXT,
              state TEXT NOT NULL,
              created_at TEXT NOT NULL,
              invalidated_at TEXT,
              UNIQUE(revision_id, provider, model, context_epoch)
            );
            CREATE TABLE IF NOT EXISTS ocr_revisions (
              id TEXT PRIMARY KEY,
              revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
              status TEXT NOT NULL,
              provider TEXT NOT NULL,
              model TEXT NOT NULL,
              raw_staging_path TEXT,
              created_at TEXT NOT NULL,
              published_at TEXT
            );
            CREATE TABLE IF NOT EXISTS ocr_pages (
              id TEXT PRIMARY KEY,
              ocr_revision_id TEXT NOT NULL REFERENCES ocr_revisions(id) ON DELETE CASCADE,
              page_number INTEGER NOT NULL CHECK (page_number >= 1),
              width INTEGER,
              height INTEGER,
              markdown TEXT,
              UNIQUE(ocr_revision_id, page_number)
            );
            CREATE TABLE IF NOT EXISTS ocr_blocks (
              id TEXT PRIMARY KEY,
              ocr_page_id TEXT NOT NULL REFERENCES ocr_pages(id) ON DELETE CASCADE,
              block_index INTEGER NOT NULL CHECK (block_index >= 0),
              block_type TEXT NOT NULL,
              text_content TEXT NOT NULL,
              content_digest TEXT NOT NULL,
              x0 INTEGER NOT NULL CHECK (x0 BETWEEN 0 AND 1000),
              y0 INTEGER NOT NULL CHECK (y0 BETWEEN 0 AND 1000),
              x1 INTEGER NOT NULL CHECK (x1 BETWEEN 0 AND 1000),
              y1 INTEGER NOT NULL CHECK (y1 BETWEEN 0 AND 1000),
              CHECK (x0 <= x1 AND y0 <= y1),
              UNIQUE(ocr_page_id, block_index)
            );
            CREATE TABLE IF NOT EXISTS artifacts (
              id TEXT PRIMARY KEY,
              paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
              revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
              ocr_revision_id TEXT REFERENCES ocr_revisions(id) ON DELETE SET NULL,
              kind TEXT NOT NULL,
              object_key TEXT NOT NULL DEFAULT '',
              version INTEGER NOT NULL CHECK (version >= 1),
              status TEXT NOT NULL,
              content_json TEXT NOT NULL,
              evidence_json TEXT NOT NULL DEFAULT '[]',
              dependency_snapshot_json TEXT NOT NULL DEFAULT '{}',
              provider_node_id TEXT REFERENCES provider_nodes(id) ON DELETE SET NULL,
              created_at TEXT NOT NULL,
              superseded_at TEXT,
              UNIQUE(paper_id, kind, object_key, version)
            );
            CREATE TABLE IF NOT EXISTS artifact_heads (
              paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
              kind TEXT NOT NULL,
              object_key TEXT NOT NULL DEFAULT '',
              artifact_id TEXT NOT NULL UNIQUE REFERENCES artifacts(id) ON DELETE RESTRICT,
              updated_at TEXT NOT NULL,
              PRIMARY KEY (paper_id, kind, object_key)
            );
            CREATE TABLE IF NOT EXISTS term_overrides (
              artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
              term_key TEXT NOT NULL,
              value_json TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              PRIMARY KEY (artifact_id, term_key)
            );
            CREATE TABLE IF NOT EXISTS symbol_overrides (
              artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
              symbol_key TEXT NOT NULL,
              value_json TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              PRIMARY KEY (artifact_id, symbol_key)
            );
            CREATE TABLE IF NOT EXISTS discussions (
              id TEXT PRIMARY KEY,
              paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
              revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
              title TEXT NOT NULL,
              status TEXT NOT NULL,
              context_root_id TEXT REFERENCES context_roots(id) ON DELETE SET NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS messages (
              id TEXT PRIMARY KEY,
              discussion_id TEXT NOT NULL REFERENCES discussions(id) ON DELETE CASCADE,
              parent_id TEXT REFERENCES messages(id) ON DELETE SET NULL,
              role TEXT NOT NULL,
              content TEXT NOT NULL,
              status TEXT NOT NULL,
              citations_json TEXT NOT NULL DEFAULT '[]',
              provider_node_id TEXT REFERENCES provider_nodes(id) ON DELETE SET NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS message_contexts (
              message_id TEXT PRIMARY KEY REFERENCES messages(id) ON DELETE CASCADE,
              block_quotes_json TEXT NOT NULL DEFAULT '[]'
            );
            CREATE TABLE IF NOT EXISTS discussion_heads (
              discussion_id TEXT PRIMARY KEY REFERENCES discussions(id) ON DELETE CASCADE,
              message_id TEXT REFERENCES messages(id) ON DELETE SET NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS lens_qa (
              id TEXT PRIMARY KEY,
              lens_artifact_id TEXT NOT NULL REFERENCES artifacts(id) ON DELETE CASCADE,
              parent_id TEXT REFERENCES lens_qa(id) ON DELETE SET NULL,
              role TEXT NOT NULL,
              content TEXT NOT NULL,
              status TEXT NOT NULL,
              provider_node_id TEXT REFERENCES provider_nodes(id) ON DELETE SET NULL,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS jobs (
              id TEXT PRIMARY KEY,
              kind TEXT NOT NULL,
              provider TEXT,
              paper_id TEXT REFERENCES papers(id) ON DELETE CASCADE,
              revision_id TEXT REFERENCES document_revisions(id) ON DELETE CASCADE,
              root_key TEXT,
              artifact_key TEXT,
              dedupe_key TEXT NOT NULL,
              state TEXT NOT NULL,
              stage TEXT NOT NULL,
              provider_committed INTEGER NOT NULL DEFAULT 0 CHECK (provider_committed IN (0, 1)),
              priority INTEGER NOT NULL DEFAULT 0,
              payload_json TEXT NOT NULL DEFAULT '{}',
              last_error TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_jobs_active_dedupe
              ON jobs(dedupe_key)
              WHERE state IN ('queued', 'running', 'paused');
            CREATE INDEX IF NOT EXISTS idx_jobs_claim
              ON jobs(state, priority DESC, created_at);
            CREATE TABLE IF NOT EXISTS job_attempts (
              id TEXT PRIMARY KEY,
              job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
              attempt_number INTEGER NOT NULL CHECK (attempt_number >= 1),
              state TEXT NOT NULL,
              provider_request_id TEXT,
              started_at TEXT NOT NULL,
              finished_at TEXT,
              error_code TEXT,
              error_detail TEXT,
              UNIQUE(job_id, attempt_number)
            );
            CREATE TABLE IF NOT EXISTS job_checkpoints (
              job_id TEXT PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
              stage TEXT NOT NULL,
              checkpoint_json TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS job_dependency_snapshots (
              job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
              dependency_kind TEXT NOT NULL,
              dependency_id TEXT NOT NULL,
              dependency_revision TEXT NOT NULL,
              PRIMARY KEY(job_id, dependency_kind, dependency_id)
            );
            CREATE TABLE IF NOT EXISTS usage_receipts (
              id TEXT PRIMARY KEY,
              operation_id TEXT NOT NULL,
              job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
              provider TEXT NOT NULL,
              model TEXT NOT NULL,
              context_epoch TEXT,
              input_tokens INTEGER,
              cached_input_tokens INTEGER,
              uncached_input_tokens INTEGER,
              output_tokens INTEGER,
              reasoning_tokens INTEGER,
              latency_ms INTEGER,
              estimated_cost TEXT,
              file_reuse INTEGER,
              session_resume INTEGER,
              paper_root_branch INTEGER,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS operation_journal (
              id TEXT PRIMARY KEY,
              operation_kind TEXT NOT NULL,
              entity_kind TEXT NOT NULL,
              entity_id TEXT NOT NULL,
              source_path TEXT,
              target_path TEXT,
              state TEXT NOT NULL,
              payload_json TEXT NOT NULL DEFAULT '{}',
              created_at TEXT NOT NULL,
              committed_at TEXT
            );
            CREATE TABLE IF NOT EXISTS reconciliation_conflicts (
              id TEXT PRIMARY KEY,
              conflict_kind TEXT NOT NULL,
              relative_path TEXT,
              paper_id TEXT REFERENCES papers(id) ON DELETE SET NULL,
              details_json TEXT NOT NULL,
              status TEXT NOT NULL,
              created_at TEXT NOT NULL,
              resolved_at TEXT
            );
            CREATE TABLE IF NOT EXISTS trash_entries (
              id TEXT PRIMARY KEY,
              paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
              original_relative_path TEXT NOT NULL,
              trash_relative_path TEXT NOT NULL,
              deleted_at TEXT NOT NULL,
              purge_after TEXT NOT NULL,
              restored_at TEXT
            );
            CREATE TABLE IF NOT EXISTS remote_tombstones (
              id TEXT PRIMARY KEY,
              provider TEXT NOT NULL,
              resource_kind TEXT NOT NULL,
              remote_id TEXT NOT NULL,
              paper_id TEXT REFERENCES papers(id) ON DELETE SET NULL,
              state TEXT NOT NULL,
              attempts INTEGER NOT NULL DEFAULT 0,
              last_error TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL,
              UNIQUE(provider, resource_kind, remote_id)
            );
            CREATE TABLE IF NOT EXISTS outline_revisions (
              id TEXT PRIMARY KEY,
              paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
              revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
              ocr_revision_id TEXT NOT NULL REFERENCES ocr_revisions(id) ON DELETE CASCADE,
              kind TEXT NOT NULL CHECK (kind IN ('overview', 'deep_dive')),
              parent_overview_id TEXT REFERENCES outline_revisions(id) ON DELETE CASCADE,
              parent_node_id TEXT,
              status TEXT NOT NULL CHECK (status IN ('partial', 'published', 'superseded')),
              protocol_version TEXT NOT NULL,
              catalog_digest TEXT NOT NULL,
              units_json TEXT NOT NULL DEFAULT '[]',
              graph_json TEXT,
              coverage_json TEXT NOT NULL DEFAULT '{}',
              dependency_snapshot_json TEXT NOT NULL DEFAULT '{}',
              provider_node_id TEXT REFERENCES provider_nodes(id) ON DELETE SET NULL,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS outline_heads (
              revision_id TEXT PRIMARY KEY REFERENCES document_revisions(id) ON DELETE CASCADE,
              overview_revision_id TEXT NOT NULL REFERENCES outline_revisions(id) ON DELETE CASCADE,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS outline_deep_dive_heads (
              overview_revision_id TEXT NOT NULL REFERENCES outline_revisions(id) ON DELETE CASCADE,
              node_id TEXT NOT NULL,
              deep_dive_revision_id TEXT NOT NULL REFERENCES outline_revisions(id) ON DELETE CASCADE,
              updated_at TEXT NOT NULL,
              PRIMARY KEY (overview_revision_id, node_id)
            );
            CREATE TABLE IF NOT EXISTS outline_plans (
              id TEXT PRIMARY KEY,
              revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
              ocr_revision_id TEXT NOT NULL REFERENCES ocr_revisions(id) ON DELETE CASCADE,
              catalog_digest TEXT NOT NULL,
              model TEXT NOT NULL,
              payload_json TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS reading_guide_revisions (
              id TEXT PRIMARY KEY,
              paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
              revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
              ocr_revision_id TEXT NOT NULL REFERENCES ocr_revisions(id) ON DELETE CASCADE,
              status TEXT NOT NULL CHECK (status IN ('partial', 'published', 'superseded')),
              protocol_version TEXT NOT NULL,
              prompt_version TEXT NOT NULL,
              model TEXT NOT NULL,
              language TEXT NOT NULL DEFAULT 'en',
              reused_outline_revision_id TEXT,
              coverage_json TEXT NOT NULL DEFAULT '{}',
              warnings_json TEXT NOT NULL DEFAULT '[]',
              context_json TEXT NOT NULL,
              inks_json TEXT NOT NULL,
              dependency_snapshot_json TEXT NOT NULL DEFAULT '{}',
              provider_node_id TEXT REFERENCES provider_nodes(id) ON DELETE SET NULL,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS reading_guide_heads (
              revision_id TEXT PRIMARY KEY REFERENCES document_revisions(id) ON DELETE CASCADE,
              guide_revision_id TEXT NOT NULL REFERENCES reading_guide_revisions(id) ON DELETE CASCADE,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS reading_guide_plans (
              id TEXT PRIMARY KEY,
              revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
              ocr_revision_id TEXT NOT NULL REFERENCES ocr_revisions(id) ON DELETE CASCADE,
              catalog_digest TEXT NOT NULL,
              model TEXT NOT NULL,
              payload_json TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            COMMIT;
            "#,
        )
        .map_err(|error| {
            let _ = connection.execute_batch("ROLLBACK;");
            error.to_string()
        })?;
    // Fresh databases start at zero. Existing recognized pre-v6 workspaces
    // may already have a later marker, so never regress their user_version
    // merely to reuse the base-schema DDL above.
    if user_version_before_initialization < 4 {
        connection
            .execute_batch("PRAGMA user_version = 4;")
            .map_err(|error| error.to_string())?;
    }
    migrate_outline_schema(&connection)?;
    migrate_reading_guide_schema(&connection)?;
    crate::guide_module::ensure_extension_tables(&connection)?;
    migrate_hot_indexes(&connection)?;
    migrate_outline_epoch(&connection)?;
    crate::roadmap_module::create_tables(&connection).map_err(|e| e.to_string())?;
    migrate_library_roots(&connection)?;
    migrate_long_pdf_warning(&connection)?;
    ensure_hub_sort_tables(&connection)?;
    connection
        .execute(
            "INSERT OR REPLACE INTO schema_meta(key, value) VALUES ('workspace_format', ?1)",
            params![WORKSPACE_FORMAT_VERSION.to_string()],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT OR REPLACE INTO schema_meta(key, value) VALUES ('schema_version', ?1)",
            params![SQLITE_V6_SCHEMA_VERSION.to_string()],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExistingDatabaseSchema {
    Legacy,
    V6,
    V7,
    V8,
}

fn inspect_database_schema_state(path: &Path) -> Result<ExistingDatabaseSchema, String> {
    let sidecars = ["-wal", "-shm", "-journal"]
        .iter()
        .map(|suffix| {
            let sidecar = sqlite_sidecar_path(path, suffix);
            match fs::symlink_metadata(&sidecar) {
                Ok(metadata) if metadata.file_type().is_file() => {
                    let bytes = fs::read(&sidecar).map_err(|error| {
                        format!(
                            "Unable to snapshot SQLite sidecar {}: {error}",
                            sidecar.display()
                        )
                    })?;
                    Ok((sidecar, true, Some(bytes)))
                }
                Ok(_) => Err(format!(
                    "SQLite sidecar is not a regular file: {}",
                    sidecar.display()
                )),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    Ok((sidecar, false, None))
                }
                Err(error) => Err(format!(
                    "Unable to inspect SQLite sidecar {}: {error}",
                    sidecar.display()
                )),
            }
        })
        .collect::<Result<Vec<_>, String>>()?;

    let result = inspect_database_schema_state_read_only(path);

    for (sidecar, present, bytes) in sidecars {
        if let Some(bytes) = bytes {
            fs::write(&sidecar, bytes).map_err(|error| {
                format!(
                    "Unable to restore SQLite sidecar {} after read-only inspection: {error}",
                    sidecar.display()
                )
            })?;
        } else if !present {
            match fs::symlink_metadata(&sidecar) {
                Ok(metadata) if metadata.file_type().is_file() => {
                    fs::remove_file(&sidecar).map_err(|error| {
                        format!(
                            "Unable to remove temporary SQLite sidecar {}: {error}",
                            sidecar.display()
                        )
                    })?;
                }
                Ok(_) => {
                    return Err(format!(
                        "Read-only inspection created a non-regular SQLite sidecar: {}",
                        sidecar.display()
                    ));
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => {
                    return Err(format!(
                        "Unable to inspect temporary SQLite sidecar {}: {error}",
                        sidecar.display()
                    ));
                }
            }
        }
    }

    result
}

fn inspect_database_schema_state_read_only(path: &Path) -> Result<ExistingDatabaseSchema, String> {
    let connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|error| format!("Unable to open Workspace database read-only: {error}"))?;
    let quick_check: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|error| format!("Unable to validate Workspace database: {error}"))?;
    if !quick_check.eq_ignore_ascii_case("ok") {
        return Err(format!(
            "Workspace schema inspection failed quick_check: {quick_check}"
        ));
    }

    let user_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| format!("Unable to read Workspace user_version: {error}"))?;
    let has_schema_meta: i64 = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_meta'
             )",
            [],
            |row| row.get(0),
        )
        .map_err(|error| format!("Unable to inspect Workspace schema metadata: {error}"))?;

    if has_schema_meta == 0 {
        if (0..=SQLITE_V6_SCHEMA_VERSION).contains(&user_version) {
            validate_legacy_database_shape(&connection)?;
            return Ok(ExistingDatabaseSchema::Legacy);
        }
        return Err(format!(
            "Workspace schema metadata is missing for user_version={user_version}"
        ));
    }
    if !table_has_column(&connection, "schema_meta", "key")?
        || !table_has_column(&connection, "schema_meta", "value")?
    {
        return Err("Workspace schema_meta table is incomplete".to_string());
    }

    let workspace_format = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'workspace_format'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("Unable to read Workspace format version: {error}"))?
        .map(|value| {
            value
                .parse::<i64>()
                .map_err(|_| "Workspace format version is not an integer".to_string())
        })
        .transpose()?;
    if workspace_format.is_some_and(|version| !(0..=WORKSPACE_FORMAT_VERSION).contains(&version)) {
        return Err(format!(
            "Unsupported Workspace format version: {}",
            workspace_format.unwrap_or_default()
        ));
    }

    let schema_version = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| format!("Unable to read Workspace schema version: {error}"))?
        .map(|value| {
            value
                .parse::<i64>()
                .map_err(|_| "Workspace schema_version is not an integer".to_string())
        })
        .transpose()?;

    match (schema_version, user_version) {
        (Some(SQLITE_SCHEMA_VERSION), version) if version == SQLITE_SCHEMA_VERSION => {
            validate_v8_database(&connection)?;
            Ok(ExistingDatabaseSchema::V8)
        }
        (Some(SQLITE_V7_SCHEMA_VERSION), SQLITE_V7_SCHEMA_VERSION) => {
            validate_v7_database(&connection)?;
            Ok(ExistingDatabaseSchema::V7)
        }
        (Some(6), 6) => {
            validate_legacy_database_shape(&connection)?;
            Ok(ExistingDatabaseSchema::V6)
        }
        (Some(schema), user) if schema == user && (0..6).contains(&schema) => {
            validate_legacy_database_shape(&connection)?;
            Ok(ExistingDatabaseSchema::Legacy)
        }
        (None, user) if (0..=SQLITE_V6_SCHEMA_VERSION).contains(&user) => {
            validate_legacy_database_shape(&connection)?;
            Ok(ExistingDatabaseSchema::Legacy)
        }
        (Some(schema), user) if schema > SQLITE_SCHEMA_VERSION || user > SQLITE_SCHEMA_VERSION => {
            Err(format!(
                "Unsupported future Workspace schema: user_version={user}, schema_version={schema}"
            ))
        }
        (schema, user) => Err(format!(
            "Workspace schema versions do not match: user_version={user}, schema_version={schema:?}"
        )),
    }
}

fn validate_legacy_database_shape(connection: &Connection) -> Result<(), String> {
    for table in ["collections", "papers", "document_revisions"] {
        let exists: i64 = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
                 )",
                params![table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists == 0 {
            return Err(format!(
                "Workspace schema is missing required table: {table}"
            ));
        }
    }
    for (table, column) in [
        ("collections", "id"),
        ("collections", "relative_path"),
        ("papers", "id"),
        ("papers", "collection_id"),
        ("papers", "relative_path"),
        ("document_revisions", "id"),
        ("document_revisions", "paper_id"),
        ("document_revisions", "sha256"),
    ] {
        if !table_has_column(connection, table, column)? {
            return Err(format!(
                "Workspace schema is missing required column: {table}.{column}"
            ));
        }
    }
    // A database which still contains any schema-7-only object must not be
    // treated as an old schema merely because both version markers were
    // edited downward. Doing so would make the legacy initializer issue v6
    // DDL against an existing v7 layout, which is neither a migration nor a
    // safe downgrade. Detect this before any writable connection or backup is
    // opened and fail closed instead.
    for table in [
        "remote_endpoint_snapshots",
        "provider_route_snapshots",
        "job_provider_requirements",
    ] {
        let exists: i64 = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
                 )",
                params![table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists != 0 {
            return Err(format!(
                "Workspace metadata reports a pre-v7 schema but contains v7 table: {table}"
            ));
        }
    }
    for (table, column) in [
        ("jobs", "provider_route_id"),
        ("jobs", "provider_route_origin"),
        ("context_roots", "provider_route_id"),
        ("provider_nodes", "provider_route_id"),
        ("usage_receipts", "provider_route_id"),
        ("remote_tombstones", "endpoint_scope"),
        ("remote_tombstones", "ownership_status"),
    ] {
        if table_has_column(connection, table, column)? {
            return Err(format!(
                "Workspace metadata reports a pre-v7 schema but contains v7 column: {table}.{column}"
            ));
        }
    }
    // Schema 8 (D-063) adds its own object set. A database holding any of it
    // while reporting a pre-v7 version is equally un-migratable downward.
    for table in [
        "library_change_seq",
        "library_domain_revisions",
        "library_action_receipts",
        "paper_lifecycle",
        "reading_engagement",
        "smart_collections",
        "job_preparations",
        "library_batches",
        "library_batch_items",
        "library_batch_job_links",
        "library_undo_tokens",
    ] {
        let exists: i64 = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
                 )",
                params![table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists != 0 {
            return Err(format!(
                "Workspace metadata reports a pre-v7 schema but contains v8 table: {table}"
            ));
        }
    }

    Ok(())
}

/// Return whether an existing database is a known V2 shape. Inspection is
/// deliberately read-only; callers move the database only after this
/// connection is dropped. Missing schema_version is accepted for workspaces
/// created before that metadata key was introduced and is filled in by the
/// migration below.
fn existing_database_needs_backup(path: &Path) -> Result<bool, String> {
    let size = fs::metadata(path)
        .map_err(|error| format!("Unable to inspect Workspace database: {error}"))?
        .len();
    if size == 0 {
        return Ok(false);
    }

    // A normal read-only connection is intentionally used instead of
    // SQLite's `immutable=1` URI mode. Immutable mode ignores the WAL file;
    // a healthy workspace with uncheckpointed schema/data pages could then
    // look corrupt and be rebuilt, losing the only current copy of rows.
    // READ_ONLY still allows SQLite to read valid WAL/SHM sidecars. SQLite
    // may refresh the SHM lock/header while doing that read, so snapshot the
    // sidecars first and restore their exact bytes before returning. This is
    // especially important when a future/corrupt database is about to be
    // moved to a user-visible backup directory.
    let sidecars = ["-wal", "-shm", "-journal"]
        .iter()
        .map(|suffix| {
            let sidecar = sqlite_sidecar_path(path, suffix);
            let metadata = fs::symlink_metadata(&sidecar).ok();
            let present = metadata.is_some();
            let bytes = metadata
                .filter(|metadata| metadata.file_type().is_file())
                .map(|_| {
                    fs::read(&sidecar).map_err(|error| {
                        format!(
                            "Unable to snapshot SQLite sidecar {}: {error}",
                            sidecar.display()
                        )
                    })
                })
                .transpose()?;
            Ok::<_, String>((sidecar, present, bytes))
        })
        .collect::<Result<Vec<_>, String>>()?;

    let result = (|| -> Result<bool, String> {
        let connection = match Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(connection) => connection,
            Err(_) => return Ok(true),
        };
        let integrity: String =
            match connection.query_row("PRAGMA quick_check", [], |row| row.get(0)) {
                Ok(value) => value,
                Err(_) => return Ok(true),
            };
        if !integrity.eq_ignore_ascii_case("ok") {
            return Ok(true);
        }

        let has_schema_meta: i64 = match connection.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_meta'
             )",
            [],
            |row| row.get(0),
        ) {
            Ok(value) => value,
            Err(_) => return Ok(true),
        };

        // `schema_meta` was introduced after the first V2 databases. Its
        // absence is a migratable state; initialize_database creates it and
        // writes the current metadata after the structural migrations.
        if has_schema_meta != 0 {
            for (key, maximum) in [
                ("workspace_format", WORKSPACE_FORMAT_VERSION),
                ("schema_version", SQLITE_SCHEMA_VERSION),
            ] {
                let value: Option<String> = match connection
                    .query_row(
                        "SELECT value FROM schema_meta WHERE key = ?1",
                        params![key],
                        |row| row.get(0),
                    )
                    .optional()
                {
                    Ok(value) => value,
                    Err(_) => return Ok(true),
                };
                if let Some(value) = value {
                    match value.parse::<i64>() {
                        Ok(version) if version <= maximum => {}
                        Ok(_) | Err(_) => return Ok(true),
                    }
                }
            }
            if !table_has_column(&connection, "schema_meta", "key")?
                || !table_has_column(&connection, "schema_meta", "value")?
            {
                return Ok(true);
            }
        }

        let user_version: i64 =
            match connection.query_row("PRAGMA user_version", [], |row| row.get(0)) {
                Ok(value) => value,
                Err(_) => return Ok(true),
            };
        if user_version > SQLITE_SCHEMA_VERSION {
            return Ok(true);
        }

        for table in ["collections", "papers", "document_revisions"] {
            let exists: i64 = match connection.query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
                 )",
                params![table],
                |row| row.get(0),
            ) {
                Ok(value) => value,
                Err(_) => return Ok(true),
            };
            if exists == 0 {
                return Ok(true);
            }
        }
        for (table, column) in [
            ("collections", "id"),
            ("collections", "relative_path"),
            ("papers", "id"),
            ("papers", "collection_id"),
            ("papers", "relative_path"),
            ("document_revisions", "id"),
            ("document_revisions", "paper_id"),
            ("document_revisions", "sha256"),
        ] {
            if !table_has_column(&connection, table, column).unwrap_or(false) {
                return Ok(true);
            }
        }
        Ok(false)
    })();

    for (sidecar, present, bytes) in sidecars {
        if let Some(bytes) = bytes {
            fs::write(&sidecar, bytes).map_err(|error| {
                format!(
                    "Unable to restore SQLite sidecar {}: {error}",
                    sidecar.display()
                )
            })?;
        } else if !present && sidecar.is_file() {
            fs::remove_file(&sidecar).map_err(|error| {
                format!(
                    "Unable to remove temporary SQLite sidecar {}: {error}",
                    sidecar.display()
                )
            })?;
        }
    }
    result
}

fn normalize_workspace_root(root: &Path) -> Result<PathBuf, String> {
    let p = if root.is_absolute() {
        root.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| format!("Unable to resolve workspace path: {e}"))?
            .join(root)
    };
    // reject parent dir components
    if p.components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Workspace path must not contain '..'".to_string());
    }
    Ok(p)
}

fn validate_workspace_root_safety(root: &Path) -> Result<(), String> {
    if root.parent().is_none() {
        return Err("Workspace cannot be filesystem root".to_string());
    }
    if root.components().count() <= 2 && root.has_root() {
        let s = root.to_string_lossy();
        if s == "/" || s.ends_with(":\\") || s.ends_with(":/") {
            return Err("Workspace cannot be filesystem root".to_string());
        }
    }
    if let Ok(meta) = fs::symlink_metadata(root) {
        if meta.file_type().is_symlink() {
            return Err("Workspace path must not be a symlink".to_string());
        }
    }
    Ok(())
}

fn backup_path_for(root: &Path, digest: &str) -> PathBuf {
    let stable_suffix = digest.chars().take(20).collect::<String>();
    root.join(".read-desktop-backups")
        .join(format!("reset-{stable_suffix}"))
}

#[derive(Debug)]
struct LegacyInventoryEntry {
    relative_path: String,
    byte_size: u64,
    modified_nanos: u128,
}

#[derive(Debug)]
struct LegacyInventory {
    file_count: u64,
    pdf_count: u64,
    total_bytes: u64,
    entries: Vec<LegacyInventoryEntry>,
}

impl LegacyInventory {
    fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"read-desktop-legacy-reset-v1");
        hasher.update(self.file_count.to_le_bytes());
        hasher.update(self.pdf_count.to_le_bytes());
        hasher.update(self.total_bytes.to_le_bytes());
        for entry in &self.entries {
            hasher.update(entry.relative_path.as_bytes());
            hasher.update([0]);
            hasher.update(entry.byte_size.to_le_bytes());
            hasher.update(entry.modified_nanos.to_le_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
}

fn inspect_legacy_file(path: &Path, label: &str) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(format!("{label} must not be a symbolic link"))
        }
        Ok(metadata) if metadata.is_file() => Ok(true),
        Ok(_) => Err(format!("{label} path is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Unable to inspect {label}: {error}")),
    }
}

fn inspect_legacy_directory(path: &Path, label: &str) -> Result<bool, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(format!("{label} must not be a symbolic link"))
        }
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(_) => Err(format!("{label} path is not a directory")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!("Unable to inspect {label}: {error}")),
    }
}

fn inventory_entry(root: &Path, path: &Path) -> Result<LegacyInventoryEntry, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Unable to inspect {}: {error}", path.display()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(format!(
            "Legacy reset only accepts regular files: {}",
            path.display()
        ));
    }
    let relative_path = path
        .strip_prefix(root)
        .map_err(|_| format!("Legacy file escaped Workspace: {}", path.display()))?
        .to_string_lossy()
        .replace('\\', "/");
    let modified_nanos = metadata
        .modified()
        .map_err(|error| {
            format!(
                "Unable to read modification time for {}: {error}",
                path.display()
            )
        })?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| format!("Invalid modification time for {}", path.display()))?
        .as_nanos();
    Ok(LegacyInventoryEntry {
        relative_path,
        byte_size: metadata.len(),
        modified_nanos,
    })
}

fn legacy_inventory(
    root: &Path,
    database: &Path,
    database_exists: bool,
    library: &Path,
    library_exists: bool,
) -> Result<LegacyInventory, String> {
    let mut entries = Vec::new();
    if database_exists {
        entries.push(inventory_entry(root, database)?);
        for suffix in ["-wal", "-shm", "-journal"] {
            let sidecar = sqlite_sidecar_path(database, suffix);
            if inspect_legacy_file(&sidecar, "Legacy database sidecar")? {
                entries.push(inventory_entry(root, &sidecar)?);
            }
        }
    }
    if library_exists {
        let mut stack = vec![library.to_path_buf()];
        while let Some(directory) = stack.pop() {
            let directory_entries = fs::read_dir(&directory)
                .map_err(|error| format!("Unable to read {}: {error}", directory.display()))?;
            for entry in directory_entries {
                let entry = entry.map_err(|error| {
                    format!("Unable to enumerate {}: {error}", directory.display())
                })?;
                let path = entry.path();
                let metadata = fs::symlink_metadata(&path)
                    .map_err(|error| format!("Unable to inspect {}: {error}", path.display()))?;
                if metadata.file_type().is_symlink() {
                    return Err(format!(
                        "Legacy library contains a symbolic link: {}",
                        path.display()
                    ));
                }
                if metadata.is_dir() {
                    stack.push(path);
                } else if metadata.is_file() {
                    entries.push(inventory_entry(root, &path)?);
                } else {
                    return Err(format!(
                        "Legacy library contains an unsupported filesystem entry: {}",
                        path.display()
                    ));
                }
            }
        }
    }
    entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let file_count = entries.len() as u64;
    let pdf_count = entries
        .iter()
        .filter(|entry| {
            Path::new(&entry.relative_path)
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("pdf"))
        })
        .count() as u64;
    let total_bytes = entries.iter().map(|entry| entry.byte_size).sum();
    Ok(LegacyInventory {
        file_count,
        pdf_count,
        total_bytes,
        entries,
    })
}

fn move_path(src: &Path, dst: &Path, moved: &mut Vec<(PathBuf, PathBuf)>) -> Result<(), String> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Unable to create backup parent: {e}"))?;
    }
    fs::rename(src, dst).map_err(|error| {
        format!(
            "Unable to atomically move {} -> {}: {error}. Reset requires a same-volume rename",
            src.display(),
            dst.display()
        )
    })?;
    moved.push((src.to_path_buf(), dst.to_path_buf()));
    Ok(())
}

fn rollback_moves(moved: &mut Vec<(PathBuf, PathBuf)>) -> Result<(), String> {
    let mut failures = Vec::new();
    for (orig, dst) in moved.drain(..).rev() {
        if orig.exists() {
            failures.push(format!(
                "Refused to overwrite restored path {}",
                orig.display()
            ));
            continue;
        }
        if let Err(error) = fs::rename(&dst, &orig) {
            failures.push(format!(
                "Unable to restore {} -> {}: {error}",
                dst.display(),
                orig.display()
            ));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

fn archive_failed_v2(root: &Path, backup_dir: &Path) -> Result<(), String> {
    let internal_path = root.join(".read-desktop");
    if !internal_path.exists() {
        return Ok(());
    }
    let failed_path = backup_dir.join("failed-v2");
    fs::rename(&internal_path, &failed_path).map_err(|error| {
        format!(
            "Unable to preserve failed V2 state {} -> {}: {error}",
            internal_path.display(),
            failed_path.display()
        )
    })
}

fn rollback_after_failure(
    backup_dir: &Path,
    moved: &mut Vec<(PathBuf, PathBuf)>,
    cause: &str,
) -> String {
    let rollback_error = rollback_moves(moved).err();
    let message = reset_failure_message(backup_dir, cause, None, None, rollback_error);
    write_rollback_record(backup_dir, &message);
    message
}

fn reset_failure_message(
    backup_dir: &Path,
    cause: &str,
    release_error: Option<String>,
    archive_error: Option<String>,
    rollback_error: Option<String>,
) -> String {
    let mut diagnostics = Vec::new();
    if let Some(error) = release_error {
        diagnostics.push(format!("lock release failed: {error}"));
    }
    if let Some(error) = archive_error {
        diagnostics.push(format!("failed V2 archive failed: {error}"));
    }
    if let Some(error) = rollback_error {
        diagnostics.push(format!("legacy restore failed: {error}"));
    }
    if diagnostics.is_empty() {
        format!(
            "V2 initialization failed; Legacy data was restored and the diagnostic backup was preserved at {}: {cause}",
            backup_dir.display()
        )
    } else {
        format!(
            "Reset failed and automatic recovery is incomplete. Do not retry; preserve {} and recover manually. Cause: {cause}. {}",
            backup_dir.display(),
            diagnostics.join("; ")
        )
    }
}

fn write_rollback_record(backup_dir: &Path, error: &str) {
    let record = serde_json::json!({
        "error": error,
        "rolledBackAt": Utc::now().to_rfc3339(),
    });
    if let Ok(json) = serde_json::to_string_pretty(&record) {
        let _ = fs::write(backup_dir.join("rollback.json"), json);
    }
}

fn sqlite_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();
    path.with_file_name(format!("{file_name}{suffix}"))
}

fn sqlite_sidecar_exists(path: &Path) -> bool {
    ["-wal", "-shm", "-journal"]
        .iter()
        .map(|suffix| sqlite_sidecar_path(path, suffix))
        .any(|sidecar| sidecar.exists())
}

/// Move an incompatible database and its SQLite journals out of the active
/// path. The backup directory is unique and timestamped so users can recover
/// the original bytes without the application ever deleting them.
fn backup_database_files(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Workspace database has no parent directory".to_string())?;
    let backup_root = parent.join("backups");
    fs::create_dir_all(&backup_root)
        .map_err(|error| format!("Unable to create database backup directory: {error}"))?;
    let backup_dir = backup_root.join(format!(
        "schema-{}-{}",
        Utc::now().format("%Y%m%dT%H%M%S%.fZ"),
        Uuid::new_v4().simple()
    ));
    fs::create_dir(&backup_dir)
        .map_err(|error| format!("Unable to create database backup: {error}"))?;

    let mut files = vec![path.to_path_buf()];
    files.extend(
        ["-wal", "-shm", "-journal"]
            .iter()
            .map(|suffix| sqlite_sidecar_path(path, suffix)),
    );
    let mut moved = Vec::new();
    for source in files {
        if !source.exists() {
            continue;
        }
        let file_name = source
            .file_name()
            .ok_or_else(|| "Workspace database backup file has no name".to_string())?;
        let destination = backup_dir.join(file_name);
        if let Err(error) = fs::rename(&source, &destination) {
            for (original, moved_to) in moved.into_iter().rev() {
                let _ = fs::rename(moved_to, original);
            }
            return Err(format!(
                "Unable to move incompatible database file {} to backup: {error}",
                source.display()
            ));
        }
        moved.push((source, destination));
    }
    Ok(backup_dir)
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> Result<bool, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    Ok(columns.iter().any(|name| name == column))
}

pub(crate) fn migrate_library_roots(connection: &Connection) -> Result<(), String> {
    let timestamp = Utc::now().to_rfc3339();
    let mut collection_statement = connection
        .prepare("SELECT id, parent_id, relative_path FROM collections")
        .map_err(|error| error.to_string())?;
    let collections = collection_statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<(String, Option<String>, String)>>>()
        .map_err(|error| error.to_string())?;
    drop(collection_statement);
    for (id, parent_id, relative_path) in collections {
        let next = if parent_id.is_none()
            && id == PAPERS_ROOT_COLLECTION_ID
            && (relative_path.is_empty() || relative_path.eq_ignore_ascii_case(PAPERS_DIR))
        {
            PAPERS_DIR.to_string()
        } else if parent_id.is_none()
            && id == TEXTBOOKS_ROOT_COLLECTION_ID
            && (relative_path.is_empty() || relative_path.eq_ignore_ascii_case(TEXTBOOKS_DIR))
        {
            TEXTBOOKS_DIR.to_string()
        } else {
            prefix_legacy_relative(&relative_path)
        };
        if next != relative_path {
            connection
                .execute(
                    "UPDATE collections SET relative_path = ?1, updated_at = ?2 WHERE id = ?3",
                    params![next, timestamp, id],
                )
                .map_err(|error| error.to_string())?;
        }
    }

    let mut paper_statement = connection
        .prepare("SELECT id, relative_path FROM papers")
        .map_err(|error| error.to_string())?;
    let paper_paths = paper_statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<(String, String)>>>()
        .map_err(|error| error.to_string())?;
    drop(paper_statement);
    for (id, relative_path) in paper_paths {
        let next = prefix_legacy_relative(&relative_path);
        if next != relative_path {
            connection
                .execute(
                    "UPDATE papers SET relative_path = ?1, updated_at = ?2 WHERE id = ?3",
                    params![next, timestamp, id],
                )
                .map_err(|error| error.to_string())?;
        }
    }

    let mut revision_statement = connection
        .prepare("SELECT id, source_relative_path FROM document_revisions")
        .map_err(|error| error.to_string())?;
    let revision_paths = revision_statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<(String, String)>>>()
        .map_err(|error| error.to_string())?;
    drop(revision_statement);
    for (id, relative_path) in revision_paths {
        let next = prefix_legacy_relative(&relative_path);
        if next != relative_path {
            connection
                .execute(
                    "UPDATE document_revisions SET source_relative_path = ?1 WHERE id = ?2",
                    params![next, id],
                )
                .map_err(|error| error.to_string())?;
        }
    }

    connection
        .execute(
            "INSERT OR IGNORE INTO collections(
               id, parent_id, name, relative_path, created_at, updated_at
             ) VALUES (?1, NULL, ?2, ?2, ?3, ?3)",
            params![TEXTBOOKS_ROOT_COLLECTION_ID, TEXTBOOKS_DIR, timestamp],
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "INSERT OR IGNORE INTO collections(
               id, parent_id, name, relative_path, created_at, updated_at
             ) VALUES (?1, NULL, ?2, ?2, ?3, ?3)",
            params![PAPERS_ROOT_COLLECTION_ID, PAPERS_DIR, timestamp],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn migrate_long_pdf_warning(connection: &Connection) -> Result<(), String> {
    if !table_has_column(connection, "reading_states", "long_pdf_warning_acked")? {
        connection
            .execute(
                "ALTER TABLE reading_states ADD COLUMN long_pdf_warning_acked INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .map_err(|error| error.to_string())?;
    }
    connection
        .execute_batch("PRAGMA user_version = 6;")
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) fn ensure_hub_sort_tables(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS collection_paper_order (
              collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
              paper_id      TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
              position      INTEGER NOT NULL CHECK (position >= 0),
              PRIMARY KEY (collection_id, paper_id)
            );
            CREATE INDEX IF NOT EXISTS idx_collection_paper_order_pos
              ON collection_paper_order(collection_id, position);
            CREATE TABLE IF NOT EXISTS collection_sort_prefs (
              collection_id TEXT NOT NULL PRIMARY KEY REFERENCES collections(id) ON DELETE CASCADE,
              sort_mode     TEXT NOT NULL CHECK (sort_mode IN ('recent','year','title','manual','chapter')),
              updated_at    TEXT NOT NULL
            );
            "#,
        )
        .map_err(|error| error.to_string())
}

fn migrate_outline_schema(connection: &Connection) -> Result<(), String> {
    if !table_has_column(connection, "reading_states", "workspace_layout")? {
        connection
            .execute(
                "ALTER TABLE reading_states ADD COLUMN workspace_layout TEXT NOT NULL DEFAULT 'pdf_discussion'",
                [],
            )
            .map_err(|error| error.to_string())?;
    }
    if !table_has_column(connection, "reading_states", "active_outline_node_id")? {
        connection
            .execute(
                "ALTER TABLE reading_states ADD COLUMN active_outline_node_id TEXT",
                [],
            )
            .map_err(|error| error.to_string())?;
    }
    if !table_has_column(connection, "reading_states", "outline_view")? {
        connection
            .execute(
                "ALTER TABLE reading_states ADD COLUMN outline_view TEXT NOT NULL DEFAULT 'overview'",
                [],
            )
            .map_err(|error| error.to_string())?;
    }
    if !table_has_column(connection, "reading_states", "outline_inspector_width")? {
        connection
            .execute(
                "ALTER TABLE reading_states ADD COLUMN outline_inspector_width INTEGER NOT NULL DEFAULT 280",
                [],
            )
            .map_err(|error| error.to_string())?;
    }
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS outline_revisions (
              id TEXT PRIMARY KEY,
              paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
              revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
              ocr_revision_id TEXT NOT NULL REFERENCES ocr_revisions(id) ON DELETE CASCADE,
              kind TEXT NOT NULL CHECK (kind IN ('overview', 'deep_dive')),
              parent_overview_id TEXT REFERENCES outline_revisions(id) ON DELETE CASCADE,
              parent_node_id TEXT,
              status TEXT NOT NULL CHECK (status IN ('partial', 'published', 'superseded')),
              protocol_version TEXT NOT NULL,
              catalog_digest TEXT NOT NULL,
              units_json TEXT NOT NULL DEFAULT '[]',
              graph_json TEXT,
              coverage_json TEXT NOT NULL DEFAULT '{}',
              dependency_snapshot_json TEXT NOT NULL DEFAULT '{}',
              provider_node_id TEXT REFERENCES provider_nodes(id) ON DELETE SET NULL,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS outline_heads (
              revision_id TEXT PRIMARY KEY REFERENCES document_revisions(id) ON DELETE CASCADE,
              overview_revision_id TEXT NOT NULL REFERENCES outline_revisions(id) ON DELETE CASCADE,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS outline_deep_dive_heads (
              overview_revision_id TEXT NOT NULL REFERENCES outline_revisions(id) ON DELETE CASCADE,
              node_id TEXT NOT NULL,
              deep_dive_revision_id TEXT NOT NULL REFERENCES outline_revisions(id) ON DELETE CASCADE,
              updated_at TEXT NOT NULL,
              PRIMARY KEY (overview_revision_id, node_id)
            );
            CREATE TABLE IF NOT EXISTS outline_plans (
              id TEXT PRIMARY KEY,
              revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
              ocr_revision_id TEXT NOT NULL REFERENCES ocr_revisions(id) ON DELETE CASCADE,
              catalog_digest TEXT NOT NULL,
              model TEXT NOT NULL,
              payload_json TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            "#,
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn migrate_reading_guide_schema(connection: &Connection) -> Result<(), String> {
    if !table_has_column(connection, "reading_states", "guide_layer_visible")? {
        connection
            .execute(
                "ALTER TABLE reading_states ADD COLUMN guide_layer_visible INTEGER NOT NULL DEFAULT 1",
                [],
            )
            .map_err(|error| error.to_string())?;
    }
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS reading_guide_revisions (
              id TEXT PRIMARY KEY,
              paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
              revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
              ocr_revision_id TEXT NOT NULL REFERENCES ocr_revisions(id) ON DELETE CASCADE,
              status TEXT NOT NULL CHECK (status IN ('partial', 'published', 'superseded')),
              protocol_version TEXT NOT NULL,
              prompt_version TEXT NOT NULL,
              model TEXT NOT NULL,
              language TEXT NOT NULL DEFAULT 'en',
              reused_outline_revision_id TEXT,
              coverage_json TEXT NOT NULL DEFAULT '{}',
              warnings_json TEXT NOT NULL DEFAULT '[]',
              context_json TEXT NOT NULL,
              inks_json TEXT NOT NULL,
              dependency_snapshot_json TEXT NOT NULL DEFAULT '{}',
              provider_node_id TEXT REFERENCES provider_nodes(id) ON DELETE SET NULL,
              created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS reading_guide_heads (
              revision_id TEXT PRIMARY KEY REFERENCES document_revisions(id) ON DELETE CASCADE,
              guide_revision_id TEXT NOT NULL REFERENCES reading_guide_revisions(id) ON DELETE CASCADE,
              updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS reading_guide_plans (
              id TEXT PRIMARY KEY,
              revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
              ocr_revision_id TEXT NOT NULL REFERENCES ocr_revisions(id) ON DELETE CASCADE,
              catalog_digest TEXT NOT NULL,
              model TEXT NOT NULL,
              payload_json TEXT NOT NULL,
              created_at TEXT NOT NULL
            );
            "#,
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Idempotent indexes for the read-heavy projections.  These are kept in a
/// separate migration so existing V2 workspaces receive the same query plan
/// without rebuilding or rewriting business rows.
fn migrate_hot_indexes(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            r#"
            CREATE INDEX IF NOT EXISTS idx_ocr_revisions_revision_status
              ON ocr_revisions(revision_id, status, published_at DESC);
            CREATE INDEX IF NOT EXISTS idx_ocr_pages_revision_page
              ON ocr_pages(ocr_revision_id, page_number);
            CREATE INDEX IF NOT EXISTS idx_ocr_blocks_page_index
              ON ocr_blocks(ocr_page_id, block_index);
            CREATE INDEX IF NOT EXISTS idx_context_roots_revision_state
              ON context_roots(revision_id, state, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_discussions_revision_status_updated
              ON discussions(revision_id, status, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_messages_discussion_created
              ON messages(discussion_id, created_at, id);
            CREATE INDEX IF NOT EXISTS idx_messages_discussion_parent
              ON messages(discussion_id, parent_id);
            CREATE INDEX IF NOT EXISTS idx_artifacts_revision_kind
              ON artifacts(revision_id, kind, object_key, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_artifact_heads_paper_kind
              ON artifact_heads(paper_id, kind, object_key);
            CREATE INDEX IF NOT EXISTS idx_jobs_revision_kind_state
              ON jobs(revision_id, kind, state, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_jobs_paper_state
              ON jobs(paper_id, state, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_job_checkpoints_updated
              ON job_checkpoints(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_usage_receipts_operation
              ON usage_receipts(operation_id);
            CREATE INDEX IF NOT EXISTS idx_usage_receipts_job
              ON usage_receipts(job_id);
            CREATE INDEX IF NOT EXISTS idx_trash_entries_paper_deleted
              ON trash_entries(paper_id, deleted_at DESC);
            CREATE INDEX IF NOT EXISTS idx_trash_entries_expiry
              ON trash_entries(purge_after, restored_at);
            CREATE INDEX IF NOT EXISTS idx_remote_tombstones_state_updated
              ON remote_tombstones(state, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_conflicts_status_created
              ON reconciliation_conflicts(status, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_outline_revisions_revision_kind
              ON outline_revisions(revision_id, kind, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_outline_revisions_parent
              ON outline_revisions(parent_overview_id);
            CREATE INDEX IF NOT EXISTS idx_outline_plans_revision_created
              ON outline_plans(revision_id, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_reading_guide_revisions_revision
              ON reading_guide_revisions(revision_id, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_reading_guide_plans_revision_created
              ON reading_guide_plans(revision_id, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_lens_qa_artifact_created
              ON lens_qa(lens_artifact_id, created_at);
            CREATE INDEX IF NOT EXISTS idx_reading_states_revision
              ON reading_states(revision_id);
            "#,
        )
        .map_err(|error| error.to_string())
}

fn migrate_outline_epoch(connection: &Connection) -> Result<(), String> {
    let current = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key = ?1",
            params![OUTLINE_EPOCH_KEY],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let current_epoch = current
        .as_deref()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    if current_epoch >= OUTLINE_EPOCH {
        return Ok(());
    }
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|error| error.to_string())?;
    let result = (|| -> Result<(), String> {
        // This marker records migration compatibility, not the graph protocol.
        // Keep older maps, plans and paid job checkpoints intact; readers and
        // workers dispatch using each record's own frozen protocol.
        connection
            .execute(
                "INSERT OR REPLACE INTO schema_meta(key, value) VALUES (?1, ?2)",
                params![OUTLINE_EPOCH_KEY, OUTLINE_EPOCH.to_string()],
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    })();
    match result {
        Ok(()) => connection
            .execute_batch("COMMIT;")
            .map_err(|error| error.to_string()),
        Err(error) => {
            let _ = connection.execute_batch("ROLLBACK;");
            Err(error)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum V7MigrationFault {
    SnapshotTables,
    JobColumns,
    ContextRootColumn,
    ProviderNodeColumn,
    UsageReceiptColumn,
    RequirementsTable,
    TombstoneCopy,
    TombstoneSwap,
    Indexes,
    Versions,
}

impl V7MigrationFault {
    #[cfg(test)]
    const ALL: [Self; 10] = [
        Self::SnapshotTables,
        Self::JobColumns,
        Self::ContextRootColumn,
        Self::ProviderNodeColumn,
        Self::UsageReceiptColumn,
        Self::RequirementsTable,
        Self::TombstoneCopy,
        Self::TombstoneSwap,
        Self::Indexes,
        Self::Versions,
    ];
}

#[allow(dead_code)]
fn migrate_v6_to_v7(connection: &mut Connection) -> Result<(), String> {
    migrate_v6_to_v7_with_fault(connection, None)
}

fn migrate_v6_to_v7_with_fault(
    connection: &mut Connection,
    injected_fault: Option<V7MigrationFault>,
) -> Result<(), String> {
    let user_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let schema_version = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Schema 7 migration requires schema_meta.schema_version".to_string())?
        .parse::<i64>()
        .map_err(|_| "Workspace schema_version is not an integer".to_string())?;

    if user_version == 7 && schema_version == 7 {
        return validate_v7_database(connection);
    }
    if user_version != 6 || schema_version != 6 {
        return Err(format!(
            "Schema 7 migration requires matching v6 versions; found user_version={user_version}, schema_version={schema_version}"
        ));
    }

    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    execute_v7_step(
        &transaction,
        injected_fault,
        V7MigrationFault::SnapshotTables,
        r#"
        CREATE TABLE remote_endpoint_snapshots (
          endpoint_scope TEXT PRIMARY KEY,
          version INTEGER NOT NULL CHECK (version >= 1),
          owner_type TEXT NOT NULL
            CHECK (owner_type IN ('paper_provider', 'mistral_ocr')),
          provider_instance_id TEXT,
          provider_name_at_capture TEXT,
          provider_kind TEXT,
          base_url TEXT,
          created_at TEXT NOT NULL,
          CHECK (
            (owner_type = 'paper_provider'
              AND provider_instance_id IS NOT NULL
              AND provider_kind IS NOT NULL)
            OR
            (owner_type = 'mistral_ocr'
              AND provider_instance_id IS NULL
              AND provider_kind IS NULL)
          )
        );

        CREATE TABLE provider_route_snapshots (
          route_id TEXT PRIMARY KEY,
          endpoint_scope TEXT NOT NULL
            REFERENCES remote_endpoint_snapshots(endpoint_scope) ON DELETE RESTRICT,
          version INTEGER NOT NULL CHECK (version >= 1),
          models_json TEXT NOT NULL,
          operation_role TEXT NOT NULL,
          created_at TEXT NOT NULL,
          UNIQUE(endpoint_scope, models_json, operation_role)
        );
        "#,
    )?;
    execute_v7_step(
        &transaction,
        injected_fault,
        V7MigrationFault::JobColumns,
        r#"
        ALTER TABLE jobs ADD COLUMN provider_route_id TEXT
          REFERENCES provider_route_snapshots(route_id) ON DELETE RESTRICT;
        ALTER TABLE jobs ADD COLUMN provider_route_origin TEXT;
        "#,
    )?;
    execute_v7_step(
        &transaction,
        injected_fault,
        V7MigrationFault::ContextRootColumn,
        r#"
        ALTER TABLE context_roots ADD COLUMN provider_route_id TEXT
          REFERENCES provider_route_snapshots(route_id) ON DELETE RESTRICT;
        "#,
    )?;
    execute_v7_step(
        &transaction,
        injected_fault,
        V7MigrationFault::ProviderNodeColumn,
        r#"
        ALTER TABLE provider_nodes ADD COLUMN provider_route_id TEXT
          REFERENCES provider_route_snapshots(route_id) ON DELETE RESTRICT;
        "#,
    )?;
    execute_v7_step(
        &transaction,
        injected_fault,
        V7MigrationFault::UsageReceiptColumn,
        r#"
        ALTER TABLE usage_receipts ADD COLUMN provider_route_id TEXT
          REFERENCES provider_route_snapshots(route_id) ON DELETE RESTRICT;
        "#,
    )?;
    execute_v7_step(
        &transaction,
        injected_fault,
        V7MigrationFault::RequirementsTable,
        r#"
        CREATE TABLE job_provider_requirements (
          job_id TEXT PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
          code TEXT NOT NULL,
          provider_kind TEXT,
          provider_instance_id TEXT,
          can_rebind INTEGER NOT NULL CHECK (can_rebind IN (0, 1)),
          safe_details_json TEXT NOT NULL DEFAULT '{}',
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        "#,
    )?;
    execute_v7_step(
        &transaction,
        injected_fault,
        V7MigrationFault::TombstoneCopy,
        r#"
        CREATE TABLE remote_tombstones_v7 (
          id TEXT PRIMARY KEY,
          provider TEXT NOT NULL,
          resource_kind TEXT NOT NULL,
          remote_id TEXT NOT NULL,
          paper_id TEXT REFERENCES papers(id) ON DELETE SET NULL,
          state TEXT NOT NULL,
          attempts INTEGER NOT NULL DEFAULT 0,
          last_error TEXT,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          endpoint_scope TEXT
            REFERENCES remote_endpoint_snapshots(endpoint_scope) ON DELETE RESTRICT,
          ownership_status TEXT NOT NULL DEFAULT 'legacy_unattributed'
            CHECK (ownership_status IN ('exact', 'legacy_unattributed', 'abandoned')),
          CHECK (
            (ownership_status = 'exact' AND endpoint_scope IS NOT NULL)
            OR (ownership_status = 'legacy_unattributed' AND endpoint_scope IS NULL)
            OR ownership_status = 'abandoned'
          )
        );

        INSERT INTO remote_tombstones_v7(
          id, provider, resource_kind, remote_id, paper_id, state,
          attempts, last_error, created_at, updated_at,
          endpoint_scope, ownership_status
        )
        SELECT
          id, provider, resource_kind, remote_id, paper_id, state,
          attempts, last_error, created_at, updated_at,
          NULL, 'legacy_unattributed'
        FROM remote_tombstones;
        "#,
    )?;
    execute_v7_step(
        &transaction,
        injected_fault,
        V7MigrationFault::TombstoneSwap,
        r#"
        DROP TABLE remote_tombstones;
        ALTER TABLE remote_tombstones_v7 RENAME TO remote_tombstones;
        "#,
    )?;
    execute_v7_step(
        &transaction,
        injected_fault,
        V7MigrationFault::Indexes,
        r#"
        DROP INDEX idx_jobs_active_dedupe;

        CREATE UNIQUE INDEX jobs_active_route_dedupe
          ON jobs(provider_route_id, dedupe_key)
          WHERE provider_route_id IS NOT NULL
            AND state IN ('queued', 'running', 'paused', 'interrupted_unknown');

        -- v6 allowed interrupted_unknown and queued rows to share a logical
        -- dedupe key. Keep both; route-aware mutation APIs use the lookup
        -- index below to guard unscoped work.
        CREATE UNIQUE INDEX jobs_active_legacy_dedupe
          ON jobs(dedupe_key)
          WHERE provider_route_id IS NULL
            AND state IN ('queued', 'running', 'paused');

        CREATE INDEX jobs_active_legacy_dedupe_lookup
          ON jobs(dedupe_key, state)
          WHERE provider_route_id IS NULL
            AND state IN ('queued', 'running', 'paused', 'interrupted_unknown');

        CREATE INDEX jobs_route_state_updated
          ON jobs(provider_route_id, state, updated_at);

        CREATE INDEX context_roots_route_lookup
          ON context_roots(revision_id, provider_route_id, model, context_epoch);

        CREATE INDEX provider_nodes_route_remote
          ON provider_nodes(provider_route_id, provider_node_id);

        CREATE INDEX usage_receipts_route
          ON usage_receipts(provider_route_id, created_at);

        CREATE INDEX idx_remote_tombstones_state_updated
          ON remote_tombstones(state, updated_at DESC);

        CREATE UNIQUE INDEX remote_tombstones_exact_unique
          ON remote_tombstones(endpoint_scope, resource_kind, remote_id)
          WHERE endpoint_scope IS NOT NULL;

        CREATE INDEX remote_tombstones_route_state
          ON remote_tombstones(endpoint_scope, state, updated_at);
        "#,
    )?;
    execute_v7_step(
        &transaction,
        injected_fault,
        V7MigrationFault::Versions,
        r#"
        UPDATE schema_meta SET value = '7' WHERE key = 'schema_version';
        PRAGMA user_version = 7;
        "#,
    )?;
    validate_v7_database(&transaction)?;
    transaction.commit().map_err(|error| error.to_string())?;
    validate_v7_database(connection)
}

fn execute_v7_step(
    transaction: &rusqlite::Transaction<'_>,
    injected_fault: Option<V7MigrationFault>,
    step: V7MigrationFault,
    sql: &str,
) -> Result<(), String> {
    transaction
        .execute_batch(sql)
        .map_err(|error| error.to_string())?;
    if injected_fault == Some(step) {
        return Err(format!("injected schema 7 migration fault at {step:?}"));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum V8MigrationFault {
    RevisionTables,
    ReceiptsTable,
    LifecycleTables,
    SmartCollectionsTable,
    JobPreparationsTable,
    BatchTables,
    BatchItemTables,
    JobLinkTable,
    UndoTokenTable,
    Indexes,
    Versions,
}

impl V8MigrationFault {
    #[cfg(test)]
    const ALL: [Self; 11] = [
        Self::RevisionTables,
        Self::ReceiptsTable,
        Self::LifecycleTables,
        Self::SmartCollectionsTable,
        Self::JobPreparationsTable,
        Self::BatchTables,
        Self::BatchItemTables,
        Self::JobLinkTable,
        Self::UndoTokenTable,
        Self::Indexes,
        Self::Versions,
    ];
}

#[allow(dead_code)]
fn migrate_v7_to_v8(connection: &mut Connection) -> Result<(), String> {
    migrate_v7_to_v8_with_fault(connection, None)
}

fn migrate_v7_to_v8_with_fault(
    connection: &mut Connection,
    injected_fault: Option<V8MigrationFault>,
) -> Result<(), String> {
    let user_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let schema_version = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Schema 8 migration requires schema_meta.schema_version".to_string())?
        .parse::<i64>()
        .map_err(|_| "Workspace schema_version is not an integer".to_string())?;

    if user_version == SQLITE_SCHEMA_VERSION && schema_version == SQLITE_SCHEMA_VERSION {
        return validate_v8_database(connection);
    }
    if user_version != SQLITE_V7_SCHEMA_VERSION || schema_version != SQLITE_V7_SCHEMA_VERSION {
        return Err(format!(
            "Schema 8 migration requires matching v7 versions; found user_version={user_version}, schema_version={schema_version}"
        ));
    }
    // The two Hub Sort auxiliary tables are part of the schema 8 signature
    // (D-063 §9.1 item 6) but are created outside the version chain, so refuse
    // to migrate a v7 database that has not had them installed yet rather than
    // baking a half-shaped signature into v8.
    for table in ["collection_paper_order", "collection_sort_prefs"] {
        let exists: i64 = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
                 )",
                params![table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists == 0 {
            return Err(format!(
                "Schema 8 migration requires the Hub Sort table {table} to exist first"
            ));
        }
    }

    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    execute_v8_step(
        &transaction,
        injected_fault,
        V8MigrationFault::RevisionTables,
        r#"
        CREATE TABLE library_change_seq (
          singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
          value INTEGER NOT NULL CHECK (value >= 0)
        );

        CREATE TABLE library_domain_revisions (
          domain TEXT PRIMARY KEY CHECK (
            domain IN ('structure','tags','lifecycle','engagement',
                       'artifacts','jobs','smart_collections','sort')
          ),
          value INTEGER NOT NULL CHECK (value >= 0)
        );

        INSERT INTO library_change_seq(singleton, value) VALUES (1, 0);
        INSERT INTO library_domain_revisions(domain, value) VALUES
          ('structure', 0),
          ('tags', 0),
          ('lifecycle', 0),
          ('engagement', 0),
          ('artifacts', 0),
          ('jobs', 0),
          ('smart_collections', 0),
          ('sort', 0);
        "#,
    )?;
    execute_v8_step(
        &transaction,
        injected_fault,
        V8MigrationFault::ReceiptsTable,
        r#"
        CREATE TABLE library_action_receipts (
          idempotency_key TEXT PRIMARY KEY,
          request_digest TEXT NOT NULL,
          result_kind TEXT NOT NULL CHECK (
            result_kind IN ('change','plan','start','control','no_op')
          ),
          result_ref TEXT,
          result_json TEXT,
          created_at TEXT NOT NULL
        );
        "#,
    )?;
    execute_v8_step(
        &transaction,
        injected_fault,
        V8MigrationFault::LifecycleTables,
        r#"
        CREATE TABLE paper_lifecycle (
          paper_id TEXT PRIMARY KEY REFERENCES papers(id) ON DELETE CASCADE,
          status TEXT NOT NULL CHECK (status IN ('unread','reading','read')),
          favorite INTEGER NOT NULL CHECK (favorite IN (0,1)),
          priority INTEGER NOT NULL CHECK (priority BETWEEN 0 AND 3),
          read_later INTEGER NOT NULL CHECK (read_later IN (0,1)),
          review_at TEXT,
          status_changed_at TEXT NOT NULL,
          completed_at TEXT,
          version INTEGER NOT NULL,
          updated_at TEXT NOT NULL
        );

        CREATE TABLE reading_engagement (
          paper_id TEXT NOT NULL REFERENCES papers(id) ON DELETE CASCADE,
          revision_id TEXT NOT NULL REFERENCES document_revisions(id) ON DELETE CASCADE,
          furthest_page INTEGER NOT NULL,
          page_count_snapshot INTEGER NOT NULL,
          first_opened_at TEXT NOT NULL,
          last_opened_at TEXT NOT NULL,
          updated_at TEXT NOT NULL,
          PRIMARY KEY (paper_id, revision_id)
        );
        "#,
    )?;
    execute_v8_step(
        &transaction,
        injected_fault,
        V8MigrationFault::SmartCollectionsTable,
        r#"
        CREATE TABLE smart_collections (
          id TEXT PRIMARY KEY,
          name TEXT NOT NULL,
          query_version INTEGER NOT NULL CHECK (query_version >= 1),
          query_json TEXT NOT NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        "#,
    )?;
    execute_v8_step(
        &transaction,
        injected_fault,
        V8MigrationFault::JobPreparationsTable,
        r#"
        CREATE TABLE job_preparations (
          id TEXT PRIMARY KEY,
          spec_digest TEXT NOT NULL,
          frozen_route_json TEXT NOT NULL,
          state TEXT NOT NULL CHECK (
            state IN ('prepared','consumed','released','expired')
          ),
          expires_at TEXT NOT NULL,
          consumed_job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
          created_at TEXT NOT NULL,
          updated_at TEXT NOT NULL
        );
        "#,
    )?;
    execute_v8_step(
        &transaction,
        injected_fault,
        V8MigrationFault::BatchTables,
        r#"
        CREATE TABLE library_batches (
          id TEXT PRIMARY KEY,
          command_kind TEXT NOT NULL CHECK (
            command_kind IN ('import','move','patch_tags','trash','export',
                             'ocr','brief','patch_lifecycle','retry','compensation')
          ),
          command_json TEXT NOT NULL,
          state TEXT NOT NULL CHECK (
            state IN ('planned','queued','running','paused','action_required',
                      'interrupted_unknown','completed','completed_with_errors',
                      'failed','cancelled')
          ),
          plan_digest TEXT NOT NULL,
          target_digest TEXT NOT NULL,
          cost_preview_json TEXT NOT NULL,
          approval_json TEXT,
          parent_batch_id TEXT REFERENCES library_batches(id) ON DELETE SET NULL,
          relation TEXT CHECK (relation IS NULL OR relation IN ('retry','compensation')),
          undo_policy TEXT NOT NULL CHECK (
            undo_policy IN ('none','full','compensating','cancel_only')
          ),
          plan_expires_at TEXT,
          cancel_requested_at TEXT,
          created_at TEXT NOT NULL,
          started_at TEXT,
          finished_at TEXT,
          updated_at TEXT NOT NULL
        );
        "#,
    )?;
    execute_v8_step(
        &transaction,
        injected_fault,
        V8MigrationFault::BatchItemTables,
        r#"
        CREATE TABLE library_batch_items (
          id TEXT PRIMARY KEY,
          batch_id TEXT NOT NULL REFERENCES library_batches(id) ON DELETE CASCADE,
          source_item_id TEXT REFERENCES library_batch_items(id) ON DELETE SET NULL,
          ordinal INTEGER NOT NULL,
          target_kind TEXT NOT NULL CHECK (target_kind IN ('paper','source')),
          target_key TEXT NOT NULL,
          dedupe_key TEXT,
          paper_id TEXT REFERENCES papers(id) ON DELETE SET NULL,
          revision_id TEXT REFERENCES document_revisions(id) ON DELETE SET NULL,
          prepared_job_handle TEXT REFERENCES job_preparations(id) ON DELETE SET NULL,
          precondition_digest TEXT NOT NULL,
          state TEXT NOT NULL CHECK (
            state IN ('planned','queued','running','paused','action_required',
                      'interrupted_unknown','succeeded','failed','skipped','cancelled')
          ),
          plan_json TEXT NOT NULL,
          result_json TEXT,
          compensation_json TEXT,
          error_code TEXT,
          error_summary TEXT,
          attempt_count INTEGER NOT NULL DEFAULT 0 CHECK (attempt_count >= 0),
          started_at TEXT,
          finished_at TEXT,
          updated_at TEXT NOT NULL,
          UNIQUE (batch_id, ordinal),
          UNIQUE (batch_id, target_key)
        );
        "#,
    )?;
    execute_v8_step(
        &transaction,
        injected_fault,
        V8MigrationFault::JobLinkTable,
        r#"
        CREATE TABLE library_batch_job_links (
          id TEXT PRIMARY KEY,
          batch_item_id TEXT NOT NULL REFERENCES library_batch_items(id) ON DELETE CASCADE,
          job_id TEXT REFERENCES jobs(id) ON DELETE SET NULL,
          ownership TEXT NOT NULL CHECK (ownership IN ('created','joined')),
          consumer_state TEXT NOT NULL CHECK (
            consumer_state IN ('active','completed','detached')
          ),
          cost_attribution TEXT NOT NULL CHECK (
            cost_attribution IN ('creator','shared_no_incremental')
          ),
          usage_receipt_id TEXT,
          job_snapshot_json TEXT NOT NULL,
          created_at TEXT NOT NULL,
          detached_at TEXT,
          UNIQUE (batch_item_id, job_id),
          CHECK (
            (ownership = 'created' AND cost_attribution = 'creator') OR
            (ownership = 'joined' AND cost_attribution = 'shared_no_incremental')
          ),
          CHECK (consumer_state != 'active' OR job_id IS NOT NULL)
        );

        CREATE UNIQUE INDEX one_created_owner_per_job
        ON library_batch_job_links(job_id)
        WHERE ownership = 'created' AND job_id IS NOT NULL;
        "#,
    )?;
    execute_v8_step(
        &transaction,
        injected_fault,
        V8MigrationFault::UndoTokenTable,
        r#"
        CREATE TABLE library_undo_tokens (
          token_hash TEXT PRIMARY KEY,
          batch_id TEXT NOT NULL REFERENCES library_batches(id) ON DELETE CASCADE,
          precondition_digest TEXT NOT NULL,
          state TEXT NOT NULL CHECK (
            state IN ('available','consumed','expired','revoked')
          ),
          expires_at TEXT NOT NULL,
          created_at TEXT NOT NULL,
          consumed_at TEXT
        );
        "#,
    )?;
    execute_v8_step(
        &transaction,
        injected_fault,
        V8MigrationFault::Indexes,
        r#"
        CREATE INDEX idx_paper_lifecycle_status
          ON paper_lifecycle(status, paper_id);

        CREATE INDEX idx_paper_lifecycle_review
          ON paper_lifecycle(review_at, paper_id)
          WHERE review_at IS NOT NULL;

        CREATE INDEX idx_paper_lifecycle_read_later
          ON paper_lifecycle(read_later, paper_id)
          WHERE read_later = 1;

        CREATE INDEX idx_paper_lifecycle_favorite
          ON paper_lifecycle(favorite, paper_id)
          WHERE favorite = 1;

        CREATE INDEX idx_reading_engagement_last_opened
          ON reading_engagement(last_opened_at, paper_id);

        CREATE INDEX idx_library_batches_state_created
          ON library_batches(state, created_at);

        CREATE INDEX idx_library_batches_parent
          ON library_batches(parent_batch_id)
          WHERE parent_batch_id IS NOT NULL;

        CREATE INDEX idx_library_batch_items_state
          ON library_batch_items(batch_id, state, ordinal);

        CREATE INDEX idx_library_batch_items_paper
          ON library_batch_items(paper_id)
          WHERE paper_id IS NOT NULL;

        CREATE INDEX idx_library_batch_items_source
          ON library_batch_items(source_item_id)
          WHERE source_item_id IS NOT NULL;

        CREATE INDEX idx_library_batch_job_links_job
          ON library_batch_job_links(job_id)
          WHERE job_id IS NOT NULL;

        CREATE INDEX idx_library_batch_job_links_item
          ON library_batch_job_links(batch_item_id, consumer_state);

        CREATE INDEX idx_job_preparations_state_expiry
          ON job_preparations(state, expires_at);

        CREATE INDEX idx_library_undo_tokens_batch
          ON library_undo_tokens(batch_id, state);
        "#,
    )?;
    execute_v8_step(
        &transaction,
        injected_fault,
        V8MigrationFault::Versions,
        r#"
        UPDATE schema_meta SET value = '8' WHERE key = 'schema_version';
        PRAGMA user_version = 8;
        "#,
    )?;
    validate_v8_database(&transaction)?;
    transaction.commit().map_err(|error| error.to_string())?;
    validate_v8_database(connection)
}

fn execute_v8_step(
    transaction: &rusqlite::Transaction<'_>,
    injected_fault: Option<V8MigrationFault>,
    step: V8MigrationFault,
    sql: &str,
) -> Result<(), String> {
    transaction
        .execute_batch(sql)
        .map_err(|error| error.to_string())?;
    if injected_fault == Some(step) {
        return Err(format!("injected schema 8 migration fault at {step:?}"));
    }
    Ok(())
}

fn validate_v8_database(connection: &Connection) -> Result<(), String> {
    let user_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let schema_version: String = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if user_version != SQLITE_SCHEMA_VERSION || schema_version != "8" {
        return Err(format!(
            "Schema 8 validation requires matching versions; found user_version={user_version}, schema_version={schema_version}"
        ));
    }
    validate_v7_structure(connection)?;

    for table in [
        "collection_paper_order",
        "collection_sort_prefs",
        "library_change_seq",
        "library_domain_revisions",
        "library_action_receipts",
        "paper_lifecycle",
        "reading_engagement",
        "smart_collections",
        "job_preparations",
        "library_batches",
        "library_batch_items",
        "library_batch_job_links",
        "library_undo_tokens",
    ] {
        let exists: i64 = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
                 )",
                params![table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists == 0 {
            return Err(format!("missing required v8 table: {table}"));
        }
    }
    for (table, column) in [
        ("library_change_seq", "singleton"),
        ("library_change_seq", "value"),
        ("library_domain_revisions", "domain"),
        ("library_domain_revisions", "value"),
        ("library_action_receipts", "request_digest"),
        ("library_action_receipts", "result_kind"),
        ("paper_lifecycle", "status"),
        ("paper_lifecycle", "favorite"),
        ("paper_lifecycle", "priority"),
        ("paper_lifecycle", "read_later"),
        ("paper_lifecycle", "review_at"),
        ("paper_lifecycle", "version"),
        ("reading_engagement", "furthest_page"),
        ("reading_engagement", "page_count_snapshot"),
        ("reading_engagement", "last_opened_at"),
        ("smart_collections", "query_version"),
        ("smart_collections", "query_json"),
        ("job_preparations", "frozen_route_json"),
        ("job_preparations", "state"),
        ("job_preparations", "expires_at"),
        ("job_preparations", "consumed_job_id"),
        ("library_batches", "command_kind"),
        ("library_batches", "state"),
        ("library_batches", "plan_digest"),
        ("library_batches", "parent_batch_id"),
        ("library_batches", "relation"),
        ("library_batches", "undo_policy"),
        ("library_batch_items", "ordinal"),
        ("library_batch_items", "target_key"),
        ("library_batch_items", "source_item_id"),
        ("library_batch_items", "prepared_job_handle"),
        ("library_batch_items", "precondition_digest"),
        ("library_batch_job_links", "ownership"),
        ("library_batch_job_links", "consumer_state"),
        ("library_batch_job_links", "cost_attribution"),
        ("library_batch_job_links", "job_snapshot_json"),
        ("library_undo_tokens", "token_hash"),
        ("library_undo_tokens", "precondition_digest"),
        ("library_undo_tokens", "state"),
        ("library_undo_tokens", "expires_at"),
    ] {
        if !table_has_column(connection, table, column)? {
            return Err(format!("missing required v8 column: {table}.{column}"));
        }
    }

    // The event/selection revision sources must be seeded exactly once.
    let change_seq_rows: i64 = connection
        .query_row("SELECT COUNT(*) FROM library_change_seq", [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;
    if change_seq_rows != 1 {
        return Err(format!(
            "library_change_seq must hold exactly one singleton row, found {change_seq_rows}"
        ));
    }
    let mut domain_rows = connection
        .prepare("SELECT domain, value FROM library_domain_revisions")
        .map_err(|error| error.to_string())?;
    let domains = domain_rows
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    if domains.len() != V8_DOMAINS.len() {
        return Err(format!(
            "library_domain_revisions must hold exactly {} domains, found {}",
            V8_DOMAINS.len(),
            domains.len()
        ));
    }
    for (domain, value) in &domains {
        if !V8_DOMAINS.contains(&domain.as_str()) {
            return Err(format!("unknown library_domain_revisions domain: {domain}"));
        }
        if *value < 0 {
            return Err(format!(
                "library_domain_revisions value is negative for {domain}"
            ));
        }
    }
    for expected in V8_DOMAINS {
        if !domains.iter().any(|(domain, _)| domain == expected) {
            return Err(format!(
                "library_domain_revisions is missing domain: {expected}"
            ));
        }
    }

    for (table, column, referenced_table, referenced_column, on_delete) in [
        ("paper_lifecycle", "paper_id", "papers", "id", "CASCADE"),
        ("reading_engagement", "paper_id", "papers", "id", "CASCADE"),
        (
            "reading_engagement",
            "revision_id",
            "document_revisions",
            "id",
            "CASCADE",
        ),
        (
            "library_batches",
            "parent_batch_id",
            "library_batches",
            "id",
            "SET NULL",
        ),
        (
            "library_batch_items",
            "batch_id",
            "library_batches",
            "id",
            "CASCADE",
        ),
        (
            "library_batch_items",
            "source_item_id",
            "library_batch_items",
            "id",
            "SET NULL",
        ),
        (
            "library_batch_items",
            "paper_id",
            "papers",
            "id",
            "SET NULL",
        ),
        (
            "library_batch_items",
            "revision_id",
            "document_revisions",
            "id",
            "SET NULL",
        ),
        (
            "library_batch_items",
            "prepared_job_handle",
            "job_preparations",
            "id",
            "SET NULL",
        ),
        (
            "library_batch_job_links",
            "batch_item_id",
            "library_batch_items",
            "id",
            "CASCADE",
        ),
        (
            "library_batch_job_links",
            "job_id",
            "jobs",
            "id",
            "SET NULL",
        ),
        (
            "job_preparations",
            "consumed_job_id",
            "jobs",
            "id",
            "SET NULL",
        ),
        (
            "library_undo_tokens",
            "batch_id",
            "library_batches",
            "id",
            "CASCADE",
        ),
    ] {
        validate_foreign_key_contract(
            connection,
            table,
            column,
            referenced_table,
            referenced_column,
            on_delete,
        )?;
    }

    for index in [
        "one_created_owner_per_job",
        "idx_paper_lifecycle_status",
        "idx_paper_lifecycle_review",
        "idx_paper_lifecycle_read_later",
        "idx_paper_lifecycle_favorite",
        "idx_reading_engagement_last_opened",
        "idx_library_batches_state_created",
        "idx_library_batches_parent",
        "idx_library_batch_items_state",
        "idx_library_batch_items_paper",
        "idx_library_batch_items_source",
        "idx_library_batch_job_links_job",
        "idx_library_batch_job_links_item",
        "idx_job_preparations_state_expiry",
        "idx_library_undo_tokens_batch",
    ] {
        let exists: i64 = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1
                 )",
                params![index],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists == 0 {
            return Err(format!("missing required v8 index: {index}"));
        }
    }

    let foreign_key_errors: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;
    if foreign_key_errors != 0 {
        return Err(format!(
            "Schema 8 migration left {foreign_key_errors} foreign key violations"
        ));
    }
    let quick_check: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if !quick_check.eq_ignore_ascii_case("ok") {
        return Err(format!("Schema 8 quick_check failed: {quick_check}"));
    }
    Ok(())
}

fn validate_v7_database(connection: &Connection) -> Result<(), String> {
    let user_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    let schema_version: String = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if user_version != SQLITE_V7_SCHEMA_VERSION || schema_version != "7" {
        return Err(format!(
            "Schema 7 validation requires matching versions; found user_version={user_version}, schema_version={schema_version}"
        ));
    }
    validate_v7_structure(connection)
}

/// Structural contract of schema 7, independent of the version markers.
/// Schema 8 keeps every v7 object (D-063 §9.1 item 6), so its validator runs
/// this list unchanged before checking the new tables.
fn validate_v7_structure(connection: &Connection) -> Result<(), String> {
    for table in [
        "remote_endpoint_snapshots",
        "provider_route_snapshots",
        "job_provider_requirements",
        "remote_tombstones",
    ] {
        let exists: i64 = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1
                 )",
                params![table],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists == 0 {
            return Err(format!("missing required v7 table: {table}"));
        }
    }
    for (table, column) in [
        ("jobs", "provider_route_id"),
        ("jobs", "provider_route_origin"),
        ("context_roots", "provider_route_id"),
        ("provider_nodes", "provider_route_id"),
        ("usage_receipts", "provider_route_id"),
        ("remote_tombstones", "endpoint_scope"),
        ("remote_tombstones", "ownership_status"),
    ] {
        if !table_has_column(connection, table, column)? {
            return Err(format!("missing required v7 column: {table}.{column}"));
        }
    }
    for index in [
        "jobs_active_route_dedupe",
        "jobs_active_legacy_dedupe",
        "jobs_active_legacy_dedupe_lookup",
        "jobs_route_state_updated",
        "context_roots_route_lookup",
        "provider_nodes_route_remote",
        "usage_receipts_route",
        "idx_remote_tombstones_state_updated",
        "remote_tombstones_exact_unique",
        "remote_tombstones_route_state",
    ] {
        let exists: i64 = connection
            .query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1
                 )",
                params![index],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if exists == 0 {
            return Err(format!("missing required v7 index: {index}"));
        }
    }
    let obsolete_index = "idx_jobs_active_dedupe";
    let obsolete_index_exists: i64 = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = ?1
             )",
            params![obsolete_index],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if obsolete_index_exists != 0 {
        return Err(format!(
            "obsolete v7 index must be absent: {obsolete_index}"
        ));
    }

    for (table, column, referenced_table, referenced_column) in [
        (
            "provider_route_snapshots",
            "endpoint_scope",
            "remote_endpoint_snapshots",
            "endpoint_scope",
        ),
        (
            "jobs",
            "provider_route_id",
            "provider_route_snapshots",
            "route_id",
        ),
        (
            "context_roots",
            "provider_route_id",
            "provider_route_snapshots",
            "route_id",
        ),
        (
            "provider_nodes",
            "provider_route_id",
            "provider_route_snapshots",
            "route_id",
        ),
        (
            "usage_receipts",
            "provider_route_id",
            "provider_route_snapshots",
            "route_id",
        ),
        (
            "remote_tombstones",
            "endpoint_scope",
            "remote_endpoint_snapshots",
            "endpoint_scope",
        ),
    ] {
        validate_foreign_key_contract(
            connection,
            table,
            column,
            referenced_table,
            referenced_column,
            "RESTRICT",
        )?;
    }

    for (index, expected_sql) in [
        (
            "jobs_active_route_dedupe",
            "CREATE UNIQUE INDEX jobs_active_route_dedupe
             ON jobs(provider_route_id, dedupe_key)
             WHERE provider_route_id IS NOT NULL
               AND state IN ('queued', 'running', 'paused', 'interrupted_unknown')",
        ),
        (
            "jobs_active_legacy_dedupe",
            "CREATE UNIQUE INDEX jobs_active_legacy_dedupe
             ON jobs(dedupe_key)
             WHERE provider_route_id IS NULL
               AND state IN ('queued', 'running', 'paused')",
        ),
        (
            "jobs_active_legacy_dedupe_lookup",
            "CREATE INDEX jobs_active_legacy_dedupe_lookup
             ON jobs(dedupe_key, state)
             WHERE provider_route_id IS NULL
               AND state IN ('queued', 'running', 'paused', 'interrupted_unknown')",
        ),
        (
            "jobs_route_state_updated",
            "CREATE INDEX jobs_route_state_updated
             ON jobs(provider_route_id, state, updated_at)",
        ),
        (
            "context_roots_route_lookup",
            "CREATE INDEX context_roots_route_lookup
             ON context_roots(revision_id, provider_route_id, model, context_epoch)",
        ),
        (
            "provider_nodes_route_remote",
            "CREATE INDEX provider_nodes_route_remote
             ON provider_nodes(provider_route_id, provider_node_id)",
        ),
        (
            "usage_receipts_route",
            "CREATE INDEX usage_receipts_route
             ON usage_receipts(provider_route_id, created_at)",
        ),
        (
            "idx_remote_tombstones_state_updated",
            "CREATE INDEX idx_remote_tombstones_state_updated
             ON remote_tombstones(state, updated_at DESC)",
        ),
        (
            "remote_tombstones_exact_unique",
            "CREATE UNIQUE INDEX remote_tombstones_exact_unique
             ON remote_tombstones(endpoint_scope, resource_kind, remote_id)
             WHERE endpoint_scope IS NOT NULL",
        ),
        (
            "remote_tombstones_route_state",
            "CREATE INDEX remote_tombstones_route_state
             ON remote_tombstones(endpoint_scope, state, updated_at)",
        ),
    ] {
        validate_index_contract(connection, index, expected_sql)?;
    }

    let foreign_key_errors: i64 = connection
        .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
            row.get(0)
        })
        .map_err(|error| error.to_string())?;
    if foreign_key_errors != 0 {
        return Err(format!(
            "Schema 7 migration left {foreign_key_errors} foreign key violations"
        ));
    }
    let quick_check: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if !quick_check.eq_ignore_ascii_case("ok") {
        return Err(format!("Schema 7 quick_check failed: {quick_check}"));
    }
    Ok(())
}

fn validate_foreign_key_contract(
    connection: &Connection,
    table: &str,
    column: &str,
    expected_table: &str,
    expected_column: &str,
    expected_on_delete: &str,
) -> Result<(), String> {
    let mut statement = connection
        .prepare(
            "SELECT \"table\", \"to\", on_delete
             FROM pragma_foreign_key_list(?1)
             WHERE \"from\" = ?2",
        )
        .map_err(|error| error.to_string())?;
    let contracts = statement
        .query_map(params![table, column], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())?;
    let expected = (
        expected_table.to_string(),
        expected_column.to_string(),
        expected_on_delete.to_string(),
    );
    if contracts.as_slice() != [expected] {
        return Err(format!(
            "workspace foreign key contract mismatch for {table}.{column}: {contracts:?}"
        ));
    }
    Ok(())
}

fn validate_index_contract(
    connection: &Connection,
    index: &str,
    expected_sql: &str,
) -> Result<(), String> {
    let actual_sql: String = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
            params![index],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if normalize_schema_sql(&actual_sql) != normalize_schema_sql(expected_sql) {
        return Err(format!(
            "workspace index contract mismatch for {index}: {actual_sql}"
        ));
    }
    Ok(())
}

fn normalize_schema_sql(sql: &str) -> String {
    sql.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn snapshot_database_file_set(path: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
        ["", "-wal", "-shm", "-journal"]
            .into_iter()
            .map(|suffix| {
                let member = if suffix.is_empty() {
                    path.to_path_buf()
                } else {
                    sqlite_sidecar_path(path, suffix)
                };
                let bytes = member
                    .is_file()
                    .then(|| fs::read(&member).expect("snapshot database file-set member"));
                (member, bytes)
            })
            .collect()
    }

    fn backup_directory_count(path: &Path) -> usize {
        fs::read_dir(path)
            .map(|entries| entries.filter_map(Result::ok).count())
            .unwrap_or(0)
    }

    fn seed_v6_provider_route_graph(connection: &Connection) {
        connection
            .execute_batch(
                r#"
                INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                VALUES ('paper-1', 'collection-root', 'paper.pdf', 'Papers/paper.pdf', '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z');
                INSERT INTO document_revisions(
                  id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at
                ) VALUES ('revision-1', 'paper-1', 'abc123', 42, 1, 'Papers/paper.pdf', '2026-08-24T00:00:00Z');
                INSERT INTO provider_nodes(
                  id, provider, model, context_epoch, provider_node_id, state, created_at
                ) VALUES ('node-1', 'gemini', 'paper-model', 'legacy-epoch', 'remote-node-1', 'ready', '2026-08-24T00:00:00Z');
                INSERT INTO context_roots(
                  id, revision_id, provider, model, context_epoch,
                  provider_file_id, provider_node_id, state, created_at
                ) VALUES (
                  'root-1', 'revision-1', 'gemini', 'paper-model', 'legacy-epoch',
                  'remote-file-1', 'remote-node-1', 'ready', '2026-08-24T00:00:00Z'
                );
                INSERT INTO discussions(
                  id, paper_id, revision_id, title, status, context_root_id, created_at, updated_at
                ) VALUES (
                  'discussion-1', 'paper-1', 'revision-1', 'Fixture', 'active',
                  'root-1', '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z'
                );
                INSERT INTO jobs(
                  id, kind, provider, paper_id, revision_id, dedupe_key, state, stage,
                  provider_committed, priority, payload_json, created_at, updated_at
                ) VALUES (
                  'job-interrupted', 'artifact', 'gemini', 'paper-1', 'revision-1',
                  'same-logical-work', 'interrupted_unknown', 'interrupted_unknown',
                  1, 0, '{"paperModel":"paper-model"}',
                  '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z'
                );
                INSERT INTO jobs(
                  id, kind, provider, paper_id, revision_id, dedupe_key, state, stage,
                  provider_committed, priority, payload_json, created_at, updated_at
                ) VALUES (
                  'job-queued', 'artifact', 'gemini', 'paper-1', 'revision-1',
                  'same-logical-work', 'queued', 'queued', 0, 0,
                  '{"paperModel":"paper-model"}',
                  '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z'
                );
                INSERT INTO job_attempts(
                  id, job_id, attempt_number, state, provider_request_id, started_at,
                  finished_at, error_code, error_detail
                ) VALUES (
                  'attempt-1', 'job-interrupted', 1, 'interrupted', 'request-1',
                  '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z',
                  'network_lost', 'safe detail'
                );
                INSERT INTO usage_receipts(
                  id, operation_id, job_id, provider, model, context_epoch,
                  input_tokens, output_tokens, created_at
                ) VALUES (
                  'receipt-1', 'operation-1', 'job-interrupted', 'gemini',
                  'paper-model', 'legacy-epoch', 10, 5, '2026-08-24T00:00:00Z'
                );
                INSERT INTO remote_tombstones(
                  id, provider, resource_kind, remote_id, paper_id, state,
                  attempts, last_error, created_at, updated_at
                ) VALUES (
                  'tombstone-1', 'gemini', 'file', 'remote-file-1', 'paper-1',
                  'pending', 3, 'safe cleanup error',
                  '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z'
                );
                "#,
            )
            .expect("seed complete v6 graph");
    }

    fn assert_seeded_provider_route_graph_is_preserved(connection: &Connection) {
        for (label, sql, expected) in [
            (
                "paper",
                "SELECT COUNT(*) FROM papers WHERE id = 'paper-1'",
                1_i64,
            ),
            (
                "revision",
                "SELECT COUNT(*) FROM document_revisions WHERE id = 'revision-1'",
                1,
            ),
            (
                "provider node",
                "SELECT COUNT(*) FROM provider_nodes WHERE id = 'node-1'",
                1,
            ),
            (
                "context root",
                "SELECT COUNT(*) FROM context_roots WHERE id = 'root-1'",
                1,
            ),
            (
                "discussion",
                "SELECT COUNT(*) FROM discussions WHERE id = 'discussion-1'",
                1,
            ),
            (
                "jobs",
                "SELECT COUNT(*) FROM jobs
                 WHERE id IN ('job-interrupted', 'job-queued')",
                2,
            ),
            (
                "job attempt",
                "SELECT COUNT(*) FROM job_attempts WHERE id = 'attempt-1'",
                1,
            ),
            (
                "usage receipt",
                "SELECT COUNT(*) FROM usage_receipts WHERE id = 'receipt-1'",
                1,
            ),
            (
                "tombstone",
                "SELECT COUNT(*) FROM remote_tombstones WHERE id = 'tombstone-1'",
                1,
            ),
        ] {
            assert_eq!(
                connection
                    .query_row(sql, [], |row| row.get::<_, i64>(0))
                    .unwrap_or_else(|error| panic!("{label} graph query failed: {error}")),
                expected,
                "{label} row was not preserved"
            );
        }
    }

    #[test]
    fn production_initializer_creates_a_complete_schema_8_database() {
        let root = tempdir().expect("temporary workspace");
        let database_path = root.path().join("workspace.sqlite3");

        initialize_database(&database_path).expect("initialize production database");

        let connection = db::open(&database_path).expect("open initialized database");
        assert_eq!(SQLITE_SCHEMA_VERSION, 8);
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("user version"),
            8
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("schema version"),
            "8"
        );
        validate_v8_database(&connection).expect("complete v8 contract");
    }

    #[test]
    fn production_initializer_backs_up_and_preserves_exact_v6_data() {
        let root = tempdir().expect("temporary workspace");
        let database_path = root.path().join("workspace.sqlite3");
        initialize_v6_database(&database_path).expect("initialize v6 fixture");
        let connection = db::open(&database_path).expect("open v6 fixture");
        seed_v6_provider_route_graph(&connection);
        drop(connection);

        initialize_database(&database_path).expect("activate schema 8");

        let connection = db::open(&database_path).expect("open migrated database");
        validate_v8_database(&connection).expect("migrated v8 contract");
        assert_seeded_provider_route_graph_is_preserved(&connection);
        drop(connection);

        let backup_root = root.path().join("backups");
        let backup_dirs = fs::read_dir(&backup_root)
            .expect("backup root")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        // Every real migration step gets its own restorable copy.
        assert_eq!(backup_dirs.len(), 2, "backup labels: {backup_dirs:?}");
        let backup_path_for = |label: &str| -> PathBuf {
            let name = backup_dirs
                .iter()
                .find(|name| name.starts_with(label))
                .unwrap_or_else(|| panic!("missing {label} backup in {backup_dirs:?}"));
            backup_root.join(name).join("workspace.sqlite3")
        };

        for label in ["pre-schema-7", "pre-schema-8"] {
            let backup_path = backup_path_for(label);
            assert!(backup_path.is_file(), "{label} database copy exists");
            assert!(!sqlite_sidecar_path(&backup_path, "-wal").exists());
            assert!(!sqlite_sidecar_path(&backup_path, "-shm").exists());
        }

        let backup = db::open(backup_path_for("pre-schema-7")).expect("open pre-schema-7 backup");
        assert_eq!(
            backup
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("backup user version"),
            6
        );
        assert_eq!(
            backup
                .query_row(
                    "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("backup schema version"),
            "6"
        );
        assert_seeded_provider_route_graph_is_preserved(&backup);
        drop(backup);

        let v7_backup =
            db::open(backup_path_for("pre-schema-8")).expect("open pre-schema-8 backup");
        assert_eq!(
            v7_backup
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("intermediate user version"),
            7
        );
        assert_eq!(
            v7_backup
                .query_row(
                    "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("intermediate schema version"),
            "7"
        );
        assert_seeded_provider_route_graph_is_preserved(&v7_backup);
    }

    #[test]
    fn reopening_exact_v8_only_validates_without_writing_or_backing_up() {
        let root = tempdir().expect("temporary workspace");
        let database_path = root.path().join("workspace.sqlite3");
        initialize_database(&database_path).expect("initialize v8 database");
        let connection = db::open(&database_path).expect("open v8 fixture");
        connection
            .execute(
                "INSERT INTO schema_meta(key, value) VALUES ('reopen-marker', 'kept')",
                [],
            )
            .expect("seed reopen marker");
        drop(connection);
        let before = snapshot_database_file_set(&database_path);
        let backup_root = root.path().join("backups");

        initialize_database(&database_path).expect("reopen exact v8");

        assert_eq!(snapshot_database_file_set(&database_path), before);
        assert_eq!(backup_directory_count(&backup_root), 0);
        let connection = db::open(&database_path).expect("open validated v8 database");
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM schema_meta WHERE key = 'reopen-marker'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("preserved reopen marker"),
            "kept"
        );
    }

    #[test]
    fn mixed_future_and_incomplete_v8_databases_fail_closed_without_file_mutation() {
        for (label, damage) in [
            (
                "meta-7-user-8",
                "UPDATE schema_meta SET value = '7' WHERE key = 'schema_version';",
            ),
            ("meta-8-user-7", "PRAGMA user_version = 7;"),
            (
                "future-9",
                "UPDATE schema_meta SET value = '9' WHERE key = 'schema_version'; PRAGMA user_version = 9;",
            ),
            ("incomplete-v7", "DROP TABLE job_provider_requirements;"),
            ("missing-hub-sort", "DROP TABLE collection_paper_order;"),
        ] {
            let root = tempdir().expect("temporary workspace");
            let database_path = root.path().join("workspace.sqlite3");
            initialize_database(&database_path).expect("initialize v8 fixture");
            let connection = db::open(&database_path).expect("open v8 fixture");
            connection
                .execute_batch(damage)
                .unwrap_or_else(|error| panic!("damage {label} fixture: {error}"));
            drop(connection);

            let before = snapshot_database_file_set(&database_path);
            let backup_root = root.path().join("backups");
            let error = match initialize_database(&database_path) {
                Ok(()) => panic!("{label} database must fail closed"),
                Err(error) => error,
            };
            assert!(
                error.contains("Schema")
                    || error.contains("schema")
                    || error.contains("v7")
                    || error.contains("v8"),
                "unexpected {label} error: {error}"
            );
            assert_eq!(
                snapshot_database_file_set(&database_path),
                before,
                "{label} database file set changed"
            );
            assert_eq!(
                backup_directory_count(&backup_root),
                0,
                "{label} database was moved or backed up"
            );
        }
    }

    #[test]
    fn explicit_v6_fixture_migration_preserves_graph() {
        let root = tempdir().expect("temporary workspace");
        let database_path = root.path().join("workspace.sqlite3");
        initialize_v6_database(&database_path).expect("initialize v6 fixture");
        let mut connection = db::open(&database_path).expect("open v6 fixture");

        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("v6 user version"),
            6
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = 'provider_route_snapshots'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("v6 fixture route table"),
            0
        );

        seed_v6_provider_route_graph(&connection);
        let rootpage_before: i64 = connection
            .query_row(
                "SELECT rootpage FROM sqlite_master
                 WHERE type = 'table' AND name = 'context_roots'",
                [],
                |row| row.get(0),
            )
            .expect("context rootpage before");

        migrate_v6_to_v7(&mut connection).expect("explicit dormant migration");
        assert_seeded_provider_route_graph_is_preserved(&connection);

        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("v7 user version"),
            7
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("v7 schema version"),
            "7"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT rootpage FROM sqlite_master
                     WHERE type = 'table' AND name = 'context_roots'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("context rootpage after"),
            rootpage_before
        );
        assert!(
            table_has_column(&connection, "context_roots", "provider_route_id")
                .expect("context route column")
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT context_root_id FROM discussions WHERE id = 'discussion-1'",
                    [],
                    |row| row.get::<_, Option<String>>(0),
                )
                .expect("discussion root")
                .as_deref(),
            Some("root-1")
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get::<_, i64>(0))
                .expect("job count"),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT id, provider, resource_kind, remote_id, paper_id, state,
                            attempts, last_error, created_at, updated_at,
                            endpoint_scope, ownership_status
                     FROM remote_tombstones WHERE id = 'tombstone-1'",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, String>(1)?,
                            row.get::<_, String>(2)?,
                            row.get::<_, String>(3)?,
                            row.get::<_, Option<String>>(4)?,
                            row.get::<_, String>(5)?,
                            row.get::<_, i64>(6)?,
                            row.get::<_, Option<String>>(7)?,
                            row.get::<_, String>(8)?,
                            row.get::<_, String>(9)?,
                            row.get::<_, Option<String>>(10)?,
                            row.get::<_, String>(11)?,
                        ))
                    },
                )
                .expect("migrated tombstone"),
            (
                "tombstone-1".to_string(),
                "gemini".to_string(),
                "file".to_string(),
                "remote-file-1".to_string(),
                Some("paper-1".to_string()),
                "pending".to_string(),
                3,
                Some("safe cleanup error".to_string()),
                "2026-08-24T00:00:00Z".to_string(),
                "2026-08-24T00:00:00Z".to_string(),
                None,
                "legacy_unattributed".to_string(),
            )
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get::<_, i64>(0)
                },)
                .expect("foreign key check"),
            0
        );
        assert_eq!(
            connection
                .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
                .expect("quick check"),
            "ok"
        );
    }

    #[test]
    fn critical_v7_ddl_faults_roll_back_schema_and_versions() {
        for fault in V7MigrationFault::ALL {
            let root = tempdir().expect("temporary workspace");
            let database_path = root.path().join("workspace.sqlite3");
            initialize_v6_database(&database_path).expect("initialize v6 fixture");
            let mut connection = db::open(&database_path).expect("open v6 fixture");
            seed_v6_provider_route_graph(&connection);

            let error = migrate_v6_to_v7_with_fault(&mut connection, Some(fault))
                .expect_err("fault must roll back migration");
            assert_seeded_provider_route_graph_is_preserved(&connection);
            assert!(
                error.contains("injected schema 7 migration fault"),
                "unexpected error for {fault:?}: {error}"
            );
            assert_eq!(
                connection
                    .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                    .expect("rolled back user version"),
                6,
                "user version changed after {fault:?}"
            );
            assert_eq!(
                connection
                    .query_row(
                        "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                        [],
                        |row| row.get::<_, String>(0),
                    )
                    .expect("rolled back schema version"),
                "6",
                "schema version changed after {fault:?}"
            );
            assert_eq!(
                connection
                    .query_row(
                        "SELECT COUNT(*) FROM sqlite_master
                         WHERE name IN (
                           'remote_endpoint_snapshots',
                           'provider_route_snapshots',
                           'job_provider_requirements',
                           'remote_tombstones_v7'
                         )",
                        [],
                        |row| row.get::<_, i64>(0),
                    )
                    .expect("rolled back v7 objects"),
                0,
                "v7 object survived {fault:?}"
            );
            assert!(
                !table_has_column(&connection, "jobs", "provider_route_id")
                    .expect("rolled back jobs column"),
                "jobs column survived {fault:?}"
            );
            assert!(
                !table_has_column(&connection, "context_roots", "provider_route_id")
                    .expect("rolled back context column"),
                "context column survived {fault:?}"
            );
            assert!(
                !table_has_column(&connection, "remote_tombstones", "ownership_status")
                    .expect("rolled back tombstone column"),
                "tombstone rebuild survived {fault:?}"
            );
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM jobs", [], |row| row.get::<_, i64>(0))
                    .expect("preserved jobs"),
                2
            );
            assert_eq!(
                connection
                    .query_row(
                        "SELECT context_root_id FROM discussions WHERE id = 'discussion-1'",
                        [],
                        |row| row.get::<_, Option<String>>(0),
                    )
                    .expect("preserved discussion")
                    .as_deref(),
                Some("root-1")
            );
            assert_eq!(
                connection
                    .query_row(
                        "SELECT attempts FROM remote_tombstones WHERE id = 'tombstone-1'",
                        [],
                        |row| row.get::<_, i64>(0),
                    )
                    .expect("preserved tombstone"),
                3
            );
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                        row.get::<_, i64>(0)
                    },)
                    .expect("foreign key check"),
                0
            );
            assert_eq!(
                connection
                    .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
                    .expect("quick check"),
                "ok"
            );
        }
    }

    fn initialize_v7_fixture(path: &Path) {
        initialize_v6_database(path).expect("initialize v6 fixture");
        migrate_database_to_schema_7(path, false).expect("migrate fixture to v7");
        let connection = db::open(path).expect("open v7 fixture");
        ensure_hub_sort_tables(&connection).expect("install Hub Sort tables");
    }

    const V8_NEW_TABLES: [&str; 11] = [
        "library_change_seq",
        "library_domain_revisions",
        "library_action_receipts",
        "paper_lifecycle",
        "reading_engagement",
        "smart_collections",
        "job_preparations",
        "library_batches",
        "library_batch_items",
        "library_batch_job_links",
        "library_undo_tokens",
    ];

    fn sqlite_object_count(connection: &Connection, kind: &str, name: &str) -> i64 {
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = ?1 AND name = ?2",
                params![kind, name],
                |row| row.get::<_, i64>(0),
            )
            .expect("count sqlite object")
    }

    #[test]
    fn schema_8_migration_refuses_v7_without_hub_sort_tables() {
        for missing_table in ["collection_paper_order", "collection_sort_prefs"] {
            let root = tempdir().expect("temporary workspace");
            let database_path = root.path().join("workspace.sqlite3");
            initialize_v7_fixture(&database_path);
            let mut connection = db::open(&database_path).expect("open v7 fixture");
            // A v7 workspace written before Hub Sort existed looks exactly like
            // this: current versions, one auxiliary table absent.
            connection
                .execute_batch(&format!("DROP TABLE {missing_table};"))
                .expect("drop hub sort table");

            let error = migrate_v7_to_v8(&mut connection)
                .expect_err("v7 without Hub Sort tables must not migrate");
            assert!(
                error.contains("Schema 8 migration requires the Hub Sort table")
                    && error.contains(missing_table),
                "unexpected error for {missing_table}: {error}"
            );
            assert_eq!(
                connection
                    .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                    .expect("user version"),
                7,
                "refused migration bumped user_version"
            );
            assert_eq!(
                connection
                    .query_row(
                        "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                        [],
                        |row| row.get::<_, String>(0),
                    )
                    .expect("schema version"),
                "7",
                "refused migration bumped schema_version"
            );
            for table in V8_NEW_TABLES {
                assert_eq!(
                    sqlite_object_count(&connection, "table", table),
                    0,
                    "refused migration created v8 table {table}"
                );
            }
        }
    }

    #[test]
    fn critical_v8_ddl_faults_roll_back_schema_and_new_tables() {
        for fault in V8MigrationFault::ALL {
            let root = tempdir().expect("temporary workspace");
            let database_path = root.path().join("workspace.sqlite3");
            initialize_v7_fixture(&database_path);
            let mut connection = db::open(&database_path).expect("open v7 fixture");
            seed_v6_provider_route_graph(&connection);

            let error = migrate_v7_to_v8_with_fault(&mut connection, Some(fault))
                .expect_err("fault must roll back the schema 8 migration");
            assert!(
                error.contains("injected schema 8 migration fault"),
                "unexpected error for {fault:?}: {error}"
            );
            assert_eq!(
                connection
                    .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                    .expect("rolled back user version"),
                7,
                "user version changed after {fault:?}"
            );
            assert_eq!(
                connection
                    .query_row(
                        "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                        [],
                        |row| row.get::<_, String>(0),
                    )
                    .expect("rolled back schema version"),
                "7",
                "schema version changed after {fault:?}"
            );
            for table in V8_NEW_TABLES {
                assert_eq!(
                    sqlite_object_count(&connection, "table", table),
                    0,
                    "v8 table {table} survived {fault:?}"
                );
            }
            for index in [
                "one_created_owner_per_job",
                "idx_paper_lifecycle_status",
                "idx_library_batch_items_state",
                "idx_library_undo_tokens_batch",
            ] {
                assert_eq!(
                    sqlite_object_count(&connection, "index", index),
                    0,
                    "v8 index {index} survived {fault:?}"
                );
            }
            // Schema 8 is purely additive, so a rolled back attempt must leave
            // every pre-existing row exactly as it was.
            assert_seeded_provider_route_graph_is_preserved(&connection);
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                        row.get::<_, i64>(0)
                    },)
                    .expect("foreign key check"),
                0,
                "rolled back {fault:?} left foreign key violations"
            );
            assert_eq!(
                connection
                    .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
                    .expect("quick check"),
                "ok",
                "rolled back {fault:?} left a corrupt database"
            );
        }
    }

    #[test]
    fn schema_8_constraints_reject_invalid_rows_and_cascade_batch_delete() {
        let root = tempdir().expect("temporary workspace");
        let database_path = root.path().join("workspace.sqlite3");
        initialize_v7_fixture(&database_path);
        let mut connection = db::open(&database_path).expect("open v7 fixture");
        seed_v6_provider_route_graph(&connection);
        migrate_v7_to_v8(&mut connection).expect("migrate fixture to v8");

        connection
            .execute_batch(
                r#"
                INSERT INTO library_batches(
                  id, command_kind, command_json, state, plan_digest, target_digest,
                  cost_preview_json, undo_policy, created_at, updated_at
                ) VALUES
                  ('batch-1','import','{}','planned','plan-1','target-1','{}','full',
                   '2026-08-31T00:00:00Z','2026-08-31T00:00:00Z'),
                  ('batch-2','retry','{}','running','plan-2','target-2','{}','compensating',
                   '2026-08-31T00:00:00Z','2026-08-31T00:00:00Z');
                INSERT INTO job_preparations(
                  id, spec_digest, frozen_route_json, state, expires_at, created_at, updated_at
                ) VALUES
                  ('prep-1','digest-1','{}','prepared','2026-08-31T00:10:00Z',
                   '2026-08-31T00:00:00Z','2026-08-31T00:00:00Z');
                INSERT INTO library_batch_items(
                  id, batch_id, ordinal, target_kind, target_key, paper_id, revision_id,
                  prepared_job_handle, precondition_digest, state, plan_json, updated_at
                ) VALUES
                  ('item-1','batch-1',0,'paper','paper-1','paper-1','revision-1','prep-1',
                   'digest-1','planned','{}','2026-08-31T00:00:00Z');
                INSERT INTO library_batch_job_links(
                  id, batch_item_id, job_id, ownership, consumer_state, cost_attribution,
                  job_snapshot_json, created_at
                ) VALUES
                  ('link-1','item-1','job-queued','created','active','creator','{}',
                   '2026-08-31T00:00:00Z');
                INSERT INTO library_undo_tokens(
                  token_hash, batch_id, precondition_digest, state, expires_at, created_at
                ) VALUES
                  ('token-1','batch-1','digest-1','available','2026-08-31T00:10:00Z',
                   '2026-08-31T00:00:00Z');
                INSERT INTO paper_lifecycle(
                  paper_id, status, favorite, priority, read_later, status_changed_at,
                  version, updated_at
                ) VALUES
                  ('paper-1','unread',0,0,0,'2026-08-31T00:00:00Z',0,'2026-08-31T00:00:00Z');
                INSERT INTO reading_engagement(
                  paper_id, revision_id, furthest_page, page_count_snapshot,
                  first_opened_at, last_opened_at, updated_at
                ) VALUES
                  ('paper-1','revision-1',3,10,'2026-08-31T00:00:00Z',
                   '2026-08-31T00:00:00Z','2026-08-31T00:00:00Z');
                "#,
            )
            .expect("seed valid v8 rows");
        validate_v8_database(&connection).expect("valid v8 rows keep the schema contract");

        for (label, sql, expected) in [
            (
                "unknown command_kind",
                "INSERT INTO library_batches(
                   id, command_kind, command_json, state, plan_digest, target_digest,
                   cost_preview_json, undo_policy, created_at, updated_at
                 ) VALUES
                   ('bad-batch','teleport','{}','planned','p','t','{}','full',
                    '2026-08-31T00:00:00Z','2026-08-31T00:00:00Z')",
                "CHECK",
            ),
            (
                "unknown batch state",
                "UPDATE library_batches SET state = 'on_fire' WHERE id = 'batch-2'",
                "CHECK",
            ),
            (
                "duplicate ordinal inside a batch",
                "INSERT INTO library_batch_items(
                   id, batch_id, ordinal, target_kind, target_key, precondition_digest,
                   state, plan_json, updated_at
                 ) VALUES
                   ('item-dup-ordinal','batch-1',0,'source','other.pdf','digest',
                    'planned','{}','2026-08-31T00:00:00Z')",
                "UNIQUE",
            ),
            (
                "duplicate target_key inside a batch",
                "INSERT INTO library_batch_items(
                   id, batch_id, ordinal, target_kind, target_key, precondition_digest,
                   state, plan_json, updated_at
                 ) VALUES
                   ('item-dup-target','batch-1',7,'paper','paper-1','digest',
                    'planned','{}','2026-08-31T00:00:00Z')",
                "UNIQUE",
            ),
            (
                "batch item without a batch",
                "INSERT INTO library_batch_items(
                   id, batch_id, ordinal, target_kind, target_key, precondition_digest,
                   state, plan_json, updated_at
                 ) VALUES
                   ('item-orphan','missing-batch',0,'source','other.pdf','digest',
                    'planned','{}','2026-08-31T00:00:00Z')",
                "FOREIGN KEY",
            ),
            (
                "created owner without creator attribution",
                "INSERT INTO library_batch_job_links(
                   id, batch_item_id, job_id, ownership, consumer_state, cost_attribution,
                   job_snapshot_json, created_at
                 ) VALUES
                   ('link-bad-cost','item-1','job-interrupted','created','active',
                    'shared_no_incremental','{}','2026-08-31T00:00:00Z')",
                "CHECK",
            ),
            (
                "active consumer without a job",
                "INSERT INTO library_batch_job_links(
                   id, batch_item_id, job_id, ownership, consumer_state, cost_attribution,
                   job_snapshot_json, created_at
                 ) VALUES
                   ('link-no-job','item-1',NULL,'joined','active','shared_no_incremental',
                    '{}','2026-08-31T00:00:00Z')",
                "CHECK",
            ),
            (
                "second created owner for one job",
                "INSERT INTO library_batch_job_links(
                   id, batch_item_id, job_id, ownership, consumer_state, cost_attribution,
                   job_snapshot_json, created_at
                 ) VALUES
                   ('link-dup-owner','item-1','job-queued','created','active','creator',
                    '{}','2026-08-31T00:00:00Z')",
                "UNIQUE",
            ),
            (
                "unknown revision domain",
                "INSERT INTO library_domain_revisions(domain, value) VALUES ('bogus', 0)",
                "CHECK",
            ),
            (
                "second change_seq singleton row",
                "INSERT INTO library_change_seq(singleton, value) VALUES (2, 0)",
                "CHECK",
            ),
            (
                "negative change_seq value",
                "UPDATE library_change_seq SET value = -1 WHERE singleton = 1",
                "CHECK",
            ),
            (
                "unknown lifecycle status",
                "UPDATE paper_lifecycle SET status = 'paused' WHERE paper_id = 'paper-1'",
                "CHECK",
            ),
            (
                "priority outside 0..3",
                "UPDATE paper_lifecycle SET priority = 9 WHERE paper_id = 'paper-1'",
                "CHECK",
            ),
            (
                "unknown undo token state",
                "UPDATE library_undo_tokens SET state = 'used' WHERE token_hash = 'token-1'",
                "CHECK",
            ),
            (
                "query version below the minimum",
                "INSERT INTO smart_collections(
                   id, name, query_version, query_json, created_at, updated_at
                 ) VALUES
                   ('smart-bad','Bad',0,'{}','2026-08-31T00:00:00Z','2026-08-31T00:00:00Z')",
                "CHECK",
            ),
        ] {
            let error = connection
                .execute_batch(sql)
                .err()
                .unwrap_or_else(|| panic!("{label} must be rejected"));
            assert!(
                error.to_string().contains(expected),
                "{label} rejected by an unexpected rule: {error}"
            );
        }
        validate_v8_database(&connection).expect("rejected writes left the contract intact");

        connection
            .execute("DELETE FROM library_batches WHERE id = 'batch-1'", [])
            .expect("delete batch");
        for (label, sql) in [
            (
                "batch items",
                "SELECT COUNT(*) FROM library_batch_items WHERE batch_id = 'batch-1'",
            ),
            (
                "undo tokens",
                "SELECT COUNT(*) FROM library_undo_tokens WHERE batch_id = 'batch-1'",
            ),
            (
                "job links",
                "SELECT COUNT(*) FROM library_batch_job_links WHERE batch_item_id = 'item-1'",
            ),
        ] {
            assert_eq!(
                connection
                    .query_row(sql, [], |row| row.get::<_, i64>(0))
                    .unwrap_or_else(|error| panic!("{label} query failed: {error}")),
                0,
                "deleting a batch left {label} behind"
            );
        }
        // Prepared handles outlive the batch that consumed them: the link is
        // gone but the preparation row keeps its audit trail.
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM job_preparations", [], |row| {
                    row.get::<_, i64>(0)
                })
                .expect("prepared rows"),
            1
        );
    }

    #[test]
    fn v7_route_indexes_isolate_exact_resources_without_rebuilding_context_roots() {
        let root = tempdir().expect("temporary workspace");
        let database_path = root.path().join("workspace.sqlite3");
        initialize_v6_database(&database_path).expect("initialize v6 fixture");
        let mut connection = db::open(&database_path).expect("open v6 fixture");
        seed_v6_provider_route_graph(&connection);
        migrate_v6_to_v7(&mut connection).expect("migrate fixture");

        connection
            .execute_batch(
                r#"
                INSERT INTO remote_endpoint_snapshots(
                  endpoint_scope, version, owner_type, provider_instance_id,
                  provider_name_at_capture, provider_kind, base_url, created_at
                ) VALUES
                  ('scope-a', 1, 'paper_provider', 'instance-a', 'A', 'gemini',
                   NULL, '2026-08-24T00:00:00Z'),
                  ('scope-b', 1, 'paper_provider', 'instance-b', 'B', 'gemini',
                   NULL, '2026-08-24T00:00:00Z');
                INSERT INTO provider_route_snapshots(
                  route_id, endpoint_scope, version, models_json, operation_role, created_at
                ) VALUES
                  ('route-a', 'scope-a', 1, '{"paper":"paper-model"}', 'paper',
                   '2026-08-24T00:00:00Z'),
                  ('route-b', 'scope-b', 1, '{"paper":"paper-model"}', 'paper',
                   '2026-08-24T00:00:00Z');
                INSERT INTO jobs(
                  id, kind, provider, paper_id, revision_id, dedupe_key, state, stage,
                  provider_committed, priority, payload_json, created_at, updated_at,
                  provider_route_id, provider_route_origin
                ) VALUES
                  ('job-route-a', 'artifact', 'gemini', 'paper-1', 'revision-1',
                   'routed-work', 'queued', 'queued', 0, 0, '{}',
                   '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z',
                   'route-a', 'captured'),
                  ('job-route-b', 'artifact', 'gemini', 'paper-1', 'revision-1',
                   'routed-work', 'queued', 'queued', 0, 0, '{}',
                   '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z',
                   'route-b', 'captured');
                INSERT INTO context_roots(
                  id, revision_id, provider, model, context_epoch, state, created_at,
                  provider_route_id
                ) VALUES
                  ('root-route-a', 'revision-1', 'gemini', 'paper-model',
                   'route-epoch-a', 'ready', '2026-08-24T00:00:00Z', 'route-a'),
                  ('root-route-b', 'revision-1', 'gemini', 'paper-model',
                   'route-epoch-b', 'ready', '2026-08-24T00:00:00Z', 'route-b');
                UPDATE provider_nodes SET provider_route_id = 'route-a' WHERE id = 'node-1';
                UPDATE usage_receipts SET provider_route_id = 'route-a' WHERE id = 'receipt-1';
                INSERT INTO remote_tombstones(
                  id, provider, resource_kind, remote_id, paper_id, state,
                  attempts, created_at, updated_at, endpoint_scope, ownership_status
                ) VALUES
                  ('exact-a', 'gemini', 'file', 'same-remote-id', 'paper-1', 'pending',
                   0, '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z',
                   'scope-a', 'exact'),
                  ('exact-b', 'gemini', 'file', 'same-remote-id', 'paper-1', 'pending',
                   0, '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z',
                   'scope-b', 'exact');
                "#,
            )
            .expect("seed two exact routes");

        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM jobs WHERE dedupe_key = 'routed-work'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("routed jobs"),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM context_roots
                     WHERE id IN ('root-route-a', 'root-route-b')",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("route roots"),
            2
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM remote_tombstones
                     WHERE remote_id = 'same-remote-id'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("endpoint tombstones"),
            2
        );
        assert!(
            connection
                .execute(
                    "INSERT INTO remote_tombstones(
                       id, provider, resource_kind, remote_id, state, created_at, updated_at,
                       endpoint_scope, ownership_status
                     ) VALUES (
                       'exact-a-duplicate', 'gemini', 'file', 'same-remote-id', 'pending',
                       '2026-08-24T00:00:00Z', '2026-08-24T00:00:00Z',
                       'scope-a', 'exact'
                     )",
                    [],
                )
                .is_err(),
            "same endpoint/resource/remote id must remain unique"
        );

        let indexes = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'index'")
            .expect("index query")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("index rows")
            .collect::<rusqlite::Result<std::collections::HashSet<_>>>()
            .expect("index names");
        for expected in [
            "jobs_active_route_dedupe",
            "jobs_active_legacy_dedupe",
            "jobs_active_legacy_dedupe_lookup",
            "jobs_route_state_updated",
            "context_roots_route_lookup",
            "provider_nodes_route_remote",
            "usage_receipts_route",
            "remote_tombstones_exact_unique",
            "remote_tombstones_route_state",
        ] {
            assert!(indexes.contains(expected), "missing v7 index {expected}");
        }
        assert!(
            !indexes.contains("idx_jobs_active_dedupe"),
            "kind-only active dedupe index must be removed"
        );

        let plan = connection
            .prepare(
                "EXPLAIN QUERY PLAN
                 SELECT id FROM jobs
                 WHERE provider_route_id = 'route-a'
                   AND dedupe_key = 'routed-work'
                   AND state IN ('queued', 'running', 'paused', 'interrupted_unknown')",
            )
            .expect("route query plan")
            .query_map([], |row| row.get::<_, String>(3))
            .expect("plan rows")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("plan details");
        assert!(
            plan.iter()
                .any(|detail| detail.contains("jobs_active_route_dedupe")),
            "route lookup should use scoped dedupe index: {plan:?}"
        );

        let legacy_plan = connection
            .prepare(
                "EXPLAIN QUERY PLAN
                 SELECT id FROM jobs
                 WHERE provider_route_id IS NULL
                   AND dedupe_key = 'same-logical-work'
                   AND state IN ('queued', 'running', 'paused', 'interrupted_unknown')",
            )
            .expect("legacy query plan")
            .query_map([], |row| row.get::<_, String>(3))
            .expect("legacy plan rows")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("legacy plan details");
        assert!(
            legacy_plan
                .iter()
                .any(|detail| detail.contains("jobs_active_legacy_dedupe_lookup")),
            "legacy interrupted-aware lookup should use its dedicated index: {legacy_plan:?}"
        );

        assert!(
            connection
                .execute(
                    "DELETE FROM provider_route_snapshots WHERE route_id = 'route-a'",
                    [],
                )
                .is_err(),
            "referenced route snapshot must be RESTRICTed"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT context_root_id FROM discussions WHERE id = 'discussion-1'",
                    [],
                    |row| row.get::<_, Option<String>>(0),
                )
                .expect("legacy discussion root")
                .as_deref(),
            Some("root-1")
        );
    }

    #[test]
    fn v7_validator_rejects_a_named_index_with_the_wrong_contract() {
        let root = tempdir().expect("temporary workspace");
        let database_path = root.path().join("workspace.sqlite3");
        initialize_v6_database(&database_path).expect("initialize v6 fixture");
        let mut connection = db::open(&database_path).expect("open v6 fixture");
        migrate_v6_to_v7(&mut connection).expect("migrate fixture");
        connection
            .execute_batch(
                "DROP INDEX jobs_active_route_dedupe;
                 CREATE INDEX jobs_active_route_dedupe
                   ON jobs(dedupe_key)
                   WHERE state = 'queued';",
            )
            .expect("replace route index with a misleading namesake");

        let error = migrate_v6_to_v7(&mut connection)
            .expect_err("a namesake index with the wrong contract must fail closed");
        assert!(
            error.contains("jobs_active_route_dedupe") && error.contains("index contract"),
            "unexpected structural validation error: {error}"
        );
    }

    #[test]
    fn v7_validator_rejects_any_named_index_with_the_wrong_contract() {
        let root = tempdir().expect("temporary workspace");
        let database_path = root.path().join("workspace.sqlite3");
        initialize_v6_database(&database_path).expect("initialize v6 fixture");
        let mut connection = db::open(&database_path).expect("open v6 fixture");
        migrate_v6_to_v7(&mut connection).expect("migrate fixture");
        connection
            .execute_batch(
                "DROP INDEX remote_tombstones_exact_unique;
                 CREATE INDEX remote_tombstones_exact_unique
                   ON remote_tombstones(remote_id);",
            )
            .expect("replace exact tombstone index with a misleading namesake");

        let error = migrate_v6_to_v7(&mut connection)
            .expect_err("every namesake index with the wrong contract must fail closed");
        assert!(
            error.contains("remote_tombstones_exact_unique") && error.contains("index contract"),
            "unexpected structural validation error: {error}"
        );
    }

    #[test]
    fn v7_validator_rejects_a_route_column_without_restrict_fk() {
        let root = tempdir().expect("temporary workspace");
        let database_path = root.path().join("workspace.sqlite3");
        initialize_v6_database(&database_path).expect("initialize v6 fixture");
        let mut connection = db::open(&database_path).expect("open v6 fixture");
        migrate_v6_to_v7(&mut connection).expect("migrate fixture");
        connection
            .execute_batch(
                "DROP INDEX usage_receipts_route;
                 ALTER TABLE usage_receipts DROP COLUMN provider_route_id;
                 ALTER TABLE usage_receipts ADD COLUMN provider_route_id TEXT;
                 CREATE INDEX usage_receipts_route
                   ON usage_receipts(provider_route_id, created_at);",
            )
            .expect("replace route column without its FK");

        let error = migrate_v6_to_v7(&mut connection)
            .expect_err("a route column without RESTRICT FK must fail closed");
        assert!(
            error.contains("usage_receipts.provider_route_id")
                && error.contains("foreign key contract"),
            "unexpected FK validation error: {error}"
        );
    }

    #[test]
    fn v7_validator_rejects_the_obsolete_kind_only_active_dedupe_index() {
        let root = tempdir().expect("temporary workspace");
        let database_path = root.path().join("workspace.sqlite3");
        initialize_v6_database(&database_path).expect("initialize v6 fixture");
        let mut connection = db::open(&database_path).expect("open v6 fixture");
        migrate_v6_to_v7(&mut connection).expect("migrate fixture");
        connection
            .execute_batch(
                "CREATE UNIQUE INDEX idx_jobs_active_dedupe
                   ON jobs(dedupe_key)
                   WHERE state IN ('queued', 'running', 'paused', 'interrupted_unknown');",
            )
            .expect("reintroduce obsolete kind-only active dedupe index");

        let error = migrate_v6_to_v7(&mut connection)
            .expect_err("the obsolete kind-only active dedupe index must fail closed");
        assert!(
            error.contains("idx_jobs_active_dedupe") && error.contains("obsolete v7 index"),
            "unexpected obsolete-index validation error: {error}"
        );
    }

    #[test]
    fn v7_migration_rejects_user_version_ahead_of_meta_without_mutation() {
        let root = tempdir().expect("mixed-version workspace");
        let database_path = root.path().join("workspace.sqlite3");
        initialize_v6_database(&database_path).expect("initialize mixed fixture");
        let mut connection = db::open(&database_path).expect("open mixed fixture");
        connection
            .execute_batch("PRAGMA user_version = 7;")
            .expect("advance only sqlite user version");

        let error = migrate_v6_to_v7(&mut connection)
            .expect_err("user_version ahead of schema_meta must fail closed");
        assert!(
            error.contains("requires matching v6 versions"),
            "unexpected reverse mixed-version error: {error}"
        );
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("preserved user version"),
            7
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("preserved schema version"),
            "6"
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = 'provider_route_snapshots'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("no partial route table"),
            0
        );
    }

    #[test]
    fn v7_migration_is_idempotent_and_rejects_mixed_or_incomplete_versions() {
        let root = tempdir().expect("temporary workspace");
        let database_path = root.path().join("workspace.sqlite3");
        initialize_v6_database(&database_path).expect("initialize v6 fixture");
        let mut connection = db::open(&database_path).expect("open v6 fixture");
        migrate_v6_to_v7(&mut connection).expect("first migration");
        drop(connection);

        let mut connection = db::open(&database_path).expect("reopen v7 fixture");
        assert_eq!(
            connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("reopened user version"),
            7
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("reopened schema version"),
            "7"
        );

        let schema_objects_before: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE name IN (
                   'remote_endpoint_snapshots',
                   'provider_route_snapshots',
                   'job_provider_requirements',
                   'jobs_active_route_dedupe',
                   'remote_tombstones_exact_unique'
                 )",
                [],
                |row| row.get(0),
            )
            .expect("v7 objects before idempotent run");
        migrate_v6_to_v7(&mut connection).expect("idempotent migration");
        let schema_objects_after: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE name IN (
                   'remote_endpoint_snapshots',
                   'provider_route_snapshots',
                   'job_provider_requirements',
                   'jobs_active_route_dedupe',
                   'remote_tombstones_exact_unique'
                 )",
                [],
                |row| row.get(0),
            )
            .expect("v7 objects after idempotent run");
        assert_eq!(schema_objects_after, schema_objects_before);

        connection
            .execute_batch("DROP TABLE job_provider_requirements;")
            .expect("damage v7 fixture");
        let error = migrate_v6_to_v7(&mut connection)
            .expect_err("incomplete v7 schema must not be accepted");
        assert!(
            error.contains("missing required v7 table"),
            "unexpected incomplete-schema error: {error}"
        );

        let mixed_root = tempdir().expect("mixed-version workspace");
        let mixed_path = mixed_root.path().join("workspace.sqlite3");
        initialize_v6_database(&mixed_path).expect("initialize mixed fixture");
        let mut mixed = db::open(&mixed_path).expect("open mixed fixture");
        mixed
            .execute(
                "UPDATE schema_meta SET value = '7' WHERE key = 'schema_version'",
                [],
            )
            .expect("create mixed versions");
        let error =
            migrate_v6_to_v7(&mut mixed).expect_err("mixed meta/user versions must fail closed");
        assert!(error.contains("requires matching v6 versions"));
        assert_eq!(
            mixed
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("mixed user version"),
            6
        );
        assert_eq!(
            mixed
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = 'provider_route_snapshots'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("mixed route table"),
            0
        );

        let ahead_root = tempdir().expect("ahead-version workspace");
        let ahead_path = ahead_root.path().join("workspace.sqlite3");
        initialize_v6_database(&ahead_path).expect("initialize ahead fixture");
        let mut ahead = db::open(&ahead_path).expect("open ahead fixture");
        ahead
            .execute_batch(
                "UPDATE schema_meta SET value = '8' WHERE key = 'schema_version';
                 PRAGMA user_version = 8;",
            )
            .expect("create ahead-version fixture");
        let error = migrate_v6_to_v7(&mut ahead)
            .expect_err("a newer schema version must never be downgraded");
        assert!(error.contains("requires matching v6 versions"));
        assert_eq!(
            ahead
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("ahead user version"),
            8
        );
        assert_eq!(
            ahead
                .query_row(
                    "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("ahead schema version"),
            "8"
        );
        assert_eq!(
            ahead
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master
                     WHERE type = 'table' AND name = 'provider_route_snapshots'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .expect("ahead route table"),
            0
        );
    }

    #[test]
    fn checkpoint_close_copy_backup_restores_a_self_contained_v6_database() {
        let root = tempdir().expect("temporary workspace");
        let database_path = root.path().join("workspace.sqlite3");
        initialize_v6_database(&database_path).expect("initialize v6 fixture");
        let connection = db::open(&database_path).expect("open v6 fixture");
        connection
            .execute_batch("PRAGMA wal_autocheckpoint = 0;")
            .expect("disable automatic checkpoint");
        seed_v6_provider_route_graph(&connection);
        drop(connection);

        let backup_path = db::checkpoint_close_copy_database(&database_path, "pre-schema-7")
            .expect("create consistent backup");
        assert_ne!(backup_path, database_path);
        assert!(backup_path.is_file());
        assert!(!sqlite_sidecar_path(&backup_path, "-wal").exists());
        assert!(!sqlite_sidecar_path(&backup_path, "-shm").exists());

        let backup_bytes_before = fs::read(&backup_path).expect("snapshot backup database");

        let mutation = db::open(&database_path).expect("open original for mutation");
        mutation
            .execute(
                "UPDATE papers SET file_name = 'mutated.pdf' WHERE id = 'paper-1'",
                [],
            )
            .expect("mutate original after backup");
        drop(mutation);

        db::restore_database_copy(&database_path, &backup_path).expect("restore consistent backup");
        let restored = db::open(&database_path).expect("open restored database");
        assert_eq!(
            restored
                .query_row(
                    "SELECT file_name FROM papers WHERE id = 'paper-1'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .expect("restored fixture row"),
            "paper.pdf"
        );
        assert_eq!(
            restored
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .expect("restored user version"),
            6
        );
        assert_eq!(
            restored
                .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |row| {
                    row.get::<_, i64>(0)
                },)
                .expect("restored foreign key check"),
            0
        );
        assert_eq!(
            restored
                .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
                .expect("restored quick check"),
            "ok"
        );
        drop(restored);
        assert_eq!(
            fs::read(&backup_path).expect("read backup after restore"),
            backup_bytes_before,
            "restore must not modify the backup database"
        );
        for suffix in ["-wal", "-shm", "-journal"] {
            assert!(
                !sqlite_sidecar_path(&backup_path, suffix).exists(),
                "restore must not create or delete backup sidecars: {suffix}"
            );
        }
    }

    #[test]
    fn restore_rejects_a_backup_with_live_wal_without_touching_either_file_set() {
        fn seed_live_wal(path: &Path, marker: &str) -> Connection {
            initialize_database(path).expect("initialize WAL source");
            let connection = db::open(path).expect("open WAL source");
            connection
                .execute_batch("PRAGMA wal_autocheckpoint = 0;")
                .expect("disable WAL auto-checkpoint");
            connection
                .execute(
                    "INSERT INTO schema_meta(key, value) VALUES (?1, 'uncheckpointed')",
                    params![marker],
                )
                .expect("commit marker into WAL");
            assert!(
                sqlite_sidecar_path(path, "-wal")
                    .metadata()
                    .expect("live WAL file")
                    .len()
                    > 32
            );
            assert!(sqlite_sidecar_path(path, "-shm").is_file());
            connection
        }

        fn copy_live_wal_file_set(source: &Path, destination: &Path) {
            fs::copy(source, destination).expect("copy main database");
            for suffix in ["-wal", "-shm"] {
                fs::copy(
                    sqlite_sidecar_path(source, suffix),
                    sqlite_sidecar_path(destination, suffix),
                )
                .unwrap_or_else(|error| panic!("copy {suffix}: {error}"));
            }
        }

        fn snapshot_file_set(path: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
            ["", "-wal", "-shm", "-journal"]
                .into_iter()
                .map(|suffix| {
                    let member = if suffix.is_empty() {
                        path.to_path_buf()
                    } else {
                        sqlite_sidecar_path(path, suffix)
                    };
                    let bytes = if member.exists() {
                        Some(fs::read(&member).expect("read file-set member"))
                    } else {
                        None
                    };
                    (member, bytes)
                })
                .collect()
        }

        fn assert_file_set_unchanged(before: &[(PathBuf, Option<Vec<u8>>)]) {
            for (path, expected) in before {
                let actual = if path.exists() {
                    Some(fs::read(path).expect("read file-set member after restore"))
                } else {
                    None
                };
                assert_eq!(
                    &actual,
                    expected,
                    "restore mutated file-set member {}",
                    path.display()
                );
            }
        }

        let root = tempdir().expect("temporary workspace");
        let backup_source = root.path().join("backup-source.sqlite3");
        let target_source = root.path().join("target-source.sqlite3");
        let backup_path = root.path().join("backup.sqlite3");
        let target_path = root.path().join("target.sqlite3");

        let backup_writer = seed_live_wal(&backup_source, "backup-wal-marker");
        let target_writer = seed_live_wal(&target_source, "target-wal-marker");
        copy_live_wal_file_set(&backup_source, &backup_path);
        copy_live_wal_file_set(&target_source, &target_path);
        fs::write(
            sqlite_sidecar_path(&target_path, "-journal"),
            b"target-journal-sentinel",
        )
        .expect("create target journal sentinel");

        let backup_before = snapshot_file_set(&backup_path);
        let target_before = snapshot_file_set(&target_path);
        let error = db::restore_database_copy(&target_path, &backup_path)
            .expect_err("a backup with sidecars must fail closed");
        assert!(
            error.contains("self-contained") && error.contains("-wal"),
            "unexpected sidecar rejection: {error}"
        );
        assert_file_set_unchanged(&backup_before);
        assert_file_set_unchanged(&target_before);

        drop(target_writer);
        drop(backup_writer);
    }

    #[test]
    fn empty_directory_opens_as_a_ready_v2_workspace() {
        let root = tempdir().expect("temporary workspace");
        let projection = WorkspaceModule::new()
            .open(root.path())
            .expect("empty directory should initialize");

        assert_eq!(projection.status, WorkspaceStatus::Ready);
        assert_eq!(projection.papers_path, root.path().join("Papers"));
        assert_eq!(projection.textbooks_path, root.path().join("Textbooks"));
        assert_eq!(
            projection.database_path,
            root.path().join(".read-desktop").join("workspace.sqlite3")
        );
        assert!(projection.papers_path.is_dir());
        assert!(projection.textbooks_path.is_dir());
        assert!(projection.database_path.is_file());
    }

    #[test]
    fn legacy_workspace_requires_confirmation_without_mutating_files() {
        let root = tempdir().expect("temporary workspace");
        fs::write(root.path().join("workspace.sqlite3"), b"legacy")
            .expect("legacy database marker");
        fs::create_dir(root.path().join("library")).expect("legacy library marker");

        let projection = WorkspaceModule::new()
            .open(root.path())
            .expect("legacy workspace should be inspectable");

        assert_eq!(projection.status, WorkspaceStatus::ResetRequired);
        assert!(!root.path().join("Papers").exists());
        assert!(!root.path().join(".read-desktop").exists());
        assert!(root.path().join("workspace.sqlite3").is_file());
        assert!(root.path().join("library").is_dir());
    }

    #[test]
    fn a_second_module_cannot_write_the_same_workspace() {
        let root = tempdir().expect("temporary workspace");
        let first = WorkspaceModule::new();
        first
            .open(root.path())
            .expect("first writer owns the workspace");

        let error = WorkspaceModule::new()
            .open(root.path())
            .expect_err("second writer must be rejected");

        assert!(error.contains("already open"));
    }

    #[test]
    fn explicit_reset_archives_legacy_data_before_creating_v2_layout() {
        let root = tempdir().expect("temporary workspace");
        fs::write(root.path().join("workspace.sqlite3"), b"legacy")
            .expect("legacy database marker");
        fs::create_dir(root.path().join("library")).expect("legacy library marker");
        fs::write(root.path().join("library").join("paper.pdf"), b"legacy pdf")
            .expect("legacy paper");

        let module = WorkspaceModule::new();
        let preview = module
            .inspect_legacy_reset(root.path())
            .expect("preview should inspect Legacy data");
        assert_eq!(preview.file_count, 2);
        assert_eq!(preview.pdf_count, 1);
        assert_eq!(preview.total_bytes, 16);
        assert!(!preview.backup_path.exists());
        assert!(root.path().join("workspace.sqlite3").is_file());
        assert!(root.path().join("library").join("paper.pdf").is_file());

        let result = module
            .execute_legacy_reset(root.path(), &preview.preview_digest)
            .expect("confirmed reset should initialize V2");
        let projection = result.projection;

        assert_eq!(projection.status, WorkspaceStatus::Ready);
        assert!(!root.path().join("workspace.sqlite3").exists());
        assert!(!root.path().join("library").exists());
        assert!(projection.database_path.is_file());
        assert_eq!(result.backup_path, preview.backup_path);
        assert_eq!(
            fs::read(result.backup_path.join("workspace.sqlite3"))
                .expect("archived Legacy database"),
            b"legacy"
        );
        assert_eq!(
            fs::read(result.backup_path.join("library").join("paper.pdf"))
                .expect("archived Legacy PDF"),
            b"legacy pdf"
        );
        assert!(result.backup_path.join("manifest.json").is_file());

        let repeat = module
            .execute_legacy_reset(root.path(), &preview.preview_digest)
            .expect_err("the same reset cannot execute twice");
        assert!(repeat.contains("currently open") || repeat.contains("changed"));
    }

    #[test]
    fn one_module_can_reopen_and_switch_workspaces_without_leaking_locks() {
        let first_root = tempdir().expect("first workspace");
        let second_root = tempdir().expect("second workspace");
        let module = WorkspaceModule::new();

        module.open(first_root.path()).expect("first open");
        module
            .open(first_root.path())
            .expect("reopening the active workspace is idempotent");
        module.open(second_root.path()).expect("workspace switch");

        WorkspaceModule::new()
            .open(first_root.path())
            .expect("switching releases the previous workspace lock");
    }

    #[test]
    fn initializes_the_required_v2_domain_schema_in_wal_mode() {
        let root = tempdir().expect("temporary workspace");
        let projection = WorkspaceModule::new()
            .open(root.path())
            .expect("workspace should initialize");
        let connection = Connection::open(&projection.database_path).expect("open database");

        let journal_mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal mode");
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");

        let mut statement = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .expect("table query");
        let tables = statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("table rows")
            .collect::<rusqlite::Result<std::collections::HashSet<_>>>()
            .expect("table names");
        for required in [
            "collections",
            "papers",
            "document_revisions",
            "paper_heads",
            "ocr_blocks",
            "artifacts",
            "artifact_heads",
            "discussions",
            "messages",
            "message_contexts",
            "jobs",
            "job_attempts",
            "usage_receipts",
            "operation_journal",
            "reconciliation_conflicts",
            "trash_entries",
            "remote_tombstones",
            "outline_revisions",
            "outline_heads",
            "outline_deep_dive_heads",
            "outline_plans",
            "reading_guide_revisions",
            "reading_guide_heads",
            "reading_guide_plans",
        ] {
            assert!(tables.contains(required), "missing V2 table {required}");
        }
    }

    #[test]
    fn database_migrations_are_idempotent_and_install_hot_indexes() {
        let root = tempdir().expect("temporary workspace");
        let database_path = root.path().join("workspace.sqlite3");

        initialize_database(&database_path).expect("first migration");
        // Running the complete migration against an already initialized V2
        // database must not rewrite rows or fail on existing indexes.
        initialize_database(&database_path).expect("second migration");

        let connection = db::open(&database_path).expect("open migrated database");
        let schema_version: String = connection
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .expect("schema version");
        let workspace_format: String = connection
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'workspace_format'",
                [],
                |row| row.get(0),
            )
            .expect("workspace format");
        assert_eq!(schema_version, SQLITE_SCHEMA_VERSION.to_string());
        assert_eq!(workspace_format, WORKSPACE_FORMAT_VERSION.to_string());

        let indexes = connection
            .prepare(
                "SELECT name FROM sqlite_master
                 WHERE type = 'index' AND name LIKE 'idx_%'",
            )
            .expect("index query")
            .query_map([], |row| row.get::<_, String>(0))
            .expect("index rows")
            .collect::<rusqlite::Result<std::collections::HashSet<_>>>()
            .expect("index names");
        for expected in [
            "idx_ocr_revisions_revision_status",
            "idx_ocr_pages_revision_page",
            "idx_ocr_blocks_page_index",
            "idx_context_roots_revision_state",
            "idx_discussions_revision_status_updated",
            "idx_messages_discussion_created",
            "idx_messages_discussion_parent",
            "idx_artifacts_revision_kind",
            "idx_jobs_revision_kind_state",
            "idx_usage_receipts_operation",
            "idx_trash_entries_expiry",
            "idx_remote_tombstones_state_updated",
            "idx_outline_revisions_revision_kind",
            "idx_outline_plans_revision_created",
            "idx_reading_guide_revisions_revision",
            "idx_reading_guide_plans_revision_created",
        ] {
            assert!(indexes.contains(expected), "missing hot index {expected}");
        }

        let plan = connection
            .prepare(
                "EXPLAIN QUERY PLAN
                 SELECT id FROM messages
                 WHERE discussion_id = 'discussion'
                 ORDER BY created_at, id",
            )
            .expect("query plan")
            .query_map([], |row| row.get::<_, String>(3))
            .expect("query plan rows")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("query plan text");
        assert!(
            plan.iter()
                .any(|detail| detail.contains("idx_messages_discussion_created")),
            "message projection should use the discussion/created index: {plan:?}"
        );
    }

    #[test]
    fn downgraded_v7_metadata_fails_closed_without_backup_or_mutation() {
        let root = tempdir().expect("temporary workspace");
        let database_path = root.path().join("workspace.sqlite3");
        initialize_database(&database_path).expect("initial v7 migration");
        let connection = db::open(&database_path).expect("open v7 database");
        connection
            .execute_batch(
                "UPDATE schema_meta SET value = '1'
                 WHERE key IN ('workspace_format', 'schema_version');
                 PRAGMA user_version = 1;",
            )
            .expect("forge stale version metadata");
        drop(connection);

        let before = snapshot_database_file_set(&database_path);
        let backup_root = root.path().join("backups");
        let error = initialize_database(&database_path)
            .expect_err("v7 artifacts with downgraded metadata must fail closed");
        assert!(
            error.contains("pre-v7 schema") && error.contains("v7 table"),
            "unexpected downgraded-metadata error: {error}"
        );
        assert_eq!(
            snapshot_database_file_set(&database_path),
            before,
            "downgraded metadata attempt changed the database file set"
        );
        assert_eq!(
            backup_directory_count(&backup_root),
            0,
            "downgraded metadata attempt created a backup"
        );
    }

    fn insert_outline_fixture(connection: &Connection, revision_id: &str) {
        connection
            .execute_batch(
                r#"
                INSERT OR IGNORE INTO collections(id, name, relative_path, created_at, updated_at)
                VALUES ('col-1', 'Inbox', 'Inbox', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT OR IGNORE INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                VALUES ('paper-1', 'col-1', 'paper.pdf', 'Inbox/paper.pdf', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT OR IGNORE INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                VALUES ('rev-1', 'paper-1', 'digest-1', 12, 8, 'Inbox/paper.pdf', '2026-08-17T00:00:00Z');
                INSERT OR IGNORE INTO ocr_revisions(id, revision_id, status, provider, model, created_at, published_at)
                VALUES ('ocr-1', 'rev-1', 'ready', 'mistral', 'mistral-ocr-latest', '2026-08-17T00:00:01Z', '2026-08-17T00:00:02Z');
                "#,
            )
            .expect("paper fixture");
        connection
            .execute(
                r#"
                INSERT INTO outline_revisions(
                  id, paper_id, revision_id, ocr_revision_id, kind, status, protocol_version,
                  catalog_digest, units_json, graph_json, coverage_json, dependency_snapshot_json, created_at
                ) VALUES (
                  ?1, 'paper-1', 'rev-1', 'ocr-1', 'overview', 'published', 'outline-compose-v2',
                  'abc', '[]', '{"title":"t","summary":"s","nodes":[],"edges":[]}', '{}', '{}', '2026-08-17T00:00:03Z'
                )
                "#,
                params![revision_id],
            )
            .expect("outline revision");
        connection
            .execute(
                r#"
                INSERT INTO outline_heads(revision_id, overview_revision_id, updated_at)
                VALUES ('rev-1', ?1, '2026-08-17T00:00:03Z')
                ON CONFLICT(revision_id) DO UPDATE SET overview_revision_id = excluded.overview_revision_id
                "#,
                params![revision_id],
            )
            .expect("outline head");
        connection
            .execute(
                r#"
                INSERT INTO jobs(
                  id, kind, dedupe_key, state, stage, payload_json, created_at, updated_at
                ) VALUES (
                  ?1, 'outline_overview', ?1, 'completed', 'done', '{}', '2026-08-17T00:00:03Z', '2026-08-17T00:00:03Z'
                )
                "#,
                params![format!("job-{revision_id}")],
            )
            .expect("outline job");
    }

    fn insert_outline_migration_fixture(connection: &Connection) {
        insert_outline_fixture(connection, "outline-old");
        connection.execute_batch(r#"
            INSERT OR REPLACE INTO schema_meta(key, value) VALUES ('outline_epoch', '2');
            INSERT INTO outline_revisions(
              id, paper_id, revision_id, ocr_revision_id, kind, parent_overview_id,
              parent_node_id, status, protocol_version, catalog_digest, graph_json, created_at
            ) VALUES ('local-old', 'paper-1', 'rev-1', 'ocr-1', 'deep_dive', 'outline-old',
              'node-old', 'published', 'outline-deep-dive-v2', 'abc', '{"old":"graph"}', '2026-08-17');
            INSERT INTO outline_deep_dive_heads VALUES ('outline-old', 'node-old', 'local-old', '2026-08-17');
            INSERT INTO outline_plans(id, revision_id, ocr_revision_id, catalog_digest, model, payload_json, created_at)
              VALUES ('plan-old', 'rev-1', 'ocr-1', 'abc', 'old-model', '{"frozen":"old prompt"}', '2026-08-17');
            INSERT INTO jobs(id, kind, dedupe_key, state, stage, payload_json, created_at, updated_at)
              VALUES ('local-job', 'outline_deep_dive', 'local-job', 'paused', 'generate',
                '{"protocolVersion":"outline-deep-dive-v2"}', '2026-08-17', '2026-08-17');
            INSERT INTO job_checkpoints VALUES ('local-job', 'generate', '{"paidResponse":"kept"}', '2026-08-17');
            INSERT INTO job_attempts(id, job_id, attempt_number, state, started_at)
              VALUES ('attempt-old', 'local-job', 1, 'paused', '2026-08-17');
            INSERT INTO usage_receipts(id, operation_id, job_id, provider, model, input_tokens, created_at)
              VALUES ('receipt-old', 'operation-old', 'local-job', 'gemini', 'old-model', 123, '2026-08-17');
            INSERT INTO reading_states(paper_id, revision_id, active_outline_node_id, outline_view, updated_at)
              VALUES ('paper-1', 'rev-1', 'node-old', 'deep_dive', '2026-08-17');
            INSERT INTO jobs(id, kind, dedupe_key, state, stage, payload_json, created_at, updated_at)
              VALUES ('other-job', 'ocr', 'other-job', 'completed', 'done', '{}', '2026-08-17', '2026-08-17');
        "#).expect("legacy map, local map and paid task fixture");
    }

    fn outline_migration_records(connection: &Connection) -> Vec<Vec<Vec<rusqlite::types::Value>>> {
        [
            "outline_revisions",
            "outline_heads",
            "outline_deep_dive_heads",
            "outline_plans",
            "jobs",
            "job_checkpoints",
            "job_attempts",
            "usage_receipts",
            "reading_states",
        ]
        .into_iter()
        .map(|table| {
            let mut statement = connection
                .prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))
                .unwrap();
            statement
                .query_map([], |row| {
                    (0..row.as_ref().column_count())
                        .map(|index| row.get(index))
                        .collect()
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        })
        .collect()
    }

    #[test]
    fn exact_workspace_reopen_and_outline_epoch_migration_preserve_all_legacy_records() {
        let root = tempdir().expect("temporary workspace");
        let database_path = {
            let module = WorkspaceModule::new();
            let projection = module.open(root.path()).expect("initialize");
            let connection = db::open(&projection.database_path).unwrap();
            insert_outline_migration_fixture(&connection);
            projection.database_path.clone()
        };
        let connection = db::open(&database_path).unwrap();
        let before = outline_migration_records(&connection);
        drop(connection);
        WorkspaceModule::new()
            .open(root.path())
            .expect("reopen only validates");
        let connection = db::open(&database_path).unwrap();
        assert_eq!(outline_migration_records(&connection), before);
        let epoch: String = connection
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'outline_epoch'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(epoch, "2");

        migrate_outline_epoch(&connection).expect("non-destructive compatibility migration");
        assert_eq!(outline_migration_records(&connection), before);
        let epoch: String = connection
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'outline_epoch'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(epoch, OUTLINE_EPOCH.to_string());
        let version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .unwrap();
        assert_eq!(version, SQLITE_SCHEMA_VERSION);
        migrate_outline_epoch(&connection).expect("idempotent migration");
        assert_eq!(outline_migration_records(&connection), before);
        drop(connection);
        WorkspaceModule::new()
            .open(root.path())
            .expect("reopen retains legacy data");
        assert_eq!(
            outline_migration_records(&db::open(&database_path).unwrap()),
            before
        );
    }

    #[test]
    fn outline_epoch_migration_rolls_back_marker_on_failure_without_touching_legacy_records() {
        let root = tempdir().expect("temporary workspace");
        let projection = WorkspaceModule::new().open(root.path()).unwrap();
        let connection = db::open(&projection.database_path).unwrap();
        insert_outline_migration_fixture(&connection);
        let before = outline_migration_records(&connection);
        connection
            .execute_batch(
                "CREATE TRIGGER fail_outline_epoch_write AFTER INSERT ON schema_meta
             WHEN NEW.key = 'outline_epoch'
             BEGIN SELECT RAISE(ABORT, 'forced epoch failure'); END;",
            )
            .unwrap();
        assert!(migrate_outline_epoch(&connection).is_err());
        assert!(
            connection.is_autocommit(),
            "failure must close the transaction"
        );
        assert_eq!(outline_migration_records(&connection), before);
        let epoch: String = connection
            .query_row(
                "SELECT value FROM schema_meta WHERE key = 'outline_epoch'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(epoch, "2");
        connection
            .execute_batch("DROP TRIGGER fail_outline_epoch_write")
            .unwrap();
        migrate_outline_epoch(&connection).expect("retry succeeds after rollback");
        assert_eq!(outline_migration_records(&connection), before);
    }

    #[test]
    fn future_schema_fails_closed_without_mutating_database_or_sidecars() {
        let root = tempdir().expect("temporary workspace");
        let first = WorkspaceModule::new();
        let projection = first.open(root.path()).expect("initialize workspace");
        let database_path = projection.database_path.clone();
        let connection = db::open(&database_path).expect("open database");
        connection
            .execute(
                "INSERT OR REPLACE INTO schema_meta(key, value) VALUES ('schema_version', '99')",
                [],
            )
            .expect("mark future schema");
        connection
            .execute_batch("PRAGMA user_version = 99;")
            .expect("mark future sqlite schema");
        drop(connection);
        let wal_path = sqlite_sidecar_path(&database_path, "-wal");
        let shm_path = sqlite_sidecar_path(&database_path, "-shm");
        fs::write(&wal_path, b"future wal").expect("write WAL marker");
        fs::write(&shm_path, b"future shm").expect("write SHM marker");
        drop(first);

        let before = snapshot_database_file_set(&database_path);
        let backup_root = root.path().join(".read-desktop/backups");
        let error = WorkspaceModule::new()
            .open(root.path())
            .expect_err("future schema must not be rebuilt automatically");
        assert!(
            error.contains("Unsupported future Workspace schema"),
            "unexpected future-schema error: {error}"
        );
        assert_eq!(
            snapshot_database_file_set(&database_path),
            before,
            "future schema attempt changed the database file set"
        );
        assert_eq!(
            backup_directory_count(&backup_root),
            0,
            "future schema attempt created a backup"
        );
    }

    #[test]
    fn corrupt_database_fails_closed_without_touching_papers_or_database_files() {
        let root = tempdir().expect("temporary workspace");
        let first = WorkspaceModule::new();
        let projection = first.open(root.path()).expect("initialize workspace");
        let database_path = projection.database_path.clone();
        let paper_path = root.path().join("Papers/keep.pdf");
        fs::write(&paper_path, b"%PDF-1.4\n%%EOF").expect("paper fixture");
        drop(first);

        fs::write(&database_path, b"not a sqlite database").expect("corrupt database");
        let wal_path = sqlite_sidecar_path(&database_path, "-wal");
        fs::write(&wal_path, b"corrupt wal").expect("corrupt WAL");
        let before = snapshot_database_file_set(&database_path);
        let backup_root = root.path().join(".read-desktop/backups");

        let error = WorkspaceModule::new()
            .open(root.path())
            .expect_err("corrupt database must not be rebuilt automatically");
        assert!(
            error.contains("Unable to validate Workspace database")
                || error.contains("Unable to open Workspace database read-only"),
            "unexpected corrupt-database error: {error}"
        );
        assert!(paper_path.is_file(), "physical papers must not be deleted");
        assert_eq!(
            snapshot_database_file_set(&database_path),
            before,
            "corrupt database attempt changed the database file set"
        );
        assert_eq!(
            backup_directory_count(&backup_root),
            0,
            "corrupt database attempt created a backup"
        );
    }

    #[test]
    fn restore_rolls_back_both_file_sets_when_promotion_fails() {
        fn snapshot_file_set(path: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
            ["", "-wal", "-shm", "-journal"]
                .into_iter()
                .map(|suffix| {
                    let member = if suffix.is_empty() {
                        path.to_path_buf()
                    } else {
                        sqlite_sidecar_path(path, suffix)
                    };
                    let bytes = member
                        .exists()
                        .then(|| fs::read(&member).expect("read file-set member"));
                    (member, bytes)
                })
                .collect()
        }

        fn assert_file_set_unchanged(before: &[(PathBuf, Option<Vec<u8>>)]) {
            for (path, expected) in before {
                let actual = path
                    .exists()
                    .then(|| fs::read(path).expect("read file-set member after failed restore"));
                assert_eq!(
                    &actual,
                    expected,
                    "failed restore mutated file-set member {}",
                    path.display()
                );
            }
        }

        let root = tempdir().expect("temporary workspace");
        let backup_source_path = root.path().join("backup-source.sqlite3");
        initialize_database(&backup_source_path).expect("initialize backup source");
        let backup_source = db::open(&backup_source_path).expect("open backup source");
        backup_source
            .execute(
                "INSERT INTO schema_meta(key, value) VALUES ('backup-marker', 'original')",
                [],
            )
            .expect("seed backup marker");
        drop(backup_source);
        let backup_path =
            db::checkpoint_close_copy_database(&backup_source_path, "rollback-fixture")
                .expect("create self-contained backup");

        let target_path = root.path().join("target.sqlite3");
        initialize_database(&target_path).expect("initialize restore target");
        let target = db::open(&target_path).expect("open restore target");
        target
            .execute(
                "INSERT INTO schema_meta(key, value) VALUES ('target-marker', 'original')",
                [],
            )
            .expect("seed target marker");
        drop(target);
        fs::write(
            sqlite_sidecar_path(&target_path, "-journal"),
            b"target-journal-sentinel",
        )
        .expect("seed target sidecar");

        let backup_before = snapshot_file_set(&backup_path);
        let target_before = snapshot_file_set(&target_path);
        let root_entries_before = fs::read_dir(root.path())
            .expect("list root before failed restore")
            .map(|entry| entry.expect("root entry").file_name())
            .collect::<std::collections::HashSet<_>>();

        let error = db::restore_database_copy_with_before_promote_fault(&target_path, &backup_path)
            .expect_err("injected promotion failure must abort restore");
        assert!(
            error.contains("injected restore failure before promotion"),
            "unexpected restore error: {error}"
        );
        assert_file_set_unchanged(&backup_before);
        assert_file_set_unchanged(&target_before);
        let root_entries_after = fs::read_dir(root.path())
            .expect("list root after failed restore")
            .map(|entry| entry.expect("root entry").file_name())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            root_entries_after, root_entries_before,
            "failed restore leaked staging or recovery artifacts"
        );
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn restore_rejects_symlinked_backup_and_target_without_mutating_file_sets() {
        #[cfg(unix)]
        fn symlink_file(source: &Path, destination: &Path) -> std::io::Result<()> {
            std::os::unix::fs::symlink(source, destination)
        }

        #[cfg(windows)]
        fn symlink_file(source: &Path, destination: &Path) -> std::io::Result<()> {
            std::os::windows::fs::symlink_file(source, destination)
        }

        fn symlink_file_or_skip(source: &Path, destination: &Path) -> bool {
            match symlink_file(source, destination) {
                Ok(()) => true,
                Err(error) => {
                    #[cfg(windows)]
                    if error.raw_os_error() == Some(1314) {
                        eprintln!(
                            "skipping symlink restore assertions: Windows OS 1314 (symbolic-link privilege is unavailable)"
                        );
                        return false;
                    }
                    panic!(
                        "create symlink {} -> {}: {error}",
                        destination.display(),
                        source.display()
                    );
                }
            }
        }

        fn snapshot_file_set(path: &Path) -> Vec<Option<Vec<u8>>> {
            ["", "-wal", "-shm", "-journal"]
                .into_iter()
                .map(|suffix| {
                    let member = if suffix.is_empty() {
                        path.to_path_buf()
                    } else {
                        sqlite_sidecar_path(path, suffix)
                    };
                    member
                        .exists()
                        .then(|| fs::read(member).expect("read file-set member"))
                })
                .collect()
        }

        fn root_entries(root: &Path) -> std::collections::HashSet<std::ffi::OsString> {
            fs::read_dir(root)
                .expect("list restore fixture root")
                .map(|entry| entry.expect("restore fixture entry").file_name())
                .collect()
        }

        let root = tempdir().expect("temporary workspace");
        let backup_source_path = root.path().join("backup-source.sqlite3");
        initialize_database(&backup_source_path).expect("initialize backup source");
        let backup_path =
            db::checkpoint_close_copy_database(&backup_source_path, "symlink-fixture")
                .expect("create self-contained backup");

        let target_path = root.path().join("target.sqlite3");
        initialize_database(&target_path).expect("initialize restore target");
        let backup_before = snapshot_file_set(&backup_path);
        let target_before = snapshot_file_set(&target_path);

        let linked_backup = root.path().join("linked-backup.sqlite3");
        if !symlink_file_or_skip(&backup_path, &linked_backup) {
            return;
        }
        assert!(fs::symlink_metadata(&linked_backup)
            .expect("inspect backup symlink")
            .file_type()
            .is_symlink());
        let entries_before_backup_rejection = root_entries(root.path());
        let error = db::restore_database_copy(&target_path, &linked_backup)
            .expect_err("a symlinked backup must fail closed");
        assert!(
            error.contains("Backup database") && error.contains("not a regular file"),
            "unexpected backup symlink rejection: {error}"
        );
        assert_eq!(snapshot_file_set(&backup_path), backup_before);
        assert_eq!(snapshot_file_set(&target_path), target_before);
        assert_eq!(root_entries(root.path()), entries_before_backup_rejection);

        let linked_target = root.path().join("linked-target.sqlite3");
        if !symlink_file_or_skip(&target_path, &linked_target) {
            return;
        }
        assert!(fs::symlink_metadata(&linked_target)
            .expect("inspect target symlink")
            .file_type()
            .is_symlink());
        let entries_before_target_rejection = root_entries(root.path());
        let error = db::restore_database_copy(&linked_target, &backup_path)
            .expect_err("a symlinked target must fail closed");
        assert!(
            error.contains("Restore target member") && error.contains("not a regular file"),
            "unexpected target symlink rejection: {error}"
        );
        assert_eq!(snapshot_file_set(&backup_path), backup_before);
        assert_eq!(snapshot_file_set(&target_path), target_before);
        assert_eq!(root_entries(root.path()), entries_before_target_rejection);
    }
}
