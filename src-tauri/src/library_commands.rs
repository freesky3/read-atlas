#![allow(dead_code)]

use crate::job_module::JobState;
use crate::library_watcher::{LibraryWatcher, LibraryWatcherConfig};
use crate::paper_module::{
    LibraryProjection, PaperModule, ReadingState, StorageReport, TrashProjection,
};
use crate::reader_context;
use crate::workspace_lifecycle::{active_paper_module, active_root, current_runtime};
use crate::{
    document_card_from_paper, document_cards_from_papers, now, open_db, queue_paper_remote_cleanup,
    spawn_remote_cleanup_worker, AppResult, AppState, DeletePaperResult, DocumentCard, ReadEvent,
};
use rusqlite::params;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::Emitter;
use tokio::sync::Notify;
use uuid::Uuid;

pub(crate) fn start_library_watcher(
    app: &tauri::AppHandle,
    paper_module: &PaperModule,
    watch_paths: Vec<std::path::PathBuf>,
    stop: Arc<AtomicBool>,
) -> AppResult<LibraryWatcher> {
    let watch_paper = paper_module.clone();
    let watch_app = app.clone();
    let workspace_root = paper_module.workspace_root().to_path_buf();
    let pending = Arc::new(AtomicBool::new(false));
    let running = Arc::new(AtomicBool::new(false));
    let wake = Arc::new(Notify::new());
    LibraryWatcher::start_with(
        watch_paths,
        [workspace_root.clone()],
        LibraryWatcherConfig::default(),
        move |paths| {
            if stop.load(Ordering::Acquire) {
                return;
            }
            let reader_hit = paths
                .iter()
                .any(|path| reader_context::is_reader_context_path(&workspace_root, path));
            let other_hit = paths
                .iter()
                .any(|path| !reader_context::is_reader_context_path(&workspace_root, path));
            if reader_hit {
                let _ = watch_app.emit(
                    "read-event",
                    ReadEvent {
                        cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
                        kind: "reader_context".to_string(),
                        entity_id: None,
                        delta: None,
                        status: None,
                    },
                );
            }
            if reader_hit && !other_hit {
                return;
            }
            pending.store(true, Ordering::Release);
            wake.notify_one();
            if running
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                return;
            }
            let pending = pending.clone();
            let running = running.clone();
            let wake = wake.clone();
            let stop = stop.clone();
            let paper = watch_paper.clone();
            let app = watch_app.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    tokio::select! {
                        _ = wake.notified() => {}
                        _ = tokio::time::sleep(Duration::from_millis(250)) => {}
                    }
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    if !pending.swap(false, Ordering::AcqRel) {
                        continue;
                    }
                    // A short async debounce merges callbacks that arrive after
                    // notify's filesystem stability probe but before reconcile.
                    tokio::time::sleep(Duration::from_millis(50)).await;
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    pending.swap(false, Ordering::AcqRel);
                    let reconcile = paper.clone();
                    let result =
                        tauri::async_runtime::spawn_blocking(move || reconcile.reconcile())
                            .await
                            .ok()
                            .and_then(Result::ok);
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    if let Some(library) = result {
                        let delta = (!library.kind_change_notices.is_empty()).then(|| {
                            serde_json::to_string(&library.kind_change_notices).unwrap_or_default()
                        });
                        let _ = app.emit(
                            "read-event",
                            ReadEvent {
                                cursor: library.cursor,
                                kind: "library".to_string(),
                                entity_id: None,
                                delta,
                                status: None,
                            },
                        );
                    }
                }
                running.store(false, Ordering::Release);
            });
        },
    )
}

pub(crate) fn open_workspace_dir_impl(state: &AppState) -> AppResult<bool> {
    let root = active_root(state)?;
    if !root.is_dir() {
        return Err("Workspace 目录当前不可访问".to_string());
    }
    std::process::Command::new("explorer")
        .arg(root)
        .spawn()
        .map_err(|error| format!("无法打开 Workspace：{error}"))?;
    Ok(true)
}

pub(crate) fn open_library_impl(state: &AppState) -> AppResult<LibraryProjection> {
    active_paper_module(state)?.list_library()
}

pub(crate) fn list_documents_impl(state: &AppState) -> AppResult<Vec<DocumentCard>> {
    let root = active_root(state)?;
    let library = active_paper_module(state)?.list_library()?;
    document_cards_from_papers(&root, library.papers)
}

