//! Journaled map provider calls and durable per-slot accounting.
use crate::*;

// Keep a completed paid outcome available for journaling before honoring a
// cancellation that arrived during the call. Pre-call cancellation still gates every call.
pub(crate) struct OutlineJobPort<'a>(pub &'a JobPaperModelAdapter);
#[async_trait::async_trait]
impl PaperModelPort for OutlineJobPort<'_> {
    fn capabilities(&self, model: &str) -> provider_ports::PaperModelCapabilities {
        self.0.inner.capabilities(model)
    }
    async fn interact(
        &self,
        r: PaperInteractionRequest,
    ) -> Result<PaperInteractionOutcome, ProviderError> {
        self.0.cancellation.check()?;
        if r.kind == PaperInteractionKind::Root {
            let job = self
                .0
                .jobs
                .get(&self.0.job_id)
                .map_err(ProviderError::local_state)?;
            let mut checkpoint = self
                .0
                .jobs
                .get_checkpoint(&self.0.job_id)
                .map_err(ProviderError::local_state)?
                .unwrap_or_else(|| json!({}));
            if job.payload["rootCalls"].as_i64() != Some(1)
                || checkpoint["rootRequestIssued"].as_bool() == Some(true)
            {
                return Err(ProviderError::local_state(
                    "本计划没有额外的 PDF 来源根调用额度，请重新计划。",
                ));
            }
            checkpoint["rootRequestIssued"] = json!(true);
            self.0
                .jobs
                .save_checkpoint(&self.0.job_id, "root_committing", &checkpoint)
                .map_err(ProviderError::local_state)?;
        }
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

pub(crate) fn record_checkpoint_receipts(
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
        let operation = format!("outline:{}:{slot}", job.id);
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

pub(crate) fn record_receipt(
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
    remote_id: &str,
    slot: &str,
    receipt: &UsageEnvelope,
) -> Result<(), ProviderError> {
    record_checkpoint_receipts(
        runtime,
        job,
        route,
        &json!({"calls":{slot:{"response":{"providerNodeId":remote_id,"receipt":receipt}}}}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_settings::{
        ProviderInstance, ProviderKind, StoredModelSettings, MODEL_SETTINGS_SCHEMA,
    };
    use crate::provider_routing::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    struct Fake {
        calls: Mutex<Vec<PaperInteractionRequest>>,
        outputs: Mutex<VecDeque<String>>,
        cancel_after: Mutex<Option<CancellationFlag>>,
    }
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
            request: PaperInteractionRequest,
        ) -> Result<PaperInteractionOutcome, ProviderError> {
            let mut calls = self.calls.lock().unwrap();
            let index = calls.len() + 1;
            let text = if request.kind == PaperInteractionKind::Root {
                json!({"acknowledged":true}).to_string()
            } else {
                if let Some(flag) = self.cancel_after.lock().unwrap().take() {
                    flag.cancel();
                }
                self.outputs
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("unexpected extra paid call")
            };
            let result = PaperInteractionOutcome {
                text,
                provider_node_id: format!("node-{index}"),
                provider_file_id: "file".into(),
                receipt: UsageEnvelope {
                    provider: "gemini".into(),
                    model: request.model.clone(),
                    context_epoch: Some(request.context_epoch.clone()),
                    ..Default::default()
                },
            };
            calls.push(request);
            Ok(result)
        }
        async fn interact_text(
            &self,
            _: TextInteractionRequest,
        ) -> Result<TextInteractionOutcome, ProviderError> {
            panic!("map must use PDF")
        }
        async fn delete_remote(&self, _: &RemoteResource) -> Result<(), ProviderError> {
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
    struct Harness {
        _dir: tempfile::TempDir,
        runtime: Arc<WorkspaceRuntime>,
        fake: Arc<Fake>,
        route: BoundProviderRoute,
        facts: crate::reading_artifact_module::DocumentFacts,
    }
    impl Harness {
        fn new(outputs: Vec<String>) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let runtime = workspace_lifecycle::open_test_runtime(dir.path(), 1);
            let conn = open_db(dir.path()).unwrap();
            conn.execute_batch("INSERT INTO collections(id,name,relative_path,created_at,updated_at) VALUES ('c','c','','t','t');
                INSERT INTO papers(id,collection_id,file_name,relative_path,created_at,updated_at) VALUES ('p','c','test.pdf','test.pdf','t','t');
                INSERT INTO document_revisions(id,paper_id,sha256,byte_size,page_count,source_relative_path,created_at) VALUES ('r','p','hash',4,12,'test.pdf','t');
                INSERT INTO ocr_revisions(id,revision_id,status,provider,model,created_at,published_at) VALUES ('ocr','r','ready','mistral','m','t','t');
                INSERT INTO ocr_pages(id,ocr_revision_id,page_number) VALUES ('pg','ocr',1),('appendix','ocr',12);
                INSERT INTO ocr_blocks(id,ocr_page_id,block_index,block_type,text_content,content_digest,x0,y0,x1,y1) VALUES ('b1','pg',0,'paragraph','方法正文','t',0,0,100,100),('b12','appendix',0,'paragraph','附录证明','t',0,0,100,100);").unwrap();
            fs::write(dir.path().join("test.pdf"), b"%PDF").unwrap();
            let fake = Arc::new(Fake {
                calls: Mutex::new(vec![]),
                outputs: Mutex::new(outputs.into()),
                cancel_after: Mutex::new(None),
            });
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
                        input_token_limit: Some(200000),
                        output_token_limit: Some(16000),
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
            let route = match ProviderRouting::with_factory(&settings, &Key, &Factory(fake.clone()))
                .capture(
                    ProviderSelection::Current,
                    FrozenModels::new("gemini-test", None::<String>).unwrap(),
                    ModelRole::Paper,
                )
                .unwrap()
            {
                ProviderRouteDecision::Ready(r) => r,
                _ => panic!("fake route"),
            };
            let facts = crate::reading_artifact_module::DocumentFacts {
                paper_id: "p".into(),
                revision_id: "r".into(),
                revision_sha256: "hash".into(),
                title: "test".into(),
                pdf_path: dir.path().join("test.pdf"),
            };
            Self {
                _dir: dir,
                runtime,
                fake,
                route,
                facts,
            }
        }
        fn store(&self) -> crate::prompt_settings::PromptSettingsProjection {
            load_store(&self._dir.path().join("prompts.json")).unwrap()
        }
        fn overview(&self, limit: i64) -> JobProjection {
            self.overview_in(limit, crate::ui_locale::UiLocale::ZhCn)
        }
        fn overview_in(&self, limit: i64, locale: crate::ui_locale::UiLocale) -> JobProjection {
            let store = crate::prompt_settings::load_store_in(
                &self._dir.path().join("prompts.json"),
                locale,
            )
            .unwrap();
            let plan = crate::outline_plan::prepare_overview_plan(
                &self.runtime.outline_module,
                &store,
                "r",
                DocumentKind::Paper,
                &self.route.frozen().route_id().database_value(),
                "gemini-test",
                true,
                Some(limit),
                "PDF 来源根",
            )
            .unwrap();
            let payload = self
                .runtime
                .outline_module
                .load_plan(plan.plan_id.as_deref().unwrap())
                .unwrap();
            self.enqueue("outline_overview", payload)
        }
        fn local(&self) -> JobProjection {
            self.local_in(crate::ui_locale::UiLocale::ZhCn)
        }
        fn local_in(&self, locale: crate::ui_locale::UiLocale) -> JobProjection {
            let store = crate::prompt_settings::load_store_in(
                &self._dir.path().join("prompts.json"),
                locale,
            )
            .unwrap();
            let payload = crate::outline_plan::prepare_deep_dive_plan(
                &self.runtime.outline_module,
                &store,
                "r",
                "n1",
                DocumentKind::Paper,
                &self.route.frozen().route_id().database_value(),
                "gemini-test",
                "PDF 来源根",
                true,
                Some(200000),
            )
            .unwrap();
            self.enqueue("outline_deep_dive", payload)
        }
        fn enqueue(&self, kind: &str, payload: Value) -> JobProjection {
            let job = self
                .runtime
                .job_module
                .enqueue_record(
                    JobSpec {
                        kind: kind.into(),
                        provider: None,
                        paper_id: Some("p".into()),
                        revision_id: Some("r".into()),
                        root_key: None,
                        artifact_key: None,
                        dedupe_key: Uuid::new_v4().to_string(),
                        priority: 1,
                        payload,
                    },
                    JobExecutionRoute::Paper(self.route.freeze()),
                )
                .unwrap()
                .job
                .project();
            self.runtime
                .job_module
                .claim_next_record()
                .unwrap()
                .unwrap();
            job
        }
        async fn run(
            &self,
            job: &JobProjection,
            cancellation: &CancellationFlag,
            stop_before_review: bool,
        ) -> Result<(), ProviderError> {
            let (_, catalog) = self
                .runtime
                .outline_module
                .catalog_for_revision("r")
                .unwrap();
            let source =
                crate::reading_artifact_module::ReadingArtifactModule::open(&self.runtime.root)
                    .unwrap();
            let adapter = job_paper_model_adapter(
                self.route.adapter_arc(),
                self.runtime.job_module.clone(),
                job.id.clone(),
                cancellation.clone(),
            );
            let port = OutlineJobPort(&adapter);
            if let Some(checkpoint) = self.runtime.job_module.get_checkpoint(&job.id).unwrap() {
                record_checkpoint_receipts(&self.runtime, job, &self.route, &checkpoint)?;
            }
            let report = |stage: &str, _: Value, _: Option<&UsageEnvelope>| {
                if stop_before_review && stage == "reviewing" {
                    Err(ProviderError::local_state("injected interruption"))
                } else {
                    Ok(())
                }
            };
            let record = |remote: &str, slot: &str, receipt: &UsageEnvelope| {
                record_receipt(&self.runtime, job, &self.route, remote, slot, receipt)
            };
            if job.kind == "outline_overview" {
                crate::outline_generation::run_overview_v4(
                    &source,
                    &self.runtime.outline_module,
                    &self.runtime.job_module,
                    &port,
                    job,
                    &self.facts,
                    &catalog,
                    12,
                    &self.route.frozen().route_id().database_value(),
                    "gemini",
                    "gemini-test",
                    cancellation,
                    report,
                    record,
                )
                .await
            } else {
                crate::outline_generation::run_deep_dive_v4(
                    &source,
                    &self.runtime.outline_module,
                    &self.runtime.job_module,
                    &port,
                    job,
                    &self.facts,
                    &catalog,
                    12,
                    &self.route.frozen().route_id().database_value(),
                    "gemini",
                    "gemini-test",
                    cancellation,
                    report,
                    record,
                )
                .await
            }
        }
        fn receipts(&self, job: &JobProjection) -> i64 {
            open_db(&self.runtime.root)
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM usage_receipts WHERE job_id=?1",
                    params![job.id],
                    |r| r.get(0),
                )
                .unwrap()
        }
        fn count(&self) -> usize {
            self.fake.calls.lock().unwrap().len()
        }
        fn head(&self) -> Option<String> {
            self.runtime
                .outline_module
                .current_overview_id("r")
                .unwrap()
        }
    }
    #[tokio::test]
    async fn english_map_jobs_freeze_language_through_overview_review_and_local_expansion() {
        let h = Harness::new(vec![graph("draft"), graph("review"), appendix()]);
        let job = h.overview_in(200000, crate::ui_locale::UiLocale::En);
        assert_eq!(job.payload["outputLanguage"], "en");
        // Reading the other bank after submission cannot change a frozen task.
        let _ = h.store();
        h.run(&job, &CancellationFlag::default(), false)
            .await
            .unwrap();
        let local = h.local_in(crate::ui_locale::UiLocale::En);
        assert_eq!(local.payload["outputLanguage"], "en");
        h.run(&local, &CancellationFlag::default(), false)
            .await
            .unwrap();
        let calls = h.fake.calls.lock().unwrap();
        let generated: Vec<Value> = calls
            .iter()
            .filter_map(|call| serde_json::from_str::<Value>(&call.user_input).ok())
            .filter(|value| value.get("language").is_some())
            .collect();
        assert_eq!(generated.len(), 3);
        assert!(generated.iter().all(|value| value["language"] == "en"));
    }

    fn graph(title: &str) -> String {
        json!({"graph":{"title":title,"summary":"主张与适用条件","nodes":[{"nodeId":"n1","title":title,"takeaway":"保留本文条件","references":[{"blockId":"b1","pageNumber":1,"purpose":"内容出处"}]}],"edges":[],"groups":[],"gaps":[]}}).to_string()
    }
    fn appendix() -> String {
        json!({"graph":{"title":"附录中的证明","summary":"展开原认识的成立条件","nodes":[{"nodeId":"local","title":"附录证明","takeaway":"本条件下成立","references":[{"blockId":"b12","pageNumber":12,"purpose":"证明依据"}]}],"edges":[]}}).to_string()
    }
    #[tokio::test]
    async fn production_pipeline_repairs_malformed_output_and_resumes_without_repaying() {
        let h = Harness::new(vec!["broken JSON".into(), graph("候选"), graph("复核定稿")]);
        let job = h.overview(200000);
        let flag = CancellationFlag::default();
        assert!(h.run(&job, &flag, true).await.is_err());
        assert_eq!(h.count(), 3); // root, malformed draft, repair
        assert!(h.head().is_none());
        let attempt = h
            .runtime
            .outline_module
            .project_without_route("r")
            .unwrap()
            .latest_attempt
            .unwrap();
        assert_eq!(attempt.review_status.as_deref(), Some("unchecked"));
        h.run(&job, &flag, false).await.unwrap();
        assert_eq!(h.count(), 4);
        assert_eq!(h.receipts(&job), 3);
        h.run(&job, &flag, false).await.unwrap();
        assert_eq!(h.count(), 4);
        assert_eq!(h.receipts(&job), 3);
        let projection = h.runtime.outline_module.project_without_route("r").unwrap();
        assert!(projection.latest_attempt.is_none());
        assert_eq!(
            projection.head.unwrap().review_status.as_deref(),
            Some("reviewed")
        );
        let calls = h.fake.calls.lock().unwrap();
        for call in &calls[1..] {
            assert_eq!(call.previous_interaction_id.as_deref(), Some("node-1"));
        }
        assert!(calls[2].user_input.contains("broken JSON"));
        assert!(calls[3].user_input.contains("candidateGraph"));
        assert!(!calls[3].user_input.contains("\"orientation\""));
    }
    #[tokio::test]
    async fn shared_repair_budget_cannot_repair_both_stages_or_repay_on_resume() {
        let h = Harness::new(vec![
            "broken".into(),
            graph("候选"),
            "invalid review".into(),
        ]);
        let job = h.overview(200000);
        let flag = CancellationFlag::default();
        assert!(h
            .run(&job, &flag, false)
            .await
            .unwrap_err()
            .to_string()
            .contains("额度已用尽"));
        assert_eq!(h.count(), 4);
        assert!(h.head().is_none());
        assert!(h.run(&job, &flag, false).await.is_err());
        assert_eq!(h.count(), 4);
        assert_eq!(h.receipts(&job), 3);
    }
    #[tokio::test]
    async fn malformed_review_is_repaired_and_original_text_is_retained() {
        let h = Harness::new(vec![
            graph("候选"),
            "malformed review".into(),
            graph("修复后"),
        ]);
        let job = h.overview(200000);
        h.run(&job, &CancellationFlag::default(), false)
            .await
            .unwrap();
        assert_eq!(h.count(), 4);
        assert!(h.fake.calls.lock().unwrap()[3]
            .user_input
            .contains("malformed review"));
        assert_eq!(h.receipts(&job), 3);
    }
    #[tokio::test]
    async fn paid_response_survives_cancellation_without_publishing() {
        let h = Harness::new(vec![graph("候选"), graph("定稿")]);
        let job = h.overview(200000);
        let flag = CancellationFlag::default();
        *h.fake.cancel_after.lock().unwrap() = Some(flag.clone());
        assert!(h.run(&job, &flag, false).await.is_err());
        let checkpoint = h
            .runtime
            .job_module
            .get_checkpoint(&job.id)
            .unwrap()
            .unwrap();
        assert!(checkpoint["calls"]["draft"]["response"]["text"].is_string());
        assert_eq!(h.receipts(&job), 1);
        assert!(h.head().is_none());
        h.run(&job, &CancellationFlag::default(), false)
            .await
            .unwrap();
        assert_eq!(h.count(), 3);
        assert_eq!(h.receipts(&job), 2);
    }
    #[tokio::test]
    async fn frozen_source_changes_stop_before_any_paid_call() {
        let h = Harness::new(vec![]);
        let job = h.overview(200000);
        open_db(&h.runtime.root)
            .unwrap()
            .execute(
                "UPDATE document_revisions SET sha256='changed' WHERE id='r'",
                [],
            )
            .unwrap();
        assert!(h
            .run(&job, &CancellationFlag::default(), false)
            .await
            .is_err());
        assert_eq!(h.count(), 0);
    }
    #[tokio::test]
    async fn review_budget_keeps_candidate_and_never_truncates_or_sends_oversized_input() {
        let mut huge: Value = serde_json::from_str(&graph("大候选")).unwrap();
        huge["graph"]["nodes"][0]["takeaway"] = json!("文".repeat(100000));
        let h = Harness::new(vec![huge.to_string()]);
        let job = h.overview(90000);
        assert!(h
            .run(&job, &CancellationFlag::default(), false)
            .await
            .unwrap_err()
            .to_string()
            .contains("超过模型窗口"));
        assert_eq!(h.count(), 2);
        assert_eq!(h.receipts(&job), 1);
        let p = h.runtime.outline_module.project_without_route("r").unwrap();
        assert!(p.head.is_none());
        assert_eq!(
            p.latest_attempt.unwrap().graph.unwrap()["nodes"][0]["takeaway"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            100000
        );
    }
    #[tokio::test]
    async fn local_map_uses_appendix_and_old_publish_cannot_replace_newer_local_or_overview() {
        let h = Harness::new(vec![
            graph("候选"),
            graph("定稿"),
            appendix(),
            appendix(),
            graph("新候选"),
            graph("新定稿"),
        ]);
        let overview = h.overview(200000);
        h.run(&overview, &CancellationFlag::default(), false)
            .await
            .unwrap();
        let local = h.local();
        h.run(&local, &CancellationFlag::default(), false)
            .await
            .unwrap();
        let first = h
            .runtime
            .outline_module
            .deep_dive_for_node("r", "n1")
            .unwrap()
            .unwrap();
        assert_eq!(
            first.graph.unwrap()["nodes"][0]["references"][0]["pageNumber"],
            12
        );
        let local2 = h.local();
        h.run(&local2, &CancellationFlag::default(), false)
            .await
            .unwrap();
        let second = h
            .runtime
            .outline_module
            .deep_dive_for_node("r", "n1")
            .unwrap()
            .unwrap()
            .id;
        let before = h.count();
        h.run(&local, &CancellationFlag::default(), false)
            .await
            .unwrap();
        assert_eq!(h.count(), before);
        assert_eq!(
            h.runtime
                .outline_module
                .deep_dive_for_node("r", "n1")
                .unwrap()
                .unwrap()
                .id,
            second
        );
        let newer = h.overview(200000);
        h.run(&newer, &CancellationFlag::default(), false)
            .await
            .unwrap();
        let new_head = h.head();
        h.run(&overview, &CancellationFlag::default(), false)
            .await
            .unwrap();
        assert_eq!(h.head(), new_head);
        h.runtime.outline_module.delete_overview("r").unwrap();
        h.run(&overview, &CancellationFlag::default(), false)
            .await
            .unwrap();
        assert!(h.head().is_none());
    }
    #[tokio::test]
    async fn parent_change_rejects_local_before_payment() {
        let h = Harness::new(vec![graph("候选"), graph("定稿")]);
        let overview = h.overview(200000);
        h.run(&overview, &CancellationFlag::default(), false)
            .await
            .unwrap();
        let local = h.local();
        h.runtime.outline_module.delete_overview("r").unwrap();
        let before = h.count();
        assert!(h
            .run(&local, &CancellationFlag::default(), false)
            .await
            .is_err());
        assert_eq!(h.count(), before);
    }
    #[tokio::test]
    async fn failed_publication_rolls_back_and_replays_received_results_without_repaying() {
        let h = Harness::new(vec![graph("候选"), graph("定稿")]);
        let job = h.overview(200000);
        let conn = open_db(&h.runtime.root).unwrap();
        conn.execute_batch("CREATE TRIGGER reject_map_head BEFORE INSERT ON outline_heads BEGIN SELECT RAISE(ABORT,'injected publish failure'); END;").unwrap();
        assert!(h
            .run(&job, &CancellationFlag::default(), false)
            .await
            .is_err());
        assert!(h.head().is_none());
        assert!(!h
            .runtime
            .outline_module
            .published_plan(job.payload["publishId"].as_str().unwrap())
            .unwrap());
        assert_eq!(h.count(), 3);
        conn.execute_batch("DROP TRIGGER reject_map_head").unwrap();
        h.run(&job, &CancellationFlag::default(), false)
            .await
            .unwrap();
        assert_eq!(h.count(), 3);
        assert_eq!(h.receipts(&job), 2);
        assert!(h.head().is_some());
    }
    #[tokio::test]
    async fn vanished_source_cache_does_not_silently_add_a_root_charge() {
        let h = Harness::new(vec![graph("候选"), graph("定稿")]);
        let first = h.overview(200000);
        h.run(&first, &CancellationFlag::default(), false)
            .await
            .unwrap();
        let next = h.overview(200000);
        assert_eq!(next.payload["rootCalls"], 0);
        open_db(&h.runtime.root)
            .unwrap()
            .execute("UPDATE context_roots SET invalidated_at='gone'", [])
            .unwrap();
        let before = h.count();
        assert!(h
            .run(&next, &CancellationFlag::default(), false)
            .await
            .unwrap_err()
            .to_string()
            .contains("缓存已失效"));
        assert_eq!(h.count(), before);
    }
    #[tokio::test]
    async fn unknown_submitted_call_is_not_resent() {
        let h = Harness::new(vec![]);
        let job = h.overview(200000);
        h.runtime
            .job_module
            .save_checkpoint(
                &job.id,
                "draft",
                &json!({"calls":{"draft":{"state":"inflight"}}}),
            )
            .unwrap();
        assert!(h
            .run(&job, &CancellationFlag::default(), false)
            .await
            .unwrap_err()
            .to_string()
            .contains("结果未知"));
        assert_eq!(h.count(), 0);
    }
    #[tokio::test]
    async fn plan_confirmation_rejects_changed_prompts_and_stale_parent_sources() {
        let h = Harness::new(vec![graph("候选"), graph("定稿")]);
        let job = h.overview(200000);
        let frozen = crate::outline_plan::frozen_plan_from_payload(&job.payload).unwrap();
        let mut store = h.store();
        crate::outline_plan::verify_overview_plan(
            &h.runtime.outline_module,
            &store,
            &frozen,
            DocumentKind::Paper,
            &h.route.frozen().route_id().database_value(),
            "gemini-test",
            "PDF 来源根",
        )
        .unwrap();
        store
            .slots
            .get_mut("outline_compose")
            .unwrap()
            .paper
            .text
            .push_str("\n修改后的要求");
        assert!(crate::outline_plan::verify_overview_plan(
            &h.runtime.outline_module,
            &store,
            &frozen,
            DocumentKind::Paper,
            &h.route.frozen().route_id().database_value(),
            "gemini-test",
            "PDF 来源根"
        )
        .is_err());
        h.run(&job, &CancellationFlag::default(), false)
            .await
            .unwrap();
        let local = h.local();
        assert!(crate::outline_plan::verify_local_plan(
            &h.runtime.outline_module,
            &h.store(),
            &local.payload,
            DocumentKind::Paper,
            &h.route.frozen().route_id().database_value(),
            "gemini-test",
            "改根",
            Some(200000)
        )
        .is_err());
        open_db(&h.runtime.root).unwrap().execute("INSERT INTO ocr_revisions(id,revision_id,status,provider,model,created_at,published_at) VALUES ('newocr','r','ready','mistral','m','z','z')",[]).unwrap();
        assert!(crate::outline_plan::prepare_deep_dive_plan(
            &h.runtime.outline_module,
            &h.store(),
            "r",
            "n1",
            DocumentKind::Paper,
            &h.route.frozen().route_id().database_value(),
            "gemini-test",
            "PDF 来源根",
            true,
            Some(200000)
        )
        .is_err());
    }
    #[tokio::test]
    async fn deleting_an_unreviewed_candidate_invalidates_its_unpublished_plan() {
        let h = Harness::new(vec![graph("候选")]);
        let job = h.overview(200000);
        assert!(h
            .run(&job, &CancellationFlag::default(), true)
            .await
            .is_err());
        h.runtime.outline_module.delete_overview("r").unwrap();
        let before = h.count();
        assert!(h
            .run(&job, &CancellationFlag::default(), false)
            .await
            .is_err());
        assert_eq!(h.count(), before);
        assert!(h.head().is_none());
    }
    #[tokio::test]
    async fn later_success_hides_old_failure_and_local_version_changes_stop_before_payment() {
        let h = Harness::new(vec![
            graph("失败前候选"),
            graph("新候选"),
            graph("新定稿"),
            appendix(),
        ]);
        let failed = h.overview(200000);
        assert!(h
            .run(&failed, &CancellationFlag::default(), true)
            .await
            .is_err());
        h.runtime.job_module.fail(&failed.id, "injected").unwrap();
        assert_eq!(
            h.runtime
                .outline_module
                .job_for_plan("r", failed.payload["planId"].as_str().unwrap())
                .unwrap()
                .as_deref(),
            Some(failed.id.as_str())
        );
        let successful = h.overview(200000);
        h.run(&successful, &CancellationFlag::default(), false)
            .await
            .unwrap();
        assert!(h
            .runtime
            .outline_module
            .project_without_route("r")
            .unwrap()
            .latest_attempt
            .is_none());
        let old_local = h.local();
        let new_local = h.local();
        h.run(&new_local, &CancellationFlag::default(), false)
            .await
            .unwrap();
        let before = h.count();
        assert!(h
            .run(&old_local, &CancellationFlag::default(), false)
            .await
            .unwrap_err()
            .to_string()
            .contains("局部图"));
        assert_eq!(h.count(), before);
    }
    #[tokio::test]
    async fn actual_root_dispatch_enforces_frozen_allowance_even_if_cache_lookup_raced() {
        let h = Harness::new(vec![]);
        let job = h.overview(200000);
        let flag = CancellationFlag::default();
        let adapter = job_paper_model_adapter(
            h.route.adapter_arc(),
            h.runtime.job_module.clone(),
            job.id.clone(),
            flag,
        );
        let port = OutlineJobPort(&adapter);
        let request = PaperInteractionRequest {
            model: "gemini-test".into(),
            context_epoch: "test".into(),
            pdf_path: h.facts.pdf_path.clone(),
            display_name: "test".into(),
            remote_file_id: None,
            previous_interaction_id: None,
            system_instruction: "source".into(),
            user_input: "ack".into(),
            response_schema: None,
            inline_images: vec![],
            kind: PaperInteractionKind::Root,
        };
        port.interact(request.clone()).await.unwrap();
        assert!(port
            .interact(request)
            .await
            .unwrap_err()
            .to_string()
            .contains("额度"));
        assert_eq!(h.count(), 1);
        assert_eq!(
            h.runtime
                .job_module
                .get_checkpoint(&job.id)
                .unwrap()
                .unwrap()["rootRequestIssued"],
            true
        );
    }
}
