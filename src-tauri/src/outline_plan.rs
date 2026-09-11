use crate::library_paths::DocumentKind;
use crate::outline_catalog::{
    outline_context_exceeds_window, OutlineCatalog, OUTPUT_RESERVE_TOKENS, TOKENS_PER_PAGE,
};
use crate::outline_map::MAP_PROTOCOL;
use crate::outline_module::{OutlineModule, OutlinePlan};
use crate::prompt_settings::{outline_bundle_protocol, PromptSettingsProjection, PromptSlotId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const SCOPE_FULL: &str = "full";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrozenOutlinePlan {
    pub plan_id: String,
    pub plan_digest: String,
    pub publish_id: String,
    pub map_protocol: String,
    pub workflow: String,
    pub revision_id: String,
    pub document_kind: String,
    pub ocr_revision_id: String,
    pub catalog_digest: String,
    pub pdf_hash: String,
    pub scope: String,
    pub route_id: String,
    pub model: String,
    pub expected_head_id: Option<String>,
    pub prompt_digests: Value,
    pub payload: Value,
}

pub fn prompt_digest(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.replace("\r\n", "\n").as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn compute_plan_digest(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0u8]);
    }
    format!("{:x}", hasher.finalize())
}

pub fn prepare_overview_plan(
    outline: &OutlineModule,
    store: &PromptSettingsProjection,
    revision_id: &str,
    kind: DocumentKind,
    route_id: &str,
    model: &str,
    supports_native_pdf: bool,
    input_token_limit: Option<i64>,
    paper_root_prompt: &str,
) -> Result<OutlinePlan, String> {
    if !supports_native_pdf {
        return Err("The current paper model does not support native PDF".to_string());
    }
    let bundle = outline_bundle_protocol(store, kind)?;
    let (ocr_revision_id, catalog) = outline.catalog_for_revision(revision_id)?;
    let page_count = outline.revision_page_count(revision_id)?;
    let pdf_hash = outline.revision_sha256(revision_id)?;
    let expected_head_id = outline.current_overview_id(revision_id)?;
    if outline_context_exceeds_window(page_count, catalog.token_estimate, input_token_limit) {
        return Err("Outline catalog and PDF exceed the model context window".to_string());
    }
    let has_paper_root =
        outline.has_source_root(revision_id, route_id, model, paper_root_prompt)?;
    if bundle == "v3" {
        return Ok(OutlinePlan {
            revision_id: revision_id.to_string(),
            ocr_revision_id,
            catalog_digest: catalog.digest,
            model: model.to_string(),
            extract_calls: 1,
            compose_calls: 1,
            max_repair_calls: 1,
            page_count,
            catalog_token_estimate: catalog.token_estimate,
            pdf_token_estimate: page_count.saturating_mul(TOKENS_PER_PAGE),
            estimated_cost: None,
            has_paper_root,
            supports_native_pdf,
            root_calls: i64::from(!has_paper_root),
            plan_id: None,
            plan_digest: None,
            publish_id: None,
            protocol_version: format!(
                "{}+{}",
                crate::outline_protocol::EXTRACT_PROTOCOL,
                crate::outline_protocol::COMPOSE_PROTOCOL
            ),
            workflow: "extract_compose".into(),
            expected_head_id,
        });
    }

    let draft =
        crate::prompt_settings::resolved_generation_text(store, PromptSlotId::OutlineExtract, kind);
    let review =
        crate::prompt_settings::resolved_generation_text(store, PromptSlotId::OutlineCompose, kind);
    preflight_request(
        page_count,
        input_token_limit,
        &draft,
        &catalog_user_value(&catalog).to_string(),
        &crate::outline_map::draft_schema(),
    )?;
    preflight_request(
        page_count,
        input_token_limit,
        &review,
        &catalog_user_value(&catalog).to_string(),
        &crate::outline_map::review_schema(),
    )?;
    let draft_digest = prompt_digest(&draft);
    let review_digest = prompt_digest(&review);
    let root_digest = prompt_digest(paper_root_prompt);
    let plan_id = Uuid::new_v4().to_string();
    let publish_id = Uuid::new_v4().to_string();
    let plan_digest = compute_plan_digest(&[
        MAP_PROTOCOL,
        revision_id,
        &ocr_revision_id,
        &catalog.digest,
        &pdf_hash,
        route_id,
        model,
        SCOPE_FULL,
        expected_head_id.as_deref().unwrap_or(""),
        &draft_digest,
        &review_digest,
        &root_digest,
        kind.as_str(),
        crate::ui_locale::output_language(store.locale),
    ]);
    let payload = json!({
        "outputLanguage": crate::ui_locale::output_language(store.locale),
        "mapProtocol": MAP_PROTOCOL,
        "planId": plan_id,
        "planDigest": plan_digest,
        "publishId": publish_id,
        "revisionId": revision_id,
        "expectedHeadId": expected_head_id,
        "ocrRevisionId": ocr_revision_id,
        "catalogDigest": catalog.digest,
        "pdfHash": pdf_hash,
        "catalog": catalog,
        "pageCount": page_count,
        "inputTokenLimit": input_token_limit,
        "rootCalls": i64::from(!has_paper_root),
        "scope": SCOPE_FULL,
        "documentKind": kind.as_str(),
        "routeId": route_id,
        "model": model,
        "prompts": {
            "draft": draft,
            "review": review,
            "paperRoot": paper_root_prompt
        },
        "promptDigests": {
            "draft": draft_digest,
            "review": review_digest,
            "paperRoot": root_digest
        }
    });
    outline.save_plan(
        &plan_id,
        revision_id,
        &ocr_revision_id,
        &catalog.digest,
        model,
        &payload,
    )?;
    Ok(OutlinePlan {
        revision_id: revision_id.to_string(),
        ocr_revision_id,
        catalog_digest: catalog.digest,
        model: model.to_string(),
        extract_calls: 1,
        compose_calls: 1,
        max_repair_calls: 1,
        page_count,
        catalog_token_estimate: catalog.token_estimate,
        pdf_token_estimate: page_count.saturating_mul(TOKENS_PER_PAGE),
        estimated_cost: None,
        has_paper_root,
        supports_native_pdf,
        root_calls: i64::from(!has_paper_root),
        plan_id: Some(plan_id),
        plan_digest: Some(plan_digest),
        publish_id: Some(publish_id),
        protocol_version: MAP_PROTOCOL.into(),
        workflow: "draft_review".into(),
        expected_head_id,
    })
}

