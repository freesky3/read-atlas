use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::GeminiModelOption;

pub const DEFAULT_OPENAI_BASE: &str = "https://api.openai.com/v1";
pub const GROK_API_BASE: &str = "https://api.x.ai/v1";
pub const DEFAULT_GEMINI_PROXY_BASE: &str = "http://localhost:8045/v1";
pub const PAPER_PROBE_VERSION: &str = "paper-probe-v1";
pub const MODEL_SETTINGS_SCHEMA: u32 = 4;
pub const DEFAULT_GEMINI_PAPER_MODEL: &str = "gemini-2.5-flash";
pub const DEFAULT_GEMINI_TRANSLATION_MODEL: &str = "gemini-2.5-flash-lite";
pub const DEFAULT_GEMINI_PROXY_PAPER_MODEL: &str = "gemini-3.7-flash-high";
pub const DEFAULT_GEMINI_PROXY_TRANSLATION_MODEL: &str = "gemini-3.1-flash-lite";
pub const MISTRAL_OCR_MODEL: &str = "mistral-ocr-latest";
pub const CREDENTIAL_STORE: &str = "Windows Credential Manager";
pub const MAX_PROVIDER_INSTANCES: usize = 10;

// ---------------------------------------------------------------------------
// Provider Kind
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    Gemini,
    OpenaiCompatible,
    Grok,
    GeminiProxy,
}

impl ProviderKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderKind::Gemini => "gemini",
            ProviderKind::OpenaiCompatible => "openai_compatible",
            ProviderKind::Grok => "grok",
            ProviderKind::GeminiProxy => "gemini_proxy",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ProviderKind::Gemini => "Gemini",
            ProviderKind::OpenaiCompatible => "OpenAI-compatible",
            ProviderKind::Grok => "Grok",
            ProviderKind::GeminiProxy => "Gemini Proxy (Antigravity)",
        }
    }

    pub fn default_name(&self) -> &'static str {
        match self {
            ProviderKind::Gemini => "Gemini",
            ProviderKind::OpenaiCompatible => "OpenAI",
            ProviderKind::Grok => "Grok",
            ProviderKind::GeminiProxy => "Gemini Proxy",
        }
    }

    /// Whether this kind uses the Chat Completions API (OpenAI-compatible / Grok / GeminiProxy)
    /// as opposed to the Gemini native API.
    pub fn is_chat_completions(&self) -> bool {
        matches!(
            self,
            ProviderKind::OpenaiCompatible | ProviderKind::Grok | ProviderKind::GeminiProxy
        )
    }

    pub fn is_gemini_proxy(&self) -> bool {
        matches!(self, ProviderKind::GeminiProxy)
    }

    pub fn fixed_base_url(&self) -> Option<&'static str> {
        match self {
            ProviderKind::Grok => Some(GROK_API_BASE),
            _ => None,
        }
    }

    pub fn needs_base_url(&self) -> bool {
        matches!(
            self,
            ProviderKind::OpenaiCompatible | ProviderKind::GeminiProxy
        )
    }

    pub fn needs_paper_probe(&self) -> bool {
        self.is_chat_completions()
    }
}

// ---------------------------------------------------------------------------
// Paper Probe Record
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PaperProbeRecord {
    pub fingerprint: String,
    pub passed_at: String,
    pub paper_model: String,
}

// ---------------------------------------------------------------------------
// Provider Instance (stored)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInstance {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    #[serde(default)]
    pub base_url: Option<String>,
    #[serde(default)]
    pub paper_model: String,
    #[serde(default)]
    pub translation_model: String,
    #[serde(default)]
    pub models: Vec<GeminiModelOption>,
    #[serde(default)]
    pub models_fetched_at: Option<String>,
    #[serde(default)]
    pub connection_verified_at: Option<String>,
    #[serde(default)]
    pub paper_probe: Option<PaperProbeRecord>,
    #[serde(default)]
    pub sort_order: u32,
}

