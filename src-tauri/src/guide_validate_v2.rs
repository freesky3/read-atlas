use crate::guide_validate::{GuideCatalogBlock, GuideInk, GuideLocator};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq)]
pub struct GuideV2Validation {
    pub inks: Vec<GuideInk>,
    pub dropped: i64,
    pub warnings: Vec<String>,
}

pub fn validate_guide_inks_v2(
    raw: &Value,
    catalog: &[GuideCatalogBlock],
    cast_ids: &[String],
    anchor_block_ids: &[String],
    batch_namespace: &str,
) -> GuideV2Validation {
    let mut warnings = Vec::new();
    let mut dropped = 0_i64;
    let Some(items) = raw
        .get("inks")
        .and_then(Value::as_array)
        .or_else(|| raw.as_array())
    else {
        return GuideV2Validation {
            inks: Vec::new(),
            dropped: 1,
            warnings: vec!["V2 旁批必须返回 {\"inks\":[...]}".to_string()],
        };
    };
    if let Some(object) = raw.as_object() {
        for key in object.keys() {
            if key != "inks" {
                return GuideV2Validation {
                    inks: Vec::new(),
                    dropped: items.len().max(1) as i64,
                    warnings: vec![format!("未允许的顶层字段：{key}")],
                };
            }
        }
    }
    let catalog_map: HashMap<&str, &GuideCatalogBlock> = catalog
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect();
    let anchors: HashSet<&str> = anchor_block_ids.iter().map(String::as_str).collect();
    let cast: HashSet<&str> = cast_ids.iter().map(String::as_str).collect();
    let mut notes_by_parent: HashSet<String> = HashSet::new();
    let mut accepted_notes: HashMap<String, String> = HashMap::new();
    let mut traces: HashSet<(String, String)> = HashSet::new();
    let mut replies_by_parent: HashMap<String, usize> = HashMap::new();
    let mut seen_ids: HashSet<String> = HashSet::new();
    let mut inks = Vec::new();
    for item in items
        .iter()
        .filter(|v| v["kind"] != "reply")
        .chain(items.iter().filter(|v| v["kind"] == "reply"))
    {
        match parse_v2_item(item, &catalog_map, &anchors, &cast, batch_namespace) {
            Ok(ink) => {
                let id = ink_id(&ink);
                if !seen_ids.insert(id.clone()) {
                    dropped += 1;
                    warnings.push(format!("重复 id：{id}"));
                    continue;
                }
                match &ink {
                    GuideInk::Note { anchor, .. } => {
                        if !notes_by_parent.insert(anchor.block_id.clone()) {
                            dropped += 1;
                            warnings.push(format!("同一块已有主 note：{}", anchor.block_id));
                            continue;
                        }
                        accepted_notes.insert(id, anchor.block_id.clone());
                        inks.push(ink);
                    }
                    GuideInk::Trace {
                        speaker_id, anchor, ..
                    } => {
                        if !traces.insert((speaker_id.clone(), anchor.block_id.clone())) {
                            continue;
                        }
                        inks.push(ink);
                    }
                    GuideInk::Reply { parent_id, .. } => {
                        if accepted_notes.get(parent_id).map(String::as_str)
                            != item["blockId"].as_str()
                        {
                            dropped += 1;
                            warnings.push(format!("reply 缺少本批 note 父项：{parent_id}"));
                            continue;
                        }
                        let count = replies_by_parent.entry(parent_id.clone()).or_default();
                        if *count >= 2 {
                            dropped += 1;
                            warnings.push(format!("同一 note 回复超过两条：{parent_id}"));
                            continue;
                        }
                        *count += 1;
                        inks.push(ink);
                    }
                }
            }
            Err(message) => {
                dropped += 1;
                warnings.push(message);
            }
        }
    }
    GuideV2Validation {
        inks,
        dropped,
        warnings,
    }
}

