use crate::artifact_module::ArtifactModule;
use crate::guide_module::GuideModule;
use crate::job_module::{JobModule, JobRecord};
use crate::library_commands::start_library_watcher;
use crate::outline_module::OutlineModule;
use crate::paper_module::PaperModule;
use crate::provider_routing::{
    FrozenModels, FrozenProviderRoute, ModelRole, ProviderInstanceId, ProviderRouteDecision,
    ProviderRouting, ProviderSelection,
};
use crate::v2_workspace::{WorkspaceProjection, WorkspaceStatus};
use crate::{
    cleanup_workspace_staging, clear_job_cancellations, now, open_db, read_model_settings,
    spawn_job_workers, spawn_remote_cleanup_worker, workspace_info, AppResult, AppState, ReadEvent,
    WorkspaceInfo,
};
use rusqlite::params;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, TryLockError};
use tauri::{Emitter, Manager};
use tokio::sync::Notify;

pub(crate) struct WorkspaceRuntime {
    #[allow(dead_code)]
    pub(crate) generation: u64,
    pub(crate) root: PathBuf,
    pub(crate) paper_module: PaperModule,
    pub(crate) job_module: JobModule,
    pub(crate) artifact_module: ArtifactModule,
    pub(crate) outline_module: OutlineModule,
    pub(crate) guide_module: GuideModule,
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) notify: Notify,
    claim_gate: Mutex<()>,
    #[allow(dead_code)]
    pub(crate) worker_count: AtomicUsize,
}
pub(crate) struct RuntimeJobClaim {
    pub(crate) runtime: Arc<WorkspaceRuntime>,
    pub(crate) job: JobRecord,
}

impl WorkspaceRuntime {
    pub(crate) fn request_stop(&self) {
        self.request_stop_impl(|| {});
    }

    fn request_stop_impl(&self, on_contended: impl FnOnce()) {
        let claim_guard = match self.claim_gate.try_lock() {
            Ok(guard) => guard,
            Err(TryLockError::WouldBlock) => {
                on_contended();
                self.claim_gate
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
            }
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
        };
        self.stop.store(true, Ordering::Release);
        drop(claim_guard);
        self.notify.notify_waiters();
    }

