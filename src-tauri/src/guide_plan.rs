//! Prepare frozen generation inputs; publication and enqueue keep their existing owners.
use crate::guide_commands::ReadingGuideRequest;
use crate::guide_memo::{digest_text, memo_cache_digest, GuideMemoCacheKey};
use crate::guide_module::GuidePlan;
use crate::*;

pub(crate) fn prepare(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    current: &CurrentPaperReadRoute,
    request: &ReadingGuideRequest,
) -> AppResult<(GuidePlan, Value)> {
    let guide = &runtime.guide_module;
    let route_id = current.route.frozen().route_id().database_value();
    let mut plan = guide.plan_for_route(
        &request.revision_id,
        &route_id,
        &current.model,
        current.supports_native_pdf,
    )?;
    let kind = revision_document_kind_from_root(&runtime.root, &request.revision_id);
    let locale = resolve_ui_locale(app)?.unwrap_or(crate::ui_locale::UiLocale::ZhCn);
    let language = crate::ui_locale::output_language(locale);
    let prompts = load_store_in(&prompt_settings_file(app)?, locale)?;
    let v2 = crate::prompt_settings::guide_workflow_protocol(&prompts, kind)? == "v2";
    let context = load_prompt_text(app, PromptSlotId::GuideContext, kind, Some(language))?;
    let annotate = load_prompt_text(app, PromptSlotId::GuideAnnotate, kind, Some(language))?;
    let root_prompt = load_prompt_text(app, PromptSlotId::PaperRoot, kind, Some(language))?;
    let reader = resolve_reader_wrapper_for_revision(&runtime.root, &request.revision_id);
    let revision = revision_record(&open_db(&runtime.root)?, &request.revision_id)?;
    let paper_id = guide.paper_id_for_revision(&request.revision_id)?;
    let mut payload = json!({"readerContext":reader,"ocrRevisionId":plan.ocr_revision_id,
        "catalogDigest":plan.catalog_digest,"pdfDigest":revision.sha256,
        "documentKind":kind.as_str(),"prompts":{"context":context,"annotate":annotate,"paperRoot":root_prompt},
        "guideProtocol":crate::guide_protocol::GUIDE_PROTOCOL_VERSION,"reusedOutline":plan.reused_outline});
    if v2 {
        let store = crate::guide_character_settings::load_store_file(&runtime.root)?;
        let ids = request.character_ids.clone().unwrap_or(
            guide
                .document_cast(&paper_id)?
                .unwrap_or_else(|| store.default_character_ids.clone()),
        );
        if ids.is_empty() {
            return Err("至少选择一位批注角色".into());
        }
        let mut avatars = Vec::new();
        for character in store.characters.iter().filter(|c| ids.contains(&c.id)) {
            if let Some(rel) = character.avatar_rel_path.as_deref() {
                let (asset, bytes) =
                    crate::guide_character_assets::read_workspace_avatar(&runtime.root, rel)?;
                let path = crate::guide_character_assets::copy_to_workspace(
                    &runtime.root,
                    &asset,
                    &bytes,
                )?;
                avatars.push((
                    asset.asset_id,
                    path.strip_prefix(&runtime.root)
                        .map_err(|e| e.to_string())?
                        .to_string_lossy()
                        .replace('\\', "/"),
                ));
            }
        }
        let localized = crate::guide_character_settings::localized_for_generation(&store, locale);
        let cast = crate::guide_cast::snapshot_from_store(&localized, &ids, &avatars)?;
        let overhead = annotate.chars().count()
            + reader.as_ref().map(|s| s.chars().count()).unwrap_or(0)
            + crate::guide_cast::cast_prompt_block(&cast).chars().count()
            + 6000
            + 2500
            + 4000
            + 1200;
        // Conservative character bound, including fixed context and output reserve.
        let limit = (current.input_token_limit.unwrap_or(32000).max(1) as usize)
            .min(crate::guide_protocol::GUIDE_V2_BATCH_CHAR_BUDGET);
        let budget = limit
            .checked_sub(overhead)
            .filter(|b| *b >= 2000)
            .ok_or("固定上下文超出旁批预算，请缩短人物设定或选择更大上下文模型")?;
        let (_, catalog, _, _) = guide.locatable_pages_for(&request.revision_id)?;
        let blocks = guide.catalog_blocks_full(&plan.ocr_revision_id, &catalog)?;
        let batches = crate::guide_catalog::plan_material_batches(
            &crate::guide_catalog::materials_from_catalog(&blocks, 800),
            budget,
        );
        let key = memo_cache_digest(&GuideMemoCacheKey {
            revision_id: request.revision_id.clone(),
            pdf_digest: revision.sha256.clone(),
            ocr_revision_id: plan.ocr_revision_id.clone(),
            catalog_digest: plan.catalog_digest.clone(),
            document_kind: kind.as_str().into(),
            memo_protocol: crate::guide_protocol::GUIDE_MEMO_PROTOCOL.into(),
            context_prompt_digest: digest_text(&format!("{language}\n{context}")),
            root_prompt_digest: digest_text(&root_prompt),
            reader_digest: digest_text(reader.as_deref().unwrap_or("")),
            route_id,
            model: current.model.clone(),
        });
        let cached = guide.load_memo(&key)?;
        let available = cached.as_ref().is_some_and(|v| {
            crate::guide_memo::parse_guide_memo(&v.to_string())
                .and_then(|m| {
                    crate::guide_memo::validate_memo_locations(&m, plan.page_count, &blocks)
                })
                .is_ok()
        });
        let source = ReadingArtifactModule::open(&runtime.root)?;
        let facts = reading_artifact_module::DocumentFacts {
            paper_id,
            revision_id: revision.id.clone(),
            revision_sha256: revision.sha256.clone(),
            title: revision.title.clone(),
            pdf_path: absolute_pdf(&runtime.root, &revision.pdf_path),
        };
        plan.has_paper_root = source
            .lookup_root_for_route_with_instruction(
                &facts,
                &current.route.frozen().route_id().database_value(),
                &current.model,
                Some(&root_prompt),
            )
            .map_err(|e| e.to_string())?
            .is_some();
        plan.root_calls = if available || plan.has_paper_root {
            0
        } else {
            1
        };
        plan.batch_count = batches.len() as i64;
        plan.annotation_calls = plan.batch_count;
        plan.understand_calls = if available { 0 } else { 1 };
        plan.repair_calls = plan.batch_count * 2 + if available { 0 } else { 1 };
        plan.reused_outline = false;
        plan.character_ids = Some(ids.clone());
        payload["guideProtocol"] = json!(crate::guide_protocol::GUIDE_PROTOCOL_V2);
        payload["outputLanguage"] = json!(language);
        payload["memoProtocol"] = json!(crate::guide_protocol::GUIDE_MEMO_PROTOCOL);
        payload["reusedOutline"] = json!(false);
        payload["memoCacheKey"] = json!(key);
        payload["characterIds"] = json!(ids);
        payload["castSnapshot"] = json!(cast);
        payload["batches"] = json!(batches);
        payload["inputCharLimit"] = json!(limit - 4000);
        payload["materialCharBudget"] = json!(budget);
    }
    plan.guide_protocol = payload["guideProtocol"].as_str().map(String::from);
    Ok((plan, payload))
}
