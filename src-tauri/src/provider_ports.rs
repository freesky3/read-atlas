use crate::artifact_module::{OcrBlockInput, OcrPageInput};
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::{HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

static GEMINI_CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();
static MISTRAL_CLIENT: OnceLock<Result<reqwest::Client, String>> = OnceLock::new();

fn shared_client(
    slot: &'static OnceLock<Result<reqwest::Client, String>>,
    timeout: Duration,
) -> ProviderResult<reqwest::Client> {
    slot.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| error.to_string())
    })
    .clone()
    .map_err(ProviderError::local_state)
}

pub type ProviderResult<T> = Result<T, ProviderError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorKind {
    Unauthorized,
    RateLimited,
    Timeout,
    InvalidResponse,
    StaleRemoteResource,
    Cancelled,
    Transport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteResource {
    pub provider: String,
    pub kind: String,
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderError {
    pub kind: ProviderErrorKind,
    pub message: String,
    pub retry_after_seconds: Option<u64>,
    pub orphaned_resource: Option<RemoteResource>,
}

impl ProviderError {
    fn new(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            retry_after_seconds: None,
            orphaned_resource: None,
        }
    }

    pub fn local_state(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::Transport, message)
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ProviderErrorKind::InvalidResponse, message)
    }

    pub fn cancelled() -> Self {
        Self::new(
            ProviderErrorKind::Cancelled,
            "Provider request was cancelled",
        )
    }

    fn with_orphaned_resource(mut self, resource: RemoteResource) -> Self {
        if self.orphaned_resource.is_none() {
            self.orphaned_resource = Some(resource);
        }
        self
    }
}

impl fmt::Display for ProviderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ProviderError {}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageEnvelope {
    pub provider: String,
    pub model: String,
    pub context_epoch: Option<String>,
    pub input_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
    pub uncached_input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub reasoning_tokens: Option<i64>,
    pub latency_ms: Option<i64>,
    pub estimated_cost: Option<String>,
    pub file_reuse: Option<bool>,
    pub session_resume: Option<bool>,
    pub paper_root_branch: Option<bool>,
}

impl UsageEnvelope {
    pub fn cache_hit_rate(&self) -> Option<f64> {
        match (self.input_tokens, self.cached_input_tokens) {
            (Some(input), Some(cached)) if input > 0 => Some(cached as f64 / input as f64),
            _ => None,
        }
    }