    #[cfg(test)]
    fn request_stop_observing_contention(&self, on_contended: impl FnOnce()) {
        self.request_stop_impl(on_contended);
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    fn claim_if_running<T>(&self, claim: impl FnOnce() -> AppResult<T>) -> AppResult<Option<T>> {
        let _claim_guard = self
            .claim_gate
            .lock()
            .map_err(|_| "Workspace runtime claim gate poisoned".to_string())?;
        if self.is_stopped() {
            return Ok(None);
        }
        claim().map(Some)
    }

    pub(crate) fn claim_next_job(self: &Arc<Self>) -> AppResult<Option<RuntimeJobClaim>> {
        Ok(self
            .claim_if_running(|| self.job_module.claim_next_record())?
            .flatten()
            .map(|job| RuntimeJobClaim {
                runtime: Arc::clone(self),
                job,
            }))
    }
}

pub(crate) fn current_runtime(state: &AppState) -> AppResult<Arc<WorkspaceRuntime>> {
    state
        .runtime
        .lock()
        .map_err(|_| "Workspace runtime lock poisoned".to_string())?
        .clone()
        .ok_or_else(|| "请先选择 Workspace".to_string())
}

pub(crate) fn swap_runtime(
    state: &AppState,
    next: Arc<WorkspaceRuntime>,
) -> AppResult<Option<Arc<WorkspaceRuntime>>> {
    let mut slot = state
        .runtime
        .lock()
        .map_err(|_| "Workspace runtime lock poisoned".to_string())?;
    let previous = slot.take();
    if let Some(previous) = previous.as_ref() {
        previous.request_stop();
    }
    *slot = Some(next);
    Ok(previous)
}

pub(crate) fn active_root(state: &AppState) -> AppResult<PathBuf> {
    Ok(current_runtime(state)?.root.clone())
}

pub(crate) fn active_paper_module(state: &AppState) -> AppResult<PaperModule> {
    Ok(current_runtime(state)?.paper_module.clone())
}

pub(crate) fn active_job_module(state: &AppState) -> AppResult<JobModule> {
    Ok(current_runtime(state)?.job_module.clone())
}

pub(crate) fn active_artifact_module(
    state: &AppState,
) -> AppResult<crate::artifact_module::ArtifactModule> {
    Ok(current_runtime(state)?.artifact_module.clone())
}

pub(crate) fn active_outline_module(state: &AppState) -> AppResult<OutlineModule> {
    Ok(current_runtime(state)?.outline_module.clone())
}

pub(crate) fn active_guide_module(state: &AppState) -> AppResult<GuideModule> {
    Ok(current_runtime(state)?.guide_module.clone())
}

#[allow(dead_code)]
pub(crate) fn runtime_is_current(state: &AppState, runtime: &WorkspaceRuntime) -> bool {
    state
        .runtime
        .lock()
        .ok()
        .and_then(|current| current.as_ref().map(|value| value.generation))
        == Some(runtime.generation)
        && !runtime.stop.load(Ordering::Acquire)
}

#[allow(dead_code)]
pub(crate) fn current_runtime_generation(state: &AppState) -> u64 {
    state
        .runtime
        .lock()
        .ok()
        .and_then(|runtime| runtime.as_ref().map(|runtime| runtime.generation))
        .unwrap_or(0)
}

pub(crate) fn cancel_runtime_work(state: &AppState) -> AppResult<()> {
    for cancellations in [
        &state.ocr_cancellations,
        &state.artifact_cancellations,
        &state.discussion_cancellations,
    ] {
        let cancellations = cancellations
            .lock()
            .map_err(|_| "runtime cancellation lock poisoned".to_string())?;
        for cancellation in cancellations.values() {
            cancellation.cancel();
        }
    }
    Ok(())
}

pub(crate) fn get_workspace_impl(state: &AppState) -> AppResult<Option<WorkspaceInfo>> {
    let runtime = match current_runtime(state) {
        Ok(runtime) => runtime,
        Err(_) => return Ok(None),
    };
    Ok(Some(workspace_info(&runtime.root)))
}

pub(crate) fn choose_workspace_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    root_path: String,
) -> AppResult<WorkspaceInfo> {
    let root = PathBuf::from(root_path);
    let projection = state.workspace_module.open(&root)?;
    if projection.status == WorkspaceStatus::ResetRequired {
        return Ok(WorkspaceInfo {
            root_path: projection.root_path.to_string_lossy().to_string(),
            database_path: projection.database_path.to_string_lossy().to_string(),
            library_path: projection.papers_path.to_string_lossy().to_string(),
            textbooks_path: projection.textbooks_path.to_string_lossy().to_string(),
            unmanaged_pdf_notice: None,
            available: false,
            status_detail: "reset_required".to_string(),
        });
    }
    activate_workspace(app, state, projection)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteLegacyResetResponse {
    pub workspace: WorkspaceInfo,
    pub backup_path: String,
    pub file_count: u64,
    pub pdf_count: u64,
    pub total_bytes: u64,
}

pub(crate) fn execute_legacy_reset_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    root_path: String,
    preview_digest: String,
) -> AppResult<ExecuteLegacyResetResponse> {
    let result = state
        .workspace_module
        .execute_legacy_reset(Path::new(&root_path), &preview_digest)?;
    let backup_path = result.backup_path.clone();
    let file_count = result.file_count;
    let pdf_count = result.pdf_count;
    let total_bytes = result.total_bytes;
    let workspace = activate_workspace(app, state, result.projection)?;
    Ok(ExecuteLegacyResetResponse {
        workspace,
        backup_path: backup_path.to_string_lossy().to_string(),
        file_count,
        pdf_count,
        total_bytes,
    })
}

fn ready_provider_routes_for_legacy(
    app: &tauri::AppHandle,
    state: &AppState,
) -> Vec<FrozenProviderRoute> {
    let Ok(settings) = read_model_settings(app) else {
        return Vec::new();
    };
    let routing = ProviderRouting::new(&settings, state);
    let mut routes = Vec::new();
    for instance in &settings.providers {
        let Ok(instance_id) = ProviderInstanceId::parse(&instance.id) else {
            continue;
        };
        let Ok(models) = FrozenModels::new(
            instance.paper_model.clone(),
            Some(instance.translation_model.clone()),
        ) else {
            continue;
        };
        for operation in [ModelRole::Paper, ModelRole::Translation] {
            if let Ok(ProviderRouteDecision::Ready(route)) = routing.capture(
                ProviderSelection::Instance(instance_id.clone()),
                models.clone(),
                operation,
            ) {
                routes.push(route.freeze());
            }
        }
    }
    routes
}

