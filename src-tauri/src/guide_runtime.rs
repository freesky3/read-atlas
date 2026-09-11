//! Durable V2 memo calls and per-slot usage receipts; no job publication here.
use crate::guide_memo::GuideMemo;
use crate::guide_validate::GuideCatalogBlock;
use crate::*;

// Keep a completed paid outcome available for journaling before honoring a
// cancellation that arrived during the call. Pre-call cancellation still gates every call.
pub(crate) struct GuideJobPort<'a>(pub &'a JobPaperModelAdapter);
#[async_trait::async_trait]
impl PaperModelPort for GuideJobPort<'_> {
    fn capabilities(&self, model: &str) -> provider_ports::PaperModelCapabilities {
        self.0.inner.capabilities(model)
    }
    async fn interact(
        &self,
        r: PaperInteractionRequest,
    ) -> Result<PaperInteractionOutcome, ProviderError> {
        self.0.before_provider_call()?;
        self.0.inner.interact(r).await
    }
    async fn interact_text(
        &self,
        r: TextInteractionRequest,
    ) -> Result<TextInteractionOutcome, ProviderError> {
        self.0.before_provider_call()?;
        self.0.inner.interact_text(r).await
    }
    async fn interact_stream(
        &self,
        r: provider_ports::PaperStreamRequest,
    ) -> Result<PaperInteractionOutcome, ProviderError> {
        self.0.before_provider_call()?;
        self.0.inner.interact_stream(r).await
    }
    async fn delete_remote(&self, r: &provider_ports::RemoteResource) -> Result<(), ProviderError> {
        self.0.inner.delete_remote(r).await
    }
}

pub(crate) fn record_receipts(
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
    checkpoint: &Value,
) -> Result<(), ProviderError> {
    let Some(calls) = checkpoint.get("calls").and_then(Value::as_object) else {
        return Ok(());
    };
    let route_id = route.frozen().route_id().database_value();
    let mut conn = open_db(&runtime.root).map_err(ProviderError::local_state)?;
    let tx = conn
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(|e| ProviderError::local_state(e.to_string()))?;
    for (slot, call) in calls {
        let Some(raw) = call.get("response") else {
            continue;
        };
        let receipt: UsageEnvelope = serde_json::from_value(raw["receipt"].clone())
            .map_err(|e| ProviderError::local_state(e.to_string()))?;
        let operation = format!("guide:{}:{slot}", job.id);
        let exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM usage_receipts WHERE operation_id=?1 AND provider_route_id=?2)",params![operation,route_id],|r|r.get(0)).map_err(|e|ProviderError::local_state(e.to_string()))?;
        if exists {
            continue;
        }
        tx.execute("INSERT INTO usage_receipts(id,operation_id,job_id,provider,model,context_epoch,provider_route_id,input_tokens,cached_input_tokens,uncached_input_tokens,output_tokens,reasoning_tokens,latency_ms,estimated_cost,file_reuse,session_resume,paper_root_branch,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18)",
            params![Uuid::new_v4().to_string(),operation,job.id,receipt.provider,receipt.model,receipt.context_epoch,route_id,receipt.input_tokens,receipt.cached_input_tokens,receipt.uncached_input_tokens,receipt.output_tokens,receipt.reasoning_tokens,receipt.latency_ms,receipt.estimated_cost,receipt.file_reuse.map(i64::from),receipt.session_resume.map(i64::from),receipt.paper_root_branch.map(i64::from),now()]).map_err(|e|ProviderError::local_state(e.to_string()))?;
        if let Some(node) = raw["providerNodeId"].as_str().filter(|s| !s.is_empty()) {
            tx.execute("INSERT INTO provider_nodes(id,provider,model,context_epoch,provider_route_id,provider_node_id,parent_id,state,created_at) VALUES (?1,?2,?3,?4,?5,?6,NULL,'complete',?7)",params![Uuid::new_v4().to_string(),route.frozen().provider_kind().as_str(),route.frozen().models().paper(),receipt.context_epoch.as_deref().unwrap_or(""),route_id,node,now()]).map_err(|e|ProviderError::local_state(e.to_string()))?;
        }
    }
    tx.commit()
        .map_err(|e| ProviderError::local_state(e.to_string()))
}

