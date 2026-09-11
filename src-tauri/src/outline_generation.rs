use crate::outline_catalog::OutlineCatalog;
use crate::outline_map::{
    parse_draft, parse_review, OutlineGraphV4, OutlineJobProtocol, OutlineMapOutcome, MAP_PROTOCOL,
};
use crate::outline_plan::catalog_user_value;
use crate::outline_validate::{format_validation_issues, validate_map_v4};
use crate::provider_ports::{
    CancellationFlag, PaperInteractionKind, PaperInteractionOutcome, PaperInteractionRequest,
    PaperModelPort, ProviderError, UsageEnvelope,
};
use crate::reading_artifact_module::{DocumentFacts, ReadingArtifactModule};
use serde_json::{json, Value};
use std::future::Future;

fn frozen_prompt(payload: &Value, key: &str) -> Result<String, ProviderError> {
    payload
        .get("prompts")
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| ProviderError::local_state("地图任务缺少冻结提示词"))
}
pub const SLOT_DRAFT: &str = "draft";
pub const SLOT_DRAFT_REPAIR: &str = "draft_repair";
pub const SLOT_REVIEW: &str = "review";
pub const SLOT_REVIEW_REPAIR: &str = "review_repair";
pub const SLOT_LOCAL: &str = "local";
pub const SLOT_LOCAL_REPAIR: &str = "local_repair";

pub fn shared_repair_remaining(used: bool) -> u32 {
    if used {
        0
    } else {
        1
    }
}

pub fn next_repair_slot(stage: &str, repair_used: bool) -> Option<&'static str> {
    if repair_used {
        return None;
    }
    match stage {
        SLOT_DRAFT => Some(SLOT_DRAFT_REPAIR),
        SLOT_REVIEW => Some(SLOT_REVIEW_REPAIR),
        SLOT_LOCAL => Some(SLOT_LOCAL_REPAIR),
        _ => None,
    }
}

pub async fn call_once<F, P>(
    checkpoint: &mut Value,
    slot: &str,
    mut persist: P,
    call: F,
) -> Result<PaperInteractionOutcome, ProviderError>
where
    F: Future<Output = Result<PaperInteractionOutcome, ProviderError>>,
    P: FnMut(&Value) -> Result<(), String>,
{
    if let Some(saved) = checkpoint.get("calls").and_then(|calls| calls.get(slot)) {
        if let Some(response) = saved.get("response") {
            return serde_json::from_value(response.clone())
                .map_err(|error| ProviderError::local_state(error.to_string()));
        }
        let detail = saved
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("上次调用结果未知；为避免重复付费，不自动重发");
        return Err(ProviderError::invalid(detail.to_string()));
    }
    if checkpoint.get("calls").is_none() {
        checkpoint["calls"] = json!({});
    }
    checkpoint["calls"][slot] = json!({"state": "inflight"});
    persist(checkpoint).map_err(ProviderError::local_state)?;
    match call.await {
        Ok(response) => {
            checkpoint["calls"][slot] = json!({
                "state": "received",
                "response": response
            });
            persist(checkpoint).map_err(ProviderError::local_state)?;
            Ok(response)
        }
        Err(error) => {
            checkpoint["calls"][slot] = json!({"state": "failed", "error": error.to_string()});
            persist(checkpoint).map_err(ProviderError::local_state)?;
            Err(error)
        }
    }
}

fn frozen_language(payload: &Value) -> &str {
    payload
        .get("outputLanguage")
        .or_else(|| payload.get("language"))
        .and_then(Value::as_str)
        .unwrap_or("zh-CN")
}

pub fn draft_user_input(
    catalog: &OutlineCatalog,
    repair: Option<(&Value, &str)>,
    language: &str,
) -> String {
    let mut value = json!({
        "task": if repair.is_some() { "repair_document_map" } else { "compose_document_map" },
        "language": language,
        "catalog": catalog_user_value(catalog),
        "scope": "full"
    });
    if let Some((previous, issues)) = repair {
        value["previousOutput"] = previous.clone();
        value["validationIssues"] = json!(issues);
    }
    value.to_string()
}