pub(crate) fn activate_workspace(
    app: &tauri::AppHandle,
    state: &AppState,
    projection: WorkspaceProjection,
) -> AppResult<WorkspaceInfo> {
    cancel_runtime_work(state)?;
    if let Some(watcher) = state
        .library_watcher
        .lock()
        .map_err(|_| "LibraryWatcher lock poisoned".to_string())?
        .take()
    {
        drop(watcher);
    }
    // Requests retain their own cancellation flag, so dropping the registry
    // here is safe and prevents a stopped workspace from retaining job IDs.
    clear_job_cancellations(state)?;
    if let Ok(mut cancellations) = state.discussion_cancellations.lock() {
        for cancellation in cancellations.values() {
            cancellation.cancel();
        }
        cancellations.clear();
    }
    let recovery_connection = open_db(&projection.root_path)?;
    recovery_connection
        .execute(
            "UPDATE messages
             SET status = 'failed', updated_at = ?1
             WHERE status = 'streaming'",
            params![now()],
        )
        .map_err(|error| error.to_string())?;
    drop(recovery_connection);
    app.asset_protocol_scope()
        .allow_directory(&projection.root_path, true)
        .map_err(|error| format!("Unable to authorize Workspace resources: {error}"))?;
    let paper_module = PaperModule::open(&projection.root_path)?;
    let job_module = JobModule::open(&projection.database_path)?;
    job_module.recover_for_startup()?;
    let ready_routes = ready_provider_routes_for_legacy(app, state);
    let _ = job_module.reconcile_legacy_provider_jobs(&ready_routes)?;
    job_module.assert_active_route_invariants()?;
    cleanup_workspace_staging(&projection.root_path, &job_module)?;
    let artifact_module = ArtifactModule::open(&projection.database_path)?;
    let outline_module = OutlineModule::open(&projection.database_path)?;
    let guide_module = GuideModule::open(&projection.database_path)?;
    let startup_library = paper_module.reconcile()?;
    let generation = state.next_generation.fetch_add(1, Ordering::AcqRel) + 1;
    let stop = Arc::new(AtomicBool::new(false));
    let runtime = Arc::new(WorkspaceRuntime {
        generation,
        root: projection.root_path.clone(),
        paper_module: paper_module.clone(),
        job_module,
        artifact_module,
        outline_module,
        guide_module,
        stop: Arc::clone(&stop),
        notify: Notify::new(),
        claim_gate: Mutex::new(()),
        worker_count: AtomicUsize::new(0),
    });
    swap_runtime(state, runtime.clone())?;
    let library_watcher = match start_library_watcher(
        app,
        &runtime.paper_module,
        vec![
            projection.papers_path.clone(),
            projection.textbooks_path.clone(),
        ],
        stop,
    ) {
        Ok(watcher) => watcher,
        Err(error) => {
            if let Ok(mut slot) = state.runtime.lock() {
                if let Some(installed) = slot.take() {
                    installed.request_stop();
                }
            }
            return Err(error);
        }
    };
    *state
        .library_watcher
        .lock()
        .map_err(|_| "LibraryWatcher lock poisoned".to_string())? = Some(library_watcher);
    spawn_job_workers(app.clone(), runtime.clone());
    spawn_remote_cleanup_worker(app.clone(), runtime.clone());
    let _ = app.emit(
        "read-event",
        ReadEvent {
            cursor: startup_library.cursor,
            kind: "library".to_string(),
            entity_id: None,
            delta: None,
            status: None,
        },
    );
    Ok(workspace_info(&projection.root_path))
}

#[cfg(test)]
pub(crate) fn open_test_runtime(root: &Path, generation: u64) -> Arc<WorkspaceRuntime> {
    let projection = crate::v2_workspace::WorkspaceModule::new()
        .open(root)
        .expect("workspace");
    let paper_module = PaperModule::open(&projection.root_path).expect("paper");
    let job_module = JobModule::open(&projection.database_path).expect("jobs");
    enable_v7_job_schema(&projection.database_path);
    let artifact_module = ArtifactModule::open(&projection.database_path).expect("artifacts");
    let outline_module = OutlineModule::open(&projection.database_path).expect("outline");
    let guide_module = GuideModule::open(&projection.database_path).expect("guide");
    Arc::new(WorkspaceRuntime {
        generation,
        root: projection.root_path,
        paper_module,
        job_module,
        artifact_module,
        outline_module,
        guide_module,
        stop: Arc::new(AtomicBool::new(false)),
        notify: Notify::new(),
        claim_gate: Mutex::new(()),
        worker_count: AtomicUsize::new(0),
    })
}