    fn with_uncached(mut self) -> Self {
        if self.uncached_input_tokens.is_none() {
            self.uncached_input_tokens = match (self.input_tokens, self.cached_input_tokens) {
                (Some(input), Some(cached)) => Some((input - cached).max(0)),
                _ => None,
            };
        }
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaperInteractionKind {
    Root,
    #[default]
    Discussion,
    Artifact,
    Lens,
    LensFollowUp,
    ContextCompaction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineImageInput {
    pub mime_type: String,
    pub base64_data: String,
}

#[derive(Debug, Clone)]
pub struct PaperInteractionRequest {
    pub model: String,
    pub context_epoch: String,
    pub pdf_path: PathBuf,
    pub display_name: String,
    pub remote_file_id: Option<String>,
    pub previous_interaction_id: Option<String>,
    pub system_instruction: String,
    pub user_input: String,
    pub response_schema: Option<Value>,
    pub inline_images: Vec<InlineImageInput>,
    /// Distinguishes paper-root, discussion, artifact, and lens calls.
    /// Adapters do not branch on it yet; callers still tag each request.
    #[allow(dead_code)]
    pub kind: PaperInteractionKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperInteractionOutcome {
    pub text: String,
    pub provider_node_id: String,
    pub provider_file_id: String,
    pub receipt: UsageEnvelope,
}

#[derive(Debug, Clone)]
pub struct PaperStreamRequest {
    pub interaction: PaperInteractionRequest,
    pub cancellation: CancellationFlag,
    pub deltas: mpsc::UnboundedSender<String>,
}

#[derive(Debug, Clone)]
pub struct TextInteractionRequest {
    pub model: String,
    pub context_epoch: String,
    pub system_instruction: String,
    pub user_input: String,
    pub response_schema: Option<Value>,
    pub kind: PaperInteractionKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextInteractionOutcome {
    pub text: String,
    pub provider_node_id: String,
    pub receipt: UsageEnvelope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PaperModelCapabilities {
    pub native_pdf: bool,
    pub interactions: bool,
    pub structured_output: bool,
    pub streaming: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CancellationFlag(Arc<AtomicBool>);

impl CancellationFlag {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub fn check(&self) -> ProviderResult<()> {
        if self.is_cancelled() {
            Err(ProviderError::cancelled())
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderProgress {
    pub stage: String,
    pub completed: u64,
    pub total: Option<u64>,
}

#[derive(Clone)]
pub struct OcrRequest {
    pub pdf_path: PathBuf,
    pub display_name: String,
    pub cancellation: CancellationFlag,
    pub progress: Option<mpsc::UnboundedSender<ProviderProgress>>,
    pub raw_staging_path: Option<PathBuf>,
    pub before_provider_commit: Option<Arc<dyn Fn() -> ProviderResult<()> + Send + Sync + 'static>>,
}

impl fmt::Debug for OcrRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OcrRequest")
            .field("pdf_path", &self.pdf_path)
            .field("display_name", &self.display_name)
            .field("cancellation", &self.cancellation)
            .field("has_progress", &self.progress.is_some())
            .field("raw_staging_path", &self.raw_staging_path)
            .field(
                "has_provider_commit_hook",
                &self.before_provider_commit.is_some(),
            )
            .finish()
    }
}

impl OcrRequest {
    fn report(&self, stage: &str, completed: u64, total: Option<u64>) {
        if let Some(sender) = &self.progress {
            let _ = sender.send(ProviderProgress {
                stage: stage.to_string(),
                completed,
                total,
            });
        }
    }

    fn commit_provider(&self) -> ProviderResult<()> {
        if let Some(commit) = &self.before_provider_commit {
            commit()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct OcrOutcome {
    pub pages: Vec<OcrPageInput>,
    pub receipt: UsageEnvelope,
    pub cleanup_warning: Option<String>,
    pub cleanup_resource: Option<RemoteResource>,
}

#[async_trait]
pub trait PaperModelPort: Send + Sync {
    fn capabilities(&self, model: &str) -> PaperModelCapabilities;
    async fn interact(
        &self,
        request: PaperInteractionRequest,
    ) -> ProviderResult<PaperInteractionOutcome>;
    async fn interact_stream(
        &self,
        request: PaperStreamRequest,
    ) -> ProviderResult<PaperInteractionOutcome> {
        request.cancellation.check()?;
        let outcome = self.interact(request.interaction).await?;
        request.cancellation.check()?;
        let _ = request.deltas.send(outcome.text.clone());
        Ok(outcome)
    }
    async fn interact_text(
        &self,
        request: TextInteractionRequest,
    ) -> ProviderResult<TextInteractionOutcome>;
    async fn delete_remote(&self, resource: &RemoteResource) -> ProviderResult<()>;
}

#[async_trait]
pub trait OcrPort: Send + Sync {
    async fn parse_pdf(&self, request: OcrRequest) -> ProviderResult<OcrOutcome>;
    async fn delete_remote(&self, resource: &RemoteResource) -> ProviderResult<()>;
}
/// Safe owner metadata for an adapter that has already been rebound to an
/// exact endpoint snapshot. Credential and endpoint-scope verification stays
/// in `provider_routing`; this port deliberately does not duplicate that hash.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RemoteEndpointOwner {
    PaperProvider {
        provider_instance_id: String,
        resource_provider: String,
    },
    MistralOcr,
}

#[allow(dead_code)]
impl RemoteEndpointOwner {
    pub(crate) fn owner_type(&self) -> &'static str {
        match self {
            Self::PaperProvider { .. } => "paper_provider",
            Self::MistralOcr => "mistral_ocr",
        }
    }

    pub(crate) fn provider_instance_id(&self) -> Option<&str> {
        match self {
            Self::PaperProvider {
                provider_instance_id,
                ..
            } => Some(provider_instance_id),
            Self::MistralOcr => None,
        }
    }

    fn resource_provider(&self) -> &str {
        match self {
            Self::PaperProvider {
                resource_provider, ..
            } => resource_provider,
            Self::MistralOcr => "mistral",
        }
    }
}

#[allow(dead_code)]
#[derive(Clone)]
enum RemoteEndpointDeleteAdapter {
    PaperProvider(Arc<dyn PaperModelPort>),
    MistralOcr(MistralOcrAdapter),
}

/// Delete capability that cannot be detached from its verified endpoint owner.
/// Callers must construct it only after `provider_routing` has verified the
/// current exact credential against the persisted endpoint scope.
#[allow(dead_code)]
#[derive(Clone)]
pub(crate) struct RemoteEndpointDeletePort {
    owner: RemoteEndpointOwner,
    adapter: RemoteEndpointDeleteAdapter,
}

#[allow(dead_code)]
impl RemoteEndpointDeletePort {
    pub(crate) fn paper_provider(
        provider_instance_id: impl Into<String>,
        resource_provider: impl Into<String>,
        adapter: Arc<dyn PaperModelPort>,
    ) -> ProviderResult<Self> {
        let provider_instance_id = provider_instance_id.into();
        let resource_provider = resource_provider.into();
        if provider_instance_id.trim().is_empty() || resource_provider.trim().is_empty() {
            return Err(ProviderError::invalid(
                "Remote endpoint owner metadata is incomplete",
            ));
        }
        Ok(Self {
            owner: RemoteEndpointOwner::PaperProvider {
                provider_instance_id,
                resource_provider,
            },
            adapter: RemoteEndpointDeleteAdapter::PaperProvider(adapter),
        })
    }

    pub(crate) fn mistral_ocr(adapter: MistralOcrAdapter) -> Self {
        Self {
            owner: RemoteEndpointOwner::MistralOcr,
            adapter: RemoteEndpointDeleteAdapter::MistralOcr(adapter),
        }
    }

    pub(crate) fn owner(&self) -> &RemoteEndpointOwner {
        &self.owner
    }

    pub(crate) async fn delete_remote(&self, resource: &RemoteResource) -> ProviderResult<()> {
        if resource.provider != self.owner.resource_provider() {
            return Err(ProviderError::invalid(
                "Remote resource endpoint owner mismatch",
            ));
        }
        match &self.adapter {
            RemoteEndpointDeleteAdapter::PaperProvider(adapter) => {
                adapter.delete_remote(resource).await
            }
            RemoteEndpointDeleteAdapter::MistralOcr(adapter) => {
                adapter.delete_remote(resource).await
            }
        }
    }
}

impl fmt::Debug for RemoteEndpointDeletePort {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemoteEndpointDeletePort")
            .field("owner", &self.owner)
            .field("credential", &"<redacted>")
            .field("endpoint", &"<redacted>")
            .finish()
    }
}

#[derive(Debug, Clone)]
pub struct GeminiInteractionsAdapter {
    client: reqwest::Client,
    api_key: String,
    api_base: String,
    upload_base: String,
}

impl GeminiInteractionsAdapter {
    pub fn production(api_key: impl Into<String>) -> ProviderResult<Self> {
        Self::with_bases(
            api_key,
            "https://generativelanguage.googleapis.com/v1beta",
            "https://generativelanguage.googleapis.com/upload/v1beta",
        )
    }

    fn with_bases(
        api_key: impl Into<String>,
        api_base: impl Into<String>,
        upload_base: impl Into<String>,
    ) -> ProviderResult<Self> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(ProviderError::new(
                ProviderErrorKind::Unauthorized,
                "Gemini credential is missing",
            ));
        }
        let client = shared_client(&GEMINI_CLIENT, Duration::from_secs(120))?;
        Ok(Self {
            client,
            api_key,
            api_base: api_base.into().trim_end_matches('/').to_string(),
            upload_base: upload_base.into().trim_end_matches('/').to_string(),
        })
    }

    async fn upload_pdf(&self, path: &PathBuf, display_name: &str) -> ProviderResult<String> {
        let boundary = format!("read-desktop-{}", uuid::Uuid::new_v4().simple());
        let safe_name: String = display_name
            .chars()
            .filter(|character| !matches!(character, '"' | '\r' | '\n'))
            .collect();
        let metadata = json!({"file": {"display_name": safe_name}}).to_string();
        let prefix = format!(
            "--{boundary}\r\nContent-Type: application/json\r\n\r\n{metadata}\r\n--{boundary}\r\nContent-Type: application/pdf\r\n\r\n"
        )
        .into_bytes();
        let suffix = format!("\r\n--{boundary}--\r\n").into_bytes();
        let file = tokio::fs::File::open(path)
            .await
            .map_err(|error| ProviderError::new(ProviderErrorKind::Transport, error.to_string()))?;
        let file_stream = futures_util::stream::try_unfold(file, |mut file| async move {
            let mut buffer = vec![0_u8; 64 * 1024];
            let read = file.read(&mut buffer).await?;
            if read == 0 {
                Ok(None)
            } else {
                buffer.truncate(read);
                Ok(Some((buffer, file)))
            }
        });
        let body_stream =
            futures_util::stream::once(async move { Ok::<Vec<u8>, std::io::Error>(prefix) })
                .chain(file_stream)
                .chain(futures_util::stream::once(async move {
                    Ok::<Vec<u8>, std::io::Error>(suffix)
                }));

        let response = self
            .client
            .post(format!("{}/files", self.upload_base))
            .query(&[("key", self.api_key.as_str())])
            .header("X-Goog-Upload-Protocol", "multipart")
            .header(
                CONTENT_TYPE,
                HeaderValue::from_str(&format!("multipart/related; boundary={boundary}"))
                    .map_err(|error| ProviderError::invalid(error.to_string()))?,
            )
            .body(reqwest::Body::wrap_stream(body_stream))
            .send()
            .await
            .map_err(transport_error)?;
        let payload = checked_json("gemini", response).await?;
        payload
            .pointer("/file/name")
            .or_else(|| payload.pointer("/file/uri"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ProviderError::invalid("Gemini file response is missing file.name"))
    }

    fn gemini_response_format(schema: &Value) -> Value {
        let json_schema = schema
            .get("schema")
            .filter(|inner| inner.is_object())
            .cloned()
            .unwrap_or_else(|| schema.clone());
        json!({
            "type": "text",
            "mime_type": "application/json",
            "schema": json_schema
        })
    }

    fn body(request: &PaperInteractionRequest, provider_file_id: &str) -> Value {
        let mut content = Vec::new();
        if request.previous_interaction_id.is_none() {
            content.push(json!({
                "type": "document",
                "uri": file_content_uri(provider_file_id),
                "mime_type": "application/pdf"
            }));
        }
        for image in &request.inline_images {
            content.push(json!({
                "type": "image",
                "mime_type": image.mime_type,
                "data": image.base64_data
            }));
        }
        content.push(json!({"type": "text", "text": request.user_input}));
        let mut body = json!({
            "model": request.model,
            "input": content,
            "system_instruction": request.system_instruction,
            "generation_config": {"temperature": 0.2},
            "store": true
        });
        if let Some(parent) = &request.previous_interaction_id {
            body["previous_interaction_id"] = Value::String(parent.clone());
        }
        if let Some(schema) = &request.response_schema {
            body["response_format"] = Self::gemini_response_format(schema);
        }
        body
    }

    fn stream_body(request: &PaperInteractionRequest, provider_file_id: &str) -> Value {
        let mut body = Self::body(request, provider_file_id);
        body["stream"] = Value::Bool(true);
        body
    }

    fn text_body(request: &TextInteractionRequest) -> Value {
        let mut body = json!({
            "model": request.model,
            "input": [{"type": "text", "text": request.user_input}],
            "system_instruction": request.system_instruction,
            "generation_config": {"temperature": 0.1},
            "store": true
        });
        if let Some(schema) = &request.response_schema {
            body["response_format"] = Self::gemini_response_format(schema);
        }
        body
    }
}

#[async_trait]
impl PaperModelPort for GeminiInteractionsAdapter {
    fn capabilities(&self, model: &str) -> PaperModelCapabilities {
        let supported = model.starts_with("gemini-2.5-") || model.starts_with("gemini-3");
        PaperModelCapabilities {
            native_pdf: supported,
            interactions: supported,
            structured_output: supported,
            streaming: supported,
        }
    }

    async fn interact(
        &self,
        request: PaperInteractionRequest,
    ) -> ProviderResult<PaperInteractionOutcome> {
        if !self.capabilities(&request.model).interactions {
            return Err(ProviderError::invalid(
                "Selected Gemini model does not support Interactions",
            ));
        }
        if request.system_instruction.trim().is_empty() || request.user_input.trim().is_empty() {
            return Err(ProviderError::invalid(
                "Interaction instructions and input are required",
            ));
        }
        let file_reuse = request.remote_file_id.is_some();
        let uploaded_here = !file_reuse;
        let provider_file_id = match &request.remote_file_id {
            Some(id) => id.clone(),
            None => {
                self.upload_pdf(&request.pdf_path, &request.display_name)
                    .await?
            }
        };
        let result: ProviderResult<PaperInteractionOutcome> = async {
            let started = Instant::now();
            let response = self
                .client
                .post(format!("{}/interactions", self.api_base))
                .query(&[("key", self.api_key.as_str())])
                .json(&Self::body(&request, &provider_file_id))
                .send()
                .await
                .map_err(transport_error)?;
            let payload = checked_json("gemini", response).await?;
            let provider_node_id = payload
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| ProviderError::invalid("Gemini interaction is missing id"))?;
            let text = interaction_text(&payload)
                .ok_or_else(|| ProviderError::invalid("Gemini interaction has no output text"))?;
            let receipt = parse_gemini_usage(
                &payload,
                &request.model,
                &request.context_epoch,
                started.elapsed(),
                file_reuse,
                request.previous_interaction_id.is_some(),
                request.previous_interaction_id.is_none(),
            );
            Ok(PaperInteractionOutcome {
                text,
                provider_node_id,
                provider_file_id: provider_file_id.clone(),
                receipt,
            })
        }
        .await;
        result.map_err(|error| {
            if uploaded_here {
                error.with_orphaned_resource(RemoteResource {
                    provider: "gemini".to_string(),
                    kind: "file".to_string(),
                    id: provider_file_id.clone(),
                })
            } else {
                error
            }
        })
    }

    async fn interact_stream(
        &self,
        request: PaperStreamRequest,
    ) -> ProviderResult<PaperInteractionOutcome> {
        let interaction = &request.interaction;
        if !self.capabilities(&interaction.model).interactions {
            return Err(ProviderError::invalid(
                "Selected Gemini model does not support Interactions",
            ));
        }
        if interaction.system_instruction.trim().is_empty()
            || interaction.user_input.trim().is_empty()
        {
            return Err(ProviderError::invalid(
                "Interaction instructions and input are required",
            ));
        }
        request.cancellation.check()?;
        let file_reuse = interaction.remote_file_id.is_some();
        let uploaded_here = !file_reuse;
        let provider_file_id = match &interaction.remote_file_id {
            Some(id) => id.clone(),
            None => {
                self.upload_pdf(&interaction.pdf_path, &interaction.display_name)
                    .await?
            }
        };
        let result: ProviderResult<PaperInteractionOutcome> = async {
            request.cancellation.check()?;
            let started = Instant::now();
            let response = self
                .client
                .post(format!("{}/interactions", self.api_base))
                .query(&[("key", self.api_key.as_str())])
                .json(&Self::stream_body(interaction, &provider_file_id))
                .send()
                .await
                .map_err(transport_error)?;
            if !response.status().is_success() {
                return match checked_json("gemini", response).await {
                    Err(error) => Err(error),
                    Ok(_) => Err(ProviderError::invalid(
                        "Gemini returned an unexpected non-success stream response",
                    )),
                };
            }

            let mut response = response;
            let mut buffer = SseBuffer::default();
            let mut accumulated = String::new();
            let mut completed: Option<Value> = None;
            while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
                request.cancellation.check()?;
                buffer.push(&chunk)?;
                while let Some(event) = buffer.next_event() {
                    request.cancellation.check()?;
                    match parse_gemini_stream_event(event)? {
                        GeminiStreamEvent::Delta(delta) => {
                            accumulated.push_str(&delta);
                            let _ = request.deltas.send(delta);
                        }
                        GeminiStreamEvent::Completed(interaction) => {
                            completed = Some(interaction);
                            break;
                        }
                        GeminiStreamEvent::Done | GeminiStreamEvent::Ignored => {}
                    }
                }
                if completed.is_some() {
                    break;
                }
            }
            if completed.is_none() && buffer.has_pending() {
                match parse_gemini_stream_event(buffer.remainder())? {
                    GeminiStreamEvent::Completed(interaction) => completed = Some(interaction),
                    GeminiStreamEvent::Delta(delta) => {
                        accumulated.push_str(&delta);
                        let _ = request.deltas.send(delta);
                    }
                    GeminiStreamEvent::Done | GeminiStreamEvent::Ignored => {}
                }
            }
            let payload = completed.ok_or_else(|| {
                ProviderError::invalid("Gemini stream ended without interaction.completed")
            })?;
            let provider_node_id = payload
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
                .ok_or_else(|| ProviderError::invalid("Gemini interaction is missing id"))?;
            if accumulated.trim().is_empty() {
                accumulated = interaction_text(&payload).ok_or_else(|| {
                    ProviderError::invalid("Gemini interaction has no output text")
                })?;
                let _ = request.deltas.send(accumulated.clone());
            }
            let receipt = parse_gemini_usage(
                &payload,
                &interaction.model,
                &interaction.context_epoch,
                started.elapsed(),
                file_reuse,
                interaction.previous_interaction_id.is_some(),
                interaction.previous_interaction_id.is_none(),
            );
            Ok(PaperInteractionOutcome {
                text: accumulated,
                provider_node_id,
                provider_file_id: provider_file_id.clone(),
                receipt,
            })
        }
        .await;
        result.map_err(|error| {
            if uploaded_here {
                error.with_orphaned_resource(RemoteResource {
                    provider: "gemini".to_string(),
                    kind: "file".to_string(),
                    id: provider_file_id.clone(),
                })
            } else {
                error
            }
        })
    }

    async fn interact_text(
        &self,
        request: TextInteractionRequest,
    ) -> ProviderResult<TextInteractionOutcome> {
        if !self.capabilities(&request.model).interactions {
            return Err(ProviderError::invalid(
                "Selected Gemini model does not support Interactions",
            ));
        }
        if request.system_instruction.trim().is_empty() || request.user_input.trim().is_empty() {
            return Err(ProviderError::invalid(
                "Text interaction instructions and input are required",
            ));
        }
        let started = Instant::now();
        let response = self
            .client
            .post(format!("{}/interactions", self.api_base))
            .query(&[("key", self.api_key.as_str())])
            .json(&Self::text_body(&request))
            .send()
            .await
            .map_err(transport_error)?;
        let payload = checked_json("gemini", response).await?;
        let provider_node_id = payload
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ProviderError::invalid("Gemini interaction is missing id"))?;
        let text = interaction_text(&payload)
            .ok_or_else(|| ProviderError::invalid("Gemini interaction has no output text"))?;
        let receipt = parse_gemini_usage(
            &payload,
            &request.model,
            &request.context_epoch,
            started.elapsed(),
            false,
            false,
            request.kind == PaperInteractionKind::Root,
        );
        Ok(TextInteractionOutcome {
            text,
            provider_node_id,
            receipt,
        })
    }

    async fn delete_remote(&self, resource: &RemoteResource) -> ProviderResult<()> {
        if resource.provider != "gemini" {
            return Err(ProviderError::invalid("Remote resource provider mismatch"));
        }
        let path = match resource.kind.as_str() {
            "file" => resource.id.trim_start_matches('/').to_string(),
            "interaction" => format!("interactions/{}", resource.id),
            _ => return Err(ProviderError::invalid("Unknown Gemini resource kind")),
        };
        let response = self
            .client
            .delete(format!("{}/{}", self.api_base, path))
            .query(&[("key", self.api_key.as_str())])
            .send()
            .await
            .map_err(transport_error)?;
        checked_empty("gemini", response).await
    }
}

#[derive(Clone)]
pub struct MistralOcrAdapter {
    client: reqwest::Client,
    api_key: String,
    api_base: String,
    model: String,
}

impl fmt::Debug for MistralOcrAdapter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MistralOcrAdapter")
            .field("credential", &"<redacted>")
            .field("endpoint", &"<redacted>")
            .field("model", &self.model)
            .finish()
    }
}

impl MistralOcrAdapter {
    pub fn production(api_key: impl Into<String>) -> ProviderResult<Self> {
        Self::with_base(api_key, "https://api.mistral.ai/v1", "mistral-ocr-latest")
    }

    fn with_base(
        api_key: impl Into<String>,
        api_base: impl Into<String>,
        model: impl Into<String>,
    ) -> ProviderResult<Self> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(ProviderError::new(
                ProviderErrorKind::Unauthorized,
                "Mistral credential is missing",
            ));
        }
        let client = shared_client(&MISTRAL_CLIENT, Duration::from_secs(300))?;
        Ok(Self {
            client,
            api_key,
            api_base: api_base.into().trim_end_matches('/').to_string(),
            model: model.into(),
        })
    }

