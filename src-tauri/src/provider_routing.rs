// Task 1 builds this route boundary before the Task 8 activation switches
// production call sites to it.
#![allow(dead_code)]

use crate::model_settings::{
    instance_is_ready, resolved_base_url, ProviderInstance, ProviderKind, StoredModelSettings,
};
use crate::provider_ports::{
    GeminiInteractionsAdapter, PaperInteractionOutcome, PaperInteractionRequest,
    PaperModelCapabilities, PaperModelPort, PaperStreamRequest, ProviderError, RemoteResource,
    TextInteractionOutcome, TextInteractionRequest,
};
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, Transaction};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::sync::Arc;
use uuid::Uuid;

const PROVIDER_ENDPOINT_VERSION: u32 = 1;
const PROVIDER_ROUTE_VERSION: u32 = 1;
const FROZEN_MODELS_VERSION: u32 = 1;
const MISTRAL_OCR_ENDPOINT_VERSION: u32 = 1;
const MISTRAL_OCR_ROUTE_VERSION: u32 = 1;
const MISTRAL_OCR_BASE_URL: &str = "https://api.mistral.ai/v1";
const ENDPOINT_DOMAIN: &[u8] = b"read-desktop/provider-endpoint/v1";
const ROUTE_DOMAIN: &[u8] = b"read-desktop/provider-route/v1";
const MISTRAL_ENDPOINT_DOMAIN: &[u8] = b"read-desktop/mistral-ocr-endpoint/v1";
const MISTRAL_ROUTE_DOMAIN: &[u8] = b"read-desktop/mistral-ocr-route/v1";
const GEMINI_ENDPOINT_MARKER: &str = "fixed:gemini-native-v1beta";
const GROK_ENDPOINT_MARKER: &str = "fixed:grok-openai-v1";

#[derive(Clone)]
pub(crate) enum PaperAdapter {
    Gemini(GeminiInteractionsAdapter),
    Chat(crate::chat_completions::ChatCompletionsAdapter),
}