pub fn review_user_input(
    catalog: &OutlineCatalog,
    candidate: &OutlineGraphV4,
    repair: Option<(&Value, &str)>,
    language: &str,
) -> String {
    let mut value = json!({
        "task": if repair.is_some() { "repair_document_map" } else { "review_document_map" },
        "language": language,
        "catalog": catalog_user_value(catalog),
        "candidateGraph": candidate,
        "scope": "full"
    });
    if let Some((previous, issues)) = repair {
        value["previousOutput"] = previous.clone();
        value["validationIssues"] = json!(issues);
    }
    value.to_string()
}

pub fn local_user_input(
    payload: &Value,
    catalog: &OutlineCatalog,
    repair: Option<(&Value, &str)>,
) -> String {
    let mut value = json!({
        "task": if repair.is_some() { "repair_local_map" } else { "compose_local_map" },
        "language": frozen_language(payload),
        "nodeId": payload.get("nodeId"),
        "targetNode": payload.get("targetNode"),
        "relatedNodes": payload.get("relatedNodes"),
        "relatedEdges": payload.get("relatedEdges"),
        "priorityPages": payload.get("priorityPages"),
        "catalog": catalog_user_value(catalog),
        "scope": payload.get("scope").and_then(Value::as_str).unwrap_or("full")
    });
    if let Some((previous, issues)) = repair {
        value["previousOutput"] = previous.clone();
        value["validationIssues"] = json!(issues);
    }
    value.to_string()
}

pub fn contains_orientation(user_input: &str) -> bool {
    user_input.contains("\"orientation\"") || user_input.contains("\"brief\"")
}

struct Generation<'a> {
    source: &'a ReadingArtifactModule,
    outline: &'a crate::outline_module::OutlineModule,
    jobs: &'a crate::job_module::JobModule,
    adapter: &'a dyn PaperModelPort,
    job: &'a crate::job_module::JobProjection,
    facts: &'a DocumentFacts,
    catalog: &'a OutlineCatalog,
    page_count: i64,
    route_id: &'a str,
    provider: &'a str,
    model: &'a str,
    cancellation: &'a CancellationFlag,
}