impl ProviderInstance {
    pub fn new(name: String, kind: ProviderKind, sort_order: u32) -> Self {
        let base_url = match &kind {
            ProviderKind::OpenaiCompatible => Some(DEFAULT_OPENAI_BASE.to_string()),
            ProviderKind::Grok => Some(GROK_API_BASE.to_string()),
            ProviderKind::GeminiProxy => Some(DEFAULT_GEMINI_PROXY_BASE.to_string()),
            _ => None,
        };
        let (paper_model, translation_model) = match &kind {
            ProviderKind::GeminiProxy => (
                DEFAULT_GEMINI_PROXY_PAPER_MODEL.to_string(),
                DEFAULT_GEMINI_PROXY_TRANSLATION_MODEL.to_string(),
            ),
            ProviderKind::Gemini => (
                DEFAULT_GEMINI_PAPER_MODEL.to_string(),
                DEFAULT_GEMINI_TRANSLATION_MODEL.to_string(),
            ),
            _ => (String::new(), String::new()),
        };
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            kind,
            base_url,
            paper_model,
            translation_model,
            models: Vec::new(),
            models_fetched_at: None,
            connection_verified_at: None,
            paper_probe: None,
            sort_order,
        }
    }
}

// ---------------------------------------------------------------------------
// Stored Model Settings (Schema v4)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StoredModelSettings {
    pub schema_version: u32,
    #[serde(default)]
    pub current_provider_id: Option<String>,
    #[serde(default)]
    pub providers: Vec<ProviderInstance>,
}

impl Default for StoredModelSettings {
    fn default() -> Self {
        Self {
            schema_version: MODEL_SETTINGS_SCHEMA,
            current_provider_id: None,
            providers: Vec::new(),
        }
    }
}

impl StoredModelSettings {
    pub fn instance(&self, id: &str) -> Result<&ProviderInstance, String> {
        self.providers
            .iter()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("Provider instance not found: {id}"))
    }

    pub fn instance_mut(&mut self, id: &str) -> Result<&mut ProviderInstance, String> {
        self.providers
            .iter_mut()
            .find(|p| p.id == id)
            .ok_or_else(|| format!("Provider instance not found: {id}"))
    }

    pub fn current_instance(&self) -> Option<&ProviderInstance> {
        self.current_provider_id
            .as_deref()
            .and_then(|id| self.providers.iter().find(|p| p.id == id))
    }
}

// ---------------------------------------------------------------------------
// View types (sent to frontend)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInstanceView {
    pub id: String,
    pub name: String,
    pub kind: ProviderKind,
    pub base_url: Option<String>,
    pub paper_model: String,
    pub translation_model: String,
    pub models: Vec<GeminiModelOption>,
    pub models_fetched_at: Option<String>,
    pub connection_verified_at: Option<String>,
    pub paper_probe: Option<PaperProbeRecord>,
    pub credential_configured: bool,
    pub paper_probe_passed: bool,
    pub is_current: bool,
    pub sort_order: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelSettingsView {
    pub current_provider_id: Option<String>,
    pub providers: Vec<ProviderInstanceView>,
    pub credential_store: String,
    pub mistral_credential_configured: bool,
    pub mistral_credential_store: String,
    pub ocr_model: String,
}

// ---------------------------------------------------------------------------
// View construction
// ---------------------------------------------------------------------------

fn instance_view(
    inst: &ProviderInstance,
    is_current: bool,
    credential_configured: bool,
) -> ProviderInstanceView {
    ProviderInstanceView {
        id: inst.id.clone(),
        name: inst.name.clone(),
        kind: inst.kind.clone(),
        base_url: inst.base_url.clone(),
        paper_model: inst.paper_model.clone(),
        translation_model: inst.translation_model.clone(),
        models: inst.models.clone(),
        models_fetched_at: inst.models_fetched_at.clone(),
        connection_verified_at: inst.connection_verified_at.clone(),
        paper_probe: inst.paper_probe.clone(),
        credential_configured,
        paper_probe_passed: inst.paper_probe.is_some(),
        is_current,
        sort_order: inst.sort_order,
    }
}