pub fn prepare_deep_dive_plan(
    outline: &OutlineModule,
    store: &PromptSettingsProjection,
    revision_id: &str,
    node_id: &str,
    kind: DocumentKind,
    route_id: &str,
    model: &str,
    paper_root_prompt: &str,
    supports_native_pdf: bool,
    input_token_limit: Option<i64>,
) -> Result<Value, String> {
    if !supports_native_pdf {
        return Err("当前模型不支持原生 PDF。".into());
    }
    let bundle = outline_bundle_protocol(store, kind)?;
    let head = outline
        .current_overview_head(revision_id)?
        .ok_or_else(|| "Generate an Overview before a Deep dive".to_string())?;
    if bundle == "v4" && !crate::outline_map::is_map_v4_protocol(&head.protocol_version) {
        return Err("局部图新路径需要 v4 总图。当前总图仍是旧版，请先用新版生成总图。".into());
    }
    let (ocr_revision_id, catalog) = outline.catalog_for_revision(revision_id)?;
    if head.ocr_revision_id != ocr_revision_id {
        return Err("总图基于旧 OCR，请先重新生成总图。".into());
    }
    let pdf_hash = outline.revision_sha256(revision_id)?;
    let page_count = outline.revision_page_count(revision_id)?;
    let expected_local_head = outline
        .deep_dive_for_node(revision_id, node_id)?
        .map(|h| h.id);
    let prompt = crate::prompt_settings::resolved_generation_text(
        store,
        PromptSlotId::OutlineDeepDive,
        kind,
    );
    if bundle == "v3" {
        if crate::outline_map::is_map_v4_protocol(&head.protocol_version) {
            return Err("旧版局部稿不能用于新版总图，请切换整套新版提示词。".into());
        }
        return Ok(json!({
            "revisionId": revision_id, "model": model, "routeId": route_id,
            "ocrRevisionId": ocr_revision_id,
            "nodeId": node_id,
            "overviewRevisionId": head.id,
            "prompts": { "deepDive": prompt, "paperRoot": paper_root_prompt },
            "documentKind": kind.as_str()
        }));
    }
    let has_paper_root =
        outline.has_source_root(revision_id, route_id, model, paper_root_prompt)?;
    let graph = head
        .graph
        .clone()
        .ok_or_else(|| "Deep dive requires a published Overview graph".to_string())?;
    let graph_v4 = crate::outline_map::load_graph(&graph)?;
    let target = graph_v4
        .nodes
        .iter()
        .find(|node| node.node_id == node_id)
        .cloned()
        .ok_or_else(|| "Selected node is not in the frozen Overview".to_string())?;
    let related_edges: Vec<_> = graph_v4
        .edges
        .iter()
        .filter(|edge| edge.source_node_id == node_id || edge.target_node_id == node_id)
        .cloned()
        .collect();
    let related_ids: std::collections::HashSet<_> = related_edges
        .iter()
        .flat_map(|edge| [edge.source_node_id.clone(), edge.target_node_id.clone()])
        .collect();
    let related_nodes: Vec<_> = graph_v4
        .nodes
        .iter()
        .filter(|node| related_ids.contains(&node.node_id) && node.node_id != node_id)
        .cloned()
        .collect();
    let mut priority_pages: Vec<_> =
        crate::outline_map::priority_pages_from_graph(&graph_v4, node_id, &catalog)
            .into_iter()
            .collect();
    priority_pages.sort_unstable();
    let plan_id = Uuid::new_v4().to_string();
    let publish_id = Uuid::new_v4().to_string();
    let digest = compute_plan_digest(&[
        crate::outline_map::DEEP_DIVE_PROTOCOL_V4,
        revision_id,
        &head.id,
        node_id,
        &ocr_revision_id,
        &catalog.digest,
        &pdf_hash,
        route_id,
        model,
        &prompt_digest(&prompt),
        &prompt_digest(paper_root_prompt),
        expected_local_head.as_deref().unwrap_or(""),
        kind.as_str(),
        SCOPE_FULL,
        crate::ui_locale::output_language(store.locale),
    ]);
    let payload = json!({
        "outputLanguage": crate::ui_locale::output_language(store.locale),
        "mapProtocol": crate::outline_map::DEEP_DIVE_PROTOCOL_V4,
        "revisionId": revision_id,
        "catalog": catalog,
        "pageCount": page_count,
        "inputTokenLimit": input_token_limit,
        "rootCalls": i64::from(!has_paper_root),
        "expectedLocalHeadId": expected_local_head,
        "planId": plan_id,
        "planDigest": digest,
        "publishId": publish_id,
        "ocrRevisionId": ocr_revision_id,
        "catalogDigest": catalog.digest,
        "pdfHash": pdf_hash,
        "nodeId": node_id,
        "overviewRevisionId": head.id,
        "targetNode": target,
        "relatedNodes": related_nodes,
        "relatedEdges": related_edges,
        "priorityPages": priority_pages,
        "scope": SCOPE_FULL,
        "documentKind": kind.as_str(),
        "routeId": route_id,
        "model": model,
        "prompts": {
            "deepDive": prompt,
            "paperRoot": paper_root_prompt
        }
    });
    preflight_request(
        page_count,
        input_token_limit,
        &prompt,
        &crate::outline_generation::local_user_input(&payload, &catalog, None),
        &crate::outline_map::deep_dive_schema(),
    )?;
    outline.save_plan(
        &plan_id,
        revision_id,
        &ocr_revision_id,
        &catalog.digest,
        model,
        &payload,
    )?;
    Ok(payload)
}

