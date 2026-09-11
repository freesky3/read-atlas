use crate::artifact_module::{
    ArtifactDraft, ArtifactModule, ArtifactProjection, EvidenceAnchor, LensQaProjection,
};
use crate::db;
use crate::lens_contract::LensProtocol;
use crate::provider_ports::{
    InlineImageInput, PaperInteractionKind, PaperInteractionRequest, PaperModelPort, ProviderError,
    ProviderErrorKind, ProviderResult, TextInteractionRequest, UsageEnvelope,
};
use crate::translation_contract::TranslationProtocol;
use base64::Engine;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex as StdMutex, OnceLock, Weak};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

/// A process-local lock key for paper context-root creation.
///
/// The database path is part of the key because multiple workspaces can be
/// open in one process.  The revision/provider/model/epoch tuple mirrors the unique
/// identity of a `context_roots` row and keeps concurrent requests for
/// unrelated roots independent.
#[derive(Debug, Clone, Eq, Hash, PartialEq)]
enum ContextRootLockScope {
    LegacyProvider(String),
    ProviderRoute(String),
}

#[derive(Debug, Clone, Eq, Hash, PartialEq)]
struct ContextRootLockKey {
    database_path: PathBuf,
    revision_id: String,
    scope: ContextRootLockScope,
    model: String,
    context_epoch: String,
}

static CONTEXT_ROOT_LOCKS: OnceLock<StdMutex<HashMap<ContextRootLockKey, Weak<AsyncMutex<()>>>>> =
    OnceLock::new();

pub(crate) fn context_root_lock(
    database_path: &Path,
    revision_id: &str,
    provider: &str,
    model: &str,
    context_epoch: &str,
) -> Arc<AsyncMutex<()>> {
    context_root_lock_for_scope(
        database_path,
        revision_id,
        ContextRootLockScope::LegacyProvider(provider.to_string()),
        model,
        context_epoch,
    )
}

/// Derives a versioned epoch without exposing the raw route identity through
/// projections or provider payloads.
fn pdf_source_epoch(anchor: &DocumentFacts, model: &str, instruction: Option<&str>) -> String {
    let instruction = instruction
        .map(str::to_owned)
        .unwrap_or_else(paper_root_system_instruction);
    let fingerprint = format!("{:x}", Sha256::digest(instruction.as_bytes()));
    format!(
        "pdf-source-v2:{}:{}:{}",
        anchor.revision_sha256, model, fingerprint
    )
}

pub(crate) fn route_scoped_context_epoch(semantic_epoch: &str, route_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"read-desktop/context-epoch/v1\0");
    hasher.update(semantic_epoch.as_bytes());
    hasher.update(b"\0");
    hasher.update(route_id.as_bytes());
    format!("route-v1:{:x}", hasher.finalize())
}

pub(crate) fn context_root_lock_for_route(
    database_path: &Path,
    revision_id: &str,
    route_id: &str,
    model: &str,
    context_epoch: &str,
) -> Arc<AsyncMutex<()>> {
    context_root_lock_for_scope(
        database_path,
        revision_id,
        ContextRootLockScope::ProviderRoute(route_id.to_string()),
        model,
        context_epoch,
    )
}