/// Build the view sent to the frontend.
/// `credential_map` maps instance ID → whether a credential is stored.
pub fn model_settings_view(
    settings: &StoredModelSettings,
    credential_map: &std::collections::HashMap<String, bool>,
    mistral_credential_configured: bool,
) -> ModelSettingsView {
    let current_id = settings.current_provider_id.as_deref();
    let mut providers: Vec<ProviderInstanceView> = settings
        .providers
        .iter()
        .map(|inst| {
            let is_current = current_id == Some(inst.id.as_str());
            let cred = credential_map.get(&inst.id).copied().unwrap_or(false);
            instance_view(inst, is_current, cred)
        })
        .collect();
    providers.sort_by_key(|p| p.sort_order);
    ModelSettingsView {
        current_provider_id: settings.current_provider_id.clone(),
        providers,
        credential_store: CREDENTIAL_STORE.to_string(),
        mistral_credential_configured,
        mistral_credential_store: CREDENTIAL_STORE.to_string(),
        ocr_model: MISTRAL_OCR_MODEL.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Parse / Load
// ---------------------------------------------------------------------------

pub fn parse_model_settings_json(raw: &str) -> Result<StoredModelSettings, String> {
    let value: serde_json::Value =
        serde_json::from_str(raw).map_err(|e| format!("模型配置文件格式无效：{e}"))?;
    let schema_version = value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(1) as u32;

    if schema_version == MODEL_SETTINGS_SCHEMA {
        let settings: StoredModelSettings =
            serde_json::from_value(value).map_err(|e| format!("模型配置文件格式无效：{e}"))?;
        // Validate: all instance IDs must be unique
        let mut seen = std::collections::HashSet::new();
        for inst in &settings.providers {
            if !seen.insert(&inst.id) {
                return Err(format!("重复的 Provider 实例 ID: {}", inst.id));
            }
        }
        if settings.providers.len() > MAX_PROVIDER_INSTANCES {
            return Err(format!(
                "Provider 实例数量超过上限 {MAX_PROVIDER_INSTANCES}"
            ));
        }
        return Ok(settings);
    }

    // Schema < 4: discard legacy, start fresh
    Ok(StoredModelSettings::default())
}

// ---------------------------------------------------------------------------
// CRUD operations
// ---------------------------------------------------------------------------

pub fn add_provider_instance(
    mut settings: StoredModelSettings,
    name: String,
    kind: ProviderKind,
) -> Result<(StoredModelSettings, String), String> {
    if settings.providers.len() >= MAX_PROVIDER_INSTANCES {
        return Err(format!(
            "最多只能创建 {MAX_PROVIDER_INSTANCES} 个 Provider 实例"
        ));
    }
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err("Provider 名称不能为空".to_string());
    }
    let sort_order = settings
        .providers
        .iter()
        .map(|p| p.sort_order)
        .max()
        .unwrap_or(0)
        + 1;
    let instance = ProviderInstance::new(name, kind, sort_order);
    let id = instance.id.clone();
    settings.providers.push(instance);
    Ok((settings, id))
}

pub fn remove_provider_instance(
    mut settings: StoredModelSettings,
    id: &str,
) -> Result<StoredModelSettings, String> {
    let index = settings
        .providers
        .iter()
        .position(|p| p.id == id)
        .ok_or_else(|| format!("Provider instance not found: {id}"))?;
    settings.providers.remove(index);
    // If the removed instance was current, clear it
    if settings.current_provider_id.as_deref() == Some(id) {
        settings.current_provider_id = None;
    }
    Ok(settings)
}

pub fn rename_provider_instance(
    mut settings: StoredModelSettings,
    id: &str,
    new_name: String,
) -> Result<StoredModelSettings, String> {
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        return Err("Provider 名称不能为空".to_string());
    }
    settings.instance_mut(id)?.name = new_name;
    Ok(settings)
}

pub fn duplicate_provider_instance(
    mut settings: StoredModelSettings,
    id: &str,
) -> Result<(StoredModelSettings, String), String> {
    if settings.providers.len() >= MAX_PROVIDER_INSTANCES {
        return Err(format!(
            "最多只能创建 {MAX_PROVIDER_INSTANCES} 个 Provider 实例"
        ));
    }
    let source = settings.instance(id)?.clone();
    let sort_order = settings
        .providers
        .iter()
        .map(|p| p.sort_order)
        .max()
        .unwrap_or(0)
        + 1;
    let new_id = Uuid::new_v4().to_string();
    let duplicate = ProviderInstance {
        id: new_id.clone(),
        name: format!("{} (copy)", source.name),
        kind: source.kind,
        base_url: source.base_url,
        paper_model: source.paper_model,
        translation_model: source.translation_model,
        models: source.models,
        models_fetched_at: source.models_fetched_at,
        connection_verified_at: None, // credential is NOT copied
        paper_probe: None,            // probe is NOT copied
        sort_order,
    };
    settings.providers.push(duplicate);
    Ok((settings, new_id))
}