pub fn verify_overview_plan(
    outline: &OutlineModule,
    store: &PromptSettingsProjection,
    plan: &FrozenOutlinePlan,
    kind: DocumentKind,
    route_id: &str,
    model: &str,
    paper_root_prompt: &str,
) -> Result<(), String> {
    if plan.map_protocol != MAP_PROTOCOL {
        return Err("计划协议已过期，请重新计划。".into());
    }
    if plan.route_id != route_id || plan.model != model {
        return Err("模型或路由已变化，请重新计划。".into());
    }
    let (ocr_revision_id, catalog) = outline.catalog_for_revision(&plan.revision_id)?;
    if ocr_revision_id != plan.ocr_revision_id || catalog.digest != plan.catalog_digest {
        return Err("OCR 或目录已变化，请重新计划。".into());
    }
    let pdf_hash = outline.revision_sha256(&plan.revision_id)?;
    if pdf_hash != plan.pdf_hash {
        return Err("PDF 已变化，请重新计划。".into());
    }
    let expected = outline.current_overview_id(&plan.revision_id)?;
    if expected != plan.expected_head_id {
        return Err("当前地图版本已变化，请重新计划。".into());
    }
    let draft =
        crate::prompt_settings::resolved_generation_text(store, PromptSlotId::OutlineExtract, kind);
    let review =
        crate::prompt_settings::resolved_generation_text(store, PromptSlotId::OutlineCompose, kind);
    let actual = compute_plan_digest(&[
        MAP_PROTOCOL,
        &plan.revision_id,
        &plan.ocr_revision_id,
        &plan.catalog_digest,
        &plan.pdf_hash,
        route_id,
        model,
        SCOPE_FULL,
        plan.expected_head_id.as_deref().unwrap_or(""),
        &prompt_digest(&draft),
        &prompt_digest(&review),
        &prompt_digest(paper_root_prompt),
        kind.as_str(),
        crate::ui_locale::output_language(store.locale),
    ]);
    if actual != plan.plan_digest {
        return Err("提示词或计划输入已变化，请重新计划。".into());
    }
    Ok(())
}

