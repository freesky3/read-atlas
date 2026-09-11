use crate::guide_cast::{cast_prompt_block, GuideCastSnapshot};
use crate::guide_catalog::{catalog_value, GuideBatchPlan};
use crate::guide_coverage::{
    build_coverage_report, count_notes, needs_supplement, GuideCoverageReport,
};
use crate::guide_memo::{select_memo_for_batch, GuideMemo};
use crate::guide_protocol::{
    GUIDE_BATCH_MAX_LOGIC_CALLS, GUIDE_PROTOCOL_V2, GUIDE_STAGE1_MAX_CALLS,
};
use crate::guide_validate::GuideInk;
use crate::guide_validate_v2::validate_guide_inks_v2;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideV2PlanMeta {
    pub plan_id: String,
    pub plan_digest: String,
    pub protocol: String,
    pub output_language: String,
}

pub fn v2_plan_meta(plan_id: &str, plan_digest: &str, language: &str) -> GuideV2PlanMeta {
    GuideV2PlanMeta {
        plan_id: plan_id.to_string(),
        plan_digest: plan_digest.to_string(),
        protocol: GUIDE_PROTOCOL_V2.to_string(),
        output_language: language.to_string(),
    }
}

pub fn job_is_v2(payload: &Value) -> bool {
    payload
        .get("guideProtocol")
        .and_then(Value::as_str)
        .map(crate::guide_protocol::is_v2_protocol)
        .unwrap_or(false)
}

pub fn unknown_future_protocol(payload: &Value) -> Option<String> {
    let protocol = payload.get("guideProtocol").and_then(Value::as_str)?;
    if protocol == crate::guide_protocol::GUIDE_PROTOCOL_VERSION
        || crate::guide_protocol::is_v2_protocol(protocol)
    {
        None
    } else {
        Some(protocol.to_string())
    }
}

pub fn memo_user_input(catalog: &Value, language: &str) -> String {
    json!({
        "task": "build_reading_memo",
        "catalog": catalog,
        "language": language,
        "instructions": "只返回读后备忘 JSON。现在不要写人物旁批。"
    })
    .to_string()
}

pub fn annotate_user_input(
    snapshot: &GuideCastSnapshot,
    memo: Option<&GuideMemo>,
    batch: &GuideBatchPlan,
    task: &str,
    repair_hint: Option<&str>,
    accepted_summary: Option<&Value>,
    language: &str,
) -> String {
    json!({
        "task": task,
        "language": language,
        "cast": {
            "order": snapshot.order,
            "block": cast_prompt_block(snapshot)
        },
        "memo": memo.map(|item| select_memo_for_batch(item, batch.page_start, batch.page_end)),
        "batch": {
            "ordinal": batch.ordinal,
            "pageStart": batch.page_start,
            "pageEnd": batch.page_end,
            "anchorBlockIds": batch.anchor_block_ids,
            "contextBlockIds": batch.context_block_ids,
            "catalog": catalog_value(batch)
        },
        "acceptedSummary": accepted_summary,
        "repairHint": repair_hint
    })
    .to_string()
}

pub fn merge_validated_batch(
    accepted: &mut Vec<GuideInk>,
    incoming: Vec<GuideInk>,
    replace_ids: &HashSet<String>,
) {
    if !replace_ids.is_empty() {
        accepted.retain(|ink| !replace_ids.contains(&ink_id(ink)));
    }
    let mut existing: HashSet<String> = accepted.iter().map(ink_id).collect();
    for ink in incoming {
        let id = ink_id(&ink);
        if !existing.insert(id) {
            continue;
        }
        // Later fragments and supplement calls cannot overwrite a correct note.
        if let GuideInk::Note { anchor, .. } = &ink {
            if accepted.iter().any(|old| matches!(old, GuideInk::Note { anchor: other, .. } if other.block_id == anchor.block_id)) { continue; }
        }
        if let GuideInk::Reply { parent_id, .. } = &ink {
            if !accepted
                .iter()
                .any(|old| matches!(old, GuideInk::Note { id, .. } if id == parent_id))
            {
                continue;
            }
        }
        accepted.push(ink);
    }
}

pub fn validate_batch(
    raw: &Value,
    catalog: &[crate::guide_validate::GuideCatalogBlock],
    snapshot: &GuideCastSnapshot,
    batch: &GuideBatchPlan,
) -> crate::guide_validate_v2::GuideV2Validation {
    validate_guide_inks_v2(
        raw,
        catalog,
        &snapshot.order,
        &batch.anchor_block_ids,
        &format!("b{}", batch.ordinal),
    )
}