    async fn upload_pdf(&self, request: &OcrRequest) -> ProviderResult<String> {
        request.cancellation.check()?;
        request.report("uploading", 0, None);
        let part = reqwest::multipart::Part::file(&request.pdf_path)
            .await
            .map_err(|error| ProviderError::new(ProviderErrorKind::Transport, error.to_string()))?
            .file_name(request.display_name.clone())
            .mime_str("application/pdf")
            .map_err(|error| ProviderError::invalid(error.to_string()))?;
        let form = reqwest::multipart::Form::new()
            .text("purpose", "ocr")
            .part("file", part);
        let response = self
            .client
            .post(format!("{}/files", self.api_base))
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(transport_error)?;
        let payload = checked_json("mistral", response).await?;
        let id = payload
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ProviderError::invalid("Mistral file response is missing id"))?;
        request.report("uploaded", 1, Some(1));
        Ok(id)
    }

    async fn signed_url(&self, file_id: &str, request: &OcrRequest) -> ProviderResult<String> {
        request.cancellation.check()?;
        request.report("requesting_signed_url", 0, Some(1));
        let response = self
            .client
            .get(format!("{}/files/{file_id}/url", self.api_base))
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(transport_error)?;
        let payload = checked_json("mistral", response).await?;
        let url = payload
            .get("url")
            .or_else(|| payload.get("signed_url"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ProviderError::invalid("Mistral signed URL is missing url"))?;
        request.report("signed_url_ready", 1, Some(1));
        Ok(url)
    }

    async fn run_ocr(&self, url: &str, request: &OcrRequest) -> ProviderResult<(Value, Duration)> {
        request.cancellation.check()?;
        request.report("provider_committed", 0, None);
        request.commit_provider()?;
        let started = Instant::now();
        let response = self
            .client
            .post(format!("{}/ocr", self.api_base))
            .bearer_auth(&self.api_key)
            .json(&json!({
                "model": self.model,
                "document": {"type": "document_url", "document_url": url},
                "include_blocks": true,
                "table_format": "html",
                "include_image_base64": false
            }))
            .send()
            .await
            .map_err(transport_error)?;
        let elapsed = started.elapsed();
        let payload = if let Some(staging_path) = &request.raw_staging_path {
            if !response.status().is_success() {
                checked_json("mistral", response).await?
            } else {
                stage_response_body(response, staging_path, &request.cancellation).await?;
                read_staged_json(staging_path)?
            }
        } else {
            checked_json("mistral", response).await?
        };
        request.report("response_received", 1, Some(1));
        if let Some(staging_path) = &request.raw_staging_path {
            // The successful body has already been atomically copied to the
            // staging path; parsing it once keeps retries resumable without a
            // second serialized in-memory copy.
            let _ = staging_path;
            request.report("response_staged", 1, Some(1));
        }
        Ok((payload, elapsed))
    }

    async fn delete_file(&self, file_id: &str) -> ProviderResult<()> {
        let response = self
            .client
            .delete(format!("{}/files/{file_id}", self.api_base))
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(transport_error)?;
        checked_empty("mistral", response).await
    }
}

