#![allow(dead_code)]

use crate::job_module::{
    JobExecutionRoute, JobProjection, RebindProviderRouteError, StoredJobRoute,
};
use crate::provider_routing::{
    ProviderInstanceId, ProviderRouteDecision, ProviderRouting, ProviderSelection,
};
use crate::read_model_settings;
use crate::workspace_lifecycle::{active_job_module, active_root, current_runtime};
use crate::{
    emit_job_event_for_runtime, list_remote_tombstones_from_root, retry_remote_tombstones,
    spawn_job_workers, AppResult, AppState, RemoteTombstoneProjection,
};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RebindProviderJobOutcome {
    pub(crate) job: Option<JobProjection>,
    pub(crate) existing_job_id: Option<String>,
}

pub(crate) fn list_jobs_impl(state: &AppState) -> AppResult<Vec<JobProjection>> {
    active_job_module(state)?.list()
}

pub(crate) fn list_remote_tombstones_impl(
    state: &AppState,
) -> AppResult<Vec<RemoteTombstoneProjection>> {
    list_remote_tombstones_from_root(&active_root(state)?)
}

pub(crate) async fn retry_remote_cleanup_impl(
    app: &tauri::AppHandle,
    tombstone_id: Option<String>,
    state: &AppState,
) -> AppResult<usize> {
    let runtime = current_runtime(state)?;
    retry_remote_tombstones(app, runtime, tombstone_id.as_deref()).await
}

pub(crate) fn pause_job_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    job_id: &str,
) -> AppResult<bool> {
    let runtime = current_runtime(state)?;
    runtime.job_module.pause(job_id)?;
    emit_job_event_for_runtime(app, &runtime, job_id);
    Ok(true)
}

pub(crate) fn resume_job_impl(
    app: tauri::AppHandle,
    state: &AppState,
    job_id: &str,
) -> AppResult<bool> {
    let runtime = current_runtime(state)?;
    runtime.job_module.resume(job_id)?;
    emit_job_event_for_runtime(&app, &runtime, job_id);
    spawn_job_workers(app, runtime);
    Ok(true)
}

pub(crate) fn reprioritize_job_impl(
    app: tauri::AppHandle,
    state: &AppState,
    job_id: &str,
    priority: i64,
) -> AppResult<bool> {
    let runtime = current_runtime(state)?;
    runtime.job_module.reprioritize(job_id, priority)?;
    emit_job_event_for_runtime(&app, &runtime, job_id);
    spawn_job_workers(app, runtime);
    Ok(true)
}

pub(crate) fn cancel_job_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    job_id: &str,
) -> AppResult<bool> {
    let runtime = current_runtime(state)?;
    if let Some(cancellation) = state
        .ocr_cancellations
        .lock()
        .map_err(|_| "OCR cancellation lock poisoned".to_string())?
        .get(job_id)
        .cloned()
    {
        cancellation.cancel();
    }
    if let Some(cancellation) = state
        .artifact_cancellations
        .lock()
        .map_err(|_| "Artifact cancellation lock poisoned".to_string())?
        .get(job_id)
        .cloned()
    {
        cancellation.cancel();
    }
    runtime.job_module.cancel(job_id)?;
    emit_job_event_for_runtime(app, &runtime, job_id);
    Ok(true)
}

pub(crate) fn recheck_provider_job_impl(
    app: tauri::AppHandle,
    state: &AppState,
    job_id: &str,
) -> AppResult<JobProjection> {
    let runtime = current_runtime(state)?;
    let record = runtime.job_module.get_record(job_id)?;
    let StoredJobRoute::Executable(JobExecutionRoute::Paper(frozen)) = &record.route else {
        return Err("Only a captured paper-provider job can be rechecked".to_string());
    };
    let settings = read_model_settings(&app)?;
    match ProviderRouting::new(&settings, state)
        .bind(frozen)
        .map_err(|error| error.to_string())?
    {
        ProviderRouteDecision::Ready(_) => {
            let updated = runtime
                .job_module
                .clear_provider_requirement_after_recheck(job_id, frozen)?;
            emit_job_event_for_runtime(&app, &runtime, job_id);
            spawn_job_workers(app, runtime);
            Ok(updated.project())
        }
        ProviderRouteDecision::ActionRequired(_) => Err(
            "The original Provider instance still needs attention before this job can run"
                .to_string(),
        ),
    }
}

pub(crate) fn rebind_provider_job_impl(
    app: tauri::AppHandle,
    state: &AppState,
    job_id: &str,
    replacement_instance_id: &str,
) -> AppResult<RebindProviderJobOutcome> {
    let runtime = current_runtime(state)?;
    let record = runtime.job_module.get_record(job_id)?;
    let StoredJobRoute::Executable(JobExecutionRoute::Paper(original)) = &record.route else {
        return Err("Only captured paper-provider work can be rebound".to_string());
    };
    let instance_id =
        ProviderInstanceId::parse(replacement_instance_id).map_err(|error| error.to_string())?;
    let settings = read_model_settings(&app)?;
    let replacement = match ProviderRouting::new(&settings, state)
        .capture(
            ProviderSelection::Instance(instance_id),
            original.models().clone(),
            original.operation(),
        )
        .map_err(|error| error.to_string())?
    {
        ProviderRouteDecision::Ready(route) => route,
        ProviderRouteDecision::ActionRequired(_) => {
            return Err(
                "The selected Provider instance must be configured and ready before rebinding"
                    .to_string(),
            )
        }
    };
    match runtime
        .job_module
        .rebind_provider_route(job_id, &replacement)
    {
        Ok(updated) => {
            emit_job_event_for_runtime(&app, &runtime, job_id);
            spawn_job_workers(app, runtime);
            Ok(RebindProviderJobOutcome {
                job: Some(updated.project()),
                existing_job_id: None,
            })
        }
        Err(RebindProviderRouteError::AlreadyActiveOnRoute { existing_job_id }) => {
            Ok(RebindProviderJobOutcome {
                job: Some(record.project()),
                existing_job_id: Some(existing_job_id),
            })
        }
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn abandon_legacy_provider_job_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    job_id: &str,
    confirmed_potential_charge: bool,
) -> AppResult<JobProjection> {
    let runtime = current_runtime(state)?;
    let updated = runtime
        .job_module
        .abandon_legacy_provider_job(job_id, confirmed_potential_charge)?;
    emit_job_event_for_runtime(app, &runtime, job_id);
    Ok(updated.project())
}