pub(crate) fn open_paper_adapter(
    provider: &str,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<PaperAdapter, String> {
    match provider {
        "gemini" => Ok(PaperAdapter::Gemini(
            GeminiInteractionsAdapter::production(api_key).map_err(|error| error.to_string())?,
        )),
        "openai_compatible" => Ok(PaperAdapter::Chat(
            crate::chat_completions::ChatCompletionsAdapter::new(
                "openai_compatible",
                api_key,
                base_url.unwrap_or(crate::model_settings::DEFAULT_OPENAI_BASE),
            )
            .map_err(|error| error.message)?,
        )),
        "gemini_proxy" => Ok(PaperAdapter::Chat(
            crate::chat_completions::ChatCompletionsAdapter::new(
                "gemini_proxy",
                api_key,
                base_url.unwrap_or(crate::model_settings::DEFAULT_GEMINI_PROXY_BASE),
            )
            .map_err(|error| error.message)?,
        )),
        "grok" => Ok(PaperAdapter::Chat(
            crate::chat_completions::ChatCompletionsAdapter::new(
                "grok",
                api_key,
                crate::model_settings::GROK_API_BASE,
            )
            .map_err(|error| error.message)?,
        )),
        other => Err(format!("Unsupported paper provider: {other}")),
    }
}

#[async_trait::async_trait]
impl PaperModelPort for PaperAdapter {
    fn capabilities(&self, model: &str) -> PaperModelCapabilities {
        match self {
            Self::Gemini(inner) => inner.capabilities(model),
            Self::Chat(inner) => inner.capabilities(model),
        }
    }

    async fn interact(
        &self,
        request: PaperInteractionRequest,
    ) -> Result<PaperInteractionOutcome, ProviderError> {
        match self {
            Self::Gemini(inner) => inner.interact(request).await,
            Self::Chat(inner) => inner.interact(request).await,
        }
    }

    async fn interact_stream(
        &self,
        request: PaperStreamRequest,
    ) -> Result<PaperInteractionOutcome, ProviderError> {
        match self {
            Self::Gemini(inner) => inner.interact_stream(request).await,
            Self::Chat(inner) => inner.interact_stream(request).await,
        }
    }

    async fn interact_text(
        &self,
        request: TextInteractionRequest,
    ) -> Result<TextInteractionOutcome, ProviderError> {
        match self {
            Self::Gemini(inner) => inner.interact_text(request).await,
            Self::Chat(inner) => inner.interact_text(request).await,
        }
    }

    async fn delete_remote(&self, resource: &RemoteResource) -> Result<(), ProviderError> {
        match self {
            Self::Gemini(inner) => inner.delete_remote(resource).await,
            Self::Chat(inner) => inner.delete_remote(resource).await,
        }
    }
}

struct ProductionPaperAdapterFactory;

static PRODUCTION_ADAPTER_FACTORY: ProductionPaperAdapterFactory = ProductionPaperAdapterFactory;

impl PaperAdapterFactory for ProductionPaperAdapterFactory {
    fn open(
        &self,
        kind: &ProviderKind,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<Arc<dyn PaperModelPort>, ProviderRoutingError> {
        open_paper_adapter(kind.as_str(), api_key, base_url)
            .map(|adapter| Arc::new(adapter) as Arc<dyn PaperModelPort>)
            .map_err(|_| ProviderRoutingError::AdapterUnavailable)
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) struct ProviderInstanceId(String);

impl ProviderInstanceId {
    pub(crate) fn parse(value: &str) -> Result<Self, ProviderRoutingError> {
        let parsed =
            Uuid::parse_str(value).map_err(|_| ProviderRoutingError::InvalidProviderInstanceId)?;
        Ok(Self(parsed.to_string()))
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ProviderInstanceId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("ProviderInstanceId")
            .field(&self.0)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModelRole {
    Paper,
    Translation,
}

impl ModelRole {
    fn canonical_marker(self) -> &'static [u8] {
        match self {
            Self::Paper => b"paper",
            Self::Translation => b"translation",
        }
    }

    fn as_database(self) -> &'static str {
        match self {
            Self::Paper => "paper",
            Self::Translation => "translation",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrozenModels {
    version: u32,
    paper: String,
    translation: Option<String>,
}

impl FrozenModels {
    pub(crate) fn new<P, T>(paper: P, translation: Option<T>) -> Result<Self, ProviderRoutingError>
    where
        P: Into<String>,
        T: Into<String>,
    {
        let paper = paper.into().trim().to_string();
        let translation = translation
            .map(Into::into)
            .map(|value| value.trim().to_string());
        if paper.is_empty() || translation.as_ref().is_some_and(|model| model.is_empty()) {
            return Err(ProviderRoutingError::InvalidFrozenModels);
        }
        Ok(Self {
            version: FROZEN_MODELS_VERSION,
            paper,
            translation,
        })
    }

    #[allow(dead_code)]
    pub(crate) fn paper(&self) -> &str {
        &self.paper
    }

    #[allow(dead_code)]
    pub(crate) fn translation(&self) -> Option<&str> {
        self.translation.as_deref()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct EndpointScope([u8; 32]);

impl fmt::Debug for EndpointScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("EndpointScope(<redacted>)")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ProviderRouteId([u8; 32]);

impl fmt::Debug for ProviderRouteId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProviderRouteId(<redacted>)")
    }
}

impl ProviderRouteId {
    pub(crate) fn database_value(&self) -> String {
        encode_identity(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SafeProviderRouteSummary {
    pub(crate) instance_id: String,
    pub(crate) instance_name: String,
    pub(crate) kind: ProviderKind,
    pub(crate) paper_model: String,
    pub(crate) translation_model: Option<String>,
    pub(crate) operation: String,
    pub(crate) endpoint_label: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
struct NormalizedBaseUrl(String);

impl NormalizedBaseUrl {
    fn parse(value: &str) -> Result<Self, ProviderRoutingError> {
        let mut url =
            reqwest::Url::parse(value.trim()).map_err(|_| ProviderRoutingError::InvalidEndpoint)?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(ProviderRoutingError::InvalidEndpoint);
        }
        let path = url.path().trim_end_matches('/').to_string();
        url.set_path(if path.is_empty() { "/" } else { &path });
        let canonical = url.to_string().trim_end_matches('/').to_string();
        Ok(Self(canonical))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for NormalizedBaseUrl {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("NormalizedBaseUrl(<redacted>)")
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ProviderEndpointSnapshot {
    version: u32,
    endpoint_scope: EndpointScope,
    provider_instance_id: ProviderInstanceId,
    provider_name_at_capture: String,
    provider_kind: ProviderKind,
    base_url: Option<NormalizedBaseUrl>,
}

impl fmt::Debug for ProviderEndpointSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderEndpointSnapshot")
            .field("version", &self.version)
            .field("provider_instance_id", &self.provider_instance_id)
            .field("provider_name_at_capture", &self.provider_name_at_capture)
            .field("provider_kind", &self.provider_kind)
            .field("endpoint", &"<redacted>")
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct FrozenProviderRoute {
    version: u32,
    endpoint: ProviderEndpointSnapshot,
    route_id: ProviderRouteId,
    models: FrozenModels,
    operation: ModelRole,
}

impl FrozenProviderRoute {
    pub(crate) fn instance_id(&self) -> &ProviderInstanceId {
        &self.endpoint.provider_instance_id
    }

    pub(crate) fn provider_kind(&self) -> &ProviderKind {
        &self.endpoint.provider_kind
    }

    pub(crate) fn models(&self) -> &FrozenModels {
        &self.models
    }

    pub(crate) fn operation(&self) -> ModelRole {
        self.operation
    }

    pub(crate) fn endpoint_base_url(&self) -> Option<&str> {
        self.endpoint
            .base_url
            .as_ref()
            .map(NormalizedBaseUrl::as_str)
    }

    pub(crate) fn endpoint_scope(&self) -> &EndpointScope {
        &self.endpoint.endpoint_scope
    }

    pub(crate) fn route_id(&self) -> &ProviderRouteId {
        &self.route_id
    }

    pub(crate) fn safe_summary(&self) -> SafeProviderRouteSummary {
        let endpoint_label = self.endpoint.base_url.as_ref().and_then(|base_url| {
            let url = reqwest::Url::parse(base_url.as_str()).ok()?;
            let host = url.host_str()?;
            Some(match url.port() {
                Some(port) => format!("{host}:{port}"),
                None => host.to_string(),
            })
        });
        SafeProviderRouteSummary {
            instance_id: self.instance_id().as_str().to_string(),
            instance_name: self.endpoint.provider_name_at_capture.clone(),
            kind: self.provider_kind().clone(),
            paper_model: self.models.paper.clone(),
            translation_model: self.models.translation.clone(),
            operation: self.operation.as_database().to_string(),
            endpoint_label: endpoint_label.or_else(|| {
                Some(match self.provider_kind() {
                    ProviderKind::Gemini => "Gemini".to_string(),
                    ProviderKind::Grok => "Grok".to_string(),
                    other => other.label().to_string(),
                })
            }),
        }
    }
}

impl fmt::Debug for FrozenProviderRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FrozenProviderRoute")
            .field("version", &self.version)
            .field("instance_id", self.instance_id())
            .field("provider_kind", self.provider_kind())
            .field("models", &self.models)
            .field("operation", &self.operation)
            .field("identity", &"<redacted>")
            .finish()
    }
}

/// A frozen Mistral OCR endpoint. It owns only a keyed endpoint identity
/// and model, never a fake paper-provider UUID.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct FrozenMistralOcrRoute {
    version: u32,
    endpoint_scope: EndpointScope,
    route_id: ProviderRouteId,
    model: String,
}

impl FrozenMistralOcrRoute {
    pub(crate) fn route_id(&self) -> &ProviderRouteId {
        &self.route_id
    }

    pub(crate) fn endpoint_scope_database_value(&self) -> String {
        encode_identity(&self.endpoint_scope.0)
    }

    pub(crate) fn model(&self) -> &str {
        &self.model
    }
}

impl fmt::Debug for FrozenMistralOcrRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FrozenMistralOcrRoute")
            .field("version", &self.version)
            .field("model", &self.model)
            .field("identity", &"<redacted>")
            .finish()
    }
}

pub(crate) struct PersistedMistralOcrRouteIds {
    route_id: String,
    endpoint_scope: String,
}

impl PersistedMistralOcrRouteIds {
    pub(crate) fn route_id_database_value(&self) -> &str {
        &self.route_id
    }

    pub(crate) fn endpoint_scope_database_value(&self) -> &str {
        &self.endpoint_scope
    }
}

pub(crate) struct BoundProviderRoute {
    frozen: FrozenProviderRoute,
    adapter: Arc<dyn PaperModelPort>,
}

impl BoundProviderRoute {
    pub(crate) fn frozen(&self) -> &FrozenProviderRoute {
        &self.frozen
    }

    #[allow(dead_code)]
    pub(crate) fn freeze(&self) -> FrozenProviderRoute {
        self.frozen.clone()
    }

    #[allow(dead_code)]
    pub(crate) fn adapter(&self) -> &dyn PaperModelPort {
        self.adapter.as_ref()
    }

    pub(crate) fn adapter_arc(&self) -> Arc<dyn PaperModelPort> {
        Arc::clone(&self.adapter)
    }
}

impl fmt::Debug for BoundProviderRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundProviderRoute")
            .field("instance_id", self.frozen.instance_id())
            .field("provider_kind", self.frozen.provider_kind())
            .field("models", self.frozen.models())
            .field("operation", &self.frozen.operation())
            .field("identity", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProviderRequirementCode {
    ProviderMissing,
    CredentialMissing,
    ProviderNotReady,
    ProviderKindChanged,
    EndpointChanged,
    CredentialChanged,
    UnsupportedSnapshotVersion,
    InvalidSnapshot,
    LegacyAmbiguous,
    LegacyUnattributed,
    ModelMismatch,
    EndpointMismatch,
    ProviderCommitted,
    Quarantined,
    RouteConflict,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct ProviderRequirement {
    code: ProviderRequirementCode,
    instance_id: Option<ProviderInstanceId>,
    provider_kind: Option<ProviderKind>,
    can_rebind: bool,
}

impl ProviderRequirement {
    fn new(
        code: ProviderRequirementCode,
        instance_id: Option<ProviderInstanceId>,
        provider_kind: Option<ProviderKind>,
    ) -> Self {
        Self {
            code,
            instance_id,
            provider_kind,
            can_rebind: false,
        }
    }

    pub(crate) fn for_job(
        code: ProviderRequirementCode,
        instance_id: Option<ProviderInstanceId>,
        provider_kind: Option<ProviderKind>,
        can_rebind: bool,
    ) -> Self {
        Self {
            code,
            instance_id,
            provider_kind,
            can_rebind,
        }
    }

    pub(crate) fn code(&self) -> ProviderRequirementCode {
        self.code
    }

    pub(crate) fn instance_id(&self) -> Option<&ProviderInstanceId> {
        self.instance_id.as_ref()
    }

    pub(crate) fn provider_kind(&self) -> Option<&ProviderKind> {
        self.provider_kind.as_ref()
    }

    pub(crate) fn can_rebind(&self) -> bool {
        self.can_rebind
    }

    pub(crate) fn database_code(&self) -> &'static str {
        match self.code {
            ProviderRequirementCode::ProviderMissing => "provider_missing",
            ProviderRequirementCode::CredentialMissing => "credential_missing",
            ProviderRequirementCode::ProviderNotReady => "provider_not_ready",
            ProviderRequirementCode::ProviderKindChanged => "provider_kind_changed",
            ProviderRequirementCode::EndpointChanged => "endpoint_changed",
            ProviderRequirementCode::CredentialChanged => "credential_changed",
            ProviderRequirementCode::UnsupportedSnapshotVersion => "unsupported_snapshot_version",
            ProviderRequirementCode::InvalidSnapshot => "invalid_snapshot",
            ProviderRequirementCode::LegacyAmbiguous => "legacy_ambiguous",
            ProviderRequirementCode::LegacyUnattributed => "legacy_unattributed",
            ProviderRequirementCode::ModelMismatch => "model_mismatch",
            ProviderRequirementCode::EndpointMismatch => "endpoint_mismatch",
            ProviderRequirementCode::ProviderCommitted => "provider_committed",
            ProviderRequirementCode::Quarantined => "quarantined",
            ProviderRequirementCode::RouteConflict => "route_conflict",
        }
    }

    pub(crate) fn from_database(
        code: &str,
        instance_id: Option<&str>,
        provider_kind: Option<&str>,
        can_rebind: bool,
    ) -> Result<Self, ProviderSnapshotError> {
        let code = match code {
            "provider_missing" => ProviderRequirementCode::ProviderMissing,
            "credential_missing" => ProviderRequirementCode::CredentialMissing,
            "provider_not_ready" => ProviderRequirementCode::ProviderNotReady,
            "provider_kind_changed" => ProviderRequirementCode::ProviderKindChanged,
            "endpoint_changed" => ProviderRequirementCode::EndpointChanged,
            "credential_changed" => ProviderRequirementCode::CredentialChanged,
            "unsupported_snapshot_version" => ProviderRequirementCode::UnsupportedSnapshotVersion,
            "invalid_snapshot" => ProviderRequirementCode::InvalidSnapshot,
            "legacy_ambiguous" => ProviderRequirementCode::LegacyAmbiguous,
            "legacy_unattributed" => ProviderRequirementCode::LegacyUnattributed,
            "model_mismatch" => ProviderRequirementCode::ModelMismatch,
            "endpoint_mismatch" => ProviderRequirementCode::EndpointMismatch,
            "provider_committed" => ProviderRequirementCode::ProviderCommitted,
            "quarantined" => ProviderRequirementCode::Quarantined,
            "route_conflict" => ProviderRequirementCode::RouteConflict,
            _ => return Err(ProviderSnapshotError::InvalidSnapshot),
        };
        let instance_id = instance_id
            .map(ProviderInstanceId::parse)
            .transpose()
            .map_err(|_| ProviderSnapshotError::InvalidSnapshot)?;
        let provider_kind = provider_kind.map(parse_provider_kind).transpose()?;
        Ok(Self::for_job(code, instance_id, provider_kind, can_rebind))
    }
}

impl fmt::Debug for ProviderRequirement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProviderRequirement")
            .field("code", &self.code)
            .field("instance_id", &self.instance_id)
            .field("provider_kind", &self.provider_kind)
            .field("can_rebind", &self.can_rebind)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) enum ProviderRouteDecision<T> {
    Ready(T),
    ActionRequired(ProviderRequirement),
}

impl<T> ProviderRouteDecision<T> {
    pub(crate) fn requirement(&self) -> Option<&ProviderRequirement> {
        match self {
            Self::Ready(_) => None,
            Self::ActionRequired(requirement) => Some(requirement),
        }
    }

    #[cfg(test)]
    fn expect_ready(self, message: &str) -> T {
        match self {
            Self::Ready(value) => value,
            Self::ActionRequired(requirement) => panic!("{message}: {requirement:?}"),
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum ProviderSelection {
    Current,
    Instance(ProviderInstanceId),
}

pub(crate) trait ProviderCredentialPort {
    fn read_exact(
        &self,
        instance_id: &ProviderInstanceId,
    ) -> Result<Option<String>, ProviderRoutingError>;
}

pub(crate) trait PaperAdapterFactory {
    fn open(
        &self,
        kind: &ProviderKind,
        api_key: &str,
        base_url: Option<&str>,
    ) -> Result<Arc<dyn PaperModelPort>, ProviderRoutingError>;
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderRoutingError {
    InvalidProviderInstanceId,
    InvalidFrozenModels,
    InvalidEndpoint,
    CredentialStoreUnavailable,
    AdapterUnavailable,
}

impl fmt::Debug for ProviderRoutingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for ProviderRoutingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidProviderInstanceId => "Provider instance ID is invalid",
            Self::InvalidFrozenModels => "Frozen provider models are invalid",
            Self::InvalidEndpoint => "Provider endpoint is invalid",
            Self::CredentialStoreUnavailable => "Provider credential store is unavailable",
            Self::AdapterUnavailable => "Provider adapter is unavailable",
        };
        formatter.write_str(message)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderSnapshotError {
    InvalidSnapshot,
    SnapshotConflict,
    Database,
}

impl fmt::Debug for ProviderSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, formatter)
    }
}

impl fmt::Display for ProviderSnapshotError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidSnapshot => "Provider route snapshot is invalid",
            Self::SnapshotConflict => "Provider route snapshot conflicts with stored identity",
            Self::Database => "Provider route snapshot database operation failed",
        })
    }
}

#[derive(Serialize, Deserialize)]
struct FrozenMistralOcrRouteSnapshot {
    version: u32,
    model: String,
}

#[derive(Serialize, Deserialize)]
struct FrozenModelsSnapshot {
    version: u32,
    paper: String,
    translation: Option<String>,
}

pub(crate) struct PersistedProviderRouteIds {
    route_id: String,
}

impl PersistedProviderRouteIds {
    pub(crate) fn route_id_database_value(&self) -> &str {
        &self.route_id
    }
}

pub(crate) struct ProviderRouting<'a> {
    settings: &'a StoredModelSettings,
    credentials: &'a dyn ProviderCredentialPort,
    adapters: &'a dyn PaperAdapterFactory,
}

impl<'a> ProviderRouting<'a> {
    pub(crate) fn new(
        settings: &'a StoredModelSettings,
        credentials: &'a dyn ProviderCredentialPort,
    ) -> Self {
        Self::with_factory(settings, credentials, &PRODUCTION_ADAPTER_FACTORY)
    }

    pub(crate) fn with_factory(
        settings: &'a StoredModelSettings,
        credentials: &'a dyn ProviderCredentialPort,
        adapters: &'a dyn PaperAdapterFactory,
    ) -> Self {
        Self {
            settings,
            credentials,
            adapters,
        }
    }

    pub(crate) fn capture(
        &self,
        selection: ProviderSelection,
        models: FrozenModels,
        operation: ModelRole,
    ) -> Result<ProviderRouteDecision<BoundProviderRoute>, ProviderRoutingError> {
        let instance = match self.selected_instance(&selection)? {
            Some(instance) => instance,
            None => {
                let instance_id = match selection {
                    ProviderSelection::Current => None,
                    ProviderSelection::Instance(instance_id) => Some(instance_id),
                };
                return Ok(ProviderRouteDecision::ActionRequired(
                    ProviderRequirement::new(
                        ProviderRequirementCode::ProviderMissing,
                        instance_id,
                        None,
                    ),
                ));
            }
        };
        let instance_id = ProviderInstanceId::parse(&instance.id)?;
        if !models_match_instance(instance, &models, operation) {
            return Err(ProviderRoutingError::InvalidFrozenModels);
        }
        let Some(api_key) = self.credentials.read_exact(&instance_id)? else {
            return Ok(ProviderRouteDecision::ActionRequired(
                ProviderRequirement::new(
                    ProviderRequirementCode::CredentialMissing,
                    Some(instance_id),
                    Some(instance.kind.clone()),
                ),
            ));
        };
        if instance_is_ready(instance, Some(&api_key)).is_err() {
            return Ok(ProviderRouteDecision::ActionRequired(
                ProviderRequirement::new(
                    ProviderRequirementCode::ProviderNotReady,
                    Some(instance_id),
                    Some(instance.kind.clone()),
                ),
            ));
        }

        let (base_url, endpoint_marker) = endpoint_for(instance)?;
        let endpoint_scope = endpoint_scope(
            &instance_id,
            &instance.kind,
            endpoint_marker.as_bytes(),
            api_key.as_bytes(),
        );
        let endpoint = ProviderEndpointSnapshot {
            version: PROVIDER_ENDPOINT_VERSION,
            endpoint_scope,
            provider_instance_id: instance_id,
            provider_name_at_capture: instance.name.clone(),
            provider_kind: instance.kind.clone(),
            base_url,
        };
        let route_id = route_id(&endpoint.endpoint_scope, &models, operation);
        let adapter = self.adapters.open(
            &instance.kind,
            &api_key,
            endpoint.base_url.as_ref().map(NormalizedBaseUrl::as_str),
        )?;
        Ok(ProviderRouteDecision::Ready(BoundProviderRoute {
            frozen: FrozenProviderRoute {
                version: PROVIDER_ROUTE_VERSION,
                endpoint,
                route_id,
                models,
                operation,
            },
            adapter,
        }))
    }

    pub(crate) fn bind(
        &self,
        frozen: &FrozenProviderRoute,
    ) -> Result<ProviderRouteDecision<BoundProviderRoute>, ProviderRoutingError> {
        if frozen.version != PROVIDER_ROUTE_VERSION
            || frozen.endpoint.version != PROVIDER_ENDPOINT_VERSION
            || frozen.models.version != FROZEN_MODELS_VERSION
        {
            return Ok(action_required(
                ProviderRequirementCode::UnsupportedSnapshotVersion,
                Some(frozen.instance_id().clone()),
                Some(frozen.provider_kind().clone()),
            ));
        }
        let expected_route_id =
            route_id(frozen.endpoint_scope(), frozen.models(), frozen.operation());
        if !constant_time_equal(&expected_route_id.0, &frozen.route_id.0) {
            return Ok(action_required(
                ProviderRequirementCode::InvalidSnapshot,
                Some(frozen.instance_id().clone()),
                Some(frozen.provider_kind().clone()),
            ));
        }

        let Some(instance) = self
            .settings
            .providers
            .iter()
            .find(|instance| instance.id == frozen.instance_id().as_str())
        else {
            return Ok(action_required(
                ProviderRequirementCode::ProviderMissing,
                Some(frozen.instance_id().clone()),
                Some(frozen.provider_kind().clone()),
            ));
        };
        if &instance.kind != frozen.provider_kind() {
            return Ok(action_required(
                ProviderRequirementCode::ProviderKindChanged,
                Some(frozen.instance_id().clone()),
                Some(instance.kind.clone()),
            ));
        }

        let Some(api_key) = self.credentials.read_exact(frozen.instance_id())? else {
            return Ok(action_required(
                ProviderRequirementCode::CredentialMissing,
                Some(frozen.instance_id().clone()),
                Some(instance.kind.clone()),
            ));
        };
        if instance_is_ready(instance, Some(&api_key)).is_err() {
            return Ok(action_required(
                ProviderRequirementCode::ProviderNotReady,
                Some(frozen.instance_id().clone()),
                Some(instance.kind.clone()),
            ));
        }

        let (base_url, endpoint_marker) = match endpoint_for(instance) {
            Ok(endpoint) => endpoint,
            Err(ProviderRoutingError::InvalidEndpoint) => {
                return Ok(action_required(
                    ProviderRequirementCode::EndpointChanged,
                    Some(frozen.instance_id().clone()),
                    Some(instance.kind.clone()),
                ));
            }
            Err(error) => return Err(error),
        };
        if base_url != frozen.endpoint.base_url {
            return Ok(action_required(
                ProviderRequirementCode::EndpointChanged,
                Some(frozen.instance_id().clone()),
                Some(instance.kind.clone()),
            ));
        }
        let current_scope = endpoint_scope(
            frozen.instance_id(),
            &instance.kind,
            endpoint_marker.as_bytes(),
            api_key.as_bytes(),
        );
        if !constant_time_equal(&current_scope.0, &frozen.endpoint.endpoint_scope.0) {
            return Ok(action_required(
                ProviderRequirementCode::CredentialChanged,
                Some(frozen.instance_id().clone()),
                Some(instance.kind.clone()),
            ));
        }

        let adapter = self.adapters.open(
            &instance.kind,
            &api_key,
            base_url.as_ref().map(NormalizedBaseUrl::as_str),
        )?;
        Ok(ProviderRouteDecision::Ready(BoundProviderRoute {
            frozen: frozen.clone(),
            adapter,
        }))
    }

    /// Rebinds only the immutable endpoint identity for durable cleanup.
    /// Model readiness deliberately does not participate: a model selection
    /// change cannot redirect or strand a remote deletion.
    pub(crate) fn bind_endpoint(
        &self,
        frozen: &FrozenProviderRoute,
    ) -> Result<ProviderRouteDecision<BoundProviderRoute>, ProviderRoutingError> {
        if frozen.version != PROVIDER_ROUTE_VERSION
            || frozen.endpoint.version != PROVIDER_ENDPOINT_VERSION
            || frozen.models.version != FROZEN_MODELS_VERSION
        {
            return Ok(action_required(
                ProviderRequirementCode::UnsupportedSnapshotVersion,
                Some(frozen.instance_id().clone()),
                Some(frozen.provider_kind().clone()),
            ));
        }
        let expected_route_id =
            route_id(frozen.endpoint_scope(), frozen.models(), frozen.operation());
        if !constant_time_equal(&expected_route_id.0, &frozen.route_id.0) {
            return Ok(action_required(
                ProviderRequirementCode::InvalidSnapshot,
                Some(frozen.instance_id().clone()),
                Some(frozen.provider_kind().clone()),
            ));
        }
        let Some(instance) = self
            .settings
            .providers
            .iter()
            .find(|instance| instance.id == frozen.instance_id().as_str())
        else {
            return Ok(action_required(
                ProviderRequirementCode::ProviderMissing,
                Some(frozen.instance_id().clone()),
                Some(frozen.provider_kind().clone()),
            ));
        };
        if &instance.kind != frozen.provider_kind() {
            return Ok(action_required(
                ProviderRequirementCode::ProviderKindChanged,
                Some(frozen.instance_id().clone()),
                Some(instance.kind.clone()),
            ));
        }
        let Some(api_key) = self.credentials.read_exact(frozen.instance_id())? else {
            return Ok(action_required(
                ProviderRequirementCode::CredentialMissing,
                Some(frozen.instance_id().clone()),
                Some(instance.kind.clone()),
            ));
        };
        let (base_url, endpoint_marker) = match endpoint_for(instance) {
            Ok(endpoint) => endpoint,
            Err(ProviderRoutingError::InvalidEndpoint) => {
                return Ok(action_required(
                    ProviderRequirementCode::EndpointChanged,
                    Some(frozen.instance_id().clone()),
                    Some(instance.kind.clone()),
                ));
            }
            Err(error) => return Err(error),
        };
        if base_url != frozen.endpoint.base_url {
            return Ok(action_required(
                ProviderRequirementCode::EndpointChanged,
                Some(frozen.instance_id().clone()),
                Some(instance.kind.clone()),
            ));
        }
        let current_scope = endpoint_scope(
            frozen.instance_id(),
            &instance.kind,
            endpoint_marker.as_bytes(),
            api_key.as_bytes(),
        );
        if !constant_time_equal(&current_scope.0, &frozen.endpoint.endpoint_scope.0) {
            return Ok(action_required(
                ProviderRequirementCode::CredentialChanged,
                Some(frozen.instance_id().clone()),
                Some(instance.kind.clone()),
            ));
        }
        let adapter = self.adapters.open(
            &instance.kind,
            &api_key,
            base_url.as_ref().map(NormalizedBaseUrl::as_str),
        )?;
        Ok(ProviderRouteDecision::Ready(BoundProviderRoute {
            frozen: frozen.clone(),
            adapter,
        }))
    }

    fn selected_instance(
        &self,
        selection: &ProviderSelection,
    ) -> Result<Option<&ProviderInstance>, ProviderRoutingError> {
        let id = match selection {
            ProviderSelection::Current => match self.settings.current_provider_id.as_deref() {
                Some(id) => ProviderInstanceId::parse(id)?,
                None => return Ok(None),
            },
            ProviderSelection::Instance(id) => id.clone(),
        };
        Ok(self
            .settings
            .providers
            .iter()
            .find(|instance| instance.id == id.as_str()))
    }
}

pub(crate) fn persist_frozen_route(
    transaction: &Transaction<'_>,
    route: &FrozenProviderRoute,
) -> Result<PersistedProviderRouteIds, ProviderSnapshotError> {
    let endpoint_scope = encode_identity(&route.endpoint.endpoint_scope.0);
    let route_id = route.route_id.database_value();
    let models_json = canonical_models_json(&route.models)?;
    let operation = route.operation.as_database();
    let timestamp = Utc::now().to_rfc3339();
    transaction
        .execute(
            "INSERT INTO remote_endpoint_snapshots(
               endpoint_scope, version, owner_type, provider_instance_id,
               provider_name_at_capture, provider_kind, base_url, created_at
             ) VALUES (?1, ?2, 'paper_provider', ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(endpoint_scope) DO NOTHING",
            params![
                endpoint_scope,
                route.endpoint.version,
                route.endpoint.provider_instance_id.as_str(),
                route.endpoint.provider_name_at_capture,
                route.endpoint.provider_kind.as_str(),
                route
                    .endpoint
                    .base_url
                    .as_ref()
                    .map(NormalizedBaseUrl::as_str),
                timestamp,
            ],
        )
        .map_err(|_| ProviderSnapshotError::Database)?;
    let stored_endpoint = transaction
        .query_row(
            "SELECT version, owner_type, provider_instance_id,
                    provider_name_at_capture, provider_kind, base_url
             FROM remote_endpoint_snapshots WHERE endpoint_scope = ?1",
            params![endpoint_scope],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .map_err(|_| ProviderSnapshotError::Database)?;
    let expected_endpoint = (
        i64::from(route.endpoint.version),
        "paper_provider".to_string(),
        Some(route.endpoint.provider_instance_id.as_str().to_string()),
        Some(route.endpoint.provider_name_at_capture.clone()),
        Some(route.endpoint.provider_kind.as_str().to_string()),
        route
            .endpoint
            .base_url
            .as_ref()
            .map(|value| value.as_str().to_string()),
    );
    if stored_endpoint != expected_endpoint {
        return Err(ProviderSnapshotError::SnapshotConflict);
    }

    transaction
        .execute(
            "INSERT INTO provider_route_snapshots(
               route_id, endpoint_scope, version, models_json, operation_role, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT DO NOTHING",
            params![
                route_id,
                endpoint_scope,
                route.version,
                models_json,
                operation,
                timestamp,
            ],
        )
        .map_err(|_| ProviderSnapshotError::Database)?;
    let stored_route = transaction
        .query_row(
            "SELECT endpoint_scope, version, models_json, operation_role
             FROM provider_route_snapshots WHERE route_id = ?1",
            params![route_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|_| ProviderSnapshotError::Database)?;
    let expected_route = (
        endpoint_scope,
        i64::from(route.version),
        models_json,
        operation.to_string(),
    );
    if stored_route.as_ref() != Some(&expected_route) {
        return Err(ProviderSnapshotError::SnapshotConflict);
    }
    Ok(PersistedProviderRouteIds { route_id })
}

pub(crate) fn load_frozen_route(
    connection: &Connection,
    route_id_value: &str,
) -> Result<Option<FrozenProviderRoute>, ProviderSnapshotError> {
    let stored = connection
        .query_row(
            "SELECT r.route_id, r.version, r.models_json, r.operation_role,
                    e.endpoint_scope, e.version, e.owner_type,
                    e.provider_instance_id, e.provider_name_at_capture,
                    e.provider_kind, e.base_url
             FROM provider_route_snapshots r
             JOIN remote_endpoint_snapshots e ON e.endpoint_scope = r.endpoint_scope
             WHERE r.route_id = ?1",
            params![route_id_value],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                ))
            },
        )
        .optional()
        .map_err(|_| ProviderSnapshotError::Database)?;
    let Some((
        stored_route_id,
        route_version,
        models_json,
        operation,
        endpoint_scope,
        endpoint_version,
        owner_type,
        instance_id,
        instance_name,
        provider_kind,
        base_url,
    )) = stored
    else {
        return Ok(None);
    };
    if owner_type != "paper_provider" || stored_route_id != route_id_value {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    let stored_route_identity = ProviderRouteId(decode_identity(route_id_value)?);
    let endpoint_scope = EndpointScope(decode_identity(&endpoint_scope)?);
    let route_version =
        u32::try_from(route_version).map_err(|_| ProviderSnapshotError::InvalidSnapshot)?;
    let endpoint_version =
        u32::try_from(endpoint_version).map_err(|_| ProviderSnapshotError::InvalidSnapshot)?;
    if route_version != PROVIDER_ROUTE_VERSION || endpoint_version != PROVIDER_ENDPOINT_VERSION {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    let instance_id = ProviderInstanceId::parse(
        instance_id
            .as_deref()
            .ok_or(ProviderSnapshotError::InvalidSnapshot)?,
    )
    .map_err(|_| ProviderSnapshotError::InvalidSnapshot)?;
    let provider_kind = parse_provider_kind(
        provider_kind
            .as_deref()
            .ok_or(ProviderSnapshotError::InvalidSnapshot)?,
    )?;
    let base_url = base_url
        .map(|value| {
            let normalized = NormalizedBaseUrl::parse(&value)
                .map_err(|_| ProviderSnapshotError::InvalidSnapshot)?;
            if normalized.as_str() != value {
                return Err(ProviderSnapshotError::InvalidSnapshot);
            }
            Ok(normalized)
        })
        .transpose()?;
    if matches!(provider_kind, ProviderKind::Gemini | ProviderKind::Grok) != base_url.is_none() {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    let models = decode_models_json(&models_json)?;
    let operation = match operation.as_str() {
        "paper" => ModelRole::Paper,
        "translation" => ModelRole::Translation,
        _ => return Err(ProviderSnapshotError::InvalidSnapshot),
    };
    let expected_route_id = route_id(&endpoint_scope, &models, operation);
    if !constant_time_equal(&expected_route_id.0, &stored_route_identity.0) {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    Ok(Some(FrozenProviderRoute {
        version: route_version,
        endpoint: ProviderEndpointSnapshot {
            version: endpoint_version,
            endpoint_scope,
            provider_instance_id: instance_id,
            provider_name_at_capture: instance_name
                .ok_or(ProviderSnapshotError::InvalidSnapshot)?,
            provider_kind,
            base_url,
        },
        route_id: stored_route_identity,
        models,
        operation,
    }))
}

pub(crate) fn capture_mistral_ocr_route(
    api_key: &str,
    model: &str,
) -> Result<FrozenMistralOcrRoute, ProviderSnapshotError> {
    if api_key.is_empty() || model.trim().is_empty() {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    let model = model.trim().to_string();
    let endpoint_scope = mistral_endpoint_scope(api_key.as_bytes());
    let route_id = mistral_ocr_route_id(&endpoint_scope, &model);
    Ok(FrozenMistralOcrRoute {
        version: MISTRAL_OCR_ROUTE_VERSION,
        endpoint_scope,
        route_id,
        model,
    })
}

pub(crate) fn persist_frozen_mistral_ocr_route(
    transaction: &Transaction<'_>,
    route: &FrozenMistralOcrRoute,
) -> Result<PersistedMistralOcrRouteIds, ProviderSnapshotError> {
    if route.version != MISTRAL_OCR_ROUTE_VERSION || route.model.trim().is_empty() {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    let expected_route_id = mistral_ocr_route_id(&route.endpoint_scope, &route.model);
    if !constant_time_equal(&expected_route_id.0, &route.route_id.0) {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    let endpoint_scope = encode_identity(&route.endpoint_scope.0);
    let route_id = route.route_id.database_value();
    let models_json = canonical_mistral_ocr_models_json(route)?;
    let timestamp = Utc::now().to_rfc3339();
    transaction
        .execute(
            "INSERT INTO remote_endpoint_snapshots(
               endpoint_scope, version, owner_type, provider_instance_id,
               provider_name_at_capture, provider_kind, base_url, created_at
             ) VALUES (?1, ?2, 'mistral_ocr', NULL, NULL, NULL, ?3, ?4)
             ON CONFLICT(endpoint_scope) DO NOTHING",
            params![
                endpoint_scope,
                MISTRAL_OCR_ENDPOINT_VERSION,
                MISTRAL_OCR_BASE_URL,
                timestamp,
            ],
        )
        .map_err(|_| ProviderSnapshotError::Database)?;
    let stored_endpoint = transaction
        .query_row(
            "SELECT version, owner_type, provider_instance_id,
                    provider_name_at_capture, provider_kind, base_url
             FROM remote_endpoint_snapshots WHERE endpoint_scope = ?1",
            params![endpoint_scope],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .map_err(|_| ProviderSnapshotError::Database)?;
    let expected_endpoint = (
        i64::from(MISTRAL_OCR_ENDPOINT_VERSION),
        "mistral_ocr".to_string(),
        None,
        None,
        None,
        Some(MISTRAL_OCR_BASE_URL.to_string()),
    );
    if stored_endpoint != expected_endpoint {
        return Err(ProviderSnapshotError::SnapshotConflict);
    }
    transaction
        .execute(
            "INSERT INTO provider_route_snapshots(
               route_id, endpoint_scope, version, models_json, operation_role, created_at
             ) VALUES (?1, ?2, ?3, ?4, 'ocr', ?5)
             ON CONFLICT DO NOTHING",
            params![
                route_id,
                endpoint_scope,
                MISTRAL_OCR_ROUTE_VERSION,
                models_json,
                timestamp,
            ],
        )
        .map_err(|_| ProviderSnapshotError::Database)?;
    let stored_route = transaction
        .query_row(
            "SELECT endpoint_scope, version, models_json, operation_role
             FROM provider_route_snapshots WHERE route_id = ?1",
            params![route_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()
        .map_err(|_| ProviderSnapshotError::Database)?;
    let expected_route = (
        endpoint_scope.clone(),
        i64::from(MISTRAL_OCR_ROUTE_VERSION),
        models_json,
        "ocr".to_string(),
    );
    if stored_route.as_ref() != Some(&expected_route) {
        return Err(ProviderSnapshotError::SnapshotConflict);
    }
    Ok(PersistedMistralOcrRouteIds {
        route_id,
        endpoint_scope,
    })
}

pub(crate) fn load_frozen_mistral_ocr_route(
    connection: &Connection,
    route_id_value: &str,
) -> Result<Option<FrozenMistralOcrRoute>, ProviderSnapshotError> {
    let stored = connection
        .query_row(
            "SELECT r.route_id, r.version, r.models_json, r.operation_role,
                    e.endpoint_scope, e.version, e.owner_type,
                    e.provider_instance_id, e.provider_name_at_capture,
                    e.provider_kind, e.base_url
             FROM provider_route_snapshots r
             JOIN remote_endpoint_snapshots e ON e.endpoint_scope = r.endpoint_scope
             WHERE r.route_id = ?1",
            params![route_id_value],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                    row.get::<_, Option<String>>(9)?,
                    row.get::<_, Option<String>>(10)?,
                ))
            },
        )
        .optional()
        .map_err(|_| ProviderSnapshotError::Database)?;
    let Some((
        stored_route_id,
        route_version,
        models_json,
        operation,
        endpoint_scope,
        endpoint_version,
        owner_type,
        instance_id,
        instance_name,
        provider_kind,
        base_url,
    )) = stored
    else {
        return Ok(None);
    };
    if stored_route_id != route_id_value
        || owner_type != "mistral_ocr"
        || instance_id.is_some()
        || instance_name.is_some()
        || provider_kind.is_some()
        || base_url.as_deref() != Some(MISTRAL_OCR_BASE_URL)
        || operation != "ocr"
    {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    let route_version =
        u32::try_from(route_version).map_err(|_| ProviderSnapshotError::InvalidSnapshot)?;
    let endpoint_version =
        u32::try_from(endpoint_version).map_err(|_| ProviderSnapshotError::InvalidSnapshot)?;
    if route_version != MISTRAL_OCR_ROUTE_VERSION
        || endpoint_version != MISTRAL_OCR_ENDPOINT_VERSION
    {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    let endpoint_scope = EndpointScope(decode_identity(&endpoint_scope)?);
    let route_id = ProviderRouteId(decode_identity(route_id_value)?);
    let snapshot: FrozenMistralOcrRouteSnapshot =
        serde_json::from_str(&models_json).map_err(|_| ProviderSnapshotError::InvalidSnapshot)?;
    if snapshot.version != MISTRAL_OCR_ROUTE_VERSION || snapshot.model.trim().is_empty() {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    let model = snapshot.model.trim().to_string();
    let frozen = FrozenMistralOcrRoute {
        version: route_version,
        endpoint_scope,
        route_id,
        model,
    };
    if canonical_mistral_ocr_models_json(&frozen)? != models_json {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    let expected_route_id = mistral_ocr_route_id(&frozen.endpoint_scope, &frozen.model);
    if !constant_time_equal(&expected_route_id.0, &frozen.route_id.0) {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    Ok(Some(frozen))
}

/// Validates the exact, non-provider-owned Mistral endpoint scope. This never
/// falls back to another credential and exposes no key material.
pub(crate) fn verify_mistral_ocr_endpoint_scope(
    connection: &Connection,
    endpoint_scope_value: &str,
    api_key: &str,
) -> Result<(), ProviderSnapshotError> {
    if api_key.is_empty() {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    let stored = connection
        .query_row(
            "SELECT endpoint_scope, version, owner_type, provider_instance_id,
                    provider_name_at_capture, provider_kind, base_url
             FROM remote_endpoint_snapshots WHERE endpoint_scope = ?1",
            params![endpoint_scope_value],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<String>>(6)?,
                ))
            },
        )
        .optional()
        .map_err(|_| ProviderSnapshotError::Database)?;
    let Some((scope, version, owner, instance, name, kind, base_url)) = stored else {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    };
    if scope != endpoint_scope_value
        || version != i64::from(MISTRAL_OCR_ENDPOINT_VERSION)
        || owner != "mistral_ocr"
        || instance.is_some()
        || name.is_some()
        || kind.is_some()
        || base_url.as_deref() != Some(MISTRAL_OCR_BASE_URL)
    {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    let persisted = EndpointScope(decode_identity(&scope)?);
    let current = mistral_endpoint_scope(api_key.as_bytes());
    if !constant_time_equal(&persisted.0, &current.0) {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    Ok(())
}

fn canonical_mistral_ocr_models_json(
    route: &FrozenMistralOcrRoute,
) -> Result<String, ProviderSnapshotError> {
    serde_json::to_string(&FrozenMistralOcrRouteSnapshot {
        version: route.version,
        model: route.model.clone(),
    })
    .map_err(|_| ProviderSnapshotError::InvalidSnapshot)
}

fn canonical_models_json(models: &FrozenModels) -> Result<String, ProviderSnapshotError> {
    serde_json::to_string(&FrozenModelsSnapshot {
        version: models.version,
        paper: models.paper.clone(),
        translation: models.translation.clone(),
    })
    .map_err(|_| ProviderSnapshotError::InvalidSnapshot)
}

fn decode_models_json(value: &str) -> Result<FrozenModels, ProviderSnapshotError> {
    let stored: FrozenModelsSnapshot =
        serde_json::from_str(value).map_err(|_| ProviderSnapshotError::InvalidSnapshot)?;
    if stored.version != FROZEN_MODELS_VERSION {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    FrozenModels::new(stored.paper, stored.translation)
        .map_err(|_| ProviderSnapshotError::InvalidSnapshot)
}

fn parse_provider_kind(value: &str) -> Result<ProviderKind, ProviderSnapshotError> {
    match value {
        "gemini" => Ok(ProviderKind::Gemini),
        "openai_compatible" => Ok(ProviderKind::OpenaiCompatible),
        "grok" => Ok(ProviderKind::Grok),
        "gemini_proxy" => Ok(ProviderKind::GeminiProxy),
        _ => Err(ProviderSnapshotError::InvalidSnapshot),
    }
}

fn encode_identity(value: &[u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(64);
    for byte in value {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

fn decode_identity(value: &str) -> Result<[u8; 32], ProviderSnapshotError> {
    if value.len() != 64 {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    let mut decoded = [0_u8; 32];
    let bytes = value.as_bytes();
    for (index, slot) in decoded.iter_mut().enumerate() {
        let high = decode_hex_digit(bytes[index * 2])?;
        let low = decode_hex_digit(bytes[index * 2 + 1])?;
        *slot = (high << 4) | low;
    }
    if encode_identity(&decoded) != value {
        return Err(ProviderSnapshotError::InvalidSnapshot);
    }
    Ok(decoded)
}

fn decode_hex_digit(value: u8) -> Result<u8, ProviderSnapshotError> {
    match value {
        b'0'..=b'9' => Ok(value - b'0'),
        b'a'..=b'f' => Ok(value - b'a' + 10),
        _ => Err(ProviderSnapshotError::InvalidSnapshot),
    }
}

fn action_required<T>(
    code: ProviderRequirementCode,
    instance_id: Option<ProviderInstanceId>,
    provider_kind: Option<ProviderKind>,
) -> ProviderRouteDecision<T> {
    ProviderRouteDecision::ActionRequired(ProviderRequirement::new(
        code,
        instance_id,
        provider_kind,
    ))
}

fn endpoint_for(
    instance: &ProviderInstance,
) -> Result<(Option<NormalizedBaseUrl>, String), ProviderRoutingError> {
    match instance.kind {
        ProviderKind::Gemini => Ok((None, GEMINI_ENDPOINT_MARKER.to_string())),
        ProviderKind::Grok => Ok((None, GROK_ENDPOINT_MARKER.to_string())),
        ProviderKind::OpenaiCompatible | ProviderKind::GeminiProxy => {
            let base_url = NormalizedBaseUrl::parse(
                &resolved_base_url(instance).map_err(|_| ProviderRoutingError::InvalidEndpoint)?,
            )?;
            let marker = base_url.as_str().to_string();
            Ok((Some(base_url), marker))
        }
    }
}

fn models_match_instance(
    instance: &ProviderInstance,
    models: &FrozenModels,
    operation: ModelRole,
) -> bool {
    models.paper == instance.paper_model.trim()
        && models
            .translation
            .as_deref()
            .is_none_or(|model| model == instance.translation_model.trim())
        && (!matches!(operation, ModelRole::Translation) || models.translation.is_some())
}

fn endpoint_scope(
    instance_id: &ProviderInstanceId,
    kind: &ProviderKind,
    endpoint_marker: &[u8],
    exact_api_key: &[u8],
) -> EndpointScope {
    let key_digest = Sha256::digest(exact_api_key);
    EndpointScope(hash_fields(&[
        ENDPOINT_DOMAIN,
        instance_id.as_str().as_bytes(),
        kind.as_str().as_bytes(),
        endpoint_marker,
        key_digest.as_slice(),
    ]))
}

fn route_id(
    endpoint_scope: &EndpointScope,
    models: &FrozenModels,
    operation: ModelRole,
) -> ProviderRouteId {
    let version = models.version.to_be_bytes();
    let translation_tag = if models.translation.is_some() {
        b"some".as_slice()
    } else {
        b"none".as_slice()
    };
    let translation = models.translation.as_deref().unwrap_or_default().as_bytes();
    ProviderRouteId(hash_fields(&[
        ROUTE_DOMAIN,
        &endpoint_scope.0,
        &version,
        models.paper.as_bytes(),
        translation_tag,
        translation,
        operation.canonical_marker(),
    ]))
}

fn mistral_endpoint_scope(exact_api_key: &[u8]) -> EndpointScope {
    let key_digest = Sha256::digest(exact_api_key);
    EndpointScope(hash_fields(&[
        MISTRAL_ENDPOINT_DOMAIN,
        MISTRAL_OCR_BASE_URL.as_bytes(),
        key_digest.as_slice(),
    ]))
}

fn mistral_ocr_route_id(endpoint_scope: &EndpointScope, model: &str) -> ProviderRouteId {
    let version = MISTRAL_OCR_ROUTE_VERSION.to_be_bytes();
    ProviderRouteId(hash_fields(&[
        MISTRAL_ROUTE_DOMAIN,
        &endpoint_scope.0,
        &version,
        model.as_bytes(),
        b"ocr",
    ]))
}

fn hash_fields(fields: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update((field.len() as u64).to_be_bytes());
        hasher.update(field);
    }
    hasher.finalize().into()
}

fn constant_time_equal(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right.iter())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_settings::{
        paper_probe_fingerprint, PaperProbeRecord, ProviderInstance, ProviderKind,
        ReadyPaperProvider, StoredModelSettings, DEFAULT_GEMINI_PAPER_MODEL,
        DEFAULT_GEMINI_TRANSLATION_MODEL, GROK_API_BASE,
    };
    use crate::provider_ports::{
        PaperInteractionOutcome, PaperInteractionRequest, PaperModelCapabilities, PaperModelPort,
        ProviderError, RemoteResource, TextInteractionOutcome, TextInteractionRequest,
    };
    use crate::GeminiModelOption;
    use async_trait::async_trait;
    use rusqlite::Connection;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    const TARGET_ID: &str = "11111111-1111-4111-8111-111111111111";
    const OTHER_ID: &str = "22222222-2222-4222-8222-222222222222";
    const TARGET_KEY: &str = "target-sentinel-key";
    const OTHER_KEY: &str = "other-sentinel-key";
    const PROXY_BASE: &str = "http://127.0.0.1:18045/v1";

    #[derive(Default)]
    struct FakeCredentials {
        values: HashMap<String, String>,
    }

    impl ProviderCredentialPort for FakeCredentials {
        fn read_exact(
            &self,
            instance_id: &ProviderInstanceId,
        ) -> Result<Option<String>, ProviderRoutingError> {
            Ok(self.values.get(instance_id.as_str()).cloned())
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct AdapterOpenCall {
        kind: ProviderKind,
        api_key: String,
        base_url: Option<String>,
    }

    #[derive(Default)]
    struct RecordingAdapterFactory {
        calls: Mutex<Vec<AdapterOpenCall>>,
    }

    impl RecordingAdapterFactory {
        fn calls(&self) -> Vec<AdapterOpenCall> {
            self.calls.lock().expect("adapter call lock").clone()
        }
    }

    impl PaperAdapterFactory for RecordingAdapterFactory {
        fn open(
            &self,
            kind: &ProviderKind,
            api_key: &str,
            base_url: Option<&str>,
        ) -> Result<Arc<dyn PaperModelPort>, ProviderRoutingError> {
            self.calls
                .lock()
                .expect("adapter call lock")
                .push(AdapterOpenCall {
                    kind: kind.clone(),
                    api_key: api_key.to_string(),
                    base_url: base_url.map(str::to_string),
                });
            Ok(Arc::new(NoopPaperPort))
        }
    }

    struct NoopPaperPort;

    #[async_trait]
    impl PaperModelPort for NoopPaperPort {
        fn capabilities(&self, _model: &str) -> PaperModelCapabilities {
            PaperModelCapabilities {
                native_pdf: false,
                interactions: false,
                structured_output: false,
                streaming: false,
            }
        }

        async fn interact(
            &self,
            _request: PaperInteractionRequest,
        ) -> Result<PaperInteractionOutcome, ProviderError> {
            unreachable!("capture must not perform HTTP")
        }

        async fn interact_text(
            &self,
            _request: TextInteractionRequest,
        ) -> Result<TextInteractionOutcome, ProviderError> {
            unreachable!("capture must not perform HTTP")
        }

        async fn delete_remote(&self, _resource: &RemoteResource) -> Result<(), ProviderError> {
            unreachable!("capture must not perform HTTP")
        }
    }

    fn ready_proxy(id: &str, name: &str, key: &str) -> ProviderInstance {
        let paper_model = "gemini-3.7-flash-high".to_string();
        ProviderInstance {
            id: id.to_string(),
            name: name.to_string(),
            kind: ProviderKind::GeminiProxy,
            base_url: Some(PROXY_BASE.to_string()),
            paper_model: paper_model.clone(),
            translation_model: "gemini-3.1-flash-lite".to_string(),
            models: Vec::new(),
            models_fetched_at: None,
            connection_verified_at: Some("2026-08-24T00:00:00Z".to_string()),
            paper_probe: Some(PaperProbeRecord {
                fingerprint: paper_probe_fingerprint(PROXY_BASE, key, &paper_model),
                passed_at: "2026-08-24T00:00:00Z".to_string(),
                paper_model,
            }),
            sort_order: 0,
        }
    }

    fn two_proxy_settings() -> StoredModelSettings {
        StoredModelSettings {
            schema_version: crate::model_settings::MODEL_SETTINGS_SCHEMA,
            current_provider_id: Some(TARGET_ID.to_string()),
            providers: vec![
                ready_proxy(TARGET_ID, "Target", TARGET_KEY),
                ready_proxy(OTHER_ID, "Other", OTHER_KEY),
            ],
        }
    }

    fn frozen_models() -> FrozenModels {
        FrozenModels::new("gemini-3.7-flash-high", Some("gemini-3.1-flash-lite"))
            .expect("valid frozen models")
    }

    fn ready_instance(
        id: &str,
        name: &str,
        kind: ProviderKind,
        key: &str,
        configured_base_url: Option<&str>,
    ) -> ProviderInstance {
        let (paper_model, translation_model) = match kind {
            ProviderKind::Gemini => (
                DEFAULT_GEMINI_PAPER_MODEL.to_string(),
                DEFAULT_GEMINI_TRANSLATION_MODEL.to_string(),
            ),
            ProviderKind::OpenaiCompatible => ("gpt-4.1".to_string(), "gpt-4.1-mini".to_string()),
            ProviderKind::Grok => ("grok-4".to_string(), "grok-4-mini".to_string()),
            ProviderKind::GeminiProxy => (
                "gemini-3.7-flash-high".to_string(),
                "gemini-3.1-flash-lite".to_string(),
            ),
        };
        let base_url = match kind {
            ProviderKind::Gemini => None,
            ProviderKind::Grok => Some(GROK_API_BASE.to_string()),
            ProviderKind::OpenaiCompatible | ProviderKind::GeminiProxy => Some(
                configured_base_url
                    .expect("configurable endpoint")
                    .to_string(),
            ),
        };
        let models = if kind == ProviderKind::Gemini {
            vec![GeminiModelOption {
                id: paper_model.clone(),
                display_name: paper_model.clone(),
                description: String::new(),
                input_token_limit: None,
                output_token_limit: None,
                supports_generate_content: true,
                supports_native_pdf: true,
                supports_interactions: true,
            }]
        } else {
            Vec::new()
        };
        let paper_probe = if kind.needs_paper_probe() {
            let probe_base = if kind == ProviderKind::Grok {
                GROK_API_BASE
            } else {
                base_url.as_deref().expect("probe endpoint")
            };
            Some(PaperProbeRecord {
                fingerprint: paper_probe_fingerprint(probe_base, key, &paper_model),
                passed_at: "2026-08-24T00:00:00Z".to_string(),
                paper_model: paper_model.clone(),
            })
        } else {
            None
        };
        ProviderInstance {
            id: id.to_string(),
            name: name.to_string(),
            kind,
            base_url,
            paper_model,
            translation_model,
            models,
            models_fetched_at: None,
            connection_verified_at: Some("2026-08-24T00:00:00Z".to_string()),
            paper_probe,
            sort_order: 0,
        }
    }

    fn capture_frozen(
        instance: ProviderInstance,
        key: &str,
        models: FrozenModels,
        operation: ModelRole,
    ) -> FrozenProviderRoute {
        let id = instance.id.clone();
        let settings = StoredModelSettings {
            schema_version: crate::model_settings::MODEL_SETTINGS_SCHEMA,
            current_provider_id: Some(id.clone()),
            providers: vec![instance],
        };
        let credentials = FakeCredentials {
            values: HashMap::from([(id, key.to_string())]),
        };
        let adapters = RecordingAdapterFactory::default();
        ProviderRouting::with_factory(&settings, &credentials, &adapters)
            .capture(ProviderSelection::Current, models, operation)
            .expect("capture")
            .expect_ready("ready")
            .freeze()
    }

    #[test]
    fn capture_explicit_proxy_uses_exact_instance_key_and_custom_endpoint() {
        let settings = two_proxy_settings();
        let credentials = FakeCredentials {
            values: HashMap::from([
                (TARGET_ID.to_string(), TARGET_KEY.to_string()),
                (OTHER_ID.to_string(), OTHER_KEY.to_string()),
            ]),
        };
        let adapters = RecordingAdapterFactory::default();
        let routing = ProviderRouting::with_factory(&settings, &credentials, &adapters);

        let decision = routing
            .capture(
                ProviderSelection::Instance(
                    ProviderInstanceId::parse(TARGET_ID).expect("target UUID"),
                ),
                frozen_models(),
                ModelRole::Paper,
            )
            .expect("capture result");
        let bound = decision.expect_ready("target route should be ready");

        assert_eq!(bound.frozen().instance_id().as_str(), TARGET_ID);
        assert_eq!(bound.frozen().provider_kind(), &ProviderKind::GeminiProxy);
        assert_eq!(bound.frozen().models(), &frozen_models());
        assert_eq!(bound.frozen().operation(), ModelRole::Paper);
        assert_eq!(
            adapters.calls(),
            vec![AdapterOpenCall {
                kind: ProviderKind::GeminiProxy,
                api_key: TARGET_KEY.to_string(),
                base_url: Some(PROXY_BASE.to_string()),
            }]
        );

        let debug = format!("{bound:?}");
        assert!(!debug.contains(TARGET_KEY));
        assert!(!debug.contains(PROXY_BASE));
        assert!(!debug.contains("endpoint_scope"));
        assert!(!debug.contains("route_id"));
    }

    #[test]
    fn capture_missing_target_key_never_falls_back_or_opens_adapter() {
        let settings = two_proxy_settings();
        let credentials = FakeCredentials {
            values: HashMap::from([(OTHER_ID.to_string(), OTHER_KEY.to_string())]),
        };
        let adapters = RecordingAdapterFactory::default();
        let routing = ProviderRouting::with_factory(&settings, &credentials, &adapters);

        let decision = routing
            .capture(
                ProviderSelection::Instance(
                    ProviderInstanceId::parse(TARGET_ID).expect("target UUID"),
                ),
                frozen_models(),
                ModelRole::Paper,
            )
            .expect("missing credential is an action requirement");

        assert_eq!(
            decision.requirement().map(|requirement| requirement.code()),
            Some(ProviderRequirementCode::CredentialMissing)
        );
        assert_eq!(
            decision
                .requirement()
                .and_then(|requirement| requirement.instance_id())
                .map(ProviderInstanceId::as_str),
            Some(TARGET_ID)
        );
        assert!(
            adapters.calls().is_empty(),
            "adapter/HTTP seam must stay closed"
        );
    }

    #[test]
    fn capture_rejects_models_not_selected_on_instance_without_opening_adapter() {
        let instance = ready_instance(
            TARGET_ID,
            "Provider",
            ProviderKind::OpenaiCompatible,
            TARGET_KEY,
            Some("https://api.example.com/v1"),
        );
        let settings = StoredModelSettings {
            schema_version: crate::model_settings::MODEL_SETTINGS_SCHEMA,
            current_provider_id: Some(TARGET_ID.to_string()),
            providers: vec![instance],
        };
        let credentials = FakeCredentials {
            values: HashMap::from([(TARGET_ID.to_string(), TARGET_KEY.to_string())]),
        };
        let adapters = RecordingAdapterFactory::default();
        let routing = ProviderRouting::with_factory(&settings, &credentials, &adapters);
        let mismatches = [
            (
                FrozenModels::new("gpt-4.2", Some("gpt-4.1-mini")).expect("paper mismatch"),
                ModelRole::Paper,
            ),
            (
                FrozenModels::new("gpt-4.1", Some("gpt-4.2-mini")).expect("translation mismatch"),
                ModelRole::Paper,
            ),
            (
                FrozenModels::new("gpt-4.1", Option::<String>::None).expect("missing translation"),
                ModelRole::Translation,
            ),
        ];

        for (models, operation) in mismatches {
            let error = routing
                .capture(ProviderSelection::Current, models, operation)
                .expect_err("unselected models must fail closed");
            assert_eq!(error, ProviderRoutingError::InvalidFrozenModels);
        }
        assert!(
            adapters.calls().is_empty(),
            "adapter/HTTP seam must stay closed"
        );
    }

    #[test]
    fn capture_supports_four_kinds_and_canonicalizes_only_configurable_endpoints() {
        let cases = [
            (
                ProviderKind::Gemini,
                None,
                DEFAULT_GEMINI_PAPER_MODEL,
                Some(DEFAULT_GEMINI_TRANSLATION_MODEL),
            ),
            (
                ProviderKind::OpenaiCompatible,
                Some("HTTPS://API.Example.COM:443/v1/"),
                "gpt-4.1",
                Some("gpt-4.1-mini"),
            ),
            (ProviderKind::Grok, None, "grok-4", Some("grok-4-mini")),
            (
                ProviderKind::GeminiProxy,
                Some(PROXY_BASE),
                "gemini-3.7-flash-high",
                Some("gemini-3.1-flash-lite"),
            ),
        ];

        for (index, (kind, base_url, paper, translation)) in cases.into_iter().enumerate() {
            let id = format!("33333333-3333-4333-8333-33333333333{index}");
            let key = format!("kind-key-{index}");
            let instance = ready_instance(&id, "Provider", kind.clone(), &key, base_url);
            let settings = StoredModelSettings {
                schema_version: crate::model_settings::MODEL_SETTINGS_SCHEMA,
                current_provider_id: Some(id.clone()),
                providers: vec![instance],
            };
            let credentials = FakeCredentials {
                values: HashMap::from([(id, key.clone())]),
            };
            let adapters = RecordingAdapterFactory::default();
            let bound = ProviderRouting::with_factory(&settings, &credentials, &adapters)
                .capture(
                    ProviderSelection::Current,
                    FrozenModels::new(paper, translation).expect("models"),
                    ModelRole::Paper,
                )
                .expect("capture")
                .expect_ready("ready");
            let calls = adapters.calls();
            assert_eq!(calls.len(), 1);
            assert_eq!(calls[0].kind, kind);
            assert_eq!(calls[0].api_key, key);
            match kind {
                ProviderKind::Gemini | ProviderKind::Grok => {
                    assert_eq!(calls[0].base_url, None);
                    assert!(bound.frozen().endpoint_base_url().is_none());
                }
                ProviderKind::OpenaiCompatible => {
                    assert_eq!(
                        calls[0].base_url.as_deref(),
                        Some("https://api.example.com/v1")
                    );
                }
                ProviderKind::GeminiProxy => {
                    assert_eq!(calls[0].base_url.as_deref(), Some(PROXY_BASE));
                }
            }
        }
    }

    #[test]
    fn capture_rejects_unsafe_urls_without_opening_adapter() {
        let unsafe_urls = [
            "ftp://api.example.com/v1",
            "https://user:password@api.example.com/v1",
            "https://api.example.com/v1?tenant=secret",
            "https://api.example.com/v1#secret",
            "https://",
        ];
        for (index, unsafe_url) in unsafe_urls.into_iter().enumerate() {
            let id = format!("44444444-4444-4444-8444-44444444444{index}");
            let key = format!("unsafe-key-{index}");
            let instance = ready_instance(
                &id,
                "Unsafe",
                ProviderKind::OpenaiCompatible,
                &key,
                Some(unsafe_url),
            );
            let settings = StoredModelSettings {
                schema_version: crate::model_settings::MODEL_SETTINGS_SCHEMA,
                current_provider_id: Some(id.clone()),
                providers: vec![instance],
            };
            let credentials = FakeCredentials {
                values: HashMap::from([(id, key)]),
            };
            let adapters = RecordingAdapterFactory::default();
            let error = ProviderRouting::with_factory(&settings, &credentials, &adapters)
                .capture(
                    ProviderSelection::Current,
                    FrozenModels::new("gpt-4.1", Some("gpt-4.1-mini")).expect("models"),
                    ModelRole::Paper,
                )
                .expect_err("unsafe endpoint must fail");
            assert_eq!(error, ProviderRoutingError::InvalidEndpoint);
            assert!(adapters.calls().is_empty());
            assert!(!format!("{error}").contains(unsafe_url));
        }
    }

    #[test]
    fn endpoint_canonicalization_handles_root_trailing_slashes_and_local_http() {
        let root = NormalizedBaseUrl::parse("HTTPS://Example.COM:443///").expect("root");
        assert_eq!(root.as_str(), "https://example.com");
        let local = NormalizedBaseUrl::parse("http://localhost:8045/v1///").expect("localhost");
        assert_eq!(local.as_str(), "http://localhost:8045/v1");
        let loopback = NormalizedBaseUrl::parse("http://127.0.0.1:8045/").expect("loopback");
        assert_eq!(loopback.as_str(), "http://127.0.0.1:8045");
    }

    #[test]
    fn endpoint_and_route_identity_change_only_for_their_owned_inputs() {
        let base = ready_instance(
            TARGET_ID,
            "Original name",
            ProviderKind::OpenaiCompatible,
            TARGET_KEY,
            Some("https://api.example.com/v1"),
        );
        let route = capture_frozen(
            base.clone(),
            TARGET_KEY,
            FrozenModels::new("gpt-4.1", Some("gpt-4.1-mini")).expect("models"),
            ModelRole::Paper,
        );

        let mut renamed = base.clone();
        renamed.name = "Renamed".to_string();
        let renamed_route = capture_frozen(
            renamed,
            TARGET_KEY,
            FrozenModels::new("gpt-4.1", Some("gpt-4.1-mini")).expect("models"),
            ModelRole::Paper,
        );
        assert_eq!(route.endpoint_scope(), renamed_route.endpoint_scope());
        assert_eq!(route.route_id(), renamed_route.route_id());

        let changed_uuid = ready_instance(
            OTHER_ID,
            "Original name",
            ProviderKind::OpenaiCompatible,
            TARGET_KEY,
            Some("https://api.example.com/v1"),
        );
        let uuid_route = capture_frozen(
            changed_uuid,
            TARGET_KEY,
            FrozenModels::new("gpt-4.1", Some("gpt-4.1-mini")).expect("models"),
            ModelRole::Paper,
        );
        assert_ne!(route.endpoint_scope(), uuid_route.endpoint_scope());

        let changed_key = ready_instance(
            TARGET_ID,
            "Original name",
            ProviderKind::OpenaiCompatible,
            OTHER_KEY,
            Some("https://api.example.com/v1"),
        );
        let key_route = capture_frozen(
            changed_key,
            OTHER_KEY,
            FrozenModels::new("gpt-4.1", Some("gpt-4.1-mini")).expect("models"),
            ModelRole::Paper,
        );
        assert_ne!(route.endpoint_scope(), key_route.endpoint_scope());

        let changed_endpoint = ready_instance(
            TARGET_ID,
            "Original name",
            ProviderKind::OpenaiCompatible,
            TARGET_KEY,
            Some("https://other.example.com/v1"),
        );
        let endpoint_route = capture_frozen(
            changed_endpoint,
            TARGET_KEY,
            FrozenModels::new("gpt-4.1", Some("gpt-4.1-mini")).expect("models"),
            ModelRole::Paper,
        );
        assert_ne!(route.endpoint_scope(), endpoint_route.endpoint_scope());

        let paper_model_route = route_id(
            route.endpoint_scope(),
            &FrozenModels::new("gpt-4.2", Some("gpt-4.1-mini")).expect("paper model"),
            ModelRole::Paper,
        );
        let translation_model_route = route_id(
            route.endpoint_scope(),
            &FrozenModels::new("gpt-4.1", Some("gpt-4.1-nano")).expect("translation model"),
            ModelRole::Paper,
        );
        let without_translation_route = route_id(
            route.endpoint_scope(),
            &FrozenModels::new("gpt-4.1", Option::<String>::None).expect("no translation"),
            ModelRole::Paper,
        );
        let operation_route = route_id(
            route.endpoint_scope(),
            route.models(),
            ModelRole::Translation,
        );
        assert_ne!(route.route_id(), &paper_model_route);
        assert_ne!(route.route_id(), &translation_model_route);
        assert_ne!(route.route_id(), &without_translation_route);
        assert_ne!(route.route_id(), &operation_route);
    }

    #[test]
    fn canonical_route_encoding_has_no_model_field_boundary_collision() {
        let endpoint = EndpointScope([7; 32]);
        let left = FrozenModels::new("ab", Some("c")).expect("left");
        let right = FrozenModels::new("a", Some("bc")).expect("right");
        assert_ne!(
            route_id(&endpoint, &left, ModelRole::Paper),
            route_id(&endpoint, &right, ModelRole::Paper)
        );
    }

    #[test]
    fn bind_fails_closed_with_typed_requirements() {
        let original = ready_instance(
            TARGET_ID,
            "Original",
            ProviderKind::OpenaiCompatible,
            TARGET_KEY,
            Some("https://api.example.com/v1"),
        );
        let frozen = capture_frozen(
            original.clone(),
            TARGET_KEY,
            FrozenModels::new("gpt-4.1", Some("gpt-4.1-mini")).expect("models"),
            ModelRole::Paper,
        );

        let cases = [
            (
                Vec::new(),
                HashMap::new(),
                frozen.clone(),
                ProviderRequirementCode::ProviderMissing,
            ),
            (
                vec![ready_instance(
                    TARGET_ID,
                    "Changed kind",
                    ProviderKind::GeminiProxy,
                    TARGET_KEY,
                    Some("https://api.example.com/v1"),
                )],
                HashMap::from([(TARGET_ID.to_string(), TARGET_KEY.to_string())]),
                frozen.clone(),
                ProviderRequirementCode::ProviderKindChanged,
            ),
            (
                vec![ready_instance(
                    TARGET_ID,
                    "Changed endpoint",
                    ProviderKind::OpenaiCompatible,
                    TARGET_KEY,
                    Some("https://other.example.com/v1"),
                )],
                HashMap::from([(TARGET_ID.to_string(), TARGET_KEY.to_string())]),
                frozen.clone(),
                ProviderRequirementCode::EndpointChanged,
            ),
            (
                vec![ready_instance(
                    TARGET_ID,
                    "Changed key",
                    ProviderKind::OpenaiCompatible,
                    OTHER_KEY,
                    Some("https://api.example.com/v1"),
                )],
                HashMap::from([(TARGET_ID.to_string(), OTHER_KEY.to_string())]),
                frozen.clone(),
                ProviderRequirementCode::CredentialChanged,
            ),
            {
                let mut future = frozen.clone();
                future.version += 1;
                (
                    vec![original.clone()],
                    HashMap::from([(TARGET_ID.to_string(), TARGET_KEY.to_string())]),
                    future,
                    ProviderRequirementCode::UnsupportedSnapshotVersion,
                )
            },
            {
                let mut future_endpoint = frozen.clone();
                future_endpoint.endpoint.version += 1;
                (
                    vec![original.clone()],
                    HashMap::from([(TARGET_ID.to_string(), TARGET_KEY.to_string())]),
                    future_endpoint,
                    ProviderRequirementCode::UnsupportedSnapshotVersion,
                )
            },
            {
                let mut future_models = frozen.clone();
                future_models.models.version += 1;
                future_models.route_id = route_id(
                    future_models.endpoint_scope(),
                    future_models.models(),
                    future_models.operation(),
                );
                (
                    vec![original.clone()],
                    HashMap::from([(TARGET_ID.to_string(), TARGET_KEY.to_string())]),
                    future_models,
                    ProviderRequirementCode::UnsupportedSnapshotVersion,
                )
            },
            {
                let mut corrupt = frozen.clone();
                corrupt.route_id.0[0] ^= 1;
                (
                    vec![original.clone()],
                    HashMap::from([(TARGET_ID.to_string(), TARGET_KEY.to_string())]),
                    corrupt,
                    ProviderRequirementCode::InvalidSnapshot,
                )
            },
            (
                vec![original.clone()],
                HashMap::new(),
                frozen.clone(),
                ProviderRequirementCode::CredentialMissing,
            ),
            {
                let mut not_ready = original.clone();
                not_ready.paper_probe = None;
                (
                    vec![not_ready],
                    HashMap::from([(TARGET_ID.to_string(), TARGET_KEY.to_string())]),
                    frozen.clone(),
                    ProviderRequirementCode::ProviderNotReady,
                )
            },
        ];

        for (providers, values, snapshot, expected) in cases {
            let settings = StoredModelSettings {
                schema_version: crate::model_settings::MODEL_SETTINGS_SCHEMA,
                current_provider_id: None,
                providers,
            };
            let credentials = FakeCredentials { values };
            let adapters = RecordingAdapterFactory::default();
            let decision = ProviderRouting::with_factory(&settings, &credentials, &adapters)
                .bind(&snapshot)
                .expect("bind decision");
            assert_eq!(
                decision.requirement().map(ProviderRequirement::code),
                Some(expected)
            );
            assert!(adapters.calls().is_empty());
        }
    }

    #[test]
    fn bind_reuses_frozen_models_after_safe_rename() {
        let original = ready_instance(
            TARGET_ID,
            "Original",
            ProviderKind::OpenaiCompatible,
            TARGET_KEY,
            Some("https://api.example.com/v1"),
        );
        let frozen = capture_frozen(
            original.clone(),
            TARGET_KEY,
            FrozenModels::new("gpt-4.1", Some("gpt-4.1-mini")).expect("models"),
            ModelRole::Paper,
        );
        let mut renamed = original;
        renamed.name = "Renamed safely".to_string();
        let settings = StoredModelSettings {
            schema_version: crate::model_settings::MODEL_SETTINGS_SCHEMA,
            current_provider_id: None,
            providers: vec![renamed],
        };
        let credentials = FakeCredentials {
            values: HashMap::from([(TARGET_ID.to_string(), TARGET_KEY.to_string())]),
        };
        let adapters = RecordingAdapterFactory::default();
        let rebound = ProviderRouting::with_factory(&settings, &credentials, &adapters)
            .bind(&frozen)
            .expect("bind")
            .expect_ready("rename must not change route");
        assert_eq!(rebound.frozen(), &frozen);
        assert_eq!(adapters.calls().len(), 1);
    }

    #[test]
    fn ready_provider_and_frozen_route_debug_are_redacted() {
        let ready = ReadyPaperProvider {
            instance_id: TARGET_ID.to_string(),
            provider: "openai_compatible".to_string(),
            kind: ProviderKind::OpenaiCompatible,
            paper_model: "gpt-4.1".to_string(),
            translation_model: "gpt-4.1-mini".to_string(),
            api_key: TARGET_KEY.to_string(),
            base_url: Some("https://secret.example.com/private/v1".to_string()),
        };
        let debug = format!("{ready:?}");
        assert!(!debug.contains(TARGET_KEY));
        assert!(!debug.contains("secret.example.com"));

        let frozen = capture_frozen(
            ready_instance(
                TARGET_ID,
                "Provider",
                ProviderKind::OpenaiCompatible,
                TARGET_KEY,
                Some("https://secret.example.com/private/v1"),
            ),
            TARGET_KEY,
            FrozenModels::new("gpt-4.1", Some("gpt-4.1-mini")).expect("models"),
            ModelRole::Paper,
        );
        let debug = format!("{frozen:?}");
        assert!(!debug.contains(TARGET_KEY));
        assert!(!debug.contains("secret.example.com"));
        assert!(!debug.contains("endpoint_scope"));
        assert!(!debug.contains("route_id"));
    }

    #[test]
    fn frozen_route_persistence_round_trips_through_the_single_owner() {
        let frozen = capture_frozen(
            ready_instance(
                TARGET_ID,
                "Provider",
                ProviderKind::OpenaiCompatible,
                TARGET_KEY,
                Some("https://api.example.com/private/v1"),
            ),
            TARGET_KEY,
            FrozenModels::new("gpt-4.1", Some("gpt-4.1-mini")).expect("models"),
            ModelRole::Paper,
        );
        let mut connection = Connection::open_in_memory().expect("database");
        connection
            .execute_batch(
                "CREATE TABLE remote_endpoint_snapshots (
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
                 );",
            )
            .expect("schema");
        let transaction = connection.transaction().expect("transaction");

        let ids = persist_frozen_route(&transaction, &frozen).expect("persist route");
        persist_frozen_route(&transaction, &frozen).expect("idempotent persist");
        transaction.commit().expect("commit");

        let loaded = load_frozen_route(&connection, ids.route_id_database_value())
            .expect("load route")
            .expect("persisted route");
        assert_eq!(loaded, frozen);
        assert_eq!(
            loaded.safe_summary().endpoint_label.as_deref(),
            Some("api.example.com")
        );
        assert!(!format!("{:?}", loaded.safe_summary()).contains("/private/v1"));
    }

    #[test]
    fn mistral_ocr_capture_binds_scope_to_key_and_route_to_key_and_model() {
        let original = capture_mistral_ocr_route("mistral-key-a", "mistral-ocr-latest")
            .expect("capture original Mistral route");
        let changed_key = capture_mistral_ocr_route("mistral-key-b", "mistral-ocr-latest")
            .expect("capture route with changed key");
        let changed_model = capture_mistral_ocr_route("mistral-key-a", "mistral-ocr-2407")
            .expect("capture route with changed model");

        assert_ne!(
            original.endpoint_scope_database_value(),
            changed_key.endpoint_scope_database_value(),
            "an exact credential change must produce a distinct endpoint scope"
        );
        assert_ne!(
            original.route_id(),
            changed_key.route_id(),
            "an exact credential change must produce a distinct route"
        );
        assert_eq!(
            original.endpoint_scope_database_value(),
            changed_model.endpoint_scope_database_value(),
            "model selection must not rewrite the endpoint identity"
        );
        assert_ne!(
            original.route_id(),
            changed_model.route_id(),
            "model selection must produce a distinct durable route"
        );
    }

    #[test]
    fn mistral_ocr_route_persistence_round_trips_and_verifies_only_the_exact_key() {
        let frozen = capture_mistral_ocr_route("mistral-exact-key", "mistral-ocr-latest")
            .expect("capture Mistral route");
        let mut connection = Connection::open_in_memory().expect("database");
        connection
            .execute_batch(
                "CREATE TABLE remote_endpoint_snapshots (
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
                 );",
            )
            .expect("schema");
        let transaction = connection.transaction().expect("transaction");

        let ids =
            persist_frozen_mistral_ocr_route(&transaction, &frozen).expect("persist Mistral route");
        persist_frozen_mistral_ocr_route(&transaction, &frozen)
            .expect("idempotent Mistral persistence");
        transaction.commit().expect("commit");

        let loaded = load_frozen_mistral_ocr_route(&connection, ids.route_id_database_value())
            .expect("load Mistral route")
            .expect("persisted Mistral route");
        assert_eq!(loaded, frozen);
        assert_eq!(
            ids.endpoint_scope_database_value(),
            frozen.endpoint_scope_database_value()
        );
        verify_mistral_ocr_endpoint_scope(
            &connection,
            ids.endpoint_scope_database_value(),
            "mistral-exact-key",
        )
        .expect("the captured credential must verify");
        assert_eq!(
            verify_mistral_ocr_endpoint_scope(
                &connection,
                ids.endpoint_scope_database_value(),
                "mistral-wrong-key",
            ),
            Err(ProviderSnapshotError::InvalidSnapshot),
            "a different credential must fail closed instead of being rebound"
        );
    }

    #[test]
    fn mistral_ocr_route_debug_redacts_key_and_full_endpoint_url() {
        let key = "mistral-private-sentinel-key";
        let route =
            capture_mistral_ocr_route(key, "mistral-ocr-latest").expect("capture Mistral route");

        let debug = format!("{route:?}");
        assert!(!debug.contains(key));
        assert!(!debug.contains(MISTRAL_OCR_BASE_URL));
        assert!(!debug.contains("api.mistral.ai"));
        assert!(!debug.contains(&route.endpoint_scope_database_value()));
        assert!(!debug.contains(&route.route_id().database_value()));
    }
}