pub(crate) async fn memo(
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
    port: &dyn PaperModelPort,
    revision: &RevisionRecord,
    catalog: &[GuideCatalogBlock],
    checkpoint: &mut Value,
) -> Result<Value, ProviderError> {
    let guide = &runtime.guide_module;
    let page_count = revision
        .page_count
        .unwrap_or_else(|| catalog.iter().map(|b| b.page_number).max().unwrap_or(1));
    let parse = |text: &str| -> Result<GuideMemo, String> {
        let memo = crate::guide_memo::parse_guide_memo(text)?;
        crate::guide_memo::validate_memo_locations(&memo, page_count, catalog)?;
        Ok(memo)
    };
    record_receipts(runtime, job, route, checkpoint)?;
    if let Some(saved) = checkpoint.get("context") {
        parse(&saved.to_string()).map_err(ProviderError::invalid)?;
        return Ok(saved.clone());
    }
    let key = job.payload["memoCacheKey"].as_str();
    if let Some(key) = key {
        if let Some(cached) = guide.load_memo(key).map_err(ProviderError::local_state)? {
            if parse(&cached.to_string()).is_ok() {
                return Ok(cached);
            }
        }
    }
    let root_prompt = job
        .payload
        .pointer("/prompts/paperRoot")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::invalid("旧 V2 任务缺少冻结根提示词，请重新计划"))?;
    let prompt = job
        .payload
        .pointer("/prompts/context")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::invalid("缺少冻结备忘提示词"))?;
    let model = route.frozen().models().paper();
    let route_id = route.frozen().route_id().database_value();
    let module = ReadingArtifactModule::open(&runtime.root).map_err(ProviderError::local_state)?;
    let facts = reading_artifact_module::DocumentFacts {
        paper_id: guide
            .paper_id_for_revision(&revision.id)
            .map_err(ProviderError::local_state)?,
        revision_id: revision.id.clone(),
        revision_sha256: revision.sha256.clone(),
        title: revision.title.clone(),
        pdf_path: revision.pdf_path.clone(),
    };
    let mut previous = checkpoint["rawMemo"].as_str().map(String::from);
    let mut error = String::new();
    if let Some(raw) = &previous {
        match parse(raw) {
            Ok(m) => {
                return serde_json::to_value(m)
                    .map_err(|e| ProviderError::local_state(e.to_string()))
            }
            Err(e) => error = e,
        }
    }
    for phase in ["initial", "repair"] {
        if phase == "initial" && previous.is_some() {
            continue;
        }
        let slot = format!("memo:{phase}");
        let response:Result<PaperInteractionOutcome,_>=crate::guide_generation::call_once(checkpoint,&slot,|c|runtime.job_module.save_checkpoint(&job.id,"understanding",c),async {
            let source=module.ensure_root_for_route(port,&facts,&route_id,route.frozen().provider_kind().as_str(),model,Some(root_prompt)).await?;
            let locators:Vec<_>=catalog.iter().map(|b|json!({"blockId":b.id,"pageNumber":b.page_number,"blockIndex":b.block_index,"blockType":b.block_type})).collect();
            port.interact(PaperInteractionRequest{
                model:model.into(),context_epoch:source.context_epoch,pdf_path:revision.pdf_path.clone(),display_name:revision.title.clone(),
                remote_file_id:Some(source.remote_file_id),previous_interaction_id:Some(source.remote_node_id),system_instruction:prompt.into(),
                user_input:reader_context::prepend_reader_context(&json!({"task":"build_reading_memo","catalog":locators,"language":job.payload.get("outputLanguage").and_then(Value::as_str).unwrap_or("zh-CN"),"previousOutput":previous,"repairHint":if phase=="repair"{Some(&error)}else{None}}).to_string(),frozen_reader_context(&job.payload).as_deref()),
                response_schema:Some(crate::guide_protocol::memo_schema()),inline_images:Vec::new(),kind:PaperInteractionKind::Artifact,
            }).await
        }).await;
        record_receipts(runtime, job, route, checkpoint)?;
        let response = response?;
        previous = Some(response.text.clone());
        match parse(&response.text) {
            Ok(memo) => {
                let value = serde_json::to_value(memo)
                    .map_err(|e| ProviderError::local_state(e.to_string()))?;
                checkpoint["context"] = value.clone();
                runtime
                    .job_module
                    .save_checkpoint(&job.id, "understanding", checkpoint)
                    .map_err(ProviderError::local_state)?;
                if let Some(key) = key {
                    guide
                        .save_memo(
                            key,
                            &revision.id,
                            job.payload["ocrRevisionId"].as_str().unwrap_or(""),
                            job.payload["documentKind"].as_str().unwrap_or("paper"),
                            &value,
                            Some(&response.text),
                        )
                        .map_err(ProviderError::local_state)?;
                }
                return Ok(value);
            }
            Err(e) => error = e,
        }
    }
    Err(ProviderError::invalid(format!(
        "备忘在一次修复后仍无效：{error}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_settings::{
        ProviderInstance, ProviderKind, StoredModelSettings, MODEL_SETTINGS_SCHEMA,
    };
    use crate::provider_routing::*;
    use std::sync::Mutex;
    #[derive(Default)]
    struct Fake(Mutex<Vec<PaperInteractionRequest>>);
    #[async_trait::async_trait]
    impl PaperModelPort for Fake {
        fn capabilities(&self, _: &str) -> provider_ports::PaperModelCapabilities {
            provider_ports::PaperModelCapabilities {
                native_pdf: true,
                interactions: true,
                structured_output: true,
                streaming: false,
            }
        }
        async fn interact(
            &self,
            r: PaperInteractionRequest,
        ) -> Result<PaperInteractionOutcome, ProviderError> {
            let mut calls = self.0.lock().unwrap();
            let n = calls.len() + 1;
            let text = if r.kind == PaperInteractionKind::Root {
                json!({"acknowledged":true}).to_string()
            } else if n == 2 {
                "broken JSON".into()
            } else {
                json!({"schemaVersion":"reading-guide-memo-v1","documentFocus":"可靠前提","spans":[],"observations":[],"connections":[],"pageHints":[],"limitations":[]}).to_string()
            };
            let response = PaperInteractionOutcome {
                text,
                provider_node_id: format!("node-{n}"),
                provider_file_id: "file-1".into(),
                receipt: UsageEnvelope {
                    provider: "gemini".into(),
                    model: r.model.clone(),
                    context_epoch: Some(r.context_epoch.clone()),
                    ..Default::default()
                },
            };
            calls.push(r);
            Ok(response)
        }
        async fn interact_text(
            &self,
            _: TextInteractionRequest,
        ) -> Result<TextInteractionOutcome, ProviderError> {
            panic!("memo must use PDF source branch")
        }
        async fn delete_remote(
            &self,
            _: &provider_ports::RemoteResource,
        ) -> Result<(), ProviderError> {
            Ok(())
        }
    }
    struct Factory(Arc<Fake>);
    impl PaperAdapterFactory for Factory {
        fn open(
            &self,
            _: &ProviderKind,
            _: &str,
            _: Option<&str>,
        ) -> Result<Arc<dyn PaperModelPort>, ProviderRoutingError> {
            Ok(self.0.clone())
        }
    }
    struct Key;
    impl ProviderCredentialPort for Key {
        fn read_exact(
            &self,
            _: &ProviderInstanceId,
        ) -> Result<Option<String>, ProviderRoutingError> {
            Ok(Some("test-only-no-network".into()))
        }
    }
    #[tokio::test]
    async fn memo_uses_pdf_root_repairs_once_and_replays_paid_outcomes_with_one_receipt_each() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = workspace_lifecycle::open_test_runtime(dir.path(), 1);
        let conn = open_db(dir.path()).unwrap();
        conn.execute_batch("INSERT INTO collections(id,name,relative_path,created_at,updated_at) VALUES ('c','c','', 't','t'); INSERT INTO papers(id,collection_id,file_name,relative_path,created_at,updated_at) VALUES ('p','c','test.pdf','test.pdf','t','t'); INSERT INTO document_revisions(id,paper_id,sha256,byte_size,page_count,source_relative_path,created_at) VALUES ('r','p','hash',4,1,'test.pdf','t');").unwrap();
        fs::write(dir.path().join("test.pdf"), b"%PDF").unwrap();
        let fake = Arc::new(Fake::default());
        let factory = Factory(fake.clone());
        let id = Uuid::new_v4().to_string();
        let settings = StoredModelSettings {
            schema_version: MODEL_SETTINGS_SCHEMA,
            current_provider_id: Some(id.clone()),
            providers: vec![ProviderInstance {
                id,
                name: "test".into(),
                kind: ProviderKind::Gemini,
                base_url: None,
                paper_model: "gemini-test".into(),
                translation_model: "gemini-test".into(),
                models: vec![GeminiModelOption {
                    id: "gemini-test".into(),
                    display_name: "fake".into(),
                    description: String::new(),
                    input_token_limit: Some(32000),
                    output_token_limit: Some(8000),
                    supports_generate_content: true,
                    supports_native_pdf: true,
                    supports_interactions: true,
                }],
                models_fetched_at: None,
                connection_verified_at: Some("t".into()),
                paper_probe: None,
                sort_order: 0,
            }],
        };
        let route = match ProviderRouting::with_factory(&settings, &Key, &factory)
            .capture(
                ProviderSelection::Current,
                FrozenModels::new("gemini-test", None::<String>).unwrap(),
                ModelRole::Paper,
            )
            .unwrap()
        {
            ProviderRouteDecision::Ready(r) => r,
            _ => panic!("fake route should bind"),
        };
        let payload = json!({"memoCacheKey":"cache-key","ocrRevisionId":"ocr","documentKind":"paper","readerContext":"只作为读者偏好","prompts":{"paperRoot":"仅保存 PDF，不总结","context":"读后备忘"}});
        let job = runtime
            .job_module
            .enqueue_record(
                JobSpec {
                    kind: "reading_guide".into(),
                    provider: None,
                    paper_id: Some("p".into()),
                    revision_id: Some("r".into()),
                    root_key: None,
                    artifact_key: None,
                    dedupe_key: "memo-test".into(),
                    priority: 1,
                    payload,
                },
                JobExecutionRoute::Paper(route.freeze()),
            )
            .unwrap()
            .job
            .project();
        runtime.job_module.claim_next_record().unwrap().unwrap();
        let mut revision = revision_record(&conn, "r").unwrap();
        revision.pdf_path = dir.path().join("test.pdf");
        let catalog = vec![GuideCatalogBlock {
            id: "b1".into(),
            page_number: 1,
            block_index: 0,
            block_type: "Text".into(),
            bbox: [0, 0, 1, 1],
            excerpt: Some("原文".into()),
        }];
        let mut checkpoint = json!({});
        let value = memo(
            &runtime,
            &job,
            &route,
            fake.as_ref(),
            &revision,
            &catalog,
            &mut checkpoint,
        )
        .await
        .unwrap();
        assert_eq!(fake.0.lock().unwrap().len(), 3);
        {
            let calls = fake.0.lock().unwrap();
            assert!(!calls[0].user_input.contains("只作为读者偏好"));
            assert_eq!(calls[1].previous_interaction_id.as_deref(), Some("node-1"));
            assert_eq!(calls[2].previous_interaction_id.as_deref(), Some("node-1"));
            assert!(calls[2].user_input.contains("broken JSON"));
        }
        conn.execute("DELETE FROM reading_guide_memos", []).unwrap();
        checkpoint.as_object_mut().unwrap().remove("context");
        let again = memo(
            &runtime,
            &job,
            &route,
            fake.as_ref(),
            &revision,
            &catalog,
            &mut checkpoint,
        )
        .await
        .unwrap();
        assert_eq!(again, value);
        assert_eq!(fake.0.lock().unwrap().len(), 3);
        record_receipts(&runtime, &job, &route, &checkpoint).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM usage_receipts WHERE job_id=?1",
                params![job.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
        let cached = memo(
            &runtime,
            &job,
            &route,
            fake.as_ref(),
            &revision,
            &catalog,
            &mut json!({}),
        )
        .await
        .unwrap();
        assert_eq!(cached, value);
        assert_eq!(fake.0.lock().unwrap().len(), 3);
    }
}