#[async_trait]
impl OcrPort for MistralOcrAdapter {
    async fn parse_pdf(&self, request: OcrRequest) -> ProviderResult<OcrOutcome> {
        let file_id = self.upload_pdf(&request).await?;
        let result = async {
            let url = self.signed_url(&file_id, &request).await?;
            let (raw_response, elapsed) = self.run_ocr(&url, &request).await?;
            request.cancellation.check()?;
            request.report("normalizing", 0, None);
            let pages = normalize_mistral_pages(&raw_response)?;
            request.report("normalized", pages.len() as u64, Some(pages.len() as u64));
            Ok(OcrOutcome {
                pages,
                receipt: parse_mistral_usage(&raw_response, &self.model, Some(elapsed)),
                cleanup_warning: None,
                cleanup_resource: None,
            })
        }
        .await;

        let cleanup = self.delete_file(&file_id).await;
        match (result, cleanup) {
            (Ok(outcome), Ok(())) => {
                request.report("remote_cleaned", 1, Some(1));
                Ok(outcome)
            }
            (Ok(mut outcome), Err(error)) => {
                outcome.cleanup_warning = Some(error.message);
                outcome.cleanup_resource = Some(RemoteResource {
                    provider: "mistral".to_string(),
                    kind: "file".to_string(),
                    id: file_id.clone(),
                });
                Ok(outcome)
            }
            (Err(error), Ok(())) => Err(error),
            (Err(mut error), Err(_cleanup_error)) => {
                error.orphaned_resource = Some(RemoteResource {
                    provider: "mistral".to_string(),
                    kind: "file".to_string(),
                    id: file_id,
                });
                error.message = format!("{}; remote cleanup also failed", error.message);
                Err(error)
            }
        }
    }

    async fn delete_remote(&self, resource: &RemoteResource) -> ProviderResult<()> {
        if resource.provider != "mistral" || resource.kind != "file" {
            return Err(ProviderError::invalid("Remote resource provider mismatch"));
        }
        self.delete_file(&resource.id).await
    }
}

#[derive(Debug, Clone, PartialEq)]
enum GeminiStreamEvent {
    Delta(String),
    Completed(Value),
    Done,
    Ignored,
}

#[derive(Debug, Default)]
struct SseBuffer {
    bytes: Vec<u8>,
    cursor: usize,
}

const SSE_MAX_BUFFER_BYTES: usize = 16 * 1024 * 1024;

impl SseBuffer {
    fn push(&mut self, chunk: &[u8]) -> ProviderResult<()> {
        self.compact();
        let pending = self.bytes.len().saturating_sub(self.cursor);
        if pending.saturating_add(chunk.len()) > SSE_MAX_BUFFER_BYTES {
            return Err(ProviderError::invalid(
                "Gemini stream event exceeds the response size limit",
            ));
        }
        self.bytes.extend_from_slice(chunk);
        Ok(())
    }

    #[cfg(test)]
    fn extend_from_slice(&mut self, chunk: &[u8]) {
        self.push(chunk).expect("bounded SSE fixture");
    }

    fn next_event(&mut self) -> Option<&[u8]> {
        self.compact();
        let separators: [&[u8]; 2] = [b"\r\n\r\n", b"\n\n"];
        let (offset, length) = separators
            .iter()
            .filter_map(|separator| {
                self.bytes[self.cursor..]
                    .windows(separator.len())
                    .position(|window| window == *separator)
                    .map(|offset| (offset, separator.len()))
            })
            .min_by_key(|(offset, _)| *offset)?;
        let start = self.cursor;
        self.cursor += offset + length;
        Some(&self.bytes[start..start + offset])
    }

    fn has_pending(&self) -> bool {
        self.cursor < self.bytes.len()
    }

    fn remainder(&self) -> &[u8] {
        &self.bytes[self.cursor..]
    }

    fn compact(&mut self) {
        // Events advance the cursor instead of shifting the whole buffer.  A
        // bounded, amortized compaction keeps an incomplete event from growing
        // without reintroducing per-event `drain` copies.
        if self.cursor >= 64 * 1024 && self.cursor * 2 >= self.bytes.len() {
            self.bytes.drain(..self.cursor);
            self.cursor = 0;
        }
    }
}

fn parse_gemini_stream_event(event: &[u8]) -> ProviderResult<GeminiStreamEvent> {
    let event = std::str::from_utf8(event)
        .map_err(|_| ProviderError::invalid("Gemini stream contains invalid UTF-8"))?;
    let mut data = String::new();
    for line in event.lines() {
        let Some(value) = line.strip_prefix("data:").map(str::trim_start) else {
            continue;
        };
        if !data.is_empty() {
            data.push('\n');
        }
        data.push_str(value);
    }
    if data.trim().is_empty() {
        return Ok(GeminiStreamEvent::Ignored);
    }
    if data.trim() == "[DONE]" {
        return Ok(GeminiStreamEvent::Done);
    }
    let payload: Value = serde_json::from_str(&data)
        .map_err(|error| ProviderError::invalid(format!("Invalid Gemini SSE payload: {error}")))?;
    match payload.get("event_type").and_then(Value::as_str) {
        Some("step.delta")
            if payload.pointer("/delta/type").and_then(Value::as_str) == Some("text") =>
        {
            Ok(payload
                .pointer("/delta/text")
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(|text| GeminiStreamEvent::Delta(text.to_string()))
                .unwrap_or(GeminiStreamEvent::Ignored))
        }
        Some("interaction.completed") => Ok(GeminiStreamEvent::Completed(
            payload.get("interaction").cloned().unwrap_or(payload),
        )),
        Some("error") => {
            let message = payload
                .pointer("/error/message")
                .or_else(|| payload.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("Gemini stream failed");
            Err(ProviderError::invalid(message))
        }
        Some("done") => Ok(GeminiStreamEvent::Done),
        _ => Ok(GeminiStreamEvent::Ignored),
    }
}

fn file_content_uri(name_or_uri: &str) -> String {
    let trimmed = name_or_uri.trim();
    if trimmed.starts_with("https://")
        || trimmed.starts_with("http://")
        || trimmed.starts_with("gs://")
    {
        return trimmed.to_string();
    }
    let id = trimmed.strip_prefix("files/").unwrap_or(trimmed);
    format!("https://generativelanguage.googleapis.com/files/{id}")
}

fn content_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str().and_then(non_empty) {
        return Some(text);
    }
    let mut chunks = Vec::new();
    for part in value.as_array().into_iter().flatten() {
        if let Some(text) = part.get("text").and_then(Value::as_str).and_then(non_empty) {
            chunks.push(text);
        }
    }
    if chunks.is_empty() {
        None
    } else {
        Some(chunks.join(""))
    }
}