pub(crate) fn import_pdf_impl(
    state: &AppState,
    path: String,
    collection: Option<String>,
) -> AppResult<DocumentCard> {
    let root = active_root(state)?;
    let imported =
        active_paper_module(state)?.import_pdf(Path::new(&path), collection.as_deref())?;
    match (imported.paper, imported.conflict) {
        (Some(paper), _) => Ok(document_card_from_paper(&root, paper)),
        (_, Some(conflict)) => Err(format!(
            "library_conflict:{}",
            serde_json::to_string(&conflict).map_err(|error| error.to_string())?
        )),
        _ => Err("Import did not publish a Paper or a conflict".to_string()),
    }
}

pub(crate) fn move_paper_impl(
    state: &AppState,
    paper_id: String,
    collection_path: String,
    file_name: String,
    confirm_kind_change: bool,
) -> AppResult<DocumentCard> {
    let root = active_root(state)?;
    let paper = active_paper_module(state)?.move_paper(
        &paper_id,
        &collection_path,
        &file_name,
        confirm_kind_change,
    )?;
    Ok(document_card_from_paper(&root, paper))
}

pub(crate) fn reconcile_library_impl(state: &AppState) -> AppResult<LibraryProjection> {
    active_paper_module(state)?.reconcile()
}

pub(crate) fn delete_revision_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    revision_id: String,
) -> AppResult<DeletePaperResult> {
    let runtime = current_runtime(state)?;
    let module = runtime.paper_module.clone();
    let paper = module
        .list_library()?
        .papers
        .into_iter()
        .find(|paper| paper.revision_id == revision_id)
        .ok_or_else(|| "The active Document Revision was not found".to_string())?;
    let root = runtime.root.clone();
    let streaming_messages = {
        let connection = open_db(&root)?;
        let mut statement = connection
            .prepare(
                "SELECT m.id
                 FROM messages m
                 JOIN discussions d ON d.id = m.discussion_id
                 WHERE d.paper_id = ?1 AND m.status = 'streaming'",
            )
            .map_err(|error| error.to_string())?;
        let messages = statement
            .query_map(params![paper.id], |row| row.get::<_, String>(0))
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        messages
    };
    if let Ok(cancellations) = state.discussion_cancellations.lock() {
        for message_id in &streaming_messages {
            if let Some(cancellation) = cancellations.get(message_id) {
                cancellation.cancel();
            }
        }
    }
    let active_jobs = runtime
        .job_module
        .list()?
        .into_iter()
        .filter(|job| {
            job.paper_id.as_deref() == Some(paper.id.as_str())
                && matches!(
                    job.state,
                    JobState::Queued | JobState::Running | JobState::Paused
                )
        })
        .collect::<Vec<_>>();
    for job in &active_jobs {
        if let Ok(cancellations) = state.ocr_cancellations.lock() {
            if let Some(cancellation) = cancellations.get(&job.id) {
                cancellation.cancel();
            }
        }
        if let Ok(cancellations) = state.artifact_cancellations.lock() {
            if let Some(cancellation) = cancellations.get(&job.id) {
                cancellation.cancel();
            }
        }
    }
    let deleted = module.trash_paper(&paper.id)?;
    let cleanup_warning = queue_paper_remote_cleanup(&root, &paper.id).err();
    spawn_remote_cleanup_worker(app.clone(), runtime);
    let _ = app.emit(
        "read-event",
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "library".to_string(),
            entity_id: Some(paper.id),
            delta: None,
            status: None,
        },
    );
    Ok(DeletePaperResult {
        deleted,
        cleanup_warning,
    })
}

pub(crate) fn restore_paper_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    paper_id: String,
) -> AppResult<DocumentCard> {
    let root = active_root(state)?;
    let paper = active_paper_module(state)?.restore_paper(&paper_id)?;
    let card = document_card_from_paper(&root, paper);
    let _ = app.emit(
        "read-event",
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "library".to_string(),
            entity_id: Some(paper_id),
            delta: None,
            status: None,
        },
    );
    Ok(card)
}

pub(crate) fn get_reading_state_impl(
    state: &AppState,
    paper_id: &str,
) -> AppResult<Option<ReadingState>> {
    active_paper_module(state)?.reading_state(paper_id)
}

pub(crate) fn save_reading_state_impl(
    state: &AppState,
    reading_state: ReadingState,
) -> AppResult<ReadingState> {
    active_paper_module(state)?.save_reading_state(&reading_state)
}