#[cfg(test)]
fn enable_v7_job_schema(database_path: &Path) {
    let connection = rusqlite::Connection::open(database_path).expect("database");
    let already_enabled = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM pragma_table_info('jobs')
               WHERE name = 'provider_route_id'
             )",
            [],
            |row| row.get::<_, i64>(0),
        )
        .expect("inspect jobs schema")
        != 0;
    if already_enabled {
        return;
    }
    connection
        .execute_batch(
            "DROP INDEX idx_jobs_active_dedupe;
             CREATE TABLE remote_endpoint_snapshots (
               endpoint_scope TEXT PRIMARY KEY,
               version INTEGER NOT NULL,
               owner_type TEXT NOT NULL,
               provider_instance_id TEXT,
               provider_name_at_capture TEXT,
               provider_kind TEXT,
               base_url TEXT,
               created_at TEXT NOT NULL
             );
             CREATE TABLE provider_route_snapshots (
               route_id TEXT PRIMARY KEY,
               endpoint_scope TEXT NOT NULL REFERENCES remote_endpoint_snapshots(endpoint_scope),
               version INTEGER NOT NULL,
               models_json TEXT NOT NULL,
               operation_role TEXT NOT NULL,
               created_at TEXT NOT NULL,
               UNIQUE(endpoint_scope, models_json, operation_role)
             );
             ALTER TABLE jobs ADD COLUMN provider_route_id TEXT
               REFERENCES provider_route_snapshots(route_id);
             ALTER TABLE jobs ADD COLUMN provider_route_origin TEXT;
             CREATE TABLE job_provider_requirements (
               job_id TEXT PRIMARY KEY REFERENCES jobs(id) ON DELETE CASCADE,
               code TEXT NOT NULL,
               provider_kind TEXT,
               provider_instance_id TEXT,
               can_rebind INTEGER NOT NULL,
               safe_details_json TEXT NOT NULL DEFAULT '{}',
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL
             );
             CREATE UNIQUE INDEX jobs_active_route_dedupe
               ON jobs(provider_route_id, dedupe_key)
               WHERE provider_route_id IS NOT NULL
                 AND state IN ('queued', 'running', 'paused', 'interrupted_unknown');
             CREATE UNIQUE INDEX jobs_active_legacy_dedupe
               ON jobs(dedupe_key)
               WHERE provider_route_id IS NULL
                 AND state IN ('queued', 'running', 'paused');
             CREATE INDEX jobs_active_legacy_dedupe_lookup
               ON jobs(dedupe_key, state)
               WHERE provider_route_id IS NULL
                 AND state IN ('queued', 'running', 'paused', 'interrupted_unknown');",
        )
        .expect("v7 job schema");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_module::{JobExecutionRoute, JobSpec, JobState};
    use crate::provider_routing::capture_mistral_ocr_route;
    use crate::AppState;
    use serde_json::json;

    #[test]
    fn current_runtime_is_absent_until_swapped() {
        let state = AppState::default();
        assert!(current_runtime(&state).is_err());
    }

    #[test]
    fn swap_runtime_installs_one_complete_snapshot_and_stops_the_previous() {
        let first_dir = tempfile::tempdir().expect("first");
        let second_dir = tempfile::tempdir().expect("second");
        let first = open_test_runtime(first_dir.path(), 1);
        let second = open_test_runtime(second_dir.path(), 2);
        let state = AppState::default();

        assert!(swap_runtime(&state, first.clone())
            .expect("install first")
            .is_none());
        let seen = current_runtime(&state).expect("first current");
        assert_eq!(seen.generation, 1);
        assert_eq!(seen.root, first.root);
        let _ = seen.paper_module.clone();
        let _ = seen.job_module.clone();
        let _ = seen.artifact_module.clone();
        let _ = seen.outline_module.clone();
        let _ = seen.guide_module.clone();

        let previous = swap_runtime(&state, second.clone())
            .expect("install second")
            .expect("previous runtime");
        assert_eq!(previous.generation, 1);
        assert!(previous.stop.load(Ordering::Acquire));
        assert!(!second.stop.load(Ordering::Acquire));
        let seen = current_runtime(&state).expect("second current");
        assert_eq!(seen.generation, 2);
        assert_eq!(seen.root, second.root);
    }

    #[test]
    fn swap_runtime_stops_previous_runtime_but_not_current() {
        let first_dir = tempfile::tempdir().expect("first");
        let second_dir = tempfile::tempdir().expect("second");
        let first = open_test_runtime(first_dir.path(), 1);
        let second = open_test_runtime(second_dir.path(), 2);
        let state = AppState::default();

        swap_runtime(&state, first).expect("install first");
        assert!(!current_runtime(&state).expect("first current").is_stopped());

        let previous = swap_runtime(&state, second)
            .expect("install second")
            .expect("previous runtime");

        assert!(previous.is_stopped());
        assert!(!current_runtime(&state)
            .expect("second current")
            .is_stopped());
    }
    #[test]
    fn stopped_runtime_does_not_claim_queued_job() {
        let directory = tempfile::tempdir().expect("runtime");
        let runtime = open_test_runtime(directory.path(), 11);
        runtime
            .job_module
            .enqueue(JobSpec {
                kind: "ocr".to_string(),
                provider: Some("mistral".to_string()),
                paper_id: None,
                revision_id: None,
                root_key: None,
                artifact_key: Some("ocr".to_string()),
                dedupe_key: "runtime-stop-claim".to_string(),
                priority: 100,
                payload: json!({}),
            })
            .expect("enqueue");

        runtime.request_stop();

        assert!(runtime
            .claim_next_job()
            .expect("claim stopped runtime")
            .is_none());
        assert_eq!(
            runtime
                .job_module
                .list()
                .expect("list jobs")
                .into_iter()
                .next()
                .expect("queued job")
                .state,
            JobState::Queued
        );
    }

    #[test]
    fn request_stop_waits_for_an_in_flight_claim_linearization() {
        let directory = tempfile::tempdir().expect("runtime");
        let runtime = open_test_runtime(directory.path(), 12);
        let (claim_entered_tx, claim_entered_rx) = std::sync::mpsc::channel();
        let (release_claim_tx, release_claim_rx) = std::sync::mpsc::channel();
        let claiming_runtime = Arc::clone(&runtime);
        let claim_thread = std::thread::spawn(move || {
            claiming_runtime
                .claim_if_running(|| {
                    claim_entered_tx.send(()).expect("report entered claim");
                    release_claim_rx.recv().expect("release claim");
                    Ok(())
                })
                .expect("linearized claim")
        });
        claim_entered_rx.recv().expect("claim entered");

        let (contended_tx, contended_rx) = std::sync::mpsc::channel();
        let stopping_runtime = Arc::clone(&runtime);
        let stop_thread = std::thread::spawn(move || {
            stopping_runtime.request_stop_observing_contention(|| {
                contended_tx.send(()).expect("report contended stop");
            });
        });

        contended_rx
            .recv()
            .expect("stop observed the in-flight claim");
        assert!(
            !runtime.is_stopped(),
            "stop became visible before the in-flight claim left its linearized section"
        );
        release_claim_tx.send(()).expect("release in-flight claim");
        assert_eq!(claim_thread.join().expect("join claim"), Some(()));
        stop_thread.join().expect("join stop");
        assert!(runtime.is_stopped());
    }

    #[test]
    fn claimed_job_retains_its_runtime_after_swap() {
        let first_dir = tempfile::tempdir().expect("first");
        let second_dir = tempfile::tempdir().expect("second");
        let first = open_test_runtime(first_dir.path(), 21);
        let second = open_test_runtime(second_dir.path(), 22);
        first
            .job_module
            .enqueue_record(
                JobSpec {
                    kind: "ocr".to_string(),
                    provider: Some("mistral".to_string()),
                    paper_id: None,
                    revision_id: None,
                    root_key: None,
                    artifact_key: Some("ocr".to_string()),
                    dedupe_key: "runtime-retained-claim".to_string(),
                    priority: 100,
                    payload: json!({}),
                },
                JobExecutionRoute::MistralOcr(
                    capture_mistral_ocr_route("runtime-retained-test-key", "mistral-ocr-latest")
                        .expect("freeze OCR route"),
                ),
            )
            .expect("enqueue");
        let state = AppState::default();
        swap_runtime(&state, first.clone()).expect("install first");

        let claimed = first.claim_next_job().expect("claim").expect("queued job");
        swap_runtime(&state, second.clone()).expect("install second");

        assert!(Arc::ptr_eq(&claimed.runtime, &first));
        assert_eq!(claimed.job.state, JobState::Running);
        assert!(Arc::ptr_eq(
            &current_runtime(&state).expect("current"),
            &second
        ));
    }

    #[test]
    fn app_state_has_no_parallel_workspace_module_locks() {
        let state = AppState::default();
        assert!(current_runtime(&state).is_err());
    }

    #[test]
    fn swap_runtime_does_not_wait_for_old_worker_count() {
        let dir = tempfile::tempdir().expect("dir");
        let runtime = open_test_runtime(dir.path(), 7);
        runtime.worker_count.store(2, Ordering::Release);
        let state = AppState::default();
        swap_runtime(&state, runtime.clone()).expect("install");
        let next_dir = tempfile::tempdir().expect("next");
        let next = open_test_runtime(next_dir.path(), 8);
        let previous = swap_runtime(&state, next).expect("swap").expect("old");
        assert!(previous.stop.load(Ordering::Acquire));
        assert_eq!(previous.worker_count.load(Ordering::Acquire), 2);
    }
}