pub fn reorder_provider_instances(
    mut settings: StoredModelSettings,
    id_order: &[String],
) -> Result<StoredModelSettings, String> {
    for (index, id) in id_order.iter().enumerate() {
        settings.instance_mut(id)?.sort_order = index as u32;
    }
    settings.providers.sort_by_key(|p| p.sort_order);
    Ok(settings)
}

// ---------------------------------------------------------------------------
// Model ID validation
// ---------------------------------------------------------------------------

pub fn normalize_gemini_model_id(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let id = trimmed.strip_prefix("models/").unwrap_or(trimmed);
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err("Gemini 模型 ID 无效".to_string());
    }
    Ok(id.to_string())
}

pub fn normalize_compatible_model_id(value: &str) -> Result<String, String> {
    let id = value.trim();
    if id.is_empty()
        || !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':' | '/'))
    {
        return Err("兼容模型 ID 无效".to_string());
    }
    Ok(id.to_string())
}

pub fn normalize_model_id(kind: &ProviderKind, value: &str) -> Result<String, String> {
    match kind {
        ProviderKind::Gemini => normalize_gemini_model_id(value),
        _ => normalize_compatible_model_id(value),
    }
}

fn sanitize_gemini_instance(inst: &mut ProviderInstance) -> Result<(), String> {
    if !inst.paper_model.trim().is_empty() {
        inst.paper_model = normalize_gemini_model_id(&inst.paper_model)?;
    }
    if !inst.translation_model.trim().is_empty() {
        inst.translation_model = normalize_gemini_model_id(&inst.translation_model)?;
    }
    inst.models
        .retain(|m| m.supports_generate_content && normalize_gemini_model_id(&m.id).is_ok());
    Ok(())
}

// ---------------------------------------------------------------------------
// Base URL normalization
// ---------------------------------------------------------------------------

pub fn normalize_chat_completions_base_url(value: &str) -> String {
    value.trim().trim_end_matches('/').to_string()
}

pub fn resolved_base_url(inst: &ProviderInstance) -> Result<String, String> {
    match &inst.kind {
        ProviderKind::Grok => Ok(GROK_API_BASE.to_string()),
        ProviderKind::OpenaiCompatible => {
            let normalized = normalize_chat_completions_base_url(
                inst.base_url.as_deref().unwrap_or(DEFAULT_OPENAI_BASE),
            );
            if normalized.is_empty() {
                Ok(DEFAULT_OPENAI_BASE.to_string())
            } else {
                Ok(normalized)
            }
        }
        ProviderKind::GeminiProxy => {
            let normalized = normalize_chat_completions_base_url(
                inst.base_url
                    .as_deref()
                    .unwrap_or(DEFAULT_GEMINI_PROXY_BASE),
            );
            if normalized.is_empty() {
                Ok(DEFAULT_GEMINI_PROXY_BASE.to_string())
            } else {
                Ok(normalized)
            }
        }
        ProviderKind::Gemini => Err("Gemini does not use a base URL".to_string()),
    }
}

// ---------------------------------------------------------------------------
// Paper probe
// ---------------------------------------------------------------------------

pub fn paper_probe_fingerprint(base_url: &str, api_key: &str, paper_model: &str) -> String {
    let base_url = normalize_chat_completions_base_url(base_url);
    let api_key = api_key.trim();
    let paper_model = paper_model.trim();
    let mut hasher = Sha256::new();
    hasher.update(base_url.as_bytes());
    hasher.update(b"\n");
    hasher.update(api_key.as_bytes());
    hasher.update(b"\n");
    hasher.update(paper_model.as_bytes());
    hasher.update(b"\n");
    hasher.update(PAPER_PROBE_VERSION.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn paper_probe_is_valid(inst: &ProviderInstance, api_key: &str) -> bool {
    let Some(probe) = &inst.paper_probe else {
        return false;
    };
    if probe.paper_model != inst.paper_model {
        return false;
    }
    let Ok(base_url) = resolved_base_url(inst) else {
        return false;
    };
    probe.fingerprint == paper_probe_fingerprint(&base_url, api_key, &inst.paper_model)
}

// ---------------------------------------------------------------------------
// Pending connection test (in-memory, per session)
// ---------------------------------------------------------------------------

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Clone)]
pub struct PendingConnectionTest {
    pub instance_id: String,
    pub api_key_hash: String,
    pub base_url: Option<String>,
    pub paper_model: String,
    pub models: Vec<GeminiModelOption>,
    pub paper_probe_passed: bool,
    #[allow(dead_code)]
    pub paper_probe_error: Option<String>,
    pub tested_at: String,
}

