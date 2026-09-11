#![allow(dead_code)]

use crate::workspace_lifecycle::active_root;
use crate::{
    diagnostic_preview_for_root, open_db, AppResult, AppState, AppStats, DiagnosticPreview,
};
use serde_json::json;
use std::fs;
use std::path::PathBuf;

pub(crate) fn preview_diagnostics_impl(state: &AppState) -> AppResult<DiagnosticPreview> {
    diagnostic_preview_for_root(&active_root(state)?)
}

pub(crate) fn export_diagnostics_impl(
    state: &AppState,
    output_path: String,
) -> AppResult<DiagnosticPreview> {
    let output = PathBuf::from(output_path);
    if output.extension().and_then(|value| value.to_str()) != Some("json") {
        return Err("Diagnostic export must use a .json file".to_string());
    }
    let parent = output
        .parent()
        .filter(|parent| parent.is_dir())
        .ok_or_else(|| "Diagnostic export directory is unavailable".to_string())?;
    if !parent.is_absolute() {
        return Err("Diagnostic export path must be absolute".to_string());
    }
    let preview = diagnostic_preview_for_root(&active_root(state)?)?;
    let payload = serde_json::to_vec_pretty(&json!({
        "formatVersion": 1,
        "diagnostic": &preview
    }))
    .map_err(|error| error.to_string())?;
    fs::write(&output, payload)
        .map_err(|error| format!("Unable to export diagnostics: {error}"))?;
    Ok(preview)
}

pub(crate) fn get_stats_impl(state: &AppState) -> AppResult<AppStats> {
    let root = active_root(state)?;
    let conn = open_db(&root)?;
    let documents: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM papers WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let threads: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM discussions WHERE status != 'archived'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let messages: i64 = conn
        .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    let input: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(input_tokens),0) FROM usage_receipts",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let cached: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(cached_input_tokens),0) FROM usage_receipts",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    let output: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(output_tokens),0) FROM usage_receipts",
            [],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(AppStats {
        documents,
        threads,
        messages,
        input_tokens: input,
        cached_tokens: cached,
        output_tokens: output,
        estimated_cost: None,
    })
}