pub fn chinese_repair_hint(errors: &[String]) -> String {
    format!(
        "上次输出未通过结构校验。请只返回 {{\"inks\":[...]}}。speakerId 必须属于本次阵容。blockId 只能使用本批 anchor 块。错误：{}",
        errors.iter().take(8).cloned().collect::<Vec<_>>().join("；")
    )
}

pub fn chinese_supplement_hint(report: &GuideCoverageReport) -> String {
    format!(
        "本批文字旁批不足。不要把 trace 算作文字覆盖。只补充遗漏的主 note，不要重写已有正确 note。缺口页：{}",
        report
            .sparse_pages
            .iter()
            .map(|page| page.to_string())
            .collect::<Vec<_>>()
            .join("、")
    )
}

pub fn batch_logic_budget() -> u32 {
    GUIDE_BATCH_MAX_LOGIC_CALLS
}

pub fn stage1_budget() -> u32 {
    GUIDE_STAGE1_MAX_CALLS
}

pub fn publishable_notes(inks: &[GuideInk]) -> bool {
    count_notes(inks) > 0
}

pub fn coverage_for(
    pages: &[(i64, String, usize)],
    inks: &[GuideInk],
    failed_batches: i64,
) -> GuideCoverageReport {
    build_coverage_report(pages, inks, failed_batches, Vec::new(), Vec::new())
}

pub fn should_supplement(report: &GuideCoverageReport, calls_used: u32) -> bool {
    calls_used < GUIDE_BATCH_MAX_LOGIC_CALLS && needs_supplement(report)
}

fn ink_id(ink: &GuideInk) -> String {
    match ink {
        GuideInk::Trace { id, .. } | GuideInk::Note { id, .. } | GuideInk::Reply { id, .. } => {
            id.clone()
        }
    }
}

pub fn fictional_preview_catalog() -> Vec<Value> {
    vec![
        json!({
            "blockId": "preview-1",
            "pageNumber": 1,
            "blockIndex": 1,
            "blockType": "Text",
            "bbox": [40, 80, 520, 140],
            "text": "若函数在闭区间上连续，则它一定能取到最大值和最小值。这个结论看起来平常，却把“连续”从直觉变成函数值在区间内能达到上下界的保证。"
        }),
        json!({
            "blockId": "preview-2",
            "pageNumber": 1,
            "blockIndex": 2,
            "blockType": "Text",
            "bbox": [40, 150, 520, 210],
            "text": "证明并不依赖函数是否可导。只要中间某处断开，最值就可能溜走；书上常把这一点一笔带过。"
        }),
        json!({
            "blockId": "preview-3",
            "pageNumber": 1,
            "blockIndex": 3,
            "blockType": "Text",
            "bbox": [40, 220, 520, 280],
            "text": "后面讨论极值时会回到这里：极值是局部的，最值是全局的。先把这个差别钉住，后面才不容易混。"
        }),
    ]
}

