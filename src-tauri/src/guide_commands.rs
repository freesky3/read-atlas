use crate::guide_module::GuideProjection;
use crate::guide_protocol::GUIDE_JOB_KIND;
use crate::workspace_lifecycle::{active_guide_module, active_job_module};
use crate::{emit_job_event, AppResult, AppState};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ReadingGuideRequest {
    pub revision_id: String,
    #[serde(default)]
    pub character_ids: Option<Vec<String>>,
    #[serde(default)]
    pub plan_id: Option<String>,
    #[serde(default)]
    pub plan_digest: Option<String>,
}

pub(crate) fn delete_reading_guide_impl(
    app: &tauri::AppHandle,
    state: &AppState,
    request: ReadingGuideRequest,
) -> AppResult<GuideProjection> {
    let jobs = active_job_module(state)?;
    for job in jobs.list_active_of(GUIDE_JOB_KIND, &request.revision_id)? {
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
        emit_job_event(app, &job.id);
    }
    active_guide_module(state)?.delete(&request.revision_id)?;
    active_guide_module(state)?.project_without_route(&request.revision_id)
}