// ---------------------------------------------------------------------------
// Save provider settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProviderRequest {
    pub id: String,
    #[serde(default)]
    pub kind: Option<ProviderKind>,
    #[serde(default)]
    pub api_key: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
    pub paper_model: String,
    pub translation_model: String,
}

#[derive(Debug, Clone)]
pub struct ProviderSaveOutcome {
    pub settings: StoredModelSettings,
    pub resolved_key: String,
    pub wrote_new_key: bool,
}

pub fn apply_save_provider(
    mut settings: StoredModelSettings,
    request: &SaveProviderRequest,
    existing_key: Option<String>,
    pending: Option<&PendingConnectionTest>,
) -> Result<ProviderSaveOutcome, String> {
    let inst = settings.instance(&request.id)?;
    let kind = request.kind.as_ref().unwrap_or(&inst.kind).clone();
    let label = kind.label();

    // Normalize model IDs
    let paper_model = normalize_model_id(&kind, &request.paper_model)?;
    let translation_model = normalize_model_id(&kind, &request.translation_model)?;

    // Resolve API key
    let draft_key = request
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_string);
    let existing_key = existing_key
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let resolved_key = draft_key
        .clone()
        .or(existing_key)
        .ok_or_else(|| format!("请先配置并测试 {label} API key"))?;
    let key_hash = sha256_hex(resolved_key.as_bytes());

    // For Gemini: require connection test for new key
    if kind == ProviderKind::Gemini {
        if draft_key.is_some() {
            let matching =
                pending.filter(|t| t.instance_id == request.id && t.api_key_hash == key_hash);
            if matching.is_none() {
                return Err("新 API key 必须先通过「测试连接 / 获取模型」".to_string());
            }
        }

        let inst = settings.instance_mut(&request.id)?;
        inst.kind = kind.clone();
        inst.paper_model = paper_model;
        inst.translation_model = translation_model;
        inst.base_url = None;
        if let Some(tested) =
            pending.filter(|t| t.instance_id == request.id && t.api_key_hash == key_hash)
        {
            inst.models = tested.models.clone();
            inst.models_fetched_at = Some(tested.tested_at.clone());
            inst.connection_verified_at = Some(tested.tested_at.clone());
        }
        if inst.kind == ProviderKind::Gemini {
            sanitize_gemini_instance(inst)?;
        }

        return Ok(ProviderSaveOutcome {
            settings,
            resolved_key,
            wrote_new_key: draft_key.is_some(),
        });
    }

    // For Chat Completions providers (OpenAI-compatible / Grok / Gemini Proxy):
    let base_url_str = match kind.fixed_base_url() {
        Some(fixed) => fixed.to_string(),
        None => {
            let default_base = match kind {
                ProviderKind::GeminiProxy => DEFAULT_GEMINI_PROXY_BASE,
                _ => DEFAULT_OPENAI_BASE,
            };
            let requested = request.base_url.as_deref().unwrap_or(default_base);
            normalize_chat_completions_base_url(requested)
        }
    };

    let matching_test = pending.filter(|t| {
        t.instance_id == request.id
            && t.api_key_hash == key_hash
            && t.base_url.as_deref() == Some(base_url_str.as_str())
            && t.paper_model == paper_model
    });

    if draft_key.is_some() && matching_test.is_none() {
        return Err("新 API key 必须先通过「测试连接 / 获取模型」".to_string());
    }

    // Verify paper probe
    let fingerprint = paper_probe_fingerprint(&base_url_str, &resolved_key, &paper_model);
    let persisted_ok = settings
        .instance(&request.id)?
        .paper_probe
        .as_ref()
        .is_some_and(|p| p.fingerprint == fingerprint && p.paper_model == paper_model);
    let session_ok = matching_test.is_some_and(|t| t.paper_probe_passed);
    if !session_ok && !persisted_ok {
        return Err(format!("{label} 论文模型必须通过 paper-probe-v1 才能保存"));
    }

    {
        let inst = settings.instance_mut(&request.id)?;
        inst.kind = kind.clone();
        inst.paper_model = paper_model.clone();
        inst.translation_model = translation_model;
        if kind.needs_base_url() {
            inst.base_url = Some(base_url_str);
        } else {
            inst.base_url = None;
        }
        if let Some(tested) = matching_test {
            inst.models = tested.models.clone();
            inst.models_fetched_at = Some(tested.tested_at.clone());
            if tested.paper_probe_passed {
                inst.paper_probe = Some(PaperProbeRecord {
                    fingerprint,
                    passed_at: tested.tested_at.clone(),
                    paper_model,
                });
                inst.connection_verified_at = Some(tested.tested_at.clone());
            }
        }
    }

    Ok(ProviderSaveOutcome {
        settings,
        resolved_key,
        wrote_new_key: draft_key.is_some(),
    })
}