fn parse_v2_item(
    value: &Value,
    catalog: &HashMap<&str, &GuideCatalogBlock>,
    anchors: &HashSet<&str>,
    cast: &HashSet<&str>,
    batch_namespace: &str,
) -> Result<GuideInk, String> {
    let object = value
        .as_object()
        .ok_or_else(|| "墨迹项必须是对象".to_string())?;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "id" | "kind" | "speakerId" | "blockId" | "weight" | "body" | "parentId"
        ) {
            return Err(format!("墨迹含有未允许字段：{key}"));
        }
    }
    for key in [
        "id",
        "kind",
        "speakerId",
        "blockId",
        "weight",
        "body",
        "parentId",
    ] {
        if !object.contains_key(key) {
            return Err(format!("墨迹缺少字段：{key}"));
        }
    }
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "缺少 kind".to_string())?;
    let speaker = object
        .get("speakerId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if speaker.is_empty() || !cast.contains(speaker) {
        return Err(format!("speakerId 不在本次阵容：{speaker}"));
    }
    let raw_id = object
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if raw_id.is_empty() {
        return Err("缺少 id".to_string());
    }
    let id = scoped_id(batch_namespace, raw_id);
    let block_id = object
        .get("blockId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let body = object
        .get("body")
        .and_then(Value::as_str)
        .ok_or_else(|| "body 必须是字符串".to_string())?
        .to_string();
    let weight = object.get("weight");
    let parent_id = object.get("parentId");
    match kind {
        "trace" => {
            if !body.trim().is_empty() {
                return Err(format!("{raw_id}：trace 的 body 必须是空字符串"));
            }
            if !weight.is_none() && !weight.unwrap().is_null() {
                return Err(format!("{raw_id}：trace 的 weight 必须为 null"));
            }
            if !parent_id.is_none() && !parent_id.unwrap().is_null() {
                return Err(format!("{raw_id}：trace 的 parentId 必须为 null"));
            }
            Ok(GuideInk::Trace {
                id,
                speaker_id: speaker.to_string(),
                anchor: locator(&block_id, catalog, anchors)?,
            })
        }
        "note" => {
            if body.trim().is_empty() {
                return Err(format!("{raw_id}：note 正文不能为空"));
            }
            let weight = weight
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{raw_id}：note 的 weight 必须是 line 或 short"))?;
            if weight != "line" && weight != "short" {
                return Err(format!("{raw_id}：note 的 weight 必须是 line 或 short"));
            }
            if !parent_id.is_none() && !parent_id.unwrap().is_null() {
                return Err(format!("{raw_id}：note 的 parentId 必须为 null"));
            }
            Ok(GuideInk::Note {
                id,
                speaker_id: speaker.to_string(),
                weight: weight.to_string(),
                anchor: locator(&block_id, catalog, anchors)?,
                body,
            })
        }
        "reply" => {
            if body.trim().is_empty() {
                return Err(format!("{raw_id}：reply 正文不能为空"));
            }
            let parent = parent_id
                .and_then(Value::as_str)
                .ok_or_else(|| format!("{raw_id}：reply 需要 parentId"))?;
            if !weight.is_none() && !weight.unwrap().is_null() {
                return Err(format!("{raw_id}：reply 的 weight 必须为 null"));
            }
            let parent = scoped_id(batch_namespace, parent);
            let _ = locator(&block_id, catalog, anchors)?;
            Ok(GuideInk::Reply {
                id,
                speaker_id: speaker.to_string(),
                parent_id: parent,
                body,
            })
        }
        other => Err(format!("未知 kind：{other}")),
    }
}

// Empty namespace is reserved for trusted stored ink IDs. Model IDs are always scoped.
fn scoped_id(namespace: &str, id: &str) -> String {
    if namespace.is_empty() {
        id.to_string()
    } else {
        format!("{namespace}:{id}")
    }
}

/// Convert the persisted UI shape back through strict V2 validation without
/// accepting its `anchor` field from model responses or renaming saved IDs.
pub fn restore_guide_inks_v2(
    raw: &Value,
    catalog: &[GuideCatalogBlock],
    cast_ids: &[String],
) -> GuideV2Validation {
    let items = raw.as_array().cloned().unwrap_or_default();
    let parents: HashMap<String, String> = items
        .iter()
        .filter(|v| v["kind"] == "note")
        .filter_map(|v| {
            Some((
                v["id"].as_str()?.into(),
                v["anchor"]["blockId"].as_str()?.into(),
            ))
        })
        .collect();
    let normalized: Vec<Value> = items
        .iter()
        .map(|v| {
            let kind = v["kind"].as_str().unwrap_or("");
            let block = if kind == "reply" {
                parents
                    .get(v["parentId"].as_str().unwrap_or(""))
                    .map(String::as_str)
                    .unwrap_or("")
            } else {
                v["anchor"]["blockId"].as_str().unwrap_or("")
            };
            json!({"id":v["id"],"kind":kind,"speakerId":v["speakerId"],"blockId":block,
            "weight":v.get("weight").unwrap_or(&Value::Null),
            "body":v.get("body").cloned().unwrap_or(json!("")),
            "parentId":v.get("parentId").unwrap_or(&Value::Null)})
        })
        .collect();
    let anchors = catalog.iter().map(|b| b.id.clone()).collect::<Vec<_>>();
    validate_guide_inks_v2(&json!(normalized), catalog, cast_ids, &anchors, "")
}