impl Generation<'_> {
    fn ready(&self) -> Result<(), ProviderError> {
        self.cancellation.check()?;
        if !matches!(
            self.jobs
                .get(&self.job.id)
                .map_err(ProviderError::local_state)?
                .state,
            crate::job_module::JobState::Running | crate::job_module::JobState::Queued
        ) {
            return Err(ProviderError::local_state("地图任务已停止，不能继续生成。"));
        }
        self.outline
            .check_frozen_inputs(&self.job.payload)
            .map_err(ProviderError::local_state)
    }

    fn validate_snapshot(&self) -> Result<(), ProviderError> {
        let p = &self.job.payload;
        if p["revisionId"].as_str() != Some(self.facts.revision_id.as_str())
            || p["pdfHash"].as_str() != Some(self.facts.revision_sha256.as_str())
            || p["routeId"].as_str() != Some(self.route_id)
            || p["model"].as_str() != Some(self.model)
            || p["scope"].as_str() != Some("full")
            || p["pageCount"].as_i64() != Some(self.page_count)
            || p.get("inputTokenLimit").is_none()
        {
            return Err(ProviderError::local_state(
                "地图任务的冻结输入不完整或已变化，请重新计划。",
            ));
        }
        let catalog: OutlineCatalog = serde_json::from_value(p["catalog"].clone())
            .map_err(|_| ProviderError::local_state("地图缺少冻结目录，请重新计划。"))?;
        if catalog != *self.catalog
            || p["catalogDigest"].as_str() != Some(self.catalog.digest.as_str())
        {
            return Err(ProviderError::local_state(
                "地图目录与冻结计划不符，请重新计划。",
            ));
        }
        Ok(())
    }

    async fn invoke<R>(
        &self,
        checkpoint: &mut Value,
        slot: &str,
        system: &str,
        input: String,
        schema: Value,
        record: &mut R,
    ) -> Result<PaperInteractionOutcome, ProviderError>
    where
        R: FnMut(&str, &str, &UsageEnvelope) -> Result<(), ProviderError>,
    {
        self.ready()?;
        // Received outcomes must be replayable even when a retry would exceed the budget.
        if checkpoint["calls"][slot].get("response").is_none() {
            crate::outline_plan::preflight_request(
                self.page_count,
                self.job.payload["inputTokenLimit"].as_i64(),
                system,
                &input,
                &schema,
            )
            .map_err(ProviderError::local_state)?;
        }
        let root_prompt = frozen_prompt(&self.job.payload, "paperRoot")?;
        if checkpoint["calls"][slot].get("response").is_none()
            && checkpoint["calls"].get(slot).is_none()
            && self
                .source
                .lookup_root_for_route_with_instruction(
                    self.facts,
                    self.route_id,
                    self.model,
                    Some(&root_prompt),
                )?
                .is_none()
        {
            if self.job.payload["rootCalls"].as_i64() != Some(1)
                || checkpoint["rootCreationReserved"].as_bool() == Some(true)
            {
                return Err(ProviderError::local_state(
                    "PDF 来源缓存已失效，继续需要额外来源调用，请重新计划。",
                ));
            }
            checkpoint["rootCreationReserved"] = json!(true);
            self.jobs
                .save_checkpoint(&self.job.id, slot, checkpoint)
                .map_err(ProviderError::local_state)?;
        }
        let outcome = call_once(
            checkpoint,
            slot,
            |c| {
                let mut merged = c.clone();
                let saved = self.jobs.get_checkpoint(&self.job.id)?;
                if saved
                    .as_ref()
                    .is_some_and(|v| v["rootRequestIssued"].as_bool() == Some(true))
                {
                    merged["rootRequestIssued"] = json!(true);
                }
                self.jobs.save_checkpoint(&self.job.id, slot, &merged)
            },
            async {
                let root = self
                    .source
                    .ensure_root_for_route(
                        self.adapter,
                        self.facts,
                        self.route_id,
                        self.provider,
                        self.model,
                        Some(&root_prompt),
                    )
                    .await?;
                self.ready()?;
                // A stale root is surfaced to the existing recovery UI. Rebuilding it here
                // would hide another root charge and a repeated generation call.
                self.adapter
                    .interact(PaperInteractionRequest {
                        model: self.model.to_string(),
                        context_epoch: root.context_epoch,
                        pdf_path: self.facts.pdf_path.clone(),
                        display_name: self.facts.title.clone(),
                        remote_file_id: Some(root.remote_file_id),
                        previous_interaction_id: Some(root.remote_node_id),
                        system_instruction: system.to_string(),
                        user_input: input,
                        response_schema: Some(schema),
                        inline_images: Vec::new(),
                        kind: PaperInteractionKind::Artifact,
                    })
                    .await
            },
        )
        .await?;
        // The recorder uses the durable logical slot, including when remote ID is empty.
        record(&outcome.provider_node_id, slot, &outcome.receipt)?;
        self.ready()?;
        Ok(outcome)
    }

    fn decode(&self, text: &str, review: bool) -> Result<(OutlineGraphV4, Vec<String>), String> {
        let (graph, notes) = if review {
            let parsed = parse_review(text)?;
            (parsed.graph, parsed.review_notes)
        } else {
            (parse_draft(text)?.graph, Vec::new())
        };
        match validate_map_v4(graph, self.catalog, self.page_count, notes) {
            OutlineMapOutcome::Valid {
                graph,
                review_notes,
            } => Ok((graph, review_notes)),
            OutlineMapOutcome::Invalid { issues, .. } => Err(format_validation_issues(&issues)),
        }
    }

    async fn stage<R>(
        &self,
        checkpoint: &mut Value,
        stage: &str,
        prompt: &str,
        candidate: Option<&OutlineGraphV4>,
        record: &mut R,
    ) -> Result<(OutlineGraphV4, Vec<String>), ProviderError>
    where
        R: FnMut(&str, &str, &UsageEnvelope) -> Result<(), ProviderError>,
    {
        let language = frozen_language(&self.job.payload);
        let input = |repair| match stage {
            SLOT_DRAFT => draft_user_input(self.catalog, repair, language),
            SLOT_REVIEW => review_user_input(
                self.catalog,
                candidate.expect("review candidate"),
                repair,
                language,
            ),
            _ => local_user_input(&self.job.payload, self.catalog, repair),
        };
        let schema = if stage == SLOT_REVIEW {
            crate::outline_map::review_schema()
        } else {
            crate::outline_map::draft_schema()
        };
        let original = self
            .invoke(
                checkpoint,
                stage,
                prompt,
                input(None),
                schema.clone(),
                record,
            )
            .await?;
        match self.decode(&original.text, stage == SLOT_REVIEW) {
            Ok(value) => Ok(value),
            Err(error) => {
                let repair = repair_slot_for_checkpoint(stage, checkpoint).ok_or_else(|| {
                    ProviderError::invalid(format!("地图无效且共享修复额度已用尽：{error}"))
                })?;
                // Preserve malformed JSON verbatim as data, rather than discarding it on parse error.
                let previous = Value::String(original.text);
                let fixed = self
                    .invoke(
                        checkpoint,
                        repair,
                        prompt,
                        input(Some((&previous, &error))),
                        schema,
                        record,
                    )
                    .await?;
                self.decode(&fixed.text, stage == SLOT_REVIEW)
                    .map_err(|e| ProviderError::invalid(format!("地图修复后仍无效：{e}")))
            }
        }
    }
}