fn interaction_text(payload: &Value) -> Option<String> {
    if let Some(steps) = payload.get("steps").and_then(Value::as_array) {
        let mut chunks = Vec::new();
        for step in steps {
            if step.get("type").and_then(Value::as_str) != Some("model_output") {
                continue;
            }
            if let Some(text) = step
                .get("content")
                .and_then(content_text)
                .or_else(|| step.get("text").and_then(Value::as_str).and_then(non_empty))
            {
                chunks.push(text);
            }
        }
        if !chunks.is_empty() {
            return Some(chunks.join(""));
        }
    }
    if let Some(text) = payload
        .pointer("/output/0/content/0/text")
        .and_then(Value::as_str)
    {
        return non_empty(text);
    }
    if let Some(text) = payload
        .pointer("/candidates/0/content/parts/0/text")
        .and_then(Value::as_str)
    {
        return non_empty(text);
    }
    for collection in ["outputs", "output"] {
        for item in payload
            .get(collection)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(text) = item.get("text").and_then(Value::as_str).and_then(non_empty) {
                return Some(text);
            }
            if let Some(content) = item.get("content") {
                if let Some(text) = content.as_str().and_then(non_empty) {
                    return Some(text);
                }
                for part in content.as_array().into_iter().flatten() {
                    if let Some(text) = part.get("text").and_then(Value::as_str).and_then(non_empty)
                    {
                        return Some(text);
                    }
                }
            }
        }
    }
    None
}

pub fn normalize_staged_ocr(staging_path: &PathBuf, model: &str) -> ProviderResult<OcrOutcome> {
    let file = fs::File::open(staging_path).map_err(|error| {
        ProviderError::new(
            ProviderErrorKind::Transport,
            format!("Unable to read staged OCR response: {error}"),
        )
    })?;
    let raw_response: Value =
        serde_json::from_reader(BufReader::with_capacity(256 * 1024, file))
            .map_err(|error| ProviderError::invalid(format!("Invalid staged OCR JSON: {error}")))?;
    let pages = normalize_mistral_pages(&raw_response)?;
    Ok(OcrOutcome {
        pages,
        receipt: parse_mistral_usage(&raw_response, model, None),
        cleanup_warning: None,
        cleanup_resource: None,
    })
}

const MAX_STAGED_OCR_BYTES: u64 = 256 * 1024 * 1024;

fn read_staged_json(staging_path: &Path) -> ProviderResult<Value> {
    let file = fs::File::open(staging_path).map_err(|error| {
        ProviderError::new(
            ProviderErrorKind::Transport,
            format!("Unable to read staged OCR response: {error}"),
        )
    })?;
    serde_json::from_reader(BufReader::with_capacity(256 * 1024, file))
        .map_err(|error| ProviderError::invalid(format!("Invalid staged OCR JSON: {error}")))
}

fn replace_staged_file(temporary: &Path, target: &Path) -> std::io::Result<()> {
    let backup = target.with_extension(format!("{}.bak", uuid::Uuid::new_v4().simple()));
    let had_existing = fs::symlink_metadata(target).is_ok();
    if had_existing {
        fs::rename(target, &backup)?;
    }
    match fs::rename(temporary, target) {
        Ok(()) => {
            if had_existing {
                let _ = fs::remove_file(backup);
            }
            Ok(())
        }
        Err(error) => {
            if had_existing {
                let _ = fs::rename(&backup, target);
            }
            Err(error)
        }
    }
}

async fn stage_response_body(
    response: reqwest::Response,
    staging_path: &Path,
    cancellation: &CancellationFlag,
) -> ProviderResult<()> {
    let parent = staging_path.parent().ok_or_else(|| {
        ProviderError::invalid("OCR staging path does not have a parent directory")
    })?;
    tokio::fs::create_dir_all(parent).await.map_err(|error| {
        ProviderError::new(
            ProviderErrorKind::Transport,
            format!("Unable to create OCR staging directory: {error}"),
        )
    })?;
    let temporary = staging_path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4().simple()));
    let result = async {
        let file = tokio::fs::File::create(&temporary).await.map_err(|error| {
            ProviderError::new(
                ProviderErrorKind::Transport,
                format!("Unable to write OCR staging file: {error}"),
            )
        })?;
        let mut writer = tokio::io::BufWriter::with_capacity(256 * 1024, file);
        let mut stream = response.bytes_stream();
        let mut total = 0_u64;
        while let Some(chunk) = stream.next().await {
            cancellation.check()?;
            let chunk = chunk.map_err(transport_error)?;
            total = total.saturating_add(chunk.len() as u64);
            if total > MAX_STAGED_OCR_BYTES {
                return Err(ProviderError::invalid(
                    "Mistral OCR response exceeds the staging size limit",
                ));
            }
            writer.write_all(&chunk).await.map_err(|error| {
                ProviderError::new(
                    ProviderErrorKind::Transport,
                    format!("Unable to write OCR staging file: {error}"),
                )
            })?;
        }
        writer.flush().await.map_err(|error| {
            ProviderError::new(
                ProviderErrorKind::Transport,
                format!("Unable to flush OCR staging file: {error}"),
            )
        })?;
        let file = writer.into_inner();
        file.sync_all().await.map_err(|error| {
            ProviderError::new(
                ProviderErrorKind::Transport,
                format!("Unable to sync OCR staging file: {error}"),
            )
        })?;
        drop(file);
        replace_staged_file(&temporary, staging_path).map_err(|error| {
            ProviderError::new(
                ProviderErrorKind::Transport,
                format!("Unable to publish OCR staging file: {error}"),
            )
        })
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_file(&temporary).await;
    }
    result
}

fn non_empty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn parse_gemini_usage(
    payload: &Value,
    model: &str,
    context_epoch: &str,
    latency: Duration,
    file_reuse: bool,
    session_resume: bool,
    root_branch: bool,
) -> UsageEnvelope {
    let metadata = payload.get("usageMetadata").unwrap_or(&Value::Null);
    let usage = payload.get("usage").unwrap_or(&Value::Null);
    UsageEnvelope {
        provider: "gemini".to_string(),
        model: model.to_string(),
        context_epoch: Some(context_epoch.to_string()),
        input_tokens: first_i64(&[
            usage.get("total_input_tokens"),
            usage.get("input_tokens"),
            metadata.get("promptTokenCount"),
            metadata.get("inputTokenCount"),
        ]),
        cached_input_tokens: first_i64(&[
            usage.get("total_cached_tokens"),
            usage.pointer("/input_tokens_details/cached_tokens"),
            usage.get("cached_input_tokens"),
            metadata.get("cachedContentTokenCount"),
        ]),
        uncached_input_tokens: None,
        output_tokens: first_i64(&[
            usage.get("total_output_tokens"),
            usage.get("output_tokens"),
            metadata.get("candidatesTokenCount"),
            metadata.get("outputTokenCount"),
        ]),
        reasoning_tokens: first_i64(&[
            usage.get("total_thought_tokens"),
            usage.pointer("/output_tokens_details/reasoning_tokens"),
            usage.get("reasoning_tokens"),
            metadata.get("thoughtsTokenCount"),
        ]),
        latency_ms: Some(latency.as_millis().min(i64::MAX as u128) as i64),
        estimated_cost: None,
        file_reuse: Some(file_reuse),
        session_resume: Some(session_resume),
        paper_root_branch: Some(root_branch),
    }
    .with_uncached()
}

fn parse_mistral_usage(payload: &Value, model: &str, latency: Option<Duration>) -> UsageEnvelope {
    let usage = payload
        .get("usage_info")
        .or_else(|| payload.get("usage"))
        .unwrap_or(&Value::Null);
    UsageEnvelope {
        provider: "mistral".to_string(),
        model: model.to_string(),
        context_epoch: None,
        input_tokens: first_i64(&[
            usage.get("input_tokens"),
            usage.get("prompt_tokens"),
            usage.get("pages_processed"),
        ]),
        cached_input_tokens: first_i64(&[usage.get("cached_input_tokens")]),
        uncached_input_tokens: None,
        output_tokens: first_i64(&[usage.get("output_tokens"), usage.get("completion_tokens")]),
        reasoning_tokens: first_i64(&[usage.get("reasoning_tokens")]),
        latency_ms: latency.map(|value| value.as_millis().min(i64::MAX as u128) as i64),
        estimated_cost: None,
        file_reuse: Some(false),
        session_resume: Some(false),
        paper_root_branch: Some(false),
    }
    .with_uncached()
}

fn first_i64(values: &[Option<&Value>]) -> Option<i64> {
    values.iter().flatten().find_map(|value| value.as_i64())
}