pub fn frozen_plan_from_payload(payload: &Value) -> Result<FrozenOutlinePlan, String> {
    let plan_id = payload
        .get("planId")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| "计划缺少 planId".to_string())?;
    Ok(FrozenOutlinePlan {
        plan_id: plan_id.to_string(),
        plan_digest: payload
            .get("planDigest")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        publish_id: payload
            .get("publishId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        map_protocol: payload
            .get("mapProtocol")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        workflow: "draft_review".into(),
        revision_id: payload
            .get("revisionId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        document_kind: payload
            .get("documentKind")
            .and_then(Value::as_str)
            .unwrap_or("paper")
            .to_string(),
        ocr_revision_id: payload
            .get("ocrRevisionId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        catalog_digest: payload
            .get("catalogDigest")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        pdf_hash: payload
            .get("pdfHash")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        scope: payload
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or(SCOPE_FULL)
            .to_string(),
        route_id: payload
            .get("routeId")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        model: payload
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        expected_head_id: payload
            .get("expectedHeadId")
            .and_then(Value::as_str)
            .map(str::to_string),
        prompt_digests: payload.get("promptDigests").cloned().unwrap_or(json!({})),
        payload: payload.clone(),
    })
}

pub fn catalog_user_value(catalog: &OutlineCatalog) -> Value {
    json!({
        "digest": catalog.digest,
        "tokenEstimate": catalog.token_estimate,
        "entries": catalog.entries,
        "note": "excerpt 约 80 字，只用于定位，不是全文。"
    })
}

/// A conservative preflight over the actual wire material, including candidate/repair output.
pub fn preflight_request(
    page_count: i64,
    limit: Option<i64>,
    system: &str,
    input: &str,
    schema: &Value,
) -> Result<(), String> {
    if page_count < 1 {
        return Err("缺少可靠的 PDF 页数，请重新计划。".into());
    }
    let text = system
        .chars()
        .count()
        .saturating_add(input.chars().count())
        .saturating_add(schema.to_string().chars().count());
    let estimate = page_count
        .saturating_mul(TOKENS_PER_PAGE)
        .saturating_add(text as i64)
        .saturating_add(OUTPUT_RESERVE_TOKENS);
    if limit.is_some_and(|n| n > 0 && estimate > n) {
        return Err("地图请求（包括原文、提示词、候选或修复材料）超过模型窗口；已保留收到的结果，请重新计划。".into());
    }
    Ok(())
}

pub fn project_local_plan(
    outline: &OutlineModule,
    payload: &Value,
    native: bool,
) -> Result<OutlinePlan, String> {
    let revision = payload["revisionId"].as_str().ok_or("局部计划缺少文档")?;
    let (_, catalog) = outline.catalog_for_revision(revision)?;
    let count = outline.revision_page_count(revision)?;
    let model = payload["model"].as_str().unwrap_or("");
    let has_root = if let Some(count) = payload["rootCalls"].as_i64() {
        count == 0
    } else {
        outline.has_source_root(
            revision,
            payload["routeId"].as_str().unwrap_or(""),
            model,
            payload["prompts"]["paperRoot"].as_str().unwrap_or(""),
        )?
    };
    Ok(OutlinePlan {
        revision_id: revision.into(),
        ocr_revision_id: payload["ocrRevisionId"].as_str().unwrap_or("").into(),
        catalog_digest: catalog.digest,
        model: model.into(),
        extract_calls: 0,
        compose_calls: 1,
        max_repair_calls: 1,
        page_count: count,
        catalog_token_estimate: catalog.token_estimate,
        pdf_token_estimate: count.saturating_mul(TOKENS_PER_PAGE),
        estimated_cost: None,
        has_paper_root: has_root,
        root_calls: i64::from(!has_root),
        supports_native_pdf: native,
        plan_id: payload["planId"].as_str().map(str::to_string),
        plan_digest: payload["planDigest"].as_str().map(str::to_string),
        publish_id: payload["publishId"].as_str().map(str::to_string),
        protocol_version: payload["mapProtocol"]
            .as_str()
            .unwrap_or(crate::outline_protocol::DEEP_DIVE_PROTOCOL)
            .into(),
        workflow: "local_map".into(),
        expected_head_id: payload["overviewRevisionId"].as_str().map(str::to_string),
    })
}

pub fn verify_local_plan(
    outline: &OutlineModule,
    store: &PromptSettingsProjection,
    payload: &Value,
    kind: DocumentKind,
    route: &str,
    model: &str,
    root_prompt: &str,
    limit: Option<i64>,
) -> Result<(), String> {
    if outline_bundle_protocol(store, kind)? != "v4"
        || payload["mapProtocol"].as_str() != Some(crate::outline_map::DEEP_DIVE_PROTOCOL_V4)
    {
        return Err("局部图提示词协议已变化，请重新计划。".into());
    }
    if payload["routeId"].as_str() != Some(route)
        || payload["outputLanguage"].as_str()
            != Some(crate::ui_locale::output_language(store.locale))
        || payload["model"].as_str() != Some(model)
        || payload["documentKind"].as_str() != Some(kind.as_str())
        || payload["inputTokenLimit"].as_i64() != limit
    {
        return Err("局部图模型或计划输入已变化，请重新计划。".into());
    }
    let prompt = crate::prompt_settings::resolved_generation_text(
        store,
        PromptSlotId::OutlineDeepDive,
        kind,
    );
    if payload["prompts"]["deepDive"].as_str() != Some(prompt.as_str())
        || payload["prompts"]["paperRoot"].as_str() != Some(root_prompt)
    {
        return Err("局部图提示词已变化，请重新计划。".into());
    }
    outline.check_frozen_inputs(payload)?;
    let current = outline
        .deep_dive_for_node(
            payload["revisionId"].as_str().unwrap_or(""),
            payload["nodeId"].as_str().unwrap_or(""),
        )?
        .map(|h| h.id);
    if current.as_deref() != payload["expectedLocalHeadId"].as_str() {
        return Err("局部图版本已变化，请重新计划。".into());
    }
    Ok(())
}