// ---------------------------------------------------------------------------
// Clear credential (resets probe and connection status)
// ---------------------------------------------------------------------------

pub fn apply_clear_provider_credential(
    mut settings: StoredModelSettings,
    id: &str,
) -> Result<StoredModelSettings, String> {
    let inst = settings.instance_mut(id)?;
    inst.paper_probe = None;
    inst.connection_verified_at = None;
    Ok(settings)
}

// ---------------------------------------------------------------------------
// Set current provider
// ---------------------------------------------------------------------------

pub fn instance_is_ready(inst: &ProviderInstance, api_key: Option<&str>) -> Result<(), String> {
    let label = inst.kind.label();
    let api_key = api_key
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| format!("请先配置并测试 {label} API key"))?;

    if inst.translation_model.trim().is_empty() {
        return Err("翻译模型不能为空".to_string());
    }

    match &inst.kind {
        ProviderKind::Gemini => {
            if inst.connection_verified_at.is_none() {
                return Err(format!(
                    "{label} configuration has not passed its connection test"
                ));
            }
            if !inst.models.iter().any(|m| {
                m.id == inst.paper_model && m.supports_native_pdf && m.supports_interactions
            }) {
                return Err("论文模型必须同时支持原生 PDF 与 Interactions".to_string());
            }
        }
        ProviderKind::OpenaiCompatible | ProviderKind::Grok | ProviderKind::GeminiProxy => {
            if !paper_probe_is_valid(inst, api_key) {
                return Err(format!("{label} 论文模型尚未通过探针，不能设为当前"));
            }
        }
    }
    Ok(())
}

pub fn apply_set_current_provider(
    mut settings: StoredModelSettings,
    id: &str,
    api_key: Option<&str>,
) -> Result<StoredModelSettings, String> {
    let inst = settings.instance(id)?;
    instance_is_ready(inst, api_key)?;
    settings.current_provider_id = Some(id.to_string());
    Ok(settings)
}

// ---------------------------------------------------------------------------
// Ready paper provider (for actual API calls)
// ---------------------------------------------------------------------------

#[derive(Clone)]
#[allow(dead_code)]
pub struct ReadyPaperProvider {
    pub instance_id: String,
    pub provider: String,
    pub kind: ProviderKind,
    pub paper_model: String,
    pub translation_model: String,
    pub api_key: String,
    pub base_url: Option<String>,
}

