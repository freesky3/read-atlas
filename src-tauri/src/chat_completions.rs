use crate::model_settings::{normalize_chat_completions_base_url, PAPER_PROBE_VERSION};
use crate::provider_ports::{
    InlineImageInput, PaperInteractionKind, PaperInteractionOutcome, PaperInteractionRequest,
    PaperModelCapabilities, PaperModelPort, PaperStreamRequest, ProviderError, ProviderErrorKind,
    ProviderResult, RemoteResource, TextInteractionOutcome, TextInteractionRequest, UsageEnvelope,
};
use crate::GeminiModelOption;
use async_trait::async_trait;
use base64::Engine;
use reqwest::Client;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

#[allow(dead_code)]
static CHAT_CLIENT: OnceLock<Result<Client, String>> = OnceLock::new();
const INLINE_PDF_ID: &str = "inline-pdf";
const PAPER_PROBE_NONCE: &str = "RD-PDF-NONCE-7F3A";
const PAPER_PROBE_LETTER_R_PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAAAAAA6mKC9AAAAJUlEQVR42mP4zwAC/+GA4T8DhCJJAIsZSHyoFiJsQYgga6ejAAAMY+cZWI+TXAAAAABJRU5ErkJggg==";
const RETRIEVAL_PDF_FAILURE: &str = "检索式 PDF 不能作为论文根";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaperProbeOutcome {
    pub passed: bool,
    pub failure: Option<String>,
}

impl PaperProbeOutcome {
    fn passed() -> Self {
        Self {
            passed: true,
            failure: None,
        }
    }