fn normalize_mistral_pages(payload: &Value) -> ProviderResult<Vec<OcrPageInput>> {
    let values = payload
        .get("pages")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::invalid("Mistral OCR response contains no pages"))?;
    let mut pages = Vec::with_capacity(values.len());
    for (position, page) in values.iter().enumerate() {
        let page_number = page
            .get("page_number")
            .and_then(Value::as_i64)
            .or_else(|| {
                page.get("index")
                    .and_then(Value::as_i64)
                    .map(|index| index + 1)
            })
            .unwrap_or(position as i64 + 1);
        let width = page
            .pointer("/dimensions/width")
            .or_else(|| page.get("width"))
            .and_then(Value::as_i64);
        let height = page
            .pointer("/dimensions/height")
            .or_else(|| page.get("height"))
            .and_then(Value::as_i64);
        let markdown = page
            .get("markdown")
            .and_then(Value::as_str)
            .map(str::to_string);
        let raw_blocks = page
            .get("blocks")
            .and_then(Value::as_array)
            .ok_or_else(|| {
                ProviderError::invalid(format!(
                    "Mistral OCR page {page_number} has no locatable Blocks"
                ))
            })?;
        let mut blocks = Vec::with_capacity(raw_blocks.len());
        for (block_index, block) in raw_blocks.iter().enumerate() {
            let block_type = block
                .get("type")
                .or_else(|| block.get("block_type"))
                .or_else(|| block.get("kind"))
                .and_then(Value::as_str)
                .unwrap_or("text")
                .trim()
                .to_lowercase();
            let text_content = ["text", "content", "markdown", "html"]
                .iter()
                .find_map(|key| block.get(*key).and_then(Value::as_str).and_then(non_empty))
                .ok_or_else(|| {
                    ProviderError::invalid(format!(
                        "Mistral OCR page {page_number} contains an empty Block"
                    ))
                })?;
            let bbox_value = block
                .get("bbox")
                .or_else(|| block.get("bounding_box"))
                .or_else(|| block.get("box"))
                .unwrap_or(block);
            blocks.push(OcrBlockInput {
                block_index: block_index as i64,
                block_type,
                text_content,
                bbox: normalize_bbox(bbox_value, width, height)?,
            });
        }
        pages.push(OcrPageInput {
            page_number,
            width,
            height,
            markdown,
            blocks,
        });
    }
    pages.sort_by_key(|page| page.page_number);
    if pages.is_empty() || pages.iter().all(|page| page.blocks.is_empty()) {
        return Err(ProviderError::invalid(
            "Mistral OCR returned Markdown without locatable text Blocks",
        ));
    }
    if pages
        .windows(2)
        .any(|pair| pair[0].page_number == pair[1].page_number)
    {
        return Err(ProviderError::invalid(
            "Mistral OCR returned duplicate page numbers",
        ));
    }
    Ok(pages)
}

fn normalize_bbox(
    value: &Value,
    width: Option<i64>,
    height: Option<i64>,
) -> ProviderResult<[i64; 4]> {
    let coordinates = if let Some(items) = value.as_array() {
        if items.len() != 4 {
            return Err(ProviderError::invalid(
                "OCR bbox must contain four coordinates",
            ));
        }
        [
            number(&items[0])?,
            number(&items[1])?,
            number(&items[2])?,
            number(&items[3])?,
        ]
    } else {
        [
            object_number(value, &["x0", "left", "top_left_x"])?,
            object_number(value, &["y0", "top", "top_left_y"])?,
            object_number(value, &["x1", "right", "bottom_right_x"])?,
            object_number(value, &["y1", "bottom", "bottom_right_y"])?,
        ]
    };
    let normalized = if coordinates
        .iter()
        .all(|coordinate| (0.0..=1.0).contains(coordinate))
    {
        coordinates.map(|coordinate| coordinate * 1000.0)
    } else if let (Some(width), Some(height)) = (width, height) {
        if width > 0
            && height > 0
            && coordinates[0] <= width as f64
            && coordinates[2] <= width as f64
            && coordinates[1] <= height as f64
            && coordinates[3] <= height as f64
        {
            [
                coordinates[0] / width as f64 * 1000.0,
                coordinates[1] / height as f64 * 1000.0,
                coordinates[2] / width as f64 * 1000.0,
                coordinates[3] / height as f64 * 1000.0,
            ]
        } else {
            coordinates
        }
    } else {
        coordinates
    };
    let result = normalized.map(|coordinate| coordinate.round().clamp(0.0, 1000.0) as i64);
    if result[0] > result[2] || result[1] > result[3] {
        return Err(ProviderError::invalid("OCR bbox has inverted coordinates"));
    }
    Ok(result)
}

fn number(value: &Value) -> ProviderResult<f64> {
    value
        .as_f64()
        .ok_or_else(|| ProviderError::invalid("OCR bbox coordinate is not numeric"))
}

fn object_number(value: &Value, keys: &[&str]) -> ProviderResult<f64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_f64))
        .ok_or_else(|| ProviderError::invalid("OCR bbox object is missing a coordinate"))
}

fn transport_error(error: reqwest::Error) -> ProviderError {
    let kind = if error.is_timeout() {
        ProviderErrorKind::Timeout
    } else {
        ProviderErrorKind::Transport
    };
    ProviderError::new(
        kind,
        format!("Provider transport failed: {}", error.without_url()),
    )
}

async fn checked_json(provider: &str, response: reqwest::Response) -> ProviderResult<Value> {
    let status = response.status();
    let retry_after_seconds = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let payload = response.json::<Value>().await.map_err(transport_error)?;
    if status.is_success() {
        Ok(payload)
    } else {
        Err(provider_http_error(
            provider,
            status.as_u16(),
            &payload,
            retry_after_seconds,
        ))
    }
}

async fn checked_empty(provider: &str, response: reqwest::Response) -> ProviderResult<()> {
    let status = response.status();
    if status.is_success() || status.as_u16() == 404 {
        return Ok(());
    }
    let retry_after_seconds = response
        .headers()
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let payload = response.json::<Value>().await.unwrap_or(Value::Null);
    Err(provider_http_error(
        provider,
        status.as_u16(),
        &payload,
        retry_after_seconds,
    ))
}