impl std::fmt::Debug for ReadyPaperProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ReadyPaperProvider")
            .field("instance_id", &self.instance_id)
            .field("provider", &self.provider)
            .field("kind", &self.kind)
            .field("paper_model", &self.paper_model)
            .field("translation_model", &self.translation_model)
            .field("credential", &"<redacted>")
            .field("endpoint", &self.base_url.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

pub fn ready_paper_provider(
    settings: &StoredModelSettings,
    api_key_lookup: &dyn Fn(&str) -> Option<String>,
) -> Result<ReadyPaperProvider, String> {
    let inst = settings.current_instance().ok_or("没有设置当前 Provider")?;
    let api_key = api_key_lookup(&inst.id)
        .ok_or_else(|| format!("请先配置并测试 {} API key", inst.kind.label()))?;
    instance_is_ready(inst, Some(&api_key))?;

    let base_url = if inst.kind.is_chat_completions() {
        Some(resolved_base_url(inst)?)
    } else {
        None
    };

    Ok(ReadyPaperProvider {
        instance_id: inst.id.clone(),
        provider: inst.kind.as_str().to_string(),
        kind: inst.kind.clone(),
        paper_model: inst.paper_model.clone(),
        translation_model: inst.translation_model.clone(),
        api_key,
        base_url,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_settings_with_gemini_tuple() -> (StoredModelSettings, String) {
        let settings = StoredModelSettings::default();
        add_provider_instance(settings, "My Gemini".to_string(), ProviderKind::Gemini)
            .expect("add gemini")
    }

    #[test]
    fn default_settings_are_empty() {
        let settings = StoredModelSettings::default();
        assert_eq!(settings.schema_version, MODEL_SETTINGS_SCHEMA);
        assert!(settings.current_provider_id.is_none());
        assert!(settings.providers.is_empty());
    }

    #[test]
    fn add_provider_creates_instance_with_uuid() {
        let (settings, id) = make_settings_with_gemini_tuple();
        assert_eq!(settings.providers.len(), 1);
        let inst = settings.instance(&id).expect("find instance");
        assert_eq!(inst.name, "My Gemini");
        assert_eq!(inst.kind, ProviderKind::Gemini);
        assert_eq!(inst.paper_model, DEFAULT_GEMINI_PAPER_MODEL);
        assert_eq!(inst.translation_model, DEFAULT_GEMINI_TRANSLATION_MODEL);
        assert!(!id.is_empty());
    }

    #[test]
    fn add_provider_enforces_max_limit() {
        let mut settings = StoredModelSettings::default();
        for i in 0..MAX_PROVIDER_INSTANCES {
            let (s, _) =
                add_provider_instance(settings, format!("Provider {i}"), ProviderKind::Gemini)
                    .expect("add provider");
            settings = s;
        }
        assert_eq!(settings.providers.len(), MAX_PROVIDER_INSTANCES);
        let err = add_provider_instance(settings, "One too many".to_string(), ProviderKind::Gemini)
            .expect_err("must fail at limit");
        assert!(err.contains("10"), "{err}");
    }

    #[test]
    fn add_provider_rejects_empty_name() {
        let settings = StoredModelSettings::default();
        let err = add_provider_instance(settings, "  ".to_string(), ProviderKind::Gemini)
            .expect_err("empty name");
        assert!(err.contains("不能为空"), "{err}");
    }

    #[test]
    fn remove_provider_clears_current_if_removed() {
        let (mut settings, id) = make_settings_with_gemini_tuple();
        settings.current_provider_id = Some(id.clone());
        let settings = remove_provider_instance(settings, &id).expect("remove");
        assert!(settings.providers.is_empty());
        assert!(settings.current_provider_id.is_none());
    }

    #[test]
    fn rename_provider_updates_name() {
        let (settings, id) = make_settings_with_gemini_tuple();
        let settings =
            rename_provider_instance(settings, &id, "Renamed Gemini".to_string()).expect("rename");
        assert_eq!(settings.instance(&id).unwrap().name, "Renamed Gemini");
    }

    #[test]
    fn rename_rejects_empty_name() {
        let (settings, id) = make_settings_with_gemini_tuple();
        let err = rename_provider_instance(settings, &id, "".to_string()).expect_err("empty name");
        assert!(err.contains("不能为空"), "{err}");
    }

    #[test]
    fn duplicate_creates_copy_without_credentials() {
        let (settings, id) = make_settings_with_gemini_tuple();
        let (settings, dup_id) = duplicate_provider_instance(settings, &id).expect("duplicate");
        assert_eq!(settings.providers.len(), 2);
        let dup = settings.instance(&dup_id).unwrap();
        assert!(dup.name.contains("copy"));
        assert!(dup.connection_verified_at.is_none());
        assert!(dup.paper_probe.is_none());
        assert_ne!(dup.id, id);
    }

    #[test]
    fn reorder_updates_sort_order() {
        let settings = StoredModelSettings::default();
        let (settings, id1) =
            add_provider_instance(settings, "First".to_string(), ProviderKind::Gemini)
                .expect("add 1");
        let (settings, id2) =
            add_provider_instance(settings, "Second".to_string(), ProviderKind::Grok)
                .expect("add 2");
        let (settings, id3) = add_provider_instance(
            settings,
            "Third".to_string(),
            ProviderKind::OpenaiCompatible,
        )
        .expect("add 3");

        // Reverse order
        let settings =
            reorder_provider_instances(settings, &[id3.clone(), id2.clone(), id1.clone()])
                .expect("reorder");

        assert_eq!(settings.providers[0].id, id3);
        assert_eq!(settings.providers[1].id, id2);
        assert_eq!(settings.providers[2].id, id1);
    }

    #[test]
    fn parse_v4_json_roundtrip() {
        let (settings, _) = make_settings_with_gemini_tuple();
        let json = serde_json::to_string(&settings).expect("serialize");
        let parsed = parse_model_settings_json(&json).expect("parse v4");
        assert_eq!(parsed.schema_version, MODEL_SETTINGS_SCHEMA);
        assert_eq!(parsed.providers.len(), 1);
    }

    #[test]
    fn parse_v3_json_returns_fresh_default() {
        let raw = r#"{"schemaVersion":3,"currentProvider":"gemini","gemini":{},"openaiCompatible":{},"grok":{}}"#;
        let settings = parse_model_settings_json(raw).expect("should parse as default");
        assert_eq!(settings.schema_version, MODEL_SETTINGS_SCHEMA);
        assert!(settings.providers.is_empty());
        assert!(settings.current_provider_id.is_none());
    }

    #[test]
    fn openai_compatible_gets_default_base_url() {
        let settings = StoredModelSettings::default();
        let (settings, id) = add_provider_instance(
            settings,
            "OpenAI".to_string(),
            ProviderKind::OpenaiCompatible,
        )
        .expect("add openai");
        let inst = settings.instance(&id).unwrap();
        assert_eq!(inst.base_url.as_deref(), Some(DEFAULT_OPENAI_BASE));
    }

    #[test]
    fn grok_gets_fixed_base_url() {
        let settings = StoredModelSettings::default();
        let (settings, id) =
            add_provider_instance(settings, "Grok".to_string(), ProviderKind::Grok)
                .expect("add grok");
        let inst = settings.instance(&id).unwrap();
        assert_eq!(inst.base_url.as_deref(), Some(GROK_API_BASE));
    }

    #[test]
    fn model_settings_view_marks_current() {
        let settings = StoredModelSettings::default();
        let (mut settings, id) =
            add_provider_instance(settings, "Test".to_string(), ProviderKind::Gemini).expect("add");
        settings.current_provider_id = Some(id.clone());
        let cred_map = std::collections::HashMap::from([(id.clone(), true)]);
        let view = model_settings_view(&settings, &cred_map, false);
        assert_eq!(view.providers.len(), 1);
        assert!(view.providers[0].is_current);
        assert!(view.providers[0].credential_configured);
    }

    #[test]
    fn set_current_provider_validates_gemini_readiness() {
        let (settings, id) = make_settings_with_gemini_tuple();
        // No connection verified, should fail
        let err = apply_set_current_provider(settings, &id, Some("test-key"))
            .expect_err("should fail without connection test");
        assert!(
            err.contains("connection test") || err.contains("Gemini"),
            "{err}"
        );
    }

    #[test]
    fn paper_probe_fingerprint_is_deterministic() {
        let fp1 = paper_probe_fingerprint("https://api.example.com", "key1", "model-a");
        let fp2 = paper_probe_fingerprint("https://api.example.com", "key1", "model-a");
        assert_eq!(fp1, fp2);
        let fp3 = paper_probe_fingerprint("https://api.example.com", "key2", "model-a");
        assert_ne!(fp1, fp3);
    }

    #[test]
    fn constants_are_stable() {
        assert_eq!(GROK_API_BASE, "https://api.x.ai/v1");
        assert_eq!(PAPER_PROBE_VERSION, "paper-probe-v1");
        assert_eq!(DEFAULT_OPENAI_BASE, "https://api.openai.com/v1");
        assert_eq!(MISTRAL_OCR_MODEL, "mistral-ocr-latest");
        assert_eq!(MODEL_SETTINGS_SCHEMA, 4);
        assert_eq!(MAX_PROVIDER_INSTANCES, 10);
    }
}