/// A fixed logical slot retains its paid outcome before any parsing. Resuming
/// replays that outcome; an in-flight request with unknown outcome is not resent.
pub async fn call_once<T, F, P>(
    checkpoint: &mut Value,
    slot: &str,
    mut persist: P,
    call: F,
) -> Result<T, crate::provider_ports::ProviderError>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
    F: std::future::Future<Output = Result<T, crate::provider_ports::ProviderError>>,
    P: FnMut(&Value) -> Result<(), String>,
{
    use crate::provider_ports::ProviderError;
    if let Some(saved) = checkpoint.get("calls").and_then(|calls| calls.get(slot)) {
        if let Some(response) = saved.get("response") {
            return serde_json::from_value(response.clone())
                .map_err(|e| ProviderError::local_state(e.to_string()));
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
    checkpoint["calls"][slot] = json!({"state":"inflight"});
    persist(checkpoint).map_err(ProviderError::local_state)?;
    match call.await {
        Ok(response) => {
            checkpoint["calls"][slot] = json!({"state":"received","response":response});
            persist(checkpoint).map_err(ProviderError::local_state)?;
            Ok(response)
        }
        Err(error) => {
            checkpoint["calls"][slot] = json!({"state":"failed","error":error.to_string()});
            persist(checkpoint).map_err(ProviderError::local_state)?;
            Err(error)
        }
    }
}

pub fn limited_memo(memo: &GuideMemo, batch: &GuideBatchPlan, budget: usize) -> Value {
    let mut selected = select_memo_for_batch(memo, batch.page_start, batch.page_end);
    // Derived context can omit low-priority entries, but source blocks remain full.
    for key in [
        "observations",
        "connections",
        "spans",
        "pageHints",
        "limitations",
    ] {
        while selected.to_string().chars().count() > budget {
            let Some(items) = selected[key].as_array_mut() else {
                break;
            };
            if items.pop().is_none() {
                break;
            }
        }
    }
    selected
}

pub fn page_coverage(
    catalog: &[crate::guide_validate::GuideCatalogBlock],
    page_count: i64,
    memo: Option<&GuideMemo>,
    inks: &[GuideInk],
    failed: i64,
) -> GuideCoverageReport {
    let pages = (1..=page_count)
        .map(|page| {
            let chars = catalog
                .iter()
                .filter(|b| b.page_number == page)
                .map(|b| b.excerpt.as_deref().unwrap_or("").chars().count())
                .sum();
            let hint = memo
                .and_then(|m| m.page_hints.iter().find(|h| h.page_number == page))
                .map(|h| h.kind.as_str());
            let kind = match hint {
                Some("chrome" | "cover" | "contents" | "references") => "chrome",
                Some("unreadable") => "unreadable",
                Some("dense") => "dense",
                Some("transition") => "transition",
                _ => crate::guide_coverage::classify_page(chars, false),
            };
            (page, kind.to_string(), chars)
        })
        .collect::<Vec<_>>();
    coverage_for(&pages, inks, failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guide_cast::snapshot_from_store;
    use crate::guide_character_settings::{factory_store, PRESET_CHITANDA};
    use crate::guide_validate::GuideCatalogBlock;

    fn catalog_block(id: &str) -> GuideCatalogBlock {
        GuideCatalogBlock {
            id: id.into(),
            page_number: 1,
            block_index: 0,
            block_type: "Text".into(),
            bbox: [0, 0, 1, 1],
            excerpt: Some("完整正文，没有先截成 240 字。".into()),
        }
    }

    #[tokio::test]
    async fn paid_outcome_is_saved_before_parse_and_replayed_without_a_call() {
        let mut checkpoint = json!({});
        let mut writes = Vec::new();
        let first: String = call_once(
            &mut checkpoint,
            "b1:initial",
            |c| {
                writes.push(c.clone());
                Ok(())
            },
            async { Ok("invalid JSON but paid".to_string()) },
        )
        .await
        .unwrap();
        assert_eq!(writes[0]["calls"]["b1:initial"]["state"], "inflight");
        assert_eq!(writes[1]["calls"]["b1:initial"]["response"], first);
        let second: String = call_once(&mut checkpoint, "b1:initial", |_| Ok(()), async {
            panic!("must not repay")
        })
        .await
        .unwrap();
        assert_eq!(second, first);
        checkpoint["calls"]["b2:initial"] = json!({"state":"inflight"});
        let unknown = call_once::<String, _, _>(&mut checkpoint, "b2:initial", |_| Ok(()), async {
            panic!("must not resend unknown")
        })
        .await;
        assert!(unknown.is_err());
    }

    #[test]
    fn repair_replaces_instead_of_accumulating() {
        let store = factory_store();
        let snapshot = snapshot_from_store(&store, &[PRESET_CHITANDA.to_string()], &[]).unwrap();
        let batch = GuideBatchPlan {
            ordinal: 1,
            page_start: 1,
            page_end: 1,
            anchor_block_ids: vec!["b1".into()],
            context_block_ids: Vec::new(),
            materials: Vec::new(),
        };
        let first = validate_batch(
            &json!({"inks":[{
                "id":"n1","kind":"note","speakerId":PRESET_CHITANDA,
                "blockId":"b1","weight":"line","body":"第一次","parentId":null
            }]}),
            &[catalog_block("b1")],
            &snapshot,
            &batch,
        );
        let mut accepted = first.inks;
        let second = validate_batch(
            &json!({"inks":[{
                "id":"n1","kind":"note","speakerId":PRESET_CHITANDA,
                "blockId":"b1","weight":"line","body":"修复后","parentId":null
            }]}),
            &[catalog_block("b1")],
            &snapshot,
            &batch,
        );
        let replace = accepted.iter().map(ink_id).collect();
        merge_validated_batch(&mut accepted, second.inks, &replace);
        assert_eq!(count_notes(&accepted), 1);
        match &accepted[0] {
            GuideInk::Note { body, .. } => assert!(body.contains("修复后")),
            _ => panic!("expected note"),
        }
    }

    #[test]
    fn future_protocol_is_not_silently_v1() {
        assert_eq!(
            unknown_future_protocol(&json!({"guideProtocol":"reading-guide-desktop-v9"}))
                .as_deref(),
            Some("reading-guide-desktop-v9")
        );
        assert!(unknown_future_protocol(&json!({})).is_none());
        assert!(job_is_v2(&json!({"guideProtocol": GUIDE_PROTOCOL_V2})));
    }
}