fn context_root_lock_for_scope(
    database_path: &Path,
    revision_id: &str,
    scope: ContextRootLockScope,
    model: &str,
    context_epoch: &str,
) -> Arc<AsyncMutex<()>> {
    // Canonicalizing an existing database makes equivalent relative/absolute
    // module paths share a lock while retaining a safe fallback for a path
    // whose parent has not been created yet.
    let database_path =
        fs::canonicalize(database_path).unwrap_or_else(|_| database_path.to_path_buf());
    let key = ContextRootLockKey {
        database_path,
        revision_id: revision_id.to_string(),
        scope,
        model: model.to_string(),
        context_epoch: context_epoch.to_string(),
    };
    let locks = CONTEXT_ROOT_LOCKS.get_or_init(|| StdMutex::new(HashMap::new()));
    let mut locks = locks
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    // Weak entries prevent a long-lived desktop process from retaining one
    // mutex per revision/model forever.
    locks.retain(|_, lock| lock.strong_count() != 0);
    if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
        return lock;
    }
    let lock = Arc::new(AsyncMutex::new(()));
    locks.insert(key, Arc::downgrade(&lock));
    lock
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingArtifactAction {
    Translate,
    Explain,
    Lens,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateReadingArtifactRequest {
    #[serde(default)]
    pub document_kind: Option<crate::library_paths::DocumentKind>,
    pub revision_id: String,
    pub ocr_revision_id: String,
    pub block_id: String,
    pub action: ReadingArtifactAction,
    #[serde(default = "default_output_language")]
    pub output_language: String,
    pub paper_model: String,
    pub translation_model: String,
    #[serde(default)]
    pub translation_protocol: TranslationProtocol,
    #[serde(default)]
    pub lens_protocol: LensProtocol,
    #[serde(default = "default_paper_provider")]
    pub provider: String,
    pub display_crop_data_url: Option<String>,
    pub model_crop_data_url: Option<String>,
    #[serde(default)]
    pub system_instruction: Option<String>,
    #[serde(default)]
    pub repair_system_instruction: Option<String>,
    #[serde(default)]
    pub paper_root_system_instruction: Option<String>,
    #[serde(default)]
    pub reader_context: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadingArtifactOutcome {
    pub artifact: ArtifactProjection,
    pub receipts: Vec<UsageEnvelope>,
    pub repaired: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskLensRequest {
    #[serde(default)]
    pub output_language: Option<String>,
    pub lens_artifact_id: String,
    pub parent_id: Option<String>,
    pub question: String,
    #[serde(default)]
    pub system_instruction: Option<String>,
    #[serde(default = "default_paper_provider")]
    pub provider: String,
    #[serde(default)]
    pub reader_context: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LensQaTurn {
    pub user: LensQaProjection,
    pub assistant: LensQaProjection,
    pub receipt: UsageEnvelope,
}

#[derive(Debug, Clone)]
pub struct ReadingArtifactModule {
    workspace_root: PathBuf,
    database_path: PathBuf,
    artifact_module: ArtifactModule,
}

/// Removes a temporary crop file unless it has been atomically published.
///
/// Crop generation can fail after the file has been created (for example on a
/// short write, flush failure, or a rename race). Keeping cleanup in a Drop
/// guard makes every early-return path safe and avoids relying on callers to
/// remember a best-effort unlink.
struct TemporaryCropFile {
    path: PathBuf,
    published: bool,
}

impl TemporaryCropFile {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            published: false,
        }
    }

    fn publish(&mut self) {
        self.published = true;
    }
}

impl Drop for TemporaryCropFile {
    fn drop(&mut self) {
        if !self.published {
            let _ = fs::remove_file(&self.path);
            if let Some(parent) = self.path.parent() {
                let _ = fs::remove_dir(parent);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DocumentFacts {
    pub(crate) paper_id: String,
    pub(crate) revision_id: String,
    pub(crate) revision_sha256: String,
    pub(crate) title: String,
    pub(crate) pdf_path: PathBuf,
}

#[derive(Debug, Clone)]
struct AnchorFacts {
    document: DocumentFacts,
    ocr_revision_id: String,
    block_id: String,
    block_type: String,
    text: String,
    content_digest: String,
    page_number: i64,
    block_index: i64,
    bbox: [i64; 4],
}

impl std::ops::Deref for AnchorFacts {
    type Target = DocumentFacts;
    fn deref(&self) -> &DocumentFacts {
        &self.document
    }
}
impl std::ops::DerefMut for AnchorFacts {
    fn deref_mut(&mut self) -> &mut DocumentFacts {
        &mut self.document
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RootContext {
    pub(crate) context_epoch: String,
    pub(crate) remote_file_id: String,
    pub(crate) remote_node_id: String,
    pub(crate) local_node_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct PaperRootCall {
    pub(crate) provider: String,
    pub(crate) model: String,
    pub(crate) system_instruction: String,
    pub(crate) user_input: String,
    pub(crate) response_schema: Value,
    pub(crate) inline_images: Vec<InlineImageInput>,
    pub(crate) paper_root_instruction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ModelCall {
    pub(crate) text: String,
    pub(crate) remote_node_id: String,
    pub(crate) context_epoch: String,
    pub(crate) parent_local_node_id: Option<String>,
    pub(crate) receipts: Vec<UsageEnvelope>,
}

impl ReadingArtifactModule {
    pub fn open(workspace_root: impl AsRef<Path>) -> Result<Self, String> {
        let workspace_root = workspace_root.as_ref().to_path_buf();
        let database_path = workspace_root
            .join(".read-desktop")
            .join("workspace.sqlite3");
        let artifact_module = ArtifactModule::open(&database_path)?;
        Ok(Self {
            workspace_root,
            database_path,
            artifact_module,
        })
    }

    pub(crate) async fn generate_for_route(
        &self,
        port: &dyn PaperModelPort,
        route_id: &str,
        request: GenerateReadingArtifactRequest,
    ) -> ProviderResult<ReadingArtifactOutcome> {
        if route_id.trim().is_empty() {
            return Err(ProviderError::invalid("Provider route is required"));
        }
        self.require_provider_route(route_id)?;
        self.generate_scoped(port, route_id, request).await
    }

    async fn generate_scoped(
        &self,
        port: &dyn PaperModelPort,
        route_id: &str,
        request: GenerateReadingArtifactRequest,
    ) -> ProviderResult<ReadingArtifactOutcome> {
        let anchor = self.load_anchor(&request)?;
        match request.action {
            ReadingArtifactAction::Translate => {
                self.generate_translation(port, &anchor, route_id, &request)
                    .await
            }
            ReadingArtifactAction::Explain => {
                self.generate_explanation(port, &anchor, route_id, &request)
                    .await
            }
            ReadingArtifactAction::Lens => {
                self.generate_lens(port, &anchor, route_id, &request).await
            }
        }
    }

    /// Continue a Lens branch only through the exact durable paper route that
    /// created its root. The route is checked before any remote request, so a
    /// user switching between same-kind provider instances cannot resume a
    /// remote node from the previous instance.
    pub(crate) async fn ask_lens_for_route(
        &self,
        port: &dyn PaperModelPort,
        route_id: &str,
        request: AskLensRequest,
    ) -> ProviderResult<LensQaTurn> {
        if route_id.trim().is_empty() {
            return Err(ProviderError::invalid("Provider route is required"));
        }
        self.require_provider_route(route_id)?;
        self.ask_lens_scoped(port, route_id, request).await
    }

    async fn ask_lens_scoped(
        &self,
        port: &dyn PaperModelPort,
        route_id: &str,
        request: AskLensRequest,
    ) -> ProviderResult<LensQaTurn> {
        let question = request.question.trim();
        if question.is_empty() {
            return Err(ProviderError::invalid("Lens question cannot be empty"));
        }
        let artifact = self
            .artifact_module
            .get(&request.lens_artifact_id)
            .map_err(ProviderError::local_state)?;
        if !artifact.kind.starts_with("lens_") {
            return Err(ProviderError::invalid(
                "Lens follow-up can only attach to a Lens Artifact",
            ));
        }
        let connection = self.connect()?;
        let (model, context_epoch, remote_node_id, local_node_id): (
            String,
            String,
            String,
            String,
        ) = connection
            .query_row(
                "SELECT n.model, n.context_epoch, n.provider_node_id, n.id
                 FROM provider_nodes n
                 WHERE n.id = ?1
                   AND n.provider_route_id = ?2
                   AND n.provider_node_id IS NOT NULL",
                params![artifact.provider_node_id, route_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .map_err(|error| {
                ProviderError::local_state(format!(
                    "Lens provider node is unavailable; regenerate the Lens: {error}"
                ))
            })?;
        let (pdf_path, title): (String, String) = connection
            .query_row(
                "SELECT p.relative_path, COALESCE(m.title, p.file_name)
                 FROM document_revisions r
                 JOIN papers p ON p.id = r.paper_id
                 LEFT JOIN paper_metadata m ON m.revision_id = r.id
                 WHERE r.id = ?1",
                params![artifact.revision_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        let remote_file_id: String = connection
            .query_row(
                "SELECT provider_file_id FROM context_roots
                 WHERE revision_id = ?1 AND model = ?2
                   AND context_epoch = ?3 AND invalidated_at IS NULL
                   AND provider_file_id IS NOT NULL
                   AND provider_route_id = ?4",
                params![artifact.revision_id, model, context_epoch, route_id],
                |row| row.get(0),
            )
            .map_err(|error| {
                ProviderError::local_state(format!(
                    "Lens paper root is unavailable; regenerate the Lens: {error}"
                ))
            })?;
        let (previous_remote_node, previous_local_node) =
            if let Some(parent_id) = request.parent_id.as_deref() {
                connection
                    .query_row(
                        "SELECT n.provider_node_id, n.id
                         FROM lens_qa q
                         JOIN provider_nodes n ON n.id = q.provider_node_id
                         WHERE q.id = ?1 AND q.lens_artifact_id = ?2
                           AND q.role = 'assistant' AND n.provider_node_id IS NOT NULL
                           AND n.provider_route_id = ?3
                           AND n.model = ?4 AND n.context_epoch = ?5",
                        params![parent_id, artifact.id, route_id, model, context_epoch],
                        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
                    )
                    .map_err(|_| {
                        ProviderError::invalid(
                            "Lens follow-up parent must be an assistant answer from this Lens",
                        )
                    })?
            } else {
                (remote_node_id, local_node_id)
            };
        drop(connection);

        let user = self
            .artifact_module
            .append_lens_qa(
                &artifact.id,
                request.parent_id.as_deref(),
                "user",
                question,
                None,
                "complete",
            )
            .map_err(ProviderError::local_state)?;
        let evidence_id = format!("block:{}", artifact.object_key);
        let schema = lens_qa_schema(&artifact.object_key);
        let outcome = port
            .interact(PaperInteractionRequest {
                model: model.clone(),
                context_epoch: context_epoch.clone(),
                pdf_path: self.absolute_pdf(Path::new(&pdf_path)),
                display_name: title,
                remote_file_id: Some(remote_file_id),
                previous_interaction_id: Some(previous_remote_node),
                system_instruction: request.system_instruction.clone().unwrap_or_else(|| {
                    lens_qa_system_instruction(crate::revision_document_kind_from_root(
                        &self.workspace_root,
                        &artifact.revision_id,
                    ))
                }),
                user_input: crate::reader_context::prepend_reader_context(
                    &lens_qa_input(
                        question,
                        &artifact.content,
                        &evidence_id,
                        request.output_language.as_deref().unwrap_or("zh-CN"),
                    ),
                    request.reader_context.as_deref(),
                ),
                response_schema: Some(schema),
                inline_images: Vec::new(),
                kind: crate::provider_ports::PaperInteractionKind::default(),
            })
            .await?;
        let validated = parse_json_object(&outcome.text, "Lens follow-up").and_then(|content| {
            validate_lens_qa(&content, &artifact.object_key)?;
            Ok(content)
        });
        let content = match validated {
            Ok(content) => content,
            Err(error) => {
                // A completed provider call remains traceable even when its output is invalid.
                self.store_provider_node_for_route(
                    route_id,
                    &request.provider,
                    &model,
                    &context_epoch,
                    &outcome.provider_node_id,
                    Some(&previous_local_node),
                    "invalid",
                )?;
                self.record_usage_scoped(&user.id, route_id, &outcome.receipt)?;
                return Err(error);
            }
        };
        let answer = required_string(&content, "answerMarkdown", "Lens follow-up")?;
        let provider_node_id = self.store_provider_node_for_route(
            route_id,
            &request.provider,
            &model,
            &context_epoch,
            &outcome.provider_node_id,
            Some(&previous_local_node),
            "complete",
        )?;
        let assistant = self
            .artifact_module
            .append_lens_qa(
                &artifact.id,
                Some(&user.id),
                "assistant",
                answer,
                Some(&provider_node_id),
                "complete",
            )
            .map_err(ProviderError::local_state)?;
        self.record_usage_scoped(&assistant.id, route_id, &outcome.receipt)?;
        Ok(LensQaTurn {
            user,
            assistant,
            receipt: outcome.receipt,
        })
    }

    async fn generate_translation(
        &self,
        port: &dyn PaperModelPort,
        anchor: &AnchorFacts,
        route_id: &str,
        request: &GenerateReadingArtifactRequest,
    ) -> ProviderResult<ReadingArtifactOutcome> {
        if is_lens_block(&anchor.block_type) {
            return Err(ProviderError::invalid(
                "Formula, Figure, and Table Blocks use Lens instead of translation",
            ));
        }
        let semantic_epoch = format!(
            "translation:{}:{}",
            anchor.revision_sha256, request.translation_model
        );
        let context_epoch = route_scoped_context_epoch(&semantic_epoch, route_id);
        let local_context = self.translation_context(anchor)?;
        let outcome = port
            .interact_text(TextInteractionRequest {
                model: request.translation_model.clone(),
                context_epoch: context_epoch.clone(),
                system_instruction: request.system_instruction.clone().unwrap_or_else(|| {
                    request
                        .translation_protocol
                        .default_prompt(&request.output_language)
                }),
                user_input: json!({
                    "targetBlock": block_payload(anchor),
                    "localContext": local_context,
                    "targetLanguage": request.output_language
                })
                .to_string(),
                response_schema: Some(request.translation_protocol.schema()),
                kind: PaperInteractionKind::Artifact,
            })
            .await?;
        let mut content = parse_json_object(&outcome.text, "Translation")?;
        request
            .translation_protocol
            .validate(&content, &request.output_language)
            .map_err(ProviderError::invalid)?;
        content.as_object_mut().unwrap().insert(
            "translationProtocol".into(),
            json!(request.translation_protocol),
        );
        attach_local_contract(
            &mut content,
            anchor,
            &request.output_language,
            "translation",
        );
        let local_node_id = self.store_provider_node_for_route(
            route_id,
            &request.provider,
            &request.translation_model,
            &context_epoch,
            &outcome.provider_node_id,
            None,
            "complete",
        )?;
        let artifact = self.publish_artifact(
            anchor,
            "translation",
            content,
            Some(local_node_id),
            &request.translation_model,
            &context_epoch,
        )?;
        self.record_usage_scoped(&artifact.id, route_id, &outcome.receipt)?;
        Ok(ReadingArtifactOutcome {
            artifact,
            receipts: vec![outcome.receipt],
            repaired: false,
        })
    }

    async fn generate_explanation(
        &self,
        port: &dyn PaperModelPort,
        anchor: &AnchorFacts,
        route_id: &str,
        request: &GenerateReadingArtifactRequest,
    ) -> ProviderResult<ReadingArtifactOutcome> {
        if is_lens_block(&anchor.block_type) {
            return Err(ProviderError::invalid(
                "Formula, Figure, and Table Blocks use Lens instead of ordinary explanation",
            ));
        }
        let call = self
            .call_from_paper_root(
                port,
                anchor,
                route_id,
                PaperRootCall {
                    provider: request.provider.clone(),
                    model: request.paper_model.clone(),
                    system_instruction: request
                        .system_instruction
                        .clone()
                        .unwrap_or_else(|| {
                            explanation_system_instruction(crate::revision_document_kind_from_root(
                                &self.workspace_root,
                                &anchor.revision_id,
                            ))
                        }),
                    user_input: crate::reader_context::prepend_reader_context(
                        &json!({
                            "task": "解释本次提供的一个正文 OCR 块，按 outputLanguage 讲清其含义、必要概念与逻辑。",
                            "block": block_payload(anchor),
                            "allowedEvidenceIds": [format!("block:{}", anchor.block_id)],
                            "outputLanguage": request.output_language
                        })
                        .to_string(),
                        request.reader_context.as_deref(),
                    ),
                    response_schema: explanation_schema(&anchor.block_id),
                    inline_images: Vec::new(),
                    paper_root_instruction: request.paper_root_system_instruction.clone(),
                },
            )
            .await?;
        let mut content = parse_json_object(&call.text, "Explanation")?;
        validate_explanation(&content, &anchor.block_id)?;
        attach_local_contract(
            &mut content,
            anchor,
            &request.output_language,
            "explanation",
        );
        let local_node_id = self.store_provider_node_for_route(
            route_id,
            &request.provider,
            &request.paper_model,
            &call.context_epoch,
            &call.remote_node_id,
            call.parent_local_node_id.as_deref(),
            "complete",
        )?;
        let artifact = self.publish_artifact(
            anchor,
            "explanation",
            content,
            Some(local_node_id),
            &request.paper_model,
            &call.context_epoch,
        )?;
        for receipt in &call.receipts {
            self.record_usage_scoped(&artifact.id, route_id, receipt)?;
        }
        Ok(ReadingArtifactOutcome {
            artifact,
            receipts: call.receipts,
            repaired: false,
        })
    }
    async fn generate_lens(
        &self,
        port: &dyn PaperModelPort,
        anchor: &AnchorFacts,
        route_id: &str,
        request: &GenerateReadingArtifactRequest,
    ) -> ProviderResult<ReadingArtifactOutcome> {
        let lens_kind = lens_kind(&anchor.block_type).ok_or_else(|| {
            ProviderError::invalid("Lens is available only for Formula, Figure, and Table Blocks")
        })?;
        let document_kind = request
            .document_kind
            .unwrap_or(crate::library_paths::DocumentKind::Paper);
        let schema = match request.lens_protocol {
            LensProtocol::V1 => lens_schema(lens_kind, &anchor.block_id),
            LensProtocol::V2 => crate::lens_contract::schema(lens_kind, &anchor.block_id),
        };
        let display_crop = request.display_crop_data_url.as_deref().ok_or_else(|| {
            ProviderError::invalid(
                "Lens requires a rendered display crop from the selected PDF Block",
            )
        })?;
        decode_image_data_url(display_crop)?;
        let inline_images = lens_inline_images(request.model_crop_data_url.as_deref())?;
        let initial = self
            .call_from_paper_root(
                port,
                anchor,
                route_id,
                PaperRootCall {
                    provider: request.provider.clone(),
                    model: request.paper_model.clone(),
                    system_instruction: request.system_instruction.clone().unwrap_or_else(|| {
                        match request.lens_protocol {
                            LensProtocol::V1 if document_kind==crate::library_paths::DocumentKind::Paper => lens_system_instruction(lens_kind,&request.output_language),
                            _ => request.lens_protocol.document_prompt(document_kind,lens_kind,false,&request.output_language),
                        }
                    }),
                    user_input: crate::reader_context::prepend_reader_context(
                        &json!({
                            "task": "解释 anchor 指定的当前对象，按本次 schema 返回完整成果；用实际可见的材料帮助读者理解，不假定已收到其他阅读成果。",
                            "kind": lens_kind,
                            "anchor": block_payload(anchor),
                            "allowedEvidenceIds": [format!("block:{}", anchor.block_id)],
                            "outputLanguage": request.output_language
                        })
                        .to_string(),
                        request.reader_context.as_deref(),
                    ),
                    response_schema: schema.clone(),
                    inline_images,
                    paper_root_instruction: request.paper_root_system_instruction.clone(),
                },
            )
            .await?;
        let initial_local_node = self.store_provider_node_for_route(
            route_id,
            &request.provider,
            &request.paper_model,
            &initial.context_epoch,
            &initial.remote_node_id,
            initial.parent_local_node_id.as_deref(),
            "validating",
        )?;
        let attempt_id = format!("lens-attempt:{}", Uuid::new_v4());

        let validated = parse_json_object(&initial.text, "Lens").and_then(|mut value| {
            if request.lens_protocol == LensProtocol::V1 {
                normalize_lens_markdown_fields(&mut value);
            }
            validate_lens_for_protocol(value, request.lens_protocol, lens_kind, anchor)
        });
        let (mut content, local_node_id, receipts, repaired) = match validated {
            Ok(content) => {
                self.set_provider_node_state(&initial_local_node, "complete")?;
                (content, initial_local_node, initial.receipts, false)
            }
            Err(first_error) => {
                self.set_provider_node_state(&initial_local_node, "invalid")?;
                let root = self
                    .lookup_root_for_route_with_instruction(
                        anchor,
                        route_id,
                        &request.paper_model,
                        request.paper_root_system_instruction.as_deref(),
                    )?
                    .ok_or_else(|| {
                        ProviderError::local_state("Paper root disappeared during Lens repair")
                    })?;
                let repair = port
                    .interact(PaperInteractionRequest {
                        model: request.paper_model.clone(),
                        context_epoch: root.context_epoch.clone(),
                        pdf_path: anchor.pdf_path.clone(),
                        display_name: anchor.title.clone(),
                        remote_file_id: Some(root.remote_file_id),
                        previous_interaction_id: Some(initial.remote_node_id.clone()),
                        system_instruction: request
                            .repair_system_instruction
                            .clone()
                            .unwrap_or_else(|| {
                                match request.lens_protocol {
                                    LensProtocol::V1 if document_kind==crate::library_paths::DocumentKind::Paper => lens_repair_system_instruction(lens_kind,&request.output_language),
                                    _ => request.lens_protocol.document_prompt(document_kind,lens_kind,true,&request.output_language),
                                }
                            }),
                        user_input: json!({
                            "task": "修复同一对象的完整成果，字段以本次 response schema 为准。保留正确解释、条件和局限，不编造内容以通过校验。",
                            "outputLanguage": request.output_language,
                            "validationError": first_error.message,
                            "invalidOutput": initial.text,
                            "anchor": block_payload(anchor),
                            "allowedEvidenceIds": [format!("block:{}", anchor.block_id)]
                        })
                        .to_string(),
                        response_schema: Some(schema),
                        inline_images: Vec::new(),
                        kind: crate::provider_ports::PaperInteractionKind::default(),
                    })
                    .await?;
                let repair_local_node = self.store_provider_node_for_route(
                    route_id,
                    &request.provider,
                    &request.paper_model,
                    &root.context_epoch,
                    &repair.provider_node_id,
                    Some(&initial_local_node),
                    "validating",
                )?;

                let repaired_content =
                    parse_json_object(&repair.text, "Lens repair").and_then(|mut value| {
                        if request.lens_protocol == LensProtocol::V1 {
                            normalize_lens_markdown_fields(&mut value);
                        }
                        validate_lens_for_protocol(value, request.lens_protocol, lens_kind, anchor)
                    });
                match repaired_content {
                    Ok(content) => {
                        self.set_provider_node_state(&repair_local_node, "complete")?;
                        let mut all = initial.receipts;
                        all.push(repair.receipt);
                        (content, repair_local_node, all, true)
                    }
                    Err(second_error) => {
                        self.set_provider_node_state(&repair_local_node, "invalid")?;
                        for receipt in &initial.receipts {
                            self.record_usage_scoped(&attempt_id, route_id, receipt)?;
                        }
                        self.record_usage_scoped(&attempt_id, route_id, &repair.receipt)?;
                        return Err(ProviderError::invalid(format!(
                            "Lens output failed validation after one repair: {}",
                            second_error.message
                        )));
                    }
                }
            }
        };
        content["documentKind"] = json!(document_kind.as_str());
        let display_crop_path =
            self.persist_display_crop(request.display_crop_data_url.as_deref())?;
        attach_local_contract(&mut content, anchor, &request.output_language, lens_kind);
        if let Some(object) = content.as_object_mut() {
            object.insert(
                "displayCropRelativePath".to_string(),
                Value::String(display_crop_path),
            );
        }
        let artifact = self.publish_artifact(
            anchor,
            &format!("lens_{lens_kind}"),
            content,
            Some(local_node_id),
            &request.paper_model,
            &initial.context_epoch,
        )?;
        for receipt in &receipts {
            self.record_usage_scoped(&artifact.id, route_id, receipt)?;
        }
        Ok(ReadingArtifactOutcome {
            artifact,
            receipts,
            repaired,
        })
    }

    pub(crate) async fn call_from_paper_root(
        &self,
        port: &dyn PaperModelPort,
        anchor: &DocumentFacts,
        route_id: &str,
        call: PaperRootCall,
    ) -> ProviderResult<ModelCall> {
        let mut current_root = self
            .ensure_root_for_route(
                port,
                anchor,
                route_id,
                &call.provider,
                &call.model,
                call.paper_root_instruction.as_deref(),
            )
            .await?;
        let request = |root: &RootContext| PaperInteractionRequest {
            model: call.model.clone(),
            context_epoch: root.context_epoch.clone(),
            pdf_path: anchor.pdf_path.clone(),
            display_name: anchor.title.clone(),
            remote_file_id: Some(root.remote_file_id.clone()),
            previous_interaction_id: Some(root.remote_node_id.clone()),
            system_instruction: call.system_instruction.clone(),
            user_input: call.user_input.clone(),
            response_schema: Some(call.response_schema.clone()),
            inline_images: call.inline_images.clone(),
            kind: crate::provider_ports::PaperInteractionKind::Artifact,
        };
        let outcome = match port.interact(request(&current_root)).await {
            Ok(outcome) => outcome,
            Err(error) if error.kind == ProviderErrorKind::StaleRemoteResource => {
                self.invalidate_root_for_route_with_instruction(
                    anchor,
                    route_id,
                    &call.model,
                    call.paper_root_instruction.as_deref(),
                )?;
                current_root = self
                    .ensure_root_for_route(
                        port,
                        anchor,
                        route_id,
                        &call.provider,
                        &call.model,
                        call.paper_root_instruction.as_deref(),
                    )
                    .await?;
                port.interact(request(&current_root)).await?
            }
            Err(error) => return Err(error),
        };
        let receipts = vec![outcome.receipt];
        Ok(ModelCall {
            text: outcome.text,
            remote_node_id: outcome.provider_node_id,
            context_epoch: current_root.context_epoch,
            parent_local_node_id: current_root.local_node_id,
            receipts,
        })
    }

    async fn ensure_root_scoped(
        &self,
        port: &dyn PaperModelPort,
        anchor: &DocumentFacts,
        route_id: Option<&str>,
        provider: &str,
        model: &str,
        paper_root_instruction: Option<&str>,
    ) -> ProviderResult<RootContext> {
        match route_id {
            Some(route_id) => {
                self.ensure_root_for_route(
                    port,
                    anchor,
                    route_id,
                    provider,
                    model,
                    paper_root_instruction,
                )
                .await
            }
            None => {
                self.ensure_root(port, anchor, provider, model, paper_root_instruction)
                    .await
            }
        }
    }

    async fn ensure_root(
        &self,
        port: &dyn PaperModelPort,
        anchor: &DocumentFacts,
        provider: &str,
        model: &str,
        paper_root_instruction: Option<&str>,
    ) -> ProviderResult<RootContext> {
        if let Some(root) = lookup_root_on_connection(
            &self.connect()?,
            &anchor.revision_id,
            provider,
            model,
            &pdf_source_epoch(anchor, model, paper_root_instruction),
        )
        .map_err(|e| ProviderError::local_state(e.to_string()))?
        {
            return Ok(root);
        }
        let context_epoch = pdf_source_epoch(anchor, model, paper_root_instruction);
        // Keep the lock across the provider call.  Root creation uploads the
        // PDF and may be billable, so a check-then-create sequence must be
        // serialized for this exact workspace/revision/provider/model/epoch tuple.
        let root_lock = context_root_lock(
            &self.database_path,
            &anchor.revision_id,
            provider,
            model,
            &context_epoch,
        );
        let _creation_guard = root_lock.lock_owned().await;
        // Another request may have completed while this request waited for the
        // lock (or another process may have committed the row), so always
        // perform a second lookup immediately before invoking the provider.
        if let Some(root) = lookup_root_on_connection(
            &self.connect()?,
            &anchor.revision_id,
            provider,
            model,
            &pdf_source_epoch(anchor, model, paper_root_instruction),
        )
        .map_err(|e| ProviderError::local_state(e.to_string()))?
        {
            return Ok(root);
        }
        let outcome = port
            .interact(PaperInteractionRequest {
                model: model.to_string(),
                context_epoch: context_epoch.clone(),
                pdf_path: anchor.pdf_path.clone(),
                display_name: anchor.title.clone(),
                remote_file_id: None,
                previous_interaction_id: None,
                system_instruction: paper_root_instruction
                    .map(str::to_string)
                    .unwrap_or_else(paper_root_system_instruction),
                user_input: json!({
                    "task": "接收完整 PDF，建立供后续独立任务复用的来源根。只返回确认对象。"
                })
                .to_string(),
                response_schema: Some(named_schema(
                    "paper_root_ack",
                    json!({
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["acknowledged"],
                        "properties": {"acknowledged": {"type": "boolean", "const": true}}
                    }),
                )),
                inline_images: Vec::new(),
                kind: crate::provider_ports::PaperInteractionKind::default(),
            })
            .await?;
        // Re-check after the provider returns as well.  This covers a second
        // process that committed the unique Context Root while this request
        // was uploading/interacting.  The paid response is retained as a
        // usage receipt and its remote resources are queued for cleanup.
        self.persist_root_outcome(anchor, provider, model, &context_epoch, outcome)
    }

    #[allow(dead_code)]
    pub(crate) async fn ensure_root_for_route(
        &self,
        port: &dyn PaperModelPort,
        anchor: &DocumentFacts,
        route_id: &str,
        provider: &str,
        model: &str,
        paper_root_instruction: Option<&str>,
    ) -> ProviderResult<RootContext> {
        if let Some(root) = self.lookup_root_for_route_with_instruction(
            anchor,
            route_id,
            model,
            paper_root_instruction,
        )? {
            return Ok(root);
        }
        let semantic_epoch = pdf_source_epoch(anchor, model, paper_root_instruction);
        let context_epoch = route_scoped_context_epoch(&semantic_epoch, route_id);
        let root_lock = context_root_lock_for_route(
            &self.database_path,
            &anchor.revision_id,
            route_id,
            model,
            &context_epoch,
        );
        let _creation_guard = root_lock.lock_owned().await;
        if let Some(root) = self.lookup_root_for_route_with_instruction(
            anchor,
            route_id,
            model,
            paper_root_instruction,
        )? {
            return Ok(root);
        }
        let outcome = port
            .interact(PaperInteractionRequest {
                model: model.to_string(),
                context_epoch: context_epoch.clone(),
                pdf_path: anchor.pdf_path.clone(),
                display_name: anchor.title.clone(),
                remote_file_id: None,
                previous_interaction_id: None,
                system_instruction: paper_root_instruction
                    .map(str::to_string)
                    .unwrap_or_else(paper_root_system_instruction),
                user_input: json!({
                    "task": "接收完整 PDF，建立供后续独立任务复用的来源根。只返回确认对象。"
                })
                .to_string(),
                response_schema: Some(named_schema(
                    "paper_root_ack",
                    json!({
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["acknowledged"],
                        "properties": {"acknowledged": {"type": "boolean", "const": true}}
                    }),
                )),
                inline_images: Vec::new(),
                kind: PaperInteractionKind::Root,
            })
            .await?;
        self.persist_root_outcome_for_route_in_epoch(
            anchor,
            route_id,
            provider,
            model,
            &context_epoch,
            outcome,
        )
    }

    /// Route-aware counterpart of `persist_root_outcome`. The route identity
    /// is mandatory at every read and write, so legacy NULL rows cannot be
    /// promoted into a newly bound provider instance.
    fn persist_root_outcome_for_route(
        &self,
        anchor: &DocumentFacts,
        route_id: &str,
        provider: &str,
        model: &str,
        outcome: crate::provider_ports::PaperInteractionOutcome,
    ) -> ProviderResult<RootContext> {
        let epoch = route_scoped_context_epoch(&pdf_source_epoch(anchor, model, None), route_id);
        self.persist_root_outcome_for_route_in_epoch(
            anchor, route_id, provider, model, &epoch, outcome,
        )
    }

    fn persist_root_outcome_for_route_in_epoch(
        &self,
        anchor: &DocumentFacts,
        route_id: &str,
        provider: &str,
        model: &str,
        epoch: &str,
        outcome: crate::provider_ports::PaperInteractionOutcome,
    ) -> ProviderResult<RootContext> {
        self.require_provider_route(route_id)?;
        let context_epoch = epoch.to_string();
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        if !is_valid_root_ack(&outcome.text) {
            let empty = RootContext {
                context_epoch: context_epoch.clone(),
                remote_file_id: String::new(),
                remote_node_id: String::new(),
                local_node_id: None,
            };
            persist_orphan_root_outcome_for_route(
                &transaction,
                anchor,
                route_id,
                provider,
                model,
                &context_epoch,
                &outcome,
                &empty,
            )?;
            transaction
                .commit()
                .map_err(|error| ProviderError::local_state(error.to_string()))?;
            return Err(ProviderError::invalid(
                "PDF 来源根只允许最小确认对象，已拒绝包含其他内容的响应",
            ));
        }
        if let Some(root) = lookup_root_on_connection_for_route(
            &transaction,
            &anchor.revision_id,
            route_id,
            model,
            &context_epoch,
        )
        .map_err(|error| ProviderError::local_state(error.to_string()))?
        {
            persist_orphan_root_outcome_for_route(
                &transaction,
                anchor,
                route_id,
                provider,
                model,
                &context_epoch,
                &outcome,
                &root,
            )?;
            transaction
                .commit()
                .map_err(|error| ProviderError::local_state(error.to_string()))?;
            return Ok(root);
        }

        let local_node_id = Uuid::new_v4().to_string();
        transaction
            .execute(
                "INSERT INTO provider_nodes(
                   id, provider, model, context_epoch, provider_node_id,
                   parent_id, state, created_at, provider_route_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'complete', ?6, ?7)",
                params![
                    local_node_id,
                    provider,
                    model,
                    context_epoch,
                    outcome.provider_node_id,
                    now(),
                    route_id
                ],
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        let root_id: String = transaction.query_row(
            "SELECT id FROM context_roots WHERE revision_id=?1 AND provider_route_id=?2 AND provider=?3 AND model=?4 AND context_epoch=?5 ORDER BY created_at DESC LIMIT 1",
            params![anchor.revision_id,route_id,provider,model,context_epoch],|row|row.get(0)
        ).optional().map_err(|e|ProviderError::local_state(e.to_string()))?.unwrap_or_else(||Uuid::new_v4().to_string());
        transaction
            .execute(
                "INSERT INTO context_roots(
                   id, revision_id, provider, model, context_epoch,
                   provider_file_id, provider_node_id, state, created_at,
                   invalidated_at, provider_route_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'active', ?8, NULL, ?9) ON CONFLICT(id) DO UPDATE SET provider_file_id=excluded.provider_file_id, provider_node_id=excluded.provider_node_id, state='active', created_at=excluded.created_at, invalidated_at=NULL",
                params![
                    root_id,
                    anchor.revision_id,
                    provider,
                    model,
                    context_epoch,
                    outcome.provider_file_id,
                    outcome.provider_node_id,
                    now(),
                    route_id
                ],
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        insert_usage_receipt_for_route(&transaction, &root_id, route_id, &outcome.receipt)
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        Ok(RootContext {
            context_epoch,
            remote_file_id: outcome.provider_file_id,
            remote_node_id: outcome.provider_node_id,
            local_node_id: Some(local_node_id),
        })
    }

    /// Persist a successful root response atomically with its provider node and
    /// usage receipt.  A second lookup is performed inside the write
    /// transaction so a root committed by another process wins without
    /// allowing a UNIQUE constraint race to discard the paid receipt.
    fn persist_root_outcome(
        &self,
        anchor: &DocumentFacts,
        provider: &str,
        model: &str,
        context_epoch: &str,
        outcome: crate::provider_ports::PaperInteractionOutcome,
    ) -> ProviderResult<RootContext> {
        let mut connection = self.connect()?;
        let transaction = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        if let Some(root) = lookup_root_on_connection(
            &transaction,
            &anchor.revision_id,
            provider,
            model,
            context_epoch,
        )
        .map_err(|error| ProviderError::local_state(error.to_string()))?
        {
            persist_orphan_root_outcome(
                &transaction,
                anchor,
                provider,
                model,
                context_epoch,
                &outcome,
                &root,
            )?;
            transaction
                .commit()
                .map_err(|error| ProviderError::local_state(error.to_string()))?;
            return Ok(root);
        }

        let local_node_id = Uuid::new_v4().to_string();
        transaction
            .execute(
                "INSERT INTO provider_nodes(
                   id, provider, model, context_epoch, provider_node_id,
                   parent_id, state, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'complete', ?6)",
                params![
                    local_node_id,
                    provider,
                    model,
                    context_epoch,
                    outcome.provider_node_id,
                    now()
                ],
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        let candidate_root_id = Uuid::new_v4().to_string();
        transaction
            .execute(
                "INSERT INTO context_roots(
                   id, revision_id, provider, model, context_epoch,
                   provider_file_id, provider_node_id, state, created_at, invalidated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'active', ?8, NULL)
                 ON CONFLICT(revision_id, provider, model, context_epoch) DO UPDATE SET
                   provider_file_id = excluded.provider_file_id,
                   provider_node_id = excluded.provider_node_id,
                   state = 'active',
                   invalidated_at = NULL",
                params![
                    candidate_root_id,
                    anchor.revision_id,
                    provider,
                    model,
                    context_epoch,
                    outcome.provider_file_id,
                    outcome.provider_node_id,
                    now()
                ],
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        let root_id: String = transaction
            .query_row(
                "SELECT id FROM context_roots
                 WHERE revision_id = ?1 AND provider = ?2
                   AND model = ?3 AND context_epoch = ?4",
                params![anchor.revision_id, provider, model, context_epoch],
                |row| row.get(0),
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        insert_usage_receipt(&transaction, &root_id, &outcome.receipt)
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        transaction
            .commit()
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        Ok(RootContext {
            context_epoch: context_epoch.to_string(),
            remote_file_id: outcome.provider_file_id,
            remote_node_id: outcome.provider_node_id,
            local_node_id: Some(local_node_id),
        })
    }

    fn lookup_root_scoped(
        &self,
        anchor: &DocumentFacts,
        route_id: Option<&str>,
        provider: &str,
        model: &str,
    ) -> ProviderResult<Option<RootContext>> {
        match route_id {
            Some(route_id) => self.lookup_root_for_route(anchor, route_id, model),
            None => self.lookup_root(anchor, provider, model),
        }
    }

    fn lookup_root(
        &self,
        anchor: &DocumentFacts,
        provider: &str,
        model: &str,
    ) -> ProviderResult<Option<RootContext>> {
        let context_epoch = pdf_source_epoch(anchor, model, None);
        let connection = self.connect()?;
        lookup_root_on_connection(
            &connection,
            &anchor.revision_id,
            provider,
            model,
            &context_epoch,
        )
        .map_err(|error| ProviderError::local_state(error.to_string()))
    }

    fn lookup_root_for_route(
        &self,
        anchor: &DocumentFacts,
        route_id: &str,
        model: &str,
    ) -> ProviderResult<Option<RootContext>> {
        self.lookup_root_for_route_with_instruction(anchor, route_id, model, None)
    }
    pub(crate) fn lookup_root_for_route_with_instruction(
        &self,
        anchor: &DocumentFacts,
        route_id: &str,
        model: &str,
        instruction: Option<&str>,
    ) -> ProviderResult<Option<RootContext>> {
        let semantic_epoch = pdf_source_epoch(anchor, model, instruction);
        let context_epoch = route_scoped_context_epoch(&semantic_epoch, route_id);
        lookup_root_on_connection_for_route(
            &self.connect()?,
            &anchor.revision_id,
            route_id,
            model,
            &context_epoch,
        )
        .map_err(|error| ProviderError::local_state(error.to_string()))
    }

    fn invalidate_root_scoped(
        &self,
        anchor: &DocumentFacts,
        route_id: Option<&str>,
        provider: &str,
        model: &str,
    ) -> ProviderResult<()> {
        match route_id {
            Some(route_id) => self.invalidate_root_for_route(anchor, route_id, model),
            None => self.invalidate_root(anchor, provider, model),
        }
    }

    fn invalidate_root(
        &self,
        anchor: &DocumentFacts,
        provider: &str,
        model: &str,
    ) -> ProviderResult<()> {
        let context_epoch = pdf_source_epoch(anchor, model, None);
        self.connect()?
            .execute(
                "UPDATE context_roots SET state = 'stale', invalidated_at = ?1
                 WHERE revision_id = ?2 AND provider = ?3
                   AND model = ?4 AND context_epoch = ?5
                   AND invalidated_at IS NULL",
                params![now(), anchor.revision_id, provider, model, context_epoch],
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn invalidate_root_for_route(
        &self,
        anchor: &DocumentFacts,
        route_id: &str,
        model: &str,
    ) -> ProviderResult<()> {
        self.invalidate_root_for_route_with_instruction(anchor, route_id, model, None)
    }
    pub(crate) fn invalidate_root_for_route_with_instruction(
        &self,
        anchor: &DocumentFacts,
        route_id: &str,
        model: &str,
        instruction: Option<&str>,
    ) -> ProviderResult<()> {
        let semantic_epoch = pdf_source_epoch(anchor, model, instruction);
        let context_epoch = route_scoped_context_epoch(&semantic_epoch, route_id);
        self.connect()?
            .execute(
                "UPDATE context_roots SET state = 'stale', invalidated_at = ?1
                 WHERE revision_id = ?2 AND provider_route_id = ?3
                   AND model = ?4 AND context_epoch = ?5
                   AND invalidated_at IS NULL",
                params![now(), anchor.revision_id, route_id, model, context_epoch],
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        Ok(())
    }

    fn load_anchor(&self, request: &GenerateReadingArtifactRequest) -> ProviderResult<AnchorFacts> {
        if request.revision_id.trim().is_empty()
            || request.ocr_revision_id.trim().is_empty()
            || request.block_id.trim().is_empty()
        {
            return Err(ProviderError::invalid(
                "Reading Artifact request is missing its revision, OCR revision, or Block",
            ));
        }
        let facts = self
            .connect()?
            .query_row(
                "SELECT r.paper_id, r.id, r.sha256,
                        COALESCE(m.title, p.file_name), p.relative_path,
                        o.id, b.id, b.block_type, b.text_content, b.content_digest,
                        pg.page_number, b.block_index, b.x0, b.y0, b.x1, b.y1
                 FROM ocr_blocks b
                 JOIN ocr_pages pg ON pg.id = b.ocr_page_id
                 JOIN ocr_revisions o ON o.id = pg.ocr_revision_id
                 JOIN document_revisions r ON r.id = o.revision_id
                 JOIN papers p ON p.id = r.paper_id
                 LEFT JOIN paper_metadata m ON m.revision_id = r.id
                 WHERE r.id = ?1 AND o.id = ?2 AND b.id = ?3
                   AND o.status = 'ready'",
                params![
                    request.revision_id,
                    request.ocr_revision_id,
                    request.block_id
                ],
                |row| {
                    let relative_path = PathBuf::from(row.get::<_, String>(4)?);
                    Ok(AnchorFacts {
                        document: DocumentFacts {
                            paper_id: row.get(0)?,
                            revision_id: row.get(1)?,
                            revision_sha256: row.get(2)?,
                            title: row.get(3)?,
                            pdf_path: self.absolute_pdf(&relative_path),
                        },
                        ocr_revision_id: row.get(5)?,
                        block_id: row.get(6)?,
                        block_type: row.get(7)?,
                        text: row.get(8)?,
                        content_digest: row.get(9)?,
                        page_number: row.get(10)?,
                        block_index: row.get(11)?,
                        bbox: [row.get(12)?, row.get(13)?, row.get(14)?, row.get(15)?],
                    })
                },
            )
            .map_err(|_| {
                ProviderError::invalid(
                    "The selected Block does not belong to the requested OCR and Document Revision",
                )
            })?;
        if !facts.pdf_path.is_file() {
            return Err(ProviderError::local_state(
                "The source PDF is unavailable for this Reading Artifact",
            ));
        }
        Ok(facts)
    }

    fn translation_context(&self, anchor: &AnchorFacts) -> ProviderResult<Value> {
        let connection = self.connect()?;
        let mut statement = connection
            .prepare(
                "SELECT b.id, b.block_index, b.block_type, b.text_content
                 FROM ocr_blocks b
                 JOIN ocr_pages pg ON pg.id = b.ocr_page_id
                 WHERE pg.ocr_revision_id = ?1 AND pg.page_number = ?2
                   AND b.block_index BETWEEN ?3 AND ?4
                 ORDER BY b.block_index",
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        let neighbors = statement
            .query_map(
                params![
                    anchor.ocr_revision_id,
                    anchor.page_number,
                    (anchor.block_index - 2).max(0),
                    anchor.block_index + 2
                ],
                |row| {
                    Ok(json!({
                        "blockId": row.get::<_, String>(0)?,
                        "blockIndex": row.get::<_, i64>(1)?,
                        "blockType": row.get::<_, String>(2)?,
                        "text": row.get::<_, String>(3)?
                    }))
                },
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        drop(statement);
        let brief = self
            .head_content(&anchor.revision_id, "brief")?
            .and_then(|value| value.get("summary").cloned())
            .unwrap_or(Value::Null);
        let combined = neighbors
            .iter()
            .filter_map(|value| value.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join(
                "
",
            )
            .to_lowercase();
        let glossary = matching_entries(
            self.head_content(&anchor.revision_id, "glossary")?,
            "term",
            &combined,
        );
        let symbols = matching_entries(
            self.head_content(&anchor.revision_id, "symbol_table")?,
            "symbol",
            &combined,
        );
        Ok(json!({
            "samePageBlocks": neighbors,
            "brief": brief,
            "matchedGlossary": glossary,
            "matchedSymbols": symbols
        }))
    }

    fn head_content(&self, revision_id: &str, kind: &str) -> ProviderResult<Option<Value>> {
        let raw: Option<String> = self
            .connect()?
            .query_row(
                "SELECT a.content_json
                 FROM artifact_heads h
                 JOIN artifacts a ON a.id = h.artifact_id
                 WHERE a.revision_id = ?1 AND h.kind = ?2 AND h.object_key = ''",
                params![revision_id, kind],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        raw.map(|value| {
            let parsed: Value = serde_json::from_str(&value)
                .map_err(|error| ProviderError::local_state(error.to_string()))?;
            crate::auxiliary_state::load_legacy_table_overrides(
                &self.connect()?,
                revision_id,
                kind,
                &parsed,
            )
            .map_err(ProviderError::local_state)
        })
        .transpose()
    }

    fn publish_artifact(
        &self,
        anchor: &AnchorFacts,
        kind: &str,
        content: Value,
        provider_node_id: Option<String>,
        model: &str,
        context_epoch: &str,
    ) -> ProviderResult<ArtifactProjection> {
        self.artifact_module
            .publish(ArtifactDraft {
                paper_id: anchor.paper_id.clone(),
                revision_id: anchor.revision_id.clone(),
                ocr_revision_id: Some(anchor.ocr_revision_id.clone()),
                kind: kind.to_string(),
                object_key: anchor.block_id.clone(),
                content,
                evidence: vec![EvidenceAnchor {
                    revision_id: anchor.revision_id.clone(),
                    page_number: anchor.page_number,
                    block_id: Some(anchor.block_id.clone()),
                    bbox: Some(anchor.bbox),
                    excerpt: Some(anchor.text.clone()),
                }],
                dependency_snapshot: json!({
                    "revisionId": anchor.revision_id,
                    "ocrRevisionId": anchor.ocr_revision_id,
                    "blockId": anchor.block_id,
                    "blockDigest": anchor.content_digest,
                    "model": model,
                    "contextEpoch": context_epoch
                }),
                provider_node_id,
            })
            .map_err(ProviderError::local_state)
    }

    /// Stores a provider node only when an optional parent belongs to the
    /// exact same route/model/epoch. Legacy NULL and cross-route parents are
    /// rejected instead of silently starting from the wrong remote branch.
    #[allow(dead_code)]
    fn store_provider_node_scoped(
        &self,
        route_id: Option<&str>,
        provider: &str,
        model: &str,
        context_epoch: &str,
        remote_node_id: &str,
        parent_id: Option<&str>,
        state: &str,
    ) -> ProviderResult<String> {
        match route_id {
            Some(route_id) => self.store_provider_node_for_route(
                route_id,
                provider,
                model,
                context_epoch,
                remote_node_id,
                parent_id,
                state,
            ),
            None => self.store_provider_node(
                provider,
                model,
                context_epoch,
                remote_node_id,
                parent_id,
                state,
            ),
        }
    }

    pub(crate) fn store_provider_node_for_route(
        &self,
        route_id: &str,
        provider: &str,
        model: &str,
        context_epoch: &str,
        remote_node_id: &str,
        parent_id: Option<&str>,
        state: &str,
    ) -> ProviderResult<String> {
        self.require_provider_route(route_id)?;
        let connection = self.connect()?;
        if let Some(parent_id) = parent_id {
            let parent_matches = connection
                .query_row(
                    "SELECT 1 FROM provider_nodes
                     WHERE id = ?1 AND provider_route_id = ?2
                       AND model = ?3 AND context_epoch = ?4",
                    params![parent_id, route_id, model, context_epoch],
                    |_| Ok(()),
                )
                .optional()
                .map_err(|error| ProviderError::local_state(error.to_string()))?
                .is_some();
            if !parent_matches {
                return Err(ProviderError::invalid(
                    "Provider node parent must belong to the same provider route, model, and context epoch",
                ));
            }
        }
        let id = Uuid::new_v4().to_string();
        connection
            .execute(
                "INSERT INTO provider_nodes(
                   id, provider, model, context_epoch, provider_node_id,
                   parent_id, state, created_at, provider_route_id
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    id,
                    provider,
                    model,
                    context_epoch,
                    remote_node_id,
                    parent_id,
                    state,
                    now(),
                    route_id
                ],
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        Ok(id)
    }

    fn store_provider_node(
        &self,
        provider: &str,
        model: &str,
        context_epoch: &str,
        remote_node_id: &str,
        parent_id: Option<&str>,
        state: &str,
    ) -> ProviderResult<String> {
        let id = Uuid::new_v4().to_string();
        self.connect()?
            .execute(
                "INSERT INTO provider_nodes(
                   id, provider, model, context_epoch, provider_node_id,
                   parent_id, state, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    id,
                    provider,
                    model,
                    context_epoch,
                    remote_node_id,
                    parent_id,
                    state,
                    now()
                ],
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        Ok(id)
    }

    fn set_provider_node_state(&self, id: &str, state: &str) -> ProviderResult<()> {
        self.connect()?
            .execute(
                "UPDATE provider_nodes SET state = ?1 WHERE id = ?2",
                params![state, id],
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        Ok(())
    }

    pub(crate) fn record_usage_scoped(
        &self,
        operation_id: &str,
        route_id: &str,
        envelope: &UsageEnvelope,
    ) -> ProviderResult<()> {
        let connection = self.connect()?;
        insert_usage_receipt_for_route(&connection, operation_id, route_id, envelope)
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        Ok(())
    }

    fn record_usage(&self, operation_id: &str, envelope: &UsageEnvelope) -> ProviderResult<()> {
        let connection = self.connect()?;
        insert_usage_receipt(&connection, operation_id, envelope)
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        Ok(())
    }

    fn persist_display_crop(&self, data_url: Option<&str>) -> ProviderResult<String> {
        let data_url = data_url.ok_or_else(|| {
            ProviderError::invalid(
                "Lens requires a rendered display crop from the selected PDF Block",
            )
        })?;
        let (mime_type, bytes) = decode_image_data_url(data_url)?;
        let extension = match mime_type.as_str() {
            "image/png" => "png",
            "image/jpeg" => "jpg",
            "image/webp" => "webp",
            _ => {
                return Err(ProviderError::invalid(
                    "Lens crop image type is unsupported",
                ))
            }
        };
        let relative = PathBuf::from(".read-desktop")
            .join("artifacts")
            .join("lens")
            .join(Uuid::new_v4().to_string())
            .join(format!("display.{extension}"));
        let target = self.workspace_root.join(&relative);
        let parent = target
            .parent()
            .ok_or_else(|| ProviderError::local_state("Lens crop path has no parent"))?;
        fs::create_dir_all(parent)
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        let staging = target.with_extension(format!("{extension}.tmp"));
        let mut temporary = TemporaryCropFile::new(staging.clone());
        let mut file = fs::File::create(&staging)
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        file.write_all(&bytes)
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        file.flush()
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        file.sync_all()
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        drop(file);

        // The directory name is random, so an existing target indicates a
        // collision or stale data. Never replace it implicitly: the temporary
        // guard removes only the new temporary file on this error path.
        match fs::symlink_metadata(&target) {
            Ok(_) => {
                return Err(ProviderError::local_state(
                    "Lens crop target already exists",
                ))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(ProviderError::local_state(error.to_string())),
        }
        fs::rename(&staging, &target)
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
        temporary.publish();
        Ok(relative.to_string_lossy().replace(char::from(92), "/"))
    }

    fn connect(&self) -> ProviderResult<Connection> {
        db::open(&self.database_path).map_err(|error| ProviderError::local_state(error.to_string()))
    }

    fn require_provider_route(&self, route_id: &str) -> ProviderResult<()> {
        if route_id.trim().is_empty() {
            return Err(ProviderError::invalid("Provider route is required"));
        }
        let exists = self
            .connect()?
            .query_row(
                "SELECT 1 FROM provider_route_snapshots WHERE route_id = ?1",
                params![route_id],
                |_| Ok(()),
            )
            .optional()
            .map_err(|error| ProviderError::local_state(error.to_string()))?
            .is_some();
        if exists {
            Ok(())
        } else {
            Err(ProviderError::invalid(
                "Provider route snapshot is unavailable",
            ))
        }
    }

    fn absolute_pdf(&self, relative: &Path) -> PathBuf {
        if relative.is_absolute() {
            relative.to_path_buf()
        } else {
            crate::library_paths::join_workspace_relative(
                &self.workspace_root,
                &relative.to_string_lossy(),
            )
            .unwrap_or_else(|_| self.workspace_root.join(relative))
        }
    }
}

fn lookup_root_on_connection_for_route(
    connection: &Connection,
    revision_id: &str,
    route_id: &str,
    model: &str,
    context_epoch: &str,
) -> rusqlite::Result<Option<RootContext>> {
    connection
        .query_row(
            "SELECT c.provider_file_id, c.provider_node_id,
                    (SELECT n.id FROM provider_nodes n
                     WHERE n.provider_route_id = c.provider_route_id
                       AND n.model = c.model
                       AND n.context_epoch = c.context_epoch
                       AND n.provider_node_id = c.provider_node_id
                       AND n.state = 'complete'
                     ORDER BY n.created_at DESC LIMIT 1)
             FROM context_roots c
             WHERE c.revision_id = ?1 AND c.provider_route_id = ?2
               AND c.model = ?3 AND c.context_epoch = ?4
               AND c.invalidated_at IS NULL
               AND c.provider_file_id IS NOT NULL
               AND c.provider_node_id IS NOT NULL
             LIMIT 1",
            params![revision_id, route_id, model, context_epoch],
            |row| {
                Ok(RootContext {
                    context_epoch: context_epoch.to_string(),
                    remote_file_id: row.get(0)?,
                    remote_node_id: row.get(1)?,
                    local_node_id: row.get(2)?,
                })
            },
        )
        .optional()
}

fn lookup_root_on_connection(
    connection: &Connection,
    revision_id: &str,
    provider: &str,
    model: &str,
    context_epoch: &str,
) -> rusqlite::Result<Option<RootContext>> {
    connection
        .query_row(
            "SELECT c.provider_file_id, c.provider_node_id,
                    (SELECT n.id FROM provider_nodes n
                     WHERE n.provider = c.provider
                       AND n.model = c.model
                       AND n.context_epoch = c.context_epoch
                       AND n.provider_node_id = c.provider_node_id
                     ORDER BY n.created_at DESC LIMIT 1)
             FROM context_roots c
             WHERE c.revision_id = ?1 AND c.provider = ?2
               AND c.model = ?3 AND c.context_epoch = ?4
               AND c.invalidated_at IS NULL
               AND c.provider_file_id IS NOT NULL
               AND c.provider_node_id IS NOT NULL
             LIMIT 1",
            params![revision_id, provider, model, context_epoch],
            |row| {
                Ok(RootContext {
                    context_epoch: context_epoch.to_string(),
                    remote_file_id: row.get(0)?,
                    remote_node_id: row.get(1)?,
                    local_node_id: row.get(2)?,
                })
            },
        )
        .optional()
}

fn persist_orphan_root_outcome_for_route(
    connection: &Connection,
    anchor: &DocumentFacts,
    route_id: &str,
    provider: &str,
    model: &str,
    context_epoch: &str,
    outcome: &crate::provider_ports::PaperInteractionOutcome,
    canonical_root: &RootContext,
) -> ProviderResult<()> {
    let orphan_node_id = Uuid::new_v4().to_string();
    connection
        .execute(
            "INSERT INTO provider_nodes(
               id, provider, model, context_epoch, provider_node_id,
               parent_id, state, created_at, provider_route_id
             ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'orphaned', ?6, ?7)",
            params![
                orphan_node_id,
                provider,
                model,
                context_epoch,
                &outcome.provider_node_id,
                now(),
                route_id
            ],
        )
        .map_err(|error| ProviderError::local_state(error.to_string()))?;
    insert_usage_receipt_for_route(connection, &orphan_node_id, route_id, &outcome.receipt)
        .map_err(|error| ProviderError::local_state(error.to_string()))?;

    let endpoint_scope: String = connection
        .query_row(
            "SELECT endpoint_scope FROM provider_route_snapshots WHERE route_id = ?1",
            params![route_id],
            |row| row.get(0),
        )
        .map_err(|error| ProviderError::local_state(error.to_string()))?;
    let resources = [
        (
            "file",
            outcome.provider_file_id.as_str(),
            canonical_root.remote_file_id.as_str(),
        ),
        (
            "interaction",
            outcome.provider_node_id.as_str(),
            canonical_root.remote_node_id.as_str(),
        ),
    ];
    let cleanup_reason = "Context root response was not retained as the canonical PDF source";
    for (resource_kind, remote_id, canonical_id) in resources {
        if remote_id.trim().is_empty() || remote_id == canonical_id {
            continue;
        }
        connection
            .execute(
                "INSERT INTO remote_tombstones(
                   id, provider, resource_kind, remote_id, paper_id, state,
                   attempts, last_error, created_at, updated_at,
                   endpoint_scope, ownership_status
                 ) VALUES (
                   ?1, ?2, ?3, ?4, ?5, 'pending', 0, ?6, ?7, ?7, ?8, 'exact'
                 )
                 ON CONFLICT(endpoint_scope, resource_kind, remote_id)
                   WHERE endpoint_scope IS NOT NULL
                 DO UPDATE SET
                   state = 'pending', last_error = excluded.last_error,
                   updated_at = excluded.updated_at, ownership_status = 'exact'",
                params![
                    Uuid::new_v4().to_string(),
                    provider,
                    resource_kind,
                    remote_id,
                    Some(anchor.paper_id.as_str()),
                    cleanup_reason,
                    now(),
                    endpoint_scope
                ],
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
    }
    Ok(())
}

fn persist_orphan_root_outcome(
    connection: &Connection,
    anchor: &DocumentFacts,
    provider: &str,
    model: &str,
    context_epoch: &str,
    outcome: &crate::provider_ports::PaperInteractionOutcome,
    canonical_root: &RootContext,
) -> ProviderResult<()> {
    let orphan_node_id = Uuid::new_v4().to_string();
    connection
        .execute(
            "INSERT INTO provider_nodes(
               id, provider, model, context_epoch, provider_node_id,
               parent_id, state, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, NULL, 'orphaned', ?6)",
            params![
                orphan_node_id,
                provider,
                model,
                context_epoch,
                &outcome.provider_node_id,
                now()
            ],
        )
        .map_err(|error| ProviderError::local_state(error.to_string()))?;
    insert_usage_receipt(connection, &orphan_node_id, &outcome.receipt)
        .map_err(|error| ProviderError::local_state(error.to_string()))?;

    // The response was successful (and therefore billable), but its remote
    // file/interaction is not the canonical root.  Keep both resources in the
    // normal retryable tombstone queue.  Comparing IDs avoids accidentally
    // tombstoning a provider that reused an existing resource.
    let resources = [
        (
            "file",
            outcome.provider_file_id.as_str(),
            canonical_root.remote_file_id.as_str(),
        ),
        (
            "interaction",
            outcome.provider_node_id.as_str(),
            canonical_root.remote_node_id.as_str(),
        ),
    ];
    let cleanup_reason = "Context root response was not retained as the canonical PDF source";
    for (resource_kind, remote_id, canonical_id) in resources {
        if remote_id.trim().is_empty() || remote_id == canonical_id {
            continue;
        }
        connection
            .execute(
                "INSERT INTO remote_tombstones(
                   id, provider, resource_kind, remote_id, paper_id, state,
                   attempts, last_error, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, 'pending', 0, ?6, ?7, ?7)
                 ON CONFLICT(provider, resource_kind, remote_id) DO UPDATE SET
                   state = 'pending', last_error = excluded.last_error,
                   updated_at = excluded.updated_at",
                params![
                    Uuid::new_v4().to_string(),
                    provider,
                    resource_kind,
                    remote_id,
                    Some(anchor.paper_id.as_str()),
                    cleanup_reason,
                    now()
                ],
            )
            .map_err(|error| ProviderError::local_state(error.to_string()))?;
    }
    Ok(())
}

fn insert_usage_receipt_for_route(
    connection: &Connection,
    operation_id: &str,
    route_id: &str,
    envelope: &UsageEnvelope,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO usage_receipts(
           id, operation_id, provider, model, context_epoch,
           input_tokens, cached_input_tokens, uncached_input_tokens,
           output_tokens, reasoning_tokens, latency_ms, estimated_cost,
           file_reuse, session_resume, paper_root_branch, created_at,
           provider_route_id
         ) VALUES (
           ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
           ?13, ?14, ?15, ?16, ?17
         )",
        params![
            Uuid::new_v4().to_string(),
            operation_id,
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
            route_id
        ],
    )?;
    Ok(())
}

fn insert_usage_receipt(
    connection: &Connection,
    operation_id: &str,
    envelope: &UsageEnvelope,
) -> rusqlite::Result<()> {
    connection.execute(
        "INSERT INTO usage_receipts(
           id, operation_id, provider, model, context_epoch,
           input_tokens, cached_input_tokens, uncached_input_tokens,
           output_tokens, reasoning_tokens, latency_ms, estimated_cost,
           file_reuse, session_resume, paper_root_branch, created_at
         ) VALUES (
           ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
           ?13, ?14, ?15, ?16
         )",
        params![
            Uuid::new_v4().to_string(),
            operation_id,
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
            now()
        ],
    )?;
    Ok(())
}

fn lens_inline_images(data_url: Option<&str>) -> ProviderResult<Vec<InlineImageInput>> {
    let data_url = data_url.ok_or_else(|| {
        ProviderError::invalid("Lens requires a cost-optimized crop from the selected PDF Block")
    })?;
    let (mime_type, bytes) = decode_image_data_url(data_url)?;
    Ok(vec![InlineImageInput {
        mime_type,
        base64_data: base64::engine::general_purpose::STANDARD.encode(bytes),
    }])
}

fn decode_image_data_url(data_url: &str) -> ProviderResult<(String, Vec<u8>)> {
    let (header, encoded) = data_url
        .split_once(',')
        .ok_or_else(|| ProviderError::invalid("Lens crop is not a data URL"))?;
    let mime_type = header
        .strip_prefix("data:")
        .and_then(|value| value.strip_suffix(";base64"))
        .filter(|value| matches!(*value, "image/png" | "image/jpeg" | "image/webp"))
        .ok_or_else(|| ProviderError::invalid("Lens crop data URL type is unsupported"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| ProviderError::invalid("Lens crop base64 is invalid"))?;
    if bytes.is_empty() || bytes.len() > 20 * 1024 * 1024 {
        return Err(ProviderError::invalid(
            "Lens crop must contain between 1 byte and 20 MiB",
        ));
    }
    Ok((mime_type.to_string(), bytes))
}

fn default_output_language() -> String {
    "zh-CN".to_string()
}

fn default_paper_provider() -> String {
    "gemini".to_string()
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn block_payload(anchor: &AnchorFacts) -> Value {
    json!({
        "blockId": anchor.block_id,
        "blockType": anchor.block_type,
        "pageNumber": anchor.page_number,
        "bboxNorm1000": anchor.bbox,
        "text": anchor.text,
        "contentDigest": anchor.content_digest
    })
}

fn is_lens_block(block_type: &str) -> bool {
    lens_kind(block_type).is_some()
}

fn lens_kind(block_type: &str) -> Option<&'static str> {
    let normalized = block_type.to_lowercase();
    if normalized.contains("formula") || normalized.contains("equation") {
        Some("formula")
    } else if normalized.contains("figure")
        || normalized.contains("image")
        || normalized.contains("picture")
    {
        Some("figure")
    } else if normalized.contains("table") {
        Some("table")
    } else {
        None
    }
}
fn parse_json_object(text: &str, label: &str) -> ProviderResult<Value> {
    let start = text
        .find('{')
        .ok_or_else(|| ProviderError::invalid(format!("{label} output is not a JSON object")))?;
    let end = text
        .rfind('}')
        .ok_or_else(|| ProviderError::invalid(format!("{label} output is incomplete")))?;
    let value: Value = serde_json::from_str(&text[start..=end]).map_err(|error| {
        ProviderError::invalid(format!("{label} output failed schema validation: {error}"))
    })?;
    if !value.is_object() {
        return Err(ProviderError::invalid(format!(
            "{label} output must be an object"
        )));
    }
    Ok(value)
}

pub(crate) fn is_valid_root_ack(text: &str) -> bool {
    let clean = strip_reasoning_markers(text);
    if let Ok(val) = serde_json::from_str::<Value>(clean) {
        return is_minimal_ack_value(&val);
    }
    if let (Some(start), Some(end)) = (clean.find('{'), clean.rfind('}')) {
        if start <= end {
            if let Ok(val) = serde_json::from_str::<Value>(&clean[start..=end]) {
                return is_minimal_ack_value(&val);
            }
        }
    }
    false
}

fn strip_reasoning_markers(text: &str) -> &str {
    let trimmed = text.trim();
    if let Some(end_think) = trimmed.rfind("</think>") {
        trimmed[end_think + "</think>".len()..].trim()
    } else {
        trimmed
    }
}

fn is_minimal_ack_value(val: &Value) -> bool {
    if let Some(map) = val.as_object() {
        map.len() == 1 && map.get("acknowledged") == Some(&Value::Bool(true))
    } else {
        false
    }
}

fn required_string<'a>(value: &'a Value, key: &str, label: &str) -> ProviderResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or_else(|| ProviderError::invalid(format!("{label} is missing {key}")))
}

/// Normalize a markdown string that may have been emitted with literal `\n`/`\t` escape
/// sequences (the model sometimes returns those inside a JSON string). Collapse runs of
/// multiple spaces, remove stray carriage returns, and ensure trailing newlines are
/// trimmed so the rendered output stays consistent.
pub(crate) fn normalize_markdown_field(input: &str) -> String {
    // 1. Decode leftover JSON `\\n` / `\\r` / `\\t` sequences, but never eat
    //    LaTeX commands such as `\nu`, `\rho`, `\text`, `\tau`.
    let decoded = decode_literal_escapes(input);
    // 2. Collapse runs of horizontal whitespace to a single space. Newlines
    //    stay intact so already-valid lists and nested bullets survive.
    let mut collapsed = String::with_capacity(decoded.len());
    let mut space_run = false;
    for ch in decoded.chars() {
        if ch == ' ' || ch == '\t' {
            if !space_run {
                collapsed.push(' ');
                space_run = true;
            }
            continue;
        }
        space_run = false;
        collapsed.push(ch);
    }
    let mut normalized = collapsed.trim().to_string();

    // 3. Heal bold markers: balance unpaired **, strip spaces around contents.
    normalized = repair_inline_bold(&normalized);

    // 4. Split packed numbered/bulleted items that the LLM crammed into one
    //    paragraph (`1. xxx；2. yyy` or `包括：1. xxx 2. yyy`).
    normalized = split_packed_list_items(&normalized);

    normalized
}

fn decode_literal_escapes(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            let next = chars[i + 1];
            let after_letter = chars
                .get(i + 2)
                .copied()
                .is_some_and(|ch| ch.is_ascii_alphabetic());
            if !after_letter && (next == 'n' || next == 'r') {
                out.push('\n');
                i += 2;
                continue;
            }
            if !after_letter && next == 't' {
                out.push('\t');
                i += 2;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn repair_inline_bold(input: &str) -> String {
    // Decode escaped ** so we never carry `**` literal.
    if input.contains("\\**") {
        return repair_inline_bold(&input.replace("\\**", "**"));
    }
    let chars: Vec<char> = input.chars().collect();
    if chars.is_empty() {
        return input.to_string();
    }
    let mut positions: Vec<usize> = Vec::new();
    let mut i = 0;
    while i + 1 < chars.len() {
        if chars[i] == '*' && chars[i + 1] == '*' {
            positions.push(i);
            i += 2;
        } else {
            i += 1;
        }
    }
    if positions.is_empty() {
        return input.to_string();
    }
    if positions.len() % 2 == 1 {
        // Unpaired stray `**` at the end. Close it before the next newline
        // (or at the document end). Use char-based splicing to keep non-ASCII
        // boundaries intact.
        let stray = *positions.last().unwrap();
        let split_at: usize = chars
            .iter()
            .enumerate()
            .skip(stray + 2)
            .find(|(_, c)| **c == '\n')
            .map(|(idx, _)| idx)
            .unwrap_or(chars.len());
        let mut next_chars: Vec<char> = chars[..split_at].to_vec();
        next_chars.push('*');
        next_chars.push('*');
        next_chars.extend_from_slice(&chars[split_at..]);
        return repair_inline_bold(&chars_to_string(&next_chars));
    }
    // Balanced pairs. Trim leading/trailing whitespace inside each pair.
    let mut result: Vec<char> = Vec::with_capacity(chars.len());
    let mut idx = 0usize;
    for pair in positions.chunks_exact(2) {
        let open = pair[0];
        let close = pair[1];
        for c in &chars[idx..open] {
            result.push(*c);
        }
        result.push('*');
        result.push('*');
        let mut content_start = open + 2;
        while content_start < close && (chars[content_start] == ' ' || chars[content_start] == '\t')
        {
            content_start += 1;
        }
        let mut content_end = close;
        while content_end > content_start
            && (chars[content_end - 1] == ' ' || chars[content_end - 1] == '\t')
        {
            content_end -= 1;
        }
        for c in &chars[content_start..content_end] {
            result.push(*c);
        }
        result.push('*');
        result.push('*');
        idx = close + 2;
    }
    for c in &chars[idx..] {
        result.push(*c);
    }
    chars_to_string(&result)
}

fn chars_to_string(chars: &[char]) -> String {
    let mut out = String::with_capacity(chars.len());
    for c in chars {
        out.push(*c);
    }
    out
}

fn split_packed_list_items(input: &str) -> String {
    // Insert a newline before a list marker when the previous item ended with
    // a terminator (`；` / `。` / `：` / …) or the numbers run 1, 2, 3… in the
    // same paragraph. Do not split prose such as "Figure 1. The results" or
    // "described in 1. Introduction and 2. Methods".
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len() + 32);
    let mut i = 0usize;
    let mut line_start = true;
    let mut last_non_space: Option<char> = None;
    let mut last_number: u32 = 0;
    let mut in_bold = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '\n' {
            out.push('\n');
            i += 1;
            line_start = true;
            last_non_space = None;
            last_number = 0;
            continue;
        }
        if c == '*' && chars.get(i + 1) == Some(&'*') {
            in_bold = !in_bold;
            out.push('*');
            out.push('*');
            i += 2;
            line_start = false;
            last_non_space = Some('*');
            continue;
        }
        if (c == ' ' || c == '\t') && !line_start {
            out.push(' ');
            i += 1;
            continue;
        }
        let marker = if in_bold {
            None
        } else {
            detect_list_marker(&chars, i)
        };
        if let Some((marker_len, number)) = marker {
            if !preceded_by_latin_word(&chars, i) {
                let sequential = number.is_some_and(|n| last_number > 0 && n == last_number + 1);
                let should_split =
                    line_start || last_non_space.is_some_and(is_list_terminator) || sequential;
                if should_split {
                    if !line_start {
                        while out.ends_with(' ') || out.ends_with('\t') {
                            out.pop();
                        }
                        out.push('\n');
                    }
                    out.push_str(&chars[i..i + marker_len].iter().collect::<String>());
                    i += marker_len;
                    line_start = false;
                    last_non_space = None;
                    if let Some(n) = number {
                        last_number = n;
                    }
                    continue;
                }
            }
        }
        out.push(c);
        if !c.is_whitespace() {
            last_non_space = Some(c);
            line_start = false;
        }
        i += 1;
    }
    out
}

fn detect_list_marker(chars: &[char], i: usize) -> Option<(usize, Option<u32>)> {
    if chars.get(i) == Some(&'-') && chars.get(i + 1) == Some(&' ') {
        return Some((2, None));
    }
    let mut j = i;
    let mut digits = String::new();
    while j < chars.len() && chars[j].is_ascii_digit() && digits.len() < 3 {
        digits.push(chars[j]);
        j += 1;
    }
    if digits.is_empty() {
        return None;
    }
    if j >= chars.len() {
        return None;
    }
    let sep = chars[j];
    let number = digits.parse::<u32>().ok();
    if sep == '.' || sep == ')' || sep == '．' {
        if j + 1 < chars.len() && (chars[j + 1] == ' ' || chars[j + 1] == '\t') {
            return Some(((j + 1) - i + 1, number));
        }
        if j + 1 == chars.len() {
            return Some((j - i + 1, number));
        }
        return None;
    }
    if sep == '、' {
        return Some((j - i + 1, number));
    }
    None
}

fn preceded_by_latin_word(chars: &[char], i: usize) -> bool {
    let mut k = i;
    while k > 0 && (chars[k - 1] == ' ' || chars[k - 1] == '\t') {
        k -= 1;
    }
    k > 0 && chars[k - 1].is_ascii_alphabetic()
}

fn is_list_terminator(c: char) -> bool {
    // Include `?` / `;` / `,` because Chinese `，` / `：` sometimes round-trip
    // through the JSON gateway as Latin placeholders. Replacement char U+FFFD
    // is the same class of corruption.
    matches!(
        c,
        '。' | '！' | '!' | '?' | ';' | '；' | '：' | ':' | '，' | ',' | '\u{FFFD}'
    )
}

/// Public wrapper so the standalone inspector example can reuse the exact
/// production normalizer. The implementation lives in `normalize_markdown_field`
/// above; this just exposes it under a distinct name to keep the example's
/// import path clean.
#[doc(hidden)]
pub fn normalize_markdown_field_for_demo(input: &str) -> String {
    normalize_markdown_field(input)
}

fn normalize_lens_markdown_fields(value: &mut Value) {
    if let Some(quick) = value
        .get_mut("quickTakeaway")
        .and_then(Value::as_object_mut)
    {
        if let Some(markdown) = quick
            .get("markdown")
            .and_then(Value::as_str)
            .map(str::to_string)
        {
            quick.insert(
                "markdown".to_string(),
                Value::String(normalize_markdown_field(&markdown)),
            );
        }
    }
    for key in ["overallMarkdown", "explanationMarkdown", "summaryMarkdown"] {
        if let Some(slot) = value.get_mut(key).and_then(Value::as_object_mut) {
            if let Some(text) = slot
                .get("markdown")
                .and_then(Value::as_str)
                .map(str::to_string)
            {
                slot.insert(
                    "markdown".to_string(),
                    Value::String(normalize_markdown_field(&text)),
                );
            }
        }
    }
    if let Some(sections) = value.get_mut("sections").and_then(Value::as_array_mut) {
        for section in sections.iter_mut() {
            if let Some(obj) = section.as_object_mut() {
                if let Some(text) = obj
                    .get("markdown")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                {
                    obj.insert(
                        "markdown".to_string(),
                        Value::String(normalize_markdown_field(&text)),
                    );
                }
            }
        }
    }
}

fn validate_evidence_ids(value: &Value, allowed: &str) -> ProviderResult<()> {
    let ids = value
        .get("evidenceIds")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::invalid("Artifact is missing evidenceIds"))?;
    if ids
        .iter()
        .any(|id| id.as_str().is_none_or(|id| id != allowed))
    {
        return Err(ProviderError::invalid(
            "Artifact referenced evidence outside the local whitelist",
        ));
    }
    Ok(())
}

fn validate_lens_for_protocol(
    mut value: Value,
    protocol: LensProtocol,
    kind: &str,
    anchor: &AnchorFacts,
) -> ProviderResult<Value> {
    if protocol == LensProtocol::V1 {
        return validate_lens(value, kind, anchor);
    }
    crate::lens_contract::validate(&value, kind, &anchor.block_id)
        .map_err(ProviderError::invalid)?;
    value["schemaVersion"] = json!(2);
    value["lensProtocol"] = json!("v2");
    value["kind"] = json!(kind);
    Ok(value)
}

fn validate_lens(mut value: Value, kind: &str, anchor: &AnchorFacts) -> ProviderResult<Value> {
    let quick = value
        .get("quickTakeaway")
        .and_then(Value::as_object)
        .ok_or_else(|| ProviderError::invalid("Lens is missing quickTakeaway"))?;
    if quick
        .get("title")
        .and_then(Value::as_str)
        .is_none_or(|text| text.trim().is_empty())
        || quick
            .get("markdown")
            .and_then(Value::as_str)
            .is_none_or(|text| text.trim().is_empty())
    {
        return Err(ProviderError::invalid("Lens quickTakeaway is incomplete"));
    }
    let sections = value
        .get("sections")
        .and_then(Value::as_array)
        .filter(|sections| !sections.is_empty())
        .ok_or_else(|| ProviderError::invalid("Lens has no explanatory sections"))?;
    let allowed = format!("block:{}", anchor.block_id);
    for section in sections {
        required_string(section, "sectionId", "Lens section")?;
        required_string(section, "title", "Lens section")?;
        required_string(section, "markdown", "Lens section")?;
        validate_evidence_ids(section, &allowed)?;
    }
    let specific = value
        .get(kind)
        .and_then(Value::as_object)
        .ok_or_else(|| ProviderError::invalid(format!("Lens is missing its {kind} payload")))?;
    match kind {
        "formula" => {
            for key in [
                "whatItDoesMarkdown",
                "startHereMarkdown",
                "reconstructedLatex",
            ] {
                required_string(&Value::Object(specific.clone()), key, "Formula Lens")?;
            }
            let symbols = specific
                .get("symbols")
                .and_then(Value::as_array)
                .filter(|symbols| !symbols.is_empty())
                .ok_or_else(|| {
                    ProviderError::invalid(
                        "Formula Lens requires a non-empty, complete symbol table",
                    )
                })?;
            for symbol in symbols {
                required_string(symbol, "symbolLatex", "Formula Lens symbol")?;
                required_string(symbol, "meaningMarkdown", "Formula Lens symbol")?;
                let provenance = required_string(symbol, "provenance", "Formula Lens symbol")?;
                if !matches!(
                    provenance,
                    "paper_defined" | "standard" | "inferred" | "unresolved"
                ) {
                    return Err(ProviderError::invalid(
                        "Formula Lens symbol provenance is invalid",
                    ));
                }
                validate_evidence_ids(symbol, &allowed)?;
            }
        }
        "figure" | "table" => {
            required_string(
                &Value::Object(specific.clone()),
                "overallMarkdown",
                "Visual Lens",
            )?;
        }
        _ => return Err(ProviderError::invalid("Unknown Lens kind")),
    }
    if let Some(object) = value.as_object_mut() {
        object.insert("schemaVersion".to_string(), json!(1));
        object.insert("kind".to_string(), json!(kind));
    }
    Ok(value)
}

fn attach_local_contract(
    value: &mut Value,
    anchor: &AnchorFacts,
    output_language: &str,
    kind: &str,
) {
    if let Some(object) = value.as_object_mut() {
        object.insert("artifactKind".to_string(), json!(kind));
        object.insert("outputLanguage".to_string(), json!(output_language));
        object.insert("sourceBlock".to_string(), block_payload(anchor));
        object.insert(
            "localEvidence".to_string(),
            json!([{
                "evidenceId": format!("block:{}", anchor.block_id),
                "pageNumber": anchor.page_number,
                "blockIds": [anchor.block_id.clone()],
                "bboxNorm1000": anchor.bbox,
                "preview": anchor.text
            }]),
        );
    }
}

fn matching_entries(content: Option<Value>, key: &str, combined: &str) -> Vec<Value> {
    content
        .and_then(|value| value.get("entries").and_then(Value::as_array).cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| {
            std::iter::once(&entry[key])
                .chain(
                    if key == "term" {
                        entry["aliases"]
                            .as_array()
                            .map(Vec::as_slice)
                            .unwrap_or(&[])
                    } else {
                        &[]
                    }
                    .iter(),
                )
                .any(|name| {
                    name.as_str().is_some_and(|name| {
                        !name.trim().is_empty() && combined.contains(&name.to_lowercase())
                    })
                })
        })
        .map(|entry| {
            let fields = if key == "term" {
                vec!["term", "aliases", "definition", "usage"]
            } else {
                vec!["symbol", "meaning", "scope"]
            };
            Value::Object(
                fields
                    .into_iter()
                    .filter_map(|k| entry.get(k).map(|v| (k.into(), v.clone())))
                    .collect(),
            )
        })
        .collect()
}

fn named_schema(name: &str, schema: Value) -> Value {
    json!({"name": name, "strict": true, "schema": schema})
}

fn validate_explanation(content: &Value, block_id: &str) -> ProviderResult<()> {
    // Enforce the existing five-field contract before application metadata is added.
    validate_evidence_ids(content, &format!("block:{block_id}"))?;
    crate::auxiliary_contract::validate(
        content,
        &explanation_schema(block_id)["schema"],
        "explanation",
    )
    .map_err(ProviderError::invalid)
}

fn explanation_schema(block_id: &str) -> Value {
    named_schema(
        "block_explanation",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["title", "explanation", "keyPoints", "paperConnection", "evidenceIds"],
            "properties": {
                "title": {"type": "string", "minLength": 1},
                "explanation": {"type": "string", "minLength": 1},
                "keyPoints": {"type": "array", "items": {"type": "string"}},
                "paperConnection": {"type": "string"},
                "evidenceIds": {
                    "type": "array",
                    "items": {"type": "string", "enum": [format!("block:{block_id}")]}
                }
            }
        }),
    )
}

fn section_schema(block_id: &str) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["sectionId", "title", "markdown", "roleTags", "evidenceIds"],
        "properties": {
            "sectionId": {"type": "string", "minLength": 1},
            "title": {"type": "string", "minLength": 1},
            "markdown": {"type": "string", "minLength": 1},
            "roleTags": {"type": "array", "items": {"type": "string"}},
            "evidenceIds": {
                "type": "array",
                "items": {"type": "string", "enum": [format!("block:{block_id}")]}
            }
        }
    })
}
fn lens_schema(kind: &str, block_id: &str) -> Value {
    let common = json!({
        "quickTakeaway": {
            "type": "object",
            "additionalProperties": false,
            "required": ["title", "markdown"],
            "properties": {
                "title": {"type": "string", "minLength": 1},
                "markdown": {"type": "string", "minLength": 1}
            }
        },
        "sections": {"type": "array", "minItems": 1, "items": section_schema(block_id)},
        "suggestedQuestions": {
            "type": "array",
            "maxItems": 3,
            "items": {"type": "string"}
        }
    });
    let mut properties = common.as_object().cloned().unwrap_or_default();
    let specific = match kind {
        "formula" => json!({
            "type": "object",
            "additionalProperties": false,
            "required": [
                "whatItDoesMarkdown", "startHereMarkdown", "reconstructedLatex",
                "symbols", "relatedFormulas", "derivation"
            ],
            "properties": {
                "whatItDoesMarkdown": {"type": "string", "minLength": 1},
                "startHereMarkdown": {"type": "string", "minLength": 1},
                "reconstructedLatex": {"type": "string", "minLength": 1},
                "symbols": {"type": "array", "minItems": 1, "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["symbolLatex", "meaningMarkdown", "provenance", "evidenceIds"],
                    "properties": {
                        "symbolLatex": {"type": "string", "minLength": 1},
                        "meaningMarkdown": {"type": "string", "minLength": 1},
                        "provenance": {
                            "type": "string",
                            "enum": ["paper_defined", "standard", "inferred", "unresolved"]
                        },
                        "evidenceIds": {
                            "type": "array",
                            "items": {"type": "string", "enum": [format!("block:{block_id}")]}
                        }
                    }
                }},
                "relatedFormulas": {"type": "array", "items": {"type": "object"}},
                "derivation": {"type": "array", "items": {"type": "object"}}
            }
        }),
        "figure" => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["overallMarkdown", "roleTags", "panels", "hotspots"],
            "properties": {
                "overallMarkdown": {"type": "string", "minLength": 1},
                "roleTags": {"type": "array", "items": {"type": "string"}},
                "panels": {"type": "array", "items": {"type": "object"}},
                "hotspots": {"type": "array", "items": {"type": "object"}}
            }
        }),
        "table" => json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["overallMarkdown", "cellLinks", "calculations"],
            "properties": {
                "overallMarkdown": {"type": "string", "minLength": 1},
                "cellLinks": {"type": "array", "items": {"type": "object"}},
                "calculations": {"type": "array", "items": {"type": "object"}}
            }
        }),
        _ => Value::Null,
    };
    properties.insert(kind.to_string(), specific);
    named_schema(
        &format!("{kind}_lens"),
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["quickTakeaway", "sections", "suggestedQuestions", kind],
            "properties": properties
        }),
    )
}

fn explanation_system_instruction(kind: crate::library_paths::DocumentKind) -> String {
    crate::prompt_settings::default_text_for_kind(
        crate::prompt_settings::PromptSlotId::Explanation,
        kind,
    )
}

fn paper_root_system_instruction() -> String {
    include_str!("../prompts/paper-root.md").to_string()
}

fn lens_system_instruction(kind: &str, output_language: &str) -> String {
    format!(
        "Create a {kind} Lens in {output_language} using the complete PDF context and the exact OCR anchor. Keep it isolated from Discussion. Provide a quick takeaway and coherent sections. Formula Lens must include every non-trivial symbol with provenance. Figure/Table Lens must include an overall explanation. Evidence IDs must come only from the supplied whitelist. Return only the strict schema."
    )
}

fn lens_repair_system_instruction(kind: &str, output_language: &str) -> String {
    format!(
        "Repair the previous {kind} Lens into the exact strict schema in {output_language}. Correct only schema, completeness, and evidence-whitelist violations. Do not add ungrounded claims. This is the single allowed repair attempt."
    )
}

fn lens_qa_system_instruction(kind: crate::library_paths::DocumentKind) -> String {
    crate::prompt_settings::default_text_for_kind(
        crate::prompt_settings::PromptSlotId::LensQa,
        kind,
    )
}

fn lens_qa_input(question: &str, lens: &Value, evidence_id: &str, output_language: &str) -> String {
    json!({
        "task": "回应当前追问，依据实际可见材料推进理解。已有 Lens 是待核对的生成内容；本分支有依据的纠正不能被重复传入的旧稿覆盖。只返回本次两字段 schema。",
        "question": question,
        "outputLanguage": output_language,
        "lens": lens,
        "allowedEvidenceIds": [evidence_id]
    }).to_string()
}

fn lens_qa_schema(block_id: &str) -> Value {
    named_schema(
        "lens_follow_up",
        json!({
            "type":"object", "additionalProperties":false,
            "required":["answerMarkdown","evidenceIds"],
            "properties":{
                "answerMarkdown":{"type":"string","minLength":1},
                "evidenceIds":{"type":"array","items":{"type":"string","enum":[format!("block:{block_id}")]}}
            }
        }),
    )
}
fn validate_lens_qa(content: &Value, block_id: &str) -> ProviderResult<()> {
    crate::auxiliary_contract::validate(content, &lens_qa_schema(block_id)["schema"], "")
        .map_err(ProviderError::invalid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v2_workspace::WorkspaceModule;
    use tempfile::tempdir;

    #[test]
    fn opens_the_canonical_v2_workspace_database() {
        let workspace_root = tempdir().expect("workspace");
        let workspace = WorkspaceModule::new();
        let projection = workspace
            .open(workspace_root.path())
            .expect("initialize V2 workspace");

        let module =
            ReadingArtifactModule::open(workspace_root.path()).expect("reading artifact module");

        assert_eq!(module.database_path, projection.database_path);
        assert!(module
            .connect()
            .expect("canonical database")
            .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row
                .get::<_, i64>(0))
            .is_ok());
    }
    fn figure_anchor() -> AnchorFacts {
        AnchorFacts {
            document: DocumentFacts {
                paper_id: "paper".to_string(),
                revision_id: "revision".to_string(),
                revision_sha256: "sha".to_string(),
                title: "Paper".to_string(),
                pdf_path: PathBuf::from("paper.pdf"),
            },
            ocr_revision_id: "ocr".to_string(),
            block_id: "block-1".to_string(),
            block_type: "figure".to_string(),
            text: "Figure 1".to_string(),
            content_digest: "digest".to_string(),
            page_number: 1,
            block_index: 0,
            bbox: [0, 0, 1000, 1000],
        }
    }

    #[test]
    fn normalize_markdown_decodes_literal_escapes_and_collapses_spaces() {
        // The model occasionally emits JSON-style escape sequences inside markdown
        // strings (e.g. a literal "\\n" instead of a newline). The normalizer
        // must turn "\\n" into a real newline while keeping already-decoded
        // newlines untouched, and collapse runs of spaces.
        let normalized = normalize_markdown_field(
            "Quick\\n takeaway   with literal  \\n1. item and  extra spaces.",
        );
        assert!(normalized.contains("Quick\n takeaway") || normalized.contains("Quick\ntakeaway"));
        assert!(normalized.contains("\n1. item"));
        assert!(!normalized.contains("  "));
        assert!(!normalized.starts_with(' '));
        assert!(!normalized.ends_with(' '));
    }

    #[test]
    fn normalize_markdown_preserves_latex_commands() {
        let normalized = normalize_markdown_field(r"use $\nu$ and $\text{Target}$ and $\rho$");
        assert!(normalized.contains(r"$\nu$"), "ate \\nu: {normalized}");
        assert!(
            normalized.contains(r"\text{Target}"),
            "ate \\text: {normalized}"
        );
        assert!(normalized.contains(r"$\rho$"), "ate \\rho: {normalized}");
    }

    #[test]
    fn normalize_markdown_does_not_split_figure_prose() {
        let normalized = normalize_markdown_field("See Figure 1. The results confirm the claim.");
        assert!(
            !normalized.contains("\n1. "),
            "split Figure 1.: {normalized}"
        );
        let prose =
            normalize_markdown_field("described in 1. Introduction and 2. Methods of the paper.");
        assert!(
            !prose.contains("\n1. ") && !prose.contains("\n2. "),
            "split prose numbers: {prose}"
        );
    }

    #[test]
    fn normalize_markdown_splits_packed_numbered_items() {
        let packed = "1. 监督学习中反向传播利用链式法则计算梯度；2. 强化学习中多巴胺神经元的相位放电定量表征奖赏预测误差（RPE）；3. TD学习通过Bellman方程解释多巴胺响应从非条件刺激向条件刺激的时间回溯迁移。";
        let normalized = normalize_markdown_field(packed);
        assert!(normalized.contains("1. 监督"), "missing 1.: {normalized}");
        assert!(
            normalized.contains("\n2. 强化"),
            "missing newline before 2.: {normalized}"
        );
        assert!(
            normalized.contains("\n3. TD学习"),
            "missing newline before 3.: {normalized}"
        );
    }

    #[test]
    fn normalize_markdown_collapses_inline_bold_padding() {
        let normalized = normalize_markdown_field("**输入与预处理 **：接收并转化");
        assert_eq!(
            normalized, "**输入与预处理**：接收并转化",
            "got: {normalized}"
        );
        assert_eq!(
            normalize_markdown_field("执行**监督学习**：最小化误差"),
            "执行**监督学习**：最小化误差"
        );
    }

    #[test]
    fn normalize_markdown_balances_unpaired_bold() {
        let input = "前置段落结束。**小脑回路作为生物学有监督学习系统";
        let normalized = normalize_markdown_field(input);
        let opens = normalized.matches("**").count();
        assert_eq!(
            opens % 2,
            0,
            "expected balanced ** markers, got: {normalized}"
        );
        assert!(normalized.contains("**小脑回路作为生物学有监督学习系统**"));
    }

    #[test]
    fn normalize_markdown_splits_packed_bullets() {
        let input = "要点：1. 第一项；2. 第二项；3. 第三项。**总结**。";
        let normalized = normalize_markdown_field(input);
        let lines: Vec<&str> = normalized.lines().collect();
        assert!(
            lines.iter().any(|l| l.trim_start().starts_with("1.")),
            "missing 1. in {normalized}"
        );
        assert!(
            lines.iter().any(|l| l.trim_start().starts_with("2.")),
            "missing 2. in {normalized}"
        );
        assert!(
            lines.iter().any(|l| l.trim_start().starts_with("3.")),
            "missing 3. in {normalized}"
        );
        assert!(
            normalized.contains("**总结**"),
            "missing bold: {normalized}"
        );
    }

    #[test]
    fn normalize_markdown_handles_glued_chinese_periods() {
        // Chinese `。` and `；` encoded as Latin placeholders (`?`, `;`) by the
        // upstream gateway still need to be treated as list separators.
        let input = "1. 第一项?2. 第二项;3. 第三项。**结束**";
        let normalized = normalize_markdown_field(input);
        assert!(normalized.contains("1. 第一项"), "got: {normalized}");
        assert!(
            normalized.contains("\n2. 第二项"),
            "missing 2.: {normalized}"
        );
        assert!(
            normalized.contains("\n3. 第三项"),
            "missing 3.: {normalized}"
        );
    }

    #[test]
    fn normalize_markdown_splits_after_stray_corrupt_terminator() {
        // The real lens database had `基底�?2. **自适应处理**` glued onto a
        // single line. The `�?` is a corrupted Chinese comma from the JSON
        // gateway, but `2. ` should still start a new list item.
        let input = "映射：1. **输入**：感觉信息由苔藓纤维传入。基底�?2. **处理**：颗粒细胞。";
        let normalized = normalize_markdown_field(input);
        assert!(
            normalized.contains("\n2. **处理**"),
            "missing newline before 2.: {normalized}"
        );
    }

    #[test]
    fn normalize_markdown_demo_against_live_db() {
        let path = std::env::var("READ_DESKTOP_DB").unwrap_or_else(|_| {
            "D:/skywalker/read_desktop/.read-desktop/workspace.sqlite3".to_string()
        });
        if !std::path::Path::new(&path).exists() {
            return; // skip when the live DB isn't present
        }
        let conn = rusqlite::Connection::open(&path).expect("open sqlite");
        let mut stmt = conn
            .prepare(
                "SELECT content_json FROM artifacts \
                 WHERE kind LIKE 'lens_%' ORDER BY created_at DESC LIMIT 3",
            )
            .expect("prepare");
        let mut rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .expect("query");
        // Just verify that every live section with ** keeps it balanced after
        // normalization. (Packed list splitting is covered by the synthetic
        // test cases; the live DB today may or may not contain packed items.)
        let mut checked = 0usize;
        while let Some(Ok(content)) = rows.next() {
            let value: serde_json::Value = match serde_json::from_str(&content) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let sections = value
                .get("sections")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for s in sections {
                let Some(obj) = s.as_object() else {
                    continue;
                };
                let raw = obj
                    .get("markdown")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                if raw.is_empty() {
                    continue;
                }
                let normalized = normalize_markdown_field(raw);
                let opens = normalized.matches("**").count();
                assert_eq!(
                    opens % 2,
                    0,
                    "unbalanced ** in normalized section: {normalized}"
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "expected at least one live section to inspect");
    }

    #[test]
    fn normalize_lens_fields_walks_quick_sections_and_panels() {
        let mut value = json!({
            "quickTakeaway": {
                "title": "Takeaway",
                "markdown": "alpha   beta  \\n1. line"
            },
            "overallMarkdown": {
                "markdown": "overall  with   gaps"
            },
            "explanationMarkdown": {
                "markdown": "explanation\nbreak"
            },
            "summaryMarkdown": {
                "markdown": "summary"
            },
            "sections": [{
                "sectionId": "s1",
                "title": "Section",
                "markdown": "section   body  \\n2. next",
                "roleTags": [],
                "evidenceIds": [format!("block:{}", figure_anchor().block_id)]
            }]
        });
        normalize_lens_markdown_fields(&mut value);
        let quick = value["quickTakeaway"]["markdown"].as_str().unwrap();
        assert!(!quick.contains("  "));
        assert!(quick.contains("\n1."));
        let section = value["sections"][0]["markdown"].as_str().unwrap();
        assert!(!section.contains("  "));
        assert!(section.contains("\n2."));
        let overall = value["overallMarkdown"]["markdown"].as_str().unwrap();
        assert!(!overall.contains("  "));
        assert_eq!(
            value["summaryMarkdown"]["markdown"].as_str().unwrap(),
            "summary"
        );
    }

    #[test]
    fn lens_validation_rejects_unknown_evidence_ids() {
        let error = validate_lens(
            json!({
                "quickTakeaway": {"title": "Takeaway", "markdown": "Meaning"},
                "sections": [{
                    "sectionId": "s1",
                    "title": "Section",
                    "markdown": "Explanation",
                    "roleTags": [],
                    "evidenceIds": ["provider:evidence"]
                }],
                "suggestedQuestions": [],
                "figure": {
                    "overallMarkdown": "Overall",
                    "roleTags": [],
                    "panels": [],
                    "hotspots": []
                }
            }),
            "figure",
            &figure_anchor(),
        )
        .expect_err("unknown evidence");
        assert!(error.message.contains("whitelist"));
    }

    #[test]
    fn explanation_validation_enforces_the_existing_schema_without_rewriting_content() {
        let valid = json!({
            "title": "条件与结论", "explanation": "先明确 $\\nu > 0$。\n\n1. 保留条件。\n2. 说明推导。",
            "keyPoints": ["结论依赖这个条件。"], "paperConnection": "", "evidenceIds": ["block:block-1"]
        });
        validate_explanation(&valid, "block-1").unwrap();
        let mut limited = valid.clone();
        limited["title"] = json!("当前选段暂无法可靠解释");
        limited["explanation"] = json!("条件所在位置无法从 PDF 和选段可靠辨认。");
        limited["keyPoints"] = json!([]);
        limited["evidenceIds"] = json!([]);
        validate_explanation(&limited, "block-1").unwrap();
        for (key, bad) in [
            ("title", json!("  ")),
            ("explanation", json!("\n")),
            ("keyPoints", Value::Null),
            ("keyPoints", json!("不是数组")),
            ("keyPoints", json!([1])),
            ("paperConnection", Value::Null),
            ("evidenceIds", json!(["block:invented"])),
            ("status", json!("ok")),
        ] {
            let mut malformed = valid.clone();
            malformed[key] = bad;
            assert!(
                validate_explanation(&malformed, "block-1").is_err(),
                "accepted {malformed}"
            );
        }
        for key in [
            "title",
            "explanation",
            "keyPoints",
            "paperConnection",
            "evidenceIds",
        ] {
            let mut malformed = valid.clone();
            malformed.as_object_mut().unwrap().remove(key);
            assert!(
                validate_explanation(&malformed, "block-1").is_err(),
                "accepted missing {key}"
            );
        }
    }

    struct ExplanationTestPort {
        response: StdMutex<Value>,
        calls: StdMutex<Vec<PaperInteractionRequest>>,
    }
    #[async_trait::async_trait]
    impl PaperModelPort for ExplanationTestPort {
        fn capabilities(&self, model: &str) -> crate::provider_ports::PaperModelCapabilities {
            NeverCalledPaperPort.capabilities(model)
        }
        async fn interact(
            &self,
            request: PaperInteractionRequest,
        ) -> ProviderResult<crate::provider_ports::PaperInteractionOutcome> {
            let mut calls = self.calls.lock().unwrap();
            let mut response = root_outcome(
                "gemini",
                &request.model,
                &request.context_epoch,
                &format!("explanation-{}", calls.len()),
            );
            if request.kind != PaperInteractionKind::Root {
                response.text = self.response.lock().unwrap().to_string();
            }
            calls.push(request);
            Ok(response)
        }
        async fn interact_text(
            &self,
            _request: TextInteractionRequest,
        ) -> ProviderResult<crate::provider_ports::TextInteractionOutcome> {
            panic!("explanation must use the full-PDF branch");
        }
        async fn delete_remote(
            &self,
            _resource: &crate::provider_ports::RemoteResource,
        ) -> ProviderResult<()> {
            panic!("unexpected cleanup");
        }
    }

    #[tokio::test]
    async fn explanation_generation_uses_complete_kind_default_and_clean_source_siblings() {
        for kind in [
            crate::library_paths::DocumentKind::Paper,
            crate::library_paths::DocumentKind::Textbook,
        ] {
            let workspace = tempdir().unwrap();
            WorkspaceModule::new().open(workspace.path()).unwrap();
            let module = ReadingArtifactModule::open(workspace.path()).unwrap();
            let connection = module.connect().unwrap();
            insert_revision(&connection);
            add_route_context_schema(&connection);
            let relative = if kind == crate::library_paths::DocumentKind::Paper {
                "Papers/Inbox/paper.pdf"
            } else {
                "Textbooks/Inbox/paper.pdf"
            };
            connection
                .execute(
                    "UPDATE papers SET relative_path=?1 WHERE id='paper-1'",
                    [relative],
                )
                .unwrap();
            connection
                .execute(
                    "UPDATE document_revisions SET source_relative_path=?1 WHERE id='rev-1'",
                    [relative],
                )
                .unwrap();
            connection.execute_batch(r#"
                INSERT INTO ocr_revisions(id,revision_id,status,provider,model,created_at)
                VALUES ('ocr-1','rev-1','ready','mistral','ocr','2026-09-09');
                INSERT INTO ocr_pages(id,ocr_revision_id,page_number) VALUES ('page-1','ocr-1',1);
                INSERT INTO ocr_blocks(id,ocr_page_id,block_index,block_type,text_content,content_digest,x0,y0,x1,y1)
                VALUES ('block-1','page-1',0,'text','The result holds under the given condition.','block-digest',0,0,100,100);
            "#).unwrap();
            let pdf = module.absolute_pdf(Path::new(relative));
            fs::create_dir_all(pdf.parent().unwrap()).unwrap();
            fs::write(&pdf, b"%PDF-fixture").unwrap();
            let body = "先明确 $\\nu > 0$。\n\n1. 保留条件。\n2. 说明推导。";
            let port = ExplanationTestPort {
                response: StdMutex::new(
                    json!({"title":"条件与结论","explanation":body,"keyPoints":[],"paperConnection":"","evidenceIds":["block:block-1"]}),
                ),
                calls: Default::default(),
            };
            let mut request: GenerateReadingArtifactRequest = serde_json::from_value(json!({
                "revisionId":"rev-1","ocrRevisionId":"ocr-1","blockId":"block-1","action":"explain",
                "outputLanguage":"zh-CN","paperModel":"paper-model","translationModel":"translation-model",
                "readerContext":"FROZEN_READER_CONTEXT"
            })).unwrap();
            let outcome = module
                .generate_for_route(&port, "route-a", request.clone())
                .await
                .unwrap();
            assert_eq!(outcome.artifact.content["explanation"], body);
            assert_eq!(outcome.artifact.status, "ready");
            assert!(!outcome.repaired);
            {
                let calls = port.calls.lock().unwrap();
                assert_eq!(calls.len(), 2);
                assert_eq!(calls[0].kind, PaperInteractionKind::Root);
                assert!(!calls[0].user_input.contains("FROZEN_READER_CONTEXT"));
                assert_eq!(calls[1].pdf_path, pdf);
                assert_eq!(
                    calls[1].system_instruction,
                    explanation_system_instruction(kind)
                );
                assert_eq!(
                    calls[1].previous_interaction_id.as_deref(),
                    Some("node-explanation-0")
                );
                assert!(calls[1].user_input.contains("FROZEN_READER_CONTEXT"));
                assert!(calls[1].user_input.contains("outputLanguage"));
                assert!(!calls[1].user_input.contains("localOrientationPack"));
                assert!(!calls[1].user_input.contains("glossary"));
                assert_eq!(
                    calls[1].response_schema.as_ref().unwrap()["schema"]["required"]
                        .as_array()
                        .unwrap()
                        .len(),
                    5
                );
            }
            let frozen = "用户已冻结的旧解释提示词";
            request.system_instruction = Some(frozen.into());
            let again = module
                .generate_for_route(&port, "route-a", request.clone())
                .await
                .unwrap();
            assert_eq!(again.artifact.content["explanation"], body);
            {
                let calls = port.calls.lock().unwrap();
                assert_eq!(calls.len(), 3);
                assert_eq!(calls[2].system_instruction, frozen);
                assert_eq!(
                    calls[2].previous_interaction_id,
                    calls[1].previous_interaction_id
                );
                assert!(!calls[2].user_input.contains(body));
            }
            port.response.lock().unwrap()["keyPoints"] = Value::Null;
            assert!(module
                .generate_for_route(&port, "route-a", request)
                .await
                .is_err());
            assert_eq!(port.calls.lock().unwrap().len(), 4); // no repair call
            assert_eq!(
                connection
                    .query_row(
                        "SELECT COUNT(*) FROM artifacts WHERE kind='explanation'",
                        [],
                        |r| r.get::<_, i64>(0)
                    )
                    .unwrap(),
                2
            );
            assert_eq!(
                connection
                    .query_row("SELECT COUNT(*) FROM context_roots", [], |r| r
                        .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        }
    }

    struct LensTestPort {
        outputs: StdMutex<std::collections::VecDeque<Value>>,
        calls: StdMutex<Vec<PaperInteractionRequest>>,
    }
    #[async_trait::async_trait]
    impl PaperModelPort for LensTestPort {
        fn capabilities(&self, model: &str) -> crate::provider_ports::PaperModelCapabilities {
            NeverCalledPaperPort.capabilities(model)
        }
        async fn interact(
            &self,
            request: PaperInteractionRequest,
        ) -> ProviderResult<crate::provider_ports::PaperInteractionOutcome> {
            let mut calls = self.calls.lock().unwrap();
            let mut outcome = root_outcome(
                "gemini",
                &request.model,
                &request.context_epoch,
                &format!("lens-{}", calls.len()),
            );
            if request.kind != PaperInteractionKind::Root {
                outcome.text = self
                    .outputs
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("no extra paid call")
                    .to_string();
            }
            calls.push(request);
            Ok(outcome)
        }
        async fn interact_text(
            &self,
            _: TextInteractionRequest,
        ) -> ProviderResult<crate::provider_ports::TextInteractionOutcome> {
            panic!("Lens requires PDF context")
        }
        async fn delete_remote(
            &self,
            _: &crate::provider_ports::RemoteResource,
        ) -> ProviderResult<()> {
            panic!("unexpected cleanup")
        }
    }
    #[tokio::test]
    async fn lens_v2_generation_and_single_repair_keep_frozen_schema_sources_and_usage() {
        for document_kind in [
            crate::library_paths::DocumentKind::Paper,
            crate::library_paths::DocumentKind::Textbook,
        ] {
            for kind in ["formula", "figure", "table"] {
                for mode in ["direct", "repair", "failure", "partial", "unavailable"] {
                    let workspace = tempdir().unwrap();
                    WorkspaceModule::new().open(workspace.path()).unwrap();
                    let module = ReadingArtifactModule::open(workspace.path()).unwrap();
                    let connection = module.connect().unwrap();
                    insert_revision(&connection);
                    add_route_context_schema(&connection);
                    connection.execute_batch(r#"
                    INSERT INTO ocr_revisions(id,revision_id,status,provider,model,created_at) VALUES('ocr-1','rev-1','ready','mistral','ocr','2026-09-09');
                    INSERT INTO ocr_pages(id,ocr_revision_id,page_number) VALUES('page-1','ocr-1',1);
                    INSERT INTO ocr_blocks(id,ocr_page_id,block_index,block_type,text_content,content_digest,x0,y0,x1,y1) VALUES('block-1','page-1',0,'formula','Original object','digest',0,0,100,100);
                "#).unwrap();
                    connection
                        .execute(
                            "UPDATE ocr_blocks SET block_type=?1 WHERE id='block-1'",
                            [kind],
                        )
                        .unwrap();
                    let pdf = module.absolute_pdf(Path::new("Inbox/paper.pdf"));
                    fs::create_dir_all(pdf.parent().unwrap()).unwrap();
                    fs::write(pdf, b"%PDF-fixture").unwrap();
                    let valid = crate::lens_contract::fixture(
                        kind,
                        if mode == "unavailable" {
                            "unavailable"
                        } else if mode == "partial" {
                            "partial"
                        } else {
                            "complete"
                        },
                    );
                    let mut invalid = valid.clone();
                    invalid["invented"] = json!("schema error");
                    let outputs = match mode {
                        "repair" => vec![invalid.clone(), valid.clone()],
                        "failure" => vec![invalid.clone(), invalid.clone()],
                        _ => vec![valid.clone()],
                    };
                    let port = LensTestPort {
                        outputs: StdMutex::new(outputs.into()),
                        calls: Default::default(),
                    };
                    let request:GenerateReadingArtifactRequest=serde_json::from_value(json!({
                    "revisionId":"rev-1","ocrRevisionId":"ocr-1","blockId":"block-1","action":"lens",
                    "paperModel":"paper-model","translationModel":"translation-model","lensProtocol":"v2","documentKind":document_kind.as_str(),
                    "displayCropDataUrl":"data:image/png;base64,aW1hZ2U=","modelCropDataUrl":"data:image/png;base64,aW1hZ2U=",
                    "readerContext":"FROZEN_READER_CONTEXT"
                })).unwrap();
                    let result = module.generate_for_route(&port, "route-a", request).await;
                    let needs_repair = matches!(mode, "repair" | "failure");
                    let calls = port.calls.lock().unwrap();
                    assert_eq!(calls.len(), if needs_repair { 3 } else { 2 });
                    assert!(!calls[0].user_input.contains("FROZEN_READER_CONTEXT"));
                    assert!(calls[1].user_input.contains("FROZEN_READER_CONTEXT"));
                    assert_eq!(calls[1].inline_images.len(), 1);
                    assert_eq!(
                        calls[1].previous_interaction_id.as_deref(),
                        Some("node-lens-0")
                    );
                    assert_eq!(
                        calls[1].response_schema.as_ref().unwrap()["name"],
                        format!("{kind}_lens_v2")
                    );
                    assert_eq!(
                        calls[1].system_instruction,
                        LensProtocol::V2.document_prompt(document_kind, kind, false, "zh-CN")
                    );
                    if needs_repair {
                        assert_eq!(calls[2].response_schema, calls[1].response_schema);
                        assert_eq!(
                            calls[2].previous_interaction_id.as_deref(),
                            Some("node-lens-1")
                        );
                        assert!(calls[2].inline_images.is_empty());
                        assert!(!calls[2].user_input.contains("FROZEN_READER_CONTEXT"));
                        assert_eq!(
                            calls[2].system_instruction,
                            LensProtocol::V2.document_prompt(document_kind, kind, true, "zh-CN")
                        );
                        let repair_input: Value =
                            serde_json::from_str(&calls[2].user_input).unwrap();
                        assert_eq!(repair_input["invalidOutput"], invalid.to_string());
                        assert_eq!(repair_input["allowedEvidenceIds"], json!(["block:block-1"]));
                    }
                    if mode == "failure" {
                        assert!(result.unwrap_err().message.contains("after one repair"));
                        assert_eq!(
                            connection
                                .query_row(
                                    "SELECT COUNT(*) FROM artifacts WHERE kind LIKE 'lens_%'",
                                    [],
                                    |r| r.get::<_, i64>(0)
                                )
                                .unwrap(),
                            0
                        );
                    } else {
                        let result = result.unwrap();
                        assert_eq!(result.repaired, mode == "repair");
                        // The source root records its receipt separately from the Lens outcome.
                        assert_eq!(result.receipts.len(), calls.len() - 1);
                        for key in valid.as_object().unwrap().keys() {
                            assert_eq!(result.artifact.content[key], valid[key]);
                        }
                        assert_eq!(result.artifact.content["schemaVersion"], 2);
                        assert_eq!(result.artifact.content["lensProtocol"], "v2");
                        assert_eq!(result.artifact.status, "ready");
                    }
                    assert_eq!(
                        connection
                            .query_row("SELECT COUNT(*) FROM usage_receipts", [], |r| r
                                .get::<_, i64>(0))
                            .unwrap(),
                        calls.len() as i64
                    );
                }
            }
        }
        let anchor = figure_anchor();
        let legacy = json!({"quickTakeaway":{"title":"legacy","markdown":"Legacy explanation"},"sections":[{"sectionId":"one","title":"T","markdown":"M","evidenceIds":[]}],"figure":{"overallMarkdown":"Original","roleTags":[],"panels":[],"hotspots":[]}});
        assert_eq!(
            validate_lens_for_protocol(legacy, LensProtocol::V1, "figure", &anchor).unwrap()
                ["schemaVersion"],
            1
        );
    }

    #[test]
    fn lens_qa_contract_rejects_extra_fields_and_preserves_math_newlines() {
        let body = "原先说法需要修正。\n\n1. 保留 $\\nu$ 的条件。\n2. 下面给出一个自拟例子。";
        let good = json!({"answerMarkdown":body,"evidenceIds":["block:block-1"]});
        let before = good.clone();
        validate_lens_qa(&good, "block-1").unwrap();
        assert_eq!(good, before);
        let wrapped = format!("```json\n{}\n```", good);
        let parsed = parse_json_object(&wrapped, "Lens follow-up").unwrap();
        validate_lens_qa(&parsed, "block-1").unwrap();
        assert_eq!(parsed["answerMarkdown"], body);
        let mut no_citation = good.clone();
        no_citation["evidenceIds"] = json!([]);
        validate_lens_qa(&no_citation, "block-1").unwrap();
        for (key, value) in [
            ("answerMarkdown", json!(" ")),
            ("answerMarkdown", json!(null)),
            ("evidenceIds", json!(["block:other"])),
            ("evidenceIds", json!([1])),
            ("evidenceIds", json!(null)),
            ("status", json!("complete")),
        ] {
            let mut bad = good.clone();
            bad[key] = value;
            assert!(validate_lens_qa(&bad, "block-1").is_err(), "accepted {key}");
        }
        for key in ["answerMarkdown", "evidenceIds"] {
            let mut bad = good.clone();
            bad.as_object_mut().unwrap().remove(key);
            assert!(validate_lens_qa(&bad, "block-1").is_err());
        }
    }
    #[tokio::test]
    async fn lens_qa_continues_exact_branch_uses_kind_default_and_keeps_original_lens() {
        for kind in [
            crate::library_paths::DocumentKind::Paper,
            crate::library_paths::DocumentKind::Textbook,
        ] {
            for initial_state in ["legacy", "unavailable"] {
                let workspace = tempdir().unwrap();
                WorkspaceModule::new().open(workspace.path()).unwrap();
                let module = ReadingArtifactModule::open(workspace.path()).unwrap();
                let connection = module.connect().unwrap();
                insert_revision(&connection);
                add_route_context_schema(&connection);
                let relative = if kind == crate::library_paths::DocumentKind::Paper {
                    "Papers/Inbox/paper.pdf"
                } else {
                    "Textbooks/Inbox/paper.pdf"
                };
                connection
                    .execute(
                        "UPDATE papers SET relative_path=?1 WHERE id='paper-1'",
                        [relative],
                    )
                    .unwrap();
                connection
                    .execute(
                        "UPDATE document_revisions SET source_relative_path=?1 WHERE id='rev-1'",
                        [relative],
                    )
                    .unwrap();
                connection.execute_batch(r#"
                    INSERT INTO ocr_revisions(id,revision_id,status,provider,model,created_at) VALUES('ocr-1','rev-1','ready','mistral','ocr','2026-09-09');
                    INSERT INTO ocr_pages(id,ocr_revision_id,page_number) VALUES('page-1','ocr-1',1);
                    INSERT INTO ocr_blocks(id,ocr_page_id,block_index,block_type,text_content,content_digest,x0,y0,x1,y1) VALUES('block-1','page-1',0,'figure','Figure 1','digest',0,0,100,100);
                "#).unwrap();
                let pdf = module.absolute_pdf(Path::new(relative));
                fs::create_dir_all(pdf.parent().unwrap()).unwrap();
                fs::write(pdf, b"%PDF-fixture").unwrap();
                let initial = if initial_state == "legacy" {
                    json!({
                        "quickTakeaway":{"title":"旧图解","markdown":"待核对的旧解释"},
                        "sections":[{"sectionId":"old","title":"旧解释","markdown":"待核对","roleTags":[],"evidenceIds":[]}],
                        "suggestedQuestions":[],"figure":{"overallMarkdown":"旧图解", "roleTags":[],"panels":[],"hotspots":[]}
                    })
                } else {
                    crate::lens_contract::fixture("figure", "unavailable")
                };
                let correction =
                    "上一版对符号的说法需要修正。\n\n1. 先理解 $\\nu$。\n2. 下面用自拟例子解释。";
                let second = "承接前面的修正，继续解释这个一般概念。";
                let port = LensTestPort {
                    outputs: StdMutex::new(
                        vec![
                            initial,
                            json!({"answerMarkdown":correction,"evidenceIds":[]}),
                            json!({"answerMarkdown":second,"evidenceIds":["block:block-1"]}),
                            json!({"answerMarkdown":"独立的问题回答。","evidenceIds":[]}),
                            json!({"answerMarkdown":"不可发布","evidenceIds":["block:invented"]}),
                        ]
                        .into(),
                    ),
                    calls: Default::default(),
                };
                let generation:GenerateReadingArtifactRequest=serde_json::from_value(json!({
                    "revisionId":"rev-1","ocrRevisionId":"ocr-1","blockId":"block-1","action":"lens",
                    "paperModel":"paper-model","translationModel":"translation-model","lensProtocol":if initial_state=="legacy" {"v1"} else {"v2"},
                    "displayCropDataUrl":"data:image/png;base64,aW1hZ2U=","modelCropDataUrl":"data:image/png;base64,aW1hZ2U="
                })).unwrap();
                let lens = module
                    .generate_for_route(&port, "route-a", generation)
                    .await
                    .unwrap()
                    .artifact;
                let snapshot = lens.content.clone();
                let make_request =
                    |question: &str, parent_id: Option<String>, reader: &str| AskLensRequest {
                        output_language: Some("en".into()),
                        lens_artifact_id: lens.id.clone(),
                        parent_id,
                        question: question.into(),
                        system_instruction: None,
                        provider: "gemini".into(),
                        reader_context: Some(reader.into()),
                    };
                let first = module
                    .ask_lens_for_route(
                        &port,
                        "route-a",
                        make_request("这个概念是什么意思？", None, "READER_FIRST"),
                    )
                    .await
                    .unwrap();
                assert_eq!(first.assistant.content, correction);
                let continuation = module
                    .ask_lens_for_route(
                        &port,
                        "route-a",
                        make_request(
                            "继续，用一个具体例子。",
                            Some(first.assistant.id.clone()),
                            "READER_SECOND",
                        ),
                    )
                    .await
                    .unwrap();
                assert_eq!(continuation.assistant.content, second);
                assert_eq!(
                    continuation.user.parent_id.as_deref(),
                    Some(first.assistant.id.as_str())
                );
                let mut branch = make_request("另一个问题。", None, "READER_BRANCH");
                branch.system_instruction = Some("CUSTOM_CURRENT_QA_PROMPT".into());
                module
                    .ask_lens_for_route(&port, "route-a", branch)
                    .await
                    .unwrap();
                let invalid_parent = module
                    .ask_lens_for_route(
                        &port,
                        "route-a",
                        make_request("不能续接用户消息", Some(first.user.id.clone()), "READER"),
                    )
                    .await
                    .unwrap_err();
                assert!(invalid_parent
                    .message
                    .contains("parent must be an assistant"));
                assert_eq!(port.calls.lock().unwrap().len(), 5);
                let invalid = module
                    .ask_lens_for_route(
                        &port,
                        "route-a",
                        make_request(
                            "校验失败时不能发布",
                            Some(continuation.assistant.id.clone()),
                            "READER_ERROR",
                        ),
                    )
                    .await
                    .unwrap_err();
                assert!(invalid.message.contains("invalid enum"));
                let calls = port.calls.lock().unwrap();
                assert_eq!(calls.len(), 6); // no repair call for QA
                assert_eq!(
                    calls[2].previous_interaction_id.as_deref(),
                    Some("node-lens-1")
                );
                assert_eq!(
                    calls[3].previous_interaction_id.as_deref(),
                    Some("node-lens-2")
                );
                assert_eq!(
                    calls[4].previous_interaction_id.as_deref(),
                    Some("node-lens-1")
                );
                assert_eq!(
                    calls[5].previous_interaction_id.as_deref(),
                    Some("node-lens-3")
                );
                assert_eq!(
                    calls[2].system_instruction,
                    lens_qa_system_instruction(kind)
                );
                assert_eq!(
                    calls[3].system_instruction,
                    lens_qa_system_instruction(kind)
                );
                assert_eq!(calls[4].system_instruction, "CUSTOM_CURRENT_QA_PROMPT");
                for (offset, reader) in [
                    "READER_FIRST",
                    "READER_SECOND",
                    "READER_BRANCH",
                    "READER_ERROR",
                ]
                .iter()
                .enumerate()
                {
                    let call = &calls[offset + 2];
                    assert!(call.user_input.starts_with(reader));
                    assert!(call.inline_images.is_empty());
                    assert_eq!(call.pdf_path, module.absolute_pdf(Path::new(relative)));
                    let input: Value = serde_json::from_str(
                        &call.user_input[call.user_input.find('{').unwrap()..],
                    )
                    .unwrap();
                    assert_eq!(input["lens"], snapshot);
                    assert_eq!(input["allowedEvidenceIds"], json!(["block:block-1"]));
                    assert!(input.get("history").is_none());
                    assert_eq!(
                        call.response_schema.as_ref().unwrap(),
                        &lens_qa_schema("block-1")
                    );
                }
                assert!(!calls[3].user_input.contains("READER_FIRST"));
                assert_eq!(
                    module.artifact_module.get(&lens.id).unwrap().content,
                    snapshot
                );
                let messages = module.artifact_module.list_lens_qa(&lens.id).unwrap();
                assert_eq!(messages.len(), 7);
                assert_eq!(messages.iter().filter(|m| m.role == "assistant").count(), 3);
                assert!(messages.iter().all(|m| m.content != "不可发布"));
                assert_eq!(
                    connection
                        .query_row("SELECT COUNT(*) FROM usage_receipts", [], |r| r
                            .get::<_, i64>(0))
                        .unwrap(),
                    calls.len() as i64
                );
                assert_eq!(
                    connection
                        .query_row(
                            "SELECT state FROM provider_nodes WHERE provider_node_id='node-lens-5'",
                            [],
                            |r| r.get::<_, String>(0)
                        )
                        .unwrap(),
                    "invalid"
                );
                assert_eq!(
                    connection
                        .query_row("SELECT COUNT(*) FROM messages", [], |r| r.get::<_, i64>(0))
                        .unwrap(),
                    0
                );
            }
        }
    }

    struct TranslationTestPort {
        response: Value,
        calls: StdMutex<Vec<TextInteractionRequest>>,
    }
    #[async_trait::async_trait]
    impl PaperModelPort for TranslationTestPort {
        fn capabilities(&self, model: &str) -> crate::provider_ports::PaperModelCapabilities {
            NeverCalledPaperPort.capabilities(model)
        }
        async fn interact(
            &self,
            _request: PaperInteractionRequest,
        ) -> ProviderResult<crate::provider_ports::PaperInteractionOutcome> {
            panic!("translation must not call the PDF branch");
        }
        async fn interact_text(
            &self,
            request: TextInteractionRequest,
        ) -> ProviderResult<crate::provider_ports::TextInteractionOutcome> {
            let receipt = UsageEnvelope {
                provider: "gemini".into(),
                model: request.model.clone(),
                context_epoch: Some(request.context_epoch.clone()),
                ..Default::default()
            };
            self.calls.lock().unwrap().push(request);
            Ok(crate::provider_ports::TextInteractionOutcome {
                text: self.response.to_string(),
                provider_node_id: Uuid::new_v4().to_string(),
                receipt,
            })
        }
        async fn delete_remote(
            &self,
            _resource: &crate::provider_ports::RemoteResource,
        ) -> ProviderResult<()> {
            panic!("unexpected cleanup");
        }
    }
    #[tokio::test]
    async fn translation_unavailable_publishes_once_without_pdf_or_reader_context_and_v1_still_works(
    ) {
        let workspace = tempdir().unwrap();
        WorkspaceModule::new().open(workspace.path()).unwrap();
        let module = ReadingArtifactModule::open(workspace.path()).unwrap();
        let connection = module.connect().unwrap();
        insert_revision(&connection);
        add_route_context_schema(&connection);
        connection.execute_batch(r#"
            INSERT INTO ocr_revisions(id,revision_id,status,provider,model,created_at)
            VALUES ('ocr-1','rev-1','ready','mistral','ocr','2026-09-09');
            INSERT INTO ocr_pages(id,ocr_revision_id,page_number) VALUES ('page-1','ocr-1',1);
            INSERT INTO ocr_blocks(id,ocr_page_id,block_index,block_type,text_content,content_digest,x0,y0,x1,y1)
            VALUES ('block-1','page-1',0,'text','???','block-digest',0,0,100,100);
        "#).unwrap();
        let pdf = module.absolute_pdf(Path::new("Inbox/paper.pdf"));
        fs::create_dir_all(pdf.parent().unwrap()).unwrap();
        fs::write(pdf, b"%PDF-fixture").unwrap();
        let mut request = GenerateReadingArtifactRequest {
            document_kind: None,
            revision_id: "rev-1".into(),
            ocr_revision_id: "ocr-1".into(),
            block_id: "block-1".into(),
            action: ReadingArtifactAction::Translate,
            output_language: "zh-CN".into(),
            paper_model: "paper-model".into(),
            translation_model: "translation-model".into(),
            translation_protocol: TranslationProtocol::V2,
            lens_protocol: LensProtocol::V1,
            provider: "gemini".into(),
            display_crop_data_url: None,
            model_crop_data_url: None,
            system_instruction: None,
            repair_system_instruction: None,
            paper_root_system_instruction: None,
            reader_context: Some("DO_NOT_SEND_READER_CONTEXT".into()),
        };
        let port = TranslationTestPort {
            response: json!({"status":"unavailable","sourceLanguage":"und","targetLanguage":"zh-CN","translation":"","notes":["输入文本只有无法辨认的符号。"],"terms":[]}),
            calls: Default::default(),
        };
        let result = module
            .generate_for_route(&port, "route-a", request.clone())
            .await
            .unwrap();
        assert_eq!(result.artifact.status, "ready");
        assert_eq!(result.artifact.content["status"], "unavailable");
        assert_eq!(result.artifact.content["translation"], "");
        assert_eq!(result.artifact.content["translationProtocol"], "v2");
        assert_eq!(result.receipts.len(), 1);
        assert!(!result.repaired);
        {
            let calls = port.calls.lock().unwrap();
            assert_eq!(calls.len(), 1);
            assert!(calls[0].system_instruction.starts_with("# 一、任务定义"));
            assert!(!calls[0].system_instruction.contains("{output_language}"));
            assert_eq!(
                calls[0].response_schema.as_ref().unwrap()["name"],
                "block_translation_v2"
            );
            let input: Value = serde_json::from_str(&calls[0].user_input).unwrap();
            assert_eq!(input.as_object().unwrap().len(), 3);
            assert!(!calls[0].user_input.contains("DO_NOT_SEND_READER_CONTEXT"));
        }
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM context_roots", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        request.translation_protocol = TranslationProtocol::V1;
        let legacy = TranslationTestPort {
            response: json!({"sourceLanguage":"en","targetLanguage":"zh-CN","translation":"旧译文","notes":[],"terms":[]}),
            calls: Default::default(),
        };
        let old = module
            .generate_for_route(&legacy, "route-a", request.clone())
            .await
            .unwrap();
        assert!(old.artifact.content.get("status").is_none());
        assert_eq!(old.artifact.content["translation"], "旧译文");
        assert_eq!(old.artifact.content["translationProtocol"], "v1");
        assert!(legacy.calls.lock().unwrap()[0]
            .system_instruction
            .starts_with("Translate only"));
        request.translation_protocol = TranslationProtocol::V2;
        assert!(module
            .generate_for_route(&legacy, "route-a", request)
            .await
            .is_err());
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM artifacts WHERE kind='translation'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            2
        );
    }

    #[test]
    fn translation_schema_has_no_pdf_input_contract() {
        let schema = TranslationProtocol::V2.schema().to_string();
        assert!(schema.contains("block_translation"));
        assert!(!schema.contains("input_file"));
        assert!(!schema.contains("pdf"));
    }

    #[test]
    fn temporary_crop_file_removes_unpublished_file_and_empty_parent() {
        let root = tempdir().expect("temporary root");
        let parent = root.path().join("lens").join("attempt");
        fs::create_dir_all(&parent).expect("crop parent");
        let path = parent.join("display.png.tmp");
        fs::write(&path, b"partial").expect("partial crop");

        {
            let _guard = TemporaryCropFile::new(path.clone());
        }

        assert!(!path.exists());
        assert!(!parent.exists());
    }

    #[test]
    fn temporary_crop_file_keeps_published_file() {
        let root = tempdir().expect("temporary root");
        let parent = root.path().join("lens").join("attempt");
        fs::create_dir_all(&parent).expect("crop parent");
        let path = parent.join("display.png.tmp");
        fs::write(&path, b"complete").expect("complete crop");

        {
            let mut guard = TemporaryCropFile::new(path.clone());
            guard.publish();
        }

        assert_eq!(fs::read(&path).expect("published crop"), b"complete");
    }

    #[tokio::test]
    async fn roadmap_generation_isolated_siblings_and_local_retry_preserve_paid_response() {
        use crate::job_module::{JobModule, JobSpec};
        use crate::roadmap_module::{generate, GenerationRequest};
        for kind in [
            crate::library_paths::DocumentKind::Paper,
            crate::library_paths::DocumentKind::Textbook,
        ] {
            let workspace = tempdir().unwrap();
            WorkspaceModule::new().open(workspace.path()).unwrap();
            let module = ReadingArtifactModule::open(workspace.path()).unwrap();
            let connection = module.connect().unwrap();
            insert_revision(&connection);
            add_route_context_schema(&connection);
            let jobs = JobModule::open(&module.database_path).unwrap();
            let body = "先辨认 $x > 0$ 的含义。\n\n1. 阅读定义。\n2. 回看定理。";
            let content = json!({"version":1,"paperTitle":"当前论文","passes":[{
                "passNumber":1,"title":"建立方向","subtitle":"先读条件","timeBudget":"约 10 分钟",
                "exitCriteria":"能辨认对象","tasks":[{"id":"p1-t1","text":body,"timeMinutes":5,
                "required":true,"completionCriteria":"能连接定义与条件","selfCheckQuestions":[],
                "evidence":[{"label":"定理 1","page":2}]}]
            }]});
            let port = ExplanationTestPort {
                response: StdMutex::new(content.clone()),
                calls: Default::default(),
            };
            let prompt = crate::prompt_settings::default_text_for_kind(
                crate::prompt_settings::PromptSlotId::ReadingRoadmap,
                kind,
            );
            let root_prompt = crate::prompt_settings::default_text_for_kind(
                crate::prompt_settings::PromptSlotId::PaperRoot,
                kind,
            );
            let new_job = |suffix: &str| {
                let job = jobs
                    .enqueue(JobSpec {
                        kind: "reading_roadmap".into(),
                        provider: None,
                        paper_id: Some("paper-1".into()),
                        revision_id: Some("rev-1".into()),
                        root_key: None,
                        artifact_key: Some("reading_roadmap".into()),
                        dedupe_key: suffix.into(),
                        priority: 85,
                        payload: json!({}),
                    })
                    .unwrap()
                    .job;
                connection
                    .execute("UPDATE jobs SET state='running' WHERE id=?1", [&job.id])
                    .unwrap();
                job.id
            };
            let request = |job_id: String, system: String| GenerationRequest {
                job_id,
                facts: DocumentFacts {
                    paper_id: "paper-1".into(),
                    revision_id: "rev-1".into(),
                    revision_sha256: "digest-1".into(),
                    title: "当前文档".into(),
                    pdf_path: workspace.path().join("paper.pdf"),
                },
                page_count: Some(8),
                provider: "gemini".into(),
                route_id: "route-a".into(),
                model: "model".into(),
                call: PaperRootCall {
                    provider: "gemini".into(),
                    model: "model".into(),
                    system_instruction: system,
                    user_input: crate::roadmap_module::task_input(
                        Some(&json!({"content":{"takeaway":"FROZEN_BRIEF"}})),
                        Some("FROZEN_READER"),
                        "zh-CN",
                    ),
                    response_schema: crate::roadmap_module::response_schema(kind),
                    inline_images: Vec::new(),
                    paper_root_instruction: Some(root_prompt.clone()),
                },
            };
            let first = new_job("first");
            connection.execute_batch("CREATE TRIGGER fail_roadmap_publish BEFORE INSERT ON artifacts BEGIN SELECT RAISE(ABORT, 'transient publish fixture'); END;").unwrap();
            assert!(generate(
                workspace.path(),
                &port,
                &jobs,
                request(first.clone(), prompt.clone())
            )
            .await
            .is_err());
            assert_eq!(port.calls.lock().unwrap().len(), 2);
            assert!(jobs
                .get_checkpoint(&first)
                .unwrap()
                .unwrap()
                .get("response")
                .is_some());
            connection
                .execute_batch("DROP TRIGGER fail_roadmap_publish;")
                .unwrap();
            let artifact = generate(
                workspace.path(),
                &port,
                &jobs,
                request(first.clone(), prompt.clone()),
            )
            .await
            .unwrap();
            assert_eq!(artifact.content, content);
            assert_eq!(port.calls.lock().unwrap().len(), 2);
            let again = generate(
                workspace.path(),
                &port,
                &jobs,
                request(first.clone(), prompt.clone()),
            )
            .await
            .unwrap();
            assert_eq!(again.id, artifact.id);
            let second = new_job("second");
            let next = generate(
                workspace.path(),
                &port,
                &jobs,
                request(second, "用户冻结的自定义路线".into()),
            )
            .await
            .unwrap();
            assert_ne!(next.id, artifact.id);
            {
                let calls = port.calls.lock().unwrap();
                assert_eq!(calls.len(), 3);
                assert_eq!(calls[0].kind, PaperInteractionKind::Root);
                assert_eq!(calls[0].system_instruction, root_prompt);
                assert!(!calls[0].user_input.contains("FROZEN_READER"));
                assert!(!calls[0].user_input.contains("FROZEN_BRIEF"));
                assert_eq!(calls[1].kind, PaperInteractionKind::Artifact);
                assert_eq!(calls[1].system_instruction, prompt);
                assert_eq!(calls[2].system_instruction, "用户冻结的自定义路线");
                assert_eq!(
                    calls[1].previous_interaction_id,
                    calls[2].previous_interaction_id
                );
                assert!(calls[1].previous_interaction_id.is_some());
                assert!(calls[1].user_input.contains("FROZEN_READER"));
                assert!(calls[1].user_input.contains("FROZEN_BRIEF"));
                assert!(!calls[2].user_input.contains(body));
            }
            let count = |sql: &str| {
                connection
                    .query_row(sql, [], |row| row.get::<_, i64>(0))
                    .unwrap()
            };
            assert_eq!(count("SELECT COUNT(*) FROM context_roots"), 1);
            assert_eq!(
                count("SELECT COUNT(*) FROM artifacts WHERE kind='reading_roadmap'"),
                2
            );
            assert_eq!(
                count("SELECT COUNT(*) FROM provider_nodes WHERE parent_id IS NOT NULL"),
                2
            );
            assert_eq!(
                count("SELECT COUNT(*) FROM usage_receipts WHERE job_id IS NOT NULL"),
                2
            );
            port.response.lock().unwrap()["passes"][0]["tasks"][0]["evidence"][0]["blockId"] =
                json!("fabricated");
            let bad = new_job("bad");
            assert!(generate(
                workspace.path(),
                &port,
                &jobs,
                request(bad.clone(), prompt.clone())
            )
            .await
            .is_err());
            assert!(
                generate(workspace.path(), &port, &jobs, request(bad, prompt))
                    .await
                    .is_err()
            );
            assert_eq!(port.calls.lock().unwrap().len(), 4); // no schema-repair or paid retry
            assert_eq!(
                count("SELECT COUNT(*) FROM artifacts WHERE kind='reading_roadmap'"),
                2
            );
            assert_eq!(
                count("SELECT COUNT(*) FROM usage_receipts WHERE job_id IS NOT NULL"),
                3
            );
        }
    }

    fn insert_revision(connection: &Connection) {
        connection
            .execute_batch(
                r#"
                INSERT INTO collections(id, name, relative_path, created_at, updated_at)
                VALUES ('col-1', 'Inbox', 'Inbox', '2026-08-18T00:00:00Z', '2026-08-18T00:00:00Z');
                INSERT INTO papers(id, collection_id, file_name, relative_path, created_at, updated_at)
                VALUES ('paper-1', 'col-1', 'paper.pdf', 'Inbox/paper.pdf', '2026-08-18T00:00:00Z', '2026-08-18T00:00:00Z');
                INSERT INTO document_revisions(id, paper_id, sha256, byte_size, page_count, source_relative_path, created_at)
                VALUES ('rev-1', 'paper-1', 'digest-1', 12, 8, 'Inbox/paper.pdf', '2026-08-18T00:00:00Z');
                "#,
            )
            .expect("revision fixture");
    }

    fn root_outcome(
        provider: &str,
        model: &str,
        context_epoch: &str,
        suffix: &str,
    ) -> crate::provider_ports::PaperInteractionOutcome {
        crate::provider_ports::PaperInteractionOutcome {
            text: r#"{"acknowledged":true}"#.to_string(),
            provider_node_id: format!("node-{suffix}"),
            provider_file_id: format!("file-{suffix}"),
            receipt: UsageEnvelope {
                provider: provider.to_string(),
                model: model.to_string(),
                context_epoch: Some(context_epoch.to_string()),
                paper_root_branch: Some(true),
                ..Default::default()
            },
        }
    }
    #[derive(Default)]
    struct RecordingSourcePort(std::sync::Mutex<Vec<PaperInteractionRequest>>);

    #[async_trait::async_trait]
    impl PaperModelPort for RecordingSourcePort {
        fn capabilities(&self, model: &str) -> crate::provider_ports::PaperModelCapabilities {
            NeverCalledPaperPort.capabilities(model)
        }
        async fn interact(
            &self,
            request: PaperInteractionRequest,
        ) -> ProviderResult<crate::provider_ports::PaperInteractionOutcome> {
            let mut calls = self.0.lock().unwrap();
            let mut response = root_outcome(
                "gemini",
                &request.model,
                &request.context_epoch,
                &format!("call-{}", calls.len()),
            );
            if request.kind != PaperInteractionKind::Root {
                response.text = json!({"generated": calls.len()}).to_string();
            }
            calls.push(request);
            Ok(response)
        }
        async fn interact_text(
            &self,
            _request: crate::provider_ports::TextInteractionRequest,
        ) -> ProviderResult<crate::provider_ports::TextInteractionOutcome> {
            Err(ProviderError::invalid("unexpected text call"))
        }
        async fn delete_remote(
            &self,
            _resource: &crate::provider_ports::RemoteResource,
        ) -> ProviderResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn source_root_skips_old_generated_root_and_is_shared_without_sibling_history() {
        let workspace = tempdir().unwrap();
        WorkspaceModule::new().open(workspace.path()).unwrap();
        let module = ReadingArtifactModule::open(workspace.path()).unwrap();
        let connection = module.connect().unwrap();
        insert_revision(&connection);
        add_route_context_schema(&connection);
        let mut anchor = figure_anchor();
        anchor.paper_id = "paper-1".into();
        anchor.revision_id = "rev-1".into();
        anchor.revision_sha256 = "digest-1".into();
        let model = "gemini-2.5-pro";
        let old_epoch = route_scoped_context_epoch(&format!("digest-1:{model}"), "route-a");
        connection.execute("INSERT INTO context_roots(id,revision_id,provider,model,context_epoch,provider_file_id,provider_node_id,state,created_at,provider_route_id)
            VALUES ('old-pack','rev-1','gemini',?1,?2,'old-file','old-generated-node','active','2026-09-08','route-a')",
            params![model, old_epoch]).unwrap();
        let port = RecordingSourcePort::default();
        for kind in ["brief", "glossary", "metadata"] {
            module
                .call_from_paper_root(
                    &port,
                    &anchor,
                    "route-a",
                    PaperRootCall {
                        provider: "gemini".into(),
                        model: model.into(),
                        system_instruction: kind.into(),
                        user_input: json!({"task":kind}).to_string(),
                        response_schema: json!({}),
                        inline_images: vec![],
                        paper_root_instruction: None,
                    },
                )
                .await
                .unwrap();
        }
        let calls = port.0.lock().unwrap();
        assert_eq!(calls.len(), 4); // one source + three isolated tasks
        assert_eq!(calls[0].kind, PaperInteractionKind::Root);
        assert!(calls[0].previous_interaction_id.is_none());
        assert!(calls[0].remote_file_id.is_none());
        assert!(!calls[0].user_input.contains("localOrientationPack"));
        for call in &calls[1..] {
            assert_eq!(call.previous_interaction_id.as_deref(), Some("node-call-0"));
            assert_eq!(call.remote_file_id.as_deref(), Some("file-call-0"));
            assert!(!call.user_input.contains("generated"));
            assert_eq!(
                call.context_epoch,
                route_scoped_context_epoch(&pdf_source_epoch(&anchor, model, None), "route-a")
            );
        }
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM artifacts", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[tokio::test]
    async fn actual_root_prompt_changes_isolate_cache_and_invalidation() {
        let workspace = tempdir().unwrap();
        WorkspaceModule::new().open(workspace.path()).unwrap();
        let module = ReadingArtifactModule::open(workspace.path()).unwrap();
        let connection = module.connect().unwrap();
        insert_revision(&connection);
        add_route_context_schema(&connection);
        let mut anchor = figure_anchor();
        anchor.paper_id = "paper-1".into();
        anchor.revision_id = "rev-1".into();
        let port = RecordingSourcePort::default();
        let a = module
            .ensure_root_for_route(&port, &anchor, "route-a", "gemini", "model", Some("根甲"))
            .await
            .unwrap();
        let b = module
            .ensure_root_for_route(&port, &anchor, "route-a", "gemini", "model", Some("根乙"))
            .await
            .unwrap();
        assert_ne!(a.context_epoch, b.context_epoch);
        assert_eq!(port.0.lock().unwrap().len(), 2);
        assert_eq!(
            module
                .ensure_root_for_route(&port, &anchor, "route-a", "gemini", "model", Some("根甲"))
                .await
                .unwrap()
                .remote_node_id,
            a.remote_node_id
        );
        module
            .invalidate_root_for_route_with_instruction(&anchor, "route-a", "model", Some("根甲"))
            .unwrap();
        assert_eq!(
            module
                .ensure_root_for_route(&port, &anchor, "route-a", "gemini", "model", Some("根乙"))
                .await
                .unwrap()
                .remote_node_id,
            b.remote_node_id
        );
        module
            .ensure_root_for_route(&port, &anchor, "route-a", "gemini", "model", Some("根甲"))
            .await
            .unwrap();
        assert_eq!(port.0.lock().unwrap().len(), 3);
    }
    #[test]
    fn aliases_match_all_meanings_without_forwarding_source_catalogs() {
        let rows = json!({"entries":[{"term":"attention","aliases":["ATTN"],"definition":"一种机制","usage":"本文编码器","sources":[{"excerpt":"large source"}]},{"term":"attention","aliases":["ATTN"],"definition":"另一含义","usage":"相关工作","sources":[]}]});
        let matched = matching_entries(Some(rows), "term", "uses attn here");
        assert_eq!(matched.len(), 2);
        assert!(matched
            .iter()
            .all(|r| r.get("usage").is_some() && r.get("sources").is_none()));
    }
    #[test]
    fn generated_content_cannot_become_a_source_root_and_its_cost_is_retained() {
        let workspace = tempdir().unwrap();
        WorkspaceModule::new().open(workspace.path()).unwrap();
        let module = ReadingArtifactModule::open(workspace.path()).unwrap();
        let connection = module.connect().unwrap();
        insert_revision(&connection);
        add_route_context_schema(&connection);
        let mut anchor = figure_anchor();
        anchor.paper_id = "paper-1".into();
        anchor.revision_id = "rev-1".into();
        let mut response = root_outcome("gemini", "model", "epoch", "rejected");
        response.text = json!({"acknowledged":true,"brief":"unwanted analysis"}).to_string();
        assert!(module
            .persist_root_outcome_for_route(&anchor, "route-a", "gemini", "model", response)
            .is_err());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM context_roots", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM usage_receipts", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT COUNT(*) FROM remote_tombstones WHERE endpoint_scope = 'endpoint-a'",
                    [],
                    |row| row.get::<_, i64>(0)
                )
                .unwrap(),
            2
        );
    }

    #[test]
    fn valid_root_ack_accepts_fenced_markdown_and_thinking_markers() {
        assert!(is_valid_root_ack(r#"{"acknowledged": true}"#));
        assert!(is_valid_root_ack(r#"{"acknowledged":true}"#));
        assert!(is_valid_root_ack(
            "\n  {\n    \"acknowledged\": true\n  }\n"
        ));
        assert!(is_valid_root_ack("```json\n{\"acknowledged\": true}\n```"));
        assert!(is_valid_root_ack("```\n{\"acknowledged\": true}\n```"));
        assert!(is_valid_root_ack(
            "<think>Receiving the source PDF.</think>\n```json\n{\"acknowledged\": true}\n```"
        ));
        assert!(is_valid_root_ack(
            "<think>Thinking step...</think>{\"acknowledged\": true}"
        ));
    }

    #[test]
    fn valid_root_ack_rejects_unwanted_content_and_invalid_formats() {
        assert!(!is_valid_root_ack(
            r#"{"acknowledged": true, "brief": "unwanted analysis"}"#
        ));
        assert!(!is_valid_root_ack(r#"{"acknowledged": false}"#));
        assert!(!is_valid_root_ack(r#"{"acknowledged": "true"}"#));
        assert!(!is_valid_root_ack(r#"{"status": "ok"}"#));
        assert!(!is_valid_root_ack(""));
        assert!(!is_valid_root_ack("Acknowledged PDF successfully"));
    }

    #[test]
    fn root_outcome_accepts_fenced_ack_and_persists_root() {
        let workspace = tempdir().unwrap();
        WorkspaceModule::new().open(workspace.path()).unwrap();
        let module = ReadingArtifactModule::open(workspace.path()).unwrap();
        let connection = module.connect().unwrap();
        insert_revision(&connection);
        add_route_context_schema(&connection);
        let mut anchor = figure_anchor();
        anchor.paper_id = "paper-1".into();
        anchor.revision_id = "rev-1".into();
        let mut response = root_outcome("gemini", "model", "epoch", "success");
        response.text = "```json\n{\"acknowledged\": true}\n```".to_string();
        let root = module
            .persist_root_outcome_for_route(&anchor, "route-a", "gemini", "model", response)
            .unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM context_roots", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(root.remote_node_id, "node-success");
    }

    struct NeverCalledPaperPort;

    #[async_trait::async_trait]
    impl PaperModelPort for NeverCalledPaperPort {
        fn capabilities(&self, _model: &str) -> crate::provider_ports::PaperModelCapabilities {
            crate::provider_ports::PaperModelCapabilities {
                native_pdf: true,
                interactions: true,
                structured_output: true,
                streaming: true,
            }
        }

        async fn interact(
            &self,
            _request: crate::provider_ports::PaperInteractionRequest,
        ) -> crate::provider_ports::ProviderResult<crate::provider_ports::PaperInteractionOutcome>
        {
            Err(crate::provider_ports::ProviderError::invalid(
                "test port must not receive an interaction",
            ))
        }

        async fn interact_text(
            &self,
            _request: crate::provider_ports::TextInteractionRequest,
        ) -> crate::provider_ports::ProviderResult<crate::provider_ports::TextInteractionOutcome>
        {
            Err(crate::provider_ports::ProviderError::invalid(
                "test port must not receive a text interaction",
            ))
        }

        async fn delete_remote(
            &self,
            _resource: &crate::provider_ports::RemoteResource,
        ) -> crate::provider_ports::ProviderResult<()> {
            Err(crate::provider_ports::ProviderError::invalid(
                "test port must not delete a resource",
            ))
        }
    }

    fn add_route_context_schema(connection: &Connection) {
        connection
            .execute_batch(
                r#"
                CREATE TABLE IF NOT EXISTS remote_endpoint_snapshots (
                  endpoint_scope TEXT PRIMARY KEY,
                  version INTEGER NOT NULL,
                  owner_type TEXT NOT NULL,
                  provider_instance_id TEXT,
                  provider_name_at_capture TEXT,
                  provider_kind TEXT,
                  base_url TEXT,
                  created_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS provider_route_snapshots (
                  route_id TEXT PRIMARY KEY,
                  endpoint_scope TEXT NOT NULL REFERENCES remote_endpoint_snapshots(endpoint_scope),
                  version INTEGER NOT NULL,
                  models_json TEXT NOT NULL,
                  operation_role TEXT NOT NULL,
                  created_at TEXT NOT NULL
                );
                INSERT OR IGNORE INTO remote_endpoint_snapshots(
                  endpoint_scope, version, owner_type, provider_instance_id,
                  provider_name_at_capture, provider_kind, base_url, created_at
                ) VALUES
                  ('endpoint-a', 1, 'paper_provider', 'instance-a', 'A', 'gemini', NULL, '2026-08-23T00:00:00Z'),
                  ('endpoint-b', 1, 'paper_provider', 'instance-b', 'B', 'gemini', NULL, '2026-08-23T00:00:00Z');
                INSERT OR IGNORE INTO provider_route_snapshots(
                  route_id, endpoint_scope, version, models_json, operation_role, created_at
                ) VALUES
                  ('route-a', 'endpoint-a', 1, '{}', 'paper', '2026-08-23T00:00:00Z'),
                  ('route-b', 'endpoint-b', 1, '{}', 'paper', '2026-08-23T00:00:00Z');
                "#,
            )
            .expect("route context schema");
    }

    #[test]
    fn route_scoped_roots_are_independent_and_do_not_reuse_legacy_null() {
        let workspace_root = tempdir().expect("workspace");
        WorkspaceModule::new()
            .open(workspace_root.path())
            .expect("initialize workspace");
        let module =
            ReadingArtifactModule::open(workspace_root.path()).expect("reading artifact module");
        let connection = module.connect().expect("connect");
        insert_revision(&connection);
        add_route_context_schema(&connection);

        let mut anchor = figure_anchor();
        anchor.paper_id = "paper-1".to_string();
        anchor.revision_id = "rev-1".to_string();
        anchor.revision_sha256 = "digest-1".to_string();
        let model = "gemini-2.5-pro";
        let semantic_epoch = pdf_source_epoch(&anchor, model, None);
        let route_a_epoch = route_scoped_context_epoch(&semantic_epoch, "route-a");
        connection
            .execute(
                "INSERT INTO context_roots(
                   id, revision_id, provider, model, context_epoch,
                   provider_file_id, provider_node_id, state, created_at, provider_route_id
                 ) VALUES ('legacy-root', ?1, 'gemini', ?2, ?3,
                           'legacy-file', 'legacy-node', 'active', '2026-08-23T00:00:00Z', NULL)",
                params![anchor.revision_id, model, route_a_epoch],
            )
            .expect("legacy root");

        assert!(lookup_root_on_connection_for_route(
            &connection,
            &anchor.revision_id,
            "route-a",
            model,
            &route_a_epoch,
        )
        .expect("route lookup")
        .is_none());
        connection
            .execute("DELETE FROM context_roots WHERE id = 'legacy-root'", [])
            .expect("remove legacy fixture");
        drop(connection);

        let root_a = module
            .persist_root_outcome_for_route(
                &anchor,
                "route-a",
                "gemini",
                model,
                root_outcome("gemini", model, &route_a_epoch, "a"),
            )
            .expect("route A root");
        let route_b_epoch = route_scoped_context_epoch(&semantic_epoch, "route-b");
        let root_b = module
            .persist_root_outcome_for_route(
                &anchor,
                "route-b",
                "gemini",
                model,
                root_outcome("gemini", model, &route_b_epoch, "b"),
            )
            .expect("route B root");

        assert_eq!(root_a.remote_file_id, "file-a");
        assert_eq!(root_b.remote_file_id, "file-b");
        assert_ne!(root_a.context_epoch, root_b.context_epoch);
        let connection = module.connect().expect("connect");
        let receipts: Vec<String> = connection
            .prepare(
                "SELECT provider_route_id FROM usage_receipts
                 WHERE provider_route_id IS NOT NULL ORDER BY provider_route_id",
            )
            .expect("receipt query")
            .query_map([], |row| row.get(0))
            .expect("receipt rows")
            .collect::<Result<_, _>>()
            .expect("receipt values");
        assert_eq!(receipts, vec!["route-a", "route-b"]);

        let outline = crate::outline_module::OutlineModule::open(&module.database_path)
            .expect("outline module");
        let outline_projection = outline
            .project_for_route(&anchor.revision_id, "route-a", Some(model))
            .expect("route-aware outline projection");
        assert!(outline_projection.has_paper_root);
        assert!(outline_projection.head.is_none());
        assert!(
            !outline
                .project_for_route(&anchor.revision_id, "route-missing", Some(model))
                .expect("missing route projection")
                .has_paper_root
        );

        let guide =
            crate::guide_module::GuideModule::open(&module.database_path).expect("guide module");
        let guide_projection = guide
            .project_for_route(&anchor.revision_id, "route-b", Some(model))
            .expect("route-aware guide projection");
        assert!(guide_projection.has_paper_root);
        assert!(guide_projection.head.is_none());

        let lock_a = context_root_lock_for_route(
            &module.database_path,
            &anchor.revision_id,
            "route-a",
            model,
            &route_a_epoch,
        );
        let lock_a_again = context_root_lock_for_route(
            &module.database_path,
            &anchor.revision_id,
            "route-a",
            model,
            &route_a_epoch,
        );
        let lock_b = context_root_lock_for_route(
            &module.database_path,
            &anchor.revision_id,
            "route-b",
            model,
            &route_b_epoch,
        );
        assert!(Arc::ptr_eq(&lock_a, &lock_a_again));
        assert!(!Arc::ptr_eq(&lock_a, &lock_b));
    }

    #[test]
    fn route_scoped_root_rejects_unknown_snapshot_without_writing_rows() {
        let workspace_root = tempdir().expect("workspace");
        WorkspaceModule::new()
            .open(workspace_root.path())
            .expect("initialize workspace");
        let module =
            ReadingArtifactModule::open(workspace_root.path()).expect("reading artifact module");
        let connection = module.connect().expect("connect");
        insert_revision(&connection);
        add_route_context_schema(&connection);
        drop(connection);

        let mut anchor = figure_anchor();
        anchor.paper_id = "paper-1".to_string();
        anchor.revision_id = "rev-1".to_string();
        anchor.revision_sha256 = "digest-1".to_string();
        let model = "gemini-2.5-pro";
        let semantic_epoch = pdf_source_epoch(&anchor, model, None);
        let route_id = "route-missing";
        let error = module
            .persist_root_outcome_for_route(
                &anchor,
                route_id,
                "gemini",
                model,
                root_outcome(
                    "gemini",
                    model,
                    &route_scoped_context_epoch(&semantic_epoch, route_id),
                    "missing",
                ),
            )
            .expect_err("an unknown route must not create durable provider state");
        assert!(error
            .message
            .contains("Provider route snapshot is unavailable"));

        let connection = module.connect().expect("connect");
        for table in ["context_roots", "provider_nodes", "usage_receipts"] {
            let count: i64 = connection
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE provider_route_id = ?1"),
                    params![route_id],
                    |row| row.get(0),
                )
                .expect("route-owned rows count");
            assert_eq!(count, 0, "{table} must stay unchanged");
        }
    }
    #[test]
    fn route_scoped_provider_node_rejects_cross_route_parent() {
        let workspace_root = tempdir().expect("workspace");
        WorkspaceModule::new()
            .open(workspace_root.path())
            .expect("initialize workspace");
        let module =
            ReadingArtifactModule::open(workspace_root.path()).expect("reading artifact module");
        let connection = module.connect().expect("connect");
        add_route_context_schema(&connection);
        drop(connection);

        let parent = module
            .store_provider_node_for_route(
                "route-a",
                "gemini",
                "gemini-2.5-pro",
                "epoch-a",
                "remote-parent",
                None,
                "complete",
            )
            .expect("route A parent");
        let error = module
            .store_provider_node_for_route(
                "route-b",
                "gemini",
                "gemini-2.5-pro",
                "epoch-b",
                "remote-child",
                Some(&parent),
                "complete",
            )
            .expect_err("cross-route parent must fail closed");
        assert!(error.message.contains("same provider route"));
    }
    #[tokio::test]
    async fn route_scoped_lens_rejects_a_node_from_another_route_before_network() {
        let workspace_root = tempdir().expect("workspace");
        WorkspaceModule::new()
            .open(workspace_root.path())
            .expect("initialize workspace");
        let module =
            ReadingArtifactModule::open(workspace_root.path()).expect("reading artifact module");
        let connection = module.connect().expect("connect");
        insert_revision(&connection);
        add_route_context_schema(&connection);
        drop(connection);

        let node_id = module
            .store_provider_node_for_route(
                "route-a",
                "gemini",
                "gemini-2.5-pro",
                "route-a-epoch",
                "remote-lens-node",
                None,
                "complete",
            )
            .expect("route A Lens node");
        let lens = module
            .artifact_module
            .publish(ArtifactDraft {
                paper_id: "paper-1".to_string(),
                revision_id: "rev-1".to_string(),
                ocr_revision_id: None,
                kind: "lens_figure".to_string(),
                object_key: "block-1".to_string(),
                content: json!({"overallMarkdown": "Lens"}),
                evidence: Vec::new(),
                dependency_snapshot: json!({}),
                provider_node_id: Some(node_id),
            })
            .expect("Lens artifact");

        let error = module
            .ask_lens_for_route(
                &NeverCalledPaperPort,
                "route-b",
                AskLensRequest {
                    output_language: None,
                    lens_artifact_id: lens.id,
                    parent_id: None,
                    question: "What does this show?".to_string(),
                    system_instruction: None,
                    provider: "gemini".to_string(),
                    reader_context: None,
                },
            )
            .await
            .expect_err("a Lens node from another route must not be resumed");
        assert!(error.message.contains("Lens provider node is unavailable"));
        let turn_count: i64 = module
            .connect()
            .expect("connect")
            .query_row("SELECT COUNT(*) FROM lens_qa", [], |row| row.get(0))
            .expect("Lens QA count");
        assert_eq!(turn_count, 0);
    }

    #[test]
    fn persist_root_stores_openai_compatible_provider_and_does_not_hit_gemini_root() {
        let workspace_root = tempdir().expect("workspace");
        WorkspaceModule::new()
            .open(workspace_root.path())
            .expect("initialize workspace");
        let module =
            ReadingArtifactModule::open(workspace_root.path()).expect("reading artifact module");
        insert_revision(&module.connect().expect("connect"));

        let mut anchor = figure_anchor();
        anchor.paper_id = "paper-1".to_string();
        anchor.revision_id = "rev-1".to_string();
        anchor.revision_sha256 = "digest-1".to_string();
        let model = "gpt-4.1";
        let context_epoch = pdf_source_epoch(&anchor, model, None);

        module
            .persist_root_outcome(
                &anchor,
                "openai_compatible",
                model,
                &context_epoch,
                root_outcome("openai_compatible", model, &context_epoch, "openai"),
            )
            .expect("persist openai-compatible root");
        module
            .persist_root_outcome(
                &anchor,
                "gemini",
                model,
                &context_epoch,
                root_outcome("gemini", model, &context_epoch, "gemini"),
            )
            .expect("persist gemini root");

        let connection = module.connect().expect("connect");
        let stored_provider: String = connection
            .query_row(
                "SELECT provider FROM context_roots
                 WHERE revision_id = ?1 AND model = ?2 AND provider_file_id = ?3",
                params![anchor.revision_id, model, "file-openai"],
                |row| row.get(0),
            )
            .expect("stored openai-compatible provider");
        assert_eq!(stored_provider, "openai_compatible");

        let gemini_lookup = lookup_root_on_connection(
            &connection,
            &anchor.revision_id,
            "gemini",
            model,
            &context_epoch,
        )
        .expect("gemini lookup");
        assert_eq!(
            gemini_lookup
                .as_ref()
                .map(|root| root.remote_file_id.as_str()),
            Some("file-gemini")
        );

        let openai_lookup = lookup_root_on_connection(
            &connection,
            &anchor.revision_id,
            "openai_compatible",
            model,
            &context_epoch,
        )
        .expect("openai-compatible lookup");
        assert_eq!(
            openai_lookup
                .as_ref()
                .map(|root| root.remote_file_id.as_str()),
            Some("file-openai")
        );
        assert_ne!(
            openai_lookup
                .as_ref()
                .map(|root| root.remote_file_id.as_str()),
            gemini_lookup
                .as_ref()
                .map(|root| root.remote_file_id.as_str())
        );
    }
}