fn provider_http_error(
    provider: &str,
    status: u16,
    payload: &Value,
    retry_after_seconds: Option<u64>,
) -> ProviderError {
    let detail = payload
        .pointer("/error/message")
        .or_else(|| payload.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("No error description");
    let lowered = detail.to_lowercase();
    let kind = match status {
        404 => ProviderErrorKind::StaleRemoteResource,
        403 if lowered.contains("permission")
            || lowered.contains("access the content")
            || lowered.contains("file") =>
        {
            ProviderErrorKind::StaleRemoteResource
        }
        401 | 403 => ProviderErrorKind::Unauthorized,
        429 => ProviderErrorKind::RateLimited,
        400 if lowered.contains("previous_interaction")
            || lowered.contains("file")
            || lowered.contains("not found") =>
        {
            ProviderErrorKind::StaleRemoteResource
        }
        _ => ProviderErrorKind::Transport,
    };
    ProviderError {
        kind,
        message: format!("{provider} request failed ({status}): {detail}"),
        retry_after_seconds,
        orphaned_resource: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn pdf_fixture() -> (TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("paper.pdf");
        fs::write(&path, b"%PDF-1.4\n%%EOF").expect("pdf");
        (directory, path)
    }

    #[tokio::test]
    async fn gemini_interaction_parses_cache_and_reasoning_usage() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/upload/v1beta/files"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "file": {"name": "files/file-1"}
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1beta/interactions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "interaction-1",
                "outputs": [{
                    "content": [{"type": "output_text", "text": "Grounded answer"}]
                }],
                "usage": {
                    "input_tokens": 100,
                    "input_tokens_details": {"cached_tokens": 40},
                    "output_tokens": 20,
                    "output_tokens_details": {"reasoning_tokens": 3}
                }
            })))
            .mount(&server)
            .await;
        let adapter = GeminiInteractionsAdapter::with_bases(
            "test-key",
            format!("{}/v1beta", server.uri()),
            format!("{}/upload/v1beta", server.uri()),
        )
        .expect("adapter");
        let (_directory, pdf_path) = pdf_fixture();
        let outcome = adapter
            .interact(PaperInteractionRequest {
                model: "gemini-2.5-flash".to_string(),
                context_epoch: "revision:model".to_string(),
                pdf_path,
                display_name: "Paper".to_string(),
                remote_file_id: None,
                previous_interaction_id: None,
                system_instruction: "Always cite exact pages.".to_string(),
                user_input: "Summarize the paper.".to_string(),
                response_schema: None,
                inline_images: Vec::new(),
                kind: PaperInteractionKind::default(),
            })
            .await
            .expect("interaction");

        assert_eq!(outcome.provider_file_id, "files/file-1");
        assert_eq!(outcome.provider_node_id, "interaction-1");
        assert_eq!(outcome.receipt.cached_input_tokens, Some(40));
        assert_eq!(outcome.receipt.uncached_input_tokens, Some(60));
        assert_eq!(outcome.receipt.reasoning_tokens, Some(3));
        assert_eq!(outcome.receipt.file_reuse, Some(false));
        assert_eq!(outcome.receipt.paper_root_branch, Some(true));
        assert_eq!(outcome.receipt.estimated_cost, None);
    }

    #[tokio::test]
    async fn gemini_stream_emits_text_deltas_and_final_usage() {
        let server = MockServer::start().await;
        let stream = concat!(
            "event: interaction.created\n",
            "data: {\"event_type\":\"interaction.created\",\"interaction\":{\"id\":\"interaction-1\"}}\n\n",
            "data: {\"event_type\":\"future.event\",\"detail\":\"ignored\"}\n\n",
            "event: step.delta\n",
            "data: {\"event_type\":\"step.delta\",\"delta\":{\"type\":\"text\",\"text\":\"Grounded \"}}\n\n",
            "data: {\"event_type\":\"step.delta\",\"delta\":{\"type\":\"text\",\"text\":\"answer\"}}\n\n",
            "event: interaction.completed\n",
            "data: {\"event_type\":\"interaction.completed\",\"interaction\":{\"id\":\"interaction-1\",\"usage\":{\"total_input_tokens\":100,\"total_cached_tokens\":40,\"total_output_tokens\":20,\"total_thought_tokens\":3}}}\n\n",
            "data: [DONE]\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/v1beta/interactions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_raw(stream, "text/event-stream"),
            )
            .mount(&server)
            .await;
        let adapter = GeminiInteractionsAdapter::with_bases(
            "test-key",
            format!("{}/v1beta", server.uri()),
            format!("{}/upload/v1beta", server.uri()),
        )
        .expect("adapter");
        let (_directory, pdf_path) = pdf_fixture();
        let (deltas, mut receiver) = mpsc::unbounded_channel();
        let outcome = adapter
            .interact_stream(PaperStreamRequest {
                interaction: PaperInteractionRequest {
                    model: "gemini-2.5-flash".to_string(),
                    context_epoch: "revision:model".to_string(),
                    pdf_path,
                    display_name: "Paper".to_string(),
                    remote_file_id: Some("files/file-1".to_string()),
                    previous_interaction_id: Some("interaction-0".to_string()),
                    system_instruction: "Always cite exact pages.".to_string(),
                    user_input: "Continue.".to_string(),
                    response_schema: None,
                    inline_images: Vec::new(),
                    kind: PaperInteractionKind::default(),
                },
                cancellation: CancellationFlag::default(),
                deltas,
            })
            .await
            .expect("stream interaction");

        let mut observed = String::new();
        while let Ok(delta) = receiver.try_recv() {
            observed.push_str(&delta);
        }
        assert_eq!(observed, "Grounded answer");
        assert_eq!(outcome.text, "Grounded answer");
        assert_eq!(outcome.provider_node_id, "interaction-1");
        assert_eq!(outcome.receipt.cached_input_tokens, Some(40));
        assert_eq!(outcome.receipt.uncached_input_tokens, Some(60));
        assert_eq!(outcome.receipt.reasoning_tokens, Some(3));
        assert_eq!(outcome.receipt.file_reuse, Some(true));
        assert_eq!(outcome.receipt.session_resume, Some(true));
        assert_eq!(outcome.receipt.paper_root_branch, Some(false));

        let requests = server.received_requests().await.expect("requests");
        let body: Value = serde_json::from_slice(&requests[0].body).expect("JSON body");
        assert_eq!(body.get("stream"), Some(&Value::Bool(true)));
    }

    #[test]
    fn sse_parser_waits_for_complete_utf8_event_and_ignores_unknown_events() {
        let mut buffer = SseBuffer::default();
        let _ = buffer.push(
            "data: {\"event_type\":\"step.delta\",\"delta\":{\"type\":\"text\",\"text\":\"论文"
                .as_bytes(),
        );
        assert!(buffer.next_event().is_none());
        buffer.extend_from_slice("\"}}\r\n\r\n".as_bytes());
        let event = buffer.next_event().expect("complete event");
        assert_eq!(
            parse_gemini_stream_event(event).expect("event"),
            GeminiStreamEvent::Delta("论文".to_string())
        );
        assert_eq!(
            parse_gemini_stream_event(
                b"data: {\"event_type\":\"step.delta\",\"delta\":{\"type\":\"image\",\"text\":\"ignored\"}}"
            )
            .expect("unknown delta"),
            GeminiStreamEvent::Ignored
        );
    }

    #[test]
    fn sse_buffer_handles_large_event_batches_without_losing_boundaries() {
        let mut buffer = SseBuffer::default();
        const EVENT_COUNT: usize = 2_000;
        for index in 0..EVENT_COUNT {
            let event = format!(
                "data: {{\"event_type\":\"step.delta\",\"delta\":{{\"type\":\"text\",\"text\":\"{index}\"}}}}\n\n"
            );
            let _ = buffer.push(event.as_bytes());
        }

        let mut seen = 0;
        while let Some(event) = buffer.next_event() {
            assert!(matches!(
                parse_gemini_stream_event(event).expect("valid event"),
                GeminiStreamEvent::Delta(_)
            ));
            seen += 1;
        }
        assert_eq!(seen, EVENT_COUNT);
        assert!(!buffer.has_pending());

        // The amortized compaction keeps the consumed prefix bounded even
        // after a large batch; a small remainder may stay until the next
        // threshold crossing.
        assert!(buffer.bytes.len() < 64 * 1024);
        assert!(buffer.cursor <= buffer.bytes.len());
    }

    #[tokio::test]
    async fn gemini_stream_honors_cancellation_before_provider_io() {
        let server = MockServer::start().await;
        let adapter = GeminiInteractionsAdapter::with_bases(
            "test-key",
            format!("{}/v1beta", server.uri()),
            format!("{}/upload/v1beta", server.uri()),
        )
        .expect("adapter");
        let (_directory, pdf_path) = pdf_fixture();
        let cancellation = CancellationFlag::default();
        cancellation.cancel();
        let (deltas, _receiver) = mpsc::unbounded_channel();
        let error = adapter
            .interact_stream(PaperStreamRequest {
                interaction: PaperInteractionRequest {
                    model: "gemini-2.5-flash".to_string(),
                    context_epoch: "revision:model".to_string(),
                    pdf_path,
                    display_name: "Paper".to_string(),
                    remote_file_id: Some("files/file-1".to_string()),
                    previous_interaction_id: None,
                    system_instruction: "Always cite exact pages.".to_string(),
                    user_input: "Summarize.".to_string(),
                    response_schema: None,
                    inline_images: Vec::new(),
                    kind: PaperInteractionKind::default(),
                },
                cancellation,
                deltas,
            })
            .await
            .expect_err("cancelled");

        assert_eq!(error.kind, ProviderErrorKind::Cancelled);
        assert!(server
            .received_requests()
            .await
            .expect("requests")
            .is_empty());
    }
    #[tokio::test]
    async fn gemini_text_interaction_never_uploads_or_attaches_a_pdf() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/interactions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "translation-1",
                "outputs": [{
                    "content": [{"type": "output_text", "text": r#"{"translation":"译文"}"#}]
                }],
                "usage": {"input_tokens": 12, "output_tokens": 4}
            })))
            .mount(&server)
            .await;
        let adapter = GeminiInteractionsAdapter::with_bases(
            "test-key",
            format!("{}/v1beta", server.uri()),
            format!("{}/upload/v1beta", server.uri()),
        )
        .expect("adapter");

        let outcome = adapter
            .interact_text(TextInteractionRequest {
                model: "gemini-2.5-flash".to_string(),
                context_epoch: "translation:revision:model".to_string(),
                system_instruction: "Translate exactly.".to_string(),
                user_input: "source text".to_string(),
                response_schema: None,
                kind: PaperInteractionKind::Artifact,
            })
            .await
            .expect("text interaction");

        assert_eq!(outcome.provider_node_id, "translation-1");
        assert_eq!(outcome.receipt.file_reuse, Some(false));
        assert_eq!(outcome.receipt.session_resume, Some(false));
        let requests = server.received_requests().await.expect("requests");
        assert_eq!(requests.len(), 1);
        let body: Value = serde_json::from_slice(&requests[0].body).expect("JSON body");
        assert_eq!(body["input"][0]["type"], "text");
        assert!(body.pointer("/input").and_then(Value::as_array).is_some());
        assert!(body.to_string().contains("source text"));
        assert!(!body.to_string().contains("document"));
        assert!(!body.to_string().contains("application/pdf"));
    }

    #[tokio::test]
    async fn gemini_classifies_a_missing_parent_as_stale() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1beta/interactions"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "error": {"message": "previous_interaction_id not found"}
            })))
            .mount(&server)
            .await;
        let adapter = GeminiInteractionsAdapter::with_bases(
            "test-key",
            format!("{}/v1beta", server.uri()),
            format!("{}/upload/v1beta", server.uri()),
        )
        .expect("adapter");
        let (_directory, pdf_path) = pdf_fixture();
        let error = adapter
            .interact(PaperInteractionRequest {
                model: "gemini-2.5-flash".to_string(),
                context_epoch: "revision:model".to_string(),
                pdf_path,
                display_name: "Paper".to_string(),
                remote_file_id: Some("files/file-1".to_string()),
                previous_interaction_id: Some("missing".to_string()),
                system_instruction: "Repeat canonical instructions.".to_string(),
                user_input: "Continue.".to_string(),
                response_schema: None,
                inline_images: Vec::new(),
                kind: PaperInteractionKind::default(),
            })
            .await
            .expect_err("stale parent");
        assert_eq!(error.kind, ProviderErrorKind::StaleRemoteResource);
    }

    #[tokio::test]
    async fn mistral_cleanup_owner_has_no_provider_uuid_and_mismatch_never_calls_endpoint() {
        let server = MockServer::start().await;
        let adapter = MistralOcrAdapter::with_base(
            "mistral-owner-key",
            format!("{}/v1/private-endpoint", server.uri()),
            "mistral-ocr-latest",
        )
        .expect("adapter");
        let endpoint = RemoteEndpointDeletePort::mistral_ocr(adapter);

        assert_eq!(endpoint.owner().owner_type(), "mistral_ocr");
        assert_eq!(endpoint.owner().provider_instance_id(), None);
        let error = endpoint
            .delete_remote(&RemoteResource {
                provider: "gemini".to_string(),
                kind: "file".to_string(),
                id: "same-remote-id".to_string(),
            })
            .await
            .expect_err("owner/resource mismatch must fail closed");

        assert_eq!(error.kind, ProviderErrorKind::InvalidResponse);
        assert!(server
            .received_requests()
            .await
            .expect("requests")
            .is_empty());
    }

    #[test]
    fn endpoint_adapter_debug_redacts_key_and_complete_url() {
        let adapter = MistralOcrAdapter::with_base(
            "mistral-debug-secret",
            "https://private.example.test/v1/account-path",
            "mistral-ocr-latest",
        )
        .expect("adapter");
        let debug = format!("{adapter:?}");

        assert!(!debug.contains("mistral-debug-secret"));
        assert!(!debug.contains("private.example.test"));
        assert!(!debug.contains("account-path"));
    }

    #[tokio::test]
    async fn mistral_normalizes_bboxes_and_deletes_the_remote_file() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/files"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "mistral-file-1"
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/files/mistral-file-1/url"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "url": "https://signed.invalid/paper.pdf"
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/ocr"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "pages": [{
                    "index": 0,
                    "dimensions": {"width": 1000, "height": 2000},
                    "markdown": "A paragraph",
                    "blocks": [{
                        "type": "text",
                        "text": "A paragraph",
                        "bbox": [100, 200, 900, 400]
                    }]
                }],
                "usage_info": {"pages_processed": 1}
            })))
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/v1/files/mistral-file-1"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let adapter = MistralOcrAdapter::with_base(
            "test-key",
            format!("{}/v1", server.uri()),
            "mistral-ocr-latest",
        )
        .expect("adapter");
        let (_directory, pdf_path) = pdf_fixture();
        let outcome = adapter
            .parse_pdf(OcrRequest {
                raw_staging_path: None,
                before_provider_commit: None,
                pdf_path,
                display_name: "paper.pdf".to_string(),
                cancellation: CancellationFlag::default(),
                progress: None,
            })
            .await
            .expect("ocr");

        assert_eq!(outcome.pages[0].blocks[0].bbox, [100, 100, 900, 200]);
        assert_eq!(outcome.pages[0].blocks[0].text_content, "A paragraph");
        assert_eq!(outcome.receipt.input_tokens, Some(1));
        assert_eq!(outcome.cleanup_warning, None);
    }

    #[test]
    fn mistral_rejects_markdown_without_locatable_blocks() {
        let error = normalize_mistral_pages(&json!({
            "pages": [{"index": 0, "markdown": "Only markdown"}]
        }))
        .expect_err("blocks required");
        assert_eq!(error.kind, ProviderErrorKind::InvalidResponse);
    }

    #[test]
    fn mistral_ocr4_flat_coordinates_become_canonical_bboxes() {
        let pages = normalize_mistral_pages(&json!({
            "pages": [{
                "index": 0,
                "dimensions": {"width": 788, "height": 1023},
                "markdown": "# Neuron",
                "blocks": [{
                    "type": "title",
                    "content": "# Neuron",
                    "top_left_x": 65,
                    "top_left_y": 78,
                    "bottom_right_x": 198,
                    "bottom_right_y": 113
                }]
            }]
        }))
        .expect("ocr4 pages");
        assert_eq!(pages[0].blocks[0].text_content, "# Neuron");
        assert_eq!(pages[0].blocks[0].block_type, "title");
        assert_eq!(pages[0].blocks[0].bbox, [82, 76, 251, 110]);
    }

    #[test]
    fn interaction_text_reads_current_model_output_steps() {
        let text = interaction_text(&json!({
            "id": "v1_example",
            "status": "completed",
            "steps": [
                {"type": "thought", "signature": "ignored"},
                {"type": "model_output", "content": [{"type": "text", "text": "PING"}]}
            ]
        }))
        .expect("text");
        assert_eq!(text, "PING");
    }

    #[test]
    fn gemini_request_uses_flat_text_and_document_uris() {
        assert_eq!(
            file_content_uri("files/file-1"),
            "https://generativelanguage.googleapis.com/files/file-1"
        );
        let request = PaperInteractionRequest {
            model: "gemini-3.6-flash".to_string(),
            context_epoch: "revision:model".to_string(),
            pdf_path: PathBuf::from("paper.pdf"),
            display_name: "Paper".to_string(),
            remote_file_id: Some("files/file-1".to_string()),
            previous_interaction_id: None,
            system_instruction: "Be brief.".to_string(),
            user_input: "What problem does this paper solve?".to_string(),
            response_schema: None,
            inline_images: Vec::new(),
            kind: PaperInteractionKind::default(),
        };
        let body = GeminiInteractionsAdapter::body(&request, "files/file-1");
        assert_eq!(body["input"][0]["type"], "document");
        assert_eq!(
            body["input"][0]["uri"],
            "https://generativelanguage.googleapis.com/files/file-1"
        );
        assert_eq!(body["input"][1]["type"], "text");
        assert!(body.get("input").and_then(Value::as_array).is_some());
        assert!(body.pointer("/input/0/role").is_none());
    }

    fn paper_request_with_schema(schema: Value) -> PaperInteractionRequest {
        PaperInteractionRequest {
            model: "gemini-2.5-flash".to_string(),
            context_epoch: "revision:model".to_string(),
            pdf_path: PathBuf::from("paper.pdf"),
            display_name: "Paper".to_string(),
            remote_file_id: Some("files/file-1".to_string()),
            previous_interaction_id: None,
            system_instruction: "Use the PDF.".to_string(),
            user_input: "Generate the Orientation Pack.".to_string(),
            response_schema: Some(schema),
            inline_images: Vec::new(),
            kind: PaperInteractionKind::default(),
        }
    }

    #[test]
    fn structured_output_uses_interactions_text_json_not_openai_json_schema() {
        let request = paper_request_with_schema(json!({
            "name": "orientation_pack",
            "strict": true,
            "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["brief"],
                "properties": {
                    "brief": {"type": "object"}
                }
            }
        }));
        let body = GeminiInteractionsAdapter::body(&request, "files/file-1");
        let format = &body["response_format"];
        assert_eq!(
            format["type"], "text",
            "Gemini Interactions rejects type=json_schema at response_format"
        );
        assert_eq!(format["mime_type"], "application/json");
        assert_eq!(format["schema"]["type"], "object");
        assert!(format["schema"].get("properties").is_some());
        assert!(format.get("json_schema").is_none());
        assert_ne!(format["type"], "json_schema");

        let text = GeminiInteractionsAdapter::text_body(&TextInteractionRequest {
            model: "gemini-2.5-flash".to_string(),
            context_epoch: "revision:model".to_string(),
            system_instruction: "Use the PDF.".to_string(),
            user_input: "Compress the path.".to_string(),
            response_schema: request.response_schema.clone(),
            kind: PaperInteractionKind::ContextCompaction,
        });
        assert_eq!(text["response_format"]["type"], "text");
        assert_eq!(text["response_format"]["schema"]["type"], "object");
    }

    #[test]
    fn cancellation_is_observable_without_provider_state() {
        let flag = CancellationFlag::default();
        flag.cancel();
        assert_eq!(
            flag.check().expect_err("cancelled").kind,
            ProviderErrorKind::Cancelled
        );
    }

    #[test]
    fn replace_staged_file_publishes_over_an_existing_target() {
        let directory = tempfile::tempdir().expect("tempdir");
        let target = directory.path().join("staged.json");
        let temporary = directory.path().join("staged.tmp");
        fs::write(&target, b"old").expect("existing");
        fs::write(&temporary, b"new").expect("temporary");
        replace_staged_file(&temporary, &target).expect("replace");
        assert_eq!(fs::read(&target).expect("published"), b"new");
        assert!(!temporary.exists());
    }

    #[test]
    fn replace_staged_file_restores_existing_target_when_publish_fails() {
        let directory = tempfile::tempdir().expect("tempdir");
        let target = directory.path().join("staged.json");
        fs::write(&target, b"old").expect("existing");
        let missing = directory.path().join("missing.tmp");
        assert!(replace_staged_file(&missing, &target).is_err());
        assert_eq!(fs::read(&target).expect("restored"), b"old");
    }
}