pub(crate) fn get_storage_report_impl(state: &AppState) -> AppResult<StorageReport> {
    active_paper_module(state)?.storage_report()
}

pub(crate) fn list_trash_impl(state: &AppState) -> AppResult<Vec<TrashProjection>> {
    active_paper_module(state)?.list_trash()
}

pub(crate) fn list_collections_impl(
    state: &AppState,
) -> AppResult<Vec<crate::paper_module::CollectionProjection>> {
    active_paper_module(state)?.list_collections_public()
}

pub(crate) fn create_collection_impl(
    state: &AppState,
    parent_path: String,
    name: Option<String>,
) -> AppResult<crate::paper_module::CollectionProjection> {
    active_paper_module(state)?.create_collection(&parent_path, name.as_deref())
}

pub(crate) fn rename_collection_impl(
    state: &AppState,
    relative_path: String,
    new_name: String,
) -> AppResult<crate::paper_module::CollectionProjection> {
    active_paper_module(state)?.rename_collection(&relative_path, &new_name)
}

pub(crate) fn move_collection_impl(
    state: &AppState,
    relative_path: String,
    dest_parent_path: String,
) -> AppResult<crate::paper_module::CollectionProjection> {
    active_paper_module(state)?.move_collection(&relative_path, &dest_parent_path)
}

pub(crate) fn trash_collection_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    relative_path: String,
) -> AppResult<serde_json::Value> {
    let runtime = current_runtime(state)?;
    // collect paper ids under folder for cancellation before trash
    let paper_ids: Vec<String> = {
        let module = active_paper_module(state)?;
        let lib = module.list_library()?;
        lib.papers
            .into_iter()
            .filter(|p| {
                p.relative_path == relative_path
                    || p.relative_path.starts_with(&format!("{}/", relative_path))
            })
            .map(|p| p.id)
            .collect()
    };
    // cancel running jobs for those papers
    for pid in &paper_ids {
        let streaming_messages = {
            let root = runtime.root.clone();
            let conn = open_db(&root)?;
            let mut stmt = conn.prepare("SELECT m.id FROM messages m JOIN discussions d ON d.id = m.discussion_id WHERE d.paper_id = ?1 AND m.status = 'streaming'").map_err(|e| e.to_string())?;
            let msgs = stmt
                .query_map(params![pid], |r| r.get::<_, String>(0))
                .map_err(|e| e.to_string())?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|e| e.to_string())?;
            msgs
        };
        if let Ok(c) = state.discussion_cancellations.lock() {
            for mid in &streaming_messages {
                if let Some(flag) = c.get(mid) {
                    flag.cancel();
                }
            }
        }
        let active_jobs = runtime
            .job_module
            .list()?
            .into_iter()
            .filter(|j| {
                j.paper_id.as_deref() == Some(pid.as_str())
                    && matches!(
                        j.state,
                        JobState::Queued | JobState::Running | JobState::Paused
                    )
            })
            .collect::<Vec<_>>();
        for job in &active_jobs {
            if let Ok(c) = state.ocr_cancellations.lock() {
                if let Some(f) = c.get(&job.id) {
                    f.cancel();
                }
            }
            if let Ok(c) = state.artifact_cancellations.lock() {
                if let Some(f) = c.get(&job.id) {
                    f.cancel();
                }
            }
        }
    }
    let trash_result = active_paper_module(state)?.trash_collection(&relative_path);
    let (trashed, deleted) = match trash_result {
        Ok(v) => v,
        Err(e) => {
            // partial success: queue tombstones for those already trashed before failure
            let trashed_now = active_paper_module(state)
                .ok()
                .and_then(|m| m.list_trash().ok())
                .unwrap_or_default()
                .into_iter()
                .filter(|t| paper_ids.contains(&t.paper_id))
                .map(|t| t.paper_id)
                .collect::<Vec<_>>();
            for pid in &trashed_now {
                let _ = queue_paper_remote_cleanup(&runtime.root, pid);
            }
            spawn_remote_cleanup_worker(app.clone(), runtime.clone());
            let _ = app.emit(
                "read-event",
                ReadEvent {
                    cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
                    kind: "library".to_string(),
                    entity_id: None,
                    delta: None,
                    status: None,
                },
            );
            return Err(e);
        }
    };
    for pid in &trashed {
        let _ = queue_paper_remote_cleanup(&runtime.root, pid);
    }
    spawn_remote_cleanup_worker(app.clone(), runtime);
    let _ = app.emit(
        "read-event",
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "library".to_string(),
            entity_id: None,
            delta: None,
            status: None,
        },
    );
    Ok(serde_json::json!({"trashedPaperIds": trashed, "deletedEmptyDirs": deleted}))
}