fn repair_slot_for_checkpoint(stage: &str, checkpoint: &Value) -> Option<&'static str> {
    let slot = next_repair_slot(stage, false)?;
    // Replaying an already submitted repair does not consume another repair allowance.
    if checkpoint["calls"].get(slot).is_some() {
        return Some(slot);
    }
    if [SLOT_DRAFT_REPAIR, SLOT_REVIEW_REPAIR, SLOT_LOCAL_REPAIR]
        .iter()
        .any(|other| checkpoint["calls"].get(*other).is_some())
        || checkpoint["repairUsed"].as_bool() == Some(true)
    {
        return None;
    }
    Some(slot)
}

#[allow(clippy::too_many_arguments)]
pub async fn run_overview_v4<P, R>(
    source: &ReadingArtifactModule,
    outline: &crate::outline_module::OutlineModule,
    jobs: &crate::job_module::JobModule,
    adapter: &dyn PaperModelPort,
    job: &crate::job_module::JobProjection,
    facts: &DocumentFacts,
    catalog: &OutlineCatalog,
    page_count: i64,
    route_id: &str,
    provider: &str,
    model: &str,
    cancellation: &CancellationFlag,
    mut report: P,
    mut record: R,
) -> Result<(), ProviderError>
where
    P: FnMut(&str, Value, Option<&UsageEnvelope>) -> Result<(), ProviderError>,
    R: FnMut(&str, &str, &UsageEnvelope) -> Result<(), ProviderError>,
{
    if crate::outline_map::job_protocol(&job.payload).map_err(ProviderError::invalid)?
        != OutlineJobProtocol::V4
    {
        return Err(ProviderError::invalid(
            "overview v4 runner received a non-v4 job",
        ));
    }
    let publish_id = job.payload["publishId"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ProviderError::local_state("地图缺少 publishId，请重新计划。"))?;
    if outline
        .published_plan(publish_id)
        .map_err(ProviderError::local_state)?
    {
        match jobs.get(&job.id).map_err(ProviderError::local_state)?.state {
            crate::job_module::JobState::Running => {
                jobs.complete(&job.id).map_err(ProviderError::local_state)?
            }
            crate::job_module::JobState::Completed => {}
            _ => return Err(ProviderError::local_state("已发布任务当前不可继续。")),
        }
        return Ok(());
    }
    let run = Generation {
        source,
        outline,
        jobs,
        adapter,
        job,
        facts,
        catalog,
        page_count,
        route_id,
        provider,
        model,
        cancellation,
    };
    run.validate_snapshot()?;
    run.ready()?;
    let draft_prompt = frozen_prompt(&job.payload, "draft")?;
    let review_prompt = frozen_prompt(&job.payload, "review")?;
    let ocr = job.payload["ocrRevisionId"]
        .as_str()
        .ok_or_else(|| ProviderError::local_state("缺少 OCR 修订。"))?;
    let mut checkpoint = jobs
        .get_checkpoint(&job.id)
        .map_err(ProviderError::local_state)?
        .unwrap_or_else(|| json!({}));
    report("drafting", json!({}), None)?;
    let (candidate, _) = run
        .stage(
            &mut checkpoint,
            SLOT_DRAFT,
            &draft_prompt,
            None,
            &mut record,
        )
        .await?;
    outline
        .save_candidate_overview(
            &facts.revision_id,
            ocr,
            &catalog.digest,
            &candidate,
            publish_id,
        )
        .map_err(ProviderError::local_state)?;
    report("reviewing", json!({"reviewStatus":"unchecked"}), None)?;
    let (graph, notes) = run
        .stage(
            &mut checkpoint,
            SLOT_REVIEW,
            &review_prompt,
            Some(&candidate),
            &mut record,
        )
        .await?;
    run.ready()?;
    outline
        .publish_overview_v4(
            &facts.revision_id,
            ocr,
            &catalog.digest,
            &graph,
            &notes,
            job.payload["expectedHeadId"].as_str(),
            publish_id,
        )
        .map_err(ProviderError::local_state)?;
    report(
        "published",
        json!({"protocolVersion":MAP_PROTOCOL,"publishId":publish_id}),
        None,
    )?;
    jobs.complete(&job.id).map_err(ProviderError::local_state)?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn run_deep_dive_v4<P, R>(
    source: &ReadingArtifactModule,
    outline: &crate::outline_module::OutlineModule,
    jobs: &crate::job_module::JobModule,
    adapter: &dyn PaperModelPort,
    job: &crate::job_module::JobProjection,
    facts: &DocumentFacts,
    catalog: &OutlineCatalog,
    page_count: i64,
    route_id: &str,
    provider: &str,
    model: &str,
    cancellation: &CancellationFlag,
    mut report: P,
    mut record: R,
) -> Result<(), ProviderError>
where
    P: FnMut(&str, Value, Option<&UsageEnvelope>) -> Result<(), ProviderError>,
    R: FnMut(&str, &str, &UsageEnvelope) -> Result<(), ProviderError>,
{
    let publish_id = job.payload["publishId"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| ProviderError::local_state("局部图缺少 publishId，请重新计划。"))?;
    if outline
        .published_plan(publish_id)
        .map_err(ProviderError::local_state)?
    {
        match jobs.get(&job.id).map_err(ProviderError::local_state)?.state {
            crate::job_module::JobState::Running => {
                jobs.complete(&job.id).map_err(ProviderError::local_state)?
            }
            crate::job_module::JobState::Completed => {}
            _ => return Err(ProviderError::local_state("已发布任务当前不可继续。")),
        }
        return Ok(());
    }
    let run = Generation {
        source,
        outline,
        jobs,
        adapter,
        job,
        facts,
        catalog,
        page_count,
        route_id,
        provider,
        model,
        cancellation,
    };
    run.validate_snapshot()?;
    run.ready()?;
    let prompt = frozen_prompt(&job.payload, "deepDive")?;
    let node = job.payload["nodeId"]
        .as_str()
        .ok_or_else(|| ProviderError::local_state("缺少局部目标。"))?;
    let parent = job.payload["overviewRevisionId"]
        .as_str()
        .ok_or_else(|| ProviderError::local_state("缺少冻结父版本。"))?;
    let ocr = job.payload["ocrRevisionId"]
        .as_str()
        .ok_or_else(|| ProviderError::local_state("缺少 OCR 修订。"))?;
    let mut checkpoint = jobs
        .get_checkpoint(&job.id)
        .map_err(ProviderError::local_state)?
        .unwrap_or_else(|| json!({}));
    report("composing", json!({"nodeId":node}), None)?;
    let (graph, _) = run
        .stage(&mut checkpoint, SLOT_LOCAL, &prompt, None, &mut record)
        .await?;
    run.ready()?;
    outline
        .publish_deep_dive_v4(
            &facts.revision_id,
            node,
            parent,
            ocr,
            &catalog.digest,
            &graph,
            publish_id,
        )
        .map_err(ProviderError::local_state)?;
    report("published", json!({"nodeId":node}), None)?;
    jobs.complete(&job.id).map_err(ProviderError::local_state)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_repair_is_one_shot_across_stages() {
        assert_eq!(next_repair_slot(SLOT_DRAFT, false), Some(SLOT_DRAFT_REPAIR));
        assert_eq!(
            next_repair_slot(SLOT_REVIEW, false),
            Some(SLOT_REVIEW_REPAIR)
        );
        assert_eq!(next_repair_slot(SLOT_REVIEW, true), None);
        assert_eq!(shared_repair_remaining(true), 0);
    }

    #[test]
    fn draft_input_has_no_orientation_pack() {
        let catalog = OutlineCatalog {
            entries: Vec::new(),
            digest: "x".into(),
            token_estimate: 0,
        };
        let input = draft_user_input(&catalog, None, "zh-CN");
        assert!(!contains_orientation(&input));
        assert!(input.contains("compose_document_map"));
    }
}
