mod annotation_module;
mod api_key_links;
mod artifact_commands;
mod artifact_module;
mod auxiliary_contract;
mod auxiliary_state;
mod chapter_sort;
mod chat_completions;
mod db;
mod diagnostic_commands;
mod discussion_commands;
mod document_artifacts;
mod export_module;
mod guide_batching;
mod guide_cast;
mod guide_catalog;
mod guide_character_assets;
mod guide_character_settings;
mod guide_commands;
mod guide_coverage;
mod guide_generation;
mod guide_memo;
mod guide_module;
mod guide_personas;
mod guide_pipeline;
mod guide_plan;
mod guide_protocol;
mod guide_runtime;
mod guide_validate;
mod guide_validate_v2;
mod job_commands;
mod job_module;
mod lens_contract;
mod library_batch;
mod library_commands;
mod library_cost;
mod library_lifecycle;
mod library_paths;
mod library_query;
mod library_watcher;
mod library_workflow;
mod model_settings;
mod outline_catalog;
mod outline_commands;
mod outline_generation;
mod outline_map;
mod outline_module;
mod outline_plan;
mod outline_protocol;
mod outline_runtime;
mod outline_validate;
mod paper_module;
mod prompt_settings;
mod provider_ports;
mod provider_routing;
mod reader_context;
mod reading_artifact_module;
pub(crate) mod roadmap_module;
mod textbook_contract;
mod translation_contract;
mod ui_locale;
mod v2_workspace;
mod webview_pinch;
mod workspace_lifecycle;

use annotation_module::{
    AnnotationLocator, UserAnnotation, UserAnnotationInput, UserAnnotationLink,
    UserAnnotationLinkInput, UserAnnotationUpdateRequest,
};
use artifact_module::{
    ArtifactDraft, ArtifactProjection, EvidenceAnchor, LensQaProjection, OcrProjection,
};
use job_module::StoredJobRoute;
use job_module::{ExitPreparation, JobExecutionRoute, JobModule, JobProjection, JobSpec, JobState};
use library_paths::DocumentKind;
use library_watcher::LibraryWatcher;
use library_workflow::{bump_library_revisions, LibraryDomain};
pub use model_settings::ModelSettingsView;
use model_settings::{
    parse_model_settings_json, PendingConnectionTest, ReadyPaperProvider, StoredModelSettings,
};
use outline_catalog::outline_context_exceeds_window;
use outline_module::{OutlinePlan, OutlineProjection};
use paper_module::{LibraryProjection, PaperModule, ReadingState, StorageReport, TrashProjection};
use prompt_settings::{
    apply_language_lock, apply_placeholders, load_store, load_store_in, prompt_settings_path,
    resolved_text, restore_default, restore_default_in, restore_previous, restore_previous_in,
    save_slot, save_slot_in, validate_slot_text, PromptSlotId,
};
use provider_ports::{
    normalize_staged_ocr, CancellationFlag, MistralOcrAdapter, OcrPort, OcrRequest,
    PaperInteractionKind, PaperInteractionOutcome, PaperInteractionRequest, PaperModelCapabilities,
    PaperModelPort, PaperStreamRequest, ProviderError, ProviderErrorKind, RemoteEndpointDeletePort,
    RemoteResource, TextInteractionOutcome, TextInteractionRequest, UsageEnvelope,
};
use provider_routing::{
    capture_mistral_ocr_route, load_frozen_mistral_ocr_route, load_frozen_route,
    persist_frozen_route, verify_mistral_ocr_endpoint_scope, BoundProviderRoute,
    FrozenMistralOcrRoute, FrozenModels, ModelRole, ProviderInstanceId, ProviderRequirement,
    ProviderRequirementCode, ProviderRouteDecision, ProviderRouting, ProviderSelection,
};
#[cfg(test)]
use provider_routing::{open_paper_adapter, PaperAdapter};
use reading_artifact_module::{
    normalize_markdown_field, route_scoped_context_epoch, AskLensRequest,
    GenerateReadingArtifactRequest, LensQaTurn, ReadingArtifactAction, ReadingArtifactModule,
};
use v2_workspace::WorkspaceModule;
pub(crate) use workspace_lifecycle::{
    active_artifact_module, active_guide_module, active_job_module, active_outline_module,
    active_paper_module, active_root, cancel_runtime_work, current_runtime, WorkspaceRuntime,
};

use chrono::Utc;
use keyring::{Entry, Error as KeyringError};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    Emitter, Manager, State,
};
use tokio::sync::Notify;
use uuid::Uuid;

pub(crate) type AppResult<T> = Result<T, String>;

const CREDENTIAL_SERVICE: &str = "com.skywalker.read-desktop";
const MISTRAL_CREDENTIAL_USER: &str = "mistral-api-key";
const MISTRAL_OCR_MODEL: &str = "mistral-ocr-latest";

#[derive(Default)]
pub struct AppState {
    pub(crate) workspace_module: WorkspaceModule,
    pub(crate) library_watcher: Mutex<Option<LibraryWatcher>>,
    pub(crate) ocr_cancellations: Mutex<HashMap<String, CancellationFlag>>,
    pub(crate) artifact_cancellations: Mutex<HashMap<String, CancellationFlag>>,
    pub(crate) discussion_cancellations: Mutex<HashMap<String, CancellationFlag>>,
    pub(crate) exit_authorized: AtomicBool,
    pub(crate) mistral_api_key: Mutex<Option<String>>,
    pub(crate) provider_keys: Mutex<HashMap<String, String>>,
    pub(crate) pending_provider_tests: Mutex<HashMap<String, PendingConnectionTest>>,
    pub(crate) runtime: Mutex<Option<Arc<WorkspaceRuntime>>>,
    pub(crate) next_generation: AtomicU64,
    #[allow(dead_code)]
    pub(crate) remote_cleanup_running: AtomicBool,
    #[allow(dead_code)]
    pub(crate) remote_cleanup_notify: Notify,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    pub root_path: String,
    pub database_path: String,
    pub library_path: String,
    pub textbooks_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unmanaged_pdf_notice: Option<String>,
    pub available: bool,
    pub status_detail: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadEvent {
    pub cursor: String,
    pub kind: String,
    pub entity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteTombstoneProjection {
    pub id: String,
    pub provider: String,
    pub resource_kind: String,
    pub paper_id: Option<String>,
    pub state: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub ownership_status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GeminiModelOption {
    pub id: String,
    pub display_name: String,
    pub description: String,
    pub input_token_limit: Option<i64>,
    pub output_token_limit: Option<i64>,
    pub supports_generate_content: bool,
    #[serde(default)]
    pub supports_native_pdf: bool,
    #[serde(default)]
    pub supports_interactions: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeminiConnectionTest {
    pub models: Vec<GeminiModelOption>,
    pub tested_at: String,
    pub using_stored_credential: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTestResult {
    pub models: Vec<GeminiModelOption>,
    pub tested_at: String,
    pub using_stored_credential: bool,
    pub paper_probe_passed: Option<bool>,
    pub paper_probe_error: Option<String>,
    pub models_fetch_error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddProviderRequest {
    pub name: String,
    pub kind: model_settings::ProviderKind,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderIdRequest {
    pub id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameProviderRequest {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderProvidersRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestProviderRequest {
    pub id: String,
    #[serde(default)]
    pub kind: Option<model_settings::ProviderKind>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub paper_model: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMistralCredentialInput {
    pub api_key: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiModelsResponse {
    #[serde(default)]
    models: Vec<GeminiApiModel>,
    next_page_token: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeminiApiModel {
    name: String,
    display_name: Option<String>,
    description: Option<String>,
    input_token_limit: Option<i64>,
    output_token_limit: Option<i64>,
    #[serde(default)]
    supported_generation_methods: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentCard {
    pub id: String,
    pub revision_id: String,
    pub title: String,
    pub authors: String,
    pub year: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year_label: Option<String>,
    pub pages: i64,
    pub collection: String,
    pub kind: String,
    pub sha256: String,
    pub pdf_path: String,
    pub source_status: String,
    pub brief_status: String,
    pub has_ocr: bool,
    pub brief_takeaway: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chapter_number: Option<String>,
    pub imported_at: String,
    pub file_name: String,
    #[serde(default)]
    pub lifecycle_status: String,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub read_later: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_at: Option<String>,
    #[serde(default)]
    pub lifecycle_version: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub furthest_page: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_opened_at: Option<String>,
    #[serde(default)]
    pub ocr_failed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Citation {
    pub revision_id: String,
    pub page: Option<i64>,
    pub region: Option<Region>,
    pub region_hash: Option<String>,
    pub excerpt: Option<String>,
    pub confidence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Region {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReceipt {
    pub input_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub uncached_input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub cache_hit_rate: Option<f64>,
    pub latency_ms: Option<i64>,
    pub estimated_cost: Option<String>,
    pub file_reuse: Option<bool>,
    pub session_resume: Option<bool>,
    pub paper_root_branch: Option<bool>,
    pub context_epoch: Option<String>,
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Message {
    pub id: String,
    pub thread_id: String,
    pub parent_id: Option<String>,
    pub role: String,
    pub content: String,
    pub citations: Vec<Citation>,
    pub block_quotes: Vec<BlockQuoteSnapshot>,
    pub status: String,
    pub created_at: String,
    pub usage: Option<UsageReceipt>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockQuoteSnapshot {
    pub revision_id: String,
    pub ocr_revision_id: String,
    pub block_id: String,
    pub page_number: i64,
    pub block_index: i64,
    pub block_type: String,
    pub text_content: String,
    pub content_digest: String,
    pub bbox: [i64; 4],
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: String,
    pub revision_id: String,
    pub kind: String,
    pub title: String,
    pub status: String,
    pub active_message_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Brief {
    pub revision_id: String,
    pub version: i64,
    pub status: String,
    #[serde(default)]
    pub takeaway: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub classification: String,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub background_and_problem: String,
    #[serde(default)]
    pub core_method: String,
    #[serde(default)]
    pub findings: String,
    #[serde(default)]
    pub evaluation: String,
    #[serde(default)]
    pub future_work: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub research_question: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub limitations: String,
    #[serde(default)]
    pub evidence: Vec<Citation>,
    pub model: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStats {
    pub documents: i64,
    pub threads: i64,
    pub messages: i64,
    pub input_tokens: i64,
    pub cached_tokens: i64,
    pub output_tokens: i64,
    pub estimated_cost: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticPreview {
    pub generated_at: String,
    pub included_sections: Vec<String>,
    pub excluded_data: Vec<String>,
    pub summary: Value,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitIntent {
    ContinueInTray,
    PauseAndExit,
    CancelAndExit,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitResolution {
    pub mode: String,
    pub jobs: ExitPreparation,
    pub cancelled_discussions: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePaperResult {
    pub deleted: bool,
    pub cleanup_warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatTurn {
    pub user: Message,
    pub assistant: Message,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRequest {
    pub revision_id: String,
    pub thread_id: String,
    pub parent_id: Option<String>,
    pub question: String,
    pub page: Option<i64>,
    #[serde(default)]
    pub block_ids: Vec<String>,
    #[serde(default)]
    pub regenerate_from_id: Option<String>,
}

struct PreparedChatTurn {
    user_id: String,
    insert_user: bool,
    question: String,
    user_parent_id: Option<String>,
    history_head_id: Option<String>,
    block_quotes: Vec<BlockQuoteSnapshot>,
}

fn load_stored_block_quotes(
    connection: &Connection,
    message_id: &str,
) -> AppResult<Vec<BlockQuoteSnapshot>> {
    Ok(connection
        .query_row(
            "SELECT block_quotes_json FROM message_contexts WHERE message_id = ?1",
            params![message_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default())
}

fn prepare_chat_turn(
    connection: &Connection,
    request: &ChatRequest,
    head: Option<String>,
) -> AppResult<PreparedChatTurn> {
    if let Some(source_id) = request
        .regenerate_from_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let (id, parent_id, role, content): (String, Option<String>, String, String) = connection
            .query_row(
                "SELECT id, parent_id, role, content
                 FROM messages
                 WHERE id = ?1 AND discussion_id = ?2",
                params![source_id, request.thread_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| {
                "The message to regenerate does not belong to this Discussion".to_string()
            })?;
        let (user_id, user_parent_id, question) = if role == "assistant" {
            let user_id = parent_id.filter(|value| !value.is_empty()).ok_or_else(|| {
                "Cannot regenerate an assistant message without a parent question".to_string()
            })?;
            let (user_parent_id, question): (Option<String>, String) = connection
                .query_row(
                    "SELECT parent_id, content FROM messages
                     WHERE id = ?1 AND discussion_id = ?2 AND role = 'user'",
                    params![user_id, request.thread_id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .map_err(|_| "The parent user message for regenerate was not found".to_string())?;
            (user_id, user_parent_id, question)
        } else if role == "user" {
            (id, parent_id, content)
        } else {
            return Err("Only user or assistant messages can be regenerated".to_string());
        };
        if question.trim().is_empty() {
            return Err("The original question is empty and cannot be regenerated".to_string());
        }
        return Ok(PreparedChatTurn {
            block_quotes: load_stored_block_quotes(connection, &user_id)?,
            history_head_id: user_parent_id.clone(),
            user_id: Uuid::new_v4().to_string(),
            insert_user: true,
            question,
            user_parent_id,
        });
    }

    let question = request.question.trim().to_string();
    if question.is_empty() {
        return Err("Question cannot be empty".to_string());
    }
    let user_parent_id = request.parent_id.clone().or(head);
    Ok(PreparedChatTurn {
        block_quotes: load_block_quote_snapshots(
            connection,
            &request.revision_id,
            &request.block_ids,
        )?,
        user_id: Uuid::new_v4().to_string(),
        insert_user: true,
        question,
        user_parent_id: user_parent_id.clone(),
        history_head_id: user_parent_id,
    })
}

#[derive(Debug, Clone)]
struct RevisionRecord {
    id: String,
    title: String,
    sha256: String,
    pdf_path: PathBuf,
    page_count: Option<i64>,
}

pub(crate) fn now() -> String {
    Utc::now().to_rfc3339()
}

pub(crate) fn workspace_info(root: &Path) -> WorkspaceInfo {
    let available = root.is_dir() && db_path(root).is_file();
    WorkspaceInfo {
        root_path: root.to_string_lossy().to_string(),
        database_path: db_path(root).to_string_lossy().to_string(),
        library_path: root.join("Papers").to_string_lossy().to_string(),
        textbooks_path: root.join("Textbooks").to_string_lossy().to_string(),
        unmanaged_pdf_notice: crate::library_paths::unmanaged_pdf_notice(root),
        available,
        status_detail: if available {
            "Workspace 可用".to_string()
        } else {
            "Workspace 路径或数据库当前不可访问".to_string()
        },
    }
}

fn normalize_model_id(value: &str) -> AppResult<String> {
    let trimmed = value.trim();
    let id = trimmed.strip_prefix("models/").unwrap_or(trimmed);
    if id.is_empty()
        || !id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
    {
        return Err("Gemini 模型 ID 无效".to_string());
    }
    Ok(id.to_string())
}

fn model_settings_path(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("无法定位应用配置目录：{error}"))?;
    Ok(directory.join("model-settings.json"))
}

pub(crate) fn read_model_settings(app: &tauri::AppHandle) -> AppResult<StoredModelSettings> {
    let path = model_settings_path(app)?;
    if !path.exists() {
        return Ok(StoredModelSettings::default());
    }
    let raw = fs::read_to_string(&path).map_err(|error| format!("无法读取模型配置：{error}"))?;
    parse_model_settings_json(&raw)
}

fn write_model_settings(app: &tauri::AppHandle, settings: &StoredModelSettings) -> AppResult<()> {
    let path = model_settings_path(app)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("无法创建应用配置目录：{error}"))?;
    }
    let payload = serde_json::to_vec_pretty(settings)
        .map_err(|error| format!("无法序列化模型配置：{error}"))?;
    fs::write(path, payload).map_err(|error| format!("无法保存模型配置：{error}"))
}

fn provider_credential_user(instance_id: &str) -> String {
    format!("provider-{instance_id}")
}

fn provider_credential_entry(instance_id: &str) -> AppResult<Entry> {
    Entry::new(CREDENTIAL_SERVICE, &provider_credential_user(instance_id))
        .map_err(|error| format!("无法访问 Windows Credential Manager：{error}"))
}

fn read_provider_credential(instance_id: &str) -> AppResult<Option<String>> {
    let entry = provider_credential_entry(instance_id)?;
    match entry.get_password() {
        Ok(secret) if !secret.trim().is_empty() => Ok(Some(secret)),
        Ok(_) | Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(format!("无法读取 Provider 凭据：{error}")),
    }
}

fn write_provider_credential(instance_id: &str, secret: &str) -> AppResult<()> {
    provider_credential_entry(instance_id)?
        .set_password(secret)
        .map_err(|error| format!("无法保存 Provider 凭据：{error}"))
}

fn delete_provider_credential(instance_id: &str) -> AppResult<()> {
    let entry = provider_credential_entry(instance_id)?;
    match entry.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(format!("无法清除 Provider 凭据：{error}")),
    }
}

fn resolve_provider_key(instance_id: &str, state: &AppState) -> AppResult<Option<String>> {
    if let Some(secret) = state
        .provider_keys
        .lock()
        .map_err(|_| "provider keys lock poisoned".to_string())?
        .get(instance_id)
        .cloned()
    {
        return Ok(Some(secret));
    }
    let stored = read_provider_credential(instance_id)?;
    if let Some(secret) = &stored {
        state
            .provider_keys
            .lock()
            .map_err(|_| "provider keys lock poisoned".to_string())?
            .insert(instance_id.to_string(), secret.clone());
    }
    Ok(stored)
}

impl provider_routing::ProviderCredentialPort for AppState {
    fn read_exact(
        &self,
        instance_id: &provider_routing::ProviderInstanceId,
    ) -> Result<Option<String>, provider_routing::ProviderRoutingError> {
        resolve_provider_key(instance_id.as_str(), self)
            .map_err(|_| provider_routing::ProviderRoutingError::CredentialStoreUnavailable)
    }
}

fn mistral_credential_entry() -> AppResult<Entry> {
    Entry::new(CREDENTIAL_SERVICE, MISTRAL_CREDENTIAL_USER)
        .map_err(|error| format!("Unable to access Windows Credential Manager: {error}"))
}

fn read_mistral_credential() -> AppResult<Option<String>> {
    let entry = mistral_credential_entry()?;
    match entry.get_password() {
        Ok(secret) if !secret.trim().is_empty() => Ok(Some(secret)),
        Ok(_) | Err(KeyringError::NoEntry) => Ok(None),
        Err(error) => Err(format!("Unable to read Mistral credential: {error}")),
    }
}

fn write_mistral_credential(secret: &str) -> AppResult<()> {
    mistral_credential_entry()?
        .set_password(secret)
        .map_err(|error| format!("Unable to save Mistral credential: {error}"))
}

fn delete_mistral_credential() -> AppResult<()> {
    let entry = mistral_credential_entry()?;
    match entry.delete_credential() {
        Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
        Err(error) => Err(format!("Unable to clear Mistral credential: {error}")),
    }
}

fn resolve_mistral_key(state: &AppState) -> AppResult<Option<String>> {
    if let Some(secret) = state
        .mistral_api_key
        .lock()
        .map_err(|_| "Mistral API key lock poisoned".to_string())?
        .clone()
    {
        return Ok(Some(secret));
    }
    let stored = read_mistral_credential()?;
    if let Some(secret) = &stored {
        *state
            .mistral_api_key
            .lock()
            .map_err(|_| "Mistral API key lock poisoned".to_string())? = Some(secret.clone());
    }
    Ok(stored)
}

fn project_model_settings_view(
    settings: StoredModelSettings,
    state: &AppState,
) -> AppResult<ModelSettingsView> {
    let mut cred_map = HashMap::new();
    for inst in &settings.providers {
        let has_key = resolve_provider_key(&inst.id, state)?.is_some();
        cred_map.insert(inst.id.clone(), has_key);
    }
    let mistral_configured = resolve_mistral_key(state)?.is_some();
    Ok(model_settings::model_settings_view(
        &settings,
        &cred_map,
        mistral_configured,
    ))
}

fn current_model_settings_view(
    app: &tauri::AppHandle,
    state: &AppState,
) -> AppResult<ModelSettingsView> {
    project_model_settings_view(read_model_settings(app)?, state)
}

fn model_option_from_api(model: GeminiApiModel) -> Option<GeminiModelOption> {
    let supports_generate_content = model
        .supported_generation_methods
        .iter()
        .any(|method| method == "generateContent");
    let is_gemini_document_family = model
        .name
        .strip_prefix("models/")
        .unwrap_or(&model.name)
        .starts_with("gemini-");
    if !supports_generate_content || !is_gemini_document_family {
        return None;
    }
    let id = normalize_model_id(&model.name).ok()?;
    let supports_interactions = id.starts_with("gemini-2.5-") || id.starts_with("gemini-3");
    let supports_native_pdf = supports_interactions;
    Some(GeminiModelOption {
        display_name: model.display_name.unwrap_or_else(|| id.clone()),
        description: model.description.unwrap_or_default(),
        id,
        input_token_limit: model.input_token_limit,
        output_token_limit: model.output_token_limit,
        supports_generate_content,
        supports_native_pdf,
        supports_interactions,
    })
}

fn gemini_error(status: reqwest::StatusCode, payload: &Value) -> String {
    let detail = payload
        .pointer("/error/message")
        .and_then(Value::as_str)
        .unwrap_or("Gemini 未返回错误说明");
    format!("Gemini 连接失败（{}）：{}", status.as_u16(), detail)
}

async fn fetch_gemini_models(api_key: &str) -> AppResult<Vec<GeminiModelOption>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("无法创建 Gemini 客户端：{error}"))?;
    let mut next_page_token: Option<String> = None;
    let mut models = Vec::new();
    for _ in 0..5 {
        let mut request = client
            .get("https://generativelanguage.googleapis.com/v1beta/models")
            .query(&[("key", api_key), ("pageSize", "100")]);
        if let Some(token) = &next_page_token {
            request = request.query(&[("pageToken", token)]);
        }
        let response = request
            .send()
            .await
            .map_err(|error| format!("无法连接 Gemini：{error}"))?;
        let status = response.status();
        let payload: Value = response
            .json()
            .await
            .map_err(|error| format!("Gemini 模型列表响应无法解析：{error}"))?;
        if !status.is_success() {
            return Err(gemini_error(status, &payload));
        }
        let page: GeminiModelsResponse = serde_json::from_value(payload)
            .map_err(|error| format!("Gemini 模型列表格式无效：{error}"))?;
        models.extend(page.models.into_iter().filter_map(model_option_from_api));
        next_page_token = page.next_page_token.filter(|token| !token.is_empty());
        if next_page_token.is_none() {
            break;
        }
    }
    models.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
            .then_with(|| left.id.cmp(&right.id))
    });
    models.dedup_by(|left, right| left.id == right.id);
    if models.is_empty() {
        return Err("连接成功，但当前 key 没有返回可用于 PDF Chat 的生成模型".to_string());
    }
    Ok(models)
}
async fn validate_mistral_key(api_key: &str) -> AppResult<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("Unable to create Mistral client: {error}"))?;
    let response = client
        .get(format!(
            "https://api.mistral.ai/v1/models/{MISTRAL_OCR_MODEL}"
        ))
        .bearer_auth(api_key)
        .send()
        .await
        .map_err(|error| format!("Unable to connect to Mistral: {error}"))?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let payload: Value = response.json().await.unwrap_or(Value::Null);
    let detail = payload
        .pointer("/message")
        .and_then(Value::as_str)
        .or_else(|| payload.pointer("/error/message").and_then(Value::as_str))
        .unwrap_or("Mistral did not return an error description");
    Err(format!(
        "Mistral connection failed ({}): {detail}",
        status.as_u16()
    ))
}

/// Hub 卡片需要的派生字段。逐条与批量路径共用同一个读取器，所以聚合投影不可能
/// 给出与单卡路径不一致的卡片值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PaperCardExtras {
    brief_status: String,
    brief_takeaway: Option<String>,
    keywords: Vec<String>,
    has_ocr: bool,
    chapter_number: Option<String>,
    lifecycle_status: String,
    favorite: bool,
    priority: i64,
    read_later: bool,
    review_at: Option<String>,
    lifecycle_version: i64,
    furthest_page: Option<i64>,
    last_opened_at: Option<String>,
    ocr_failed: bool,
}

impl PaperCardExtras {
    /// 派生数据完全读不到时的卡片值。
    fn unavailable() -> Self {
        Self {
            brief_status: "missing".to_string(),
            brief_takeaway: None,
            keywords: Vec::new(),
            has_ocr: false,
            chapter_number: None,
            lifecycle_status: "unread".to_string(),
            favorite: false,
            priority: 0,
            read_later: false,
            review_at: None,
            lifecycle_version: 0,
            furthest_page: None,
            last_opened_at: None,
            ocr_failed: false,
        }
    }
}

/// 一次聚合投影实际发出的 SQL 往返数，用于守住「Hub 加载不随 Paper 数线性增长」。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct HubProjectionStats {
    pub statement_count: usize,
}

/// 分块 `IN (…)` 每次绑定的 id 上限，远低于 SQLite 的变量上限。
const HUB_PROJECTION_CHUNK: usize = 400;

fn in_placeholders(count: usize) -> String {
    let mut sql = String::with_capacity(count * 2);
    for index in 0..count {
        if index > 0 {
            sql.push(',');
        }
        sql.push('?');
    }
    sql
}

/// 执行一条以 `IN (…)` 收尾的查询；`ids` 必须与占位符数量一致且非空。
fn query_ids_in<T, F>(
    connection: &Connection,
    sql: &str,
    ids: &[&str],
    read: F,
) -> AppResult<Vec<T>>
where
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut statement = connection
        .prepare_cached(sql)
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(ids.iter().copied()), read)
        .map_err(|error| error.to_string())?
        .map(|row| row.map_err(|error| error.to_string()))
        .collect::<AppResult<Vec<T>>>()?;
    Ok(rows)
}

/// 判定共用：queued Job 优先于已有产物，产物不存在时才看终态失败的 Job。
fn brief_fields(
    queued: bool,
    brief: Option<(String, String)>,
    failed: bool,
) -> (String, Option<String>, Vec<String>) {
    if queued {
        return ("queued".to_string(), None, Vec::new());
    }
    if let Some((status, content_json)) = brief {
        let parsed: Option<Value> = serde_json::from_str(&content_json).ok();
        let takeaway = parsed
            .as_ref()
            .and_then(|val| {
                val.get("takeaway")
                    .or_else(|| val.get("summary"))
                    .or_else(|| val.get("findings"))
                    .or_else(|| val.get("researchQuestion"))
                    .and_then(Value::as_str)
                    .map(|s| s.trim().to_string())
            })
            .filter(|s| !s.is_empty());
        let keywords = parsed
            .as_ref()
            .and_then(|val| {
                val.get("keywords").and_then(Value::as_array).map(|arr| {
                    arr.iter()
                        .filter_map(Value::as_str)
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect::<Vec<_>>()
                })
            })
            .unwrap_or_default();
        let final_status = if status == "ready" {
            "ready".to_string()
        } else {
            status
        };
        return (final_status, takeaway, keywords);
    }
    if failed {
        ("failed".to_string(), None, Vec::new())
    } else {
        ("missing".to_string(), None, Vec::new())
    }
}

fn chapter_number_from_metadata(content_json: &str) -> Option<String> {
    serde_json::from_str::<Value>(content_json)
        .ok()
        .and_then(|value| {
            (if value["_format"] == "auxiliary-v2" {
                value.get("_display")
            } else {
                Some(&value)
            })
            .and_then(|value| value.get("chapterNumber"))
            .and_then(Value::as_str)
            .map(|text| text.trim().to_string())
        })
        .filter(|text| !text.is_empty())
}

/// D-063 §10：Hub 一次读的聚合投影。把逐 Paper 的 `open_db` 与派生查询压缩成按
/// id 分块的固定几条语句；键为 `(paper_id, revision_id)`，结果按 `paper_id` 索引。
pub(crate) fn load_paper_card_extras(
    connection: &Connection,
    keys: &[(String, String)],
) -> AppResult<(HashMap<String, PaperCardExtras>, HubProjectionStats)> {
    let mut stats = HubProjectionStats::default();
    let mut extras = HashMap::with_capacity(keys.len());
    if keys.is_empty() {
        return Ok((extras, stats));
    }
    let paper_ids: Vec<&str> = keys.iter().map(|(paper_id, _)| paper_id.as_str()).collect();
    let revision_ids: Vec<&str> = keys
        .iter()
        .map(|(_, revision_id)| revision_id.as_str())
        .collect();

    let mut ocr_revisions: HashSet<String> = HashSet::new();
    let mut queued_revisions: HashSet<String> = HashSet::new();
    let mut failed_revisions: HashSet<String> = HashSet::new();
    for chunk in revision_ids.chunks(HUB_PROJECTION_CHUNK) {
        stats.statement_count += 1;
        let sql = format!(
            "SELECT DISTINCT revision_id FROM ocr_revisions WHERE revision_id IN ({})",
            in_placeholders(chunk.len())
        );
        ocr_revisions.extend(query_ids_in(connection, &sql, chunk, |row| {
            row.get::<_, String>(0)
        })?);
        stats.statement_count += 1;
        let sql = format!(
            "SELECT DISTINCT revision_id FROM jobs
             WHERE kind = 'orientation_pack'
               AND state IN ('queued', 'running', 'paused')
               AND revision_id IN ({})",
            in_placeholders(chunk.len())
        );
        queued_revisions.extend(query_ids_in(connection, &sql, chunk, |row| {
            row.get::<_, String>(0)
        })?);
        stats.statement_count += 1;
        let sql = format!(
            "SELECT DISTINCT revision_id FROM jobs
             WHERE kind = 'orientation_pack'
               AND state IN ('failed', 'interrupted_unknown')
               AND revision_id IN ({})",
            in_placeholders(chunk.len())
        );
        failed_revisions.extend(query_ids_in(connection, &sql, chunk, |row| {
            row.get::<_, String>(0)
        })?);
    }

    // `artifact_heads` 的 PK 是 (paper_id, kind, object_key)，所以每种 kind 每题最多一行。
    let mut latest_ocr_jobs: HashMap<String, String> = HashMap::new();
    for chunk in revision_ids.chunks(HUB_PROJECTION_CHUNK) {
        stats.statement_count += 1;
        let sql = format!(
            "SELECT j.revision_id, j.state FROM jobs j
             WHERE j.kind = 'ocr' AND j.revision_id IN ({})
               AND NOT EXISTS (
                 SELECT 1 FROM jobs newer
                  WHERE newer.revision_id = j.revision_id AND newer.kind = 'ocr'
                    AND (newer.updated_at > j.updated_at
                         OR (newer.updated_at = j.updated_at AND newer.id > j.id))
               )",
            in_placeholders(chunk.len())
        );
        for (revision_id, state) in query_ids_in(connection, &sql, chunk, |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            latest_ocr_jobs.entry(revision_id).or_insert(state);
        }
    }

    let mut brief_rows: HashMap<String, (String, String)> = HashMap::new();
    let mut metadata_rows: HashMap<String, String> = HashMap::new();
    let mut lifecycle_rows: HashMap<String, (String, bool, i64, bool, Option<String>, i64)> =
        HashMap::new();
    let mut engagement_rows: HashMap<String, (String, i64, String)> = HashMap::new();
    for chunk in paper_ids.chunks(HUB_PROJECTION_CHUNK) {
        stats.statement_count += 1;
        let sql = format!(
            "SELECT h.paper_id, a.status, a.content_json FROM artifacts a
             JOIN artifact_heads h ON h.artifact_id = a.id
             WHERE h.kind = 'brief' AND h.object_key = '' AND h.paper_id IN ({})",
            in_placeholders(chunk.len())
        );
        for (paper_id, status, content_json) in query_ids_in(connection, &sql, chunk, |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })? {
            brief_rows
                .entry(paper_id)
                .or_insert_with(|| (status, content_json));
        }
        stats.statement_count += 1;
        let sql = format!(
            "SELECT h.paper_id, a.content_json FROM artifacts a
             JOIN artifact_heads h ON h.artifact_id = a.id
             WHERE h.kind = 'metadata' AND h.object_key = '' AND h.paper_id IN ({})",
            in_placeholders(chunk.len())
        );
        for (paper_id, content_json) in query_ids_in(connection, &sql, chunk, |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })? {
            metadata_rows.entry(paper_id).or_insert(content_json);
        }
        stats.statement_count += 1;
        let sql = format!(
            "SELECT paper_id, status, favorite, priority, read_later, review_at, version
             FROM paper_lifecycle WHERE paper_id IN ({})",
            in_placeholders(chunk.len())
        );
        for (paper_id, status, favorite, priority, read_later, review_at, version) in
            query_ids_in(connection, &sql, chunk, |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)? != 0,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)? != 0,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })?
        {
            lifecycle_rows
                .entry(paper_id)
                .or_insert((status, favorite, priority, read_later, review_at, version));
        }
        stats.statement_count += 1;
        let sql = format!(
            "SELECT paper_id, revision_id, furthest_page, last_opened_at
             FROM reading_engagement WHERE paper_id IN ({})",
            in_placeholders(chunk.len())
        );
        for (paper_id, revision_id, furthest_page, last_opened_at) in
            query_ids_in(connection, &sql, chunk, |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
        {
            engagement_rows.insert(
                format!("{paper_id}\u{1f}{revision_id}"),
                (revision_id, furthest_page, last_opened_at),
            );
        }
    }

    for (paper_id, revision_id) in keys {
        let (brief_status, brief_takeaway, keywords) = brief_fields(
            queued_revisions.contains(revision_id),
            brief_rows.get(paper_id).cloned(),
            failed_revisions.contains(revision_id),
        );
        let chapter_number = metadata_rows
            .get(paper_id)
            .and_then(|content_json| chapter_number_from_metadata(content_json));
        let lifecycle = lifecycle_rows.get(paper_id);
        let engagement = engagement_rows.get(&format!("{paper_id}\u{1f}{revision_id}"));
        let ocr_job_state = latest_ocr_jobs.get(revision_id).map(String::as_str);
        extras.insert(
            paper_id.clone(),
            PaperCardExtras {
                brief_status,
                brief_takeaway,
                keywords,
                has_ocr: ocr_revisions.contains(revision_id),
                chapter_number,
                lifecycle_status: lifecycle
                    .map(|(status, _, _, _, _, _)| status.clone())
                    .unwrap_or_else(|| "unread".to_string()),
                favorite: lifecycle.map(|row| row.1).unwrap_or(false),
                priority: lifecycle.map(|row| row.2).unwrap_or(0),
                read_later: lifecycle.map(|row| row.3).unwrap_or(false),
                review_at: lifecycle.and_then(|row| row.4.clone()),
                lifecycle_version: lifecycle.map(|row| row.5).unwrap_or(0),
                furthest_page: engagement.map(|row| row.1),
                last_opened_at: engagement.map(|row| row.2.clone()),
                ocr_failed: !ocr_revisions.contains(revision_id)
                    && matches!(ocr_job_state, Some("failed") | Some("interrupted_unknown")),
            },
        );
    }
    Ok((extras, stats))
}

fn document_card_from_extras(
    root: &Path,
    paper: paper_module::PaperProjection,
    extras: Option<PaperCardExtras>,
) -> DocumentCard {
    let PaperCardExtras {
        brief_status,
        brief_takeaway,
        keywords,
        has_ocr,
        chapter_number,
        lifecycle_status,
        favorite,
        priority,
        read_later,
        review_at,
        lifecycle_version,
        furthest_page,
        last_opened_at,
        ocr_failed,
    } = extras.unwrap_or_else(PaperCardExtras::unavailable);
    DocumentCard {
        id: paper.id,
        revision_id: paper.revision_id,
        title: paper.title,
        authors: paper.authors.join(", "),
        year: paper
            .display_metadata
            .as_ref()
            .map(|v| v["year"].as_i64())
            .unwrap_or(paper.publication_year)
            .map(|year| year.to_string())
            .unwrap_or_default(),
        year_label: paper
            .display_metadata
            .as_ref()
            .and_then(|v| v["yearLabel"].as_str())
            .map(str::to_string),
        pages: paper.page_count.unwrap_or(0),
        collection: if paper.collection_path.is_empty() {
            "Papers".to_string()
        } else {
            paper.collection_path
        },
        kind: crate::library_paths::document_kind_from_relative(&paper.relative_path)
            .unwrap_or(crate::library_paths::DocumentKind::Paper)
            .as_str()
            .to_string(),
        sha256: paper.sha256,
        pdf_path: crate::library_paths::join_workspace_relative(root, &paper.relative_path)
            .unwrap_or_else(|_| root.join(&paper.relative_path))
            .to_string_lossy()
            .to_string(),
        source_status: "ready".to_string(),
        brief_status,
        has_ocr,
        brief_takeaway,
        keywords,
        chapter_number,
        imported_at: paper.imported_at,
        file_name: paper.file_name,
        lifecycle_status,
        favorite,
        priority,
        read_later,
        review_at,
        lifecycle_version,
        furthest_page,
        last_opened_at,
        ocr_failed,
    }
}

/// 单卡路径：打开一次库、走同一个聚合读取器。
pub(crate) fn document_card_from_paper(
    root: &Path,
    paper: paper_module::PaperProjection,
) -> DocumentCard {
    let key = (paper.id.clone(), paper.revision_id.clone());
    let extras = open_db(root).ok().and_then(|connection| {
        load_paper_card_extras(&connection, std::slice::from_ref(&key))
            .ok()
            .and_then(|(mut loaded, _)| loaded.remove(&key.0))
    });
    document_card_from_extras(root, paper, extras)
}

/// 批量路径：整批 Paper 只打开一次库，派生字段按 id 分块读取。
pub(crate) fn document_cards_from_papers(
    root: &Path,
    papers: Vec<paper_module::PaperProjection>,
) -> AppResult<Vec<DocumentCard>> {
    if papers.is_empty() {
        return Ok(Vec::new());
    }
    let connection = open_db(root)?;
    let keys: Vec<(String, String)> = papers
        .iter()
        .map(|paper| (paper.id.clone(), paper.revision_id.clone()))
        .collect();
    let (mut extras, _stats) = load_paper_card_extras(&connection, &keys)?;
    Ok(papers
        .into_iter()
        .map(|paper| {
            let loaded = extras.remove(&paper.id);
            document_card_from_extras(root, paper, loaded)
        })
        .collect())
}
pub(crate) fn db_path(root: &Path) -> PathBuf {
    root.join(".read-desktop").join("workspace.sqlite3")
}

pub(crate) fn open_db(root: &Path) -> AppResult<Connection> {
    db::open(db_path(root)).map_err(|e| e.to_string())
}

pub(crate) fn clear_job_cancellations(state: &AppState) -> AppResult<()> {
    for cancellations in [&state.ocr_cancellations, &state.artifact_cancellations] {
        cancellations
            .lock()
            .map_err(|_| "job cancellation lock poisoned".to_string())?
            .clear();
    }
    Ok(())
}

pub(crate) fn cleanup_workspace_staging(root: &Path, jobs: &JobModule) -> AppResult<()> {
    let staging = root.join(".read-desktop").join("staging");
    if !staging.is_dir() {
        return Ok(());
    }
    let listed = jobs.list().unwrap_or_default();
    let retain_ocr = listed
        .iter()
        .filter(|job| {
            job.kind == "ocr"
                && matches!(
                    job.state,
                    JobState::Queued | JobState::Running | JobState::Paused | JobState::Failed
                )
        })
        .map(|job| job.id.clone())
        .collect::<HashSet<_>>();
    let ocr_dir = staging.join("ocr");
    if ocr_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&ocr_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if name.ends_with(".tmp") {
                    let _ = fs::remove_file(&path);
                    continue;
                }
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    if !retain_ocr.contains(stem) {
                        let _ = fs::remove_file(&path);
                    }
                }
            }
        }
    }
    Ok(())
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn revision_record(conn: &Connection, revision_id: &str) -> AppResult<RevisionRecord> {
    conn.query_row(
        "SELECT r.id, COALESCE(m.title, p.file_name), r.sha256, p.relative_path, r.page_count
         FROM document_revisions r
         JOIN papers p ON p.id = r.paper_id
         LEFT JOIN paper_metadata m ON m.revision_id = r.id
         WHERE r.id = ?1",
        params![revision_id],
        |row| {
            Ok(RevisionRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                sha256: row.get(2)?,
                pdf_path: PathBuf::from(row.get::<_, String>(3)?),
                page_count: row.get(4)?,
            })
        },
    )
    .map_err(|e| e.to_string())
}

#[cfg(windows)]
extern "system" {
    fn DwmSetWindowAttribute(
        hwnd: isize,
        dw_attribute: u32,
        pv_attribute: *const std::ffi::c_void,
        cb_attribute: u32,
    ) -> i32;
}

#[cfg(windows)]
const DWMWA_USE_IMMERSIVE_DARK_MODE: u32 = 20;
#[cfg(windows)]
const DWMWA_USE_IMMERSIVE_DARK_MODE_FALLBACK: u32 = 19;
#[cfg(windows)]
const DWMWA_CAPTION_COLOR: u32 = 35;
#[cfg(windows)]
const DWMWA_TEXT_COLOR: u32 = 36;

pub fn sync_window_theme(window: &tauri::WebviewWindow, theme: &str) {
    let tauri_theme = match theme {
        "liquid-dark" | "dark" => tauri::Theme::Dark,
        _ => tauri::Theme::Light,
    };
    let _ = window.set_theme(Some(tauri_theme));

    #[cfg(windows)]
    {
        if let Ok(hwnd) = window.hwnd() {
            let hwnd_val = hwnd.0 as isize;
            unsafe {
                let is_dark: i32 = match theme {
                    "liquid-dark" | "dark" => 1,
                    _ => 0,
                };
                let _ = DwmSetWindowAttribute(
                    hwnd_val,
                    DWMWA_USE_IMMERSIVE_DARK_MODE,
                    &is_dark as *const i32 as *const std::ffi::c_void,
                    std::mem::size_of::<i32>() as u32,
                );
                let _ = DwmSetWindowAttribute(
                    hwnd_val,
                    DWMWA_USE_IMMERSIVE_DARK_MODE_FALLBACK,
                    &is_dark as *const i32 as *const std::ffi::c_void,
                    std::mem::size_of::<i32>() as u32,
                );

                // COLORREF format is 0x00BBGGRR
                let (caption_color, text_color): (u32, u32) = match theme {
                    "liquid-dark" | "dark" => (0x00160D09, 0x00FCFAF8), // #090d16, #f8fafc
                    "warm-editorial" => (0x00DFE9EE, 0x00382315),       // #eee9df, #152338
                    _ => (0x00EBE6E2, 0x002A170F),                      // #e2e6eb, #0f172a
                };
                let _ = DwmSetWindowAttribute(
                    hwnd_val,
                    DWMWA_CAPTION_COLOR,
                    &caption_color as *const u32 as *const std::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                );
                let _ = DwmSetWindowAttribute(
                    hwnd_val,
                    DWMWA_TEXT_COLOR,
                    &text_color as *const u32 as *const std::ffi::c_void,
                    std::mem::size_of::<u32>() as u32,
                );
            }
        }
    }
}

#[tauri::command]
fn set_window_theme(window: tauri::WebviewWindow, theme: String) -> AppResult<()> {
    sync_window_theme(&window, &theme);
    Ok(())
}

fn absolute_pdf(root: &Path, relative: &Path) -> PathBuf {
    if relative.is_absolute() {
        relative.to_path_buf()
    } else {
        crate::library_paths::join_workspace_relative(root, &relative.to_string_lossy())
            .unwrap_or_else(|_| root.join(relative))
    }
}

#[tauri::command]
fn get_workspace(state: State<'_, AppState>) -> AppResult<Option<WorkspaceInfo>> {
    workspace_lifecycle::get_workspace_impl(&state)
}

#[tauri::command]
fn choose_workspace(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    root_path: String,
) -> AppResult<WorkspaceInfo> {
    workspace_lifecycle::choose_workspace_impl(&app, &state, root_path)
}

#[tauri::command]
fn inspect_legacy_reset(
    state: State<'_, AppState>,
    root_path: String,
) -> AppResult<crate::v2_workspace::LegacyResetPreview> {
    state
        .workspace_module
        .inspect_legacy_reset(Path::new(&root_path))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExecuteLegacyResetRequest {
    root_path: String,
    preview_digest: String,
}

#[tauri::command]
fn execute_legacy_reset(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: ExecuteLegacyResetRequest,
) -> AppResult<workspace_lifecycle::ExecuteLegacyResetResponse> {
    workspace_lifecycle::execute_legacy_reset_impl(
        &app,
        &state,
        request.root_path,
        request.preview_digest,
    )
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RetryJobRequest {
    job_id: String,
    #[serde(default)]
    confirmed_potential_charge: bool,
}

#[tauri::command]
fn retry_job(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: RetryJobRequest,
) -> AppResult<crate::job_module::JobProjection> {
    let job_id = request.job_id;
    let runtime = workspace_lifecycle::current_runtime(&state)?;
    let job = runtime
        .job_module
        .get_record(&job_id)
        .map_err(|e| e.to_string())?;
    let (retry_disposition, retry_reason) = job.retry_disposition();
    match retry_disposition {
        crate::job_module::RetryDisposition::Safe => {}
        crate::job_module::RetryDisposition::ConfirmPossibleCharge
            if request.confirmed_potential_charge => {}
        crate::job_module::RetryDisposition::ConfirmPossibleCharge => {
            return Err(format!(
                "需确认可能重复请求或计费后才能重试：{retry_reason}"
            ));
        }
        crate::job_module::RetryDisposition::Unavailable => {
            return Err(format!("该任务不可安全重试：{retry_reason}"));
        }
    }
    let mut payload = job.payload.clone();
    if !payload.is_object() {
        payload = serde_json::json!({ "originalPayload": payload });
    }
    if let serde_json::Value::Object(ref mut map) = payload {
        map.insert(
            "retryOfJobId".to_string(),
            serde_json::Value::String(job_id.clone()),
        );
        map.insert(
            "confirmedPotentialCharge".to_string(),
            serde_json::Value::Bool(request.confirmed_potential_charge),
        );
    }
    let spec = crate::job_module::JobSpec {
        kind: job.kind.clone(),
        provider: job.provider.clone(),
        paper_id: job.paper_id.clone(),
        revision_id: job.revision_id.clone(),
        root_key: job.root_key.clone(),
        artifact_key: job.artifact_key.clone(),
        dedupe_key: format!("retry:{job_id}"),
        priority: job.priority,
        payload,
    };
    let route: crate::job_module::JobExecutionRoute = match job.route {
        crate::job_module::StoredJobRoute::Executable(r) => r,
        crate::job_module::StoredJobRoute::LegacyUnattributed
        | crate::job_module::StoredJobRoute::Unavailable { .. } => {
            return Err("该任务没有可验证的原始 Provider 路由".to_string());
        }
    };
    if matches!(route, crate::job_module::JobExecutionRoute::Local) {
        return Err("该任务没有已注册的本地 worker，不能重试".to_string());
    }
    let enqueue = runtime.job_module.enqueue_record(spec, route)?;
    let new_job = enqueue.job.project();
    emit_job_event_for_runtime(&app, &runtime, &new_job.id);
    spawn_job_workers(app, runtime);
    Ok(new_job)
}

fn revision_document_kind_from_root(root: &Path, revision_id: &str) -> DocumentKind {
    let Ok(connection) = open_db(root) else {
        return DocumentKind::Paper;
    };
    connection
        .query_row(
            "SELECT p.relative_path
             FROM papers p
             JOIN document_revisions r ON r.paper_id = p.id
             WHERE r.id = ?1",
            params![revision_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .and_then(|path| library_paths::document_kind_from_relative(&path))
        .unwrap_or(DocumentKind::Paper)
}

fn resolve_ui_locale(app: &tauri::AppHandle) -> AppResult<Option<ui_locale::UiLocale>> {
    ui_locale::load_ui_locale(&app_config_dir(app)?)
}

fn resolve_output_language(app: &tauri::AppHandle, requested: Option<&str>) -> AppResult<String> {
    match resolve_ui_locale(app)? {
        Some(loc) => Ok(ui_locale::output_language(loc).to_string()),
        None => requested
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| ui_locale::message(None, "请先选择语言", "Choose a language first")),
    }
}

fn require_ui_locale(app: &tauri::AppHandle) -> AppResult<ui_locale::UiLocale> {
    resolve_ui_locale(app)?
        .ok_or_else(|| ui_locale::message(None, "请先选择语言", "Choose a language first"))
}

fn load_prompt_text(
    app: &tauri::AppHandle,
    slot: PromptSlotId,
    kind: DocumentKind,
    output_language: Option<&str>,
) -> AppResult<String> {
    let locale = match output_language {
        Some("en") => ui_locale::UiLocale::En,
        Some("zh-CN") => ui_locale::UiLocale::ZhCn,
        _ => resolve_ui_locale(app)?.unwrap_or_default(),
    };
    let store = load_store_in(&prompt_settings_file(app)?, locale)?;
    let raw = resolved_text(&store, slot, kind);
    validate_slot_text(slot, &raw)?;
    let language = output_language.or(Some(ui_locale::output_language(locale)));
    let filled = apply_placeholders(&raw, language);
    let is_default = store
        .slots
        .get(slot.as_str())
        .map(|pair| match kind {
            DocumentKind::Paper => pair.paper.is_default,
            DocumentKind::Textbook => pair.textbook.is_default,
        })
        .unwrap_or(true);
    Ok(apply_language_lock(&filled, locale, is_default))
}

fn load_prompt_for_revision_from_root(
    app: &tauri::AppHandle,
    root: &Path,
    revision_id: &str,
    slot: PromptSlotId,
    output_language: Option<&str>,
) -> AppResult<String> {
    load_prompt_text(
        app,
        slot,
        revision_document_kind_from_root(root, revision_id),
        output_language,
    )
}

fn revision_relative_path_from_root(root: &Path, revision_id: &str) -> Option<String> {
    let Ok(connection) = open_db(root) else {
        return None;
    };
    connection
        .query_row(
            "SELECT p.relative_path
             FROM papers p
             JOIN document_revisions r ON r.paper_id = p.id
             WHERE r.id = ?1",
            params![revision_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
}

fn live_paper_relative_path(root: &Path, paper_id: &str) -> AppResult<String> {
    open_db(root)?
        .query_row(
            "SELECT relative_path FROM papers WHERE id = ?1 AND deleted_at IS NULL",
            params![paper_id],
            |row| row.get::<_, String>(0),
        )
        .map_err(|_| "Paper is not in the live library".to_string())
}

fn resolve_reader_wrapper_for_revision(root: &Path, revision_id: &str) -> Option<String> {
    let relative = revision_relative_path_from_root(root, revision_id)?;
    reader_context::resolve_wrapper_for_paper(root, &relative)
        .ok()
        .flatten()
}

fn frozen_reader_context(payload: &Value) -> Option<String> {
    payload
        .get("readerContext")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn outline_prompts_from_payload(payload: &Value) -> Result<(String, String), String> {
    let extract = payload
        .get("prompts")
        .and_then(|value| value.get("extract"))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| "Outline job is missing frozen prompts".to_string())?;
    let compose = payload
        .get("prompts")
        .and_then(|value| value.get("compose"))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| "Outline job is missing frozen prompts".to_string())?;
    Ok((extract.to_string(), compose.to_string()))
}

fn frozen_prompt_field(payload: &Value, key: &str) -> Result<String, String> {
    payload
        .get("prompts")
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Job is missing frozen prompts".to_string())
}

fn lens_prompt_slots(block_type: &str) -> AppResult<(PromptSlotId, PromptSlotId)> {
    let normalized = block_type.to_ascii_lowercase();
    if normalized.contains("formula") || normalized.contains("equation") {
        Ok((PromptSlotId::LensFormula, PromptSlotId::LensRepairFormula))
    } else if normalized.contains("figure")
        || normalized.contains("image")
        || normalized.contains("picture")
    {
        Ok((PromptSlotId::LensFigure, PromptSlotId::LensRepairFigure))
    } else if normalized.contains("table") {
        Ok((PromptSlotId::LensTable, PromptSlotId::LensRepairTable))
    } else {
        Err("Lens is available only for Formula, Figure, and Table Blocks".to_string())
    }
}

fn prompt_settings_file(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    let directory = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("无法定位应用配置目录：{error}"))?;
    Ok(prompt_settings_path(&directory))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavePromptSlotRequest {
    #[serde(default)]
    locale: Option<ui_locale::UiLocale>,
    slot: PromptSlotId,
    text: String,
    #[serde(default)]
    kind: DocumentKind,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PromptSlotRequest {
    #[serde(default)]
    locale: Option<ui_locale::UiLocale>,
    slot: PromptSlotId,
    #[serde(default)]
    kind: DocumentKind,
}

#[tauri::command]
fn get_prompt_settings(
    app: tauri::AppHandle,
    locale: Option<ui_locale::UiLocale>,
) -> AppResult<prompt_settings::PromptSettingsProjection> {
    let locale = locale
        .or(resolve_ui_locale(&app)?)
        .unwrap_or(ui_locale::UiLocale::ZhCn);
    load_store_in(&prompt_settings_file(&app)?, locale)
}

#[tauri::command]
fn get_ui_locale(app: tauri::AppHandle) -> AppResult<ui_locale::UiLocaleProjection> {
    Ok(ui_locale::UiLocaleProjection {
        locale: ui_locale::load_ui_locale(&app_config_dir(&app)?)?,
    })
}

#[tauri::command]
fn set_ui_locale(
    app: tauri::AppHandle,
    locale: ui_locale::UiLocale,
) -> AppResult<ui_locale::UiLocaleProjection> {
    let locale = ui_locale::save_ui_locale(&app_config_dir(&app)?, locale)?;
    refresh_tray_locale(&app, locale);
    Ok(ui_locale::UiLocaleProjection {
        locale: Some(locale),
    })
}

#[tauri::command]
fn save_prompt_slot(
    app: tauri::AppHandle,
    request: SavePromptSlotRequest,
) -> AppResult<prompt_settings::PromptSettingsProjection> {
    let locale = match request.locale {
        Some(locale) => locale,
        None => require_ui_locale(&app)?,
    };
    save_slot_in(
        &prompt_settings_file(&app)?,
        request.slot,
        request.kind,
        locale,
        &request.text,
    )
}

#[tauri::command]
fn restore_prompt_previous(
    app: tauri::AppHandle,
    request: PromptSlotRequest,
) -> AppResult<prompt_settings::PromptSettingsProjection> {
    let locale = match request.locale {
        Some(locale) => locale,
        None => require_ui_locale(&app)?,
    };
    restore_previous_in(
        &prompt_settings_file(&app)?,
        request.slot,
        request.kind,
        locale,
    )
}

#[tauri::command]
fn restore_prompt_default(
    app: tauri::AppHandle,
    request: PromptSlotRequest,
) -> AppResult<prompt_settings::PromptSettingsProjection> {
    let locale = match request.locale {
        Some(locale) => locale,
        None => require_ui_locale(&app)?,
    };
    restore_default_in(
        &prompt_settings_file(&app)?,
        request.slot,
        request.kind,
        locale,
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutlinePromptBundleRequest {
    #[serde(default)]
    locale: Option<ui_locale::UiLocale>,
    kind: DocumentKind,
}

#[tauri::command]
fn restore_outline_prompt_bundle(
    app: tauri::AppHandle,
    request: OutlinePromptBundleRequest,
) -> AppResult<prompt_settings::PromptSettingsProjection> {
    let locale = match request.locale {
        Some(locale) => locale,
        None => require_ui_locale(&app)?,
    };
    prompt_settings::restore_outline_bundle_default_in(
        &prompt_settings_file(&app)?,
        request.kind,
        locale,
    )
}

fn app_config_dir(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    app.path()
        .app_config_dir()
        .map_err(|error| format!("无法定位应用配置目录：{error}"))
}

fn character_workspace(state: &AppState) -> AppResult<PathBuf> {
    Ok(current_runtime(state)?.root.clone())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveGuideCharacterRequest {
    expected_store_revision: u32,
    character: crate::guide_character_settings::GuideCharacterDraft,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GuideCharacterIdRequest {
    expected_store_revision: u32,
    character_id: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveGuideDefaultCastRequest {
    expected_store_revision: u32,
    character_ids: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveGuidePresetCastsRequest {
    expected_store_revision: u32,
    preset_casts: Vec<crate::guide_character_settings::GuidePresetCast>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreGuidePresetCastsRequest {
    expected_store_revision: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ImportGuideAvatarRequest {
    bytes_base64: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GetGuideAvatarRequest {
    asset_id: String,
}

#[tauri::command]
fn get_guide_character_settings(
    state: State<'_, AppState>,
) -> AppResult<crate::guide_character_settings::GuideCharacterSettingsProjection> {
    crate::guide_character_settings::load_store(&character_workspace(&state)?)
}

#[tauri::command]
fn save_guide_character(
    state: State<'_, AppState>,
    request: SaveGuideCharacterRequest,
) -> AppResult<crate::guide_character_settings::GuideCharacterSettingsProjection> {
    crate::guide_character_settings::save_character(
        &character_workspace(&state)?,
        request.expected_store_revision,
        request.character,
    )
}

#[tauri::command]
fn delete_guide_character(
    state: State<'_, AppState>,
    request: GuideCharacterIdRequest,
) -> AppResult<crate::guide_character_settings::GuideCharacterSettingsProjection> {
    crate::guide_character_settings::delete_character(
        &character_workspace(&state)?,
        request.expected_store_revision,
        &request.character_id,
    )
}

#[tauri::command]
fn restore_guide_character_preset(
    state: State<'_, AppState>,
    request: GuideCharacterIdRequest,
) -> AppResult<crate::guide_character_settings::GuideCharacterSettingsProjection> {
    crate::guide_character_settings::restore_character_preset(
        &character_workspace(&state)?,
        request.expected_store_revision,
        &request.character_id,
    )
}

#[tauri::command]
fn duplicate_guide_character(
    state: State<'_, AppState>,
    request: GuideCharacterIdRequest,
) -> AppResult<crate::guide_character_settings::GuideCharacterSettingsProjection> {
    crate::guide_character_settings::duplicate_character(
        &character_workspace(&state)?,
        request.expected_store_revision,
        &request.character_id,
    )
}

#[tauri::command]
fn save_guide_default_cast(
    state: State<'_, AppState>,
    request: SaveGuideDefaultCastRequest,
) -> AppResult<crate::guide_character_settings::GuideCharacterSettingsProjection> {
    crate::guide_character_settings::save_default_cast(
        &character_workspace(&state)?,
        request.expected_store_revision,
        request.character_ids,
    )
}

#[tauri::command]
fn save_guide_preset_casts(
    state: State<'_, AppState>,
    request: SaveGuidePresetCastsRequest,
) -> AppResult<crate::guide_character_settings::GuideCharacterSettingsProjection> {
    crate::guide_character_settings::save_preset_casts(
        &character_workspace(&state)?,
        request.expected_store_revision,
        request.preset_casts,
    )
}

#[tauri::command]
fn restore_guide_factory_preset_casts(
    state: State<'_, AppState>,
    request: RestoreGuidePresetCastsRequest,
) -> AppResult<crate::guide_character_settings::GuideCharacterSettingsProjection> {
    crate::guide_character_settings::restore_factory_preset_casts(
        &character_workspace(&state)?,
        request.expected_store_revision,
    )
}

#[tauri::command]
fn import_guide_character_avatar(
    state: State<'_, AppState>,
    request: ImportGuideAvatarRequest,
) -> AppResult<serde_json::Value> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(request.bytes_base64.trim())
        .map_err(|_| "头像数据无效".to_string())?;
    let asset = crate::guide_character_assets::import_workspace_avatar(
        &character_workspace(&state)?,
        &bytes,
    )?;
    Ok(json!({
        "assetId": asset.asset_id,
        "mime": asset.mime,
        "extension": asset.extension
    }))
}

#[tauri::command]
fn get_guide_character_avatar(
    state: State<'_, AppState>,
    request: GetGuideAvatarRequest,
) -> AppResult<serde_json::Value> {
    use base64::Engine;
    let root = character_workspace(&state)?;
    let (asset, bytes) =
        crate::guide_character_assets::read_workspace_avatar(&root, &request.asset_id)?;
    let mime = asset.mime;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(json!({
        "assetId": request.asset_id,
        "mime": mime,
        "dataUrl": format!("data:{mime};base64,{b64}")
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PreviewGuideCharacterRequest {
    request_id: String,
    drafts: Vec<crate::guide_character_settings::GuideCharacterDraft>,
    document_kind: Option<String>,
}

#[tauri::command]
async fn preview_guide_character(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: PreviewGuideCharacterRequest,
) -> AppResult<serde_json::Value> {
    let snapshot = crate::guide_cast::snapshot_from_drafts(&request.drafts)?;
    let current = capture_current_paper_read_route(&app, &state)?;
    let locale = resolve_ui_locale(&app)?.unwrap_or(ui_locale::UiLocale::ZhCn);
    let prompt_store = load_store_in(&prompt_settings_file(&app)?, locale)?;
    let document_kind = match request
        .document_kind
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some("textbook") => crate::library_paths::DocumentKind::Textbook,
        _ => crate::library_paths::DocumentKind::Paper,
    };
    let annotate_prompt = load_prompt_text(
        &app,
        crate::prompt_settings::PromptSlotId::GuideAnnotate,
        document_kind,
        Some(ui_locale::output_language(locale)),
    )?;
    let catalog = crate::guide_generation::fictional_preview_catalog();
    let catalog_blocks: Vec<crate::guide_validate::GuideCatalogBlock> = catalog
        .iter()
        .filter_map(|value| {
            let bbox = value.get("bbox")?.as_array()?;
            Some(crate::guide_validate::GuideCatalogBlock {
                id: value.get("blockId")?.as_str()?.to_string(),
                page_number: value.get("pageNumber")?.as_i64()?,
                block_index: value.get("blockIndex")?.as_i64()?,
                block_type: value
                    .get("blockType")
                    .and_then(Value::as_str)
                    .unwrap_or("Text")
                    .to_string(),
                excerpt: value
                    .get("text")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                bbox: [
                    bbox.first()?.as_i64()?,
                    bbox.get(1)?.as_i64()?,
                    bbox.get(2)?.as_i64()?,
                    bbox.get(3)?.as_i64()?,
                ],
            })
        })
        .collect();
    let anchors: Vec<String> = catalog_blocks
        .iter()
        .map(|block| block.id.clone())
        .collect();
    if crate::prompt_settings::guide_workflow_protocol(&prompt_store, document_kind)? != "v2" {
        return Err("当前旁批提示词属于旧协议，请先切换为人物旁批 V2 配对提示词".into());
    }
    Uuid::parse_str(&request.request_id).map_err(|_| "试写请求标识无效")?;
    let cancellation = CancellationFlag::default();
    let key = format!("guide-preview:{}", request.request_id);
    {
        let mut active = state
            .artifact_cancellations
            .lock()
            .map_err(|_| "试写锁不可用")?;
        if active.contains_key(&key) {
            return Err("该试写请求正在运行".into());
        }
        active.insert(key.clone(), cancellation.clone());
    }
    let batch = crate::guide_catalog::GuideBatchPlan {
        ordinal: 1,
        page_start: 1,
        page_end: 1,
        anchor_block_ids: anchors.clone(),
        context_block_ids: Vec::new(),
        materials: crate::guide_catalog::materials_from_catalog(&catalog_blocks, 800),
    };
    let pending = current
        .route
        .adapter()
        .interact_text(TextInteractionRequest {
            model: current.model.clone(),
            context_epoch: format!("guide-preview:{}", request.request_id),
            system_instruction: annotate_prompt,
            user_input: crate::guide_generation::annotate_user_input(
                &snapshot,
                None,
                &batch,
                "write_margin_inks",
                None,
                None,
                ui_locale::output_language(locale),
            ),
            response_schema: Some(crate::guide_protocol::inks_schema_v2(&snapshot.order)),
            kind: PaperInteractionKind::Artifact,
        });
    let response = tokio::select! {
        result = pending => result.map_err(|e| e.to_string()),
        _ = async { while cancellation.check().is_ok() { tokio::time::sleep(std::time::Duration::from_millis(100)).await; } } => Err("试写已取消；已发送的模型请求可能仍计费".to_string()),
    };
    state
        .artifact_cancellations
        .lock()
        .map_err(|_| "试写锁不可用")?
        .remove(&key);
    let response = response?;
    let mut accepted = Vec::new();
    let mut dropped = 0_i64;
    let mut warnings = Vec::new();
    apply_guide_batch_text_v2(
        &response.text,
        &catalog_blocks,
        &snapshot,
        &anchors,
        1,
        &mut accepted,
        &mut dropped,
        &mut warnings,
    );
    Ok(json!({
        "inks": accepted.iter().map(crate::guide_validate::GuideInk::to_value).collect::<Vec<_>>(),
        "dropped": dropped,
        "warnings": warnings,
        "usage": response.receipt,
        "castSnapshot": snapshot,
    }))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CancelGuideCharacterPreviewRequest {
    request_id: String,
}

#[tauri::command]
fn cancel_guide_character_preview(
    state: State<'_, AppState>,
    request: CancelGuideCharacterPreviewRequest,
) -> AppResult<()> {
    if let Some(flag) = state
        .artifact_cancellations
        .lock()
        .map_err(|_| "试写锁不可用")?
        .get(&format!("guide-preview:{}", request.request_id))
    {
        flag.cancel();
    }
    Ok(())
}

fn reader_context_paper_relative(
    root: &Path,
    request_scope: reader_context::ReaderContextScope,
    paper_id: Option<&str>,
) -> AppResult<Option<String>> {
    if !matches!(request_scope, reader_context::ReaderContextScope::Paper) {
        return Ok(None);
    }
    let paper_id = paper_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "PDF reader context requires paperId".to_string())?;
    Ok(Some(live_paper_relative_path(root, paper_id)?))
}

#[tauri::command]
fn get_reader_context(
    state: State<'_, AppState>,
    request: reader_context::GetReaderContextRequest,
) -> AppResult<reader_context::ReaderContextProjection> {
    let root = current_runtime(&state)?.root.clone();
    let paper_relative =
        reader_context_paper_relative(&root, request.scope, request.paper_id.as_deref())?;
    let path = reader_context::path_for_scope(
        &root,
        request.scope,
        request.collection_path.as_deref(),
        paper_relative.as_deref(),
    )?;
    reader_context::project_from_path(
        request.scope,
        &path,
        request.collection_path,
        request.paper_id,
    )
}

#[tauri::command]
fn save_reader_context(
    state: State<'_, AppState>,
    request: reader_context::SaveReaderContextRequest,
) -> AppResult<reader_context::ReaderContextProjection> {
    let root = current_runtime(&state)?.root.clone();
    let paper_relative =
        reader_context_paper_relative(&root, request.scope, request.paper_id.as_deref())?;
    let path = reader_context::path_for_scope(
        &root,
        request.scope,
        request.collection_path.as_deref(),
        paper_relative.as_deref(),
    )?;
    reader_context::write_or_delete(&path, &request.text)?;
    reader_context::project_from_path(
        request.scope,
        &path,
        request.collection_path,
        request.paper_id,
    )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreReaderFolderRequest {
    id: String,
}

#[tauri::command]
fn restore_reader_folder_context(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: RestoreReaderFolderRequest,
) -> AppResult<reader_context::ReaderFolderTrashMeta> {
    let root = current_runtime(&state)?.root.clone();
    let meta = reader_context::restore_folder_trash(&root, &request.id)?;
    let _ = app.emit(
        "read-event",
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "reader_context".to_string(),
            entity_id: Some(meta.original_relative_path.clone()),
            delta: None,
            status: None,
        },
    );
    Ok(meta)
}

#[tauri::command]
fn get_model_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<ModelSettingsView> {
    current_model_settings_view(&app, &state)
}

#[tauri::command]
fn add_provider(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: AddProviderRequest,
) -> AppResult<ModelSettingsView> {
    let settings = read_model_settings(&app)?;
    let (settings, _) =
        model_settings::add_provider_instance(settings, request.name, request.kind)?;
    write_model_settings(&app, &settings)?;
    project_model_settings_view(settings, &state)
}

#[tauri::command]
fn remove_provider(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: ProviderIdRequest,
) -> AppResult<ModelSettingsView> {
    let settings = read_model_settings(&app)?;
    let settings = model_settings::remove_provider_instance(settings, &request.id)?;
    write_model_settings(&app, &settings)?;
    let _ = delete_provider_credential(&request.id);
    if let Ok(mut keys) = state.provider_keys.lock() {
        keys.remove(&request.id);
    }
    if let Ok(mut pending) = state.pending_provider_tests.lock() {
        pending.remove(&request.id);
    }
    project_model_settings_view(settings, &state)
}

#[tauri::command]
fn rename_provider(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: RenameProviderRequest,
) -> AppResult<ModelSettingsView> {
    let settings = read_model_settings(&app)?;
    let settings = model_settings::rename_provider_instance(settings, &request.id, request.name)?;
    write_model_settings(&app, &settings)?;
    project_model_settings_view(settings, &state)
}

#[tauri::command]
fn duplicate_provider(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: ProviderIdRequest,
) -> AppResult<ModelSettingsView> {
    let settings = read_model_settings(&app)?;
    let (settings, _) = model_settings::duplicate_provider_instance(settings, &request.id)?;
    write_model_settings(&app, &settings)?;
    project_model_settings_view(settings, &state)
}

#[tauri::command]
fn reorder_providers(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: ReorderProvidersRequest,
) -> AppResult<ModelSettingsView> {
    let settings = read_model_settings(&app)?;
    let settings = model_settings::reorder_provider_instances(settings, &request.ids)?;
    write_model_settings(&app, &settings)?;
    project_model_settings_view(settings, &state)
}

#[tauri::command]
async fn test_provider_connection(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: TestProviderRequest,
) -> AppResult<ConnectionTestResult> {
    let settings = read_model_settings(&app)?;
    let inst = settings.instance(&request.id)?;
    let kind = request.kind.unwrap_or_else(|| inst.kind.clone());
    let label = kind.label();

    let draft = request
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    let using_stored_credential = draft.is_none();
    let resolved_key = match draft {
        Some(v) => v,
        None => resolve_provider_key(&request.id, &state)?
            .ok_or_else(|| format!("请先输入 {label} API key"))?,
    };

    let tested_at = now();

    match kind {
        model_settings::ProviderKind::Gemini => {
            let models = fetch_gemini_models(&resolved_key).await?;
            state
                .pending_provider_tests
                .lock()
                .map_err(|_| "pending provider test lock poisoned".to_string())?
                .insert(
                    request.id.clone(),
                    model_settings::PendingConnectionTest {
                        instance_id: request.id,
                        api_key_hash: sha256_bytes(resolved_key.as_bytes()),
                        base_url: None,
                        paper_model: String::new(),
                        models: models.clone(),
                        paper_probe_passed: true,
                        paper_probe_error: None,
                        tested_at: tested_at.clone(),
                    },
                );
            Ok(ConnectionTestResult {
                models,
                tested_at,
                using_stored_credential,
                paper_probe_passed: None,
                paper_probe_error: None,
                models_fetch_error: None,
            })
        }
        model_settings::ProviderKind::OpenaiCompatible
        | model_settings::ProviderKind::Grok
        | model_settings::ProviderKind::GeminiProxy => {
            let base_url = match kind {
                model_settings::ProviderKind::Grok => model_settings::GROK_API_BASE.to_string(),
                model_settings::ProviderKind::GeminiProxy => {
                    let req_url = request.base_url.as_deref().unwrap_or("");
                    if !req_url.trim().is_empty() {
                        model_settings::normalize_chat_completions_base_url(req_url)
                    } else if let Some(stored) = &inst.base_url {
                        model_settings::normalize_chat_completions_base_url(stored)
                    } else {
                        model_settings::DEFAULT_GEMINI_PROXY_BASE.to_string()
                    }
                }
                _ => {
                    let req_url = request.base_url.as_deref().unwrap_or("");
                    if !req_url.trim().is_empty() {
                        model_settings::normalize_chat_completions_base_url(req_url)
                    } else if let Some(stored) = &inst.base_url {
                        model_settings::normalize_chat_completions_base_url(stored)
                    } else {
                        model_settings::DEFAULT_OPENAI_BASE.to_string()
                    }
                }
            };
            let adapter = chat_completions::ChatCompletionsAdapter::new(
                kind.as_str(),
                &resolved_key,
                &base_url,
            )
            .map_err(|e| e.message)?;
            let (models, models_fetch_error) = match adapter.list_models().await {
                Ok(models) => (models, None),
                Err(error) => (Vec::new(), Some(error)),
            };

            let default_paper_model = match kind {
                model_settings::ProviderKind::GeminiProxy => {
                    model_settings::DEFAULT_GEMINI_PROXY_PAPER_MODEL
                }
                _ => "",
            };
            let mut candidate_paper_model = request
                .paper_model
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .unwrap_or(inst.paper_model.as_str());
            if candidate_paper_model.trim().is_empty() {
                candidate_paper_model = default_paper_model;
            }
            if candidate_paper_model.trim().is_empty() && !models.is_empty() {
                candidate_paper_model = &models[0].id;
            }
            let paper_model = if !candidate_paper_model.trim().is_empty() {
                model_settings::normalize_compatible_model_id(candidate_paper_model)?
            } else {
                return Err("请先输入或选择主阅读模型 (Paper Model)".to_string());
            };

            let probe = adapter.run_paper_probe(&paper_model).await?;
            let models =
                chat_completions::apply_paper_probe_to_models(models, &paper_model, probe.passed);
            state
                .pending_provider_tests
                .lock()
                .map_err(|_| "pending provider test lock poisoned".to_string())?
                .insert(
                    request.id.clone(),
                    model_settings::PendingConnectionTest {
                        instance_id: request.id,
                        api_key_hash: sha256_bytes(resolved_key.as_bytes()),
                        base_url: Some(base_url),
                        paper_model,
                        models: models.clone(),
                        paper_probe_passed: probe.passed,
                        paper_probe_error: probe.failure.clone(),
                        tested_at: tested_at.clone(),
                    },
                );
            Ok(ConnectionTestResult {
                models,
                tested_at,
                using_stored_credential,
                paper_probe_passed: Some(probe.passed),
                paper_probe_error: probe.failure,
                models_fetch_error,
            })
        }
    }
}

#[tauri::command]
fn save_provider_settings(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: model_settings::SaveProviderRequest,
) -> AppResult<ModelSettingsView> {
    let existing_key = resolve_provider_key(&request.id, &state)?;
    let previous_key = existing_key.clone();
    let pending = state
        .pending_provider_tests
        .lock()
        .map_err(|_| "pending provider test lock poisoned".to_string())?
        .get(&request.id)
        .cloned();
    let outcome = model_settings::apply_save_provider(
        read_model_settings(&app)?,
        &request,
        existing_key,
        pending.as_ref(),
    )?;
    if outcome.wrote_new_key {
        write_provider_credential(&request.id, &outcome.resolved_key)?;
    }
    if let Err(save_error) = write_model_settings(&app, &outcome.settings) {
        if outcome.wrote_new_key {
            let rollback = match previous_key {
                Some(secret) => write_provider_credential(&request.id, &secret),
                None => delete_provider_credential(&request.id),
            };
            if let Err(rollback_error) = rollback {
                return Err(format!(
                    "{save_error}；同时无法恢复原凭据：{rollback_error}"
                ));
            }
        }
        return Err(save_error);
    }
    state
        .provider_keys
        .lock()
        .map_err(|_| "provider keys lock poisoned".to_string())?
        .insert(request.id.clone(), outcome.resolved_key);
    if pending.is_some() {
        state
            .pending_provider_tests
            .lock()
            .map_err(|_| "pending provider test lock poisoned".to_string())?
            .remove(&request.id);
    }
    project_model_settings_view(outcome.settings, &state)
}

#[tauri::command]
fn clear_provider_credential(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: ProviderIdRequest,
) -> AppResult<ModelSettingsView> {
    let settings =
        model_settings::apply_clear_provider_credential(read_model_settings(&app)?, &request.id)?;
    write_model_settings(&app, &settings)?;
    delete_provider_credential(&request.id)?;
    state
        .provider_keys
        .lock()
        .map_err(|_| "provider keys lock poisoned".to_string())?
        .remove(&request.id);
    state
        .pending_provider_tests
        .lock()
        .map_err(|_| "pending provider test lock poisoned".to_string())?
        .remove(&request.id);
    project_model_settings_view(settings, &state)
}

#[tauri::command]
fn set_current_provider(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: ProviderIdRequest,
) -> AppResult<ModelSettingsView> {
    let api_key = resolve_provider_key(&request.id, &state)?;
    let settings = model_settings::apply_set_current_provider(
        read_model_settings(&app)?,
        &request.id,
        api_key.as_deref(),
    )?;
    write_model_settings(&app, &settings)?;
    project_model_settings_view(settings, &state)
}

#[tauri::command]
async fn save_mistral_credential(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    input: SaveMistralCredentialInput,
) -> AppResult<ModelSettingsView> {
    let api_key = input.api_key.trim().to_string();
    if api_key.is_empty() {
        return Err("Enter a Mistral API key".to_string());
    }
    validate_mistral_key(&api_key).await?;
    write_mistral_credential(&api_key)?;
    *state
        .mistral_api_key
        .lock()
        .map_err(|_| "Mistral API key lock poisoned".to_string())? = Some(api_key);
    current_model_settings_view(&app, &state)
}

#[tauri::command]
fn clear_mistral_credential(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<ModelSettingsView> {
    delete_mistral_credential()?;
    *state
        .mistral_api_key
        .lock()
        .map_err(|_| "Mistral API key lock poisoned".to_string())? = None;
    current_model_settings_view(&app, &state)
}

fn require_current_paper_provider(
    app: &tauri::AppHandle,
    state: &AppState,
) -> AppResult<ReadyPaperProvider> {
    let lookup = |id: &str| -> Option<String> { resolve_provider_key(id, state).ok().flatten() };
    model_settings::ready_paper_provider(&read_model_settings(app)?, &lookup)
}

#[tauri::command]
fn open_workspace_dir(state: State<'_, AppState>) -> AppResult<bool> {
    let root = active_root(&state)?;
    if !root.is_dir() {
        return Err("Workspace 目录当前不可访问".to_string());
    }
    std::process::Command::new("explorer")
        .arg(root)
        .spawn()
        .map_err(|error| format!("无法打开 Workspace：{error}"))?;
    Ok(true)
}

#[tauri::command]
fn open_api_key_page(request: api_key_links::ApiKeyPageRequest) -> AppResult<()> {
    api_key_links::open(request)
}

#[tauri::command]
fn open_resource_dir(state: State<'_, AppState>, path: String) -> AppResult<bool> {
    crate::library_commands::open_resource_dir_impl(&state, path)
}
#[tauri::command]
fn open_library(state: State<'_, AppState>) -> AppResult<LibraryProjection> {
    active_paper_module(&state)?.list_library()
}

#[tauri::command]
fn list_documents(state: State<'_, AppState>) -> AppResult<Vec<DocumentCard>> {
    library_commands::list_documents_impl(&state)
}

/// D-063 §4.2：Hub 的读取入口。请求是判别联合，不存在 `{ command, payload }` 形状。
#[tauri::command]
fn library_read(
    state: State<'_, AppState>,
    request: library_query::LibraryReadRequest,
) -> Result<library_query::LibraryReadResult, library_query::LibraryQueryError> {
    let root = active_root(&state).map_err(library_query::LibraryQueryError::unavailable)?;
    library_query::read(&root, &request)
}

/// D-063 §10.2：批量动作的唯一入口。计划、执行、重试、撤销与取消都从这里进，
/// 返回值与错误都是 typed projection，调用方拿不到原始 SQLite 文本。
#[tauri::command]
fn library_act(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    mut request: library_batch::LibraryActRequest,
) -> Result<library_batch::LibraryActResponse, library_batch::LibraryActError> {
    let root = active_root(&state).map_err(library_batch::LibraryActError::unavailable)?;
    if let library_batch::LibraryActRequest::PlanBatch {
        command: library_batch::BatchCommand::Export { locale, .. },
        ..
    } = &mut request
    {
        *locale = resolve_ui_locale(&app)
            .map_err(library_batch::LibraryActError::unavailable)?
            .unwrap_or_default();
    }
    let provider = capture_provider_act_context(&app, &state);
    let response = library_batch::act_with_provider(&root, &request, &provider)?;
    emit_batch_event(&app, &response);
    if provider_batch_needs_workers(&response) {
        if let Ok(runtime) = current_runtime(&state) {
            spawn_job_workers(app, runtime);
        }
    }
    Ok(response)
}

fn capture_provider_act_context(
    app: &tauri::AppHandle,
    state: &AppState,
) -> library_batch::ProviderActContext {
    let ocr = resolve_mistral_key(state)
        .ok()
        .flatten()
        .and_then(|key| capture_mistral_ocr_route(&key, MISTRAL_OCR_MODEL).ok());
    let captured = (|| {
        let current = require_current_paper_provider(app, state).ok()?;
        let model = normalize_paper_model_id(&current.provider, &current.paper_model).ok()?;
        let translation =
            normalize_paper_model_id(&current.provider, &current.translation_model).ok()?;
        let route = capture_current_paper_job_route(
            app,
            state,
            &current,
            &model,
            Some(&translation),
            ModelRole::Paper,
        )
        .ok()?;
        let locale = resolve_ui_locale(app)
            .ok()?
            .unwrap_or(ui_locale::UiLocale::ZhCn);
        let store = load_store_in(&prompt_settings_file(app).ok()?, locale).ok()?;
        let prompt = |slot, kind| load_prompt_text(app, slot, kind, None).ok();
        Some(library_batch::ProviderActContext {
            ocr: ocr.clone(),
            paper: Some(route.freeze()),
            orientation_prompt: prompt(PromptSlotId::OrientationPack, DocumentKind::Paper),
            textbook_orientation_prompt: prompt(
                PromptSlotId::OrientationPack,
                DocumentKind::Textbook,
            ),
            paper_root_prompt: prompt(PromptSlotId::PaperRoot, DocumentKind::Paper),
            textbook_root_prompt: prompt(PromptSlotId::PaperRoot, DocumentKind::Textbook),
            textbook_brief_protocol: Some(prompt_settings::resolved_protocol(
                &store,
                PromptSlotId::OrientationPack,
                DocumentKind::Textbook,
            )),
        })
    })();
    captured.unwrap_or(library_batch::ProviderActContext {
        ocr,
        ..Default::default()
    })
}

fn provider_batch_needs_workers(response: &library_batch::LibraryActResponse) -> bool {
    let Some(batch) = response.batch_id().and_then(|_| match response {
        library_batch::LibraryActResponse::StartBatch { batch, .. }
        | library_batch::LibraryActResponse::ControlBatch { batch, .. } => Some(batch),
        _ => None,
    }) else {
        return false;
    };
    matches!(
        batch.command_kind,
        library_batch::BatchCommandKind::Ocr | library_batch::BatchCommandKind::Brief
    ) || matches!(
        batch.parent_command_kind,
        Some(library_batch::BatchCommandKind::Ocr | library_batch::BatchCommandKind::Brief)
    )
}

/// §10.3：事件只带批次身份与聚合态，不带成员或路径。
/// `plan` 只写了批次自己的行，Hub 可见状态没动，所以不发事件。
fn emit_batch_event(app: &tauri::AppHandle, response: &library_batch::LibraryActResponse) {
    let Some(kind) = response.notified_kind() else {
        return;
    };
    let entity_id = response.batch_id().map(str::to_string);
    let _ = app.emit(
        "read-event",
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: kind.to_string(),
            entity_id,
            delta: None,
            status: response.notified_state(),
        },
    );
}

#[tauri::command]
fn import_pdf(
    state: State<'_, AppState>,
    path: String,
    collection: Option<String>,
) -> AppResult<DocumentCard> {
    let root = active_root(&state)?;
    let imported =
        active_paper_module(&state)?.import_pdf(Path::new(&path), collection.as_deref())?;
    match (imported.paper, imported.conflict) {
        (Some(paper), _) => Ok(document_card_from_paper(&root, paper)),
        (_, Some(conflict)) => Err(format!(
            "library_conflict:{}",
            serde_json::to_string(&conflict).map_err(|error| error.to_string())?
        )),
        _ => Err("Import did not publish a Paper or a conflict".to_string()),
    }
}

#[tauri::command]
fn move_paper(
    state: State<'_, AppState>,
    paper_id: String,
    collection_path: String,
    file_name: String,
    confirm_kind_change: bool,
) -> AppResult<DocumentCard> {
    let root = active_root(&state)?;
    let paper = active_paper_module(&state)?.move_paper(
        &paper_id,
        &collection_path,
        &file_name,
        confirm_kind_change,
    )?;
    Ok(document_card_from_paper(&root, paper))
}

#[allow(non_snake_case)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateCollectionRequest {
    parentPath: String,
    name: Option<String>,
}
#[allow(non_snake_case)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenameCollectionRequest {
    relativePath: String,
    newName: String,
}
#[allow(non_snake_case)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MoveCollectionRequest {
    relativePath: String,
    destParentPath: String,
}
#[allow(non_snake_case)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TrashCollectionRequest {
    relativePath: String,
}
#[allow(non_snake_case)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RenamePaperRequest {
    paperId: String,
    newFileName: String,
}
#[allow(non_snake_case)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReorderCollectionPapersRequest {
    collectionId: String,
    paperIds: Vec<String>,
}
#[allow(non_snake_case)]
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SetCollectionSortModeRequest {
    collectionId: String,
    sortMode: String,
}

#[tauri::command]
fn list_collections(
    state: State<'_, AppState>,
) -> AppResult<Vec<paper_module::CollectionProjection>> {
    crate::library_commands::list_collections_impl(&state)
}
#[tauri::command]
fn create_collection(
    state: State<'_, AppState>,
    request: CreateCollectionRequest,
) -> AppResult<paper_module::CollectionProjection> {
    crate::library_commands::create_collection_impl(&state, request.parentPath, request.name)
}
#[tauri::command]
fn rename_collection(
    state: State<'_, AppState>,
    request: RenameCollectionRequest,
) -> AppResult<paper_module::CollectionProjection> {
    crate::library_commands::rename_collection_impl(&state, request.relativePath, request.newName)
}
#[tauri::command]
fn move_collection(
    state: State<'_, AppState>,
    request: MoveCollectionRequest,
) -> AppResult<paper_module::CollectionProjection> {
    crate::library_commands::move_collection_impl(
        &state,
        request.relativePath,
        request.destParentPath,
    )
}
#[tauri::command]
fn trash_collection(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: TrashCollectionRequest,
) -> AppResult<serde_json::Value> {
    crate::library_commands::trash_collection_impl(&app, &state, request.relativePath)
}
#[tauri::command]
fn reorder_collection_papers(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: ReorderCollectionPapersRequest,
) -> AppResult<()> {
    crate::library_commands::reorder_collection_papers_impl(
        &app,
        &state,
        request.collectionId,
        request.paperIds,
    )
}
#[tauri::command]
fn set_collection_sort_mode(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: SetCollectionSortModeRequest,
) -> AppResult<()> {
    crate::library_commands::set_collection_sort_mode_impl(
        &app,
        &state,
        request.collectionId,
        request.sortMode,
    )
}
#[tauri::command]
fn rename_paper(
    state: State<'_, AppState>,
    request: RenamePaperRequest,
) -> AppResult<DocumentCard> {
    crate::library_commands::rename_paper_impl(&state, request.paperId, request.newFileName)
}

#[tauri::command]
fn export_reading_bundle(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    revision_id: String,
) -> AppResult<String> {
    let root = active_root(&state)?;
    let module = active_paper_module(&state)?;
    // 旧的单篇合同按 revision_id 认 Paper，批量侧按 paper_id。渲染与命名规则都在
    // `export_module` 一份实现，这里只做一次身份换算——两处各写一遍就会导出两个东西。
    let paper = module
        .list_library()?
        .papers
        .into_iter()
        .find(|candidate| candidate.revision_id == revision_id)
        .ok_or_else(|| "The active Document Revision was not found".to_string())?;
    let bundle = crate::export_module::export_bundle(
        &module,
        &root,
        &paper.id,
        resolve_ui_locale(&app)?.unwrap_or_default(),
    )
    .map_err(|error| match error {
        crate::export_module::ExportError::PaperMissing => {
            "The active Document Revision was not found".to_string()
        }
        crate::export_module::ExportError::WriteFailed(text)
        | crate::export_module::ExportError::Storage(text) => text,
    })?;
    Ok(root
        .join(
            bundle
                .relative_path
                .replace('/', std::path::MAIN_SEPARATOR_STR),
        )
        .to_string_lossy()
        .to_string())
}
#[tauri::command]
fn reconcile_library(state: State<'_, AppState>) -> AppResult<LibraryProjection> {
    active_paper_module(&state)?.reconcile()
}

#[tauri::command]
fn delete_revision(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    revision_id: String,
) -> AppResult<DeletePaperResult> {
    let runtime = current_runtime(&state)?;
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
                    job_module::JobState::Queued
                        | job_module::JobState::Running
                        | job_module::JobState::Paused
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

#[tauri::command]
fn restore_paper(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    paper_id: String,
) -> AppResult<DocumentCard> {
    let root = active_root(&state)?;
    let paper = active_paper_module(&state)?.restore_paper(&paper_id)?;
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

#[tauri::command]
fn get_reading_state(
    state: State<'_, AppState>,
    paper_id: String,
) -> AppResult<Option<ReadingState>> {
    active_paper_module(&state)?.reading_state(&paper_id)
}

#[tauri::command]
fn save_reading_state(
    state: State<'_, AppState>,
    reading_state: ReadingState,
) -> AppResult<ReadingState> {
    active_paper_module(&state)?.save_reading_state(&reading_state)
}

#[tauri::command]
fn list_annotations(
    state: State<'_, AppState>,
    paper_id: String,
) -> AppResult<Vec<UserAnnotation>> {
    active_paper_module(&state)?.list_annotations(&paper_id)
}

#[tauri::command]
fn create_annotation(
    state: State<'_, AppState>,
    request: UserAnnotationInput,
) -> AppResult<UserAnnotation> {
    active_paper_module(&state)?.create_annotation(&request)
}

#[tauri::command]
fn update_annotation(
    state: State<'_, AppState>,
    request: UserAnnotationUpdateRequest,
) -> AppResult<Option<UserAnnotation>> {
    active_paper_module(&state)?.update_annotation(&request)
}

#[tauri::command]
fn delete_annotation(state: State<'_, AppState>, annotation_id: String) -> AppResult<bool> {
    active_paper_module(&state)?.delete_annotation(&annotation_id)
}

#[tauri::command]
fn list_annotation_links(
    state: State<'_, AppState>,
    paper_id: String,
) -> AppResult<Vec<UserAnnotationLink>> {
    active_paper_module(&state)?.list_annotation_links(&paper_id)
}

#[tauri::command]
fn create_annotation_link(
    state: State<'_, AppState>,
    request: UserAnnotationLinkInput,
) -> AppResult<UserAnnotationLink> {
    active_paper_module(&state)?.create_annotation_link(&request)
}

#[tauri::command]
fn delete_annotation_link(state: State<'_, AppState>, link_id: String) -> AppResult<bool> {
    active_paper_module(&state)?.delete_annotation_link(&link_id)
}

#[tauri::command]
fn get_storage_report(state: State<'_, AppState>) -> AppResult<StorageReport> {
    active_paper_module(&state)?.storage_report()
}
#[tauri::command]
fn list_trash(state: State<'_, AppState>) -> AppResult<Vec<TrashProjection>> {
    let runtime = current_runtime(&state)?;
    let mut items = runtime.paper_module.list_trash()?;
    for meta in reader_context::list_folder_trash(&runtime.root)? {
        items.push(TrashProjection {
            id: meta.id,
            paper_id: String::new(),
            title: format!("目录读者上下文 · {}", meta.original_relative_path),
            file_name: reader_context::FOLDER_FILE_NAME.to_string(),
            original_relative_path: meta.original_relative_path,
            source_bytes: 0,
            deleted_at: meta.deleted_at,
            purge_after: meta.purge_after,
            kind: "reader_folder".to_string(),
        });
    }
    items.sort_by(|left, right| right.deleted_at.cmp(&left.deleted_at));
    Ok(items)
}

#[tauri::command]
fn list_artifacts(
    state: State<'_, AppState>,
    paper_id: String,
) -> AppResult<Vec<ArtifactProjection>> {
    active_artifact_module(&state)?.list(&paper_id)
}

#[tauri::command]
fn get_artifact(state: State<'_, AppState>, artifact_id: String) -> AppResult<ArtifactProjection> {
    active_artifact_module(&state)?.get(&artifact_id)
}

#[tauri::command]
fn delete_artifact(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    artifact_id: String,
) -> AppResult<Option<ArtifactProjection>> {
    let result = active_artifact_module(&state)?.delete(&artifact_id)?;
    let _ = app.emit(
        "read-event",
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "paper".to_string(),
            entity_id: Some(artifact_id),
            delta: None,
            status: None,
        },
    );
    Ok(result)
}

#[tauri::command]
fn get_artifact_asset_path(
    state: State<'_, AppState>,
    artifact_id: String,
) -> AppResult<Option<String>> {
    let artifact = active_artifact_module(&state)?.get(&artifact_id)?;
    let relative = artifact
        .content
        .get("displayCropRelativePath")
        .and_then(Value::as_str);
    let Some(relative) = relative else {
        return Ok(None);
    };
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
        || !relative
            .replace(char::from(92), "/")
            .starts_with(".read-desktop/artifacts/lens/")
    {
        return Err("Stored Artifact asset path is unsafe".to_string());
    }
    let root = active_root(&state)?;
    let target = root.join(relative_path);
    if !target.is_file() {
        return Ok(None);
    }
    let canonical_root = root.canonicalize().map_err(|error| error.to_string())?;
    let canonical_target = target.canonicalize().map_err(|error| error.to_string())?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err("Stored Artifact asset escaped the Workspace".to_string());
    }
    Ok(Some(canonical_target.to_string_lossy().to_string()))
}

fn outline_stage_steps(kind: &str, stage: &str) -> (i64, i64) {
    if kind == "outline_deep_dive" {
        return match stage {
            "composing" | "repairing" | "published" => (1, 1),
            _ => (0, 1),
        };
    }
    if kind == crate::guide_protocol::GUIDE_JOB_KIND {
        return match stage {
            "understanding" => (1, 3),
            "annotating" => (2, 3),
            "publishing" | "published" => (3, 3),
            _ => (0, 3),
        };
    }
    match stage {
        "extracting" | "extracted" | "drafting" | "drafted" => (1, 3),
        "composing" | "reviewing" => (2, 3),
        "repairing" | "published" => (3, 3),
        _ => (0, 3),
    }
}

fn merge_usage_into_checkpoint(checkpoint: &mut Value, receipt: &UsageEnvelope) {
    let add = |checkpoint: &mut Value, key: &str, delta: Option<i64>| {
        if let Some(delta) = delta {
            let previous = checkpoint.get(key).and_then(Value::as_i64).unwrap_or(0);
            checkpoint[key] = json!(previous.saturating_add(delta));
        }
    };
    add(checkpoint, "inputTokens", receipt.input_tokens);
    add(checkpoint, "outputTokens", receipt.output_tokens);
    add(checkpoint, "cachedInputTokens", receipt.cached_input_tokens);
}

fn record_route_interaction(
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
    context_epoch: &str,
    remote_provider_node_id: &str,
    receipt: &UsageEnvelope,
) -> Result<(), ProviderError> {
    let provider_route_id = route.frozen().route_id().database_value();
    let provider = route.frozen().provider_kind().as_str();
    let model = route.frozen().models().paper();
    let receipt_epoch = receipt.context_epoch.as_deref().unwrap_or(context_epoch);
    let operation_id = if remote_provider_node_id.trim().is_empty() {
        format!("{}:{}", job.id, Uuid::new_v4())
    } else {
        remote_provider_node_id.to_string()
    };
    let mut connection = open_db(&runtime.root).map_err(ProviderError::local_state)?;
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|error| ProviderError::local_state(error.to_string()))?;
    if !remote_provider_node_id.trim().is_empty() {
        transaction
            .execute(
                "INSERT INTO provider_nodes(
                   id, provider, model, context_epoch, provider_route_id, provider_node_id,
                   parent_id, state, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, 'complete', ?7)",
                params![
                    Uuid::new_v4().to_string(),
                    provider,
                    model,
                    receipt_epoch,
                    provider_route_id,
                    remote_provider_node_id,
                    now()
                ],
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
    }
    transaction
        .execute(
            "INSERT INTO usage_receipts(
               id, operation_id, job_id, provider, model, context_epoch, provider_route_id,
               input_tokens, cached_input_tokens, uncached_input_tokens,
               output_tokens, reasoning_tokens, latency_ms, estimated_cost, file_reuse,
               session_resume, paper_root_branch, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                Uuid::new_v4().to_string(),
                operation_id,
                job.id,
                provider,
                model,
                receipt_epoch,
                provider_route_id,
                receipt.input_tokens,
                receipt.cached_input_tokens,
                receipt.uncached_input_tokens,
                receipt.output_tokens,
                receipt.reasoning_tokens,
                receipt.latency_ms,
                receipt.estimated_cost,
                receipt.file_reuse.map(i64::from),
                receipt.session_resume.map(i64::from),
                receipt.paper_root_branch.map(i64::from),
                now()
            ],
        )
        .map_err(|error| ProviderError::local_state(error.to_string()))?;
    transaction
        .commit()
        .map_err(|error| ProviderError::local_state(error.to_string()))
}

fn report_outline_progress(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    jobs: &JobModule,
    job: &JobProjection,
    stage: &str,
    extra: Value,
    receipt: Option<&UsageEnvelope>,
) -> Result<(), ProviderError> {
    let mut checkpoint = jobs
        .get_checkpoint(&job.id)
        .map_err(ProviderError::local_state)?
        .unwrap_or_else(|| json!({}));
    let (step, steps) = outline_stage_steps(&job.kind, stage);
    checkpoint["step"] = json!(step);
    checkpoint["steps"] = json!(steps);
    if let Some(object) = extra.as_object() {
        for (key, value) in object {
            checkpoint[key] = value.clone();
        }
    }
    if let Some(receipt) = receipt {
        merge_usage_into_checkpoint(&mut checkpoint, receipt);
    }
    jobs.save_checkpoint(&job.id, stage, &checkpoint)
        .map_err(ProviderError::local_state)?;
    emit_job_event_for_runtime(app, runtime, &job.id);
    Ok(())
}

pub(crate) fn emit_job_event(app: &tauri::AppHandle, job_id: &str) {
    let _ = app.emit(
        "read-event",
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "job".to_string(),
            entity_id: Some(job_id.to_string()),
            delta: None,
            status: None,
        },
    );
}

// `publish` runs while the runtime slot is locked and must not re-enter that lock.
pub(crate) fn publish_if_current(
    state: &AppState,
    runtime: &Arc<WorkspaceRuntime>,
    publish: impl FnOnce(),
) -> bool {
    let Ok(slot) = state.runtime.lock() else {
        return false;
    };
    let Some(current) = slot.as_ref() else {
        return false;
    };
    if runtime.is_stopped() || !Arc::ptr_eq(current, runtime) {
        return false;
    }
    publish();
    true
}

pub(crate) fn emit_job_event_for_runtime(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    job_id: &str,
) {
    let state = app.state::<AppState>();
    publish_if_current(&state, runtime, || emit_job_event(app, job_id));
}

fn emit_read_event_for_runtime(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    event: ReadEvent,
) {
    let state = app.state::<AppState>();
    publish_if_current(&state, runtime, || {
        let _ = app.emit("read-event", event);
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiscussionUserInput {
    Incremental,
    Recovery,
}

fn discussion_user_input(provider: &str, has_previous: bool) -> DiscussionUserInput {
    if provider == "gemini" && has_previous {
        DiscussionUserInput::Incremental
    } else {
        DiscussionUserInput::Recovery
    }
}

fn capture_current_paper_job_route(
    app: &tauri::AppHandle,
    state: &AppState,
    current: &ReadyPaperProvider,
    paper_model: &str,
    translation_model: Option<&str>,
    operation: ModelRole,
) -> AppResult<BoundProviderRoute> {
    let settings = read_model_settings(app)?;
    let instance_id =
        ProviderInstanceId::parse(&current.instance_id).map_err(|error| error.to_string())?;
    let models = FrozenModels::new(
        paper_model.to_string(),
        translation_model.map(str::to_string),
    )
    .map_err(|error| error.to_string())?;
    match ProviderRouting::new(&settings, state)
        .capture(ProviderSelection::Instance(instance_id), models, operation)
        .map_err(|error| error.to_string())?
    {
        ProviderRouteDecision::Ready(route) => Ok(route),
        ProviderRouteDecision::ActionRequired(requirement) => Err(format!(
            "Provider action is required before this task can start ({})",
            requirement.database_code()
        )),
    }
}

struct CurrentPaperReadRoute {
    route: BoundProviderRoute,
    model: String,
    supports_native_pdf: bool,
    input_token_limit: Option<i64>,
}

fn capture_current_paper_read_route(
    app: &tauri::AppHandle,
    state: &AppState,
) -> AppResult<CurrentPaperReadRoute> {
    let current = require_current_paper_provider(app, state)?;
    let model = normalize_paper_model_id(&current.provider, &current.paper_model)?;
    let translation_model =
        normalize_paper_model_id(&current.provider, &current.translation_model)?;
    let settings = read_model_settings(app)?;
    let selected_model = settings
        .instance(&current.instance_id)
        .map_err(|error| error.to_string())?
        .models
        .iter()
        .find(|candidate| candidate.id == model)
        .ok_or_else(|| {
            "The selected paper model is no longer available on the current Provider".to_string()
        })?;
    let route = capture_current_paper_job_route(
        app,
        state,
        &current,
        &model,
        Some(&translation_model),
        ModelRole::Paper,
    )?;
    Ok(CurrentPaperReadRoute {
        route,
        model,
        supports_native_pdf: selected_model.supports_native_pdf,
        input_token_limit: selected_model.input_token_limit,
    })
}

fn route_scoped_root_key(revision_id: &str, route: &BoundProviderRoute) -> String {
    format!(
        "{revision_id}:{}",
        route.frozen().route_id().database_value()
    )
}

fn enqueue_paper_job(
    jobs: &JobModule,
    spec: JobSpec,
    route: &BoundProviderRoute,
) -> AppResult<job_module::EnqueueResult> {
    let result = jobs.enqueue_record(spec, JobExecutionRoute::Paper(route.freeze()))?;
    Ok(job_module::EnqueueResult {
        job: result.job.project(),
        coalesced: result.coalesced,
    })
}

fn normalize_paper_model_id(provider: &str, value: &str) -> AppResult<String> {
    match provider {
        "gemini" => normalize_model_id(value),
        _ => model_settings::normalize_compatible_model_id(value),
    }
}

fn looks_like_unsupported_paper_capability(error: &ProviderError) -> bool {
    if matches!(
        error.kind,
        ProviderErrorKind::StaleRemoteResource
            | ProviderErrorKind::Unauthorized
            | ProviderErrorKind::RateLimited
            | ProviderErrorKind::Cancelled
    ) {
        return false;
    }
    let message = error.message.to_ascii_lowercase();
    let is_client_error = ["(400)", "(413)", "(415)", "(422)"]
        .iter()
        .any(|code| message.contains(code));
    if !is_client_error {
        return false;
    }
    [
        "pdf",
        "image",
        "response_format",
        "json_schema",
        "schema",
        "file type",
        "unsupported",
        "invalid image",
        "does not support",
    ]
    .iter()
    .any(|hint| message.contains(hint))
}

fn invalidate_paper_probe_slot(app: &tauri::AppHandle, provider: &str) -> AppResult<()> {
    let mut settings = read_model_settings(app)?;
    if let Some(inst) = settings
        .providers
        .iter_mut()
        .find(|instance| instance.id == provider)
    {
        inst.paper_probe = None;
        write_model_settings(app, &settings)?;
    }
    Ok(())
}

fn maybe_invalidate_paper_probe(app: &tauri::AppHandle, provider: &str, error: &ProviderError) {
    if looks_like_unsupported_paper_capability(error) {
        let _ = invalidate_paper_probe_slot(app, provider);
    }
}

fn job_paper_model_adapter(
    inner: Arc<dyn PaperModelPort>,
    jobs: JobModule,
    job_id: String,
    cancellation: CancellationFlag,
) -> JobPaperModelAdapter {
    JobPaperModelAdapter {
        inner,
        jobs,
        job_id,
        cancellation,
        committed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    }
}

#[derive(Clone)]
struct JobPaperModelAdapter {
    inner: Arc<dyn PaperModelPort>,
    jobs: JobModule,
    job_id: String,
    cancellation: CancellationFlag,
    committed: Arc<std::sync::atomic::AtomicBool>,
}

impl JobPaperModelAdapter {
    fn before_provider_call(&self) -> Result<(), ProviderError> {
        self.cancellation.check()?;
        if self
            .committed
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_ok()
        {
            self.jobs
                .mark_provider_committed(&self.job_id, None)
                .map_err(ProviderError::local_state)?;
        }
        Ok(())
    }

    fn after_provider_call<T>(
        &self,
        outcome: Result<T, ProviderError>,
    ) -> Result<T, ProviderError> {
        let outcome = outcome?;
        self.cancellation.check()?;
        Ok(outcome)
    }
}

#[async_trait::async_trait]
impl PaperModelPort for JobPaperModelAdapter {
    fn capabilities(&self, model: &str) -> PaperModelCapabilities {
        self.inner.capabilities(model)
    }

    async fn interact(
        &self,
        request: PaperInteractionRequest,
    ) -> Result<PaperInteractionOutcome, ProviderError> {
        self.before_provider_call()?;
        let outcome = self.inner.interact(request).await;
        self.after_provider_call(outcome)
    }

    async fn interact_stream(
        &self,
        request: PaperStreamRequest,
    ) -> Result<PaperInteractionOutcome, ProviderError> {
        self.before_provider_call()?;
        let outcome = self.inner.interact_stream(request).await;
        self.after_provider_call(outcome)
    }

    async fn interact_text(
        &self,
        request: TextInteractionRequest,
    ) -> Result<TextInteractionOutcome, ProviderError> {
        self.before_provider_call()?;
        let outcome = self.inner.interact_text(request).await;
        self.after_provider_call(outcome)
    }

    async fn delete_remote(&self, resource: &RemoteResource) -> Result<(), ProviderError> {
        self.inner.delete_remote(resource).await
    }
}

const ARTIFACT_STAGING_LIMIT: usize = 28 * 1024 * 1024;

fn artifact_staging_dir(root: &Path, staging_key: &str) -> AppResult<PathBuf> {
    Uuid::parse_str(staging_key).map_err(|_| "Artifact staging key is invalid".to_string())?;
    Ok(root
        .join(".read-desktop")
        .join("staging")
        .join("artifacts")
        .join(staging_key))
}

fn stage_artifact_material(
    root: &Path,
    staging_key: &str,
    name: &str,
    data_url: Option<&str>,
) -> AppResult<Option<String>> {
    let Some(data_url) = data_url else {
        return Ok(None);
    };
    if !matches!(name, "display" | "model")
        || !data_url.starts_with("data:image/")
        || data_url.len() > ARTIFACT_STAGING_LIMIT
    {
        return Err("Lens crop staging material is invalid or too large".to_string());
    }
    let directory = artifact_staging_dir(root, staging_key)?;
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let target = directory.join(format!("{name}.data-url"));
    let temporary = directory.join(format!("{name}.{}.tmp", Uuid::new_v4().simple()));
    fs::write(&temporary, data_url.as_bytes()).map_err(|error| error.to_string())?;
    fs::rename(&temporary, &target).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        error.to_string()
    })?;
    let relative = target
        .strip_prefix(root)
        .map_err(|_| "Artifact staging path escaped the Workspace".to_string())?;
    Ok(Some(
        relative.to_string_lossy().replace(char::from(92), "/"),
    ))
}

fn read_artifact_material(
    root: &Path,
    relative: Option<&str>,
) -> Result<Option<String>, ProviderError> {
    let Some(relative) = relative else {
        return Ok(None);
    };
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
        || !relative
            .replace(char::from(92), "/")
            .starts_with(".read-desktop/staging/artifacts/")
    {
        return Err(ProviderError::local_state(
            "Artifact staging path is unsafe",
        ));
    }
    let target = root.join(path);
    let metadata = fs::metadata(&target).map_err(|error| {
        ProviderError::local_state(format!("Artifact staging material is unavailable: {error}"))
    })?;
    if metadata.len() > ARTIFACT_STAGING_LIMIT as u64 {
        return Err(ProviderError::local_state(
            "Artifact staging material exceeds its size limit",
        ));
    }
    fs::read_to_string(target)
        .map(Some)
        .map_err(|error| ProviderError::local_state(error.to_string()))
}

fn cleanup_artifact_staging(root: &Path, staging_key: Option<&str>) {
    let Some(staging_key) = staging_key else {
        return;
    };
    let Ok(target) = artifact_staging_dir(root, staging_key) else {
        return;
    };
    let expected_parent = root.join(".read-desktop").join("staging").join("artifacts");
    if target.parent() == Some(expected_parent.as_path()) && target.is_dir() {
        let _ = fs::remove_dir_all(target);
    }
}
fn ocr_staging_path(root: &Path, job_id: &str) -> PathBuf {
    root.join(".read-desktop")
        .join("staging")
        .join("ocr")
        .join(format!("{job_id}.json"))
}

fn reusable_ocr_staging(
    root: &Path,
    jobs: &JobModule,
    revision_id: &str,
    current_job_id: &str,
) -> Option<PathBuf> {
    let own = ocr_staging_path(root, current_job_id);
    if own.is_file() {
        return Some(own);
    }
    jobs.list().ok()?.into_iter().find_map(|job| {
        if job.kind != "ocr"
            || job.id == current_job_id
            || job.revision_id.as_deref() != Some(revision_id)
            || !matches!(job.state, JobState::Failed | JobState::InterruptedUnknown)
        {
            return None;
        }
        let path = ocr_staging_path(root, &job.id);
        path.is_file().then_some(path)
    })
}

fn record_remote_tombstone(
    root: &Path,
    resource: &RemoteResource,
    paper_id: Option<&str>,
    endpoint_scope: Option<&str>,
    error: &str,
) -> AppResult<()> {
    let (endpoint_scope, ownership_status) =
        match endpoint_scope.filter(|scope| !scope.trim().is_empty()) {
            Some(scope) => (Some(scope), "exact"),
            None => (None, "legacy_unattributed"),
        };
    open_db(root)?
        .execute(
            "INSERT INTO remote_tombstones(
               id, provider, resource_kind, remote_id, paper_id, state,
               attempts, last_error, created_at, updated_at,
               endpoint_scope, ownership_status
             ) VALUES (
               ?1, ?2, ?3, ?4, ?5, 'pending', 0, ?6, ?7, ?7, ?8, ?9
             )
             ON CONFLICT(endpoint_scope, resource_kind, remote_id)
               WHERE endpoint_scope IS NOT NULL
             DO UPDATE SET
               state = 'pending', attempts = 0, last_error = excluded.last_error,
               updated_at = excluded.updated_at, ownership_status = 'exact'",
            params![
                Uuid::new_v4().to_string(),
                resource.provider,
                resource.kind,
                resource.id,
                paper_id,
                error,
                now(),
                endpoint_scope,
                ownership_status,
            ],
        )
        .map_err(|database_error| database_error.to_string())?;
    Ok(())
}

pub(crate) fn queue_paper_remote_cleanup(root: &Path, paper_id: &str) -> AppResult<usize> {
    let connection = open_db(root)?;
    let mut resources = HashSet::new();
    {
        let mut statement = connection
            .prepare(
                "SELECT DISTINCT cr.provider, cr.provider_file_id, routes.endpoint_scope
                 FROM context_roots cr
                 JOIN document_revisions r ON r.id = cr.revision_id
                 LEFT JOIN provider_route_snapshots routes
                   ON routes.route_id = cr.provider_route_id
                 WHERE r.paper_id = ?1 AND cr.provider_file_id IS NOT NULL",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![paper_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        for row in rows {
            let (provider, id, endpoint_scope) = row.map_err(|error| error.to_string())?;
            resources.insert((provider, "file".to_string(), id, endpoint_scope));
        }
    }
    {
        let mut statement = connection
            .prepare(
                "SELECT resources.provider, resources.remote_id, routes.endpoint_scope
                 FROM (
                   SELECT cr.provider AS provider, cr.provider_node_id AS remote_id,
                          cr.provider_route_id AS provider_route_id
                   FROM context_roots cr
                   JOIN document_revisions r ON r.id = cr.revision_id
                   WHERE r.paper_id = ?1 AND cr.provider_node_id IS NOT NULL
                   UNION
                   SELECT pn.provider AS provider, pn.provider_node_id AS remote_id,
                          pn.provider_route_id AS provider_route_id
                   FROM provider_nodes pn
                   JOIN messages m ON m.provider_node_id = pn.id
                   JOIN discussions d ON d.id = m.discussion_id
                   WHERE d.paper_id = ?1 AND pn.provider_node_id IS NOT NULL
                   UNION
                   SELECT pn.provider AS provider, pn.provider_node_id AS remote_id,
                          pn.provider_route_id AS provider_route_id
                   FROM provider_nodes pn
                   JOIN artifacts a ON a.provider_node_id = pn.id
                   WHERE a.paper_id = ?1 AND pn.provider_node_id IS NOT NULL
                   UNION
                   SELECT pn.provider AS provider, pn.provider_node_id AS remote_id,
                          pn.provider_route_id AS provider_route_id
                   FROM provider_nodes pn
                   JOIN lens_qa q ON q.provider_node_id = pn.id
                   JOIN artifacts a ON a.id = q.lens_artifact_id
                   WHERE a.paper_id = ?1 AND pn.provider_node_id IS NOT NULL
                   UNION
                   SELECT pn.provider AS provider, pn.provider_node_id AS remote_id,
                          pn.provider_route_id AS provider_route_id
                   FROM provider_nodes pn
                   JOIN usage_receipts ur
                     ON ur.operation_id = pn.provider_node_id
                    AND ur.provider_route_id = pn.provider_route_id
                   JOIN jobs j ON j.id = ur.job_id
                   WHERE j.paper_id = ?1 AND pn.provider_node_id IS NOT NULL
                 ) resources
                 LEFT JOIN provider_route_snapshots routes
                   ON routes.route_id = resources.provider_route_id
                 WHERE resources.remote_id IS NOT NULL",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![paper_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        for row in rows {
            let (provider, id, endpoint_scope) = row.map_err(|error| error.to_string())?;
            resources.insert((provider, "interaction".to_string(), id, endpoint_scope));
        }
    }
    drop(connection);
    let count = resources.len();
    for (provider, kind, id, endpoint_scope) in resources {
        record_remote_tombstone(
            root,
            &RemoteResource { provider, kind, id },
            Some(paper_id),
            endpoint_scope.as_deref(),
            "Queued after Paper soft-delete",
        )?;
    }
    Ok(count)
}

pub(crate) fn list_remote_tombstones_from_root(
    root: &Path,
) -> AppResult<Vec<RemoteTombstoneProjection>> {
    let connection = open_db(root)?;
    let mut statement = connection
        .prepare(
            "SELECT id, provider, resource_kind, paper_id, state, attempts,
                    last_error, ownership_status, created_at, updated_at
             FROM remote_tombstones
             ORDER BY CASE state WHEN 'pending' THEN 0 WHEN 'retrying' THEN 1 ELSE 2 END,
                      updated_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(RemoteTombstoneProjection {
                id: row.get(0)?,
                provider: row.get(1)?,
                resource_kind: row.get(2)?,
                paper_id: row.get(3)?,
                state: row.get(4)?,
                attempts: row.get(5)?,
                last_error: row.get(6)?,
                ownership_status: row.get(7)?,
                created_at: row.get(8)?,
                updated_at: row.get(9)?,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())
}

fn mark_remote_resource_deleted(
    root: &Path,
    resource: &RemoteResource,
    endpoint_scope: &str,
) -> AppResult<()> {
    let connection = open_db(root)?;
    let timestamp = now();
    match resource.kind.as_str() {
        "file" => {
            connection
                .execute(
                    "UPDATE context_roots
                     SET provider_file_id = NULL, state = 'invalidated', invalidated_at = ?1
                     WHERE provider = ?2 AND provider_file_id = ?3
                       AND provider_route_id IN (
                         SELECT route_id FROM provider_route_snapshots
                         WHERE endpoint_scope = ?4
                       )",
                    params![timestamp, resource.provider, resource.id, endpoint_scope],
                )
                .map_err(|error| error.to_string())?;
        }
        "interaction" => {
            connection
                .execute(
                    "UPDATE provider_nodes
                     SET provider_node_id = NULL, state = 'remote_deleted'
                     WHERE provider = ?1 AND provider_node_id = ?2
                       AND provider_route_id IN (
                         SELECT route_id FROM provider_route_snapshots
                         WHERE endpoint_scope = ?3
                       )",
                    params![resource.provider, resource.id, endpoint_scope],
                )
                .map_err(|error| error.to_string())?;
            connection
                .execute(
                    "UPDATE context_roots
                     SET provider_node_id = NULL, state = 'invalidated', invalidated_at = ?1
                     WHERE provider = ?2 AND provider_node_id = ?3
                       AND provider_route_id IN (
                         SELECT route_id FROM provider_route_snapshots
                         WHERE endpoint_scope = ?4
                       )",
                    params![timestamp, resource.provider, resource.id, endpoint_scope],
                )
                .map_err(|error| error.to_string())?;
        }
        _ => {}
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct RemoteTombstone {
    id: String,
    resource: RemoteResource,
    endpoint_scope: String,
}

async fn retry_remote_tombstones_with_delete<F, Fut, G>(
    runtime: Arc<WorkspaceRuntime>,
    only_id: Option<&str>,
    mut delete_remote: F,
    mut on_changed: G,
) -> AppResult<usize>
where
    F: FnMut(RemoteTombstone) -> Fut,
    Fut: Future<Output = Result<(), ProviderError>>,
    G: FnMut(&str),
{
    let root = runtime.root.clone();
    let rows = {
        let connection = open_db(&root)?;
        let mut statement = connection
            .prepare(
                "SELECT id, provider, resource_kind, remote_id, endpoint_scope
                 FROM remote_tombstones
                 WHERE state IN ('pending', 'retrying')
                   AND ownership_status = 'exact'
                   AND endpoint_scope IS NOT NULL
                   AND (?1 IS NULL OR id = ?1)
                 ORDER BY updated_at ASC
                 LIMIT 32",
            )
            .map_err(|error| error.to_string())?;
        let tombstones = statement
            .query_map(params![only_id], |row| {
                Ok(RemoteTombstone {
                    id: row.get(0)?,
                    resource: RemoteResource {
                        provider: row.get(1)?,
                        kind: row.get(2)?,
                        id: row.get(3)?,
                    },
                    endpoint_scope: row.get(4)?,
                })
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        tombstones
    };

    let mut resolved = 0;
    for tombstone in rows {
        // A runtime swap stops the next request, but never discards the
        // captured runtime's result after a request has begun.
        if runtime.is_stopped() {
            break;
        }
        let updated = open_db(&root)?
            .execute(
                "UPDATE remote_tombstones
                 SET state = 'retrying', attempts = attempts + 1, updated_at = ?1
                 WHERE id = ?2 AND endpoint_scope = ?3
                   AND ownership_status = 'exact'",
                params![now(), tombstone.id, tombstone.endpoint_scope],
            )
            .map_err(|error| error.to_string())?;
        if updated == 0 {
            continue;
        }
        let result = delete_remote(tombstone.clone()).await;
        match result {
            Ok(()) => {
                mark_remote_resource_deleted(
                    &root,
                    &tombstone.resource,
                    &tombstone.endpoint_scope,
                )?;
                open_db(&root)?
                    .execute(
                        "UPDATE remote_tombstones
                         SET state = 'resolved', last_error = NULL, updated_at = ?1
                         WHERE id = ?2 AND endpoint_scope = ?3
                           AND ownership_status = 'exact'",
                        params![now(), tombstone.id, tombstone.endpoint_scope],
                    )
                    .map_err(|error| error.to_string())?;
                resolved += 1;
            }
            Err(error) => {
                open_db(&root)?
                    .execute(
                        "UPDATE remote_tombstones
                         SET state = 'pending', last_error = ?1, updated_at = ?2
                         WHERE id = ?3 AND endpoint_scope = ?4
                           AND ownership_status = 'exact'",
                        params![error.message, now(), tombstone.id, tombstone.endpoint_scope],
                    )
                    .map_err(|database_error| database_error.to_string())?;
            }
        }
        on_changed(&tombstone.id);
    }
    Ok(resolved)
}

struct RemoteDeleteContext<'a> {
    state: &'a AppState,
    settings: StoredModelSettings,
    root: PathBuf,
}

async fn delete_remote_resource(
    context: Arc<RemoteDeleteContext<'_>>,
    tombstone: RemoteTombstone,
) -> Result<(), ProviderError> {
    if tombstone.resource.provider == "mistral" {
        let key = resolve_mistral_key(context.state)
            .map_err(ProviderError::local_state)?
            .ok_or_else(|| {
                ProviderError::local_state(
                    "Mistral credential is unavailable for exact remote cleanup",
                )
            })?;
        let connection = open_db(&context.root).map_err(ProviderError::local_state)?;
        let route_id = connection
            .query_row(
                "SELECT route_id FROM provider_route_snapshots
                 WHERE endpoint_scope = ?1 AND operation_role = 'ocr'
                 ORDER BY created_at ASC, route_id ASC
                 LIMIT 1",
                params![tombstone.endpoint_scope],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .map_err(|error| ProviderError::local_state(error.to_string()))?
            .ok_or_else(|| {
                ProviderError::local_state("Remote cleanup has no Mistral OCR route snapshot")
            })?;
        let frozen = load_frozen_mistral_ocr_route(&connection, &route_id)
            .map_err(|error| ProviderError::local_state(error.to_string()))?
            .ok_or_else(|| {
                ProviderError::local_state("Remote cleanup Mistral route snapshot is missing")
            })?;
        if frozen.endpoint_scope_database_value() != tombstone.endpoint_scope {
            return Err(ProviderError::local_state(
                "Remote cleanup Mistral route does not own this endpoint",
            ));
        }
        verify_mistral_ocr_endpoint_scope(&connection, &tombstone.endpoint_scope, &key)
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        drop(connection);
        let delete_port =
            RemoteEndpointDeletePort::mistral_ocr(MistralOcrAdapter::production(key)?);
        return delete_port.delete_remote(&tombstone.resource).await;
    }

    let connection = open_db(&context.root).map_err(ProviderError::local_state)?;
    let route_id = connection
        .query_row(
            "SELECT route_id FROM provider_route_snapshots
             WHERE endpoint_scope = ?1
               AND operation_role IN ('paper', 'translation')
             ORDER BY created_at ASC, route_id ASC
             LIMIT 1",
            params![tombstone.endpoint_scope],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| ProviderError::local_state(error.to_string()))?
        .ok_or_else(|| {
            ProviderError::local_state("Remote cleanup has no paper endpoint snapshot")
        })?;
    let frozen = load_frozen_route(&connection, &route_id)
        .map_err(|error| ProviderError::local_state(error.to_string()))?
        .ok_or_else(|| ProviderError::local_state("Remote cleanup route snapshot is missing"))?;
    drop(connection);

    let bound = match ProviderRouting::new(&context.settings, context.state)
        .bind_endpoint(&frozen)
        .map_err(|error| ProviderError::local_state(error.to_string()))?
    {
        ProviderRouteDecision::Ready(route) => route,
        ProviderRouteDecision::ActionRequired(requirement) => {
            return Err(ProviderError::local_state(format!(
                "Remote cleanup requires provider action ({})",
                requirement.database_code()
            )));
        }
    };
    let delete_port = RemoteEndpointDeletePort::paper_provider(
        frozen.instance_id().as_str(),
        frozen.provider_kind().as_str(),
        bound.adapter_arc(),
    )?;
    delete_port.delete_remote(&tombstone.resource).await
}

pub(crate) async fn retry_remote_tombstones(
    app: &tauri::AppHandle,
    runtime: Arc<WorkspaceRuntime>,
    only_id: Option<&str>,
) -> AppResult<usize> {
    let state = app.state::<AppState>();
    let context = Arc::new(RemoteDeleteContext {
        state: &state,
        settings: read_model_settings(app)?,
        root: runtime.root.clone(),
    });
    let event_runtime = Arc::clone(&runtime);
    retry_remote_tombstones_with_delete(
        runtime,
        only_id,
        move |tombstone| delete_remote_resource(Arc::clone(&context), tombstone),
        move |tombstone_id| emit_job_event_for_runtime(app, &event_runtime, tombstone_id),
    )
    .await
}

pub(crate) fn spawn_remote_cleanup_worker(app: tauri::AppHandle, runtime: Arc<WorkspaceRuntime>) {
    tauri::async_runtime::spawn(async move {
        let _ = retry_remote_tombstones(&app, runtime, None).await;
    });
}
fn record_ocr_usage(
    root: &Path,
    job_id: &str,
    operation_id: &str,
    provider_route_id: &str,
    envelope: UsageEnvelope,
) -> AppResult<()> {
    open_db(root)?
        .execute(
            "INSERT INTO usage_receipts(
               id, operation_id, job_id, provider, model, context_epoch,
               input_tokens, cached_input_tokens, uncached_input_tokens,
               output_tokens, reasoning_tokens, latency_ms, estimated_cost,
               file_reuse, session_resume, paper_root_branch, created_at,
               provider_route_id
             ) VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
               ?13, ?14, ?15, ?16, ?17, ?18
             )",
            params![
                Uuid::new_v4().to_string(),
                operation_id,
                job_id,
                envelope.provider,
                envelope.model,
                envelope.context_epoch,
                envelope.input_tokens,
                envelope.cached_input_tokens,
                envelope.uncached_input_tokens,
                envelope.output_tokens,
                envelope.reasoning_tokens,
                envelope.latency_ms,
                envelope.estimated_cost,
                envelope.file_reuse.map(i64::from),
                envelope.session_resume.map(i64::from),
                envelope.paper_root_branch.map(i64::from),
                now(),
                provider_route_id,
            ],
        )
        .map_err(|database_error| database_error.to_string())?;
    Ok(())
}

async fn execute_ocr_job(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &FrozenMistralOcrRoute,
) -> Result<(), ProviderError> {
    if route.model() != MISTRAL_OCR_MODEL {
        return Err(ProviderError::local_state(
            "Frozen Mistral OCR model is not supported by this application version",
        ));
    }
    let provider_route_id = route.route_id().database_value();
    let endpoint_scope = route.endpoint_scope_database_value();
    let state = app.state::<AppState>();
    let root = runtime.root.clone();
    let job_module = runtime.job_module.clone();
    let artifact_module = runtime.artifact_module.clone();
    let connection = open_db(&root).map_err(ProviderError::local_state)?;
    let revision_id = job
        .revision_id
        .as_deref()
        .ok_or_else(|| ProviderError::local_state("OCR job is missing revisionId"))?;
    let mut revision =
        revision_record(&connection, revision_id).map_err(ProviderError::local_state)?;
    revision.pdf_path = absolute_pdf(&root, &revision.pdf_path);
    let paper_id: String = connection
        .query_row(
            "SELECT paper_id FROM document_revisions WHERE id = ?1",
            params![revision_id],
            |row| row.get(0),
        )
        .map_err(|error| ProviderError::local_state(error.to_string()))?;
    drop(connection);

    let staging_path = reusable_ocr_staging(&root, &job_module, revision_id, &job.id)
        .unwrap_or_else(|| ocr_staging_path(&root, &job.id));
    let outcome = if staging_path.is_file() {
        job_module
            .save_checkpoint(
                &job.id,
                "normalizing_staged",
                &json!({"stagingPath": staging_path}),
            )
            .map_err(ProviderError::local_state)?;
        normalize_staged_ocr(&staging_path, route.model())?
    } else {
        let api_key = resolve_mistral_key(&state)
            .map_err(ProviderError::local_state)?
            .ok_or_else(|| ProviderError::local_state("Configure Mistral OCR first"))?;
        let verification = open_db(&root).map_err(ProviderError::local_state)?;
        verify_mistral_ocr_endpoint_scope(&verification, &endpoint_scope, &api_key)
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        drop(verification);
        let adapter = MistralOcrAdapter::production(api_key)?;
        let cancellation = CancellationFlag::default();
        state
            .ocr_cancellations
            .lock()
            .map_err(|_| ProviderError::local_state("OCR cancellation lock poisoned"))?
            .insert(job.id.clone(), cancellation.clone());

        let commit_jobs = job_module.clone();
        let commit_job_id = job.id.clone();
        let before_provider_commit = Arc::new(move || {
            commit_jobs
                .mark_provider_committed(&commit_job_id, None)
                .map_err(ProviderError::local_state)
        });
        let (progress_sender, mut progress_receiver) =
            tokio::sync::mpsc::unbounded_channel::<provider_ports::ProviderProgress>();
        let progress_jobs = job_module.clone();
        let progress_job_id = job.id.clone();
        let progress_staging_path = staging_path.clone();
        let progress_app = app.clone();
        let progress_runtime = Arc::clone(runtime);
        tauri::async_runtime::spawn(async move {
            while let Some(progress) = progress_receiver.recv().await {
                let _ = progress_jobs.save_checkpoint(
                    &progress_job_id,
                    &progress.stage,
                    &json!({
                        "completed": progress.completed,
                        "total": progress.total,
                        "stagingPath": progress_staging_path
                    }),
                );
                emit_job_event_for_runtime(&progress_app, &progress_runtime, &progress_job_id);
            }
        });
        let result = adapter
            .parse_pdf(OcrRequest {
                pdf_path: revision.pdf_path.clone(),
                display_name: revision.title.clone(),
                cancellation,
                progress: Some(progress_sender),
                raw_staging_path: Some(staging_path.clone()),
                before_provider_commit: Some(before_provider_commit),
            })
            .await;
        if let Ok(mut cancellations) = state.ocr_cancellations.lock() {
            cancellations.remove(&job.id);
        }
        result?
    };

    job_module
        .save_checkpoint(&job.id, "publishing", &json!({"stagingPath": staging_path}))
        .map_err(ProviderError::local_state)?;
    let ocr = artifact_module
        .publish_ocr(
            revision_id,
            "mistral",
            route.model(),
            Some(staging_path.to_string_lossy().as_ref()),
            &outcome.pages,
        )
        .map_err(ProviderError::local_state)?;
    record_ocr_usage(&root, &job.id, &ocr.id, &provider_route_id, outcome.receipt)
        .map_err(ProviderError::local_state)?;
    if let Some(resource) = outcome.cleanup_resource {
        record_remote_tombstone(
            &root,
            &resource,
            Some(&paper_id),
            Some(&endpoint_scope),
            outcome
                .cleanup_warning
                .as_deref()
                .unwrap_or("Remote cleanup failed"),
        )
        .map_err(ProviderError::local_state)?;
    }
    job_module
        .complete(&job.id)
        .map_err(ProviderError::local_state)?;
    emit_job_event_for_runtime(app, runtime, &job.id);
    emit_read_event_for_runtime(
        app,
        runtime,
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "paper".to_string(),
            entity_id: Some(revision_id.to_string()),
            delta: None,
            status: None,
        },
    );
    Ok(())
}

fn required_job_payload(job: &JobProjection, key: &str) -> Result<String, ProviderError> {
    job.payload
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| ProviderError::local_state(format!("Job payload is missing {key}")))
}

async fn execute_reading_artifact_job(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
) -> Result<(), ProviderError> {
    let state = app.state::<AppState>();
    let root = runtime.root.clone();
    let job_module = runtime.job_module.clone();
    let revision_id = job
        .revision_id
        .clone()
        .ok_or_else(|| ProviderError::local_state("Reading Artifact job has no revisionId"))?;
    let ocr_revision_id = required_job_payload(job, "ocrRevisionId")?;
    let block_id = required_job_payload(job, "blockId")?;
    let action = match required_job_payload(job, "action")?.as_str() {
        "translate" => ReadingArtifactAction::Translate,
        "explain" => ReadingArtifactAction::Explain,
        "lens" => ReadingArtifactAction::Lens,
        _ => {
            return Err(ProviderError::local_state(
                "Reading Artifact action is invalid",
            ))
        }
    };
    let paper_model = route.frozen().models().paper().to_string();
    let translation_model = route
        .frozen()
        .models()
        .translation()
        .ok_or_else(|| {
            ProviderError::local_state("Reading Artifact route has no translation model")
        })?
        .to_string();
    let output_language = required_job_payload(job, "outputLanguage")?;
    let staging_key = job.payload.get("stagingKey").and_then(Value::as_str);
    let display_crop_data_url = read_artifact_material(
        &root,
        job.payload.get("displayCropPath").and_then(Value::as_str),
    )?;
    let model_crop_data_url = read_artifact_material(
        &root,
        job.payload.get("modelCropPath").and_then(Value::as_str),
    )?;
    let provider = route.frozen().provider_kind().as_str().to_string();

    job_module
        .save_checkpoint(
            &job.id,
            "validating",
            &json!({
                "revisionId": revision_id,
                "ocrRevisionId": ocr_revision_id,
                "blockId": block_id,
                "stagingKey": staging_key
            }),
        )
        .map_err(ProviderError::local_state)?;
    let cancellation = CancellationFlag::default();
    state
        .artifact_cancellations
        .lock()
        .map_err(|_| ProviderError::local_state("Artifact cancellation lock poisoned"))?
        .insert(job.id.clone(), cancellation.clone());
    let job_adapter = job_paper_model_adapter(
        route.adapter_arc(),
        job_module.clone(),
        job.id.clone(),
        cancellation,
    );
    let module = ReadingArtifactModule::open(&root).map_err(ProviderError::local_state)?;
    let provider_route_id = route.frozen().route_id().database_value();
    let generated = module
        .generate_for_route(
            &job_adapter,
            &provider_route_id,
            GenerateReadingArtifactRequest {
                document_kind: job
                    .payload
                    .get("documentKind")
                    .cloned()
                    .map(serde_json::from_value)
                    .transpose()
                    .map_err(|e| ProviderError::local_state(format!("文档类型无效：{e}")))?,
                revision_id: revision_id.clone(),
                ocr_revision_id,
                block_id,
                action,
                output_language,
                paper_model,
                translation_model,
                lens_protocol: lens_contract::LensProtocol::from_job(&job.payload)
                    .map_err(ProviderError::local_state)?,
                translation_protocol: translation_contract::TranslationProtocol::from_job(
                    &job.payload,
                )
                .map_err(ProviderError::local_state)?,
                provider,
                display_crop_data_url,
                model_crop_data_url,
                system_instruction: job
                    .payload
                    .get("prompts")
                    .and_then(|value| value.get("system"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                repair_system_instruction: job
                    .payload
                    .get("prompts")
                    .and_then(|value| value.get("repair"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                paper_root_system_instruction: job
                    .payload
                    .get("prompts")
                    .and_then(|value| value.get("paperRoot"))
                    .and_then(Value::as_str)
                    .map(str::to_string),
                reader_context: frozen_reader_context(&job.payload),
            },
        )
        .await;
    if let Ok(mut cancellations) = state.artifact_cancellations.lock() {
        cancellations.remove(&job.id);
    }
    let outcome = generated?;

    open_db(&root)
        .map_err(ProviderError::local_state)?
        .execute(
            "UPDATE usage_receipts SET job_id = ?1
             WHERE operation_id = ?2 AND job_id IS NULL",
            params![job.id, outcome.artifact.id],
        )
        .map_err(|error| ProviderError::local_state(error.to_string()))?;
    job_module
        .save_checkpoint(
            &job.id,
            "published",
            &json!({"artifactId": outcome.artifact.id, "repaired": outcome.repaired}),
        )
        .map_err(ProviderError::local_state)?;
    job_module
        .complete(&job.id)
        .map_err(ProviderError::local_state)?;
    cleanup_artifact_staging(&root, staging_key);
    emit_job_event_for_runtime(app, runtime, &job.id);
    emit_read_event_for_runtime(
        app,
        runtime,
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "paper".to_string(),
            entity_id: Some(revision_id),
            delta: None,
            status: None,
        },
    );
    Ok(())
}

#[async_trait::async_trait]
trait JobExecutionPort: Send + Sync {
    async fn execute(
        &self,
        claim: &workspace_lifecycle::RuntimeJobClaim,
    ) -> Result<(), ProviderError>;
}

fn mistral_ocr_preflight_requirement(
    state: &AppState,
    runtime: &WorkspaceRuntime,
    job: &JobProjection,
    route: &FrozenMistralOcrRoute,
) -> Result<Option<ProviderRequirement>, ProviderError> {
    let revision_id = job
        .revision_id
        .as_deref()
        .ok_or_else(|| ProviderError::local_state("OCR job is missing revisionId"))?;
    if reusable_ocr_staging(&runtime.root, &runtime.job_module, revision_id, &job.id).is_some() {
        return Ok(None);
    }
    let Some(key) = resolve_mistral_key(state).map_err(ProviderError::local_state)? else {
        return Ok(Some(ProviderRequirement::for_job(
            ProviderRequirementCode::CredentialMissing,
            None,
            None,
            false,
        )));
    };
    let connection = open_db(&runtime.root).map_err(ProviderError::local_state)?;
    if verify_mistral_ocr_endpoint_scope(&connection, &route.endpoint_scope_database_value(), &key)
        .is_err()
    {
        return Ok(Some(ProviderRequirement::for_job(
            ProviderRequirementCode::CredentialChanged,
            None,
            None,
            false,
        )));
    }
    Ok(None)
}

struct ProductionJobExecution {
    app: tauri::AppHandle,
}

impl ProductionJobExecution {
    async fn execute_paper(
        &self,
        runtime: &Arc<WorkspaceRuntime>,
        job: &JobProjection,
        route: &BoundProviderRoute,
    ) -> Result<(), ProviderError> {
        match job.kind.as_str() {
            "reading_artifact" => {
                execute_reading_artifact_job(&self.app, runtime, job, route).await
            }
            "orientation_pack" if job.payload.get("documentArtifactProtocol").is_some() => {
                document_artifacts::execute(&self.app, runtime, job, route).await
            }
            "document_artifact" => {
                document_artifacts::execute(&self.app, runtime, job, route).await
            }
            "orientation_pack" => execute_orientation_job(&self.app, runtime, job, route).await,
            "reading_roadmap" => execute_roadmap_job(&self.app, runtime, job, route).await,
            "outline_overview" => {
                execute_outline_overview_job(&self.app, runtime, job, route).await
            }
            "outline_deep_dive" => {
                execute_outline_deep_dive_job(&self.app, runtime, job, route).await
            }
            "reading_guide" => execute_reading_guide_job(&self.app, runtime, job, route).await,
            _ => Err(ProviderError::local_state(format!(
                "No paper worker is registered for job kind {}",
                job.kind
            ))),
        }
    }
}

#[async_trait::async_trait]
impl JobExecutionPort for ProductionJobExecution {
    async fn execute(
        &self,
        claim: &workspace_lifecycle::RuntimeJobClaim,
    ) -> Result<(), ProviderError> {
        let record = &claim.job;
        let job = record.project();
        let result = match &record.route {
            StoredJobRoute::Executable(JobExecutionRoute::MistralOcr(route))
                if job.kind == "ocr" =>
            {
                let requirement = {
                    let state = self.app.state::<AppState>();
                    mistral_ocr_preflight_requirement(&state, &claim.runtime, &job, route)?
                };
                if let Some(requirement) = requirement {
                    claim
                        .runtime
                        .job_module
                        .block_for_provider_action(&job.id, requirement)
                        .map_err(ProviderError::local_state)?;
                    emit_job_event_for_runtime(&self.app, &claim.runtime, &job.id);
                    Ok(())
                } else {
                    execute_ocr_job(&self.app, &claim.runtime, &job, route).await
                }
            }
            StoredJobRoute::Executable(JobExecutionRoute::Paper(frozen)) => {
                let state = self.app.state::<AppState>();
                let settings =
                    read_model_settings(&self.app).map_err(ProviderError::local_state)?;
                let decision = ProviderRouting::new(&settings, &*state)
                    .bind(frozen)
                    .map_err(|error| ProviderError::local_state(error.to_string()))?;
                drop(state);
                match decision {
                    ProviderRouteDecision::Ready(route) => {
                        self.execute_paper(&claim.runtime, &job, &route).await
                    }
                    ProviderRouteDecision::ActionRequired(requirement) => {
                        claim
                            .runtime
                            .job_module
                            .block_for_provider_action(&job.id, requirement)
                            .map_err(ProviderError::local_state)?;
                        emit_job_event_for_runtime(&self.app, &claim.runtime, &job.id);
                        Ok(())
                    }
                }
            }
            StoredJobRoute::Executable(JobExecutionRoute::MistralOcr(_)) => Err(
                ProviderError::local_state("Mistral route is attached to a non-OCR job"),
            ),
            StoredJobRoute::Executable(JobExecutionRoute::Local) => Err(
                ProviderError::local_state("No local worker is registered for this job"),
            ),
            StoredJobRoute::LegacyUnattributed => Err(ProviderError::local_state(
                "Legacy unattributed provider work cannot execute",
            )),
            StoredJobRoute::Unavailable { .. } => Err(ProviderError::local_state(
                "Unavailable provider route cannot execute",
            )),
        };
        if matches!(
            job.kind.as_str(),
            "reading_artifact"
                | "orientation_pack"
                | "document_artifact"
                | "reading_guide"
                | "outline_overview"
                | "outline_deep_dive"
        ) {
            let state = self.app.state::<AppState>();
            if let Ok(mut cancellations) = state.artifact_cancellations.lock() {
                cancellations.remove(&job.id);
            };
        }
        result
    }
}

fn failed_job_endpoint_scope(claim: &workspace_lifecycle::RuntimeJobClaim) -> Option<String> {
    match &claim.job.route {
        StoredJobRoute::Executable(JobExecutionRoute::MistralOcr(route)) => {
            Some(route.endpoint_scope_database_value())
        }
        StoredJobRoute::Executable(JobExecutionRoute::Paper(route)) => open_db(&claim.runtime.root)
            .ok()?
            .query_row(
                "SELECT endpoint_scope FROM provider_route_snapshots WHERE route_id = ?1",
                params![route.route_id().database_value()],
                |row| row.get::<_, String>(0),
            )
            .optional()
            .ok()
            .flatten(),
        StoredJobRoute::Executable(JobExecutionRoute::Local)
        | StoredJobRoute::LegacyUnattributed
        | StoredJobRoute::Unavailable { .. } => None,
    }
}

fn finalize_failed_job(claim: &workspace_lifecycle::RuntimeJobClaim, error: &ProviderError) {
    let job = &claim.job;
    if matches!(
        job.kind.as_str(),
        "reading_artifact" | "orientation_pack" | "document_artifact"
    ) {
        cleanup_artifact_staging(
            &claim.runtime.root,
            job.payload.get("stagingKey").and_then(Value::as_str),
        );
    }
    if let Some(resource) = &error.orphaned_resource {
        let endpoint_scope = failed_job_endpoint_scope(claim);
        let _ = record_remote_tombstone(
            &claim.runtime.root,
            resource,
            job.paper_id.as_deref(),
            endpoint_scope.as_deref(),
            &error.message,
        );
    }
    let _ = claim.runtime.job_module.fail(&job.id, &error.message);
}

async fn run_job_worker_core<E, C, F>(
    runtime: Arc<WorkspaceRuntime>,
    execution: &E,
    mut on_claimed: C,
    mut on_failed: F,
) where
    E: JobExecutionPort + ?Sized,
    C: FnMut(&workspace_lifecycle::RuntimeJobClaim),
    F: FnMut(&workspace_lifecycle::RuntimeJobClaim, &ProviderError),
{
    loop {
        let claim = match runtime.claim_next_job() {
            Ok(Some(claim)) => claim,
            _ => return,
        };
        on_claimed(&claim);
        if let Err(error) = execution.execute(&claim).await {
            finalize_failed_job(&claim, &error);
            on_failed(&claim, &error);
        }
    }
}

async fn run_job_worker(app: tauri::AppHandle, runtime: Arc<WorkspaceRuntime>) {
    let execution = ProductionJobExecution { app: app.clone() };
    let claimed_app = app.clone();
    let failed_app = app;
    run_job_worker_core(
        runtime,
        &execution,
        move |claim| emit_job_event_for_runtime(&claimed_app, &claim.runtime, &claim.job.id),
        move |claim, error| {
            let job = &claim.job;
            if let StoredJobRoute::Executable(JobExecutionRoute::Paper(route)) = &job.route {
                maybe_invalidate_paper_probe(&failed_app, route.instance_id().as_str(), error);
            }
            emit_job_event_for_runtime(&failed_app, &claim.runtime, &job.id);
        },
    )
    .await;
}

pub(crate) fn spawn_job_workers(app: tauri::AppHandle, runtime: Arc<WorkspaceRuntime>) {
    for _ in 0..2 {
        tauri::async_runtime::spawn(run_job_worker(app.clone(), runtime.clone()));
    }
}

fn ensure_long_pdf_acknowledged(
    runtime: &WorkspaceRuntime,
    revision_id: &str,
    page_count: Option<i64>,
) -> AppResult<()> {
    if page_count.is_some_and(|count| count >= crate::library_paths::LONG_PDF_PAGE_LIMIT) {
        let root = &runtime.root;
        let connection = open_db(&root)?;
        let paper_id: String = connection
            .query_row(
                "SELECT paper_id FROM document_revisions WHERE id = ?1",
                params![revision_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !runtime.paper_module.long_pdf_warning_acked(&paper_id)? {
            return Err("long_pdf_warning_required".to_string());
        }
    }
    Ok(())
}

#[tauri::command]
fn ack_long_pdf_warning(
    state: State<'_, AppState>,
    paper_id: String,
    revision_id: String,
) -> AppResult<()> {
    active_paper_module(&state)?.ack_long_pdf_warning(&paper_id, &revision_id)
}

#[tauri::command]
fn start_ocr(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    revision_id: String,
) -> AppResult<JobProjection> {
    let api_key = resolve_mistral_key(&state)?
        .ok_or_else(|| "Configure Mistral OCR before starting OCR".to_string())?;
    let route = capture_mistral_ocr_route(&api_key, MISTRAL_OCR_MODEL)
        .map_err(|error| error.to_string())?;
    let runtime = current_runtime(&state)?;
    let root = runtime.root.clone();
    let connection = open_db(&root)?;
    let paper_id: String = connection
        .query_row(
            "SELECT paper_id FROM document_revisions WHERE id = ?1",
            params![revision_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let page_count: Option<i64> = connection
        .query_row(
            "SELECT page_count FROM document_revisions WHERE id = ?1",
            params![revision_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    drop(connection);
    ensure_long_pdf_acknowledged(&runtime, &revision_id, page_count)?;
    let enqueue = runtime.job_module.enqueue_record(
        JobSpec {
            kind: "ocr".to_string(),
            provider: Some("mistral".to_string()),
            paper_id: Some(paper_id),
            revision_id: Some(revision_id.clone()),
            root_key: None,
            artifact_key: Some("ocr".to_string()),
            dedupe_key: format!("ocr:{revision_id}"),
            priority: 100,
            payload: json!({"model": MISTRAL_OCR_MODEL}),
        },
        JobExecutionRoute::MistralOcr(route),
    )?;
    emit_job_event_for_runtime(&app, &runtime, &enqueue.job.id);
    spawn_job_workers(app, runtime);
    Ok(enqueue.job.project())
}

#[tauri::command]
fn latest_ocr(state: State<'_, AppState>, revision_id: String) -> AppResult<Option<OcrProjection>> {
    active_artifact_module(&state)?.latest_ocr(&revision_id)
}

#[tauri::command]
fn get_ocr(state: State<'_, AppState>, ocr_revision_id: String) -> AppResult<OcrProjection> {
    active_artifact_module(&state)?.ocr(&ocr_revision_id)
}

#[tauri::command]
fn delete_ocr_cascade(state: State<'_, AppState>, revision_id: String) -> AppResult<bool> {
    crate::artifact_commands::delete_ocr_cascade_impl(&state, &revision_id)
}

#[tauri::command]
fn update_paper_tags(
    state: State<'_, AppState>,
    paper_id: String,
    tags: Vec<String>,
) -> AppResult<()> {
    let root = active_root(&state)?;
    let mut connection = open_db(&root)?;
    let transaction = connection
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;

    // 1. Update latest brief artifact content_json.keywords
    let brief_row: Option<(String, String)> = transaction
        .query_row(
            "SELECT a.id, a.content_json FROM artifacts a
             JOIN artifact_heads h ON h.artifact_id = a.id
             WHERE a.paper_id = ?1 AND a.kind = 'brief'",
            params![paper_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    if let Some((brief_id, content_raw)) = brief_row {
        if let Ok(mut content_val) = serde_json::from_str::<Value>(&content_raw) {
            if let Some(obj) = content_val.as_object_mut() {
                obj.insert("keywords".to_string(), json!(tags));
                let new_content = serde_json::to_string(&content_val).unwrap_or(content_raw);
                transaction
                    .execute(
                        "UPDATE artifacts SET content_json = ?1 WHERE id = ?2",
                        params![new_content, brief_id],
                    )
                    .map_err(|e| e.to_string())?;
            }
        }
    }

    // 2. Synchronize tags and paper_tags
    transaction
        .execute(
            "DELETE FROM paper_tags WHERE paper_id = ?1",
            params![paper_id],
        )
        .map_err(|e| e.to_string())?;

    let timestamp = now();
    for tag_name in &tags {
        let clean_name = tag_name.trim();
        if clean_name.is_empty() {
            continue;
        }
        let tag_id = Uuid::new_v4().to_string();
        transaction
            .execute(
                "INSERT OR IGNORE INTO tags(id, name, created_at) VALUES (?1, ?2, ?3)",
                params![tag_id, clean_name, timestamp],
            )
            .map_err(|e| e.to_string())?;

        let actual_tag_id: String = transaction
            .query_row(
                "SELECT id FROM tags WHERE name = ?1",
                params![clean_name],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;

        transaction
            .execute(
                "INSERT OR IGNORE INTO paper_tags(paper_id, tag_id) VALUES (?1, ?2)",
                params![paper_id, actual_tag_id],
            )
            .map_err(|e| e.to_string())?;
    }

    bump_library_revisions(
        &transaction,
        &[LibraryDomain::Tags, LibraryDomain::Artifacts],
    )
    .map_err(|e| e.to_string())?;
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn update_paper_metadata(
    state: State<'_, AppState>,
    revision_id: String,
    metadata: Value,
    pinned_fields: Vec<String>,
) -> AppResult<()> {
    let root = active_root(&state)?;
    edit_auxiliary(
        &root,
        &revision_id,
        "metadata",
        metadata["_editBaseArtifactId"].as_str(),
        |old| {
            crate::auxiliary_state::edit_metadata(&old.content, &metadata, pinned_fields, &old.id)
        },
    )
}

#[tauri::command]
fn update_orientation_table(
    state: State<'_, AppState>,
    revision_id: String,
    kind: String,
    entries: Vec<Value>,
    pinned_keys: Vec<String>,
    artifact_id: Option<String>,
) -> AppResult<()> {
    if kind != "glossary" && kind != "symbol_table" {
        return Err("Only glossary and symbol_table can be updated via this command".into());
    }
    let root = active_root(&state)?;
    edit_auxiliary(&root, &revision_id, &kind, artifact_id.as_deref(), |old| {
        crate::auxiliary_state::edit_table(&old.content, entries, pinned_keys)
    })
}
fn edit_auxiliary<F>(
    root: &Path,
    revision_id: &str,
    kind: &str,
    expected_id: Option<&str>,
    edit: F,
) -> AppResult<()>
where
    F: FnOnce(&ArtifactProjection) -> Value,
{
    let module =
        crate::artifact_module::ArtifactModule::open(root.join(".read-desktop/workspace.sqlite3"))?;
    let mut existing = module
        .head_for_revision(revision_id, kind, "")?
        .ok_or("当前成果不存在")?;
    if expected_id.is_some_and(|id| id != existing.id) {
        return Err("成果已更新，请刷新后重试，未覆盖其他修改。".into());
    }
    existing.content = crate::auxiliary_state::apply_legacy_table_overrides(
        &existing.content,
        &existing.overrides,
    );
    let content = edit(&existing);
    let expected = expected_id.unwrap_or(&existing.id);
    let revision = revision_record(&open_db(root)?, revision_id)?;
    let mut dependency = existing.dependency_snapshot.clone();
    if let Some(obj) = dependency.as_object_mut() {
        obj.remove("jobId");
    }
    dependency["manualSourceArtifactId"] = json!(expected);
    module.publish_prepared(ArtifactDraft{paper_id:existing.paper_id,revision_id:revision_id.into(),ocr_revision_id:existing.ocr_revision_id,kind:kind.into(),object_key:String::new(),content,evidence:existing.evidence,dependency_snapshot:dependency,provider_node_id:existing.provider_node_id},|connection,draft|{
        let current:String=connection.query_row("SELECT artifact_id FROM artifact_heads WHERE paper_id=?1 AND kind=?2 AND object_key=''",params![draft.paper_id,kind],|r|r.get(0)).map_err(|e|e.to_string())?;
        if current!=expected{return Err("成果已更新，请刷新后重试，未覆盖其他修改。".into());}
        if kind=="metadata"{document_artifacts::sync_metadata(connection,&revision,&draft.content)?;}
        Ok(())
    })?;
    Ok(())
}

#[tauri::command]
fn set_artifact_override(
    state: State<'_, AppState>,
    artifact_id: String,
    kind: String,
    key: String,
    value: Value,
) -> AppResult<bool> {
    active_artifact_module(&state)?.set_override(&artifact_id, &kind, &key, &value)?;
    Ok(true)
}

#[tauri::command]
fn list_lens_qa(
    state: State<'_, AppState>,
    lens_artifact_id: String,
) -> AppResult<Vec<LensQaProjection>> {
    active_artifact_module(&state)?.list_lens_qa(&lens_artifact_id)
}

#[tauri::command]
#[expect(
    clippy::too_many_arguments,
    reason = "Tauri command parameters are the stable DesktopClient IPC contract"
)]
fn generate_reading_artifact(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    revision_id: String,
    ocr_revision_id: String,
    block_id: String,
    action: ReadingArtifactAction,
    output_language: Option<String>,
    display_crop_data_url: Option<String>,
    model_crop_data_url: Option<String>,
) -> AppResult<JobProjection> {
    let current = require_current_paper_provider(&app, &state)?;
    let paper_model = normalize_paper_model_id(&current.provider, &current.paper_model)?;
    let translation_model =
        normalize_paper_model_id(&current.provider, &current.translation_model)?;
    let output_language = resolve_output_language(&app, output_language.as_deref())?;
    let runtime = current_runtime(&state)?;
    let root = runtime.root.clone();
    let connection = open_db(&root)?;
    let (paper_id, block_digest, block_type): (String, String, String) = connection
        .query_row(
            "SELECT r.paper_id, b.content_digest, b.block_type
             FROM ocr_blocks b
             JOIN ocr_pages pg ON pg.id = b.ocr_page_id
             JOIN ocr_revisions o ON o.id = pg.ocr_revision_id
             JOIN document_revisions r ON r.id = o.revision_id
             WHERE r.id = ?1 AND o.id = ?2 AND b.id = ?3 AND o.status = 'ready'",
            params![revision_id, ocr_revision_id, block_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .map_err(|_| {
            "The selected Block does not belong to the requested OCR and Document Revision"
                .to_string()
        })?;
    drop(connection);

    let action_key = match action {
        ReadingArtifactAction::Translate => "translate",
        ReadingArtifactAction::Explain => "explain",
        ReadingArtifactAction::Lens => "lens",
    };
    let normalized_block_type = block_type.trim().to_ascii_lowercase();
    let lens_block = matches!(
        normalized_block_type.as_str(),
        "formula" | "equation" | "figure" | "image" | "table"
    );
    if (matches!(action, ReadingArtifactAction::Lens) && !lens_block)
        || (!matches!(action, ReadingArtifactAction::Lens) && lens_block)
    {
        return Err(
            "Formula, Figure, and Table Blocks use Lens; ordinary text uses translation or explanation"
                .to_string(),
        );
    }
    if matches!(action, ReadingArtifactAction::Lens)
        && (display_crop_data_url.is_none() || model_crop_data_url.is_none())
    {
        return Err("Lens requires both display and model crops".to_string());
    }
    let route = capture_current_paper_job_route(
        &app,
        &state,
        &current,
        &paper_model,
        Some(&translation_model),
        if matches!(action, ReadingArtifactAction::Translate) {
            ModelRole::Translation
        } else {
            ModelRole::Paper
        },
    )?;
    let staging_key = Uuid::new_v4().to_string();
    let display_crop_path = if matches!(action, ReadingArtifactAction::Lens) {
        stage_artifact_material(
            &root,
            &staging_key,
            "display",
            display_crop_data_url.as_deref(),
        )?
    } else {
        None
    };
    let model_crop_path = if matches!(action, ReadingArtifactAction::Lens) {
        match stage_artifact_material(&root, &staging_key, "model", model_crop_data_url.as_deref())
        {
            Ok(path) => path,
            Err(error) => {
                cleanup_artifact_staging(&root, Some(&staging_key));
                return Err(error);
            }
        }
    } else {
        None
    };
    let language = Some(output_language.as_str());
    let document_kind = revision_document_kind_from_root(&runtime.root, &revision_id);
    let locale = resolve_ui_locale(&app)?.unwrap_or(ui_locale::UiLocale::ZhCn);
    let artifact_prompt_store = load_store_in(&prompt_settings_file(&app)?, locale)?;
    let paper_root_prompt =
        load_prompt_text(&app, PromptSlotId::PaperRoot, document_kind, language)?;
    let artifact_prompts = match action {
        ReadingArtifactAction::Translate => {
            let text = load_prompt_text(&app, PromptSlotId::Translation, document_kind, language)?;
            json!({
                "system": text,
                "translationProtocol": prompt_settings::resolved_protocol(&artifact_prompt_store, PromptSlotId::Translation, document_kind),
                "paperRoot": paper_root_prompt
            })
        }
        ReadingArtifactAction::Explain => json!({
            "system": load_prompt_text(&app, PromptSlotId::Explanation, document_kind, language)?,
            "paperRoot": paper_root_prompt
        }),
        ReadingArtifactAction::Lens => {
            let (generate_slot, repair_slot) = lens_prompt_slots(&block_type)?;
            json!({
                "system": load_prompt_text(&app, generate_slot, document_kind, language)?,
                "repair": load_prompt_text(&app, repair_slot, document_kind, language)?,
                "lensProtocol": prompt_settings::resolved_protocol(&artifact_prompt_store, generate_slot, document_kind),
                "paperRoot": paper_root_prompt
            })
        }
    };
    let job_module = runtime.job_module.clone();
    let reader_context = match action {
        ReadingArtifactAction::Explain | ReadingArtifactAction::Lens => {
            resolve_reader_wrapper_for_revision(&runtime.root, &revision_id)
        }
        ReadingArtifactAction::Translate => None,
    };
    let payload = json!({
        "action": action_key,
        "ocrRevisionId": ocr_revision_id,
        "blockId": block_id,
        "blockDigest": block_digest,
        "outputLanguage": output_language,
        "displayCropPath": display_crop_path,
        "modelCropPath": model_crop_path,
        "stagingKey": staging_key,
        "prompts": artifact_prompts,
        "documentKind": document_kind.as_str(),
        "readerContext": reader_context
    });
    let root_key = (!matches!(action, ReadingArtifactAction::Translate))
        .then(|| route_scoped_root_key(&revision_id, &route));
    let enqueue = match enqueue_paper_job(
        &job_module,
        JobSpec {
            kind: "reading_artifact".to_string(),
            provider: None,
            paper_id: Some(paper_id),
            revision_id: Some(revision_id.clone()),
            root_key,
            artifact_key: Some(format!("{action_key}:{block_id}")),
            dedupe_key: format!(
                "artifact:{revision_id}:{ocr_revision_id}:{block_id}:{action_key}:{output_language}"
            ),
            priority: 80,
            payload,
        },
        &route,
    ) {
        Ok(enqueue) => enqueue,
        Err(error) => {
            cleanup_artifact_staging(&root, Some(&staging_key));
            return Err(error);
        }
    };
    if enqueue.coalesced {
        cleanup_artifact_staging(&root, Some(&staging_key));
        emit_job_event_for_runtime(&app, &runtime, &enqueue.job.id);
        return Ok(enqueue.job);
    }

    let dependencies = [
        ("document_revision", revision_id.clone(), revision_id),
        ("ocr_revision", ocr_revision_id.clone(), ocr_revision_id),
        ("ocr_block", block_id.clone(), block_digest),
        ("paper_model", "paper_model".to_string(), paper_model),
        (
            "translation_model",
            "translation_model".to_string(),
            translation_model,
        ),
    ];
    let dependency_result = (|| -> AppResult<()> {
        let mut connection = open_db(&root)?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        for (kind, id, revision) in dependencies {
            transaction
                .execute(
                    "INSERT INTO job_dependency_snapshots(
                       job_id, dependency_kind, dependency_id, dependency_revision
                     ) VALUES (?1, ?2, ?3, ?4)",
                    params![enqueue.job.id, kind, id, revision],
                )
                .map_err(|error| error.to_string())?;
        }
        transaction.commit().map_err(|error| error.to_string())
    })();
    if let Err(error) = dependency_result {
        let _ = job_module.cancel(&enqueue.job.id);
        cleanup_artifact_staging(&root, Some(&staging_key));
        return Err(error);
    }

    emit_job_event_for_runtime(&app, &runtime, &enqueue.job.id);
    spawn_job_workers(app, runtime);
    Ok(enqueue.job)
}
#[tauri::command]
async fn ask_lens(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    lens_artifact_id: String,
    parent_id: Option<String>,
    question: String,
) -> AppResult<LensQaTurn> {
    let runtime = current_runtime(&state)?;
    let current = require_current_paper_provider(&app, &state)?;
    let paper_model = normalize_paper_model_id(&current.provider, &current.paper_model)?;
    let translation_model =
        normalize_paper_model_id(&current.provider, &current.translation_model)?;
    let route = capture_current_paper_job_route(
        &app,
        &state,
        &current,
        &paper_model,
        Some(&translation_model),
        ModelRole::Paper,
    )?;
    let route_id = route.frozen().route_id().database_value();
    let root = runtime.root.clone();
    let module = ReadingArtifactModule::open(&root)?;
    let revision_id: String = open_db(&root)?
        .query_row(
            "SELECT revision_id FROM artifacts WHERE id = ?1",
            params![lens_artifact_id],
            |row| row.get(0),
        )
        .unwrap_or_default();
    let output_language = resolve_output_language(&app, None)?;
    let lens_qa_prompt = load_prompt_for_revision_from_root(
        &app,
        &root,
        &revision_id,
        PromptSlotId::LensQa,
        Some(&output_language),
    )?;
    module
        .ask_lens_for_route(
            route.adapter(),
            &route_id,
            AskLensRequest {
                output_language: Some(output_language),
                lens_artifact_id,
                parent_id,
                question,
                system_instruction: Some(lens_qa_prompt),
                provider: current.provider,
                reader_context: resolve_reader_wrapper_for_revision(&root, &revision_id),
            },
        )
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_jobs(state: State<'_, AppState>) -> AppResult<Vec<JobProjection>> {
    active_job_module(&state)?.list()
}

#[tauri::command]
fn delete_outline(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: OutlineRequest,
) -> AppResult<OutlineProjection> {
    let runtime = current_runtime(&state)?;
    let jobs = runtime.job_module.clone();
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
        emit_job_event_for_runtime(&app, &runtime, &job.id);
    }
    runtime
        .outline_module
        .delete_overview(&request.revision_id)?;
    runtime
        .outline_module
        .project_without_route(&request.revision_id)
}

#[tauri::command]
fn delete_outline_deep_dive(
    state: State<'_, AppState>,
    request: OutlineDeepDiveRequest,
) -> AppResult<bool> {
    let jobs = active_job_module(&state)?;
    let running = jobs
        .list_active_of("outline_deep_dive", &request.revision_id)?
        .into_iter()
        .any(|job| {
            job.payload.get("nodeId").and_then(Value::as_str) == Some(request.node_id.as_str())
        });
    if running {
        return Err("Cancel the running Deep dive from the task center first.".to_string());
    }
    active_outline_module(&state)?.delete_deep_dive(&request.revision_id, &request.node_id)?;
    Ok(true)
}

#[tauri::command]
fn list_remote_tombstones(state: State<'_, AppState>) -> AppResult<Vec<RemoteTombstoneProjection>> {
    list_remote_tombstones_from_root(&active_root(&state)?)
}

#[tauri::command]
async fn retry_remote_cleanup(
    app: tauri::AppHandle,
    tombstone_id: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<usize> {
    let runtime = current_runtime(&state)?;
    retry_remote_tombstones(&app, runtime, tombstone_id.as_deref()).await
}

fn abandon_legacy_remote_tombstone(
    root: &Path,
    tombstone_id: &str,
    confirmed: bool,
) -> AppResult<bool> {
    if !confirmed {
        return Err("Explicit confirmation is required to abandon remote cleanup".to_string());
    }
    let changed = open_db(root)?
        .execute(
            "UPDATE remote_tombstones
             SET ownership_status = 'abandoned', state = 'resolved',
                 last_error = COALESCE(
                   NULLIF(last_error, ''),
                   'Abandoned locally; no remote deletion request was made'
                 ),
                 updated_at = ?1
             WHERE id = ?2
               AND ownership_status = 'legacy_unattributed'
               AND endpoint_scope IS NULL",
            params![now(), tombstone_id],
        )
        .map_err(|error| error.to_string())?;
    Ok(changed > 0)
}

#[tauri::command]
fn abandon_remote_cleanup(
    app: tauri::AppHandle,
    tombstone_id: String,
    confirmed: bool,
    state: State<'_, AppState>,
) -> AppResult<bool> {
    let runtime = current_runtime(&state)?;
    let abandoned = abandon_legacy_remote_tombstone(&runtime.root, &tombstone_id, confirmed)?;
    if abandoned {
        emit_job_event_for_runtime(&app, &runtime, &tombstone_id);
    }
    Ok(abandoned)
}

#[tauri::command]
fn recheck_provider_job(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    job_id: String,
) -> AppResult<JobProjection> {
    job_commands::recheck_provider_job_impl(app, &state, &job_id)
}

#[tauri::command]
fn rebind_provider_job(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    replacement_instance_id: String,
) -> AppResult<job_commands::RebindProviderJobOutcome> {
    job_commands::rebind_provider_job_impl(app, &state, &job_id, &replacement_instance_id)
}

#[tauri::command]
fn abandon_legacy_provider_job(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    confirmed_potential_charge: bool,
) -> AppResult<JobProjection> {
    job_commands::abandon_legacy_provider_job_impl(
        &app,
        &state,
        &job_id,
        confirmed_potential_charge,
    )
}

#[tauri::command]
fn pause_job(app: tauri::AppHandle, state: State<'_, AppState>, job_id: String) -> AppResult<bool> {
    let runtime = current_runtime(&state)?;
    runtime.job_module.pause(&job_id)?;
    emit_job_event_for_runtime(&app, &runtime, &job_id);
    Ok(true)
}

#[tauri::command]
fn resume_job(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    job_id: String,
) -> AppResult<bool> {
    let runtime = current_runtime(&state)?;
    runtime.job_module.resume(&job_id)?;
    emit_job_event_for_runtime(&app, &runtime, &job_id);
    spawn_job_workers(app, runtime);
    Ok(true)
}

#[tauri::command]
fn reprioritize_job(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    job_id: String,
    priority: i64,
) -> AppResult<bool> {
    let runtime = current_runtime(&state)?;
    runtime.job_module.reprioritize(&job_id, priority)?;
    emit_job_event_for_runtime(&app, &runtime, &job_id);
    Ok(true)
}

#[tauri::command]
fn cancel_job(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    job_id: String,
) -> AppResult<bool> {
    let runtime = current_runtime(&state)?;
    if let Some(cancellation) = state
        .ocr_cancellations
        .lock()
        .map_err(|_| "OCR cancellation lock poisoned".to_string())?
        .get(&job_id)
        .cloned()
    {
        cancellation.cancel();
    }
    if let Some(cancellation) = state
        .artifact_cancellations
        .lock()
        .map_err(|_| "Artifact cancellation lock poisoned".to_string())?
        .get(&job_id)
        .cloned()
    {
        cancellation.cancel();
    }
    runtime.job_module.cancel(&job_id)?;
    emit_job_event_for_runtime(&app, &runtime, &job_id);
    Ok(true)
}

fn finish_streaming_discussions(root: &Path) -> AppResult<usize> {
    open_db(root)?
        .execute(
            "UPDATE messages
             SET status = 'cancelled', updated_at = ?1
             WHERE status = 'streaming'",
            params![now()],
        )
        .map_err(|error| error.to_string())
}

fn active_work_exists(state: &AppState) -> bool {
    if state
        .discussion_cancellations
        .lock()
        .map(|cancellations| !cancellations.is_empty())
        .unwrap_or(true)
    {
        return true;
    }
    current_runtime(state)
        .ok()
        .and_then(|runtime| runtime.job_module.list().ok())
        .is_some_and(|jobs| {
            jobs.into_iter().any(|job| {
                matches!(
                    job.state,
                    job_module::JobState::Queued
                        | job_module::JobState::Running
                        | job_module::JobState::Paused
                )
            })
        })
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn emit_exit_choice_required(app: &tauri::AppHandle) {
    let _ = app.emit(
        "read-event",
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "lifecycle".to_string(),
            entity_id: None,
            delta: None,
            status: Some("exit_choice_required".to_string()),
        },
    );
}

fn request_exit_or_prompt(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    if active_work_exists(&state) {
        show_main_window(app);
        emit_exit_choice_required(app);
    } else {
        state.exit_authorized.store(true, Ordering::Release);
        app.exit(0);
    }
}

#[tauri::command]
fn resolve_exit_intent(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    mode: ExitIntent,
) -> AppResult<ExitResolution> {
    if matches!(mode, ExitIntent::ContinueInTray) {
        let window = app
            .get_webview_window("main")
            .ok_or_else(|| "Main window is unavailable".to_string())?;
        window.hide().map_err(|error| error.to_string())?;
        return Ok(ExitResolution {
            mode: "continue_in_tray".to_string(),
            jobs: ExitPreparation::default(),
            cancelled_discussions: 0,
        });
    }

    cancel_runtime_work(&state)?;
    let runtime = current_runtime(&state).ok();
    let cancelled_discussions = runtime
        .as_ref()
        .map(|runtime| finish_streaming_discussions(&runtime.root))
        .transpose()?
        .unwrap_or(0);
    let jobs = {
        let module = runtime.as_ref().map(|runtime| runtime.job_module.clone());
        match (mode.clone(), module) {
            (ExitIntent::PauseAndExit, Some(module)) => module.prepare_for_pause_exit()?,
            (ExitIntent::CancelAndExit, Some(module)) => module.cancel_all_active()?,
            _ => ExitPreparation::default(),
        }
    };
    let mode = match mode {
        ExitIntent::PauseAndExit => "pause_and_exit",
        ExitIntent::CancelAndExit => "cancel_and_exit",
        ExitIntent::ContinueInTray => unreachable!(),
    }
    .to_string();
    let resolution = ExitResolution {
        mode,
        jobs,
        cancelled_discussions,
    };
    state.exit_authorized.store(true, Ordering::Release);
    app.exit(0);
    Ok(resolution)
}

#[tauri::command]
fn open_pdf_external(state: State<'_, AppState>, path: String) -> AppResult<bool> {
    let root = fs::canonicalize(active_root(&state)?)
        .map_err(|error| format!("Unable to resolve Workspace path: {error}"))?;
    let pdf = fs::canonicalize(PathBuf::from(path))
        .map_err(|error| format!("Unable to resolve PDF path: {error}"))?;
    if !pdf.starts_with(&root)
        || pdf
            .extension()
            .and_then(|value| value.to_str())
            .map(str::to_ascii_lowercase)
            != Some("pdf".to_string())
    {
        return Err("Only PDFs inside the active Workspace can be opened externally".to_string());
    }
    std::process::Command::new("explorer")
        .arg(pdf)
        .spawn()
        .map_err(|error| format!("Unable to open the system PDF application: {error}"))?;
    Ok(true)
}

fn thread_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Thread> {
    Ok(Thread {
        id: row.get(0)?,
        revision_id: row.get(1)?,
        kind: "global".to_string(),
        title: row.get(2)?,
        status: row.get(3)?,
        active_message_id: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn query_threads(conn: &Connection, revision_id: &str, status: &str) -> AppResult<Vec<Thread>> {
    let mut statement = conn
        .prepare(
            "SELECT d.id, d.revision_id, d.title, d.status, h.message_id,
                    d.created_at, d.updated_at
             FROM discussions d
             LEFT JOIN discussion_heads h ON h.discussion_id = d.id
             WHERE d.revision_id = ?1 AND d.status = ?2
             ORDER BY d.updated_at DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![revision_id, status], thread_from_row)
        .map_err(|error| error.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())
}

fn load_thread(conn: &Connection, thread_id: &str) -> AppResult<Thread> {
    conn.query_row(
        "SELECT d.id, d.revision_id, d.title, d.status, h.message_id,
                d.created_at, d.updated_at
         FROM discussions d
         LEFT JOIN discussion_heads h ON h.discussion_id = d.id
         WHERE d.id = ?1",
        params![thread_id],
        thread_from_row,
    )
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_threads(state: State<'_, AppState>, revision_id: String) -> AppResult<Vec<Thread>> {
    let root = active_root(&state)?;
    let conn = open_db(&root)?;
    query_threads(&conn, &revision_id, "active")
}

#[tauri::command]
fn list_archived_threads(
    state: State<'_, AppState>,
    revision_id: String,
) -> AppResult<Vec<Thread>> {
    let root = active_root(&state)?;
    let conn = open_db(&root)?;
    query_threads(&conn, &revision_id, "archived")
}

#[tauri::command]
fn create_thread(
    state: State<'_, AppState>,
    revision_id: String,
    title: Option<String>,
    _kind: Option<String>,
) -> AppResult<Thread> {
    let root = active_root(&state)?;
    let conn = open_db(&root)?;
    let paper_id: String = conn
        .query_row(
            "SELECT paper_id FROM document_revisions WHERE id = ?1",
            params![revision_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let id = Uuid::new_v4().to_string();
    let title = title.unwrap_or_else(|| "Main discussion".to_string());
    let timestamp = now();
    conn.execute_batch("BEGIN IMMEDIATE;")
        .map_err(|error| error.to_string())?;
    let published = conn
        .execute(
            "INSERT INTO discussions(
               id, paper_id, revision_id, title, status, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?5)",
            params![id, paper_id, revision_id, title, timestamp],
        )
        .and_then(|_| {
            conn.execute(
                "INSERT INTO discussion_heads(discussion_id, message_id, updated_at)
                 VALUES (?1, NULL, ?2)",
                params![id, timestamp],
            )
        });
    if let Err(error) = published {
        let _ = conn.execute_batch("ROLLBACK;");
        return Err(error.to_string());
    }
    conn.execute_batch("COMMIT;")
        .map_err(|error| error.to_string())?;
    Ok(Thread {
        id,
        revision_id,
        kind: "global".to_string(),
        title,
        status: "active".to_string(),
        active_message_id: None,
        created_at: timestamp.clone(),
        updated_at: timestamp,
    })
}

#[tauri::command]
fn list_messages(state: State<'_, AppState>, thread_id: String) -> AppResult<Vec<Message>> {
    let root = active_root(&state)?;
    let conn = open_db(&root)?;
    let mut statement = conn
        .prepare(
            "SELECT m.id, m.discussion_id, m.parent_id, m.role, m.content,
                    m.citations_json, m.status, m.created_at,
                    u.id, u.provider, u.model, u.context_epoch, u.input_tokens,
                    u.cached_input_tokens, u.uncached_input_tokens, u.output_tokens,
                    u.reasoning_tokens, u.latency_ms, u.estimated_cost,
                    u.file_reuse, u.session_resume, u.paper_root_branch,
                    mc.block_quotes_json
             FROM messages m
             LEFT JOIN usage_receipts u ON u.operation_id = m.id
             LEFT JOIN message_contexts mc ON mc.message_id = m.id
             WHERE m.discussion_id = ?1 AND m.status != 'failed'
             ORDER BY m.created_at ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![thread_id], |row| {
            let citations_raw: String = row.get(5)?;
            let citations =
                serde_json::from_str::<Vec<Citation>>(&citations_raw).unwrap_or_default();
            let block_quotes = row
                .get::<_, Option<String>>(22)?
                .and_then(|raw| serde_json::from_str::<Vec<BlockQuoteSnapshot>>(&raw).ok())
                .unwrap_or_default();
            let usage = if row.get::<_, Option<String>>(8)?.is_some() {
                let input: Option<i64> = row.get(12)?;
                let cached: Option<i64> = row.get(13)?;
                Some(UsageReceipt {
                    input_tokens: input,
                    cached_input_tokens: cached,
                    uncached_input_tokens: row.get(14)?,
                    output_tokens: row.get(15)?,
                    reasoning_tokens: row.get(16)?,
                    cache_hit_rate: match (input, cached) {
                        (Some(input), Some(cached)) if input > 0 => {
                            Some(cached as f64 / input as f64)
                        }
                        _ => None,
                    },
                    latency_ms: row.get(17)?,
                    estimated_cost: row.get(18)?,
                    file_reuse: row.get::<_, Option<i64>>(19)?.map(|value| value != 0),
                    session_resume: row.get::<_, Option<i64>>(20)?.map(|value| value != 0),
                    paper_root_branch: row.get::<_, Option<i64>>(21)?.map(|value| value != 0),
                    context_epoch: row.get(11)?,
                    provider: row.get(9)?,
                    model: row.get(10)?,
                })
            } else {
                None
            };
            Ok(Message {
                id: row.get(0)?,
                thread_id: row.get(1)?,
                parent_id: row.get(2)?,
                role: row.get(3)?,
                content: row.get(4)?,
                citations,
                block_quotes,
                status: row.get(6)?,
                created_at: row.get(7)?,
                usage,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(|error| error.to_string())
}

fn receipt_from_envelope(envelope: UsageEnvelope) -> UsageReceipt {
    UsageReceipt {
        cache_hit_rate: envelope.cache_hit_rate(),
        input_tokens: envelope.input_tokens,
        cached_input_tokens: envelope.cached_input_tokens,
        uncached_input_tokens: envelope.uncached_input_tokens,
        output_tokens: envelope.output_tokens,
        reasoning_tokens: envelope.reasoning_tokens,
        latency_ms: envelope.latency_ms,
        estimated_cost: envelope.estimated_cost,
        file_reuse: envelope.file_reuse,
        session_resume: envelope.session_resume,
        paper_root_branch: envelope.paper_root_branch,
        context_epoch: envelope.context_epoch,
        provider: envelope.provider,
        model: envelope.model,
    }
}
fn citations_for_answer(
    revision_id: &str,
    text: &str,
    page_count: Option<i64>,
    allowed_blocks: &[BlockQuoteSnapshot],
) -> Vec<Citation> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (idx, _) in text.match_indices("[p.") {
        let tail = text[idx + 3..].trim_start();
        let digits: String = tail.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(page) = digits.parse::<i64>() {
            if page < 1 || page_count.is_some_and(|maximum| page > maximum) {
                continue;
            }
            if seen.insert(format!("page:{page}")) {
                out.push(Citation {
                    revision_id: revision_id.to_string(),
                    page: Some(page),
                    region: None,
                    region_hash: None,
                    excerpt: None,
                    confidence: Some("medium".to_string()),
                });
            }
        }
    }
    for (idx, _) in text.match_indices("[block:") {
        let tail = text[idx + 7..].trim_start();
        let Some(end) = tail.find(']') else {
            continue;
        };
        let block_id = tail[..end].trim();
        let Some(block) = allowed_blocks
            .iter()
            .find(|candidate| candidate.block_id == block_id)
        else {
            continue;
        };
        if !seen.insert(format!("block:{}", block.block_id)) {
            continue;
        }
        out.push(Citation {
            revision_id: block.revision_id.clone(),
            page: Some(block.page_number),
            region: Some(Region {
                x: block.bbox[0] as f32,
                y: block.bbox[1] as f32,
                width: (block.bbox[2] - block.bbox[0]) as f32,
                height: (block.bbox[3] - block.bbox[1]) as f32,
            }),
            region_hash: Some(block.content_digest.clone()),
            excerpt: Some(block.text_content.clone()),
            confidence: Some("high".to_string()),
        });
    }
    out
}

#[derive(Debug, Clone)]
struct DiscussionPathMessage {
    id: String,
    role: String,
    status: String,
    content: String,
    block_quotes: Vec<BlockQuoteSnapshot>,
}

fn discussion_path(
    connection: &Connection,
    discussion_id: &str,
    head_id: Option<&str>,
) -> AppResult<Vec<DiscussionPathMessage>> {
    let mut path = Vec::new();
    let mut cursor = head_id.map(str::to_string);
    let mut visited = std::collections::HashSet::new();
    while let Some(message_id) = cursor {
        if !visited.insert(message_id.clone()) || visited.len() > 1000 {
            return Err("Discussion path is cyclic or unreasonably deep".to_string());
        }
        let (parent_id, role, content, block_quotes_json, status) = connection
            .query_row(
                "SELECT m.parent_id, m.role, m.content,
                        COALESCE(mc.block_quotes_json, '[]'), m.status
                 FROM messages m
                 LEFT JOIN message_contexts mc ON mc.message_id = m.id
                 WHERE m.id = ?1 AND m.discussion_id = ?2",
                params![message_id, discussion_id],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                    ))
                },
            )
            .optional()
            .map_err(|error| error.to_string())?
            .ok_or_else(|| "Discussion head does not belong to this Discussion".to_string())?;
        path.push(DiscussionPathMessage {
            id: message_id,
            role,
            status,
            content,
            block_quotes: serde_json::from_str(&block_quotes_json).unwrap_or_default(),
        });
        cursor = parent_id;
    }
    path.reverse();
    Ok(path)
}

fn load_block_quote_snapshots(
    connection: &Connection,
    revision_id: &str,
    block_ids: &[String],
) -> AppResult<Vec<BlockQuoteSnapshot>> {
    if block_ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut normalized = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for block_id in block_ids {
        let block_id = block_id.trim();
        if block_id.is_empty() {
            return Err("Block references cannot contain an empty ID".to_string());
        }
        if seen.insert(block_id.to_string()) {
            normalized.push(block_id.to_string());
        }
    }
    if normalized.len() > 32 {
        return Err("A Discussion message can quote at most 32 OCR Blocks".to_string());
    }
    let ocr_revision_id = connection
        .query_row(
            "SELECT id FROM ocr_revisions
             WHERE revision_id = ?1 AND status = 'ready'
             ORDER BY published_at DESC LIMIT 1",
            params![revision_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "Run OCR before quoting Blocks in a Discussion".to_string())?;
    let mut statement = connection
        .prepare(
            "SELECT pg.page_number, b.block_index, b.block_type, b.text_content,
                    b.content_digest, b.x0, b.y0, b.x1, b.y1
             FROM ocr_blocks b
             JOIN ocr_pages pg ON pg.id = b.ocr_page_id
             JOIN ocr_revisions o ON o.id = pg.ocr_revision_id
             WHERE b.id = ?1 AND o.id = ?2 AND o.revision_id = ?3
               AND o.status = 'ready'",
        )
        .map_err(|error| error.to_string())?;
    normalized
        .into_iter()
        .map(|block_id| {
            statement
                .query_row(params![block_id, ocr_revision_id, revision_id], |row| {
                    Ok(BlockQuoteSnapshot {
                        revision_id: revision_id.to_string(),
                        ocr_revision_id: ocr_revision_id.clone(),
                        block_id: block_id.clone(),
                        page_number: row.get(0)?,
                        block_index: row.get(1)?,
                        block_type: row.get(2)?,
                        text_content: row.get(3)?,
                        content_digest: row.get(4)?,
                        bbox: [row.get(5)?, row.get(6)?, row.get(7)?, row.get(8)?],
                    })
                })
                .optional()
                .map_err(|error| error.to_string())?
                .ok_or_else(|| {
                    "A quoted Block is stale or does not belong to the current published OCR"
                        .to_string()
                })
        })
        .collect()
}

fn block_quotes_for_prompt(label: &str, block_quotes: &[BlockQuoteSnapshot]) -> String {
    if block_quotes.is_empty() {
        return String::new();
    }
    format!(
        "\n{label}:\n{}\n",
        serde_json::to_string_pretty(block_quotes).unwrap_or_else(|_| "[]".to_string())
    )
}

fn discussion_needs_compaction(
    history: &[DiscussionPathMessage],
    page_count: Option<i64>,
    input_token_limit: Option<i64>,
) -> bool {
    if history.len() < 8 {
        return false;
    }
    let Some(limit) = input_token_limit.filter(|limit| *limit > 0) else {
        return false;
    };
    let history_tokens = history.iter().fold(0_i64, |total, message| {
        let content_characters = message.content.chars().count();
        let quote_characters = message
            .block_quotes
            .iter()
            .map(|quote| quote.text_content.chars().count())
            .sum::<usize>();
        let characters = content_characters
            .saturating_add(quote_characters)
            .min(i64::MAX as usize) as i64;
        total.saturating_add(characters.max(1))
    });
    let pdf_tokens = page_count.unwrap_or(0).max(0).saturating_mul(300);
    let reserved_tokens = 4_096_i64;
    history_tokens
        .saturating_add(pdf_tokens)
        .saturating_add(reserved_tokens)
        > limit.saturating_mul(4) / 5
}

#[derive(Clone)]
struct DiscussionGeneration {
    root: PathBuf,
    revision: RevisionRecord,
    thread_id: String,
    assistant_id: String,
    provider: String,
    provider_instance_id: String,
    provider_route_id: String,
    model: String,
    context_epoch: String,
    adapter: Arc<dyn PaperModelPort>,
    cached_file_id: Option<String>,
    previous_local_provider_id: Option<String>,
    previous_remote_provider_id: Option<String>,
    system_instruction: String,
    compaction_instruction: String,
    incremental_input: String,
    recovery_input: String,
    history: Vec<DiscussionPathMessage>,
    current_block_quotes: Vec<BlockQuoteSnapshot>,
    needs_compaction: bool,
    cancellation: CancellationFlag,
}

fn emit_generation_event(
    app: &tauri::AppHandle,
    message_id: &str,
    delta: Option<String>,
    status: Option<&str>,
) {
    let _ = app.emit(
        "read-event",
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "generation".to_string(),
            entity_id: Some(message_id.to_string()),
            delta,
            status: status.map(str::to_string),
        },
    );
}

fn persist_generation_snapshot(
    root: &Path,
    assistant_id: &str,
    content: &str,
) -> Result<(), ProviderError> {
    let connection = open_db(root).map_err(ProviderError::local_state)?;
    connection
        .execute(
            "UPDATE messages
             SET content = ?1, updated_at = ?2
             WHERE id = ?3 AND status = 'streaming'",
            params![content, now(), assistant_id],
        )
        .map_err(|error| ProviderError::local_state(error.to_string()))?;
    Ok(())
}

async fn stream_discussion_once(
    app: &tauri::AppHandle,
    root: &Path,
    assistant_id: &str,
    adapter: &dyn PaperModelPort,
    request: PaperInteractionRequest,
    cancellation: CancellationFlag,
    partial: &mut String,
) -> Result<PaperInteractionOutcome, ProviderError> {
    let (deltas, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut interaction = Box::pin(adapter.interact_stream(PaperStreamRequest {
        interaction: request,
        cancellation: cancellation.clone(),
        deltas,
    }));
    let mut checkpoint = tokio::time::interval_at(
        tokio::time::Instant::now() + Duration::from_millis(250),
        Duration::from_millis(250),
    );
    checkpoint.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut dirty = false;
    let mut channel_open = true;

    loop {
        tokio::select! {
            delta = receiver.recv(), if channel_open => {
                match delta {
                    Some(delta) => {
                        partial.push_str(&delta);
                        dirty = true;
                        emit_generation_event(
                            app,
                            assistant_id,
                            Some(delta),
                            Some("streaming"),
                        );
                    }
                    None => channel_open = false,
                }
            }
            _ = checkpoint.tick() => {
                if dirty {
                    persist_generation_snapshot(root, assistant_id, partial)?;
                    dirty = false;
                }
                if cancellation.is_cancelled() {
                    return Err(ProviderError::cancelled());
                }
            }
            outcome = &mut interaction => {
                while let Ok(delta) = receiver.try_recv() {
                    partial.push_str(&delta);
                    emit_generation_event(
                        app,
                        assistant_id,
                        Some(delta),
                        Some("streaming"),
                    );
                }
                let outcome = outcome?;
                *partial = outcome.text.clone();
                persist_generation_snapshot(root, assistant_id, partial)?;
                return Ok(outcome);
            }
        }
    }
}

fn finish_discussion_with_error(
    app: &tauri::AppHandle,
    generation: &DiscussionGeneration,
    partial: &str,
    error: &ProviderError,
) {
    if error.kind == ProviderErrorKind::Cancelled {
        if let Ok(connection) = open_db(&generation.root) {
            let timestamp = now();
            let _ = connection.execute_batch("BEGIN IMMEDIATE;");
            let result = connection
                .execute(
                    "UPDATE messages
                     SET content = ?1, status = 'cancelled', updated_at = ?2
                     WHERE id = ?3 AND status = 'streaming'",
                    params![partial, timestamp, generation.assistant_id],
                )
                .and_then(|_| {
                    connection.execute(
                        "UPDATE discussions SET updated_at = ?1 WHERE id = ?2",
                        params![timestamp, generation.thread_id],
                    )
                });
            if result.is_ok() {
                let _ = connection.execute_batch("COMMIT;");
            } else {
                let _ = connection.execute_batch("ROLLBACK;");
            }
        }
        emit_generation_event(app, &generation.assistant_id, None, Some("cancelled"));
        return;
    }

    if let Ok(connection) = open_db(&generation.root) {
        let timestamp = now();
        let parent_id: Option<String> = connection
            .query_row(
                "SELECT parent_id FROM messages WHERE id = ?1",
                params![generation.assistant_id],
                |row| row.get::<_, Option<String>>(0),
            )
            .ok()
            .flatten();
        let _ = connection.execute_batch("BEGIN IMMEDIATE;");
        let result = connection
            .execute(
                "DELETE FROM messages WHERE id = ?1 AND status = 'streaming'",
                params![generation.assistant_id],
            )
            .and_then(|_| {
                connection.execute(
                    "UPDATE discussion_heads SET message_id = ?1, updated_at = ?2
                     WHERE discussion_id = ?3",
                    params![parent_id, timestamp, generation.thread_id],
                )
            })
            .and_then(|_| {
                connection.execute(
                    "UPDATE discussions SET updated_at = ?1 WHERE id = ?2",
                    params![timestamp, generation.thread_id],
                )
            });
        if result.is_ok() {
            let _ = connection.execute_batch("COMMIT;");
        } else {
            let _ = connection.execute_batch("ROLLBACK;");
        }
    }
    emit_generation_event(
        app,
        &generation.assistant_id,
        Some(error.message.clone()),
        Some("failed"),
    );
}

fn publish_discussion_outcome(
    app: &tauri::AppHandle,
    generation: &DiscussionGeneration,
    outcome: PaperInteractionOutcome,
) -> AppResult<()> {
    let answer = outcome.text;
    let remote_provider_node_id = outcome.provider_node_id;
    let provider_file_id = outcome.provider_file_id;
    let receipt = receipt_from_envelope(outcome.receipt);
    let created_root_branch = receipt.paper_root_branch == Some(true);
    let provider_parent_id = if created_root_branch {
        None
    } else {
        generation.previous_local_provider_id.clone()
    };
    let root_remote_node_id = created_root_branch.then_some(remote_provider_node_id.clone());
    let citations = citations_for_answer(
        &generation.revision.id,
        &answer,
        generation.revision.page_count,
        &generation.current_block_quotes,
    );
    let citations_json = serde_json::to_string(&citations).map_err(|error| error.to_string())?;
    let connection = open_db(&generation.root)?;
    let context_root_id = connection
        .query_row(
            "SELECT id FROM context_roots
             WHERE revision_id = ?1 AND provider_route_id = ?2 AND model = ?3
               AND context_epoch = ?4 AND invalidated_at IS NULL LIMIT 1",
            params![
                generation.revision.id,
                generation.provider_route_id,
                generation.model,
                generation.context_epoch
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let local_provider_node_id = Uuid::new_v4().to_string();
    let timestamp = now();
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|error| error.to_string())?;
    let published = connection
        .execute(
            "INSERT INTO context_roots(
               id, revision_id, provider, model, context_epoch, provider_route_id,
               provider_file_id, provider_node_id, state, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active', ?9)
             ON CONFLICT(revision_id, provider, model, context_epoch) DO UPDATE SET
               provider_file_id = COALESCE(excluded.provider_file_id, context_roots.provider_file_id),
               provider_node_id = COALESCE(excluded.provider_node_id, context_roots.provider_node_id),
               state = 'active', invalidated_at = NULL",
            params![
                context_root_id,
                generation.revision.id,
                generation.provider,
                generation.model,
                generation.context_epoch,
                generation.provider_route_id,
                provider_file_id,
                root_remote_node_id,
                timestamp
            ],
        )
        .and_then(|_| {
            connection.execute(
                "INSERT INTO provider_nodes(
                   id, provider, model, context_epoch, provider_route_id, provider_node_id,
                   parent_id, state, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'complete', ?8)",
                params![
                    local_provider_node_id,
                    generation.provider,
                    generation.model,
                    generation.context_epoch,
                    generation.provider_route_id,
                    remote_provider_node_id,
                    provider_parent_id,
                    timestamp
                ],
            )
        })
        .and_then(|_| {
            connection.execute(
                "UPDATE messages
                 SET content = ?1, status = 'complete', citations_json = ?2,
                     provider_node_id = ?3, updated_at = ?4
                 WHERE id = ?5 AND status = 'streaming'",
                params![
                    answer,
                    citations_json,
                    local_provider_node_id,
                    timestamp,
                    generation.assistant_id
                ],
            )
        })
        .and_then(|changed| {
            if changed != 1 {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
            connection.execute(
                "INSERT INTO usage_receipts(
                   id, operation_id, provider, model, context_epoch, provider_route_id,
                   input_tokens, cached_input_tokens, uncached_input_tokens,
                   output_tokens, reasoning_tokens, latency_ms, estimated_cost,
                   file_reuse, session_resume, paper_root_branch, created_at
                 ) VALUES (
                   ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                   ?13, ?14, ?15, ?16, ?17
                 )",
                params![
                    Uuid::new_v4().to_string(),
                    generation.assistant_id,
                    receipt.provider,
                    receipt.model,
                    receipt
                        .context_epoch
                        .as_deref()
                        .unwrap_or(&generation.context_epoch),
                    generation.provider_route_id,
                    receipt.input_tokens,
                    receipt.cached_input_tokens,
                    receipt.uncached_input_tokens,
                    receipt.output_tokens,
                    receipt.reasoning_tokens,
                    receipt.latency_ms,
                    receipt.estimated_cost,
                    receipt.file_reuse.map(i64::from),
                    receipt.session_resume.map(i64::from),
                    receipt.paper_root_branch.map(i64::from),
                    timestamp
                ],
            )
        })
        .and_then(|_| {
            connection.execute(
                "UPDATE discussion_heads SET message_id = ?1, updated_at = ?2
                 WHERE discussion_id = ?3",
                params![
                    generation.assistant_id,
                    timestamp,
                    generation.thread_id
                ],
            )
        })
        .and_then(|_| {
            connection.execute(
                "UPDATE discussions
                 SET context_root_id = ?1, updated_at = ?2
                 WHERE id = ?3",
                params![context_root_id, timestamp, generation.thread_id],
            )
        });
    if let Err(error) = published {
        let _ = connection.execute_batch("ROLLBACK;");
        return Err(error.to_string());
    }
    connection
        .execute_batch("COMMIT;")
        .map_err(|error| error.to_string())?;
    let _ = connection.execute(
        "DELETE FROM messages
         WHERE discussion_id = ?1
           AND role = 'assistant'
           AND status = 'failed'
           AND id != ?2
           AND parent_id = (SELECT parent_id FROM messages WHERE id = ?2)",
        params![generation.thread_id, generation.assistant_id],
    );
    emit_generation_event(app, &generation.assistant_id, None, Some("complete"));
    Ok(())
}

#[derive(Debug)]
struct ContextCompactionSeed {
    local_provider_node_id: String,
    remote_provider_node_id: String,
    provider_file_id: String,
    summary_markdown: String,
    recovery_context: Value,
}

fn context_compaction_schema() -> Value {
    json!({
        "name": "context_compaction",
        "strict": true,
        "schema": {
            "type": "object",
            "additionalProperties": false,
            "required": [
                "summaryMarkdown",
                "retainedClaims",
                "unresolvedQuestions"
            ],
            "properties": {
                "summaryMarkdown": {"type": "string", "minLength": 1},
                "retainedClaims": {
                    "type": "array",
                    "items": {"type": "string", "minLength": 1}
                },
                "unresolvedQuestions": {
                    "type": "array",
                    "items": {"type": "string", "minLength": 1}
                }
            }
        }
    })
}

fn parse_context_compaction(text: &str) -> Result<Value, ProviderError> {
    let start = text
        .find('{')
        .ok_or_else(|| ProviderError::invalid("Context compaction is not a JSON object"))?;
    let end = text
        .rfind('}')
        .ok_or_else(|| ProviderError::invalid("Context compaction JSON is incomplete"))?;
    let value: Value = serde_json::from_str(&text[start..=end])
        .map_err(|error| ProviderError::invalid(format!("Invalid context compaction: {error}")))?;
    let summary = value
        .get("summaryMarkdown")
        .and_then(Value::as_str)
        .filter(|summary| !summary.trim().is_empty());
    let claims = value.get("retainedClaims").and_then(Value::as_array);
    let questions = value.get("unresolvedQuestions").and_then(Value::as_array);
    if value.as_object().is_none_or(|object| object.len() != 3)
        || summary.is_none()
        || claims.is_none()
        || questions.is_none()
        || claims
            .into_iter()
            .flatten()
            .chain(questions.into_iter().flatten())
            .any(|entry| entry.as_str().is_none_or(|text| text.trim().is_empty()))
    {
        return Err(ProviderError::invalid(
            "Context compaction failed its typed schema",
        ));
    }
    Ok(value)
}

fn discussion_recovery_from_compaction(
    title: &str,
    content: &Value,
    current_input: &str,
) -> String {
    let memory = json!({
        "summaryMarkdown": content["summaryMarkdown"],
        "retainedClaims": content["retainedClaims"],
        "unresolvedQuestions": content["unresolvedQuestions"],
    });
    format!(
        "文档标题：{title}\n\n当前分支的压缩记录（有损历史，不是已验证的文档事实；历史引用不增加本轮块白名单）：\n{memory}\n\n本轮请求：\n{current_input}"
    )
}

fn discussion_turn_input(
    question: &str,
    block_quotes: &[BlockQuoteSnapshot],
    page: Option<i64>,
    regenerate: bool,
) -> String {
    let mut input = String::new();
    if let Some(page) = page {
        input.push_str(&format!(
            "读者位置：PDF 物理页 {page}（仅为位置线索，不是证据）\n"
        ));
    }
    if regenerate {
        input.push_str("本次重新生成下列用户问题的答复，不虚构新的用户问题；只以本次提供的块作为可点击块引用白名单。\n");
    }
    input.push_str(&block_quotes_for_prompt(
        "本轮允许引用的 OCR 块；只有以下 ID 可以使用 [block: BLOCK_ID] 标记",
        block_quotes,
    ));
    if block_quotes.is_empty() {
        input.push_str("本轮没有允许引用的 OCR 块，不得生成 [block: BLOCK_ID] 标记。\n");
    }
    input.push_str(&format!("USER: {question}"));
    input
}

fn context_compaction_input(generation: &DiscussionGeneration) -> String {
    context_compaction_source_input(&generation.revision.title, &generation.history)
}

fn context_compaction_source_input(title: &str, history: &[DiscussionPathMessage]) -> String {
    json!({
        "task": "只压缩 sourceMessages 指定的当前分支，保存目标、认识的条件与来源、修正和未决事项；不要回答新问题或新增结论。消息 status 表示记录是否完成，不表示内容已验证。",
        "paperTitle": title,
        "sourceMessages": history
            .iter()
            .map(|message| json!({
                "id": message.id,
                "role": message.role,
                "status": message.status,
                "content": message.content,
                "blockQuotes": message.block_quotes
            }))
            .collect::<Vec<_>>()
    })
    .to_string()
}

fn publish_context_compaction(
    app: &tauri::AppHandle,
    generation: &DiscussionGeneration,
    outcome: PaperInteractionOutcome,
    mut content: Value,
) -> AppResult<ContextCompactionSeed> {
    let remote_provider_node_id = outcome.provider_node_id;
    let provider_file_id = outcome.provider_file_id;
    let receipt = receipt_from_envelope(outcome.receipt);
    let summary_markdown = content
        .get("summaryMarkdown")
        .and_then(Value::as_str)
        .ok_or_else(|| "Context compaction summary is missing".to_string())?
        .to_string();
    let recovery_context = content.clone();
    let source_messages = generation
        .history
        .iter()
        .map(|message| {
            let block_quotes = serde_json::to_vec(&message.block_quotes).unwrap_or_default();
            json!({
                "messageId": message.id,
                "role": message.role,
                "status": message.status,
                "contentSha256": sha256_bytes(message.content.as_bytes()),
                "blockQuotesSha256": sha256_bytes(&block_quotes)
            })
        })
        .collect::<Vec<_>>();
    if let Some(object) = content.as_object_mut() {
        object.insert(
            "sourceMessageIds".to_string(),
            Value::Array(
                generation
                    .history
                    .iter()
                    .map(|message| Value::String(message.id.clone()))
                    .collect(),
            ),
        );
        object.insert(
            "sourceMessageCount".to_string(),
            json!(generation.history.len()),
        );
    }
    let dependency_snapshot = json!({
        "revisionId": generation.revision.id,
        "contextEpoch": generation.context_epoch,
        "model": generation.model,
        "sourceMessages": source_messages
    });

    let connection = open_db(&generation.root)?;
    let paper_id: String = connection
        .query_row(
            "SELECT paper_id FROM document_revisions WHERE id = ?1",
            params![generation.revision.id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let version: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(version), 0) + 1
             FROM artifacts
             WHERE paper_id = ?1 AND kind = 'context_compaction' AND object_key = ?2",
            params![paper_id, generation.thread_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let previous_head: Option<String> = connection
        .query_row(
            "SELECT artifact_id FROM artifact_heads
             WHERE paper_id = ?1 AND kind = 'context_compaction' AND object_key = ?2",
            params![paper_id, generation.thread_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let context_root_id = connection
        .query_row(
            "SELECT id FROM context_roots
             WHERE revision_id = ?1 AND provider_route_id = ?2 AND model = ?3
               AND context_epoch = ?4 AND invalidated_at IS NULL LIMIT 1",
            params![
                generation.revision.id,
                generation.provider_route_id,
                generation.model,
                generation.context_epoch
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let local_provider_node_id = Uuid::new_v4().to_string();
    let artifact_id = Uuid::new_v4().to_string();
    let timestamp = now();
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|error| error.to_string())?;
    let published = connection
        .execute(
            "INSERT INTO context_roots(
               id, revision_id, provider, model, context_epoch, provider_route_id,
               provider_file_id, provider_node_id, state, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, 'active', ?8)
             ON CONFLICT(revision_id, provider, model, context_epoch) DO UPDATE SET
               provider_file_id = excluded.provider_file_id,
               state = 'active', invalidated_at = NULL",
            params![
                context_root_id,
                generation.revision.id,
                generation.provider,
                generation.model,
                generation.context_epoch,
                generation.provider_route_id,
                provider_file_id,
                timestamp
            ],
        )
        .and_then(|_| {
            connection.execute(
                "INSERT INTO provider_nodes(
                   id, provider, model, context_epoch, provider_route_id, provider_node_id,
                   parent_id, state, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, 'complete', ?7)",
                params![
                    local_provider_node_id,
                    generation.provider,
                    generation.model,
                    generation.context_epoch,
                    generation.provider_route_id,
                    remote_provider_node_id,
                    timestamp
                ],
            )
        })
        .and_then(|_| {
            connection.execute(
                "INSERT INTO artifacts(
                   id, paper_id, revision_id, ocr_revision_id, kind, object_key,
                   version, status, content_json, evidence_json,
                   dependency_snapshot_json, provider_node_id, created_at
                 ) VALUES (
                   ?1, ?2, ?3, NULL, 'context_compaction', ?4,
                   ?5, 'ready', ?6, '[]', ?7, ?8, ?9
                 )",
                params![
                    artifact_id,
                    paper_id,
                    generation.revision.id,
                    generation.thread_id,
                    version,
                    content.to_string(),
                    dependency_snapshot.to_string(),
                    local_provider_node_id,
                    timestamp
                ],
            )
        })
        .and_then(|_| {
            connection.execute(
                "INSERT INTO artifact_heads(
                   paper_id, kind, object_key, artifact_id, updated_at
                 ) VALUES (?1, 'context_compaction', ?2, ?3, ?4)
                 ON CONFLICT(paper_id, kind, object_key) DO UPDATE SET
                   artifact_id = excluded.artifact_id,
                   updated_at = excluded.updated_at",
                params![paper_id, generation.thread_id, artifact_id, timestamp],
            )
        })
        .and_then(|_| {
            if let Some(previous) = &previous_head {
                connection.execute(
                    "UPDATE artifacts SET superseded_at = ?1 WHERE id = ?2",
                    params![timestamp, previous],
                )
            } else {
                Ok(0)
            }
        })
        .and_then(|_| {
            connection.execute(
                "INSERT INTO usage_receipts(
                   id, operation_id, provider, model, context_epoch, provider_route_id,
                   input_tokens, cached_input_tokens, uncached_input_tokens,
                   output_tokens, reasoning_tokens, latency_ms, estimated_cost,
                   file_reuse, session_resume, paper_root_branch, created_at
                 ) VALUES (
                   ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
                   ?13, ?14, ?15, ?16, ?17
                 )",
                params![
                    Uuid::new_v4().to_string(),
                    artifact_id,
                    receipt.provider,
                    receipt.model,
                    receipt
                        .context_epoch
                        .as_deref()
                        .unwrap_or(&generation.context_epoch),
                    generation.provider_route_id,
                    receipt.input_tokens,
                    receipt.cached_input_tokens,
                    receipt.uncached_input_tokens,
                    receipt.output_tokens,
                    receipt.reasoning_tokens,
                    receipt.latency_ms,
                    receipt.estimated_cost,
                    receipt.file_reuse.map(i64::from),
                    receipt.session_resume.map(i64::from),
                    receipt.paper_root_branch.map(i64::from),
                    timestamp
                ],
            )
        });
    if let Err(error) = published {
        let _ = connection.execute_batch("ROLLBACK;");
        return Err(error.to_string());
    }
    connection
        .execute_batch("COMMIT;")
        .map_err(|error| error.to_string())?;
    let _ = app.emit(
        "read-event",
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "paper".to_string(),
            entity_id: Some(generation.revision.id.clone()),
            delta: None,
            status: None,
        },
    );
    Ok(ContextCompactionSeed {
        local_provider_node_id,
        remote_provider_node_id,
        provider_file_id,
        summary_markdown,
        recovery_context,
    })
}

async fn compact_discussion(
    app: &tauri::AppHandle,
    generation: &DiscussionGeneration,
    adapter: &dyn PaperModelPort,
) -> Result<ContextCompactionSeed, ProviderError> {
    generation.cancellation.check()?;
    let compaction_instruction = &generation.compaction_instruction;
    let build_request = |remote_file_id: Option<String>| PaperInteractionRequest {
        model: generation.model.clone(),
        context_epoch: generation.context_epoch.clone(),
        pdf_path: generation.revision.pdf_path.clone(),
        display_name: generation.revision.title.clone(),
        remote_file_id,
        previous_interaction_id: None,
        system_instruction: compaction_instruction.clone(),
        user_input: context_compaction_input(generation),
        response_schema: Some(context_compaction_schema()),
        inline_images: Vec::new(),
        kind: PaperInteractionKind::ContextCompaction,
    };
    let run = |request| {
        let (deltas, _receiver) = tokio::sync::mpsc::unbounded_channel();
        adapter.interact_stream(PaperStreamRequest {
            interaction: request,
            cancellation: generation.cancellation.clone(),
            deltas,
        })
    };
    let first = run(build_request(generation.cached_file_id.clone())).await;
    let outcome = match first {
        Err(error)
            if error.kind == ProviderErrorKind::StaleRemoteResource
                && generation.cached_file_id.is_some()
                && !generation.cancellation.is_cancelled() =>
        {
            run(build_request(None)).await?
        }
        result => result?,
    };
    generation.cancellation.check()?;
    let content = parse_context_compaction(&outcome.text)?;
    publish_context_compaction(app, generation, outcome, content)
        .map_err(ProviderError::local_state)
}
fn remove_discussion_cancellation(app: &tauri::AppHandle, assistant_id: &str) {
    if let Ok(mut cancellations) = app.state::<AppState>().discussion_cancellations.lock() {
        cancellations.remove(assistant_id);
    }
}

async fn run_discussion_generation(app: tauri::AppHandle, mut generation: DiscussionGeneration) {
    let adapter: &dyn PaperModelPort = generation.adapter.as_ref();
    if generation.needs_compaction {
        let _ = app.emit(
            "discussion_compaction_started",
            serde_json::json!({
                "threadId": generation.thread_id,
                "assistantId": generation.assistant_id,
                "revisionId": generation.revision.id,
            }),
        );
        match compact_discussion(&app, &generation, adapter).await {
            Ok(compaction) => {
                let _ = app.emit(
                    "discussion_compaction_finished",
                    serde_json::json!({
                        "threadId": generation.thread_id,
                        "assistantId": generation.assistant_id,
                        "revisionId": generation.revision.id,
                        "summary": compaction.summary_markdown,
                    }),
                );
                generation.cached_file_id = Some(compaction.provider_file_id);
                generation.previous_local_provider_id = Some(compaction.local_provider_node_id);
                generation.previous_remote_provider_id = Some(compaction.remote_provider_node_id);
                generation.recovery_input = discussion_recovery_from_compaction(
                    &generation.revision.title,
                    &compaction.recovery_context,
                    &generation.incremental_input,
                );
            }
            Err(error) => {
                let _ = app.emit(
                    "discussion_compaction_finished",
                    serde_json::json!({
                        "threadId": generation.thread_id,
                        "assistantId": generation.assistant_id,
                        "revisionId": generation.revision.id,
                        "error": error.to_string(),
                    }),
                );
                maybe_invalidate_paper_probe(&app, &generation.provider_instance_id, &error);
                finish_discussion_with_error(&app, &generation, "", &error);
                remove_discussion_cancellation(&app, &generation.assistant_id);
                return;
            }
        }
    }
    let has_previous = generation.previous_remote_provider_id.is_some();
    let input_kind = discussion_user_input(&generation.provider, has_previous);
    let incremental = input_kind == DiscussionUserInput::Incremental;
    let initial_kind = if incremental {
        PaperInteractionKind::Discussion
    } else {
        PaperInteractionKind::Root
    };
    let initial_input = if incremental {
        generation.incremental_input.clone()
    } else {
        generation.recovery_input.clone()
    };
    let previous_interaction_id = if incremental {
        generation.previous_remote_provider_id.clone()
    } else {
        None
    };
    let mut partial = String::new();
    let first = stream_discussion_once(
        &app,
        &generation.root,
        &generation.assistant_id,
        adapter,
        PaperInteractionRequest {
            model: generation.model.clone(),
            context_epoch: generation.context_epoch.clone(),
            pdf_path: generation.revision.pdf_path.clone(),
            display_name: generation.revision.title.clone(),
            remote_file_id: generation.cached_file_id.clone(),
            previous_interaction_id,
            system_instruction: generation.system_instruction.clone(),
            user_input: initial_input,
            response_schema: None,
            inline_images: Vec::new(),
            kind: initial_kind,
        },
        generation.cancellation.clone(),
        &mut partial,
    )
    .await;
    let outcome = match first {
        Err(error)
            if error.kind == ProviderErrorKind::StaleRemoteResource
                && (generation.previous_remote_provider_id.is_some()
                    || generation.cached_file_id.is_some())
                && !generation.cancellation.is_cancelled() =>
        {
            partial.clear();
            let _ = persist_generation_snapshot(&generation.root, &generation.assistant_id, "");
            emit_generation_event(&app, &generation.assistant_id, None, Some("streaming"));
            stream_discussion_once(
                &app,
                &generation.root,
                &generation.assistant_id,
                adapter,
                PaperInteractionRequest {
                    model: generation.model.clone(),
                    context_epoch: generation.context_epoch.clone(),
                    pdf_path: generation.revision.pdf_path.clone(),
                    display_name: generation.revision.title.clone(),
                    remote_file_id: None,
                    previous_interaction_id: None,
                    system_instruction: generation.system_instruction.clone(),
                    user_input: generation.recovery_input.clone(),
                    response_schema: None,
                    inline_images: Vec::new(),
                    kind: PaperInteractionKind::Root,
                },
                generation.cancellation.clone(),
                &mut partial,
            )
            .await
        }
        result => result,
    };

    match outcome {
        Ok(outcome) => {
            if let Err(error) = publish_discussion_outcome(&app, &generation, outcome) {
                finish_discussion_with_error(
                    &app,
                    &generation,
                    &partial,
                    &ProviderError::local_state(error),
                );
            }
        }
        Err(error) => {
            maybe_invalidate_paper_probe(&app, &generation.provider_instance_id, &error);
            finish_discussion_with_error(&app, &generation, &partial, &error);
        }
    }
    remove_discussion_cancellation(&app, &generation.assistant_id);
}

#[tauri::command]
async fn send_chat(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: ChatRequest,
) -> AppResult<ChatTurn> {
    let runtime = current_runtime(&state)?;
    let root = runtime.root.clone();
    let mut connection = open_db(&root)?;
    let mut revision = revision_record(&connection, &request.revision_id)?;
    revision.pdf_path = absolute_pdf(&root, &revision.pdf_path);
    let head: Option<String> = connection
        .query_row(
            "SELECT h.message_id FROM discussions d
             JOIN discussion_heads h ON h.discussion_id = d.id
             WHERE d.id = ?1 AND d.revision_id = ?2 AND d.status = 'active'",
            params![request.thread_id, request.revision_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    let prepared = prepare_chat_turn(&connection, &request, head)?;
    let question = prepared.question.clone();
    let block_quotes = prepared.block_quotes.clone();
    let parent_id = prepared.user_parent_id.clone();
    let history = discussion_path(
        &connection,
        &request.thread_id,
        prepared.history_head_id.as_deref(),
    )?;
    let current = require_current_paper_provider(&app, &state)?;
    let provider = current.provider.clone();
    let model = normalize_paper_model_id(&provider, &current.paper_model)?;
    let translation_model = normalize_paper_model_id(&provider, &current.translation_model)?;
    let route = capture_current_paper_job_route(
        &app,
        &state,
        &current,
        &model,
        Some(&translation_model),
        ModelRole::Paper,
    )?;
    let provider_instance_id = route.frozen().instance_id().as_str().to_string();
    let provider_route_id = route.frozen().route_id().database_value();
    let persisted_route_id = {
        let transaction = connection
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|error| error.to_string())?;
        let route_id = persist_frozen_route(&transaction, route.frozen())
            .map_err(|error| error.to_string())?
            .route_id_database_value()
            .to_string();
        transaction.commit().map_err(|error| error.to_string())?;
        route_id
    };
    if persisted_route_id != provider_route_id {
        return Err(
            "Captured Discussion route did not persist with its exact identity".to_string(),
        );
    }
    let adapter = route.adapter_arc();
    let previous_provider_node: Option<(String, String)> = match parent_id.as_deref() {
        Some(message_id) => connection
            .query_row(
                "SELECT p.id, p.provider_node_id
                 FROM messages m
                 JOIN provider_nodes p ON p.id = m.provider_node_id
                 WHERE m.id = ?1 AND m.discussion_id = ?2
                   AND p.provider_route_id = ?3 AND p.provider_node_id IS NOT NULL",
                params![message_id, request.thread_id, provider_route_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?,
        None => None,
    };
    let in_flight: bool = connection
        .query_row(
            "SELECT EXISTS(
               SELECT 1 FROM messages
               WHERE discussion_id = ?1 AND status = 'streaming'
             )",
            params![request.thread_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if in_flight {
        return Err("This Discussion already has a response in progress".to_string());
    }

    let model_settings = read_model_settings(&app)?;
    let input_token_limit = model_settings
        .instance(&current.instance_id)
        .ok()
        .and_then(|inst| {
            inst.models
                .iter()
                .find(|candidate| candidate.id == model)
                .and_then(|candidate| candidate.input_token_limit)
        });
    let needs_compaction =
        discussion_needs_compaction(&history, revision.page_count, input_token_limit);
    let semantic_epoch = format!("{}:{}", revision.sha256, model);
    let context_epoch = route_scoped_context_epoch(&semantic_epoch, &provider_route_id);
    let cached_file_id: Option<String> = connection
        .query_row(
            "SELECT provider_file_id FROM context_roots
             WHERE revision_id = ?1 AND provider_route_id = ?2 AND model = ?3
               AND context_epoch = ?4 AND invalidated_at IS NULL
             ORDER BY created_at DESC LIMIT 1",
            params![revision.id, provider_route_id, model, context_epoch],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    if cached_file_id.is_none() {
        ensure_long_pdf_acknowledged(&runtime, &revision.id, revision.page_count)?;
    }

    let output_language = resolve_output_language(&app, None)?;
    let system_instruction = load_prompt_for_revision_from_root(
        &app,
        &runtime.root,
        &revision.id,
        PromptSlotId::Discussion,
        Some(&output_language),
    )?;
    let compaction_instruction = load_prompt_for_revision_from_root(
        &app,
        &runtime.root,
        &revision.id,
        PromptSlotId::DiscussionCompaction,
        Some(&output_language),
    )?;
    let system_instruction =
        format!("{system_instruction}\n\nApplication output language: {output_language}.");
    let mut recovery_input = format!("PAPER TITLE: {}\n\nDISCUSSION BRANCH:\n", revision.title);
    if history.is_empty() {
        recovery_input.push_str("(new discussion)\n");
    } else {
        for message in &history {
            recovery_input.push_str(&format!(
                "{} (status={}): {}\n",
                message.role.to_ascii_uppercase(),
                message.status,
                message.content
            ));
            recovery_input.push_str(&block_quotes_for_prompt(
                "IMMUTABLE BLOCK SNAPSHOTS QUOTED BY THIS HISTORICAL MESSAGE",
                &message.block_quotes,
            ));
        }
    }
    let mut incremental_input = discussion_turn_input(
        &question,
        &block_quotes,
        request.page,
        request
            .regenerate_from_id
            .as_deref()
            .is_some_and(|id| !id.trim().is_empty()),
    );
    recovery_input.push_str(&format!("\n{incremental_input}"));
    if let Some(wrapper) = resolve_reader_wrapper_for_revision(&runtime.root, &revision.id) {
        incremental_input =
            reader_context::prepend_reader_context(&incremental_input, Some(&wrapper));
        recovery_input = reader_context::prepend_reader_context(&recovery_input, Some(&wrapper));
    }

    let block_quotes_json =
        serde_json::to_string(&block_quotes).map_err(|error| error.to_string())?;
    let user_id = prepared.user_id.clone();
    let assistant_id = Uuid::new_v4().to_string();
    let timestamp = now();
    connection
        .execute_batch("BEGIN IMMEDIATE;")
        .map_err(|error| error.to_string())?;
    let published = (|| -> rusqlite::Result<usize> {
        if prepared.insert_user {
            connection.execute(
                "INSERT INTO messages(
                   id, discussion_id, parent_id, role, content, status,
                   citations_json, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, 'user', ?4, 'complete', '[]', ?5, ?5)",
                params![user_id, request.thread_id, parent_id, question, timestamp],
            )?;
            connection.execute(
                "INSERT INTO message_contexts(message_id, block_quotes_json)
                 VALUES (?1, ?2)",
                params![user_id, block_quotes_json],
            )?;
        }
        connection.execute(
            "INSERT INTO messages(
               id, discussion_id, parent_id, role, content, status,
               citations_json, created_at, updated_at
             ) VALUES (?1, ?2, ?3, 'assistant', '', 'streaming', '[]', ?4, ?4)",
            params![assistant_id, request.thread_id, user_id, timestamp],
        )?;
        connection.execute(
            "UPDATE discussion_heads SET message_id = ?1, updated_at = ?2
             WHERE discussion_id = ?3",
            params![user_id, timestamp, request.thread_id],
        )?;
        connection.execute(
            "UPDATE discussions SET updated_at = ?1 WHERE id = ?2",
            params![timestamp, request.thread_id],
        )
    })();
    if let Err(error) = published {
        let _ = connection.execute_batch("ROLLBACK;");
        return Err(error.to_string());
    }
    connection
        .execute_batch("COMMIT;")
        .map_err(|error| error.to_string())?;
    drop(connection);

    let cancellation = CancellationFlag::default();
    state
        .discussion_cancellations
        .lock()
        .map_err(|_| "Discussion cancellation lock poisoned".to_string())?
        .insert(assistant_id.clone(), cancellation.clone());
    let generation = DiscussionGeneration {
        root,
        revision,
        thread_id: request.thread_id.clone(),
        assistant_id: assistant_id.clone(),
        provider,
        provider_instance_id,
        provider_route_id,
        model,
        context_epoch,
        adapter,
        cached_file_id,
        previous_local_provider_id: previous_provider_node
            .as_ref()
            .map(|(local_id, _)| local_id.clone()),
        previous_remote_provider_id: previous_provider_node.map(|(_, remote_id)| remote_id),
        system_instruction,
        compaction_instruction,
        incremental_input,
        recovery_input,
        history,
        current_block_quotes: block_quotes.clone(),
        needs_compaction,
        cancellation,
    };
    emit_generation_event(&app, &assistant_id, None, Some("streaming"));
    tauri::async_runtime::spawn(run_discussion_generation(app.clone(), generation));

    Ok(ChatTurn {
        user: Message {
            id: user_id.clone(),
            thread_id: request.thread_id.clone(),
            parent_id,
            role: "user".to_string(),
            content: question,
            citations: Vec::new(),
            block_quotes,
            status: "complete".to_string(),
            created_at: timestamp.clone(),
            usage: None,
        },
        assistant: Message {
            id: assistant_id,
            thread_id: request.thread_id,
            parent_id: Some(user_id),
            role: "assistant".to_string(),
            content: String::new(),
            citations: Vec::new(),
            block_quotes: Vec::new(),
            status: "streaming".to_string(),
            created_at: timestamp,
            usage: None,
        },
    })
}

#[tauri::command]
fn cancel_generation(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    message_id: String,
) -> AppResult<bool> {
    let cancellation = state
        .discussion_cancellations
        .lock()
        .map_err(|_| "Discussion cancellation lock poisoned".to_string())?
        .get(&message_id)
        .cloned();
    let Some(cancellation) = cancellation else {
        return Ok(false);
    };
    cancellation.cancel();
    emit_generation_event(&app, &message_id, None, Some("cancelling"));
    Ok(true)
}

#[tauri::command]
fn create_branch(
    state: State<'_, AppState>,
    thread_id: String,
    message_id: String,
) -> AppResult<String> {
    let root = active_root(&state)?;
    let connection = open_db(&root)?;
    let exists = connection
        .query_row(
            "SELECT 1 FROM messages WHERE id = ?1 AND discussion_id = ?2",
            params![message_id, thread_id],
            |_| Ok(true),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or(false);
    if !exists {
        return Err("The branch point does not belong to this Discussion".to_string());
    }
    Ok(message_id)
}

#[tauri::command]
fn set_active_branch(
    state: State<'_, AppState>,
    thread_id: String,
    message_id: String,
) -> AppResult<bool> {
    let root = active_root(&state)?;
    let connection = open_db(&root)?;
    let changed = connection
        .execute(
            "UPDATE discussion_heads
             SET message_id = ?1, updated_at = ?2
             WHERE discussion_id = ?3
               AND EXISTS (
                 SELECT 1 FROM messages
                 WHERE id = ?1 AND discussion_id = ?3
               )",
            params![message_id, now(), thread_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("The selected message does not belong to this Discussion".to_string());
    }
    connection
        .execute(
            "UPDATE discussions SET updated_at = ?1 WHERE id = ?2",
            params![now(), thread_id],
        )
        .map_err(|error| error.to_string())?;
    Ok(true)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeleteDiscussionTurnResult {
    deleted: i64,
    next_head_id: Option<String>,
}

fn turn_user_id(
    connection: &Connection,
    thread_id: &str,
    message_id: &str,
) -> AppResult<(String, Option<String>)> {
    let (id, parent_id, role, status): (String, Option<String>, String, String) = connection
        .query_row(
            "SELECT id, parent_id, role, status FROM messages
             WHERE id = ?1 AND discussion_id = ?2",
            params![message_id, thread_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "The discussion turn was not found".to_string())?;
    if status == "streaming" {
        return Err("Cannot delete a turn that is still generating".to_string());
    }
    if role == "user" {
        return Ok((id, parent_id));
    }
    if role == "assistant" {
        let user_id = parent_id
            .clone()
            .ok_or_else(|| "Cannot delete an answer without its question".to_string())?;
        let user_parent: Option<String> = connection
            .query_row(
                "SELECT parent_id FROM messages WHERE id = ?1 AND discussion_id = ?2 AND role = 'user'",
                params![user_id, thread_id],
                |row| row.get(0),
            )
            .map_err(|_| "The question for this turn was not found".to_string())?;
        return Ok((user_id, user_parent));
    }
    Err("Only a question or answer can be deleted".to_string())
}

fn collect_turn_subtree(
    connection: &Connection,
    thread_id: &str,
    user_id: &str,
) -> AppResult<Vec<String>> {
    let mut ids = Vec::new();
    let mut stack = vec![user_id.to_string()];
    while let Some(current) = stack.pop() {
        if ids.iter().any(|id| id == &current) {
            continue;
        }
        ids.push(current.clone());
        let mut statement = connection
            .prepare(
                "SELECT id, role, status FROM messages
                 WHERE discussion_id = ?1 AND parent_id = ?2",
            )
            .map_err(|error| error.to_string())?;
        let children = statement
            .query_map(params![thread_id, current], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| error.to_string())?;
        drop(statement);
        for (child_id, _role, status) in children {
            if status == "streaming" {
                return Err("Cannot delete a turn that is still generating".to_string());
            }
            stack.push(child_id);
        }
    }
    Ok(ids)
}

fn delete_discussion_subtree(
    connection: &Connection,
    thread_id: &str,
    message_id: &str,
) -> AppResult<DeleteDiscussionTurnResult> {
    let (user_id, user_parent_id) = turn_user_id(connection, thread_id, message_id)?;
    let ids = collect_turn_subtree(connection, thread_id, &user_id)?;
    let current_head: Option<String> = connection
        .query_row(
            "SELECT message_id FROM discussion_heads WHERE discussion_id = ?1",
            params![thread_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    let head_deleted = current_head
        .as_ref()
        .is_some_and(|head| ids.iter().any(|id| id == head));
    let next_head_id = if head_deleted {
        user_parent_id
    } else {
        current_head
    };
    if head_deleted {
        connection
            .execute(
                "UPDATE discussion_heads SET message_id = ?1, updated_at = ?2
                 WHERE discussion_id = ?3",
                params![next_head_id, now(), thread_id],
            )
            .map_err(|error| error.to_string())?;
    }
    let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let mut params: Vec<rusqlite::types::Value> = ids
        .iter()
        .map(|id| rusqlite::types::Value::Text(id.clone()))
        .collect();
    params.push(rusqlite::types::Value::Text(thread_id.to_string()));
    let deleted = connection
        .execute(
            &format!("DELETE FROM messages WHERE id IN ({placeholders}) AND discussion_id = ?"),
            rusqlite::params_from_iter(params),
        )
        .map_err(|error| error.to_string())?;
    connection
        .execute(
            "UPDATE discussions SET updated_at = ?1 WHERE id = ?2",
            params![now(), thread_id],
        )
        .map_err(|error| error.to_string())?;
    Ok(DeleteDiscussionTurnResult {
        deleted: deleted as i64,
        next_head_id,
    })
}

#[tauri::command]
fn delete_discussion_turn(
    state: State<'_, AppState>,
    thread_id: String,
    message_id: String,
) -> AppResult<DeleteDiscussionTurnResult> {
    let root = active_root(&state)?;
    let connection = open_db(&root)?;
    delete_discussion_subtree(&connection, &thread_id, &message_id)
}

#[tauri::command]
fn archive_thread(state: State<'_, AppState>, thread_id: String) -> AppResult<bool> {
    let root = active_root(&state)?;
    let connection = open_db(&root)?;
    connection
        .execute(
            "UPDATE discussions SET status = 'archived', updated_at = ?1 WHERE id = ?2",
            params![now(), thread_id],
        )
        .map_err(|error| error.to_string())?;
    Ok(true)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloseThreadResult {
    action: String,
    thread_id: String,
}

fn close_discussion(conn: &Connection, thread_id: &str) -> AppResult<CloseThreadResult> {
    let (revision_id, status): (String, String) = conn
        .query_row(
            "SELECT revision_id, status FROM discussions WHERE id = ?1",
            params![thread_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .map_err(|error| error.to_string())?;
    if status != "active" {
        return Err("Only an open discussion can be closed".to_string());
    }
    let active_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM discussions WHERE revision_id = ?1 AND status = 'active'",
            params![revision_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if active_count <= 1 {
        return Err("Keep at least one open discussion".to_string());
    }
    let user_messages: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM messages WHERE discussion_id = ?1 AND role = 'user'",
            params![thread_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if user_messages == 0 {
        conn.execute("DELETE FROM discussions WHERE id = ?1", params![thread_id])
            .map_err(|error| error.to_string())?;
        return Ok(CloseThreadResult {
            action: "discarded".to_string(),
            thread_id: thread_id.to_string(),
        });
    }
    conn.execute(
        "UPDATE discussions SET status = 'archived', updated_at = ?1 WHERE id = ?2",
        params![now(), thread_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(CloseThreadResult {
        action: "archived".to_string(),
        thread_id: thread_id.to_string(),
    })
}

#[tauri::command]
fn close_thread(state: State<'_, AppState>, thread_id: String) -> AppResult<CloseThreadResult> {
    let root = active_root(&state)?;
    let connection = open_db(&root)?;
    close_discussion(&connection, &thread_id)
}

#[tauri::command]
fn restore_thread(state: State<'_, AppState>, thread_id: String) -> AppResult<Thread> {
    let root = active_root(&state)?;
    let connection = open_db(&root)?;
    let changed = connection
        .execute(
            "UPDATE discussions SET status = 'active', updated_at = ?1
             WHERE id = ?2 AND status = 'archived'",
            params![now(), thread_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("Only a closed discussion can be restored".to_string());
    }
    load_thread(&connection, &thread_id)
}

#[tauri::command]
fn delete_thread(state: State<'_, AppState>, thread_id: String) -> AppResult<bool> {
    let root = active_root(&state)?;
    let connection = open_db(&root)?;
    let changed = connection
        .execute(
            "DELETE FROM discussions WHERE id = ?1 AND status = 'archived'",
            params![thread_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("Only a closed discussion can be deleted".to_string());
    }
    Ok(true)
}

#[tauri::command]
fn rename_thread(
    state: State<'_, AppState>,
    thread_id: String,
    title: String,
) -> AppResult<Thread> {
    let title = title.trim().to_string();
    if title.is_empty() {
        return Err("Discussion title cannot be empty".to_string());
    }
    let root = active_root(&state)?;
    let connection = open_db(&root)?;
    let changed = connection
        .execute(
            "UPDATE discussions SET title = ?1, updated_at = ?2 WHERE id = ?3",
            params![title, now(), thread_id],
        )
        .map_err(|error| error.to_string())?;
    if changed == 0 {
        return Err("Discussion was not found".to_string());
    }
    load_thread(&connection, &thread_id)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeneratedOrientationPack {
    brief: GeneratedBrief,
    glossary: Vec<Value>,
    symbol_table: Vec<Value>,
    metadata: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeneratedBrief {
    #[serde(default)]
    takeaway: String,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    classification: String,
    #[serde(default)]
    context: String,
    #[serde(default)]
    background_and_problem: String,
    #[serde(default)]
    core_method: String,
    #[serde(default)]
    findings: String,
    #[serde(default)]
    evaluation: String,
    #[serde(default)]
    future_work: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    research_question: String,
    #[serde(default)]
    method: String,
    #[serde(default)]
    limitations: String,
    #[serde(default)]
    evidence: Vec<GeneratedEvidence>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeneratedEvidence {
    page: i64,
    excerpt: Option<String>,
    confidence: Option<String>,
}

fn sanitize_orientation_pack_json(raw: &str) -> String {
    let mut result = String::with_capacity(raw.len() + 128);
    let chars: Vec<char> = raw.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_string = false;
    let mut escaped = false;

    while i < len {
        let ch = chars[i];
        if in_string {
            if escaped {
                escaped = false;
                if ch == 'b' && i + 1 < len && chars[i + 1].is_alphabetic() {
                    result.push('\\');
                    result.push('b');
                } else if ch == 'r' && i + 1 < len && chars[i + 1].is_alphabetic() {
                    result.push('\\');
                    result.push('r');
                } else if ch == 'f' && i + 1 < len && chars[i + 1].is_alphabetic() {
                    result.push('\\');
                    result.push('f');
                } else if ch == 't' && i + 2 < len && chars[i + 1].is_alphabetic() {
                    result.push('\\');
                    result.push('t');
                } else if ch == 'n' && i + 1 < len && (chars[i + 1] == 'u' || chars[i + 1] == 'a') {
                    result.push('\\');
                    result.push('n');
                } else {
                    result.push(ch);
                }
            } else if ch == '\\' {
                escaped = true;
                result.push('\\');
            } else if ch == '"' {
                in_string = false;
                result.push('"');
            } else if ch == '\x08' {
                result.push_str("\\\\b");
            } else if ch == '\x0C' {
                result.push_str("\\\\f");
            } else {
                result.push(ch);
            }
        } else {
            if ch == '"' {
                in_string = true;
            }
            result.push(ch);
        }
        i += 1;
    }
    result
}

fn parse_orientation_pack(
    text: &str,
    revision: &RevisionRecord,
    model: &str,
) -> AppResult<(Brief, Vec<Value>, Vec<Value>, Value)> {
    let start = text
        .find('{')
        .ok_or_else(|| "Orientation response does not contain a JSON object".to_string())?;
    let end = text
        .rfind('}')
        .ok_or_else(|| "Orientation response has an incomplete JSON object".to_string())?;
    let raw_slice = &text[start..=end];
    let sanitized = sanitize_orientation_pack_json(raw_slice);
    let pack: GeneratedOrientationPack = serde_json::from_str(&sanitized)
        .or_else(|_| serde_json::from_str(raw_slice))
        .map_err(|error| format!("Orientation response failed schema validation: {error}"))?;

    let takeaway = if !pack.brief.takeaway.trim().is_empty() {
        pack.brief.takeaway.trim().to_string()
    } else if !pack.brief.summary.trim().is_empty() {
        pack.brief.summary.trim().to_string()
    } else {
        return Err("Orientation response contains an empty takeaway/summary".to_string());
    };

    let core_method = if !pack.brief.core_method.trim().is_empty() {
        pack.brief.core_method.trim().to_string()
    } else {
        pack.brief.method.trim().to_string()
    };

    let summary = if !pack.brief.summary.trim().is_empty() {
        pack.brief.summary.trim().to_string()
    } else {
        takeaway.clone()
    };

    let method = if !pack.brief.method.trim().is_empty() {
        pack.brief.method.trim().to_string()
    } else {
        core_method.clone()
    };

    if pack.brief.keywords.is_empty()
        || !pack.metadata.is_object()
        || pack.glossary.iter().any(|entry| !entry.is_object())
        || pack.symbol_table.iter().any(|entry| !entry.is_object())
    {
        return Err("Orientation response contains an empty or invalid field".to_string());
    }

    let mut evidence = Vec::new();
    for anchor in pack.brief.evidence {
        if anchor.page < 1
            || revision
                .page_count
                .is_some_and(|maximum| anchor.page > maximum)
        {
            return Err("Orientation evidence page is outside the PDF".to_string());
        }
        evidence.push(Citation {
            revision_id: revision.id.clone(),
            page: Some(anchor.page),
            region: None,
            region_hash: None,
            excerpt: anchor.excerpt,
            confidence: anchor.confidence,
        });
    }

    let brief = Brief {
        revision_id: revision.id.clone(),
        version: 1,
        status: "ready".to_string(),
        takeaway: normalize_markdown_field(&takeaway),
        keywords: pack.brief.keywords,
        classification: normalize_markdown_field(&pack.brief.classification),
        context: normalize_markdown_field(&pack.brief.context),
        background_and_problem: normalize_markdown_field(&pack.brief.background_and_problem),
        core_method: normalize_markdown_field(&core_method),
        findings: if !pack.brief.findings.trim().is_empty() {
            normalize_markdown_field(&pack.brief.findings)
        } else {
            normalize_markdown_field(&takeaway)
        },
        evaluation: if !pack.brief.evaluation.trim().is_empty() {
            normalize_markdown_field(&pack.brief.evaluation)
        } else {
            normalize_markdown_field(&pack.brief.limitations)
        },
        future_work: normalize_markdown_field(&pack.brief.future_work),
        summary: normalize_markdown_field(&summary),
        research_question: normalize_markdown_field(&pack.brief.research_question),
        method: normalize_markdown_field(&method),
        limitations: normalize_markdown_field(&pack.brief.limitations),
        evidence,
        model: model.to_string(),
        created_at: now(),
    };
    Ok((brief, pack.glossary, pack.symbol_table, pack.metadata))
}

fn brief_evidence(brief: &Brief) -> Vec<EvidenceAnchor> {
    brief
        .evidence
        .iter()
        .filter_map(|citation| {
            citation.page.map(|page_number| EvidenceAnchor {
                revision_id: brief.revision_id.clone(),
                page_number,
                block_id: None,
                bbox: None,
                excerpt: citation.excerpt.clone(),
            })
        })
        .collect()
}

#[tauri::command]
fn get_brief(state: State<'_, AppState>, revision_id: String) -> AppResult<Option<Value>> {
    let artifact = active_artifact_module(&state)?.head_for_revision(&revision_id, "brief", "")?;
    artifact
        .map(|artifact| {
            let mut value = artifact.content;
            if value["briefProtocol"].as_str() == Some(textbook_contract::BRIEF_PROTOCOL) {
                let body: serde_json::Map<String, Value> = textbook_contract::BRIEF_FIELDS
                    .iter()
                    .filter_map(|key| value.get(*key).map(|v| ((*key).into(), v.clone())))
                    .collect();
                textbook_contract::validate_body(&Value::Object(body))?;
            } else {
                let _: Brief = serde_json::from_value(value.clone())
                    .map_err(|e| format!("Stored Brief is invalid: {e}"))?;
            }
            value["version"] = json!(artifact.version);
            value["status"] = json!(artifact.status);
            value["createdAt"] = json!(artifact.created_at);
            Ok(value)
        })
        .transpose()
}

async fn publish_orientation_pack(
    app: tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
) -> AppResult<Brief> {
    let revision_id = job
        .revision_id
        .clone()
        .ok_or_else(|| "Orientation job has no revisionId".to_string())?;
    let job_id = job.id.clone();
    let state = app.state::<AppState>();
    let root = runtime.root.clone();
    let connection = open_db(&root)?;
    let mut revision = revision_record(&connection, &revision_id)?;
    revision.pdf_path = absolute_pdf(&root, &revision.pdf_path);
    let paper_id: String = connection
        .query_row(
            "SELECT paper_id FROM document_revisions WHERE id = ?1",
            params![revision_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let provider = route.frozen().provider_kind().as_str().to_string();
    let model = route.frozen().models().paper().to_string();
    let provider_route_id = route.frozen().route_id().database_value();
    let context_epoch = route_scoped_context_epoch(
        &format!("{}:{}", revision.sha256, model),
        &provider_route_id,
    );
    let cached_uri: Option<String> = connection
        .query_row(
            "SELECT provider_file_id FROM context_roots
             WHERE revision_id = ?1 AND provider_route_id = ?2 AND model = ?3
               AND context_epoch = ?4 AND invalidated_at IS NULL LIMIT 1",
            params![revision.id, provider_route_id, model, context_epoch],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    drop(connection);

    let document_kind = revision_document_kind_from_root(&runtime.root, &revision_id);
    let metadata_schema = match document_kind {
        crate::library_paths::DocumentKind::Textbook => json!({
            "type": "object", "additionalProperties": false,
            "required": ["title", "bookName", "chapterNumber", "authors"],
            "properties": {
                "title": {"type": "string", "minLength": 1},
                "bookName": {"type": "string"},
                "isbn": {"type": "string"},
                "chapterNumber": {"type": "string"},
                "authors": {"type": "array", "items": {"type": "string"}},
                "publicationYear": {"type": ["integer", "null"]},
                "abstract": {"type": "string"}
            }
        }),
        crate::library_paths::DocumentKind::Paper => json!({
            "type": "object", "additionalProperties": false,
            "required": ["title", "authors", "publicationYear", "venue", "doi", "abstract"],
            "properties": {
                "title": {"type": "string", "minLength": 1},
                "authors": {"type": "array", "items": {"type": "string"}},
                "publicationYear": {"type": ["integer", "null"]},
                "venue": {"type": "string"},
                "doi": {"type": "string"},
                "abstract": {"type": "string"}
            }
        }),
    };
    let response_schema = json!({
        "name": "orientation_pack",
        "strict": true,
        "schema": {
            "type": "object",
            "additionalProperties": false,
            "required": ["brief", "glossary", "symbolTable", "metadata"],
            "properties": {
                "brief": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": [
                        "takeaway",
                        "keywords",
                        "classification",
                        "context",
                        "backgroundAndProblem",
                        "coreMethod",
                        "findings",
                        "evaluation",
                        "futureWork"
                    ],
                    "properties": {
                        "takeaway": {"type": "string", "minLength": 1},
                        "keywords": {"type": "array", "minItems": 1, "items": {"type": "string", "minLength": 1}},
                        "classification": {"type": "string", "minLength": 1},
                        "context": {"type": "string", "minLength": 1},
                        "backgroundAndProblem": {"type": "string", "minLength": 1},
                        "coreMethod": {"type": "string", "minLength": 1},
                        "findings": {"type": "string", "minLength": 1},
                        "evaluation": {"type": "string", "minLength": 1},
                        "futureWork": {"type": "string", "minLength": 1}
                    }
                },
                "glossary": {"type": "array", "items": {
                    "type": "object", "additionalProperties": false,
                    "required": ["term", "definition", "aliases"],
                    "properties": {
                        "term": {"type": "string", "minLength": 1},
                        "definition": {"type": "string", "minLength": 1},
                        "aliases": {"type": "array", "items": {"type": "string"}}
                    }
                }},
                "symbolTable": {"type": "array", "items": {
                    "type": "object", "additionalProperties": false,
                    "required": ["symbol", "meaning", "scope"],
                    "properties": {
                        "symbol": {"type": "string", "minLength": 1},
                        "meaning": {"type": "string", "minLength": 1},
                        "scope": {"type": "string"}
                    }
                }},
                "metadata": metadata_schema
            }
        }
    });
    let job_module = runtime.job_module.clone();
    job_module
        .save_checkpoint(
            &job_id,
            "requesting",
            &json!({"revisionId": revision_id, "model": model}),
        )
        .ok();
    let cancellation = CancellationFlag::default();
    state
        .artifact_cancellations
        .lock()
        .map_err(|_| "Artifact cancellation lock poisoned".to_string())?
        .insert(job_id.clone(), cancellation.clone());
    let adapter = job_paper_model_adapter(
        route.adapter_arc(),
        job_module.clone(),
        job_id.clone(),
        cancellation,
    );
    let system_instruction = {
        let jobs = runtime.job_module.clone();
        jobs.get(&job_id)
            .ok()
            .and_then(|job| frozen_prompt_field(&job.payload, "orientation").ok())
            .map(Ok)
            .unwrap_or_else(|| {
                load_prompt_for_revision_from_root(
                    &app,
                    &runtime.root,
                    &revision_id,
                    PromptSlotId::OrientationPack,
                    None,
                )
            })?
    };
    let outcome = match adapter
        .interact(PaperInteractionRequest {
            model: model.clone(),
            context_epoch: context_epoch.clone(),
            pdf_path: revision.pdf_path.clone(),
            display_name: revision.title.clone(),
            remote_file_id: cached_uri.clone(),
            previous_interaction_id: None,
            system_instruction: system_instruction.clone(),
            user_input:
                "Generate the Brief, professional glossary, symbol table, and paper metadata now."
                    .to_string(),
            response_schema: Some(response_schema.clone()),
            inline_images: Vec::new(),
            kind: PaperInteractionKind::Root,
        })
        .await
    {
        Ok(outcome) => outcome,
        Err(error)
            if cached_uri.is_some()
                && (error.kind == ProviderErrorKind::StaleRemoteResource
                    || error.message.contains("403")
                    || error.message.contains("permission")
                    || error.message.contains("404")
                    || error.message.contains("not found")) =>
        {
            if let Ok(connection) = open_db(&root) {
                let _ = connection.execute(
                    "UPDATE context_roots SET state = 'invalidated', invalidated_at = ?1
                     WHERE revision_id = ?2 AND provider_route_id = ?3 AND model = ?4
                       AND context_epoch = ?5",
                    params![now(), revision.id, provider_route_id, model, context_epoch],
                );
            }
            adapter
                .interact(PaperInteractionRequest {
                    model: model.clone(),
                    context_epoch: context_epoch.clone(),
                    pdf_path: revision.pdf_path.clone(),
                    display_name: revision.title.clone(),
                    remote_file_id: None,
                    previous_interaction_id: None,
                    system_instruction,
                    user_input:
                        "Generate the Brief, professional glossary, symbol table, and paper metadata now."
                            .to_string(),
                    response_schema: Some(response_schema),
                    inline_images: Vec::new(),
                    kind: PaperInteractionKind::Root,
                })
                .await
                .map_err(|err| err.to_string())?
        }
        Err(error) => return Err(error.to_string()),
    };
    if let Ok(mut cancellations) = state.artifact_cancellations.lock() {
        cancellations.remove(&job_id);
    }
    let remote_provider_node_id = outcome.provider_node_id;
    let provider_file_id = outcome.provider_file_id;
    let receipt = receipt_from_envelope(outcome.receipt);
    let text = outcome.text;
    let (mut brief, glossary, symbol_table, metadata) =
        parse_orientation_pack(&text, &revision, &model)?;

    let connection = open_db(&root)?;
    let context_root_id = connection
        .query_row(
            "SELECT id FROM context_roots
             WHERE revision_id = ?1 AND provider_route_id = ?2 AND model = ?3
               AND context_epoch = ?4 AND invalidated_at IS NULL LIMIT 1",
            params![revision.id, provider_route_id, model, context_epoch],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    connection
        .execute(
            "INSERT INTO context_roots(
           id, revision_id, provider, model, context_epoch, provider_route_id,
           provider_file_id, provider_node_id, state, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'active', ?9)
         ON CONFLICT(revision_id, provider, model, context_epoch) DO UPDATE SET
           provider_file_id = excluded.provider_file_id,
           provider_node_id = excluded.provider_node_id,
           state = 'active', invalidated_at = NULL",
            params![
                context_root_id,
                revision.id,
                provider,
                model,
                context_epoch,
                provider_route_id,
                provider_file_id,
                remote_provider_node_id,
                now()
            ],
        )
        .map_err(|error| error.to_string())?;
    let provider_node_id = Uuid::new_v4().to_string();
    connection
        .execute(
            "INSERT INTO provider_nodes(
           id, provider, model, context_epoch, provider_route_id, provider_node_id, state, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'complete', ?7)",
            params![
                provider_node_id,
                provider,
                model,
                context_epoch,
                provider_route_id,
                remote_provider_node_id,
                now()
            ],
        )
        .map_err(|error| error.to_string())?;

    let existing_glossary_raw: Option<String> = connection
        .query_row(
            "SELECT a.content_json FROM artifacts a
             JOIN artifact_heads h ON h.artifact_id = a.id
             WHERE a.revision_id = ?1 AND a.kind = 'glossary'",
            params![revision.id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let (final_glossary, final_glossary_pinned) = if let Some(raw) = existing_glossary_raw {
        let val: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        let existing_entries = val
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let pinned_keys: Vec<String> = val
            .get("_pinned")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        let mut merged = Vec::<Value>::new();
        let mut pinned_found = Vec::<String>::new();

        for new_entry in glossary {
            let term_key = new_entry.get("term").and_then(Value::as_str).unwrap_or("");
            if pinned_keys.contains(&term_key.to_string()) {
                if let Some(existing_pinned) = existing_entries
                    .iter()
                    .find(|e| e.get("term").and_then(Value::as_str) == Some(term_key))
                {
                    merged.push(existing_pinned.clone());
                    pinned_found.push(term_key.to_string());
                    continue;
                }
            }
            merged.push(new_entry);
        }

        for existing in existing_entries {
            let term_key = existing
                .get("term")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if pinned_keys.contains(&term_key) && !pinned_found.contains(&term_key) {
                pinned_found.push(term_key);
                merged.push(existing);
            }
        }
        (merged, pinned_keys)
    } else {
        (glossary, Vec::new())
    };

    let existing_symbols_raw: Option<String> = connection
        .query_row(
            "SELECT a.content_json FROM artifacts a
             JOIN artifact_heads h ON h.artifact_id = a.id
             WHERE a.revision_id = ?1 AND a.kind = 'symbol_table'",
            params![revision.id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;

    let (final_symbols, final_symbols_pinned) = if let Some(raw) = existing_symbols_raw {
        let val: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
        let existing_entries = val
            .get("entries")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let pinned_keys: Vec<String> = val
            .get("_pinned")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();

        let mut merged = Vec::<Value>::new();
        let mut pinned_found = Vec::<String>::new();

        for new_entry in symbol_table {
            let sym_key = new_entry
                .get("symbol")
                .and_then(Value::as_str)
                .unwrap_or("");
            if pinned_keys.contains(&sym_key.to_string()) {
                if let Some(existing_pinned) = existing_entries
                    .iter()
                    .find(|e| e.get("symbol").and_then(Value::as_str) == Some(sym_key))
                {
                    merged.push(existing_pinned.clone());
                    pinned_found.push(sym_key.to_string());
                    continue;
                }
            }
            merged.push(new_entry);
        }

        for existing in existing_entries {
            let sym_key = existing
                .get("symbol")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            if pinned_keys.contains(&sym_key) && !pinned_found.contains(&sym_key) {
                pinned_found.push(sym_key);
                merged.push(existing);
            }
        }
        (merged, pinned_keys)
    } else {
        (symbol_table, Vec::new())
    };

    let batch_id = Uuid::new_v4().to_string();
    let dependencies = serde_json::json!({
        "batchId": batch_id,
        "protocol": "orientation-pack-legacy",
        "revisionId": revision.id,
        "contextEpoch": context_epoch,
        "providerRouteId": provider_route_id,
        "model": model
    });
    let artifact_module = runtime.artifact_module.clone();
    let batch = artifact_module.publish_batch(vec![
        ArtifactDraft {
            paper_id: paper_id.clone(),
            revision_id: revision.id.clone(),
            ocr_revision_id: None,
            kind: "glossary".to_string(),
            object_key: String::new(),
            content: serde_json::json!({
                "entries": final_glossary,
                "_pinned": final_glossary_pinned
            }),
            evidence: Vec::new(),
            dependency_snapshot: dependencies.clone(),
            provider_node_id: Some(provider_node_id.clone()),
        },
        ArtifactDraft {
            paper_id: paper_id.clone(),
            revision_id: revision.id.clone(),
            ocr_revision_id: None,
            kind: "symbol_table".to_string(),
            object_key: String::new(),
            content: serde_json::json!({
                "entries": final_symbols,
                "_pinned": final_symbols_pinned
            }),
            evidence: Vec::new(),
            dependency_snapshot: dependencies.clone(),
            provider_node_id: Some(provider_node_id.clone()),
        },
        ArtifactDraft {
            paper_id: paper_id.clone(),
            revision_id: revision.id.clone(),
            ocr_revision_id: None,
            kind: "metadata".to_string(),
            object_key: String::new(),
            content: metadata.clone(),
            evidence: Vec::new(),
            dependency_snapshot: dependencies.clone(),
            provider_node_id: Some(provider_node_id.clone()),
        },
        ArtifactDraft {
            paper_id: paper_id.clone(),
            revision_id: revision.id.clone(),
            ocr_revision_id: None,
            kind: "brief".to_string(),
            object_key: String::new(),
            content: serde_json::to_value(&brief).map_err(|error| error.to_string())?,
            evidence: brief_evidence(&brief),
            dependency_snapshot: dependencies,
            provider_node_id: Some(provider_node_id),
        },
    ])?;
    let brief_artifact = batch
        .into_iter()
        .find(|artifact| artifact.kind == "brief")
        .ok_or_else(|| "Orientation batch did not publish the Brief".to_string())?;
    brief.version = brief_artifact.version;
    brief.created_at = brief_artifact.created_at;

    let existing_meta_row: Option<(
        String,
        String,
        Option<i64>,
        Option<String>,
        Option<String>,
        Option<String>,
        String,
    )> = connection
        .query_row(
            "SELECT title, authors_json, publication_year, venue, doi, abstract_text, metadata_json
             FROM paper_metadata WHERE revision_id = ?1",
            params![revision.id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;

    let (pinned_fields, existing_vals) = if let Some(row) = existing_meta_row {
        let meta_obj: Value = serde_json::from_str(&row.6).unwrap_or(Value::Null);
        let pinned: Vec<String> = meta_obj
            .get("_pinned")
            .and_then(Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default();
        (pinned, Some(row))
    } else {
        (Vec::new(), None)
    };

    let title_is_pinned = pinned_fields.contains(&"title".to_string());
    let title = if title_is_pinned && existing_vals.is_some() {
        existing_vals.as_ref().unwrap().0.clone()
    } else {
        metadata
            .get("title")
            .and_then(Value::as_str)
            .filter(|title| !title.trim().is_empty())
            .map(String::from)
            .unwrap_or_else(|| revision.title.clone())
    };

    let authors_is_pinned = pinned_fields.contains(&"authors".to_string());
    let authors_json = if authors_is_pinned && existing_vals.is_some() {
        existing_vals.as_ref().unwrap().1.clone()
    } else {
        metadata
            .get("authors")
            .filter(|value| value.is_array())
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]))
            .to_string()
    };

    let year_is_pinned = pinned_fields.contains(&"publicationYear".to_string());
    let publication_year = if year_is_pinned && existing_vals.is_some() {
        existing_vals.as_ref().unwrap().2
    } else {
        metadata.get("publicationYear").and_then(Value::as_i64)
    };

    let venue_is_pinned = pinned_fields.contains(&"venue".to_string());
    let venue = if venue_is_pinned && existing_vals.is_some() {
        existing_vals.as_ref().unwrap().3.clone()
    } else {
        metadata
            .get("venue")
            .and_then(Value::as_str)
            .map(String::from)
    };

    let doi_is_pinned = pinned_fields.contains(&"doi".to_string());
    let doi = if doi_is_pinned && existing_vals.is_some() {
        existing_vals.as_ref().unwrap().4.clone()
    } else {
        metadata
            .get("doi")
            .and_then(Value::as_str)
            .map(String::from)
    };

    let abstract_is_pinned = pinned_fields.contains(&"abstract".to_string());
    let abstract_text = if abstract_is_pinned && existing_vals.is_some() {
        existing_vals.as_ref().unwrap().5.clone()
    } else {
        metadata
            .get("abstract")
            .and_then(Value::as_str)
            .map(String::from)
    };

    let mut merged_meta = metadata.as_object().cloned().unwrap_or_default();
    if !pinned_fields.is_empty() {
        merged_meta.insert("_pinned".to_string(), json!(pinned_fields));
    }
    let metadata_str = serde_json::to_string(&merged_meta).unwrap_or_else(|_| metadata.to_string());

    connection
        .execute(
            "UPDATE paper_metadata SET
           title = ?1, authors_json = ?2, publication_year = ?3,
           venue = ?4, doi = ?5, abstract_text = ?6,
           metadata_json = ?7, source = 'model'
         WHERE revision_id = ?8 AND NOT EXISTS (SELECT 1 FROM artifact_heads h JOIN artifacts a ON a.id=h.artifact_id WHERE a.revision_id=?8 AND a.kind='metadata' AND json_extract(a.dependency_snapshot_json,'$.protocol') LIKE 'document-artifact-%')",
            params![
                title,
                authors_json,
                publication_year,
                venue,
                doi,
                abstract_text,
                metadata_str,
                revision.id
            ],
        )
        .map_err(|error| error.to_string())?;

    connection
        .execute(
            "INSERT INTO usage_receipts(
           id, operation_id, job_id, provider, model, context_epoch, provider_route_id,
           input_tokens, cached_input_tokens, uncached_input_tokens,
           output_tokens, reasoning_tokens, latency_ms, estimated_cost, file_reuse,
           session_resume, paper_root_branch, created_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                Uuid::new_v4().to_string(),
                batch_id,
                job_id,
                receipt.provider,
                receipt.model,
                receipt.context_epoch.as_deref().unwrap_or(&context_epoch),
                provider_route_id,
                receipt.input_tokens,
                receipt.cached_input_tokens,
                receipt.uncached_input_tokens,
                receipt.output_tokens,
                receipt.reasoning_tokens,
                receipt.latency_ms,
                receipt.estimated_cost,
                receipt.file_reuse.map(i64::from),
                receipt.session_resume.map(i64::from),
                receipt.paper_root_branch.map(i64::from),
                now()
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(brief)
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutlineRequest {
    revision_id: String,
    #[serde(default)]
    plan_id: Option<String>,
    #[serde(default)]
    plan_digest: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutlineDeepDiveRequest {
    revision_id: String,
    node_id: String,
    #[serde(default)]
    plan_id: Option<String>,
    #[serde(default)]
    plan_digest: Option<String>,
}

#[tauri::command]
fn get_outline(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    revision_id: String,
) -> AppResult<OutlineProjection> {
    let outline = active_outline_module(&state)?;
    match capture_current_paper_read_route(&app, &state) {
        Ok(current) => outline.project_for_route(
            &revision_id,
            &current.route.frozen().route_id().database_value(),
            Some(&current.model),
        ),
        Err(_) => outline.project_without_route(&revision_id),
    }
}

#[tauri::command]
fn get_reading_guide(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    revision_id: String,
) -> AppResult<guide_module::GuideProjection> {
    let guide = active_guide_module(&state)?;
    match capture_current_paper_read_route(&app, &state) {
        Ok(current) => guide.project_for_route(
            &revision_id,
            &current.route.frozen().route_id().database_value(),
            Some(&current.model),
        ),
        Err(_) => guide.project_without_route(&revision_id),
    }
}

#[tauri::command]
fn delete_reading_guide(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: guide_commands::ReadingGuideRequest,
) -> AppResult<guide_module::GuideProjection> {
    guide_commands::delete_reading_guide_impl(&app, &state, request)
}

#[tauri::command]
fn plan_reading_guide(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: guide_commands::ReadingGuideRequest,
) -> AppResult<guide_module::GuidePlan> {
    let runtime = current_runtime(&state)?;
    let current = capture_current_paper_read_route(&app, &state)?;
    let (mut plan, mut payload) = crate::guide_plan::prepare(&app, &runtime, &current, &request)?;
    runtime.guide_module.save_frozen_plan(
        &mut plan,
        &mut payload,
        &current.route.frozen().route_id().database_value(),
    )?;
    Ok(plan)
}

static GUIDE_START_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tauri::command]
fn start_reading_guide(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: guide_commands::ReadingGuideRequest,
) -> AppResult<JobProjection> {
    let runtime = current_runtime(&state)?;
    let _start_guard = GUIDE_START_LOCK.lock().map_err(|_| "旁批启动锁不可用")?;
    if let Some(active) = runtime
        .job_module
        .list_active_of(crate::guide_protocol::GUIDE_JOB_KIND, &request.revision_id)?
        .into_iter()
        .next()
    {
        return Ok(active);
    }
    let current = capture_current_paper_read_route(&app, &state)?;
    let (plan, candidate) = crate::guide_plan::prepare(&app, &runtime, &current, &request)?;
    let route = current.route;
    let jobs = runtime.job_module.clone();
    let guide = runtime.guide_module.clone();
    let paper_id = guide.paper_id_for_revision(&request.revision_id)?;
    let payload = if crate::guide_generation::job_is_v2(&candidate) || request.plan_id.is_some() {
        guide.load_frozen_plan(
            &request.revision_id,
            request.plan_id.as_deref().ok_or("请先确认旁批计划")?,
            request
                .plan_digest
                .as_deref()
                .ok_or("旁批计划缺少摘要，请重新计划")?,
            &candidate,
            &route.frozen().route_id().database_value(),
            &current.model,
        )?
    } else {
        candidate
    };
    let enqueue = enqueue_paper_job(
        &jobs,
        JobSpec {
            kind: crate::guide_protocol::GUIDE_JOB_KIND.to_string(),
            provider: None,
            paper_id: Some(paper_id.clone()),
            revision_id: Some(request.revision_id.clone()),
            root_key: Some(route_scoped_root_key(&request.revision_id, &route)),
            artifact_key: Some(format!("reading-guide:{}", plan.ocr_revision_id)),
            dedupe_key: format!("reading-guide:{}", request.revision_id),
            priority: 70,
            payload,
        },
        &route,
    )?;
    if let Some(ids) = enqueue
        .job
        .payload
        .get("characterIds")
        .and_then(Value::as_array)
    {
        guide.save_document_cast(
            &paper_id,
            &ids.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect::<Vec<_>>(),
        )?;
    }
    emit_job_event_for_runtime(&app, &runtime, &enqueue.job.id);
    spawn_job_workers(app, runtime);
    Ok(enqueue.job)
}

fn apply_guide_batch_text(
    text: &str,
    catalog_blocks: &[crate::guide_validate::GuideCatalogBlock],
    accepted: &mut Vec<crate::guide_validate::GuideInk>,
    dropped: &mut i64,
    warnings: &mut Vec<String>,
) -> bool {
    if text.trim().is_empty() {
        return false;
    }
    match crate::guide_validate::decode_inks_envelope(text) {
        Ok(raw) => {
            let outcome = crate::guide_validate::validate_guide_inks(&raw, catalog_blocks);
            *dropped += outcome.dropped;
            warnings.extend(outcome.warnings);
            accepted.extend(outcome.inks);
            true
        }
        Err(_) => false,
    }
}

fn apply_guide_batch_text_v2(
    text: &str,
    catalog_blocks: &[crate::guide_validate::GuideCatalogBlock],
    snapshot: &crate::guide_cast::GuideCastSnapshot,
    anchor_ids: &[String],
    batch_ordinal: i64,
    accepted: &mut Vec<crate::guide_validate::GuideInk>,
    dropped: &mut i64,
    warnings: &mut Vec<String>,
) -> bool {
    if text.trim().is_empty() {
        return false;
    }
    match crate::guide_validate::decode_inks_envelope(text) {
        Ok(raw) => {
            let outcome = crate::guide_validate_v2::validate_guide_inks_v2(
                &raw,
                catalog_blocks,
                &snapshot.order,
                anchor_ids,
                &format!("b{batch_ordinal}"),
            );
            let valid = outcome.dropped == 0;
            *dropped += outcome.dropped;
            warnings.extend(outcome.warnings);
            let replace: std::collections::HashSet<String> = outcome
                .inks
                .iter()
                .map(|ink| match ink {
                    crate::guide_validate::GuideInk::Note { id, .. }
                    | crate::guide_validate::GuideInk::Trace { id, .. }
                    | crate::guide_validate::GuideInk::Reply { id, .. } => id.clone(),
                })
                .collect();
            crate::guide_generation::merge_validated_batch(accepted, outcome.inks, &replace);
            valid
        }
        Err(_) => false,
    }
}

fn count_guide_notes_on_pages(
    inks: &[crate::guide_validate::GuideInk],
    page_start: i64,
    page_end: i64,
) -> usize {
    inks.iter()
        .filter(|ink| matches!(ink, crate::guide_validate::GuideInk::Note { anchor, .. } if anchor.page_number >= page_start && anchor.page_number <= page_end))
        .count()
}

fn count_guide_inks_on_pages(
    inks: &[crate::guide_validate::GuideInk],
    page_start: i64,
    page_end: i64,
) -> usize {
    inks.iter()
        .filter(|ink| {
            ink.page_number()
                .is_some_and(|page| page >= page_start && page <= page_end)
        })
        .count()
}

async fn interact_text_with_retry(
    adapter: &JobPaperModelAdapter,
    request: TextInteractionRequest,
) -> Result<TextInteractionOutcome, ProviderError> {
    match adapter.interact_text(request.clone()).await {
        Ok(outcome) => Ok(outcome),
        Err(error) if error.kind == ProviderErrorKind::Transport => {
            adapter.interact_text(request).await
        }
        Err(error) => Err(error),
    }
}

async fn execute_reading_guide_job(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
) -> Result<(), ProviderError> {
    let revision_id = job
        .revision_id
        .clone()
        .ok_or_else(|| ProviderError::local_state("Reading guide job has no revisionId"))?;
    let context_prompt = job
        .payload
        .pointer("/prompts/context")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::local_state("Reading guide job is missing context prompt"))?
        .to_string();
    let annotate_prompt = job
        .payload
        .pointer("/prompts/annotate")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::local_state("Reading guide job is missing annotate prompt"))?
        .to_string();
    let guide_reader_context = frozen_reader_context(&job.payload);
    let expected_ocr = job
        .payload
        .get("ocrRevisionId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if let Some(protocol) = crate::guide_generation::unknown_future_protocol(&job.payload) {
        return Err(ProviderError::invalid(format!(
            "不支持的旁批协议：{protocol}"
        )));
    }
    let v2 = crate::guide_generation::job_is_v2(&job.payload);
    let output_language = job
        .payload
        .get("outputLanguage")
        .and_then(Value::as_str)
        .unwrap_or(if v2 {
            crate::guide_protocol::GUIDE_LANGUAGE_ZH
        } else {
            crate::guide_protocol::GUIDE_LANGUAGE
        })
        .to_string();
    let state = app.state::<AppState>();
    let job_module = runtime.job_module.clone();
    let guide = runtime.guide_module.clone();
    let model = route.frozen().models().paper().to_string();
    let (ocr_revision_id, catalog, pages, reused_context) = guide
        .locatable_pages_for(&revision_id)
        .map_err(ProviderError::local_state)?;
    let reused_context = if v2 { None } else { reused_context };
    if ocr_revision_id != expected_ocr {
        return Err(ProviderError::invalid(
            "OCR changed after the reading guide was planned. Create a new plan.",
        ));
    }
    let root = runtime.root.clone();
    let connection = open_db(&root).map_err(ProviderError::local_state)?;
    let mut revision =
        revision_record(&connection, &revision_id).map_err(ProviderError::local_state)?;
    drop(connection);
    revision.pdf_path = absolute_pdf(&root, &revision.pdf_path);
    if v2 {
        if job.payload["catalogDigest"].as_str() != Some(catalog.digest.as_str())
            || job.payload["pdfDigest"].as_str() != Some(revision.sha256.as_str())
        {
            return Err(ProviderError::invalid(
                "文档材料已变化或旧任务未冻结 PDF，请重新计划",
            ));
        }
        let cast: crate::guide_cast::GuideCastSnapshot =
            serde_json::from_value(job.payload["castSnapshot"].clone())
                .map_err(|e| ProviderError::invalid(format!("人物快照无效：{e}")))?;
        if cast.order.is_empty() {
            return Err(ProviderError::invalid("人物快照为空"));
        }
    }
    let context_epoch = route_scoped_context_epoch(
        &format!("{}:{}", revision.sha256, model),
        &route.frozen().route_id().database_value(),
    );
    let cancellation = CancellationFlag::default();
    if let Ok(mut locks) = state.artifact_cancellations.lock() {
        locks.insert(job.id.clone(), cancellation.clone());
    }
    let adapter = job_paper_model_adapter(
        route.adapter_arc(),
        job_module.clone(),
        job.id.clone(),
        cancellation.clone(),
    );
    let mut checkpoint = job_module
        .get_checkpoint(&job.id)
        .map_err(ProviderError::local_state)?
        .unwrap_or_else(|| json!({}));
    let reused_outline_id = reused_context
        .as_ref()
        .and_then(|value| value.get("reusedOutlineRevisionId"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let catalog_blocks = if v2 {
        guide
            .catalog_blocks_full(&ocr_revision_id, &catalog)
            .map_err(ProviderError::local_state)?
    } else {
        guide
            .catalog_blocks(&ocr_revision_id, &catalog)
            .map_err(ProviderError::local_state)?
    };
    let context = if v2 {
        crate::guide_runtime::memo(
            runtime,
            job,
            route,
            &crate::guide_runtime::GuideJobPort(&adapter),
            &revision,
            &catalog_blocks,
            &mut checkpoint,
        )
        .await?
    } else if let Some(saved) = checkpoint.get("context").cloned() {
        saved
    } else if let Some(reused) = reused_context.clone() {
        reused
    } else {
        report_outline_progress(
            app,
            runtime,
            &job_module,
            job,
            "understanding",
            json!({"ocrRevisionId": ocr_revision_id}),
            None,
        )?;
        let response = adapter
            .interact(PaperInteractionRequest {
                model: model.clone(),
                context_epoch: context_epoch.clone(),
                pdf_path: revision.pdf_path.clone(),
                display_name: revision.title.clone(),
                remote_file_id: None,
                previous_interaction_id: None,
                system_instruction: context_prompt,
                user_input: reader_context::prepend_reader_context(
                    &json!({
                        "task": "build_reading_context",
                        "catalog": catalog,
                        "language": output_language
                    })
                    .to_string(),
                    guide_reader_context.as_deref(),
                ),
                response_schema: Some(if v2 {
                    crate::guide_protocol::memo_schema()
                } else {
                    crate::guide_protocol::context_schema()
                }),
                inline_images: Vec::new(),
                kind: PaperInteractionKind::Artifact,
            })
            .await?;
        cancellation.check()?;
        record_route_interaction(
            runtime,
            job,
            route,
            &context_epoch,
            &response.provider_node_id,
            &response.receipt,
        )?;
        checkpoint["rawMemo"] = Value::String(response.text.clone());
        job_module
            .save_checkpoint(&job.id, "understanding", &checkpoint)
            .map_err(ProviderError::local_state)?;
        let parsed = if v2 {
            let memo = crate::guide_memo::parse_guide_memo(&response.text)
                .map_err(ProviderError::invalid)?;
            let value = serde_json::to_value(&memo)
                .map_err(|error| ProviderError::invalid(error.to_string()))?;
            if let Some(digest) = job.payload.get("catalogDigest").and_then(Value::as_str) {
                let kind = job
                    .payload
                    .get("documentKind")
                    .and_then(Value::as_str)
                    .unwrap_or("paper");
                let _ = guide.save_memo(
                    digest,
                    &revision_id,
                    &ocr_revision_id,
                    kind,
                    &value,
                    Some(&response.text),
                );
            }
            value
        } else {
            crate::guide_validate::parse_guide_context(&response.text)
                .map_err(ProviderError::invalid)?
        };
        report_outline_progress(
            app,
            runtime,
            &job_module,
            job,
            "understanding",
            json!({"context": parsed}),
            Some(&response.receipt),
        )?;
        parsed
    };
    checkpoint["context"] = context.clone();
    let section_starts = if v2 {
        context
            .get("spans")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|span| span.get("pageStart").and_then(Value::as_i64))
            .collect::<Vec<_>>()
    } else {
        context
            .get("sections")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|section| section.get("pageStart").and_then(Value::as_i64))
            .collect::<Vec<_>>()
    };
    let v2_batches: Option<Vec<crate::guide_catalog::GuideBatchPlan>> = if v2 {
        Some(
            serde_json::from_value(job.payload["batches"].clone())
                .map_err(|e| ProviderError::invalid(format!("缺少有效的冻结批次：{e}")))?,
        )
    } else {
        None
    };
    let batches = if let Some(plans) = &v2_batches {
        plans
            .iter()
            .map(|plan| crate::guide_batching::GuideBatch {
                ordinal: plan.ordinal,
                page_start: plan.page_start,
                page_end: plan.page_end,
                block_ids: plan.anchor_block_ids.clone(),
            })
            .collect::<Vec<_>>()
    } else {
        crate::guide_batching::plan_guide_batches(&pages, &section_starts)
    };
    if batches.is_empty() {
        return Err(ProviderError::invalid(
            "Reading guide needs locatable OCR blocks",
        ));
    };
    let mut accepted = Vec::new();
    let mut warnings: Vec<String> = checkpoint
        .get("warnings")
        .cloned()
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    let mut failed_batches = checkpoint["failedBatches"].as_i64().unwrap_or(0);
    let mut dropped = checkpoint["dropped"].as_i64().unwrap_or(0);
    let completed = checkpoint
        .get("completedBatches")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(Value::as_i64)
        .collect::<std::collections::HashSet<_>>();
    let snapshot: Option<crate::guide_cast::GuideCastSnapshot> = job
        .payload
        .get("castSnapshot")
        .cloned()
        .and_then(|value| serde_json::from_value(value).ok());
    if let Some(saved_inks) = checkpoint.get("inks") {
        let restored = if let Some(cast) = &snapshot {
            crate::guide_validate_v2::restore_guide_inks_v2(
                saved_inks,
                &catalog_blocks,
                &cast.order,
            )
        } else {
            let outcome = crate::guide_validate::validate_guide_inks(saved_inks, &catalog_blocks);
            crate::guide_validate_v2::GuideV2Validation {
                inks: outcome.inks,
                dropped: outcome.dropped,
                warnings: outcome.warnings,
            }
        };
        accepted.extend(restored.inks);
        dropped += restored.dropped;
        warnings.extend(restored.warnings);
    }
    for batch in &batches {
        if completed.contains(&batch.ordinal) {
            continue;
        }
        report_outline_progress(
            app,
            runtime,
            &job_module,
            job,
            &format!("annotating {}/{}", batch.ordinal, batches.len()),
            json!({
                "batch": batch.ordinal,
                "batches": batches.len(),
                "step": batch.ordinal,
                "steps": batches.len()
            }),
            None,
        )?;
        if let (Some(plans), Some(cast)) = (&v2_batches, &snapshot) {
            cancellation.check()?;
            let material = plans
                .iter()
                .find(|p| p.ordinal == batch.ordinal)
                .ok_or_else(|| ProviderError::invalid("冻结批次不存在"))?;
            let memo: crate::guide_memo::GuideMemo = serde_json::from_value(context.clone())
                .map_err(|e| ProviderError::invalid(e.to_string()))?;
            let input = crate::guide_pipeline::BatchInput {
                batch: material,
                cast,
                memo: &memo,
                catalog: &catalog_blocks,
                model: &model,
                epoch: &context_epoch,
                prompt: &annotate_prompt,
                language: &output_language,
                reader: guide_reader_context.as_deref(),
                input_limit: job.payload["inputCharLimit"]
                    .as_u64()
                    .ok_or_else(|| ProviderError::invalid("缺少冻结输入预算"))?
                    as usize,
            };
            let result = crate::guide_pipeline::run_batch(
                &crate::guide_runtime::GuideJobPort(&adapter),
                &input,
                &accepted,
                &mut checkpoint,
                |c| {
                    job_module.save_checkpoint(&job.id, "annotating", c)?;
                    crate::guide_runtime::record_receipts(runtime, job, route, c)
                        .map_err(|e| e.to_string())
                },
            )
            .await?;
            crate::guide_runtime::record_receipts(runtime, job, route, &checkpoint)?;
            cancellation.check()?;
            accepted = result.inks;
            warnings.extend(result.warnings);
            dropped += result.dropped;
            let has_note=accepted.iter().any(|ink| matches!(ink,crate::guide_validate::GuideInk::Note{anchor,..} if material.anchor_block_ids.contains(&anchor.block_id)));
            if !has_note {
                failed_batches += 1;
                let mut missing = checkpoint["untreatedBlocks"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                missing.extend(material.anchor_block_ids.iter().map(|id| json!(id)));
                checkpoint["untreatedBlocks"] = json!(missing);
            }
            let mut done = checkpoint["completedBatches"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            done.push(json!(batch.ordinal));
            checkpoint["completedBatches"] = json!(done);
            checkpoint["inks"] = json!(accepted
                .iter()
                .map(crate::guide_validate::GuideInk::to_value)
                .collect::<Vec<_>>());
            checkpoint["warnings"] = json!(warnings);
            checkpoint["failedBatches"] = json!(failed_batches);
            checkpoint["dropped"] = json!(dropped);
            job_module
                .save_checkpoint(&job.id, "annotating", &checkpoint)
                .map_err(ProviderError::local_state)?;
            continue;
        }
        let batch_blocks: Vec<_> = catalog_blocks
            .iter()
            .filter(|block| batch.block_ids.iter().any(|id| id == &block.id))
            .cloned()
            .collect();
        let batch_catalog = if v2 {
            Value::Array(
                batch_blocks
                    .iter()
                    .map(|block| {
                        json!({
                            "blockId": block.id,
                            "parentBlockId": block.id,
                            "ref": format!("p{}-{}", block.page_number, block.block_index),
                            "pageNumber": block.page_number,
                            "blockIndex": block.block_index,
                            "blockType": block.block_type,
                            "bbox": block.bbox,
                            "role": "anchor",
                            "text": block.excerpt
                        })
                    })
                    .collect(),
            )
        } else {
            crate::guide_validate::locator_catalog_value(&batch_blocks)
        };
        let batch_context = if v2 {
            serde_json::from_value::<crate::guide_memo::GuideMemo>(context.clone())
                .map(|memo| {
                    crate::guide_memo::select_memo_for_batch(
                        &memo,
                        batch.page_start,
                        batch.page_end,
                    )
                })
                .unwrap_or_else(|_| context.clone())
        } else {
            crate::guide_validate::compact_guide_context(&context, batch.page_start, batch.page_end)
        };
        let cast_block = snapshot
            .as_ref()
            .map(|cast| json!({"order": cast.order, "block": crate::guide_cast::cast_prompt_block(cast)}));
        let request = |repair: Option<&str>| {
            TextInteractionRequest {
            model: model.clone(),
            context_epoch: context_epoch.clone(),
            system_instruction: annotate_prompt.clone(),
            user_input: reader_context::prepend_reader_context(
                &json!({
                    "task": if repair.is_some() { "repair_margin_inks" } else { "write_margin_inks" },
                    "context": batch_context,
                    "cast": cast_block,
                    "batch": {
                        "pageStart": batch.page_start,
                        "pageEnd": batch.page_end,
                        "anchorBlockIds": batch.block_ids,
                        "catalog": batch_catalog
                    },
                    "repairHint": repair
                })
                .to_string(),
                guide_reader_context.as_deref(),
            ),
            response_schema: Some(if let Some(cast) = &snapshot {
                crate::guide_protocol::inks_schema_v2(&cast.order)
            } else {
                crate::guide_protocol::inks_schema()
            }),
            kind: PaperInteractionKind::Artifact,
        }
        };
        let apply = |text: &str,
                     accepted: &mut Vec<crate::guide_validate::GuideInk>,
                     dropped: &mut i64,
                     warnings: &mut Vec<String>| {
            if let Some(cast) = &snapshot {
                apply_guide_batch_text_v2(
                    text,
                    &catalog_blocks,
                    cast,
                    &batch.block_ids,
                    batch.ordinal,
                    accepted,
                    dropped,
                    warnings,
                )
            } else {
                apply_guide_batch_text(text, &catalog_blocks, accepted, dropped, warnings)
            }
        };
        let mut batch_ok = false;
        match interact_text_with_retry(&adapter, request(None)).await {
            Ok(response) => {
                cancellation.check()?;
                record_route_interaction(
                    runtime,
                    job,
                    route,
                    &context_epoch,
                    &response.provider_node_id,
                    &response.receipt,
                )?;
                let mut decoded = apply(&response.text, &mut accepted, &mut dropped, &mut warnings);
                if !decoded {
                    match interact_text_with_retry(
                        &adapter,
                        request(Some(
                            if v2 {
                                "只返回 {\"inks\":[...]}。speakerId 必须属于本次冻结阵容。blockId 只能从本批目录复制。"
                            } else {
                                "Return only {\"inks\":[...]} . speakerId must be alin, laozhou, or xiaxia. Copy blockId or ref from the catalog."
                            },
                        )),
                    )
                    .await
                    {
                        Ok(repaired) => {
                            cancellation.check()?;
                            record_route_interaction(
                                runtime,
                                job,
                                route,
                                &context_epoch,
                                &repaired.provider_node_id,
                                &repaired.receipt,
                            )?;
                            decoded = apply(
                                &repaired.text,
                                &mut accepted,
                                &mut dropped,
                                &mut warnings,
                            );
                        }
                        Err(error) => {
                            warnings.push(format!("Batch {} repair failed: {error}", batch.ordinal));
                        }
                    }
                }
                if crate::guide_batching::batch_too_sparse(
                    batch.page_start,
                    batch.page_end,
                    if v2 {
                        count_guide_notes_on_pages(&accepted, batch.page_start, batch.page_end)
                    } else {
                        count_guide_inks_on_pages(&accepted, batch.page_start, batch.page_end)
                    },
                ) {
                    warnings.push(format!(
                        "Batch {} was sparse; asking for more locatable ink",
                        batch.ordinal
                    ));
                    match interact_text_with_retry(
                        &adapter,
                        request(Some(
                            if v2 {
                                "本批文字旁批不足。不要把 trace 算作文字覆盖。逐页补充遗漏的主 note，不要重写已有正确 note。speakerId 必须属于本次阵容。"
                            } else {
                                "This batch was too thin. Walk the catalog excerpts page by page and return several locatable notes plus traces. speakerId must be alin, laozhou, or xiaxia. Copy blockId or ref. Do not return a single summary card."
                            },
                        )),
                    )
                    .await
                    {
                        Ok(repaired) => {
                            cancellation.check()?;
                            record_route_interaction(
                                runtime,
                                job,
                                route,
                                &context_epoch,
                                &repaired.provider_node_id,
                                &repaired.receipt,
                            )?;
                            apply(
                                &repaired.text,
                                &mut accepted,
                                &mut dropped,
                                &mut warnings,
                            );
                        }
                        Err(error) => {
                            warnings.push(format!(
                                "Batch {} density repair failed: {error}",
                                batch.ordinal
                            ));
                        }
                    }
                }
                if decoded
                    || if v2 {
                        count_guide_notes_on_pages(&accepted, batch.page_start, batch.page_end) > 0
                    } else {
                        count_guide_inks_on_pages(&accepted, batch.page_start, batch.page_end) > 0
                    }
                {
                    batch_ok = true;
                } else {
                    failed_batches += 1;
                    warnings.push(format!("Batch {} produced no locatable ink", batch.ordinal));
                }
                report_outline_progress(
                    app,
                    runtime,
                    &job_module,
                    job,
                    &format!("annotating {}/{}", batch.ordinal, batches.len()),
                    json!({
                        "batch": batch.ordinal,
                        "batches": batches.len(),
                        "step": batch.ordinal,
                        "steps": batches.len()
                    }),
                    Some(&response.receipt),
                )?;
            }
            Err(error) => {
                failed_batches += 1;
                warnings.push(format!("Batch {} failed: {error}", batch.ordinal));
            }
        }
        if batch_ok {
            let mut done = checkpoint
                .get("completedBatches")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            done.push(json!(batch.ordinal));
            checkpoint["completedBatches"] = Value::Array(done);
            checkpoint["inks"] = json!(accepted
                .iter()
                .map(crate::guide_validate::GuideInk::to_value)
                .collect::<Vec<_>>());
            job_module
                .save_checkpoint(&job.id, "annotating", &checkpoint)
                .map_err(ProviderError::local_state)?;
        }
    }
    let (final_inks, final_dropped, final_warnings) = if let Some(cast) = &snapshot {
        let outcome = crate::guide_validate_v2::restore_guide_inks_v2(
            &json!(accepted
                .iter()
                .map(crate::guide_validate::GuideInk::to_value)
                .collect::<Vec<_>>()),
            &catalog_blocks,
            &cast.order,
        );
        (outcome.inks, outcome.dropped, outcome.warnings)
    } else {
        let outcome = crate::guide_validate::validate_guide_inks(
            &json!(accepted
                .iter()
                .map(crate::guide_validate::GuideInk::to_value)
                .collect::<Vec<_>>()),
            &catalog_blocks,
        );
        (outcome.inks, outcome.dropped, outcome.warnings)
    };
    dropped += final_dropped;
    warnings.extend(final_warnings);
    accepted = final_inks;
    if !v2 && accepted.is_empty() {
        if let Some(batch) = batches.first() {
            let batch_blocks: Vec<_> = catalog_blocks
                .iter()
                .filter(|block| batch.block_ids.iter().any(|id| id == &block.id))
                .cloned()
                .collect();
            warnings.push("All batches were empty; retrying the first batch".to_string());
            match interact_text_with_retry(
                &adapter,
                TextInteractionRequest {
                    model: model.clone(),
                    context_epoch: context_epoch.clone(),
                    system_instruction: annotate_prompt.clone(),
                    user_input: reader_context::prepend_reader_context(
                        &json!({
                            "task": "repair_margin_inks",
                            "context": crate::guide_validate::compact_guide_context(
                                &context,
                                batch.page_start,
                                batch.page_end,
                            ),
                            "batch": {
                                "pageStart": batch.page_start,
                                "pageEnd": batch.page_end,
                                "catalog": crate::guide_validate::locator_catalog_value(&batch_blocks)
                            },
                            "repairHint": "Previous output had no locatable ink. Return at least a few traces or notes. speakerId must be alin, laozhou, or xiaxia. Copy blockId or ref from the catalog."
                        })
                        .to_string(),
                        guide_reader_context.as_deref(),
                    ),
                    response_schema: Some(crate::guide_protocol::inks_schema()),
                    kind: PaperInteractionKind::Artifact,
                },
            )
            .await
            {
                Ok(salvaged) => {
                    cancellation.check()?;
                    record_route_interaction(
                        runtime,
                        job,
                        route,
                        &context_epoch,
                        &salvaged.provider_node_id,
                        &salvaged.receipt,
                    )?;
                    if let Ok(raw) =
                        crate::guide_validate::decode_inks_envelope(&salvaged.text)
                    {
                        let outcome = crate::guide_validate::validate_guide_inks(
                            &raw,
                            &catalog_blocks,
                        );
                        dropped += outcome.dropped;
                        warnings.extend(outcome.warnings);
                        accepted.extend(outcome.inks);
                    } else {
                        warnings.push("Salvage pass did not return inks".to_string());
                    }
                    report_outline_progress(
                        app,
            runtime,
                        &job_module,
                        job,
                        "annotating",
                        json!({"batch": batch.ordinal, "salvage": true}),
                        Some(&salvaged.receipt),
                    )?;
                }
                Err(error) => {
                    warnings.push(format!("Salvage pass failed: {error}"));
                }
            }
        }
    }
    if v2 && !crate::guide_generation::publishable_notes(&accepted) {
        return Err(ProviderError::invalid(
            "没有生成可用的文字旁批。仅有色笔痕迹不能作为成功旁批。".to_string(),
        ));
    }
    if accepted.is_empty() {
        let detail = warnings
            .iter()
            .take(8)
            .cloned()
            .collect::<Vec<_>>()
            .join(" · ");
        return Err(ProviderError::invalid(if detail.is_empty() {
            "Reading guide produced no locatable ink".to_string()
        } else {
            format!("Reading guide produced no locatable ink. {detail}")
        }));
    }
    cancellation.check()?;
    let coverage = if v2 {
        let memo: crate::guide_memo::GuideMemo = serde_json::from_value(context.clone())
            .map_err(|e| ProviderError::invalid(e.to_string()))?;
        let mut report = crate::guide_generation::page_coverage(
            &catalog_blocks,
            revision
                .page_count
                .unwrap_or_else(|| catalog.entries.iter().map(|b| b.page).max().unwrap_or(1)),
            Some(&memo),
            &accepted,
            failed_batches,
        );
        report.untreated_blocks = checkpoint["untreatedBlocks"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(String::from)
            .collect();
        report.material_gaps = report
            .pages
            .iter()
            .filter(|p| p.kind == "unreadable")
            .map(|p| format!("第 {} 页缺少可靠文字材料", p.page_number))
            .collect();
        let complete = failed_batches == 0
            && report.sparse_pages.is_empty()
            && report.material_gaps.is_empty();
        let mut value = json!(report);
        value["complete"] = json!(complete);
        value["batchCount"] = json!(batches.len());
        value["dropped"] = json!(dropped);
        value
    } else {
        json!({"complete":failed_batches==0,"batchCount":batches.len(),"failedBatches":failed_batches,"dropped":dropped})
    };
    let status = if coverage["complete"] == true {
        "published"
    } else {
        "partial"
    };
    report_outline_progress(
        app,
        runtime,
        &job_module,
        job,
        "publishing",
        json!({"coverage": coverage}),
        None,
    )?;
    let head = if v2 {
        let cast = job
            .payload
            .get("castSnapshot")
            .cloned()
            .unwrap_or_else(|| json!({}));
        guide
            .publish_v2(
                &revision_id,
                &ocr_revision_id,
                &model,
                &output_language,
                &context,
                &accepted,
                &coverage,
                &warnings,
                status,
                &cast,
                job.payload.get("publishId").and_then(Value::as_str),
            )
            .map_err(ProviderError::local_state)?
    } else {
        guide
            .publish(
                &revision_id,
                &ocr_revision_id,
                &model,
                &context,
                &accepted,
                reused_outline_id.as_deref(),
                &coverage,
                &warnings,
                status,
            )
            .map_err(ProviderError::local_state)?
    };
    job_module
        .complete(&job.id)
        .map_err(ProviderError::local_state)?;
    emit_job_event_for_runtime(app, runtime, &job.id);
    emit_read_event_for_runtime(
        app,
        runtime,
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "paper".to_string(),
            entity_id: Some(revision_id),
            delta: Some("reading_guide".to_string()),
            status: None,
        },
    );
    let _ = head.id;
    Ok(())
}

#[tauri::command]
fn get_outline_deep_dive(
    state: State<'_, AppState>,
    revision_id: String,
    node_id: String,
) -> AppResult<Option<outline_module::OutlineHeadProjection>> {
    active_outline_module(&state)?.deep_dive_for_node(&revision_id, &node_id)
}

#[tauri::command]
fn plan_outline(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: OutlineRequest,
) -> AppResult<OutlinePlan> {
    let current = capture_current_paper_read_route(&app, &state).map_err(|_| {
        "Configure and verify the current Provider before planning an Outline".to_string()
    })?;
    let runtime = current_runtime(&state)?;
    let kind = revision_document_kind_from_root(&runtime.root, &request.revision_id);
    let store = load_store_in(
        &prompt_settings_file(&app)?,
        resolve_ui_locale(&app)?.unwrap_or(ui_locale::UiLocale::ZhCn),
    )?;
    let paper_root = load_prompt_for_revision_from_root(
        &app,
        &runtime.root,
        &request.revision_id,
        PromptSlotId::PaperRoot,
        None,
    )?;
    crate::outline_plan::prepare_overview_plan(
        &runtime.outline_module,
        &store,
        &request.revision_id,
        kind,
        &current.route.frozen().route_id().database_value(),
        &current.model,
        current.supports_native_pdf,
        current.input_token_limit,
        &paper_root,
    )
}

static OUTLINE_START_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[tauri::command]
fn start_outline(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: OutlineRequest,
) -> AppResult<JobProjection> {
    let runtime = current_runtime(&state)?;
    let _guard = OUTLINE_START_LOCK.lock().map_err(|_| "地图启动锁不可用")?;
    if let Some(existing) = runtime
        .job_module
        .active_of("outline_overview", &request.revision_id)?
    {
        return Ok(existing);
    }
    if !runtime
        .job_module
        .list_active_of("outline_deep_dive", &request.revision_id)?
        .is_empty()
    {
        return Err("请等待局部图任务结束。".into());
    }
    let CurrentPaperReadRoute {
        route,
        model,
        supports_native_pdf,
        input_token_limit,
    } = capture_current_paper_read_route(&app, &state)?;
    if !supports_native_pdf {
        return Err("The current paper model does not support native PDF".to_string());
    }
    let outline = runtime.outline_module.clone();
    let kind = revision_document_kind_from_root(&runtime.root, &request.revision_id);
    let store = load_store_in(
        &prompt_settings_file(&app)?,
        resolve_ui_locale(&app)?.unwrap_or(ui_locale::UiLocale::ZhCn),
    )?;
    let bundle = crate::prompt_settings::outline_bundle_protocol(&store, kind)?;
    let paper_id = outline.paper_id_for_revision(&request.revision_id)?;
    let jobs = runtime.job_module.clone();
    let (payload, ocr_revision_id, dedupe_key) = if bundle == "v4" {
        let plan_id = request
            .plan_id
            .as_deref()
            .ok_or_else(|| "新版地图需要先确认冻结计划。".to_string())?;
        let payload = outline.load_plan(plan_id)?;
        let frozen = crate::outline_plan::frozen_plan_from_payload(&payload)?;
        if frozen.revision_id == request.revision_id
            && request.plan_digest.as_deref() == Some(frozen.plan_digest.as_str())
        {
            if let Some(id) = outline.job_for_plan(&request.revision_id, plan_id)? {
                return jobs.get(&id);
            }
        }
        if frozen.revision_id != request.revision_id
            || request.plan_digest.as_deref() != Some(frozen.plan_digest.as_str())
            || payload["inputTokenLimit"].as_i64() != input_token_limit
        {
            return Err("地图计划与当前文档或模型能力不符，请重新计划。".into());
        }
        let paper_root = load_prompt_for_revision_from_root(
            &app,
            &runtime.root,
            &request.revision_id,
            PromptSlotId::PaperRoot,
            None,
        )?;
        crate::outline_plan::verify_overview_plan(
            &outline,
            &store,
            &frozen,
            kind,
            &route.frozen().route_id().database_value(),
            &model,
            &paper_root,
        )?;
        let ocr = payload
            .get("ocrRevisionId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let digest = payload
            .get("planDigest")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        (
            payload,
            ocr.clone(),
            format!(
                "outline-overview-v4:{}:{}:{}",
                request.revision_id, ocr, digest
            ),
        )
    } else {
        let (ocr_revision_id, catalog) = outline.catalog_for_revision(&request.revision_id)?;
        let page_count = outline.revision_page_count(&request.revision_id)?;
        if outline_context_exceeds_window(page_count, catalog.token_estimate, input_token_limit) {
            return Err("Outline catalog and PDF exceed the model context window".to_string());
        }
        let extract = load_prompt_for_revision_from_root(
            &app,
            &runtime.root,
            &request.revision_id,
            PromptSlotId::OutlineExtract,
            None,
        )?;
        let compose = load_prompt_for_revision_from_root(
            &app,
            &runtime.root,
            &request.revision_id,
            PromptSlotId::OutlineCompose,
            None,
        )?;
        (
            json!({
                "ocrRevisionId": ocr_revision_id,
                "catalogDigest": catalog.digest,
                "outputLanguage": resolve_output_language(&app, None)?,
                "prompts": {
                    "extract": extract,
                    "compose": compose
                },
                "documentKind": kind.as_str()
            }),
            ocr_revision_id.clone(),
            format!(
                "outline-overview:{}:{}:{}+{}",
                request.revision_id,
                ocr_revision_id,
                crate::outline_protocol::EXTRACT_PROTOCOL,
                crate::outline_protocol::COMPOSE_PROTOCOL
            ),
        )
    };
    let enqueue = enqueue_paper_job(
        &jobs,
        JobSpec {
            kind: "outline_overview".to_string(),
            provider: None,
            paper_id: Some(paper_id),
            revision_id: Some(request.revision_id.clone()),
            root_key: Some(route_scoped_root_key(&request.revision_id, &route)),
            artifact_key: Some(format!("outline-overview:{ocr_revision_id}")),
            dedupe_key,
            priority: 80,
            payload,
        },
        &route,
    )?;
    emit_job_event_for_runtime(&app, &runtime, &enqueue.job.id);
    spawn_job_workers(app, runtime);
    Ok(enqueue.job)
}

async fn execute_outline_overview_v4_job(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
) -> Result<(), ProviderError> {
    let revision_id = job
        .revision_id
        .clone()
        .ok_or_else(|| ProviderError::local_state("Outline job has no revisionId"))?;
    let state = app.state::<AppState>();
    let job_module = runtime.job_module.clone();
    let outline = runtime.outline_module.clone();
    let model = route.frozen().models().paper().to_string();
    let route_id = route.frozen().route_id().database_value();
    let provider = route.frozen().provider_kind().as_str().to_string();
    let (ocr_revision_id, catalog) = outline
        .catalog_for_revision(&revision_id)
        .map_err(ProviderError::local_state)?;
    let _ = ocr_revision_id;
    let page_count = outline
        .revision_page_count(&revision_id)
        .map_err(ProviderError::local_state)?;
    let paper_id = outline
        .paper_id_for_revision(&revision_id)
        .map_err(ProviderError::local_state)?;
    let sha256 = outline
        .revision_sha256(&revision_id)
        .map_err(ProviderError::local_state)?;
    let connection = open_db(&runtime.root).map_err(ProviderError::local_state)?;
    let mut revision =
        revision_record(&connection, &revision_id).map_err(ProviderError::local_state)?;
    drop(connection);
    revision.pdf_path = absolute_pdf(&runtime.root, &revision.pdf_path);
    let facts = crate::reading_artifact_module::DocumentFacts {
        paper_id,
        revision_id: revision_id.clone(),
        revision_sha256: sha256,
        title: revision.title.clone(),
        pdf_path: revision.pdf_path.clone(),
    };
    let cancellation = CancellationFlag::default();
    if let Ok(mut locks) = state.artifact_cancellations.lock() {
        locks.insert(job.id.clone(), cancellation.clone());
    }
    let adapter = job_paper_model_adapter(
        route.adapter_arc(),
        job_module.clone(),
        job.id.clone(),
        cancellation.clone(),
    );
    let source = crate::reading_artifact_module::ReadingArtifactModule::open(&runtime.root)
        .map_err(ProviderError::local_state)?;
    let report = |stage: &str, extra: Value, receipt: Option<&UsageEnvelope>| {
        report_outline_progress(app, runtime, &job_module, job, stage, extra, receipt)
    };
    if let Some(checkpoint) = job_module
        .get_checkpoint(&job.id)
        .map_err(ProviderError::local_state)?
    {
        crate::outline_runtime::record_checkpoint_receipts(runtime, job, route, &checkpoint)?;
    }
    let mut record = |remote_id: &str, slot: &str, receipt: &UsageEnvelope| {
        crate::outline_runtime::record_receipt(runtime, job, route, remote_id, slot, receipt)
    };
    let journaled = crate::outline_runtime::OutlineJobPort(&adapter);
    crate::outline_generation::run_overview_v4(
        &source,
        &outline,
        &job_module,
        &journaled,
        job,
        &facts,
        &catalog,
        page_count,
        &route_id,
        &provider,
        &model,
        &cancellation,
        report,
        &mut record,
    )
    .await?;
    emit_read_event_for_runtime(
        app,
        runtime,
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "paper".to_string(),
            entity_id: Some(revision_id),
            delta: Some("outline".to_string()),
            status: None,
        },
    );
    Ok(())
}

async fn execute_outline_overview_job(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
) -> Result<(), ProviderError> {
    match crate::outline_map::job_protocol(&job.payload).map_err(ProviderError::invalid)? {
        crate::outline_map::OutlineJobProtocol::V4 => {
            return execute_outline_overview_v4_job(app, runtime, job, route).await;
        }
        crate::outline_map::OutlineJobProtocol::V3 => {}
    }
    let revision_id = job
        .revision_id
        .clone()
        .ok_or_else(|| ProviderError::local_state("Outline job has no revisionId"))?;
    let (extract_prompt, compose_prompt) =
        outline_prompts_from_payload(&job.payload).map_err(ProviderError::local_state)?;
    let state = app.state::<AppState>();
    let job_module = runtime.job_module.clone();
    let outline = runtime.outline_module.clone();
    let outline_kind = revision_document_kind_from_root(&runtime.root, &revision_id);
    let model = route.frozen().models().paper().to_string();
    let (ocr_revision_id, catalog) = outline
        .catalog_for_revision(&revision_id)
        .map_err(ProviderError::local_state)?;
    let allowed = catalog
        .entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<std::collections::HashSet<_>>();
    let root = runtime.root.clone();
    let connection = open_db(&root).map_err(ProviderError::local_state)?;
    let mut revision =
        revision_record(&connection, &revision_id).map_err(ProviderError::local_state)?;
    drop(connection);
    revision.pdf_path = absolute_pdf(&root, &revision.pdf_path);
    let context_epoch = route_scoped_context_epoch(
        &format!("{}:{}", revision.sha256, model),
        &route.frozen().route_id().database_value(),
    );
    let orientation = outline
        .orientation_context(&revision_id)
        .map_err(ProviderError::local_state)?;
    let cancellation = CancellationFlag::default();
    if let Ok(mut locks) = state.artifact_cancellations.lock() {
        locks.insert(job.id.clone(), cancellation.clone());
    }
    let adapter = job_paper_model_adapter(
        route.adapter_arc(),
        job_module.clone(),
        job.id.clone(),
        cancellation.clone(),
    );

    let checkpoint = job_module
        .get_checkpoint(&job.id)
        .map_err(ProviderError::local_state)?;
    let units = if let Some(saved) = checkpoint
        .as_ref()
        .and_then(|value| value.get("units"))
        .cloned()
    {
        serde_json::from_value(saved).map_err(|error| {
            ProviderError::invalid(format!("Invalid Outline unit checkpoint: {error}"))
        })?
    } else {
        report_outline_progress(
            app,
            runtime,
            &job_module,
            job,
            "extracting",
            json!({"ocrRevisionId": ocr_revision_id, "catalogDigest": catalog.digest}),
            None,
        )?;
        let extract = adapter
            .interact(PaperInteractionRequest {
                model: model.clone(),
                context_epoch: context_epoch.clone(),
                pdf_path: revision.pdf_path.clone(),
                display_name: revision.title.clone(),
                remote_file_id: None,
                previous_interaction_id: None,
                system_instruction: extract_prompt.clone(),
                user_input: json!({
                    "task": "extract_argument_units",
                    "catalog": catalog,
                    "orientation": orientation,
                    "language": job.payload.get("outputLanguage").and_then(Value::as_str).unwrap_or("zh-CN")
                })
                .to_string(),
                response_schema: Some(crate::outline_protocol::extract_schema()),
                inline_images: Vec::new(),
                kind: PaperInteractionKind::Artifact,
            })
            .await?;
        cancellation.check()?;
        record_route_interaction(
            runtime,
            job,
            route,
            &context_epoch,
            &extract.provider_node_id,
            &extract.receipt,
        )?;
        let parsed = crate::outline_protocol::parse_extract(&extract.text, outline_kind)
            .map_err(ProviderError::invalid)?;
        let units = crate::outline_validate::sanitize_units(parsed.units, &allowed);
        crate::outline_validate::validate_extract(&units).map_err(ProviderError::invalid)?;
        report_outline_progress(
            app,
            runtime,
            &job_module,
            job,
            "extracted",
            json!({
                "ocrRevisionId": ocr_revision_id,
                "catalogDigest": catalog.digest,
                "units": units,
                "unitCount": units.len()
            }),
            Some(&extract.receipt),
        )?;
        units
    };

    report_outline_progress(
        app,
        runtime,
        &job_module,
        job,
        "composing",
        json!({
            "ocrRevisionId": ocr_revision_id,
            "catalogDigest": catalog.digest,
            "units": units,
            "unitCount": units.len()
        }),
        None,
    )?;
    let compose_request = |repair: Option<&str>| PaperInteractionRequest {
        model: model.clone(),
        context_epoch: context_epoch.clone(),
        pdf_path: revision.pdf_path.clone(),
        display_name: revision.title.clone(),
        remote_file_id: None,
        previous_interaction_id: None,
        system_instruction: compose_prompt.clone(),
        user_input: json!({
            "task": if repair.is_some() { "repair_argument_map" } else { "compose_argument_map" },
            "units": units,
            "catalog": catalog,
            "repairHint": repair
        })
        .to_string(),
        response_schema: Some(crate::outline_protocol::compose_schema()),
        inline_images: Vec::new(),
        kind: PaperInteractionKind::Artifact,
    };
    let compose = adapter.interact(compose_request(None)).await?;
    cancellation.check()?;
    record_route_interaction(
        runtime,
        job,
        route,
        &context_epoch,
        &compose.provider_node_id,
        &compose.receipt,
    )?;
    report_outline_progress(
        app,
        runtime,
        &job_module,
        job,
        "composing",
        json!({"unitCount": units.len()}),
        Some(&compose.receipt),
    )?;
    let mut parsed_compose = crate::outline_protocol::parse_compose(&compose.text, outline_kind)
        .map_err(ProviderError::invalid)?;
    let mut outcome =
        crate::outline_validate::finish_overview(units.clone(), parsed_compose, &allowed);
    if matches!(
        outcome,
        crate::outline_validate::OutlineGraphOutcome::Partial { .. }
    ) {
        report_outline_progress(
            app,
            runtime,
            &job_module,
            job,
            "repairing",
            json!({"units": units, "ocrRevisionId": ocr_revision_id, "unitCount": units.len()}),
            None,
        )?;
        let repaired = adapter
            .interact(compose_request(Some(
                "Keep all valid units. Fix narrative into one weakly connected DAG. Do not return empty nodes.",
            )))
            .await?;
        cancellation.check()?;
        record_route_interaction(
            runtime,
            job,
            route,
            &context_epoch,
            &repaired.provider_node_id,
            &repaired.receipt,
        )?;
        report_outline_progress(
            app,
            runtime,
            &job_module,
            job,
            "repairing",
            json!({"unitCount": units.len()}),
            Some(&repaired.receipt),
        )?;
        parsed_compose = crate::outline_protocol::parse_compose(&repaired.text, outline_kind)
            .map_err(ProviderError::invalid)?;
        if parsed_compose.nodes.is_empty() {
            return Err(ProviderError::invalid(
                "Empty repaired nodes are not a successful Outline result",
            ));
        }
        outcome = crate::outline_validate::finish_overview(units.clone(), parsed_compose, &allowed);
    }

    let head = outline
        .publish_overview(
            &revision_id,
            &ocr_revision_id,
            &catalog.digest,
            &units,
            &outcome,
        )
        .map_err(ProviderError::local_state)?;
    report_outline_progress(
        app,
        runtime,
        &job_module,
        job,
        "published",
        json!({"outlineRevisionId": head.id, "status": head.status, "unitCount": units.len()}),
        None,
    )?;
    job_module
        .complete(&job.id)
        .map_err(ProviderError::local_state)?;
    emit_job_event_for_runtime(app, runtime, &job.id);
    emit_read_event_for_runtime(
        app,
        runtime,
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "paper".to_string(),
            entity_id: Some(revision_id),
            delta: Some("outline".to_string()),
            status: None,
        },
    );
    Ok(())
}

#[tauri::command]
fn plan_outline_deep_dive(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: OutlineDeepDiveRequest,
) -> AppResult<OutlinePlan> {
    let runtime = current_runtime(&state)?;
    let current = capture_current_paper_read_route(&app, &state)?;
    let kind = revision_document_kind_from_root(&runtime.root, &request.revision_id);
    let store = load_store_in(
        &prompt_settings_file(&app)?,
        resolve_ui_locale(&app)?.unwrap_or(ui_locale::UiLocale::ZhCn),
    )?;
    let root = load_prompt_text(&app, PromptSlotId::PaperRoot, kind, None)?;
    let payload = crate::outline_plan::prepare_deep_dive_plan(
        &runtime.outline_module,
        &store,
        &request.revision_id,
        &request.node_id,
        kind,
        &current.route.frozen().route_id().database_value(),
        &current.model,
        &root,
        current.supports_native_pdf,
        current.input_token_limit,
    )?;
    crate::outline_plan::project_local_plan(
        &runtime.outline_module,
        &payload,
        current.supports_native_pdf,
    )
}

#[tauri::command]
fn start_outline_deep_dive(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: OutlineDeepDiveRequest,
) -> AppResult<JobProjection> {
    let runtime = current_runtime(&state)?;
    let _guard = OUTLINE_START_LOCK.lock().map_err(|_| "地图启动锁不可用")?;
    if runtime
        .job_module
        .active_of("outline_overview", &request.revision_id)?
        .is_some()
    {
        return Err("请等待总图任务结束。".into());
    }
    if let Some(existing) = runtime
        .job_module
        .list_active_of("outline_deep_dive", &request.revision_id)?
        .into_iter()
        .find(|j| j.payload["nodeId"].as_str() == Some(request.node_id.as_str()))
    {
        return Ok(existing);
    }
    let current = capture_current_paper_read_route(&app, &state)?;
    if !current.supports_native_pdf {
        return Err("当前模型不支持原生 PDF。".into());
    }
    let route = current.route;
    let kind = revision_document_kind_from_root(&runtime.root, &request.revision_id);
    let store = load_store_in(
        &prompt_settings_file(&app)?,
        resolve_ui_locale(&app)?.unwrap_or(ui_locale::UiLocale::ZhCn),
    )?;
    let root = load_prompt_text(&app, PromptSlotId::PaperRoot, kind, None)?;
    let payload = if crate::prompt_settings::outline_bundle_protocol(&store, kind)? == "v4" {
        let plan = runtime
            .outline_module
            .load_plan(request.plan_id.as_deref().ok_or("请先确认局部图计划。")?)?;
        if plan["revisionId"].as_str() != Some(request.revision_id.as_str())
            || plan["nodeId"].as_str() != Some(request.node_id.as_str())
            || plan["planDigest"].as_str() != request.plan_digest.as_deref()
        {
            return Err("局部图计划与选择不符，请重新计划。".into());
        }
        if let Some(id) = runtime.outline_module.job_for_plan(
            &request.revision_id,
            request.plan_id.as_deref().unwrap_or(""),
        )? {
            return runtime.job_module.get(&id);
        }
        crate::outline_plan::verify_local_plan(
            &runtime.outline_module,
            &store,
            &plan,
            kind,
            &route.frozen().route_id().database_value(),
            &current.model,
            &root,
            current.input_token_limit,
        )?;
        plan
    } else {
        crate::outline_plan::prepare_deep_dive_plan(
            &runtime.outline_module,
            &store,
            &request.revision_id,
            &request.node_id,
            kind,
            &route.frozen().route_id().database_value(),
            &current.model,
            &root,
            current.supports_native_pdf,
            current.input_token_limit,
        )?
    };
    let parent = payload["overviewRevisionId"]
        .as_str()
        .ok_or("局部图缺少父版本")?
        .to_string();
    let enqueue = enqueue_paper_job(
        &runtime.job_module,
        JobSpec {
            kind: "outline_deep_dive".into(),
            provider: None,
            paper_id: Some(
                runtime
                    .outline_module
                    .paper_id_for_revision(&request.revision_id)?,
            ),
            revision_id: Some(request.revision_id.clone()),
            root_key: Some(route_scoped_root_key(&request.revision_id, &route)),
            artifact_key: Some(format!("outline-deep-dive:{parent}:{}", request.node_id)),
            dedupe_key: format!("outline-deep-dive:{parent}:{}", request.node_id),
            priority: 85,
            payload,
        },
        &route,
    )?;
    emit_job_event_for_runtime(&app, &runtime, &enqueue.job.id);
    spawn_job_workers(app, runtime);
    Ok(enqueue.job)
}

async fn execute_outline_deep_dive_v4_job(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
) -> Result<(), ProviderError> {
    let revision_id = job
        .revision_id
        .clone()
        .ok_or_else(|| ProviderError::local_state("Deep dive job has no revisionId"))?;
    let state = app.state::<AppState>();
    let job_module = runtime.job_module.clone();
    let outline = runtime.outline_module.clone();
    let model = route.frozen().models().paper().to_string();
    let route_id = route.frozen().route_id().database_value();
    let provider = route.frozen().provider_kind().as_str().to_string();
    let (_, catalog) = outline
        .catalog_for_revision(&revision_id)
        .map_err(ProviderError::local_state)?;
    let page_count = outline
        .revision_page_count(&revision_id)
        .map_err(ProviderError::local_state)?;
    let paper_id = outline
        .paper_id_for_revision(&revision_id)
        .map_err(ProviderError::local_state)?;
    let sha256 = outline
        .revision_sha256(&revision_id)
        .map_err(ProviderError::local_state)?;
    let connection = open_db(&runtime.root).map_err(ProviderError::local_state)?;
    let mut revision =
        revision_record(&connection, &revision_id).map_err(ProviderError::local_state)?;
    drop(connection);
    revision.pdf_path = absolute_pdf(&runtime.root, &revision.pdf_path);
    let facts = crate::reading_artifact_module::DocumentFacts {
        paper_id,
        revision_id: revision_id.clone(),
        revision_sha256: sha256,
        title: revision.title.clone(),
        pdf_path: revision.pdf_path.clone(),
    };
    let cancellation = CancellationFlag::default();
    if let Ok(mut locks) = state.artifact_cancellations.lock() {
        locks.insert(job.id.clone(), cancellation.clone());
    }
    let adapter = job_paper_model_adapter(
        route.adapter_arc(),
        job_module.clone(),
        job.id.clone(),
        cancellation.clone(),
    );
    let source = crate::reading_artifact_module::ReadingArtifactModule::open(&runtime.root)
        .map_err(ProviderError::local_state)?;
    let report = |stage: &str, extra: Value, receipt: Option<&UsageEnvelope>| {
        report_outline_progress(app, runtime, &job_module, job, stage, extra, receipt)
    };
    if let Some(checkpoint) = job_module
        .get_checkpoint(&job.id)
        .map_err(ProviderError::local_state)?
    {
        crate::outline_runtime::record_checkpoint_receipts(runtime, job, route, &checkpoint)?;
    }
    let mut record = |remote_id: &str, slot: &str, receipt: &UsageEnvelope| {
        crate::outline_runtime::record_receipt(runtime, job, route, remote_id, slot, receipt)
    };
    let journaled = crate::outline_runtime::OutlineJobPort(&adapter);
    crate::outline_generation::run_deep_dive_v4(
        &source,
        &outline,
        &job_module,
        &journaled,
        job,
        &facts,
        &catalog,
        page_count,
        &route_id,
        &provider,
        &model,
        &cancellation,
        report,
        &mut record,
    )
    .await?;
    emit_read_event_for_runtime(
        app,
        runtime,
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "paper".to_string(),
            entity_id: Some(revision_id),
            delta: Some("outline".to_string()),
            status: None,
        },
    );
    Ok(())
}

async fn execute_outline_deep_dive_job(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
) -> Result<(), ProviderError> {
    if crate::outline_map::job_protocol(&job.payload).map_err(ProviderError::invalid)?
        == crate::outline_map::OutlineJobProtocol::V4
    {
        return execute_outline_deep_dive_v4_job(app, runtime, job, route).await;
    }
    let revision_id = job
        .revision_id
        .clone()
        .ok_or_else(|| ProviderError::local_state("Deep dive job has no revisionId"))?;
    let node_id = job
        .payload
        .get("nodeId")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::local_state("Deep dive job has no nodeId"))?
        .to_string();
    let deep_dive_prompt =
        frozen_prompt_field(&job.payload, "deepDive").map_err(ProviderError::local_state)?;
    let state = app.state::<AppState>();
    let job_module = runtime.job_module.clone();
    let outline = runtime.outline_module.clone();
    let outline_kind = revision_document_kind_from_root(&runtime.root, &revision_id);
    let model = route.frozen().models().paper().to_string();
    let (ocr_revision_id, catalog) = outline
        .catalog_for_revision(&revision_id)
        .map_err(ProviderError::local_state)?;
    let graph = outline
        .current_graph(&revision_id)
        .map_err(ProviderError::local_state)?
        .ok_or_else(|| {
            ProviderError::local_state("Deep dive requires a published Overview graph")
        })?;
    let pages = crate::outline_catalog::outline_local_pages(&graph, &node_id, &catalog);
    let local_catalog = crate::outline_catalog::filter_catalog_to_pages(&catalog, &pages);
    let allowed = local_catalog
        .entries
        .iter()
        .map(|entry| entry.id.clone())
        .collect::<std::collections::HashSet<_>>();
    let root = runtime.root.clone();
    let connection = open_db(&root).map_err(ProviderError::local_state)?;
    let mut revision =
        revision_record(&connection, &revision_id).map_err(ProviderError::local_state)?;
    drop(connection);
    revision.pdf_path = absolute_pdf(&root, &revision.pdf_path);
    let context_epoch = route_scoped_context_epoch(
        &format!("{}:{}", revision.sha256, model),
        &route.frozen().route_id().database_value(),
    );
    let cancellation = CancellationFlag::default();
    if let Ok(mut locks) = state.artifact_cancellations.lock() {
        locks.insert(job.id.clone(), cancellation.clone());
    }
    let adapter = job_paper_model_adapter(
        route.adapter_arc(),
        job_module.clone(),
        job.id.clone(),
        cancellation.clone(),
    );
    report_outline_progress(
        app,
        runtime,
        &job_module,
        job,
        "composing",
        json!({"nodeId": node_id}),
        None,
    )?;
    let compose = adapter
        .interact(PaperInteractionRequest {
            model: model.clone(),
            context_epoch: context_epoch.clone(),
            pdf_path: revision.pdf_path.clone(),
            display_name: revision.title.clone(),
            remote_file_id: None,
            previous_interaction_id: None,
            system_instruction: deep_dive_prompt.clone(),
            user_input: json!({
                "task": "compose_deep_dive",
                "nodeId": node_id,
                "overview": graph,
                "catalog": local_catalog
            })
            .to_string(),
            response_schema: Some(crate::outline_protocol::compose_schema()),
            inline_images: Vec::new(),
            kind: PaperInteractionKind::Artifact,
        })
        .await?;
    cancellation.check()?;
    record_route_interaction(
        runtime,
        job,
        route,
        &context_epoch,
        &compose.provider_node_id,
        &compose.receipt,
    )?;
    report_outline_progress(
        app,
        runtime,
        &job_module,
        job,
        "composing",
        json!({"nodeId": node_id}),
        Some(&compose.receipt),
    )?;
    let parsed = crate::outline_protocol::parse_compose(&compose.text, outline_kind)
        .map_err(ProviderError::invalid)?;
    let units = parsed
        .nodes
        .iter()
        .map(|node| crate::outline_protocol::OutlineUnit {
            unit_id: node.node_id.clone(),
            role_class: node.role_class.clone(),
            role_label: node.role_label.clone(),
            title: node.title.clone(),
            takeaway: node.takeaway.clone(),
            importance: node.importance.clone(),
            evidence_ids: node.evidence_ids.clone(),
            confidence: node.confidence,
        })
        .collect::<Vec<_>>();
    let outcome = crate::outline_validate::finish_overview(units.clone(), parsed, &allowed);
    outline
        .publish_deep_dive(
            &revision_id,
            &node_id,
            &ocr_revision_id,
            &local_catalog.digest,
            &units,
            &outcome,
        )
        .map_err(ProviderError::local_state)?;
    report_outline_progress(
        app,
        runtime,
        &job_module,
        job,
        "published",
        json!({"nodeId": node_id}),
        None,
    )?;
    job_module
        .complete(&job.id)
        .map_err(ProviderError::local_state)?;
    emit_job_event_for_runtime(app, runtime, &job.id);
    emit_read_event_for_runtime(
        app,
        runtime,
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "paper".to_string(),
            entity_id: Some(revision_id),
            delta: Some("outline".to_string()),
            status: None,
        },
    );
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GenerateDocumentArtifactRequest {
    revision_id: String,
    kind: document_artifacts::DocumentArtifactKind,
    #[serde(default)]
    use_brief: bool,
}

#[tauri::command]
fn generate_document_artifact(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: GenerateDocumentArtifactRequest,
) -> AppResult<JobProjection> {
    enqueue_document_artifact(app, state, request)
}

#[tauri::command]
fn generate_brief(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    revision_id: String,
) -> AppResult<JobProjection> {
    enqueue_document_artifact(
        app,
        state,
        GenerateDocumentArtifactRequest {
            revision_id,
            kind: document_artifacts::DocumentArtifactKind::Brief,
            use_brief: false,
        },
    )
}

fn enqueue_document_artifact(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: GenerateDocumentArtifactRequest,
) -> AppResult<JobProjection> {
    let GenerateDocumentArtifactRequest {
        revision_id,
        kind,
        use_brief,
    } = request;
    if use_brief && kind != document_artifacts::DocumentArtifactKind::Glossary {
        return Err("只有术语表可选择参考 Brief".to_string());
    }
    let runtime = current_runtime(&state)?;
    let current = require_current_paper_provider(&app, &state)?;
    let model = normalize_paper_model_id(&current.provider, &current.paper_model)?;
    let root = runtime.root.clone();
    let translation_model =
        normalize_paper_model_id(&current.provider, &current.translation_model)?;
    let route = capture_current_paper_job_route(
        &app,
        &state,
        &current,
        &model,
        Some(&translation_model),
        ModelRole::Paper,
    )?;
    let connection = open_db(&root)?;
    let paper_id: String = connection
        .query_row(
            "SELECT paper_id FROM document_revisions WHERE id = ?1",
            params![revision_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let page_count: Option<i64> = connection
        .query_row(
            "SELECT page_count FROM document_revisions WHERE id = ?1",
            params![revision_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| error.to_string())?
        .flatten();
    drop(connection);
    ensure_long_pdf_acknowledged(&runtime, &revision_id, page_count)?;
    let brief_source = if use_brief {
        let brief = runtime
            .artifact_module
            .head_for_revision(&revision_id, "brief", "")?
            .ok_or("当前文档没有可参考的 Brief")?;
        Some(document_artifacts::brief_hints(&brief))
    } else {
        None
    };
    let document_kind = revision_document_kind_from_root(&runtime.root, &revision_id);
    let locale = resolve_ui_locale(&app)?.unwrap_or(ui_locale::UiLocale::ZhCn);
    let prompt_store = load_store_in(&prompt_settings_file(&app)?, locale)?;
    let protocol = prompt_settings::resolved_protocol(&prompt_store, kind.slot(), document_kind);
    let textbook_brief = kind == document_artifacts::DocumentArtifactKind::Brief
        && document_kind == DocumentKind::Textbook
        && protocol == textbook_contract::BRIEF_PROTOCOL;
    if kind == document_artifacts::DocumentArtifactKind::Brief
        && !["v1", textbook_contract::BRIEF_PROTOCOL].contains(&protocol.as_str())
    {
        return Err("未知 Brief 协议".into());
    }
    let frozen_schema = if textbook_brief {
        textbook_contract::response_schema()
    } else if protocol == "v2" {
        auxiliary_contract::response_schema(kind)
    } else {
        document_artifacts::response_schema(kind, document_kind == DocumentKind::Textbook)
    };
    let payload = json!({
        "documentArtifactProtocol": if textbook_brief { "v1" } else { protocol.as_str() },
        "briefProtocol": if textbook_brief { Some(textbook_contract::BRIEF_PROTOCOL) } else { None },
        "responseSchema": frozen_schema,
        "documentArtifactKind": kind,
        "briefSource": brief_source,
        "prompts": {
            "orientation": load_prompt_text(&app, kind.slot(), document_kind, None)?,
            "paperRoot": load_prompt_text(&app, PromptSlotId::PaperRoot, document_kind, None)?,
            "documentKind": document_kind.as_str()
        }
    });
    let enqueue = enqueue_paper_job(
        &runtime.job_module,
        JobSpec {
            kind: if kind == document_artifacts::DocumentArtifactKind::Brief {
                "orientation_pack"
            } else {
                "document_artifact"
            }
            .to_string(),
            provider: None,
            paper_id: Some(paper_id),
            revision_id: Some(revision_id.clone()),
            root_key: Some(route_scoped_root_key(&revision_id, &route)),
            artifact_key: Some(kind.as_str().to_string()),
            dedupe_key: if kind == document_artifacts::DocumentArtifactKind::Brief {
                format!("orientation:{revision_id}")
            } else {
                format!("document-artifact:{}:{revision_id}", kind.as_str())
            },
            priority: 90,
            payload,
        },
        &route,
    )?;
    emit_job_event_for_runtime(&app, &runtime, &enqueue.job.id);
    spawn_job_workers(app, runtime);
    Ok(enqueue.job)
}

async fn execute_orientation_job(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
) -> Result<(), ProviderError> {
    let revision_id = job
        .revision_id
        .clone()
        .ok_or_else(|| ProviderError::local_state("Orientation job has no revisionId"))?;
    let job_module = runtime.job_module.clone();
    let brief = publish_orientation_pack(app.clone(), runtime, job, route)
        .await
        .map_err(ProviderError::local_state)?;
    job_module
        .save_checkpoint(
            &job.id,
            "published",
            &json!({"briefVersion": brief.version, "revisionId": revision_id}),
        )
        .map_err(ProviderError::local_state)?;
    job_module
        .complete(&job.id)
        .map_err(ProviderError::local_state)?;
    emit_job_event_for_runtime(app, runtime, &job.id);
    emit_read_event_for_runtime(
        app,
        runtime,
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "paper".to_string(),
            entity_id: Some(revision_id),
            delta: None,
            status: None,
        },
    );
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartRoadmapJobRequest {
    pub revision_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleRoadmapTaskRequest {
    pub paper_id: String,
    pub roadmap_id: String,
    pub task_id: String,
    pub completed: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListRoadmapProgressRequest {
    pub paper_id: String,
    pub roadmap_id: String,
}

#[tauri::command]
fn start_roadmap_job(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: StartRoadmapJobRequest,
) -> AppResult<JobProjection> {
    let runtime = current_runtime(&state)?;
    let current = require_current_paper_provider(&app, &state)?;
    let model = normalize_paper_model_id(&current.provider, &current.paper_model)?;
    let root = runtime.root.clone();
    let translation_model =
        normalize_paper_model_id(&current.provider, &current.translation_model)?;
    let route = capture_current_paper_job_route(
        &app,
        &state,
        &current,
        &model,
        Some(&translation_model),
        ModelRole::Paper,
    )?;
    let connection = open_db(&root)?;
    let paper_id: String = connection
        .query_row(
            "SELECT paper_id FROM document_revisions WHERE id = ?1",
            params![request.revision_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    drop(connection);
    let document_kind = revision_document_kind_from_root(&root, &request.revision_id);
    let locale = resolve_ui_locale(&app)?.unwrap_or(ui_locale::UiLocale::ZhCn);
    let _prompt_store = load_store_in(&prompt_settings_file(&app)?, locale)?;
    let brief_source = runtime.artifact_module
        .head_for_revision(&request.revision_id, "brief", "")?
        .map(|brief| json!({"artifactId": brief.id, "version": brief.version, "content": brief.content}));
    let payload = json!({
        "prompts": {
            "roadmap": load_prompt_text(&app, PromptSlotId::ReadingRoadmap, document_kind, None)?,
            "paperRoot": load_prompt_text(&app, PromptSlotId::PaperRoot, document_kind, None)?,
            "documentKind": document_kind.as_str()
        },
        "outputLanguage": resolve_output_language(&app, None)?,
        "responseSchema": roadmap_module::response_schema(document_kind),
        "briefSource": brief_source,
        "readerContext": resolve_reader_wrapper_for_revision(&root, &request.revision_id)
    });
    let enqueue = enqueue_paper_job(
        &runtime.job_module,
        JobSpec {
            kind: "reading_roadmap".to_string(),
            provider: None,
            paper_id: Some(paper_id),
            revision_id: Some(request.revision_id.clone()),
            root_key: Some(route_scoped_root_key(&request.revision_id, &route)),
            artifact_key: Some("reading_roadmap".to_string()),
            dedupe_key: format!("roadmap:{}", request.revision_id),
            priority: 85,
            payload,
        },
        &route,
    )?;
    emit_job_event_for_runtime(&app, &runtime, &enqueue.job.id);
    spawn_job_workers(app, runtime);
    Ok(enqueue.job)
}

#[tauri::command]
fn get_roadmap(
    state: State<'_, AppState>,
    revision_id: String,
) -> AppResult<Option<serde_json::Value>> {
    let root = active_root(&state)?;
    let connection = open_db(&root)?;
    let row: Option<(String, String, String, String, String, String)> = connection
        .query_row(
            "SELECT a.id, a.paper_id, a.revision_id, a.status, a.content_json, a.created_at
             FROM artifact_heads h
             JOIN artifacts a ON a.id = h.artifact_id
             WHERE a.revision_id = ?1 AND h.kind = 'reading_roadmap'",
            params![revision_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .optional()
        .map_err(|e| e.to_string())?;

    if let Some((id, paper_id, rev_id, status, content_json, created_at)) = row {
        let content: serde_json::Value =
            serde_json::from_str(&content_json).unwrap_or(serde_json::Value::Null);
        Ok(Some(json!({
            "id": id,
            "paperId": paper_id,
            "revisionId": rev_id,
            "status": status,
            "content": content,
            "activeJobId": null,
            "lastError": null,
            "createdAt": created_at
        })))
    } else {
        Ok(None)
    }
}

#[tauri::command]
fn toggle_roadmap_task(
    state: State<'_, AppState>,
    request: ToggleRoadmapTaskRequest,
) -> AppResult<()> {
    let root = active_root(&state)?;
    let connection = open_db(&root)?;
    roadmap_module::set_task_progress(
        &connection,
        &request.paper_id,
        &request.roadmap_id,
        &request.task_id,
        request.completed,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn list_roadmap_progress(
    state: State<'_, AppState>,
    request: ListRoadmapProgressRequest,
) -> AppResult<Vec<roadmap_module::RoadmapProgressRow>> {
    let root = active_root(&state)?;
    let connection = open_db(&root)?;
    roadmap_module::list_progress(&connection, &request.paper_id, &request.roadmap_id)
        .map_err(|e| e.to_string())
}

async fn execute_roadmap_job(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
) -> Result<(), ProviderError> {
    let revision_id = job
        .revision_id
        .clone()
        .ok_or_else(|| ProviderError::local_state("Roadmap job has no revisionId"))?;
    let job_module = runtime.job_module.clone();
    let artifact = publish_reading_roadmap(app.clone(), runtime, job, route)
        .await
        .map_err(ProviderError::local_state)?;
    job_module
        .save_checkpoint(
            &job.id,
            "published",
            &json!({"roadmapVersion": artifact.version, "revisionId": revision_id}),
        )
        .map_err(ProviderError::local_state)?;
    job_module
        .complete(&job.id)
        .map_err(ProviderError::local_state)?;
    emit_job_event_for_runtime(app, runtime, &job.id);
    emit_read_event_for_runtime(
        app,
        runtime,
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "paper".to_string(),
            entity_id: Some(revision_id),
            delta: None,
            status: None,
        },
    );
    Ok(())
}

async fn publish_reading_roadmap(
    app: tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
) -> AppResult<ArtifactProjection> {
    let revision_id = job
        .revision_id
        .as_deref()
        .ok_or("Roadmap job has no revisionId")?;
    let (mut revision, paper_id) = {
        let connection = open_db(&runtime.root)?;
        let revision = revision_record(&connection, revision_id)?;
        let paper_id: String = connection
            .query_row(
                "SELECT paper_id FROM document_revisions WHERE id = ?1",
                [revision_id],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        (revision, paper_id)
    };
    revision.pdf_path = absolute_pdf(&runtime.root, &revision.pdf_path);
    let document_kind = match job.payload["prompts"]["documentKind"].as_str() {
        Some("paper") => DocumentKind::Paper,
        Some("textbook") => DocumentKind::Textbook,
        None => revision_document_kind_from_root(&runtime.root, revision_id),
        Some(_) => return Err("Unsupported frozen roadmap document kind".into()),
    };
    let system_instruction =
        frozen_prompt_field(&job.payload, "roadmap").or_else(|_| -> AppResult<String> {
            Ok(prompt_settings::default_text_for_kind_in(
                PromptSlotId::ReadingRoadmap,
                document_kind,
                if job.payload["outputLanguage"].as_str() == Some("en") {
                    ui_locale::UiLocale::En
                } else {
                    ui_locale::UiLocale::ZhCn
                },
            ))
        })?;
    let paper_root_instruction =
        frozen_prompt_field(&job.payload, "paperRoot").or_else(|_| -> AppResult<String> {
            Ok(prompt_settings::default_text_for_kind_in(
                PromptSlotId::PaperRoot,
                document_kind,
                if job.payload["outputLanguage"].as_str() == Some("en") {
                    ui_locale::UiLocale::En
                } else {
                    ui_locale::UiLocale::ZhCn
                },
            ))
        })?;
    // New jobs freeze the presence/absence and exact content of the optional Brief.
    // Legacy jobs without this key retain their previous read-at-execution behavior.
    let legacy_brief;
    let brief_source = if let Some(value) = job.payload.get("briefSource") {
        (!value.is_null()).then_some(value)
    } else {
        legacy_brief = runtime.artifact_module.head_for_revision(revision_id, "brief", "")?
            .map(|brief| json!({"artifactId": brief.id, "version": brief.version, "content": brief.content}));
        legacy_brief.as_ref()
    };
    let schema = job
        .payload
        .get("responseSchema")
        .cloned()
        .unwrap_or_else(|| roadmap_module::response_schema(document_kind));
    let adapter = job_paper_model_adapter(
        route.adapter_arc(),
        runtime.job_module.clone(),
        job.id.clone(),
        CancellationFlag::default(),
    );
    roadmap_module::generate(
        &runtime.root,
        &adapter,
        &runtime.job_module,
        roadmap_module::GenerationRequest {
            job_id: job.id.clone(),
            facts: reading_artifact_module::DocumentFacts {
                paper_id,
                revision_id: revision.id,
                revision_sha256: revision.sha256,
                title: revision.title,
                pdf_path: revision.pdf_path,
            },
            page_count: revision.page_count,
            provider: route.frozen().provider_kind().as_str().to_string(),
            route_id: route.frozen().route_id().database_value(),
            model: route.frozen().models().paper().to_string(),
            call: reading_artifact_module::PaperRootCall {
                provider: route.frozen().provider_kind().as_str().to_string(),
                model: route.frozen().models().paper().to_string(),
                system_instruction,
                user_input: roadmap_module::task_input(
                    brief_source,
                    frozen_reader_context(&job.payload).as_deref(),
                    job.payload
                        .get("outputLanguage")
                        .and_then(Value::as_str)
                        .unwrap_or("zh-CN"),
                ),
                response_schema: schema,
                inline_images: Vec::new(),
                paper_root_instruction: Some(paper_root_instruction),
            },
        },
    )
    .await
}

pub(crate) fn diagnostic_preview_for_root(root: &Path) -> AppResult<DiagnosticPreview> {
    let connection = open_db(root)?;
    let count = |sql: &str| {
        connection
            .query_row(sql, [], |row| row.get::<_, i64>(0))
            .map_err(|error| error.to_string())
    };
    let schema_version = connection
        .query_row(
            "SELECT value FROM schema_meta WHERE key = 'schema_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| error.to_string())?;
    let storage = PaperModule::open(root)?.storage_report()?;
    Ok(DiagnosticPreview {
        generated_at: now(),
        included_sections: vec![
            "application and schema versions".to_string(),
            "aggregate library, Discussion, Artifact, OCR, and Job counts".to_string(),
            "aggregate Job states and pending cleanup/conflict counts".to_string(),
            "Workspace total, PDF total, and internal total byte counts".to_string(),
        ],
        excluded_data: vec![
            "API keys, credentials, and authorization headers".to_string(),
            "PDF bytes, Prompt text, OCR/Block text, and Discussion content".to_string(),
            "absolute paths, file names, paper titles, and user metadata".to_string(),
            "provider raw responses, signed URLs, provider IDs, and task error text".to_string(),
        ],
        summary: json!({
            "applicationVersion": env!("CARGO_PKG_VERSION"),
            "schemaVersion": schema_version,
            "counts": {
                "papers": count("SELECT COUNT(*) FROM papers WHERE deleted_at IS NULL")?,
                "documentRevisions": count("SELECT COUNT(*) FROM document_revisions")?,
                "discussions": count("SELECT COUNT(*) FROM discussions")?,
                "messages": count("SELECT COUNT(*) FROM messages")?,
                "ocrRevisions": count("SELECT COUNT(*) FROM ocr_revisions")?,
                "artifacts": count("SELECT COUNT(*) FROM artifacts")?,
                "jobs": count("SELECT COUNT(*) FROM jobs")?,
                "jobsQueued": count("SELECT COUNT(*) FROM jobs WHERE state = 'queued'")?,
                "jobsRunning": count("SELECT COUNT(*) FROM jobs WHERE state = 'running'")?,
                "jobsPaused": count("SELECT COUNT(*) FROM jobs WHERE state = 'paused'")?,
                "jobsFailed": count("SELECT COUNT(*) FROM jobs WHERE state IN ('failed', 'interrupted_unknown')")?,
                "pendingRemoteCleanup": count("SELECT COUNT(*) FROM remote_tombstones WHERE state != 'resolved'")?,
                "unresolvedConflicts": count("SELECT COUNT(*) FROM reconciliation_conflicts WHERE resolved_at IS NULL")?,
                "trashEntries": count("SELECT COUNT(*) FROM trash_entries")?
            },
            "storageBytes": {
                "workspace": storage.workspace_bytes,
                "papers": storage.papers_bytes,
                "internal": storage.internal_bytes
            }
        }),
    })
}

#[tauri::command]
fn preview_diagnostics(state: State<'_, AppState>) -> AppResult<DiagnosticPreview> {
    diagnostic_preview_for_root(&active_root(&state)?)
}

#[tauri::command]
fn export_diagnostics(
    state: State<'_, AppState>,
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
    let preview = diagnostic_preview_for_root(&active_root(&state)?)?;
    let payload = serde_json::to_vec_pretty(&json!({
        "formatVersion": 1,
        "diagnostic": &preview
    }))
    .map_err(|error| error.to_string())?;
    fs::write(&output, payload)
        .map_err(|error| format!("Unable to export diagnostics: {error}"))?;
    Ok(preview)
}

#[tauri::command]
fn get_stats(state: State<'_, AppState>) -> AppResult<AppStats> {
    let root = active_root(&state)?;
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
        // Provider pricing is model- and epoch-dependent. Keep cost unknown until
        // a provider receipt supplies a decimal amount instead of estimating it.
        estimated_cost: None,
    })
}

struct TrayLocaleMenu {
    show: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

fn refresh_tray_locale(app: &tauri::AppHandle, locale: ui_locale::UiLocale) {
    if let Some(menu) = app.try_state::<TrayLocaleMenu>() {
        let _ = menu.show.set_text(ui_locale::message(
            Some(locale),
            "打开 Read Atlas",
            "Open Read Atlas",
        ));
        let _ = menu
            .quit
            .set_text(ui_locale::message(Some(locale), "退出", "Quit"));
    }
}

fn install_tray(app: &mut tauri::App) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "tray_show", "Open Read Atlas", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "tray_quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &quit])?;
    app.manage(TrayLocaleMenu { show, quit });
    refresh_tray_locale(
        app.handle(),
        resolve_ui_locale(app.handle())
            .ok()
            .flatten()
            .unwrap_or_default(),
    );
    let mut tray = TrayIconBuilder::with_id("read-desktop-tray")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("Read Atlas")
        .on_menu_event(|app, event| match event.id().as_ref() {
            "tray_show" => show_main_window(app),
            "tray_quit" => request_exit_or_prompt(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(event, TrayIconEvent::DoubleClick { .. }) {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }
    tray.build(app)?;
    Ok(())
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            install_tray(app)?;
            webview_pinch::install(app);
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle();
                let state = app.state::<AppState>();
                if state.exit_authorized.load(Ordering::Acquire) {
                    return;
                }
                api.prevent_close();
                request_exit_or_prompt(app);
            }
        })
        .invoke_handler(tauri::generate_handler![
            set_window_theme,
            get_workspace,
            choose_workspace,
            open_workspace_dir,
            inspect_legacy_reset,
            execute_legacy_reset,
            get_model_settings,
            get_prompt_settings,
            get_ui_locale,
            set_ui_locale,
            save_prompt_slot,
            restore_prompt_previous,
            restore_prompt_default,
            restore_outline_prompt_bundle,
            get_guide_character_settings,
            save_guide_character,
            delete_guide_character,
            restore_guide_character_preset,
            duplicate_guide_character,
            save_guide_default_cast,
            save_guide_preset_casts,
            restore_guide_factory_preset_casts,
            import_guide_character_avatar,
            get_guide_character_avatar,
            preview_guide_character,
            cancel_guide_character_preview,
            get_reader_context,
            save_reader_context,
            restore_reader_folder_context,
            add_provider,
            remove_provider,
            rename_provider,
            duplicate_provider,
            reorder_providers,
            test_provider_connection,
            save_provider_settings,
            clear_provider_credential,
            set_current_provider,
            save_mistral_credential,
            clear_mistral_credential,
            list_documents,
            library_read,
            library_act,
            list_collections,
            create_collection,
            rename_collection,
            move_collection,
            trash_collection,
            reorder_collection_papers,
            set_collection_sort_mode,
            rename_paper,
            import_pdf,
            open_library,
            move_paper,
            export_reading_bundle,
            reconcile_library,
            open_pdf_external,
            delete_revision,
            open_resource_dir,
            open_api_key_page,
            restore_paper,
            list_threads,
            list_archived_threads,
            start_ocr,
            ack_long_pdf_warning,
            get_reading_state,
            save_reading_state,
            list_annotations,
            create_annotation,
            update_annotation,
            delete_annotation,
            list_annotation_links,
            create_annotation_link,
            delete_annotation_link,
            retry_job,
            get_storage_report,
            list_trash,
            list_artifacts,
            get_artifact,
            delete_artifact,
            get_artifact_asset_path,
            latest_ocr,
            get_ocr,
            delete_ocr_cascade,
            update_paper_tags,
            update_paper_metadata,
            update_orientation_table,
            set_artifact_override,
            list_lens_qa,
            generate_reading_artifact,
            ask_lens,
            list_jobs,
            list_remote_tombstones,
            retry_remote_cleanup,
            abandon_remote_cleanup,
            recheck_provider_job,
            rebind_provider_job,
            abandon_legacy_provider_job,
            pause_job,
            resume_job,
            reprioritize_job,
            cancel_job,
            resolve_exit_intent,
            create_thread,
            list_messages,
            send_chat,
            cancel_generation,
            create_branch,
            set_active_branch,
            delete_discussion_turn,
            archive_thread,
            close_thread,
            restore_thread,
            delete_thread,
            rename_thread,
            get_brief,
            get_outline,
            get_outline_deep_dive,
            plan_outline,
            plan_outline_deep_dive,
            start_outline,
            start_outline_deep_dive,
            delete_outline,
            delete_outline_deep_dive,
            get_reading_guide,
            plan_reading_guide,
            start_reading_guide,
            delete_reading_guide,
            generate_brief,
            generate_document_artifact,
            get_roadmap,
            toggle_roadmap_task,
            list_roadmap_progress,
            start_roadmap_job,
            preview_diagnostics,
            export_diagnostics,
            get_stats
        ])
        .run(tauri::generate_context!())
        .expect("error while running Read Atlas");
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BlockingFailureExecution {
        calls: std::sync::atomic::AtomicUsize,
        entered: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<u64>>>,
        release: std::sync::Mutex<Option<tokio::sync::oneshot::Receiver<()>>>,
        error: ProviderError,
    }

    #[async_trait::async_trait]
    impl JobExecutionPort for BlockingFailureExecution {
        async fn execute(
            &self,
            claim: &workspace_lifecycle::RuntimeJobClaim,
        ) -> Result<(), ProviderError> {
            let call = self.calls.fetch_add(1, Ordering::AcqRel);
            if call == 0 {
                self.entered
                    .lock()
                    .expect("entered lock")
                    .take()
                    .expect("entered sender")
                    .send(claim.runtime.generation)
                    .expect("report claimed runtime");
                let release = self
                    .release
                    .lock()
                    .expect("release lock")
                    .take()
                    .expect("release receiver");
                release.await.expect("release worker");
            }
            Err(self.error.clone())
        }
    }

    struct TestRouteCredential {
        id: String,
        key: String,
    }

    impl crate::provider_routing::ProviderCredentialPort for TestRouteCredential {
        fn read_exact(
            &self,
            instance_id: &crate::provider_routing::ProviderInstanceId,
        ) -> Result<Option<String>, crate::provider_routing::ProviderRoutingError> {
            Ok((instance_id.as_str() == self.id).then(|| self.key.clone()))
        }
    }

    fn worker_test_paper_route() -> BoundProviderRoute {
        let instance_id = "11111111-1111-4111-8111-111111111111";
        let api_key = "worker-test-exact-key";
        let paper_model = "gpt-4.1".to_string();
        let translation_model = "gpt-4.1-mini".to_string();
        let base_url = "https://example.invalid/v1";
        let settings = StoredModelSettings {
            schema_version: model_settings::MODEL_SETTINGS_SCHEMA,
            current_provider_id: Some(instance_id.to_string()),
            providers: vec![model_settings::ProviderInstance {
                id: instance_id.to_string(),
                name: "Worker test provider".to_string(),
                kind: model_settings::ProviderKind::OpenaiCompatible,
                base_url: Some(base_url.to_string()),
                paper_model: paper_model.clone(),
                translation_model: translation_model.clone(),
                models: Vec::new(),
                models_fetched_at: None,
                connection_verified_at: Some("2026-08-24T00:00:00Z".to_string()),
                paper_probe: Some(model_settings::PaperProbeRecord {
                    fingerprint: model_settings::paper_probe_fingerprint(
                        base_url,
                        api_key,
                        &paper_model,
                    ),
                    passed_at: "2026-08-24T00:00:00Z".to_string(),
                    paper_model: paper_model.clone(),
                }),
                sort_order: 0,
            }],
        };
        let credentials = TestRouteCredential {
            id: instance_id.to_string(),
            key: api_key.to_string(),
        };
        match ProviderRouting::new(&settings, &credentials)
            .capture(
                ProviderSelection::Current,
                FrozenModels::new(paper_model, Some(translation_model)).expect("models"),
                ModelRole::Paper,
            )
            .expect("capture route")
        {
            ProviderRouteDecision::Ready(route) => route,
            ProviderRouteDecision::ActionRequired(requirement) => {
                panic!("worker test route unexpectedly unavailable: {requirement:?}")
            }
        }
    }
    #[test]
    fn parse_roadmap_json_tolerates_code_fences_and_prefix_text() {
        let pure = roadmap_module::parse_json(r#"{"version":1,"paperTitle":"t","passes":[]}"#)
            .expect("pure json");
        assert_eq!(pure["version"], 1);

        let fenced = roadmap_module::parse_json(
            "```json\n{\"version\":1,\"paperTitle\":\"t\",\"passes\":[]}\n```",
        )
        .expect("fenced json");
        assert_eq!(fenced["paperTitle"], "t");

        let prefixed = roadmap_module::parse_json(
            "Here is the roadmap:\n{\"version\":1,\"paperTitle\":\"t\",\"passes\":[]}",
        )
        .expect("prefixed json");
        assert_eq!(prefixed["version"], 1);

        assert!(roadmap_module::parse_json("").is_err());
        assert!(roadmap_module::parse_json("no json here").is_err());
    }

    #[test]
    fn roadmap_schema_forks_required_fields_by_kind() {
        let paper = roadmap_module::response_schema(crate::library_paths::DocumentKind::Paper);
        let paper_required: Vec<&str> = paper["schema"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(!paper_required.contains(&"elevatorPitch"));
        assert!(!paper_required.contains(&"oneChart"));
        assert!(paper_required.contains(&"passes"));

        let textbook =
            roadmap_module::response_schema(crate::library_paths::DocumentKind::Textbook);
        let textbook_required: Vec<&str> = textbook["schema"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert!(!textbook_required.contains(&"elevatorPitch"));
        assert!(!textbook_required.contains(&"oneChart"));
        assert!(!textbook_required.contains(&"coreClaims"));
        assert!(textbook_required.contains(&"passes"));
        assert!(textbook["schema"]["properties"]["learningObjectives"].is_object());
    }

    #[tokio::test]
    async fn run_job_worker_finalizes_claimed_job_in_original_workspace_after_swap() {
        let state = AppState::default();
        let runtime_a_dir = tempfile::tempdir().expect("runtime A");
        let runtime_b_dir = tempfile::tempdir().expect("runtime B");
        let runtime_a = workspace_lifecycle::open_test_runtime(runtime_a_dir.path(), 101);
        let runtime_b = workspace_lifecycle::open_test_runtime(runtime_b_dir.path(), 102);
        let staging_key = Uuid::new_v4().to_string();
        let route = worker_test_paper_route();

        let enqueue_failure_job = |runtime: &Arc<WorkspaceRuntime>, dedupe_key: &str| {
            enqueue_paper_job(
                &runtime.job_module,
                JobSpec {
                    kind: "reading_artifact".to_string(),
                    provider: None,
                    paper_id: None,
                    revision_id: None,
                    root_key: None,
                    artifact_key: Some("runtime-failure".to_string()),
                    dedupe_key: dedupe_key.to_string(),
                    priority: 100,
                    payload: json!({"stagingKey": staging_key}),
                },
                &route,
            )
            .expect("enqueue failure job")
            .job
        };
        let job_a = enqueue_failure_job(&runtime_a, "runtime-a-failure");
        let job_b = enqueue_failure_job(&runtime_b, "runtime-b-failure");
        let staging_a =
            artifact_staging_dir(&runtime_a.root, &staging_key).expect("A staging path");
        let staging_b =
            artifact_staging_dir(&runtime_b.root, &staging_key).expect("B staging path");
        fs::create_dir_all(&staging_a).expect("create A staging");
        fs::create_dir_all(&staging_b).expect("create B staging");
        fs::write(staging_a.join("sentinel"), b"A").expect("write A sentinel");
        fs::write(staging_b.join("sentinel"), b"B").expect("write B sentinel");
        workspace_lifecycle::swap_runtime(&state, Arc::clone(&runtime_a))
            .expect("install runtime A");

        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let execution = Arc::new(BlockingFailureExecution {
            calls: std::sync::atomic::AtomicUsize::new(0),
            entered: std::sync::Mutex::new(Some(entered_tx)),
            release: std::sync::Mutex::new(Some(release_rx)),
            error: ProviderError {
                kind: ProviderErrorKind::Transport,
                message: "forced worker failure".to_string(),
                retry_after_seconds: None,
                orphaned_resource: Some(RemoteResource {
                    provider: "openai_compatible".to_string(),
                    kind: "file".to_string(),
                    id: "remote-runtime-a".to_string(),
                }),
            },
        });

        tokio::join!(
            run_job_worker_core(
                Arc::clone(&runtime_a),
                execution.as_ref(),
                |_| {},
                |_, _| {},
            ),
            async {
                assert_eq!(
                    entered_rx.await.expect("worker entered"),
                    runtime_a.generation
                );
                workspace_lifecycle::swap_runtime(&state, Arc::clone(&runtime_b))
                    .expect("switch to runtime B");
                release_tx.send(()).expect("release failed execution");
            }
        );

        assert_eq!(
            runtime_a
                .job_module
                .get(&job_a.id)
                .expect("A job after failure")
                .state,
            JobState::Failed
        );
        assert!(!staging_a.exists());
        assert_eq!(
            list_remote_tombstones_from_root(&runtime_a.root)
                .expect("A tombstones")
                .len(),
            1
        );
        assert_eq!(
            runtime_b
                .job_module
                .get(&job_b.id)
                .expect("B queued job")
                .state,
            JobState::Queued
        );
        assert_eq!(
            fs::read(staging_b.join("sentinel")).expect("B sentinel"),
            b"B"
        );
        assert!(list_remote_tombstones_from_root(&runtime_b.root)
            .expect("B tombstones")
            .is_empty());
        assert_eq!(execution.calls.load(Ordering::Acquire), 1);
    }
    #[test]
    fn current_runtime_publish_is_atomic_and_stale_runtime_is_suppressed() {
        let directory_a = tempfile::tempdir().expect("runtime A tempdir");
        let directory_b = tempfile::tempdir().expect("runtime B tempdir");
        let runtime_a = workspace_lifecycle::open_test_runtime(directory_a.path(), 301);
        let runtime_b = workspace_lifecycle::open_test_runtime(directory_b.path(), 302);
        let state = AppState::default();
        let published = std::sync::atomic::AtomicUsize::new(0);

        workspace_lifecycle::swap_runtime(&state, Arc::clone(&runtime_a))
            .expect("install runtime A");
        assert!(publish_if_current(&state, &runtime_a, || {
            published.fetch_add(1, Ordering::AcqRel);
        }));
        workspace_lifecycle::swap_runtime(&state, Arc::clone(&runtime_b))
            .expect("switch to runtime B");
        assert!(!publish_if_current(&state, &runtime_a, || {
            published.fetch_add(10, Ordering::AcqRel);
        }));
        assert!(publish_if_current(&state, &runtime_b, || {
            published.fetch_add(1, Ordering::AcqRel);
        }));
        assert_eq!(published.load(Ordering::Acquire), 2);
    }

    #[test]
    fn app_state_fits_comfortably_on_a_small_stack_frame() {
        assert!(
            std::mem::size_of::<AppState>() < 4096,
            "AppState is {} bytes; keep it heap-indirect so Windows debug startup cannot overflow",
            std::mem::size_of::<AppState>()
        );
    }

    #[test]
    fn provider_credential_format_matches_contract() {
        assert_eq!(provider_credential_user("inst-123"), "provider-inst-123");
        assert_eq!(MISTRAL_CREDENTIAL_USER, "mistral-api-key");
        assert_eq!(CREDENTIAL_SERVICE, "com.skywalker.read-desktop");
    }

    #[test]
    fn provider_keys_resolve_from_memory() {
        let state = AppState::default();
        state
            .provider_keys
            .lock()
            .expect("key lock")
            .insert("p-1".to_string(), "sk-openai".to_string());
        assert_eq!(
            resolve_provider_key("p-1", &state)
                .expect("provider resolve")
                .as_deref(),
            Some("sk-openai")
        );
    }

    #[test]
    fn discussion_user_input_is_incremental_only_for_gemini_with_previous() {
        assert_eq!(
            discussion_user_input("gemini", true),
            DiscussionUserInput::Incremental
        );
        assert_eq!(
            discussion_user_input("gemini", false),
            DiscussionUserInput::Recovery
        );
        assert_eq!(
            discussion_user_input("openai_compatible", true),
            DiscussionUserInput::Recovery
        );
        assert_eq!(
            discussion_user_input("grok", true),
            DiscussionUserInput::Recovery
        );
    }

    #[test]
    fn open_paper_adapter_routes_known_providers() {
        assert!(matches!(
            open_paper_adapter("gemini", "k", None),
            Ok(PaperAdapter::Gemini(_))
        ));
        assert!(matches!(
            open_paper_adapter(
                "openai_compatible",
                "k",
                Some(model_settings::DEFAULT_OPENAI_BASE)
            ),
            Ok(PaperAdapter::Chat(_))
        ));
        assert!(matches!(
            open_paper_adapter("grok", "k", None),
            Ok(PaperAdapter::Chat(_))
        ));
        let error = match open_paper_adapter("azure", "k", None) {
            Err(error) => error,
            Ok(_) => panic!("unsupported provider should fail"),
        };
        assert!(error.contains("Unsupported paper provider"));
    }

    #[test]
    fn outline_prompts_from_payload_requires_frozen_texts() {
        let err = outline_prompts_from_payload(&serde_json::json!({"model": "x"})).unwrap_err();
        assert!(err.contains("frozen prompts"));
        let (extract, compose) = outline_prompts_from_payload(&serde_json::json!({
            "prompts": {"extract": "E", "compose": "C"}
        }))
        .unwrap();
        assert_eq!(extract, "E");
        assert_eq!(compose, "C");
    }

    #[test]
    fn sha256_is_stable_for_deduplication() {
        assert_eq!(
            sha256_bytes(b"read-desktop"),
            "7da793820a2d1fde12aeb11efb6ca1ab0f43b5eec751766b0dd9af65b8d0f990"
        );
    }

    #[test]
    fn citations_extract_page_markers() {
        let citations = citations_for_answer("rev-1", "See [p. 2] and [p. 14]", Some(20), &[]);
        assert_eq!(citations.len(), 2);
        assert_eq!(citations[0].page, Some(2));
        assert_eq!(citations[1].page, Some(14));
    }

    #[test]
    fn uncited_answers_remain_uncited() {
        let citations = citations_for_answer("rev-1", "No explicit marker", Some(20), &[]);
        assert!(citations.is_empty());
    }

    #[test]
    fn citations_reject_non_positive_pages() {
        let citations = citations_for_answer("rev-1", "Invalid [p. 0] [p. 21]", Some(20), &[]);
        assert!(citations.is_empty());
    }

    #[test]
    fn block_citations_accept_only_current_canonical_snapshots() {
        let allowed = vec![BlockQuoteSnapshot {
            revision_id: "rev-1".to_string(),
            ocr_revision_id: "ocr-1".to_string(),
            block_id: "block-safe".to_string(),
            page_number: 7,
            block_index: 3,
            block_type: "paragraph".to_string(),
            text_content: "Canonical evidence".to_string(),
            content_digest: "digest-safe".to_string(),
            bbox: [100, 200, 700, 500],
        }];
        let citations = citations_for_answer(
            "rev-1",
            "Grounded [block: block-safe], forged [block: block-other], duplicate [block: block-safe]",
            Some(20),
            &allowed,
        );
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].page, Some(7));
        assert_eq!(citations[0].region_hash.as_deref(), Some("digest-safe"));
        assert_eq!(citations[0].excerpt.as_deref(), Some("Canonical evidence"));
        assert_eq!(
            citations[0].region.as_ref().map(|region| (
                region.x,
                region.y,
                region.width,
                region.height
            )),
            Some((100.0, 200.0, 600.0, 300.0))
        );
    }

    #[test]
    fn current_ocr_blocks_are_deduplicated_and_message_snapshots_survive_reocr() {
        let directory = tempfile::tempdir().expect("tempdir");
        let projection = WorkspaceModule::new()
            .open(directory.path())
            .expect("workspace");
        let connection = Connection::open(&projection.database_path).expect("database");
        connection
            .execute_batch(
                r#"
                INSERT INTO collections(id, name, relative_path, created_at, updated_at)
                VALUES ('collection-q', 'Quotes', 'Quotes', '2026-08-16T00:00:00Z', '2026-08-16T00:00:00Z');
                INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                VALUES ('paper-q', 'collection-q', 'paper.pdf', 'Quotes/paper.pdf', '2026-08-16T00:00:00Z', '2026-08-16T00:00:00Z');
                INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                VALUES ('revision-q', 'paper-q', 'digest-revision', 10, 12, 'Quotes/paper.pdf', '2026-08-16T00:00:00Z');
                INSERT INTO ocr_revisions(id, revision_id, status, provider, model, created_at, published_at)
                VALUES
                  ('ocr-old', 'revision-q', 'ready', 'mistral', 'mistral-ocr-latest', '2026-08-16T00:00:01Z', '2026-08-16T00:00:02Z'),
                  ('ocr-current', 'revision-q', 'ready', 'mistral', 'mistral-ocr-latest', '2026-08-16T00:00:03Z', '2026-08-16T00:00:04Z');
                INSERT INTO ocr_pages(id, ocr_revision_id, page_number) VALUES
                  ('page-old', 'ocr-old', 2),
                  ('page-current-2', 'ocr-current', 2),
                  ('page-current-9', 'ocr-current', 9);
                INSERT INTO ocr_blocks(id, ocr_page_id, block_index, block_type, text_content, content_digest, x0, y0, x1, y1)
                VALUES
                  ('block-old', 'page-old', 0, 'paragraph', 'Old OCR', 'digest-old', 10, 10, 200, 100),
                  ('block-current-2', 'page-current-2', 1, 'paragraph', 'Current page two', 'digest-current-2', 100, 200, 600, 400),
                  ('block-current-9', 'page-current-9', 4, 'table', 'Current page nine', 'digest-current-9', 50, 100, 950, 900);
                "#,
            )
            .expect("quote fixture");

        let snapshots = load_block_quote_snapshots(
            &connection,
            "revision-q",
            &[
                "block-current-9".to_string(),
                "block-current-2".to_string(),
                "block-current-9".to_string(),
            ],
        )
        .expect("canonical snapshots");
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].page_number, 9);
        assert_eq!(snapshots[1].page_number, 2);
        assert_eq!(snapshots[0].ocr_revision_id, "ocr-current");
        assert!(
            load_block_quote_snapshots(&connection, "revision-q", &["block-old".to_string()])
                .is_err()
        );
        assert!(load_block_quote_snapshots(
            &connection,
            "revision-q",
            &["forged-block".to_string()]
        )
        .is_err());

        connection
            .execute_batch(
                r#"
                INSERT INTO discussions(id, paper_id, revision_id, title, status, created_at, updated_at)
                VALUES ('discussion-q', 'paper-q', 'revision-q', 'Quote discussion', 'active', '2026-08-16T00:00:05Z', '2026-08-16T00:00:05Z');
                INSERT INTO messages(id, discussion_id, role, content, status, citations_json, created_at, updated_at)
                VALUES ('message-q', 'discussion-q', 'user', 'Compare these Blocks', 'complete', '[]', '2026-08-16T00:00:05Z', '2026-08-16T00:00:05Z');
                "#,
            )
            .expect("discussion fixture");
        connection
            .execute(
                "INSERT INTO message_contexts(message_id, block_quotes_json) VALUES (?1, ?2)",
                params![
                    "message-q",
                    serde_json::to_string(&snapshots).expect("snapshot json")
                ],
            )
            .expect("message snapshots");
        connection
            .execute_batch(
                r#"
                INSERT INTO ocr_revisions(id, revision_id, status, provider, model, created_at, published_at)
                VALUES ('ocr-newer', 'revision-q', 'ready', 'mistral', 'mistral-ocr-latest', '2026-08-16T00:00:06Z', '2026-08-16T00:00:07Z');
                INSERT INTO ocr_pages(id, ocr_revision_id, page_number)
                VALUES ('page-newer', 'ocr-newer', 2);
                INSERT INTO ocr_blocks(id, ocr_page_id, block_index, block_type, text_content, content_digest, x0, y0, x1, y1)
                VALUES ('block-newer', 'page-newer', 0, 'paragraph', 'Changed by re-OCR', 'digest-newer', 20, 20, 500, 200);
                "#,
            )
            .expect("new OCR");

        let path = discussion_path(&connection, "discussion-q", Some("message-q"))
            .expect("discussion path");
        assert_eq!(path.len(), 1);
        assert_eq!(path[0].block_quotes, snapshots);
        assert_eq!(path[0].block_quotes[0].text_content, "Current page nine");
    }

    #[test]
    fn context_compaction_budget_is_conservative_and_not_message_count_only() {
        let short_path = (0..7)
            .map(|index| DiscussionPathMessage {
                id: format!("m-{index}"),
                role: "user".to_string(),
                status: "complete".to_string(),
                content: "x".repeat(2_000),
                block_quotes: Vec::new(),
            })
            .collect::<Vec<_>>();
        assert!(!discussion_needs_compaction(
            &short_path,
            Some(500),
            Some(8_000)
        ));

        let long_path = (0..8)
            .map(|index| DiscussionPathMessage {
                id: format!("m-{index}"),
                role: if index % 2 == 0 {
                    "user".to_string()
                } else {
                    "assistant".to_string()
                },
                status: "complete".to_string(),
                content: "x".repeat(200),
                block_quotes: Vec::new(),
            })
            .collect::<Vec<_>>();
        assert!(discussion_needs_compaction(
            &long_path,
            Some(10),
            Some(8_000)
        ));
        assert!(!discussion_needs_compaction(
            &long_path,
            Some(10),
            Some(1_000_000)
        ));
    }

    #[test]
    fn compaction_source_preserves_roles_status_math_and_does_not_add_current_question() {
        let history = vec![
            DiscussionPathMessage {
                id: "u1".into(),
                role: "user".into(),
                status: "complete".into(),
                content: "假设 $x > 0$，先只给提示。".into(),
                block_quotes: Vec::new(),
            },
            DiscussionPathMessage {
                id: "a1".into(),
                role: "assistant".into(),
                status: "cancelled".into(),
                content: "尝试使用\\n不是实际换行\n$\\alpha$，但推导尚未完成".into(),
                block_quotes: Vec::new(),
            },
        ];
        let value: Value =
            serde_json::from_str(&context_compaction_source_input("教材", &history)).unwrap();
        assert_eq!(value["sourceMessages"].as_array().unwrap().len(), 2);
        assert_eq!(value["sourceMessages"][0]["role"], "user");
        assert_eq!(value["sourceMessages"][1]["status"], "cancelled");
        assert_eq!(value["sourceMessages"][1]["content"], history[1].content);
        assert!(value.get("readerContext").is_none());
        assert!(value.get("currentQuestion").is_none());
        let empty: Value =
            serde_json::from_str(&context_compaction_source_input("论文", &[])).unwrap();
        assert_eq!(empty["sourceMessages"], json!([]));
    }

    #[test]
    fn compaction_recovery_keeps_claims_questions_and_current_block_permissions() {
        let content = json!({
            "summaryMarkdown": "仍在检查交换极限的条件。\n读者要求用中文。",
            "retainedClaims": ["修正：仅在 $x > 0$ 时成立；历史块 old-block 尚未重新核实。"],
            "unresolvedQuestions": ["待验证假设：是否存在一致上界 $\\alpha$？"],
        });
        let parsed = parse_context_compaction(&content.to_string()).unwrap();
        let input = discussion_turn_input("继续检查上界", &[], Some(4), false);
        let wrapped =
            reader_context::prepend_reader_context(&input, Some("Reader context: 已学过实分析"));
        let recovery = discussion_recovery_from_compaction("当前文档", &parsed, &wrapped);
        let json_line = recovery.lines().find(|line| line.starts_with('{')).unwrap();
        assert_eq!(serde_json::from_str::<Value>(json_line).unwrap(), content);
        assert!(recovery.ends_with(&wrapped));
        assert!(recovery.contains("本轮没有允许引用的 OCR 块"));
        assert!(!recovery.contains("[block: old-block]"));
        // Persistence metadata must not become instructions or extra model-memory fields.
        let mut stored = content.clone();
        stored["sourceMessageIds"] = json!(["historical-message"]);
        assert_eq!(
            discussion_recovery_from_compaction("当前文档", &stored, &wrapped),
            recovery
        );
    }

    #[test]
    fn context_compaction_requires_the_typed_audit_contract() {
        let value = parse_context_compaction(
            r#"{"summaryMarkdown":"Stable summary","retainedClaims":["claim"],"unresolvedQuestions":[]}"#,
        )
        .expect("valid compaction");
        assert_eq!(
            value.get("summaryMarkdown").and_then(Value::as_str),
            Some("Stable summary")
        );
        assert!(parse_context_compaction(
            r#"{"summaryMarkdown":"","retainedClaims":[],"unresolvedQuestions":[]}"#
        )
        .is_err());
        for invalid in [
            json!({"summaryMarkdown":"ok","retainedClaims":[],"unresolvedQuestions":[],"inventedField":true}),
            json!({"summaryMarkdown":"ok","retainedClaims":[]}),
            json!({"summaryMarkdown":null,"retainedClaims":[],"unresolvedQuestions":[]}),
            json!({"summaryMarkdown":" \n ","retainedClaims":[],"unresolvedQuestions":[]}),
            json!({"summaryMarkdown":"ok","retainedClaims":[42],"unresolvedQuestions":[]}),
            json!({"summaryMarkdown":"ok","retainedClaims":["  "],"unresolvedQuestions":[]}),
            json!({"summaryMarkdown":"ok","retainedClaims":[],"unresolvedQuestions":[{}]}),
            json!({"summaryMarkdown":"ok","retainedClaims":[],"unresolvedQuestions":null}),
        ] {
            assert!(
                parse_context_compaction(&invalid.to_string()).is_err(),
                "{invalid}"
            );
        }
        assert!(parse_context_compaction("```json\n{\"summaryMarkdown\":\"无历史\",\"retainedClaims\":[],\"unresolvedQuestions\":[]}\n```").is_ok());
    }

    #[test]
    fn diagnostic_preview_excludes_paths_content_and_secrets() {
        let directory = tempfile::tempdir().expect("tempdir");
        let workspace = WorkspaceModule::new();
        workspace.open(directory.path()).expect("workspace");
        let preview = diagnostic_preview_for_root(directory.path()).expect("diagnostic preview");
        let serialized = serde_json::to_string(&preview).expect("diagnostic json");
        assert!(!serialized.contains(&directory.path().to_string_lossy().to_string()));
        assert!(!serialized.contains("api-key-secret"));
        assert!(preview
            .excluded_data
            .iter()
            .any(|entry| entry.contains("Prompt text")));
        assert_eq!(
            preview
                .summary
                .pointer("/counts/papers")
                .and_then(Value::as_i64),
            Some(0)
        );
    }

    #[test]
    fn paper_remote_cleanup_queues_distinct_file_and_interaction_tombstones() {
        let directory = tempfile::tempdir().expect("tempdir");
        let workspace = WorkspaceModule::new();
        let projection = workspace.open(directory.path()).expect("workspace");
        let connection = Connection::open(&projection.database_path).expect("database");
        let timestamp = now();
        connection
            .execute(
                "INSERT INTO collections(id, name, relative_path, created_at, updated_at)
                 VALUES ('collection-test', 'Test', 'Test', ?1, ?1)",
                params![timestamp],
            )
            .expect("collection");
        connection
            .execute(
                "INSERT INTO papers(
                   id, collection_id, file_name, relative_path, created_at, updated_at
                 ) VALUES (
                   'paper-test', 'collection-test', 'paper.pdf', 'Test/paper.pdf', ?1, ?1
                 )",
                params![timestamp],
            )
            .expect("paper");
        connection
            .execute(
                "INSERT INTO document_revisions(
                   id, paper_id, sha256, byte_size, page_count,
                   source_relative_path, created_at
                 ) VALUES (
                   'revision-test', 'paper-test', 'digest', 1, 1, 'Test/paper.pdf', ?1
                 )",
                params![timestamp],
            )
            .expect("revision");
        connection
            .execute(
                "INSERT INTO context_roots(
                   id, revision_id, provider, model, context_epoch,
                   provider_file_id, provider_node_id, state, created_at
                 ) VALUES (
                   'root-test', 'revision-test', 'gemini', 'gemini-2.5-flash',
                   'epoch', 'files/file-1', 'interaction-1', 'active', ?1
                 )",
                params![timestamp],
            )
            .expect("context root");
        drop(connection);

        assert_eq!(
            queue_paper_remote_cleanup(directory.path(), "paper-test").expect("queue cleanup"),
            2
        );
        let tombstones = list_remote_tombstones_from_root(directory.path()).expect("tombstones");
        assert_eq!(tombstones.len(), 2);
        assert!(tombstones.iter().all(|item| item.state == "pending"));
        assert!(tombstones.iter().any(|item| item.resource_kind == "file"));
        assert!(tombstones
            .iter()
            .any(|item| item.resource_kind == "interaction"));
    }

    #[test]
    fn paper_remote_cleanup_tombstones_the_row_provider_not_hardcoded_gemini() {
        let directory = tempfile::tempdir().expect("tempdir");
        let workspace = WorkspaceModule::new();
        let projection = workspace.open(directory.path()).expect("workspace");
        let connection = Connection::open(&projection.database_path).expect("database");
        let timestamp = now();
        connection
            .execute(
                "INSERT INTO collections(id, name, relative_path, created_at, updated_at)
                 VALUES ('collection-test', 'Test', 'Test', ?1, ?1)",
                params![timestamp],
            )
            .expect("collection");
        connection
            .execute(
                "INSERT INTO papers(
                   id, collection_id, file_name, relative_path, created_at, updated_at
                 ) VALUES (
                   'paper-test', 'collection-test', 'paper.pdf', 'Test/paper.pdf', ?1, ?1
                 )",
                params![timestamp],
            )
            .expect("paper");
        connection
            .execute(
                "INSERT INTO document_revisions(
                   id, paper_id, sha256, byte_size, page_count,
                   source_relative_path, created_at
                 ) VALUES (
                   'revision-test', 'paper-test', 'digest', 1, 1, 'Test/paper.pdf', ?1
                 )",
                params![timestamp],
            )
            .expect("revision");
        connection
            .execute(
                "INSERT INTO context_roots(
                   id, revision_id, provider, model, context_epoch,
                   provider_file_id, provider_node_id, state, created_at
                 ) VALUES (
                   'root-openai', 'revision-test', 'openai_compatible', 'gpt-4.1',
                   'epoch', 'files/file-openai', 'chatcmpl-1', 'active', ?1
                 )",
                params![timestamp],
            )
            .expect("openai-compatible context root");
        drop(connection);

        assert_eq!(
            queue_paper_remote_cleanup(directory.path(), "paper-test").expect("queue cleanup"),
            2
        );
        let tombstones = list_remote_tombstones_from_root(directory.path()).expect("tombstones");
        assert_eq!(tombstones.len(), 2);
        assert!(tombstones
            .iter()
            .all(|item| item.provider == "openai_compatible"));
        assert!(tombstones.iter().any(|item| item.resource_kind == "file"));
        assert!(tombstones
            .iter()
            .any(|item| item.resource_kind == "interaction"));
    }

    #[tokio::test]
    async fn remote_cleanup_in_flight_after_swap_only_finalizes_captured_runtime() {
        let directory_a = tempfile::tempdir().expect("runtime A tempdir");
        let directory_b = tempfile::tempdir().expect("runtime B tempdir");
        let runtime_a = workspace_lifecycle::open_test_runtime(directory_a.path(), 201);
        let runtime_b = workspace_lifecycle::open_test_runtime(directory_b.path(), 202);
        let tombstone_id = "tombstone-shared";

        for runtime in [&runtime_a, &runtime_b] {
            let connection = open_db(&runtime.root).expect("open runtime database");
            connection
                .execute(
                    "INSERT INTO remote_endpoint_snapshots(
                       endpoint_scope, version, owner_type, provider_instance_id,
                       provider_name_at_capture, provider_kind, base_url, created_at
                     ) VALUES (
                       'scope-runtime', 1, 'paper_provider', 'instance-runtime',
                       'Runtime', 'openai_compatible', 'https://example.invalid/v1', ?1
                     )",
                    params![now()],
                )
                .expect("seed endpoint snapshot");
            connection
                .execute(
                    "INSERT INTO remote_tombstones(
                       id, provider, resource_kind, remote_id, paper_id, state,
                       attempts, last_error, created_at, updated_at,
                       endpoint_scope, ownership_status
                     ) VALUES (?1, 'openai_compatible', 'file', 'file-shared',
                               NULL, 'pending', 0, NULL, ?2, ?2,
                               'scope-runtime', 'exact')",
                    params![tombstone_id, now()],
                )
                .expect("seed exact tombstone");
        }

        let state = AppState::default();
        workspace_lifecycle::swap_runtime(&state, Arc::clone(&runtime_a))
            .expect("install runtime A");
        let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = tokio::sync::oneshot::channel();
        let mut entered_tx = Some(entered_tx);
        let mut release_rx = Some(release_rx);
        let cleanup = retry_remote_tombstones_with_delete(
            Arc::clone(&runtime_a),
            Some(tombstone_id),
            move |tombstone| {
                let entered = entered_tx.take().expect("single cleanup request");
                let release = release_rx.take().expect("single cleanup release");
                async move {
                    entered.send(tombstone).expect("report remote request");
                    release.await.expect("release remote request");
                    Ok::<(), ProviderError>(())
                }
            },
            |_| {},
        );
        let (cleanup_result, ()) = tokio::join!(cleanup, async {
            let tombstone = entered_rx.await.expect("cleanup entered remote request");
            assert_eq!(tombstone.resource.id, "file-shared");
            workspace_lifecycle::swap_runtime(&state, Arc::clone(&runtime_b))
                .expect("switch to runtime B");
            release_tx.send(()).expect("release remote cleanup");
        });

        assert_eq!(cleanup_result.expect("cleanup result"), 1);
        let projection = |runtime: &Arc<WorkspaceRuntime>| {
            open_db(&runtime.root)
                .expect("open runtime database")
                .query_row(
                    "SELECT state, attempts FROM remote_tombstones WHERE id = ?1",
                    params![tombstone_id],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
                )
                .expect("tombstone projection")
        };
        assert_eq!(projection(&runtime_a), ("resolved".to_string(), 1));
        assert_eq!(projection(&runtime_b), ("pending".to_string(), 0));
    }

    #[test]
    fn model_ids_are_normalized_and_restricted() {
        assert_eq!(
            normalize_model_id("models/gemini-2.5-flash").unwrap(),
            "gemini-2.5-flash"
        );
        assert!(normalize_model_id("gemini/../../secret").is_err());
        assert!(normalize_model_id("").is_err());
    }

    #[test]
    fn model_catalog_filters_non_generation_models() {
        let embedding = GeminiApiModel {
            name: "models/text-embedding-004".to_string(),
            display_name: Some("Embedding".to_string()),
            description: None,
            input_token_limit: Some(2048),
            output_token_limit: None,
            supported_generation_methods: vec!["embedContent".to_string()],
        };
        let generation = GeminiApiModel {
            name: "models/gemini-2.5-flash".to_string(),
            display_name: Some("Gemini 2.5 Flash".to_string()),
            description: Some("Fast multimodal model".to_string()),
            input_token_limit: Some(1_048_576),
            output_token_limit: Some(65_536),
            supported_generation_methods: vec!["generateContent".to_string()],
        };
        assert!(model_option_from_api(embedding).is_none());
        let option = model_option_from_api(generation).unwrap();
        assert_eq!(option.id, "gemini-2.5-flash");
        assert!(option.supports_native_pdf);

        let gemma = GeminiApiModel {
            name: "models/gemma-3-27b-it".to_string(),
            display_name: Some("Gemma".to_string()),
            description: None,
            input_token_limit: Some(131_072),
            output_token_limit: Some(8_192),
            supported_generation_methods: vec!["generateContent".to_string()],
        };
        assert!(model_option_from_api(gemma).is_none());
    }

    #[test]
    fn chat_request_deserializes_camel_case_and_regenerate() {
        let request: ChatRequest = serde_json::from_value(json!({
            "revisionId": "rev-1",
            "threadId": "thread-1",
            "parentId": null,
            "question": "",
            "page": 2,
            "blockIds": [],
            "regenerateFromId": "assistant-1"
        }))
        .expect("chat request");
        assert_eq!(request.revision_id, "rev-1");
        assert_eq!(request.regenerate_from_id.as_deref(), Some("assistant-1"));
    }

    #[test]
    fn prepare_chat_turn_regenerates_as_sibling_user_turn() {
        let directory = tempfile::tempdir().expect("tempdir");
        let projection = WorkspaceModule::new()
            .open(directory.path())
            .expect("workspace");
        let connection = Connection::open(&projection.database_path).expect("database");
        connection
            .execute_batch(
                r#"
                INSERT INTO collections(id, name, relative_path, created_at, updated_at)
                VALUES ('c1', 'Papers', '', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                VALUES ('p1', 'c1', 'paper.pdf', 'paper.pdf', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                VALUES ('r1', 'p1', 'digest', 10, 4, 'paper.pdf', '2026-08-17T00:00:00Z');
                INSERT INTO discussions(id, paper_id, revision_id, title, status, created_at, updated_at)
                VALUES ('d1', 'p1', 'r1', 'Main', 'active', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO messages(id, discussion_id, parent_id, role, content, status, citations_json, created_at, updated_at)
                VALUES
                  ('u1', 'd1', NULL, 'user', 'What is the claim?', 'complete', '[]', '2026-08-17T00:00:01Z', '2026-08-17T00:00:01Z'),
                  ('a1', 'd1', 'u1', 'assistant', 'A first answer', 'failed', '[]', '2026-08-17T00:00:02Z', '2026-08-17T00:00:02Z');
                INSERT INTO message_contexts(message_id, block_quotes_json)
                VALUES ('u1', '[{"revisionId":"r1","ocrRevisionId":"o1","blockId":"b1","pageNumber":1,"blockIndex":0,"blockType":"paragraph","textContent":"quoted","contentDigest":"d","bbox":[1,2,3,4]}]');
                "#,
            )
            .expect("fixture");

        let prepared = prepare_chat_turn(
            &connection,
            &ChatRequest {
                revision_id: "r1".to_string(),
                thread_id: "d1".to_string(),
                parent_id: None,
                question: String::new(),
                page: Some(1),
                block_ids: Vec::new(),
                regenerate_from_id: Some("a1".to_string()),
            },
            Some("u1".to_string()),
        )
        .expect("prepare regenerate");
        assert!(prepared.insert_user);
        assert_ne!(prepared.user_id, "u1");
        assert_eq!(prepared.question, "What is the claim?");
        assert_eq!(prepared.history_head_id.as_deref(), None);
        assert_eq!(prepared.user_parent_id.as_deref(), None);
        assert_eq!(prepared.block_quotes.len(), 1);
        assert_eq!(prepared.block_quotes[0].block_id, "b1");
        let input =
            discussion_turn_input(&prepared.question, &prepared.block_quotes, Some(3), true);
        assert!(input.contains("重新生成"));
        assert!(input.contains("USER: What is the claim?"));
        assert!(input.contains("quoted"));
        assert!(input.contains("b1"));
        assert!(!input.contains("A first answer"));
        let citations = citations_for_answer(
            "r1",
            "[block: b1] [block: old-block] [p. 8]",
            Some(4),
            &prepared.block_quotes,
        );
        assert_eq!(citations.len(), 1);
        assert_eq!(citations[0].page, Some(1));
        let path = discussion_path(&connection, "d1", Some("a1")).unwrap();
        assert_eq!(path[0].status, "complete");
        assert_eq!(path[1].status, "failed");
        let source: Value =
            serde_json::from_str(&context_compaction_source_input("当前文档", &path)).unwrap();
        assert_eq!(
            source["sourceMessages"][0]["blockQuotes"][0]["blockId"],
            "b1"
        );
        assert_eq!(source["sourceMessages"][1]["status"], "failed");
    }

    #[test]
    fn brief_status_reports_queued_orientation_job() {
        let directory = tempfile::tempdir().expect("tempdir");
        let projection = WorkspaceModule::new()
            .open(directory.path())
            .expect("workspace");
        let connection = Connection::open(&projection.database_path).expect("database");
        connection
            .execute_batch(
                r#"
                INSERT INTO collections(id, name, relative_path, created_at, updated_at)
                VALUES ('c1', 'Papers', '', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                VALUES ('p1', 'c1', 'paper.pdf', 'paper.pdf', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                VALUES ('r1', 'p1', 'digest', 10, 4, 'paper.pdf', '2026-08-17T00:00:00Z');
                INSERT INTO jobs(
                  id, kind, provider, paper_id, revision_id, root_key, artifact_key,
                  dedupe_key, state, stage, provider_committed, priority, payload_json,
                  created_at, updated_at
                ) VALUES (
                  'job-1', 'orientation_pack', 'gemini', 'p1', 'r1', 'r1:model',
                  'orientation_pack', 'orientation:r1:model', 'queued', 'queued', 0, 90, '{}',
                  '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z'
                );
                "#,
            )
            .expect("job fixture");
        let (extras, _stats) =
            load_paper_card_extras(&connection, &[("p1".to_string(), "r1".to_string())])
                .expect("aggregate projection");
        assert_eq!(extras["p1"].brief_status, "queued");
    }

    fn hub_fixture_papers(count: usize) -> Vec<paper_module::PaperProjection> {
        (1..=count)
            .map(|index| paper_module::PaperProjection {
                id: format!("p{index}"),
                revision_id: format!("r{index}"),
                title: format!("Paper {index}"),
                authors: vec!["Alice".to_string(), "Bob".to_string()],
                publication_year: Some(2026),
                display_metadata: None,
                page_count: Some(4),
                collection_id: "c1".to_string(),
                collection_path: "Papers".to_string(),
                file_name: format!("paper{index}.pdf"),
                relative_path: format!("Papers/paper{index}.pdf"),
                sha256: format!("digest{index}"),
                byte_size: 10,
                imported_at: "2026-08-17T00:00:00Z".to_string(),
            })
            .collect()
    }

    fn seed_hub_paper(connection: &Connection, index: usize) {
        connection
            .execute_batch(&format!(
                "INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                 VALUES ('p{index}', 'c1', 'paper{index}.pdf', 'Papers/paper{index}.pdf',
                         '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                 INSERT INTO document_revisions(
                   id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at
                 ) VALUES ('r{index}', 'p{index}', 'digest{index}', 10, 4,
                           'Papers/paper{index}.pdf', '2026-08-17T00:00:00Z');"
            ))
            .expect("seed hub paper");
    }

    fn seed_hub_brief_head(connection: &Connection, index: usize, status: &str, content: &str) {
        connection
            .execute_batch(&format!(
                "INSERT INTO artifacts(
                   id, paper_id, revision_id, kind, version, status, content_json, created_at
                 ) VALUES ('brief-{index}', 'p{index}', 'r{index}', 'brief', 1, '{status}',
                           '{content}', '2026-08-17T00:00:00Z');
                 INSERT INTO artifact_heads(paper_id, kind, object_key, artifact_id, updated_at)
                 VALUES ('p{index}', 'brief', '', 'brief-{index}', '2026-08-17T00:00:00Z');"
            ))
            .expect("seed brief head");
    }

    fn seed_hub_metadata_head(connection: &Connection, index: usize, content: &str) {
        connection
            .execute_batch(&format!(
                "INSERT INTO artifacts(
                   id, paper_id, revision_id, kind, version, status, content_json, created_at
                 ) VALUES ('meta-{index}', 'p{index}', 'r{index}', 'metadata', 1, 'ready',
                           '{content}', '2026-08-17T00:00:00Z');
                 INSERT INTO artifact_heads(paper_id, kind, object_key, artifact_id, updated_at)
                 VALUES ('p{index}', 'metadata', '', 'meta-{index}', '2026-08-17T00:00:00Z');"
            ))
            .expect("seed metadata head");
    }

    fn seed_hub_orientation_job(connection: &Connection, index: usize, state: &str, kind: &str) {
        connection
            .execute_batch(&format!(
                "INSERT INTO jobs(
                   id, kind, provider, paper_id, revision_id, root_key, artifact_key,
                   dedupe_key, state, stage, provider_committed, priority, payload_json,
                   created_at, updated_at
                 ) VALUES (
                   'job-{kind}-{index}', '{kind}', 'gemini', 'p{index}', 'r{index}',
                   'r{index}:model', 'orientation_pack', 'orientation:r{index}:{state}',
                   '{state}', '{state}', 0, 90, '{{}}',
                   '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z'
                 );"
            ))
            .expect("seed orientation job");
    }

    #[test]
    fn aggregate_hub_projection_matches_the_single_paper_path() {
        let directory = tempfile::tempdir().expect("tempdir");
        let projection = WorkspaceModule::new()
            .open(directory.path())
            .expect("workspace");
        let connection = Connection::open(&projection.database_path).expect("database");
        connection
            .execute_batch(
                "INSERT INTO collections(id, name, relative_path, created_at, updated_at)
                 VALUES ('c1', 'Papers', '', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');",
            )
            .expect("collection");
        for index in 1..=6 {
            seed_hub_paper(&connection, index);
        }
        // p1：完整派生数据。
        connection
            .execute_batch(
                "INSERT INTO ocr_revisions(id, revision_id, status, provider, model, created_at)
                 VALUES ('o1', 'r1', 'ready', 'gemini', 'paper-model', '2026-08-17T00:00:00Z');",
            )
            .expect("ocr revision");
        seed_hub_brief_head(
            &connection,
            1,
            "ready",
            "{\"takeaway\":\" 主结论 \",\"keywords\":[\"A\",\" \",\"B\"]}",
        );
        seed_hub_metadata_head(&connection, 1, "{\"chapterNumber\":\" 第3章 \"}");
        // p2：只有排队中的 Job。
        seed_hub_orientation_job(&connection, 2, "queued", "orientation_pack");
        // p3：终态失败的 Job。
        seed_hub_orientation_job(&connection, 3, "interrupted_unknown", "orientation_pack");
        // p5：产物存在但仍未就绪，且内容不是 JSON。
        seed_hub_brief_head(&connection, 5, "generating", "not-json");
        // p6：摘要只能从 summary 取到，章节号是空白。
        seed_hub_brief_head(&connection, 6, "ready", "{\"summary\":\" packed text \"}");
        seed_hub_metadata_head(&connection, 6, "{\"chapterNumber\":\"   \"}");
        // 干扰数据：无 head 的产物、非 orientation_pack 的排队 Job、别的 object_key 的 head，
        // 都不允许进入 p4 的卡片。
        connection
            .execute_batch(
                "INSERT INTO artifacts(
                   id, paper_id, revision_id, kind, version, status, content_json, created_at
                 ) VALUES
                   ('brief-4-headless','p4','r4','brief',1,'ready','{\"takeaway\":\"不该出现\"}',
                    '2026-08-17T00:00:00Z'),
                   ('brief-4-scoped','p4','r4','brief',2,'ready','{\"takeaway\":\"不该出现\"}',
                    '2026-08-17T00:00:00Z');
                 INSERT INTO artifact_heads(paper_id, kind, object_key, artifact_id, updated_at)
                 VALUES ('p4', 'brief', 'chapter-1', 'brief-4-scoped', '2026-08-17T00:00:00Z');",
            )
            .expect("distractor artifacts");
        seed_hub_orientation_job(&connection, 4, "queued", "artifact");

        let root = directory.path();
        let papers = hub_fixture_papers(6);
        let aggregate = document_cards_from_papers(root, papers.clone()).expect("aggregate cards");
        let per_paper = papers
            .iter()
            .map(|paper| document_card_from_paper(root, paper.clone()))
            .collect::<Vec<_>>();
        assert_eq!(aggregate, per_paper, "批量与逐条卡片必须逐字段一致");

        assert_eq!(aggregate[0].brief_status, "ready");
        assert_eq!(aggregate[0].brief_takeaway.as_deref(), Some("主结论"));
        assert_eq!(
            aggregate[0].keywords,
            vec!["A".to_string(), "B".to_string()]
        );
        assert!(aggregate[0].has_ocr);
        assert_eq!(aggregate[0].chapter_number.as_deref(), Some("第3章"));
        assert_eq!(aggregate[1].brief_status, "queued");
        assert_eq!(aggregate[2].brief_status, "failed");
        assert_eq!(aggregate[3].brief_status, "missing");
        assert_eq!(aggregate[3].brief_takeaway, None);
        assert!(!aggregate[3].has_ocr);
        assert_eq!(aggregate[3].chapter_number, None);
        assert_eq!(aggregate[4].brief_status, "generating");
        assert_eq!(aggregate[4].brief_takeaway, None);
        assert_eq!(aggregate[5].brief_status, "ready");
        assert_eq!(aggregate[5].brief_takeaway.as_deref(), Some("packed text"));
        assert_eq!(aggregate[5].chapter_number, None);
    }

    #[test]
    fn aggregate_hub_projection_statement_count_is_bounded_by_chunks() {
        let directory = tempfile::tempdir().expect("tempdir");
        let projection = WorkspaceModule::new()
            .open(directory.path())
            .expect("workspace");
        let connection = Connection::open(&projection.database_path).expect("database");
        connection
            .execute_batch(
                "INSERT INTO collections(id, name, relative_path, created_at, updated_at)
                 VALUES ('c1', 'Papers', '', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');",
            )
            .expect("collection");
        let mut fixture = String::new();
        for index in 1..=900 {
            fixture.push_str(&format!(
                "INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                 VALUES ('p{index}', 'c1', 'paper{index}.pdf', 'Papers/paper{index}.pdf',
                         '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                 INSERT INTO document_revisions(
                   id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at
                 ) VALUES ('r{index}', 'p{index}', 'digest{index}', 10, 4,
                           'Papers/paper{index}.pdf', '2026-08-17T00:00:00Z');"
            ));
        }
        connection.execute_batch(&fixture).expect("900 papers");

        for count in [1_usize, 400, 401, 900] {
            let keys: Vec<(String, String)> = (1..=count)
                .map(|index| (format!("p{index}"), format!("r{index}")))
                .collect();
            let (extras, stats) = load_paper_card_extras(&connection, &keys).expect("projection");
            let chunks = count.div_ceil(HUB_PROJECTION_CHUNK);
            assert_eq!(extras.len(), count, "{count} keys must all be answered");
            assert_eq!(
                stats.statement_count,
                chunks * 8,
                "{count} keys should issue {chunks} chunk(s) x 8 statements, not one per paper"
            );
        }
    }

    #[test]
    fn reusable_ocr_staging_prefers_a_failed_revision_snapshot() {
        let directory = tempfile::tempdir().expect("tempdir");
        let projection = WorkspaceModule::new()
            .open(directory.path())
            .expect("workspace");
        Connection::open(&projection.database_path)
            .expect("database")
            .execute_batch(
                r#"
                INSERT INTO collections(id, name, relative_path, created_at, updated_at)
                VALUES ('c-ocr', 'Papers', '', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                VALUES ('p-ocr', 'c-ocr', 'paper.pdf', 'paper.pdf', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                VALUES ('r-ocr', 'p-ocr', 'digest', 10, 4, 'paper.pdf', '2026-08-17T00:00:00Z');
                INSERT INTO jobs(
                  id, kind, provider, paper_id, revision_id, root_key, artifact_key,
                  dedupe_key, state, stage, provider_committed, priority, payload_json,
                  last_error, created_at, updated_at
                ) VALUES (
                  'ocr-failed', 'ocr', 'mistral', 'p-ocr', 'r-ocr', NULL, 'ocr',
                  'ocr:r-ocr:failed', 'failed', 'failed', 1, 100, '{}',
                  'Mistral OCR page 1 Block 0 has no bbox',
                  '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z'
                );
                "#,
            )
            .expect("paper fixture");
        let jobs = JobModule::open(&projection.database_path).expect("jobs");
        let staged = ocr_staging_path(&projection.root_path, "ocr-failed");
        fs::create_dir_all(staged.parent().expect("parent")).expect("staging dir");
        fs::write(&staged, b"{\"pages\":[]}").expect("staging");
        let next = jobs
            .enqueue(JobSpec {
                kind: "ocr".to_string(),
                provider: Some("mistral".to_string()),
                paper_id: Some("p-ocr".to_string()),
                revision_id: Some("r-ocr".to_string()),
                root_key: None,
                artifact_key: Some("ocr".to_string()),
                dedupe_key: "ocr:r-ocr:retry".to_string(),
                priority: 100,
                payload: json!({"model": "mistral-ocr-latest"}),
            })
            .expect("enqueue retry")
            .job;
        let reused = reusable_ocr_staging(&projection.root_path, &jobs, "r-ocr", &next.id)
            .expect("reuse staging");
        assert_eq!(reused, staged);
    }

    fn discussion_fixture() -> (tempfile::TempDir, Connection) {
        let directory = tempfile::tempdir().expect("tempdir");
        let projection = WorkspaceModule::new()
            .open(directory.path())
            .expect("workspace");
        let connection = Connection::open(&projection.database_path).expect("database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("fk");
        connection
            .execute_batch(
                r#"
                INSERT INTO collections(id, name, relative_path, created_at, updated_at)
                VALUES ('c-d', 'Papers', '', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                VALUES ('p-d', 'c-d', 'paper.pdf', 'paper.pdf', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                VALUES ('r-d', 'p-d', 'digest', 10, 4, 'paper.pdf', '2026-08-17T00:00:00Z');
                INSERT INTO discussions(id, paper_id, revision_id, title, status, created_at, updated_at)
                VALUES
                  ('d-keep', 'p-d', 'r-d', 'Main discussion', 'active', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z'),
                  ('d-empty', 'p-d', 'r-d', '新对话', 'active', '2026-08-17T00:00:01Z', '2026-08-17T00:00:01Z'),
                  ('d-full', 'p-d', 'r-d', 'What is attention?', 'active', '2026-08-17T00:00:02Z', '2026-08-17T00:00:02Z');
                INSERT INTO discussion_heads(discussion_id, message_id, updated_at)
                VALUES
                  ('d-keep', NULL, '2026-08-17T00:00:00Z'),
                  ('d-empty', NULL, '2026-08-17T00:00:01Z'),
                  ('d-full', NULL, '2026-08-17T00:00:02Z');
                INSERT INTO messages(id, discussion_id, role, content, status, citations_json, created_at, updated_at)
                VALUES ('m-full', 'd-full', 'user', 'What is attention?', 'complete', '[]', '2026-08-17T00:00:03Z', '2026-08-17T00:00:03Z');
                "#,
            )
            .expect("discussion fixture");
        (directory, connection)
    }

    #[test]
    fn close_discussion_discards_empty_and_archives_named_threads() {
        let (_directory, connection) = discussion_fixture();
        let discarded = close_discussion(&connection, "d-empty").expect("discard empty");
        assert_eq!(discarded.action, "discarded");
        let archived = close_discussion(&connection, "d-full").expect("archive full");
        assert_eq!(archived.action, "archived");
        assert!(close_discussion(&connection, "d-keep").is_err());
        assert_eq!(
            query_threads(&connection, "r-d", "active")
                .expect("active")
                .len(),
            1
        );
        assert_eq!(
            query_threads(&connection, "r-d", "archived")
                .expect("archived")
                .len(),
            1
        );
    }

    #[test]
    fn delete_discussion_subtree_removes_descendants_and_retreats_head() {
        let directory = tempfile::tempdir().expect("tempdir");
        let projection = WorkspaceModule::new()
            .open(directory.path())
            .expect("workspace");
        let connection = Connection::open(&projection.database_path).expect("database");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("fk");
        connection
            .execute_batch(
                r#"
                INSERT INTO collections(id, name, relative_path, created_at, updated_at)
                VALUES ('c1', 'Papers', '', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                VALUES ('p1', 'c1', 'paper.pdf', 'paper.pdf', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                VALUES ('r1', 'p1', 'digest', 10, 4, 'paper.pdf', '2026-08-17T00:00:00Z');
                INSERT INTO discussions(id, paper_id, revision_id, title, status, created_at, updated_at)
                VALUES ('d1', 'p1', 'r1', 'Main', 'active', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO messages(id, discussion_id, parent_id, role, content, status, citations_json, created_at, updated_at)
                VALUES
                  ('u1', 'd1', NULL, 'user', 'Q1', 'complete', '[]', '2026-08-17T00:00:01Z', '2026-08-17T00:00:01Z'),
                  ('a1', 'd1', 'u1', 'assistant', 'A1', 'complete', '[]', '2026-08-17T00:00:02Z', '2026-08-17T00:00:02Z'),
                  ('u2', 'd1', 'a1', 'user', 'Q2', 'complete', '[]', '2026-08-17T00:00:03Z', '2026-08-17T00:00:03Z'),
                  ('a2', 'd1', 'u2', 'assistant', 'A2', 'complete', '[]', '2026-08-17T00:00:04Z', '2026-08-17T00:00:04Z');
                INSERT INTO discussion_heads(discussion_id, message_id, updated_at)
                VALUES ('d1', 'a2', '2026-08-17T00:00:04Z');
                "#,
            )
            .expect("turn fixture");

        let result = delete_discussion_subtree(&connection, "d1", "u2").expect("delete child");
        assert_eq!(result.deleted, 2);
        assert_eq!(result.next_head_id.as_deref(), Some("a1"));
        let remaining: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM messages WHERE discussion_id = 'd1'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(remaining, 2);
    }

    #[test]
    fn update_orientation_table_updates_content_and_pinned_keys() {
        let directory = tempfile::tempdir().expect("tempdir");
        let projection = WorkspaceModule::new()
            .open(directory.path())
            .expect("workspace");
        let connection = Connection::open(&projection.database_path).expect("database");
        connection
            .execute_batch(
                r#"
                INSERT INTO collections(id, name, relative_path, created_at, updated_at)
                VALUES ('c1', 'Papers', '', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                VALUES ('p1', 'c1', 'paper.pdf', 'paper.pdf', '2026-08-17T00:00:00Z', '2026-08-17T00:00:00Z');
                INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                VALUES ('r1', 'p1', 'digest', 10, 4, 'paper.pdf', '2026-08-17T00:00:00Z');
                INSERT INTO artifacts(id, paper_id, revision_id, ocr_revision_id, kind, object_key, version, status, content_json, dependency_snapshot_json, created_at)
                VALUES ('art-g1', 'p1', 'r1', NULL, 'glossary', '', 1, 'ready', '{"entries":[{"term":"Attention","definition":"Focus mechanism","aliases":["Self-Attention"]}],"_pinned":[]}', '{}', '2026-08-17T00:00:00Z');
                INSERT INTO artifact_heads(paper_id, kind, object_key, artifact_id, updated_at)
                VALUES ('p1', 'glossary', '', 'art-g1', '2026-08-17T00:00:00Z');
                "#,
            )
            .expect("fixture");

        let new_entries = serde_json::json!([
            {"term": "Attention", "definition": "User edited definition", "aliases": ["Self-Attention", "$A_{ij}$"]},
            {"term": "Transformer", "definition": "Attention-based network", "aliases": []}
        ]);
        let pinned = vec!["Attention".to_string()];

        edit_auxiliary(directory.path(), "r1", "glossary", Some("art-g1"), |old| {
            crate::auxiliary_state::edit_table(
                &old.content,
                new_entries.as_array().unwrap().clone(),
                pinned,
            )
        })
        .expect("edit");
        let module =
            crate::artifact_module::ArtifactModule::open(&projection.database_path).unwrap();
        let head = module
            .head_for_revision("r1", "glossary", "")
            .unwrap()
            .unwrap();
        assert_eq!(head.version, 2);
        assert_eq!(
            head.content["entries"][0]["definition"],
            "User edited definition"
        );
        assert_eq!(
            module.get("art-g1").unwrap().content["entries"][0]["definition"],
            "Focus mechanism"
        );
        assert!(edit_auxiliary(
            directory.path(),
            "r1",
            "glossary",
            Some("art-g1"),
            |_| panic!("stale edit must be rejected")
        )
        .is_err());
    }
}