pub(crate) fn reorder_collection_papers_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    collection_id: String,
    paper_ids: Vec<String>,
) -> AppResult<()> {
    active_paper_module(state)?.reorder_collection_papers(&collection_id, &paper_ids)?;
    let _ = app.emit(
        "read-event",
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "library".to_string(),
            entity_id: Some(collection_id),
            delta: None,
            status: None,
        },
    );
    Ok(())
}

pub(crate) fn set_collection_sort_mode_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    collection_id: String,
    sort_mode: String,
) -> AppResult<()> {
    active_paper_module(state)?.set_collection_sort_mode(&collection_id, &sort_mode)?;
    let _ = app.emit(
        "read-event",
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "library".to_string(),
            entity_id: Some(collection_id),
            delta: None,
            status: None,
        },
    );
    Ok(())
}

pub(crate) fn rename_paper_impl(
    state: &AppState,
    paper_id: String,
    new_file_name: String,
) -> AppResult<DocumentCard> {
    let root = active_root(state)?;
    let stem = if new_file_name.to_lowercase().ends_with(".pdf") {
        new_file_name
            .trim()
            .strip_suffix(".pdf")
            .or_else(|| new_file_name.trim().strip_suffix(".PDF"))
            .unwrap_or(&new_file_name)
            .to_string()
    } else {
        new_file_name.trim().to_string()
    };
    let stem = stem.trim().trim_end_matches(".pdf").trim().to_string();
    // peel .pdf if user typed with extension - spec says single box is stem, .pdf locked, so strip
    let paper = active_paper_module(state)?.rename_paper(&paper_id, &stem)?;
    Ok(document_card_from_paper(&root, paper))
}

fn reject_symlink_components_inline(root: &Path, target: &Path) -> AppResult<()> {
    if std::fs::symlink_metadata(root)
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
    {
        return Err("路径包含符号链接".to_string());
    }
    let rel = target
        .strip_prefix(root)
        .map_err(|_| "Path escaped".to_string())?;
    let mut cur = root.to_path_buf();
    for comp in rel.components() {
        if let std::path::Component::Normal(name) = comp {
            cur.push(name);
            if std::fs::symlink_metadata(&cur)
                .map(|m| m.file_type().is_symlink())
                .unwrap_or(false)
            {
                return Err("路径包含符号链接".to_string());
            }
        } else {
            return Err("路径不合法".to_string());
        }
    }
    Ok(())
}

pub(crate) fn open_resource_dir_impl(state: &AppState, path: String) -> AppResult<bool> {
    let p = PathBuf::from(&path);
    let root = active_root(state)?;
    // reject any path containing ParentDir component exactly ".."
    if Path::new(&path)
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("路径不合法".to_string());
    }
    if let Ok(meta) = std::fs::symlink_metadata(&p) {
        if meta.file_type().is_symlink() {
            return Err("路径包含符号链接".to_string());
        }
    }
    // handle relative Hub folder like "Papers/X"
    if !p.is_absolute() {
        let abs = root.join(&p);
        // ensure abs is still inside root
        if !abs.starts_with(&root) {
            return Err("路径不合法".to_string());
        }
        return open_resource_dir_impl(state, abs.to_string_lossy().to_string());
    }
    // absolute path must be inside workspace for Hub cases; outside workspace is rejected
    if !p.starts_with(&root) {
        return Err("路径不合法".to_string());
    }
    // now p is absolute and inside root
    let canonical_root = root.canonicalize().map_err(|e| e.to_string())?;
    let canonical_target = p
        .canonicalize()
        .map_err(|e| format!("无法解析路径: {}", e))?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err("Path escaped the Workspace".to_string());
    }
    reject_symlink_components_inline(&canonical_root, &canonical_target)?;
    if !p.exists() {
        return Err("资源不存在".to_string());
    }
    let meta = std::fs::metadata(&p).map_err(|e| e.to_string())?;
    if meta.is_file() {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", p.to_string_lossy()))
            .spawn()
            .map_err(|e| e.to_string())?;
    } else {
        std::process::Command::new("explorer")
            .arg(p)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(true)
}
