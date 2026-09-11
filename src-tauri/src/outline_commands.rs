#![allow(dead_code)]

use crate::outline_module::OutlineProjection;
use crate::workspace_lifecycle::current_runtime;
use crate::{
    emit_job_event_for_runtime, AppResult, AppState, OutlineDeepDiveRequest, OutlineRequest,
    OUTLINE_START_LOCK,
};
use serde_json::Value;

pub(crate) fn delete_outline_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    request: OutlineRequest,
) -> AppResult<OutlineProjection> {
    let runtime = current_runtime(state)?;
    let _guard = OUTLINE_START_LOCK.lock().map_err(|_| "地图启动锁不可用")?;
    let jobs = &runtime.job_module;
    if jobs
        .active_of("outline_overview", &request.revision_id)?
        .is_some()
    {
        return Err("Cancel the running Outline from the task center first.".to_string());
    }
    for job in jobs.list_active_of("outline_deep_dive", &request.revision_id)? {
        if let Some(cancellation) = state
            .artifact_cancellations
            .lock()
            .map_err(|_| "Artifact cancellation lock poisoned".to_string())?
            .get(&job.id)
            .cloned()
        {
            cancellation.cancel();
        }
        jobs.cancel(&job.id)?;
        emit_job_event_for_runtime(app, &runtime, &job.id);
    }
    runtime
        .outline_module
        .delete_overview(&request.revision_id)?;
    runtime
        .outline_module
        .project_without_route(&request.revision_id)
}

pub(crate) fn delete_outline_deep_dive_impl(
    state: &AppState,
    request: OutlineDeepDiveRequest,
) -> AppResult<bool> {
    let runtime = current_runtime(state)?;
    let _guard = OUTLINE_START_LOCK.lock().map_err(|_| "地图启动锁不可用")?;
    let jobs = &runtime.job_module;
    let running = jobs
        .list_active_of("outline_deep_dive", &request.revision_id)?
        .into_iter()
        .any(|job| {
            job.payload.get("nodeId").and_then(Value::as_str) == Some(request.node_id.as_str())
        });
    if running {
        return Err("Cancel the running Deep dive from the task center first.".to_string());
    }
    runtime
        .outline_module
        .delete_deep_dive(&request.revision_id, &request.node_id)?;
    Ok(true)
}
