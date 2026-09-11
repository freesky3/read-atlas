#![allow(dead_code)]

use crate::artifact_module::{ArtifactProjection, LensQaProjection, OcrProjection};
use crate::workspace_lifecycle::active_artifact_module;
use crate::{AppResult, AppState};

pub(crate) fn list_artifacts_impl(
    state: &AppState,
    paper_id: &str,
) -> AppResult<Vec<ArtifactProjection>> {
    active_artifact_module(state)?.list(paper_id)
}

pub(crate) fn get_artifact_impl(
    state: &AppState,
    artifact_id: &str,
) -> AppResult<ArtifactProjection> {
    active_artifact_module(state)?.get(artifact_id)
}

pub(crate) fn latest_ocr_impl(
    state: &AppState,
    revision_id: &str,
) -> AppResult<Option<OcrProjection>> {
    active_artifact_module(state)?.latest_ocr(revision_id)
}

pub(crate) fn get_ocr_impl(state: &AppState, ocr_revision_id: &str) -> AppResult<OcrProjection> {
    active_artifact_module(state)?.ocr(ocr_revision_id)
}

pub(crate) fn set_artifact_override_impl(
    state: &AppState,
    artifact_id: &str,
    kind: &str,
    key: &str,
    value: &serde_json::Value,
) -> AppResult<ArtifactProjection> {
    let module = active_artifact_module(state)?;
    module.set_override(artifact_id, kind, key, value)?;
    module.get(artifact_id)
}

pub(crate) fn list_lens_qa_impl(
    state: &AppState,
    lens_artifact_id: &str,
) -> AppResult<Vec<LensQaProjection>> {
    active_artifact_module(state)?.list_lens_qa(lens_artifact_id)
}

pub(crate) fn delete_artifact_impl(
    state: &AppState,
    artifact_id: &str,
) -> AppResult<Option<ArtifactProjection>> {
    active_artifact_module(state)?.delete(artifact_id)
}

pub(crate) fn delete_ocr_cascade_impl(state: &AppState, revision_id: &str) -> AppResult<bool> {
    active_artifact_module(state)?.delete_ocr_cascade(revision_id)
}