fn locator(
    block_id: &str,
    catalog: &HashMap<&str, &GuideCatalogBlock>,
    anchors: &HashSet<&str>,
) -> Result<GuideLocator, String> {
    if block_id.is_empty() {
        return Err("缺少 blockId".to_string());
    }
    if !anchors.contains(block_id) {
        return Err(format!("blockId 不在本批落笔白名单：{block_id}"));
    }
    let block = catalog
        .get(block_id)
        .ok_or_else(|| format!("blockId 不在当前 OCR 目录：{block_id}"))?;
    Ok(GuideLocator {
        block_id: block.id.clone(),
        page_number: block.page_number,
        block_type: block.block_type.clone(),
        bbox: block.bbox,
    })
}

fn ink_id(ink: &GuideInk) -> String {
    match ink {
        GuideInk::Trace { id, .. } | GuideInk::Note { id, .. } | GuideInk::Reply { id, .. } => {
            id.clone()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn block(id: &str) -> GuideCatalogBlock {
        GuideCatalogBlock {
            id: id.into(),
            page_number: 1,
            block_index: 0,
            block_type: "Text".into(),
            bbox: [0, 0, 1, 1],
            excerpt: Some("hello".into()),
        }
    }

    #[test]
    fn stored_round_trip_keeps_ids_anchors_and_replies() {
        let cast = vec!["preset:chitanda".into()];
        let catalog = vec![block("b1")];
        let raw = json!({"inks":[
            {"id":"note:1","kind":"note","speakerId":cast[0],"blockId":"b1","weight":"line","body":"留下来","parentId":null},
            {"id":"r1","kind":"reply","speakerId":cast[0],"blockId":"b1","weight":null,"body":"有道理","parentId":"note:1"}
        ]});
        let first = validate_guide_inks_v2(&raw, &catalog, &cast, &["b1".into()], "b1-initial");
        assert_eq!(first.inks.len(), 2);
        let saved = json!(first
            .inks
            .iter()
            .map(GuideInk::to_value)
            .collect::<Vec<_>>());
        let restored = restore_guide_inks_v2(&saved, &catalog, &cast);
        assert_eq!(restored.inks, first.inks);
        assert_eq!(restore_guide_inks_v2(&saved, &catalog, &cast).dropped, 0);
        assert!(
            validate_guide_inks_v2(&saved, &catalog, &cast, &["b1".into()], "model")
                .inks
                .is_empty()
        );
    }

    #[test]
    fn rejects_unknown_speaker_instead_of_mapping_to_alin() {
        let raw = json!({"inks":[{
            "id":"n1","kind":"note","speakerId":"alin","blockId":"b1",
            "weight":"line","body":"x","parentId":null
        }]});
        let outcome = validate_guide_inks_v2(
            &raw,
            &[block("b1")],
            &["preset:chitanda".into()],
            &["b1".into()],
            "b1",
        );
        assert!(outcome.inks.is_empty());
        assert!(outcome.warnings.iter().any(|item| item.contains("阵容")));
    }

    #[test]
    fn rejects_context_block_and_extra_fields() {
        let raw = json!({"inks":[{
            "id":"n1","kind":"note","speakerId":"preset:chitanda","blockId":"ctx",
            "weight":"line","body":"x","parentId":null,"pageNumber":1
        }]});
        let outcome = validate_guide_inks_v2(
            &raw,
            &[block("b1"), block("ctx")],
            &["preset:chitanda".into()],
            &["b1".into()],
            "b1",
        );
        assert!(outcome.inks.is_empty());
    }

    #[test]
    fn one_note_per_parent_and_two_replies() {
        let raw = json!({"inks":[
            {"id":"n1","kind":"note","speakerId":"preset:chitanda","blockId":"b1","weight":"line","body":"一","parentId":null},
            {"id":"n2","kind":"note","speakerId":"preset:oreki","blockId":"b1","weight":"line","body":"二","parentId":null},
            {"id":"r1","kind":"reply","speakerId":"preset:oreki","blockId":"b1","weight":null,"body":"接","parentId":"n1"},
            {"id":"r2","kind":"reply","speakerId":"preset:frieren","blockId":"b1","weight":null,"body":"再","parentId":"n1"},
            {"id":"r3","kind":"reply","speakerId":"preset:oreki","blockId":"b1","weight":null,"body":"超","parentId":"n1"}
        ]});
        let outcome = validate_guide_inks_v2(
            &raw,
            &[block("b1")],
            &[
                "preset:chitanda".into(),
                "preset:oreki".into(),
                "preset:frieren".into(),
            ],
            &["b1".into()],
            "batch1",
        );
        assert_eq!(
            outcome
                .inks
                .iter()
                .filter(|ink| matches!(ink, GuideInk::Note { .. }))
                .count(),
            1
        );
        assert_eq!(
            outcome
                .inks
                .iter()
                .filter(|ink| matches!(ink, GuideInk::Reply { .. }))
                .count(),
            2
        );
    }
}