    fn failed(message: impl Into<String>) -> Self {
        Self {
            passed: false,
            failure: Some(message.into()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatCompletionsAdapter {
    client: Client,
    provider: String,
    api_key: String,
    api_base: String,
}

#[allow(dead_code)]
impl ChatCompletionsAdapter {
    pub fn new(
        provider: impl Into<String>,
        api_key: impl Into<String>,
        api_base: impl Into<String>,
    ) -> ProviderResult<Self> {
        let api_key = api_key.into();
        if api_key.trim().is_empty() {
            return Err(ProviderError {
                kind: ProviderErrorKind::Unauthorized,
                message: "API credential is missing".to_string(),
                retry_after_seconds: None,
                orphaned_resource: None,
            });
        }
        let api_base = normalize_chat_completions_base_url(&api_base.into());
        if api_base.is_empty() {
            return Err(ProviderError::local_state("API base URL is required"));
        }
        let client = CHAT_CLIENT
            .get_or_init(|| {
                Client::builder()
                    .timeout(Duration::from_secs(120))
                    .build()
                    .map_err(|error| error.to_string())
            })
            .clone()
            .map_err(ProviderError::local_state)?;
        Ok(Self {
            client,
            provider: provider.into(),
            api_key,
            api_base,
        })
    }

    pub fn is_gemini_proxy(&self) -> bool {
        self.provider == "gemini_proxy"
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.api_base)
    }

    fn files_url(&self) -> String {
        format!("{}/files", self.api_base)
    }

    #[allow(dead_code)]
    fn models_url(&self) -> String {
        format!("{}/models", self.api_base)
    }

    async fn upload_pdf(&self, path: &Path, display_name: &str) -> ProviderResult<String> {
        let file_name = safe_filename(display_name);
        let part = reqwest::multipart::Part::file(path)
            .await
            .map_err(|error| ProviderError::local_state(error.to_string()))?
            .file_name(file_name)
            .mime_str("application/pdf")
            .map_err(|error| ProviderError::invalid(error.to_string()))?;
        let form = reqwest::multipart::Form::new()
            .text("purpose", "user_data")
            .part("file", part);
        let response = self
            .client
            .post(self.files_url())
            .bearer_auth(&self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(transport_error)?;
        let payload = checked_json(&self.provider, response).await?;
        payload
            .get("id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ProviderError::invalid("File upload response is missing id"))
    }

    async fn resolve_pdf(
        &self,
        request: &PaperInteractionRequest,
    ) -> ProviderResult<(Value, String, bool)> {
        if self.is_gemini_proxy() {
            let part = inline_pdf_part(&request.pdf_path, &request.display_name, true).await?;
            return Ok((part, INLINE_PDF_ID.to_string(), false));
        }
        if let Some(id) = &request.remote_file_id {
            return Ok((file_id_part(id), id.clone(), false));
        }
        match self
            .upload_pdf(&request.pdf_path, &request.display_name)
            .await
        {
            Ok(id) => Ok((file_id_part(&id), id, true)),
            Err(_) => {
                let part = inline_pdf_part(&request.pdf_path, &request.display_name, false).await?;
                Ok((part, INLINE_PDF_ID.to_string(), false))
            }
        }
    }

    async fn send_chat(
        &self,
        mut body: Value,
        retry_file_variant: bool,
    ) -> ProviderResult<reqwest::Response> {
        let response = self
            .client
            .post(self.chat_url())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(transport_error)?;
        if retry_file_variant
            && response.status().is_client_error()
            && replace_file_part_with_input_file(&mut body)
        {
            let _ = response.bytes().await;
            return self
                .client
                .post(self.chat_url())
                .bearer_auth(&self.api_key)
                .json(&body)
                .send()
                .await
                .map_err(transport_error);
        }
        Ok(response)
    }

    pub async fn list_models(&self) -> Result<Vec<GeminiModelOption>, String> {
        let response = match self
            .client
            .get(self.models_url())
            .bearer_auth(&self.api_key)
            .send()
            .await
        {
            Ok(res) => res,
            Err(error) => {
                if self.is_gemini_proxy() {
                    return Ok(default_gemini_proxy_models());
                }
                return Err(redact_secret(
                    &transport_error(error).message,
                    &self.api_key,
                ));
            }
        };
        let payload = match checked_json(&self.provider, response).await {
            Ok(p) => p,
            Err(err) => {
                if self.is_gemini_proxy() {
                    return Ok(default_gemini_proxy_models());
                }
                return Err(redact_secret(&err.message, &self.api_key));
            }
        };
        let mut models = if let Some(data) = payload.get("data").and_then(Value::as_array) {
            data.iter()
                .filter_map(|item| {
                    let id = item.get("id").and_then(Value::as_str)?.trim();
                    if id.is_empty() {
                        return None;
                    }
                    Some(openai_model_option(id, false))
                })
                .collect()
        } else {
            Vec::new()
        };
        if models.is_empty() && self.is_gemini_proxy() {
            models = default_gemini_proxy_models();
        }
        Ok(models)
    }

    pub async fn run_paper_probe(&self, paper_model: &str) -> Result<PaperProbeOutcome, String> {
        let paper_model = paper_model.trim();
        if paper_model.is_empty() {
            return Err("论文模型不能为空".to_string());
        }
        let first = self.probe_pdf_and_image(paper_model).await?;
        if !first.passed {
            return Ok(first);
        }
        self.probe_structured_output(paper_model).await
    }

    async fn probe_pdf_and_image(&self, paper_model: &str) -> Result<PaperProbeOutcome, String> {
        let request = PaperInteractionRequest {
            model: paper_model.to_string(),
            context_epoch: format!("{PAPER_PROBE_VERSION}:{paper_model}"),
            pdf_path: PathBuf::new(),
            display_name: "paper-probe.pdf".to_string(),
            remote_file_id: None,
            previous_interaction_id: None,
            system_instruction: "You verify whether this model can read an attached PDF and image. Reply with JSON or plain text. Do not call tools.".to_string(),
            user_input: "Read the attached PDF and the attached image. Return the exact printed text from the PDF and the capital letter shown in the image.".to_string(),
            response_schema: None,
            inline_images: vec![InlineImageInput {
                mime_type: "image/png".to_string(),
                base64_data: PAPER_PROBE_LETTER_R_PNG_BASE64.to_string(),
            }],
            kind: PaperInteractionKind::Root,
        };
        let is_proxy = self.is_gemini_proxy();
        let body = chat_body(
            &request,
            inline_pdf_part_from_bytes("paper-probe.pdf", &paper_probe_pdf_bytes(), is_proxy),
            false,
            is_proxy,
        );
        let payload = match self.send_probe_chat(body).await {
            Ok(payload) => payload,
            Err(outcome) => return Ok(outcome),
        };
        if retrieval_tool_in_payload(&payload) {
            return Ok(PaperProbeOutcome::failed(RETRIEVAL_PDF_FAILURE));
        }
        let text = completion_text(&payload).unwrap_or_default();
        if !text.contains(PAPER_PROBE_NONCE) {
            return Ok(PaperProbeOutcome::failed("探针未读到 PDF nonce"));
        }
        if !text_has_independent_letter_r(&text) {
            return Ok(PaperProbeOutcome::failed("探针未读到图像字母 R"));
        }
        Ok(PaperProbeOutcome::passed())
    }

    async fn probe_structured_output(
        &self,
        paper_model: &str,
    ) -> Result<PaperProbeOutcome, String> {
        let result = self
            .interact_text(TextInteractionRequest {
                model: paper_model.to_string(),
                context_epoch: format!("{PAPER_PROBE_VERSION}:{paper_model}"),
                system_instruction: "Follow the response schema.".to_string(),
                user_input: "Return ok true.".to_string(),
                response_schema: Some(json!({
                    "type": "object",
                    "properties": {"ok": {"type": "boolean"}},
                    "required": ["ok"],
                    "additionalProperties": false
                })),
                kind: PaperInteractionKind::Root,
            })
            .await;
        let text = match result {
            Ok(outcome) => outcome.text,
            Err(error) => {
                return Ok(PaperProbeOutcome::failed(redact_secret(
                    &error.message,
                    &self.api_key,
                )));
            }
        };
        if json_ok_is_true(&text) {
            Ok(PaperProbeOutcome::passed())
        } else {
            Ok(PaperProbeOutcome::failed("结构化输出探针失败"))
        }
    }

    async fn send_probe_chat(&self, body: Value) -> Result<Value, PaperProbeOutcome> {
        let response = self.send_chat(body, false).await.map_err(|error| {
            PaperProbeOutcome::failed(redact_secret(&error.message, &self.api_key))
        })?;
        checked_json(&self.provider, response)
            .await
            .map_err(|error| {
                PaperProbeOutcome::failed(redact_secret(&error.message, &self.api_key))
            })
    }

    fn paper_outcome(
        &self,
        payload: &Value,
        request: &PaperInteractionRequest,
        provider_file_id: String,
        text: String,
        latency: Duration,
        file_reuse: bool,
    ) -> PaperInteractionOutcome {
        PaperInteractionOutcome {
            text,
            provider_node_id: completion_id(payload),
            provider_file_id,
            receipt: parse_usage(
                payload,
                &self.provider,
                &request.model,
                &request.context_epoch,
                latency,
                file_reuse,
                request.previous_interaction_id.is_none(),
            ),
        }
    }
}

#[async_trait]
impl PaperModelPort for ChatCompletionsAdapter {
    fn capabilities(&self, _model: &str) -> PaperModelCapabilities {
        PaperModelCapabilities {
            native_pdf: true,
            interactions: true,
            structured_output: true,
            streaming: true,
        }
    }

    async fn interact(
        &self,
        request: PaperInteractionRequest,
    ) -> ProviderResult<PaperInteractionOutcome> {
        require_instructions(&request.system_instruction, &request.user_input, false)?;
        let file_reuse = request.remote_file_id.is_some();
        let (pdf_part, provider_file_id, uploaded_here) = self.resolve_pdf(&request).await?;
        let retry_file_variant = provider_file_id != INLINE_PDF_ID;
        let is_proxy = self.is_gemini_proxy();
        let result = async {
            let started = Instant::now();
            let body = chat_body(&request, pdf_part, false, is_proxy);
            let response = self.send_chat(body, retry_file_variant).await?;
            let payload = checked_json(&self.provider, response).await?;
            let text = completion_text(&payload)
                .ok_or_else(|| ProviderError::invalid("Chat completion has no output text"))?;
            Ok(self.paper_outcome(
                &payload,
                &request,
                provider_file_id.clone(),
                text,
                started.elapsed(),
                file_reuse,
            ))
        }
        .await;
        result
            .map_err(|error| attach_orphan(error, uploaded_here, &self.provider, &provider_file_id))
    }

    async fn interact_stream(
        &self,
        request: PaperStreamRequest,
    ) -> ProviderResult<PaperInteractionOutcome> {
        request.cancellation.check()?;
        require_instructions(
            &request.interaction.system_instruction,
            &request.interaction.user_input,
            false,
        )?;
        let file_reuse = request.interaction.remote_file_id.is_some();
        let (pdf_part, provider_file_id, uploaded_here) =
            self.resolve_pdf(&request.interaction).await?;
        let retry_file_variant = provider_file_id != INLINE_PDF_ID;
        let is_proxy = self.is_gemini_proxy();
        let result = async {
            request.cancellation.check()?;
            let started = Instant::now();
            let body = chat_body(&request.interaction, pdf_part, true, is_proxy);
            let response = self.send_chat(body, retry_file_variant).await?;
            if !response.status().is_success() {
                return match checked_json(&self.provider, response).await {
                    Err(error) => Err(error),
                    Ok(_) => Err(ProviderError::invalid(
                        "Chat completion returned an unexpected non-success stream response",
                    )),
                };
            }

            let mut response = response;
            let mut buffer = LineBuffer::default();
            let mut accumulated = String::new();
            let mut provider_node_id = None;
            let mut usage_payload = Value::Null;
            let mut done = false;
            while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
                request.cancellation.check()?;
                buffer.push(&chunk);
                while let Some(line) = buffer.next_line()? {
                    request.cancellation.check()?;
                    let Some(data) = sse_data(&line) else {
                        continue;
                    };
                    if data.is_empty() {
                        continue;
                    }
                    if data == "[DONE]" {
                        done = true;
                        break;
                    }
                    let payload: Value = serde_json::from_str(data).map_err(|error| {
                        ProviderError::invalid(format!("Invalid chat SSE payload: {error}"))
                    })?;
                    if provider_node_id.is_none() {
                        if let Some(id) = payload.get("id").and_then(Value::as_str) {
                            provider_node_id = Some(id.to_string());
                        }
                    }
                    if payload.get("usage").is_some() {
                        usage_payload = payload.clone();
                    }
                    if let Some(delta) = payload
                        .pointer("/choices/0/delta/content")
                        .and_then(Value::as_str)
                    {
                        if !delta.is_empty() {
                            accumulated.push_str(delta);
                            let _ = request.deltas.send(delta.to_string());
                        }
                    }
                }
                if done {
                    break;
                }
            }
            if !done {
                if let Some(line) = buffer.remainder()? {
                    if let Some(data) = sse_data(&line) {
                        if data != "[DONE]" && !data.is_empty() {
                            if let Ok(payload) = serde_json::from_str::<Value>(data) {
                                if provider_node_id.is_none() {
                                    if let Some(id) = payload.get("id").and_then(Value::as_str) {
                                        provider_node_id = Some(id.to_string());
                                    }
                                }
                                if let Some(delta) = payload
                                    .pointer("/choices/0/delta/content")
                                    .and_then(Value::as_str)
                                {
                                    if !delta.is_empty() {
                                        accumulated.push_str(delta);
                                        let _ = request.deltas.send(delta.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if accumulated.trim().is_empty() {
                return Err(ProviderError::invalid(
                    "Chat stream ended without output text",
                ));
            }
            let mut payload = if usage_payload.is_null() {
                json!({})
            } else {
                usage_payload
            };
            if let Some(id) = &provider_node_id {
                payload["id"] = Value::String(id.clone());
            }
            Ok(self.paper_outcome(
                &payload,
                &request.interaction,
                provider_file_id.clone(),
                accumulated,
                started.elapsed(),
                file_reuse,
            ))
        }
        .await;
        result
            .map_err(|error| attach_orphan(error, uploaded_here, &self.provider, &provider_file_id))
    }

    async fn interact_text(
        &self,
        request: TextInteractionRequest,
    ) -> ProviderResult<TextInteractionOutcome> {
        require_instructions(&request.system_instruction, &request.user_input, true)?;
        let started = Instant::now();
        let mut system_instruction = request.system_instruction;
        let mut response_format_val = None;
        if let Some(schema) = &request.response_schema {
            if self.is_gemini_proxy() {
                let schema_str =
                    serde_json::to_string_pretty(schema).unwrap_or_else(|_| schema.to_string());
                system_instruction = format!(
                    "{}\n\n[IMPORTANT: You MUST respond with a valid JSON object strictly adhering to this JSON Schema:\n{}]",
                    system_instruction, schema_str
                );
                response_format_val = Some(json!({"type": "json_object"}));
            } else {
                response_format_val = Some(response_format(schema));
            }
        }
        let mut body = json!({
            "model": request.model,
            "temperature": 0.1,
            "messages": [
                {"role": "system", "content": system_instruction},
                {"role": "user", "content": request.user_input}
            ]
        });
        if let Some(fmt) = response_format_val {
            body["response_format"] = fmt;
        }
        let response = self
            .client
            .post(self.chat_url())
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(transport_error)?;
        let payload = checked_json(&self.provider, response).await?;
        let text = completion_text(&payload)
            .ok_or_else(|| ProviderError::invalid("Chat completion has no output text"))?;
        Ok(TextInteractionOutcome {
            text,
            provider_node_id: completion_id(&payload),
            receipt: parse_usage(
                &payload,
                &self.provider,
                &request.model,
                &request.context_epoch,
                started.elapsed(),
                false,
                request.kind == PaperInteractionKind::Root,
            ),
        })
    }

    async fn delete_remote(&self, resource: &RemoteResource) -> ProviderResult<()> {
        if resource.kind != "file" || resource.id == INLINE_PDF_ID {
            return Ok(());
        }
        let response = self
            .client
            .delete(format!(
                "{}/{}",
                self.files_url(),
                resource.id.trim_start_matches('/')
            ))
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(transport_error)?;
        checked_empty(&self.provider, response).await
    }
}

#[allow(dead_code)]
pub fn paper_probe_fingerprint(base_url: &str, api_key: &str, paper_model: &str) -> String {
    crate::model_settings::paper_probe_fingerprint(base_url, api_key, paper_model)
}

#[allow(dead_code)]
pub async fn list_openai_models(
    adapter: &ChatCompletionsAdapter,
) -> Result<Vec<GeminiModelOption>, String> {
    adapter.list_models().await
}

#[allow(dead_code)]
pub async fn run_paper_probe(
    adapter: &ChatCompletionsAdapter,
    paper_model: &str,
) -> Result<PaperProbeOutcome, String> {
    adapter.run_paper_probe(paper_model).await
}

#[allow(dead_code)]
pub fn apply_paper_probe_to_models(
    mut models: Vec<GeminiModelOption>,
    paper_model: &str,
    probe_passed: bool,
) -> Vec<GeminiModelOption> {
    if !probe_passed || paper_model.trim().is_empty() {
        for model in &mut models {
            model.supports_native_pdf = false;
            model.supports_interactions = false;
        }
        return models;
    }
    let mut found = false;
    for model in &mut models {
        let probed = model.id == paper_model;
        model.supports_native_pdf = probed;
        model.supports_interactions = probed;
        found |= probed;
    }
    if !found {
        models.push(openai_model_option(paper_model, true));
    }
    models
}

fn default_gemini_proxy_models() -> Vec<GeminiModelOption> {
    vec![
        openai_model_option("gemini-3.7-flash-high", false),
        openai_model_option("gemini-3.7-flash-medium", false),
        openai_model_option("gemini-3.7-flash-low", false),
        openai_model_option("gemini-3.6-flash-high", false),
        openai_model_option("gemini-3.6-flash-medium", false),
        openai_model_option("gemini-3.6-flash-low", false),
        openai_model_option("gemini-3.5-flash-high", false),
        openai_model_option("gemini-3.5-flash-medium", false),
        openai_model_option("gemini-3.5-flash-low", false),
        openai_model_option("gemini-3.1-pro-high", false),
        openai_model_option("gemini-3.1-pro-low", false),
        openai_model_option("gemini-3.1-flash-lite", false),
        openai_model_option("gemini-2.5-flash", false),
        openai_model_option("gemini-2.5-pro", false),
    ]
}

fn openai_model_option(id: &str, probed: bool) -> GeminiModelOption {
    GeminiModelOption {
        id: id.to_string(),
        display_name: id.to_string(),
        description: String::new(),
        input_token_limit: None,
        output_token_limit: None,
        supports_generate_content: true,
        supports_native_pdf: probed,
        supports_interactions: probed,
    }
}

fn paper_probe_pdf_bytes() -> Vec<u8> {
    let content = format!("BT\n/F1 12 Tf\n20 100 Td\n({PAPER_PROBE_NONCE}) Tj\nET\n");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>".to_string(),
        format!(
            "<< /Length {} >>\nstream\n{}endstream",
            content.len(),
            content
        ),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (index, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", index + 1).as_bytes());
        pdf.extend_from_slice(body.as_bytes());
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref_at = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}

fn inline_pdf_part_from_bytes(display_name: &str, bytes: &[u8], is_gemini_proxy: bool) -> Value {
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    if is_gemini_proxy {
        json!({
            "type": "image_url",
            "image_url": {
                "url": format!("data:application/pdf;base64,{encoded}")
            }
        })
    } else {
        json!({
            "type": "file",
            "file": {
                "filename": safe_filename(display_name),
                "file_data": format!("data:application/pdf;base64,{encoded}")
            }
        })
    }
}

fn retrieval_tool_in_payload(payload: &Value) -> bool {
    if tool_calls_contain_retrieval(payload.get("tool_calls")) {
        return true;
    }
    if let Some(choices) = payload.get("choices").and_then(Value::as_array) {
        for choice in choices {
            let message = choice.get("message").unwrap_or(choice);
            if tool_calls_contain_retrieval(message.get("tool_calls"))
                || tool_calls_contain_retrieval(choice.get("tool_calls"))
            {
                return true;
            }
        }
    }
    payload
        .get("output")
        .is_some_and(output_contains_retrieval_type)
}

fn tool_calls_contain_retrieval(value: Option<&Value>) -> bool {
    let Some(Value::Array(items)) = value else {
        return false;
    };
    items.iter().any(|item| {
        item.get("name")
            .and_then(Value::as_str)
            .is_some_and(is_retrieval_tool_name)
            || item
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(is_retrieval_tool_name)
            || item
                .pointer("/function/name")
                .and_then(Value::as_str)
                .is_some_and(is_retrieval_tool_name)
    })
}

fn output_contains_retrieval_type(value: &Value) -> bool {
    match value {
        Value::Array(items) => items.iter().any(output_contains_retrieval_type),
        Value::Object(map) => {
            map.get("type")
                .and_then(Value::as_str)
                .is_some_and(is_retrieval_tool_name)
                || map.values().any(output_contains_retrieval_type)
        }
        _ => false,
    }
}

fn is_retrieval_tool_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.contains("attachment_search")
        || lower.contains("file_search")
        || lower.contains("document_search")
}

fn text_has_independent_letter_r(text: &str) -> bool {
    let stripped = text.replace(PAPER_PROBE_NONCE, "");
    let chars: Vec<char> = stripped.chars().collect();
    chars.iter().enumerate().any(|(index, character)| {
        *character == 'R'
            && (index == 0 || !chars[index - 1].is_ascii_alphanumeric())
            && (index + 1 == chars.len() || !chars[index + 1].is_ascii_alphanumeric())
    })
}

fn json_ok_is_true(text: &str) -> bool {
    parse_json_object(text)
        .and_then(|value| value.get("ok").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn parse_json_object(text: &str) -> Option<Value> {
    let trimmed = text.trim();
    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        return Some(value);
    }
    let start = trimmed.find('{')?;
    let end = trimmed.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&trimmed[start..=end]).ok()
}

fn redact_secret(message: &str, secret: &str) -> String {
    if secret.is_empty() {
        message.to_string()
    } else {
        message.replace(secret, "[redacted]")
    }
}

fn require_instructions(system: &str, user: &str, text_mode: bool) -> ProviderResult<()> {
    if system.trim().is_empty() || user.trim().is_empty() {
        return Err(ProviderError::invalid(if text_mode {
            "Text interaction instructions and input are required"
        } else {
            "Interaction instructions and input are required"
        }));
    }
    Ok(())
}

fn safe_filename(display_name: &str) -> String {
    let safe: String = display_name
        .chars()
        .filter(|character| !matches!(character, '"' | '\r' | '\n'))
        .collect();
    if safe.trim().is_empty() {
        "paper.pdf".to_string()
    } else {
        safe
    }
}

fn file_id_part(file_id: &str) -> Value {
    json!({
        "type": "file",
        "file": {"file_id": file_id}
    })
}

fn input_file_part(file_id: &str) -> Value {
    json!({
        "type": "input_file",
        "file_id": file_id
    })
}

async fn inline_pdf_part(
    path: &Path,
    display_name: &str,
    is_gemini_proxy: bool,
) -> ProviderResult<Value> {
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|error| ProviderError::local_state(format!("Unable to read PDF: {error}")))?;
    Ok(inline_pdf_part_from_bytes(
        display_name,
        &bytes,
        is_gemini_proxy,
    ))
}

fn user_content(pdf_part: Value, request: &PaperInteractionRequest) -> Vec<Value> {
    let mut content = vec![pdf_part];
    for image in &request.inline_images {
        content.push(json!({
            "type": "image_url",
            "image_url": {
                "url": format!("data:{};base64,{}", image.mime_type, image.base64_data)
            }
        }));
    }
    content.push(json!({"type": "text", "text": request.user_input}));
    content
}

fn chat_body(
    request: &PaperInteractionRequest,
    pdf_part: Value,
    stream: bool,
    is_gemini_proxy: bool,
) -> Value {
    let mut system_instruction = request.system_instruction.clone();
    let mut response_format_val = None;
    if let Some(schema) = &request.response_schema {
        if is_gemini_proxy {
            let schema_str =
                serde_json::to_string_pretty(schema).unwrap_or_else(|_| schema.to_string());
            system_instruction = format!(
                "{}\n\n[IMPORTANT: You MUST respond with a valid JSON object strictly adhering to this JSON Schema:\n{}]",
                system_instruction, schema_str
            );
            response_format_val = Some(json!({"type": "json_object"}));
        } else {
            response_format_val = Some(response_format(schema));
        }
    }
    let mut body = json!({
        "model": request.model,
        "temperature": 0.2,
        "messages": [
            {"role": "system", "content": system_instruction},
            {"role": "user", "content": user_content(pdf_part, request)}
        ]
    });
    if stream {
        body["stream"] = Value::Bool(true);
    }
    if let Some(fmt) = response_format_val {
        body["response_format"] = fmt;
    }
    body
}

fn response_format(schema: &Value) -> Value {
    let json_schema = schema
        .get("schema")
        .filter(|inner| inner.is_object())
        .cloned()
        .unwrap_or_else(|| schema.clone());
    json!({
        "type": "json_schema",
        "json_schema": {
            "name": "read_desktop",
            "strict": true,
            "schema": json_schema
        }
    })
}

fn replace_file_part_with_input_file(body: &mut Value) -> bool {
    let Some(content) = body
        .pointer_mut("/messages/1/content")
        .and_then(Value::as_array_mut)
    else {
        return false;
    };
    for part in content {
        if part.get("type").and_then(Value::as_str) != Some("file") {
            continue;
        }
        if let Some(file_id) = part
            .pointer("/file/file_id")
            .and_then(Value::as_str)
            .map(str::to_string)
        {
            *part = input_file_part(&file_id);
            return true;
        }
    }
    false
}

fn completion_id(payload: &Value) -> String {
    payload
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("chatcmpl-local-{}", uuid::Uuid::new_v4()))
}

fn completion_text(payload: &Value) -> Option<String> {
    payload
        .pointer("/choices/0/message/content")
        .and_then(json_text)
        .or_else(|| payload.pointer("/choices/0/text").and_then(json_text))
}

fn json_text(value: &Value) -> Option<String> {
    if let Some(text) = value.as_str() {
        return (!text.trim().is_empty()).then(|| text.to_string());
    }
    let mut chunks = Vec::new();
    for part in value.as_array().into_iter().flatten() {
        if let Some(text) = part.get("text").and_then(Value::as_str) {
            if !text.is_empty() {
                chunks.push(text);
            }
        }
    }
    if chunks.is_empty() {
        None
    } else {
        Some(chunks.join(""))
    }
}

fn parse_usage(
    payload: &Value,
    provider: &str,
    model: &str,
    context_epoch: &str,
    latency: Duration,
    file_reuse: bool,
    paper_root_branch: bool,
) -> UsageEnvelope {
    let usage = payload.get("usage").unwrap_or(&Value::Null);
    let input_tokens = first_i64(&[
        usage.get("prompt_tokens"),
        usage.get("input_tokens"),
        usage.get("total_input_tokens"),
    ]);
    let cached_input_tokens = first_i64(&[
        usage.pointer("/prompt_tokens_details/cached_tokens"),
        usage.get("cached_tokens"),
        usage.get("cached_input_tokens"),
    ]);
    UsageEnvelope {
        provider: provider.to_string(),
        model: model.to_string(),
        context_epoch: Some(context_epoch.to_string()),
        input_tokens,
        cached_input_tokens,
        uncached_input_tokens: match (input_tokens, cached_input_tokens) {
            (Some(input), Some(cached)) => Some((input - cached).max(0)),
            _ => None,
        },
        output_tokens: first_i64(&[
            usage.get("completion_tokens"),
            usage.get("output_tokens"),
            usage.get("total_output_tokens"),
        ]),
        reasoning_tokens: first_i64(&[
            usage.pointer("/completion_tokens_details/reasoning_tokens"),
            usage.get("reasoning_tokens"),
        ]),
        latency_ms: Some(latency.as_millis().min(i64::MAX as u128) as i64),
        estimated_cost: None,
        file_reuse: Some(file_reuse),
        session_resume: Some(false),
        paper_root_branch: Some(paper_root_branch),
    }
}

fn first_i64(values: &[Option<&Value>]) -> Option<i64> {
    values.iter().flatten().find_map(|value| {
        value
            .as_i64()
            .or_else(|| value.as_u64().map(|n| n.min(i64::MAX as u64) as i64))
    })
}

fn attach_orphan(
    mut error: ProviderError,
    uploaded_here: bool,
    provider: &str,
    file_id: &str,
) -> ProviderError {
    if uploaded_here && error.orphaned_resource.is_none() {
        error.orphaned_resource = Some(RemoteResource {
            provider: provider.to_string(),
            kind: "file".to_string(),
            id: file_id.to_string(),
        });
    }
    error
}

fn transport_error(error: reqwest::Error) -> ProviderError {
    let kind = if error.is_timeout() {
        ProviderErrorKind::Timeout
    } else {
        ProviderErrorKind::Transport
    };
    let err_str = error.without_url().to_string();
    let message = if err_str.contains("Proxy service is currently disabled") {
        "Antigravity-Manager 代理服务尚未开启。请在 Antigravity-Manager 软件中点击开启“代理服务 (Enable Proxy)”开关。".to_string()
    } else if err_str.contains("10061")
        || err_str.contains("connection refused")
        || err_str.contains("os error 10061")
    {
        "无法连接到 Antigravity-Manager (端口 8045)。请确认 Antigravity-Manager 客户端正在运行。"
            .to_string()
    } else {
        format!("Provider transport failed: {err_str}")
    };
    ProviderError {
        kind,
        message,
        retry_after_seconds: None,
        orphaned_resource: None,
    }
}

async fn checked_json(provider: &str, response: reqwest::Response) -> ProviderResult<Value> {
    let status = response.status();
    let retry_after_seconds = retry_after(response.headers());
    let bytes = response.bytes().await.map_err(transport_error)?;
    if bytes.is_empty() {
        if status.is_success() {
            return Ok(Value::Null);
        } else {
            return Err(ProviderError::invalid(format!(
                "{provider} HTTP {status}: empty response body"
            )));
        }
    }
    match serde_json::from_slice::<Value>(&bytes) {
        Ok(payload) => {
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
        Err(_) => {
            let text = String::from_utf8_lossy(&bytes);
            let message = if text.contains("Proxy service is currently disabled") {
                "Antigravity-Manager 代理服务尚未开启。请在 Antigravity-Manager 软件中点击开启“代理服务 (Enable Proxy)”开关。".to_string()
            } else if status.is_success() {
                format!("{provider} returned invalid JSON: {text}")
            } else {
                format!("{provider} HTTP {status}: {text}")
            };
            Err(ProviderError::invalid(message))
        }
    }
}

async fn checked_empty(provider: &str, response: reqwest::Response) -> ProviderResult<()> {
    let status = response.status();
    if status.is_success() || status.as_u16() == 404 {
        return Ok(());
    }
    let retry_after_seconds = retry_after(response.headers());
    let payload = response.json::<Value>().await.unwrap_or(Value::Null);
    Err(provider_http_error(
        provider,
        status.as_u16(),
        &payload,
        retry_after_seconds,
    ))
}

fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
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
    let lowered = detail.to_ascii_lowercase();
    let kind = match status {
        401 | 403 => ProviderErrorKind::Unauthorized,
        429 => ProviderErrorKind::RateLimited,
        404 => ProviderErrorKind::StaleRemoteResource,
        400 if lowered.contains("file")
            || lowered.contains("not found")
            || lowered.contains("no such") =>
        {
            ProviderErrorKind::StaleRemoteResource
        }
        _ => ProviderErrorKind::Transport,
    };
    let message = if detail.contains("Proxy service is currently disabled") {
        "Antigravity-Manager 代理服务尚未开启。请在 Antigravity-Manager 软件中点击开启“代理服务 (Enable Proxy)”开关。".to_string()
    } else {
        format!("{provider} request failed ({status}): {detail}")
    };
    ProviderError {
        kind,
        message,
        retry_after_seconds,
        orphaned_resource: None,
    }
}

fn sse_data(line: &str) -> Option<&str> {
    line.strip_prefix("data:").map(str::trim)
}

#[derive(Default)]
struct LineBuffer {
    bytes: Vec<u8>,
}

impl LineBuffer {
    fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
    }

    fn next_line(&mut self) -> ProviderResult<Option<String>> {
        let Some(position) = self.bytes.iter().position(|byte| *byte == b'\n') else {
            return Ok(None);
        };
        let mut line = self.bytes.drain(..=position).collect::<Vec<_>>();
        line.pop();
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        String::from_utf8(line)
            .map(Some)
            .map_err(|_| ProviderError::invalid("Chat stream contains invalid UTF-8"))
    }

    fn remainder(&mut self) -> ProviderResult<Option<String>> {
        if self.bytes.is_empty() {
            return Ok(None);
        }
        let line = std::mem::take(&mut self.bytes);
        String::from_utf8(line)
            .map(|line| Some(line.trim_end_matches(['\r', '\n']).to_string()))
            .map_err(|_| ProviderError::invalid("Chat stream contains invalid UTF-8"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_ports::{
        CancellationFlag, InlineImageInput, PaperInteractionKind, PaperStreamRequest,
    };
    use serde_json::{json, Value};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;
    use tokio::sync::mpsc;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn pdf_fixture() -> (TempDir, PathBuf) {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("paper.pdf");
        fs::write(&path, b"%PDF-1.4\n%%EOF").expect("pdf");
        (directory, path)
    }

    fn adapter(server: &MockServer) -> ChatCompletionsAdapter {
        ChatCompletionsAdapter::new("openai_compatible", "test-key", server.uri()).expect("adapter")
    }

    fn paper_request(
        pdf_path: PathBuf,
        remote_file_id: Option<&str>,
        previous_interaction_id: Option<&str>,
    ) -> PaperInteractionRequest {
        PaperInteractionRequest {
            model: "gpt-4.1".to_string(),
            context_epoch: "revision:model".to_string(),
            pdf_path,
            display_name: "Paper.pdf".to_string(),
            remote_file_id: remote_file_id.map(str::to_string),
            previous_interaction_id: previous_interaction_id.map(str::to_string),
            system_instruction: "Always cite exact pages.".to_string(),
            user_input: "Summarize the paper.".to_string(),
            response_schema: None,
            inline_images: Vec::new(),
            kind: PaperInteractionKind::default(),
        }
    }

    fn chat_ok() -> Value {
        json!({
            "id": "chatcmpl-1",
            "choices": [{
                "message": {"role": "assistant", "content": "Grounded answer"}
            }],
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 20,
                "prompt_tokens_details": {"cached_tokens": 40},
                "completion_tokens_details": {"reasoning_tokens": 3}
            }
        })
    }

    async fn mount_chat_json(server: &MockServer, body: Value) {
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(server)
            .await;
    }

    fn request_header(request: &wiremock::Request, name: &str) -> String {
        request
            .headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("")
            .to_string()
    }

    async fn received(server: &MockServer) -> Vec<wiremock::Request> {
        server.received_requests().await.expect("requests")
    }

    fn path_ends_with(request: &wiremock::Request, suffix: &str) -> bool {
        request.url.path().ends_with(suffix)
    }

    fn chat_request(requests: &[wiremock::Request]) -> &wiremock::Request {
        requests
            .iter()
            .find(|request| path_ends_with(request, "/chat/completions"))
            .expect("chat completions request")
    }

    fn chat_json(requests: &[wiremock::Request]) -> Value {
        serde_json::from_slice(&chat_request(requests).body).expect("JSON body")
    }

    #[tokio::test]
    async fn chat_completions_uploads_pdf_then_sends_chat() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/files"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"id": "file-abc"})))
            .mount(&server)
            .await;
        mount_chat_json(&server, chat_ok()).await;
        let adapter = adapter(&server);
        let (_directory, pdf_path) = pdf_fixture();
        let outcome = adapter
            .interact(paper_request(pdf_path, None, None))
            .await
            .expect("interaction");

        assert_eq!(outcome.provider_file_id, "file-abc");
        assert_eq!(outcome.provider_node_id, "chatcmpl-1");
        assert_eq!(outcome.text, "Grounded answer");
        assert_eq!(outcome.receipt.provider, "openai_compatible");
        assert_eq!(outcome.receipt.file_reuse, Some(false));
        assert_eq!(outcome.receipt.session_resume, Some(false));
        assert_eq!(outcome.receipt.paper_root_branch, Some(true));
        assert_eq!(outcome.receipt.input_tokens, Some(100));
        assert_eq!(outcome.receipt.cached_input_tokens, Some(40));
        assert_eq!(outcome.receipt.output_tokens, Some(20));
        assert_eq!(outcome.receipt.reasoning_tokens, Some(3));
        assert_eq!(outcome.receipt.estimated_cost, None);

        let requests = received(&server).await;
        let files = requests
            .iter()
            .find(|request| path_ends_with(request, "/files"))
            .expect("files upload");
        let files_body = String::from_utf8_lossy(&files.body);
        assert_eq!(request_header(files, "authorization"), "Bearer test-key");
        assert!(files_body.contains("user_data"), "{files_body}");

        let chat = chat_request(&requests);
        assert_eq!(request_header(chat, "authorization"), "Bearer test-key");
        let body = chat_json(&requests);
        assert_eq!(body["messages"][0]["role"], "system");
        assert_eq!(body["messages"][0]["content"], "Always cite exact pages.");
        assert_eq!(body["messages"][1]["role"], "user");
        assert_eq!(body["messages"][1]["content"][0]["type"], "file");
        assert_eq!(
            body["messages"][1]["content"][0]["file"]["file_id"],
            "file-abc"
        );
        assert_eq!(body["messages"][1]["content"][1]["type"], "text");
        assert_eq!(
            body["messages"][1]["content"][1]["text"],
            "Summarize the paper."
        );
        assert!(body.get("previous_interaction_id").is_none());
    }

    #[tokio::test]
    async fn chat_completions_falls_back_to_inline_pdf_when_files_404() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/files"))
            .respond_with(ResponseTemplate::new(404).set_body_json(json!({
                "error": {"message": "not found"}
            })))
            .mount(&server)
            .await;
        mount_chat_json(&server, chat_ok()).await;
        let adapter = adapter(&server);
        let (_directory, pdf_path) = pdf_fixture();
        let outcome = adapter
            .interact(paper_request(pdf_path, None, None))
            .await
            .expect("interaction");

        assert_eq!(outcome.provider_file_id, "inline-pdf");
        assert_eq!(outcome.receipt.file_reuse, Some(false));

        let requests = received(&server).await;
        assert!(requests
            .iter()
            .any(|request| path_ends_with(request, "/files")));
        let body = chat_json(&requests);
        let file = &body["messages"][1]["content"][0]["file"];
        let file_data = file["file_data"].as_str().unwrap_or("");
        assert_eq!(file["filename"], "Paper.pdf");
        assert!(
            file_data.contains("base64") && file_data.contains("JVBER"),
            "{file_data}"
        );
        let encoded = file_data
            .rsplit(',')
            .next()
            .unwrap_or(file_data)
            .to_string();
        let bytes = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            encoded.as_bytes(),
        )
        .expect("pdf base64");
        assert_eq!(bytes, b"%PDF-1.4\n%%EOF");
    }

    #[tokio::test]
    async fn chat_completions_ignores_previous_interaction_id_in_body() {
        let server = MockServer::start().await;
        mount_chat_json(&server, chat_ok()).await;
        let adapter = adapter(&server);
        let (_directory, pdf_path) = pdf_fixture();
        let outcome = adapter
            .interact(paper_request(
                pdf_path,
                Some("file-abc"),
                Some("chatcmpl-prev"),
            ))
            .await
            .expect("interaction");

        assert_eq!(outcome.receipt.session_resume, Some(false));
        assert_eq!(outcome.receipt.paper_root_branch, Some(false));
        assert_eq!(outcome.receipt.file_reuse, Some(true));

        let body = chat_json(&received(&server).await);
        let serialized = body.to_string();
        assert!(body.get("previous_interaction_id").is_none());
        assert!(body.get("previous_response_id").is_none());
        assert!(
            !serialized.contains("previous_interaction_id"),
            "{serialized}"
        );
        assert!(!serialized.contains("previous_response_id"), "{serialized}");
        assert!(!serialized.contains("chatcmpl-prev"), "{serialized}");
    }

    #[tokio::test]
    async fn chat_completions_stream_emits_text_deltas() {
        let server = MockServer::start().await;
        let stream = concat!(
            "data: {\"id\":\"chatcmpl-1\",\"choices\":[{\"delta\":{\"content\":\"Grounded \"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"answer\"}}]}\n\n",
            "data: [DONE]\n\n"
        );
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_raw(stream, "text/event-stream"),
            )
            .mount(&server)
            .await;
        let adapter = adapter(&server);
        let (_directory, pdf_path) = pdf_fixture();
        let (deltas, mut receiver) = mpsc::unbounded_channel();
        let outcome = adapter
            .interact_stream(PaperStreamRequest {
                interaction: paper_request(pdf_path, Some("file-abc"), None),
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
        assert_eq!(outcome.provider_node_id, "chatcmpl-1");

        let body = chat_json(&received(&server).await);
        assert_eq!(body.get("stream"), Some(&Value::Bool(true)));
    }

    #[tokio::test]
    async fn chat_completions_structured_output_uses_json_schema() {
        let server = MockServer::start().await;
        mount_chat_json(&server, chat_ok()).await;
        let adapter = adapter(&server);
        let (_directory, pdf_path) = pdf_fixture();
        let mut request = paper_request(pdf_path, Some("file-abc"), None);
        request.response_schema = Some(json!({
            "name": "orientation_pack",
            "strict": true,
            "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["ok"],
                "properties": {
                    "ok": {"type": "boolean"}
                }
            }
        }));
        adapter.interact(request).await.expect("interaction");

        let body = chat_json(&received(&server).await);
        let format = &body["response_format"];
        assert_eq!(format["type"], "json_schema");
        assert_eq!(format["json_schema"]["name"], "read_desktop");
        assert_eq!(format["json_schema"]["strict"], true);
        assert_eq!(format["json_schema"]["schema"]["type"], "object");
        assert!(format["json_schema"]["schema"].get("properties").is_some());
    }

    #[tokio::test]
    async fn chat_completions_includes_image_url_parts() {
        let server = MockServer::start().await;
        mount_chat_json(&server, chat_ok()).await;
        let adapter = adapter(&server);
        let (_directory, pdf_path) = pdf_fixture();
        let mut request = paper_request(pdf_path, Some("file-abc"), None);
        request.inline_images = vec![InlineImageInput {
            mime_type: "image/png".to_string(),
            base64_data: "aaa".to_string(),
        }];
        adapter.interact(request).await.expect("interaction");

        let body = chat_json(&received(&server).await);
        let content = body["messages"][1]["content"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert_eq!(content[0]["type"], "file");
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(content[1]["image_url"]["url"], "data:image/png;base64,aaa");
        assert_eq!(content[2]["type"], "text");
    }

    #[tokio::test]
    async fn chat_completions_maps_file_not_found_400_to_stale_remote_resource() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": {"message": "No such File object: file-abc"}
            })))
            .mount(&server)
            .await;
        let adapter = adapter(&server);
        let (_directory, pdf_path) = pdf_fixture();
        let error = adapter
            .interact(paper_request(pdf_path, Some("file-abc"), None))
            .await
            .expect_err("file-not-found 400");
        assert_eq!(error.kind, ProviderErrorKind::StaleRemoteResource);
        assert!(
            error.message.contains("file-abc"),
            "unexpected error: {}",
            error.message
        );
    }

    #[tokio::test]
    async fn chat_completions_delete_remote_file() {
        let server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/files/file-abc"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let adapter = adapter(&server);
        adapter
            .delete_remote(&RemoteResource {
                provider: "openai_compatible".to_string(),
                kind: "file".to_string(),
                id: "file-abc".to_string(),
            })
            .await
            .expect("delete");

        let requests = received(&server).await;
        assert!(
            requests.iter().any(|request| {
                request.method.as_str() == "DELETE" && path_ends_with(request, "/files/file-abc")
            }),
            "expected DELETE /files/file-abc, got {:?}",
            requests
                .iter()
                .map(|request| format!("{} {}", request.method, request.url.path()))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            request_header(
                requests
                    .iter()
                    .find(|request| request.method.as_str() == "DELETE")
                    .expect("delete request"),
                "authorization"
            ),
            "Bearer test-key"
        );
    }

    #[test]
    fn paper_probe_fingerprint_is_stable_and_changes_with_key() {
        let first = paper_probe_fingerprint("https://api.openai.com/v1", "sk-test", "gpt-4.1");
        let again = paper_probe_fingerprint("https://api.openai.com/v1", "sk-test", "gpt-4.1");
        assert_eq!(first, again);
        assert_eq!(first.len(), 64);
        assert!(
            first.chars().all(|character| character.is_ascii_hexdigit()),
            "{first}"
        );
        assert_ne!(
            first,
            paper_probe_fingerprint("https://api.openai.com/v1", "sk-other", "gpt-4.1")
        );
        assert_ne!(
            first,
            paper_probe_fingerprint("https://api.x.ai/v1", "sk-test", "gpt-4.1")
        );
        assert_ne!(
            first,
            paper_probe_fingerprint("https://api.openai.com/v1", "sk-test", "grok-3")
        );
        assert_eq!(
            first,
            paper_probe_fingerprint(" https://api.openai.com/v1/ ", " sk-test ", " gpt-4.1 ")
        );
    }

    #[tokio::test]
    async fn paper_probe_passes_when_nonce_letter_r_and_schema_ok() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_string_contains("image_url"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-probe-1",
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "RD-PDF-NONCE-7F3A R"
                    }
                }]
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(body_string_contains("json_schema"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-probe-2",
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": "{\"ok\":true}"
                    }
                }]
            })))
            .mount(&server)
            .await;

        let adapter = adapter(&server);
        let outcome = run_paper_probe(&adapter, "gpt-4.1")
            .await
            .expect("probe result");
        assert!(outcome.passed, "{outcome:?}");
        assert_eq!(outcome.failure, None);

        let chats: Vec<Value> = received(&server)
            .await
            .iter()
            .filter(|request| path_ends_with(request, "/chat/completions"))
            .map(|request| serde_json::from_slice(&request.body).expect("chat json"))
            .collect();
        assert_eq!(chats.len(), 2, "{chats:?}");
        assert!(chats[0].get("response_format").is_none());
        assert_eq!(chats[0]["messages"][1]["content"][1]["type"], "image_url");
        assert!(
            !chats[0].to_string().contains("RD-PDF-NONCE-7F3A"),
            "nonce must live in the PDF, not the prompt: {}",
            chats[0]
        );
        assert_eq!(chats[1]["response_format"]["type"], "json_schema");
        assert_eq!(
            chats[1]["response_format"]["json_schema"]["schema"],
            json!({
                "type": "object",
                "properties": {"ok": {"type": "boolean"}},
                "required": ["ok"],
                "additionalProperties": false
            })
        );
        assert_eq!(chats[1]["messages"][1]["content"], "Return ok true.");
    }

    #[tokio::test]
    async fn paper_probe_fails_on_attachment_search_tool_call() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "chatcmpl-probe-tools",
                "choices": [{
                    "message": {
                        "role": "assistant",
                        "content": null,
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "attachment_search",
                                "arguments": "{}"
                            }
                        }]
                    }
                }]
            })))
            .mount(&server)
            .await;

        let adapter = adapter(&server);
        let outcome = run_paper_probe(&adapter, "gpt-4.1")
            .await
            .expect("probe result");
        assert!(!outcome.passed, "{outcome:?}");
        let failure = outcome.failure.as_deref().unwrap_or("");
        assert!(failure.contains("检索式 PDF 不能作为论文根"), "{failure}");
        assert!(!failure.contains("test-key"), "{failure}");

        let chats = received(&server)
            .await
            .iter()
            .filter(|request| path_ends_with(request, "/chat/completions"))
            .count();
        assert_eq!(chats, 1, "schema round must not run after a tool-call fail");
    }

    #[tokio::test]
    async fn list_openai_models_returns_ids() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": [
                    {"id": "gpt-4.1"},
                    {"id": "gpt-4o-mini"}
                ]
            })))
            .mount(&server)
            .await;

        let adapter = adapter(&server);
        let models = list_openai_models(&adapter).await.expect("models");
        assert_eq!(
            models
                .iter()
                .map(|model| model.id.as_str())
                .collect::<Vec<_>>(),
            ["gpt-4.1", "gpt-4o-mini"]
        );
        assert!(models.iter().all(|model| model.supports_generate_content));
        assert!(models
            .iter()
            .all(|model| !model.supports_native_pdf && !model.supports_interactions));

        let request = received(&server)
            .await
            .into_iter()
            .find(|request| request.method.as_str() == "GET" && path_ends_with(request, "/models"))
            .expect("GET /models");
        assert_eq!(request_header(&request, "authorization"), "Bearer test-key");
    }

    #[test]
    fn apply_paper_probe_to_models_marks_and_inserts_paper_model() {
        let listed = apply_paper_probe_to_models(
            vec![openai_model_option("gpt-4o-mini", false)],
            "gpt-4.1",
            true,
        );
        assert_eq!(listed.len(), 2);
        let probed = listed
            .iter()
            .find(|model| model.id == "gpt-4.1")
            .expect("inserted paper model");
        assert!(probed.supports_native_pdf);
        assert!(probed.supports_interactions);
        let other = listed
            .iter()
            .find(|model| model.id == "gpt-4o-mini")
            .expect("listed model");
        assert!(!other.supports_native_pdf);
        assert!(!other.supports_interactions);
    }

    #[tokio::test]
    async fn gemini_proxy_inlines_pdf_as_image_url_without_hitting_files_endpoint() {
        let server = MockServer::start().await;
        mount_chat_json(&server, chat_ok()).await;
        let adapter = ChatCompletionsAdapter::new("gemini_proxy", "test-key", &server.uri())
            .expect("adapter");
        let (_directory, pdf_path) = pdf_fixture();
        let outcome = adapter
            .interact(paper_request(pdf_path, None, None))
            .await
            .expect("interaction");

        assert_eq!(outcome.provider_file_id, "inline-pdf");
        let requests = received(&server).await;
        assert!(
            !requests.iter().any(|r| path_ends_with(r, "/files")),
            "Gemini Proxy should not call /files endpoint"
        );

        let body = chat_json(&requests);
        let content = body["messages"][1]["content"]
            .as_array()
            .expect("content array");
        assert_eq!(content[0]["type"], "image_url");
        let url = content[0]["image_url"]["url"].as_str().expect("url");
        assert!(url.starts_with("data:application/pdf;base64,"));
    }

    #[tokio::test]
    async fn gemini_proxy_injects_json_schema_into_system_instruction_and_uses_json_object() {
        let server = MockServer::start().await;
        mount_chat_json(&server, chat_ok()).await;
        let adapter = ChatCompletionsAdapter::new("gemini_proxy", "test-key", &server.uri())
            .expect("adapter");
        let (_directory, pdf_path) = pdf_fixture();
        let mut request = paper_request(pdf_path, None, None);
        request.response_schema = Some(json!({
            "type": "object",
            "required": ["summary"],
            "properties": {
                "summary": {"type": "string"}
            }
        }));
        adapter.interact(request).await.expect("interaction");

        let body = chat_json(&received(&server).await);
        let system_msg = body["messages"][0]["content"]
            .as_str()
            .expect("system message");
        assert!(system_msg.contains("[IMPORTANT: You MUST respond with a valid JSON object"));
        assert!(system_msg.contains("summary"));
        assert_eq!(body["response_format"]["type"], "json_object");
    }
}
