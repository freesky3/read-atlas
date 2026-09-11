use crate::guide_personas::{is_known_speaker, normalize_speaker};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

const ANCHOR_IOU: f64 = 0.5;
const MAX_REPLIES_PER_NOTE: usize = 2;

#[derive(Debug, Clone, PartialEq)]
pub struct GuideCatalogBlock {
    pub id: String,
    pub page_number: i64,
    pub block_index: i64,
    pub block_type: String,
    pub bbox: [i64; 4],
    pub excerpt: Option<String>,
}

impl GuideCatalogBlock {
    pub fn shorthand(&self) -> String {
        format!("p{}-{}", self.page_number, self.block_index)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideLocator {
    pub block_id: String,
    pub page_number: i64,
    pub block_type: String,
    pub bbox: [i64; 4],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GuideInk {
    Trace {
        id: String,
        speaker_id: String,
        anchor: GuideLocator,
    },
    Note {
        id: String,
        speaker_id: String,
        weight: String,
        anchor: GuideLocator,
        body: String,
    },
    Reply {
        id: String,
        speaker_id: String,
        parent_id: String,
        body: String,
    },
}

fn unwrap_ink_value(value: &Value) -> &Value {
    let Some(object) = value.as_object() else {
        return value;
    };
    if object.len() != 1 {
        return value;
    }
    let Some((key, inner)) = object.iter().next() else {
        return value;
    };
    if inner.is_object() && !normalize_kind(key).is_empty() {
        inner
    } else {
        value
    }
}

fn parse_ink(value: &Value) -> Result<GuideInk, ()> {
    let value = unwrap_ink_value(value);
    let object = value.as_object().ok_or(())?;
    let body = string_field(
        object,
        &["body", "text", "content", "note", "prose", "comment"],
    );
    let declared = string_field(object, &["kind", "type", "inkKind", "ink_kind", "noteType"]);
    let kind = if let Some(kind) = declared
        .as_deref()
        .map(normalize_kind)
        .filter(|kind| !kind.is_empty())
    {
        kind
    } else if body.is_some() {
        if string_field(object, &["parentId", "parent_id", "inReplyTo"]).is_some() {
            "reply"
        } else {
            "note"
        }
    } else {
        "trace"
    };
    let speaker_id = speaker_from_value(object).ok_or(())?;
    let id = string_field(object, &["id"]).unwrap_or_else(|| format!("{kind}-{speaker_id}"));
    match kind {
        "trace" => Ok(GuideInk::Trace {
            id,
            speaker_id,
            anchor: parse_locator(value).ok_or(())?,
        }),
        "note" => Ok(GuideInk::Note {
            id,
            speaker_id,
            weight: string_field(object, &["weight"]).unwrap_or_else(|| "line".to_string()),
            anchor: parse_locator(value).ok_or(())?,
            body: body.ok_or(())?,
        }),
        "reply" => Ok(GuideInk::Reply {
            id,
            speaker_id,
            parent_id: string_field(object, &["parentId", "parent_id", "inReplyTo"]).ok_or(())?,
            body: body.ok_or(())?,
        }),
        _ => Err(()),
    }
}

fn normalize_kind(kind: &str) -> &'static str {
    let compact = kind
        .trim()
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect::<String>();
    match compact.as_str() {
        "trace" | "mark" | "highlight" | "underline" | "stroke" | "color" => "trace",
        "reply" | "response" | "followup" => "reply",
        "note" | "notes" | "annotation" | "margin" | "comment" | "remark" | "prose" | "line"
        | "short" | "ink" => "note",
        _ => "",
    }
}

fn speaker_from_value(object: &serde_json::Map<String, Value>) -> Option<String> {
    if let Some(text) = string_field(
        object,
        &[
            "speakerId",
            "speaker_id",
            "speaker",
            "persona",
            "personaId",
            "author",
            "name",
            "who",
            "friend",
        ],
    ) {
        return normalize_speaker(&text).map(str::to_string);
    }
    for key in ["speaker", "persona", "author"] {
        if let Some(inner) = object.get(key).and_then(Value::as_object) {
            if let Some(text) = string_field(inner, &["id", "speakerId", "name", "displayName"]) {
                if let Some(speaker) = normalize_speaker(&text) {
                    return Some(speaker.to_string());
                }
            }
        }
    }
    None
}

fn parse_locator(value: &Value) -> Option<GuideLocator> {
    let object = value.as_object()?;
    if let Some(text) = object
        .get("anchor")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
    {
        return Some(GuideLocator {
            block_id: text.to_string(),
            page_number: 0,
            block_type: "paragraph".to_string(),
            bbox: [0, 0, 0, 0],
        });
    }
    let nested = object
        .get("anchor")
        .or_else(|| object.get("locator"))
        .and_then(Value::as_object);
    let source = nested.unwrap_or(object);
    let block_id = if nested.is_some() {
        string_field(source, &["blockId", "block_id", "id", "ref"])
    } else {
        string_field(
            source,
            &["blockId", "block_id", "ref", "evidenceId", "evidence_id"],
        )
    }
    .or_else(|| {
        first_id_in_list(
            object,
            &[
                "evidenceIds",
                "evidence_ids",
                "blockIds",
                "block_ids",
                "catalogIds",
            ],
        )
    })?;
    let page_number = int_field(source, &["pageNumber", "page_number", "page"])
        .or_else(|| int_field(object, &["pageNumber", "page_number", "page"]))
        .unwrap_or(0);
    let block_type = string_field(source, &["blockType", "block_type", "type"])
        .unwrap_or_else(|| "paragraph".to_string());
    let bbox = parse_bbox(source.get("bbox"))
        .or_else(|| parse_bbox(object.get("bbox")))
        .unwrap_or([0, 0, 0, 0]);
    Some(GuideLocator {
        block_id,
        page_number,
        block_type,
        bbox,
    })
}

fn first_id_in_list(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        match object.get(*key) {
            Some(Value::String(text)) => {
                let text = text.trim();
                if !text.is_empty() {
                    return Some(text.to_string());
                }
            }
            Some(Value::Array(items)) => {
                for item in items {
                    if let Some(text) = item.as_str().map(str::trim).filter(|text| !text.is_empty())
                    {
                        return Some(text.to_string());
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn string_field(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(value) = object.get(*key) {
            if let Some(text) = value
                .as_str()
                .map(str::trim)
                .filter(|text| !text.is_empty())
            {
                return Some(text.to_string());
            }
        }
    }
    None
}

fn int_field(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| object.get(*key).and_then(parse_i64))
}

fn parse_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().map(|n| n as i64))
        .or_else(|| value.as_f64().map(|n| n.round() as i64))
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

fn parse_bbox(value: Option<&Value>) -> Option<[i64; 4]> {
    let value = value?;
    if let Some(items) = value.as_array() {
        if items.len() < 4 {
            return None;
        }
        return Some([
            parse_i64(&items[0])?,
            parse_i64(&items[1])?,
            parse_i64(&items[2])?,
            parse_i64(&items[3])?,
        ]);
    }
    let object = value.as_object()?;
    Some([
        int_field(object, &["x0", "left"])?,
        int_field(object, &["y0", "top"])?,
        int_field(object, &["x1", "right"])?,
        int_field(object, &["y1", "bottom"])?,
    ])
}

impl GuideInk {
    pub fn page_number(&self) -> Option<i64> {
        match self {
            Self::Trace { anchor, .. } | Self::Note { anchor, .. } => Some(anchor.page_number),
            Self::Reply { .. } => None,
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            Self::Trace {
                id,
                speaker_id,
                anchor,
            } => serde_json::json!({
                "id": id,
                "kind": "trace",
                "speakerId": speaker_id,
                "anchor": anchor,
            }),
            Self::Note {
                id,
                speaker_id,
                weight,
                anchor,
                body,
            } => serde_json::json!({
                "id": id,
                "kind": "note",
                "speakerId": speaker_id,
                "weight": weight,
                "anchor": anchor,
                "body": body,
            }),
            Self::Reply {
                id,
                speaker_id,
                parent_id,
                body,
            } => serde_json::json!({
                "id": id,
                "kind": "reply",
                "speakerId": speaker_id,
                "parentId": parent_id,
                "body": body,
            }),
        }
    }
}

pub fn parse_guide_context(text: &str) -> Result<Value, String> {
    let parsed = decode_json_value(text)?;
    let object = if parsed.get("thesis").is_some() {
        parsed
    } else if let Some(inner) = parsed
        .get("result")
        .or(parsed.get("data"))
        .or(parsed.get("output"))
    {
        inner.clone()
    } else {
        parsed
    };
    if object.get("thesis").is_none() || object.get("sections").is_none() {
        return Err("Guide context is missing thesis or sections".to_string());
    }
    Ok(object)
}

pub fn decode_inks_envelope(text: &str) -> Result<Value, String> {
    let parsed = decode_json_value(text)?;
    extract_inks_array(&parsed, true)
}

pub fn locator_catalog_value(blocks: &[GuideCatalogBlock]) -> Value {
    compact_locator_catalog(
        blocks,
        crate::guide_protocol::GUIDE_MAX_BLOCKS_PER_PAGE,
        crate::guide_protocol::GUIDE_EXCERPT_CHARS,
    )
}

pub fn compact_locator_catalog(
    blocks: &[GuideCatalogBlock],
    max_per_page: usize,
    excerpt_chars: usize,
) -> Value {
    let mut by_page: std::collections::BTreeMap<i64, Vec<&GuideCatalogBlock>> =
        std::collections::BTreeMap::new();
    for block in blocks {
        by_page.entry(block.page_number).or_default().push(block);
    }
    let mut selected = Vec::new();
    for mut group in by_page.into_values() {
        group.sort_by_key(|block| {
            let kind = block.block_type.to_ascii_lowercase();
            let priority = if kind.contains("formula") || kind.contains("equation") {
                0
            } else if kind.contains("figure") || kind.contains("table") {
                1
            } else if kind.contains("caption") {
                2
            } else {
                3
            };
            let len = block.excerpt.as_ref().map(String::len).unwrap_or(0);
            (priority, std::cmp::Reverse(len), block.block_index)
        });
        let limit = max_per_page.max(1);
        selected.extend(group.into_iter().take(limit));
    }
    Value::Array(
        selected
            .into_iter()
            .map(|block| {
                json!({
                    "blockId": block.id,
                    "ref": block.shorthand(),
                    "pageNumber": block.page_number,
                    "blockIndex": block.block_index,
                    "blockType": block.block_type,
                    "bbox": block.bbox,
                    "excerpt": clip_excerpt(block.excerpt.as_deref(), excerpt_chars)
                })
            })
            .collect(),
    )
}

pub fn compact_guide_context(context: &Value, page_start: i64, page_end: i64) -> Value {
    let thesis = context.get("thesis").cloned().unwrap_or(json!(""));
    let sections = context
        .get("sections")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|section| {
            let start = section
                .get("pageStart")
                .and_then(Value::as_i64)
                .unwrap_or(1);
            let end = section
                .get("pageEnd")
                .and_then(Value::as_i64)
                .unwrap_or(start);
            end >= page_start && start <= page_end
        })
        .collect::<Vec<_>>();
    json!({
        "thesis": thesis,
        "sections": sections
    })
}

fn clip_excerpt(text: Option<&str>, max_chars: usize) -> Option<String> {
    let text = text?.trim();
    if text.is_empty() {
        return None;
    }
    if text.chars().count() <= max_chars {
        Some(text.to_string())
    } else {
        Some(text.chars().take(max_chars).collect())
    }
}

fn decode_json_value(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("empty".to_string());
    }
    if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
        return Ok(parsed);
    }
    let bytes = trimmed.as_bytes();
    let start = bytes
        .iter()
        .position(|byte| *byte == b'{' || *byte == b'[')
        .ok_or_else(|| "Guide JSON is invalid: expected value at line 1 column 1".to_string())?;
    let open = bytes[start];
    let close = if open == b'{' { b'}' } else { b']' };
    let end = bytes
        .iter()
        .rposition(|byte| *byte == close)
        .filter(|end| *end >= start)
        .ok_or_else(|| "Guide JSON is incomplete".to_string())?;
    serde_json::from_str(&trimmed[start..=end])
        .map_err(|error| format!("Guide JSON is invalid: {error}"))
}

fn extract_inks_array(value: &Value, allow_string: bool) -> Result<Value, String> {
    if value.is_array() {
        return Ok(value.clone());
    }
    if let Some(object) = value.as_object() {
        for key in ["inks", "annotations", "notes"] {
            if let Some(inner) = object.get(key) {
                return extract_inks_array(inner, true);
            }
        }
        let mut wrapper: Option<&str> = None;
        for key in ["result", "data", "output"] {
            if object.get(key).is_some() {
                if wrapper.is_some() {
                    return Err("Conflicting Guide wrappers".to_string());
                }
                wrapper = Some(key);
            }
        }
        if let Some(key) = wrapper {
            return extract_inks_array(&object[key], false);
        }
    }
    if allow_string {
        if let Some(text) = value.as_str() {
            let nested: Value = serde_json::from_str(text)
                .map_err(|error| format!("Guide JSON string is invalid: {error}"))?;
            return extract_inks_array(&nested, false);
        }
    }
    Err("Guide response did not contain an inks array".to_string())
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuideValidationOutcome {
    pub inks: Vec<GuideInk>,
    pub dropped: i64,
    pub warnings: Vec<String>,
}

pub fn validate_guide_inks(raw: &Value, blocks: &[GuideCatalogBlock]) -> GuideValidationOutcome {
    let Some(items) = raw.as_array() else {
        return GuideValidationOutcome {
            inks: Vec::new(),
            dropped: 0,
            warnings: vec!["Guide inks must be an array".to_string()],
        };
    };

    let mut catalog: HashMap<String, &GuideCatalogBlock> = HashMap::new();
    for block in blocks {
        catalog.insert(block.id.clone(), block);
        catalog.insert(block.shorthand(), block);
    }
    let mut dropped = 0_i64;
    let mut warnings = Vec::new();
    let mut notes_by_block: HashSet<String> = HashSet::new();
    let mut accepted_notes: HashSet<String> = HashSet::new();
    let mut replies_by_parent: HashMap<String, usize> = HashMap::new();
    let mut accepted = Vec::new();
    let mut pending_replies = Vec::new();

    for item in items {
        match parse_ink(item).or_else(|_| recover_loose_ink(item, &catalog)) {
            Ok(ink) => match ink {
                GuideInk::Reply { .. } => pending_replies.push(ink),
                other => {
                    if let Some(located) = accept_located_ink(
                        other,
                        &catalog,
                        &mut notes_by_block,
                        &mut accepted_notes,
                        &mut dropped,
                        &mut warnings,
                    ) {
                        accepted.push(located);
                    }
                }
            },
            Err(_) => {
                dropped += 1;
                warnings.push(format!(
                    "Dropped an ink that did not match the schema: {}",
                    compact_json(item)
                ));
            }
        }
    }

    for reply in pending_replies {
        let GuideInk::Reply {
            id,
            speaker_id,
            parent_id,
            body,
        } = reply
        else {
            continue;
        };
        if !is_known_speaker(&speaker_id) {
            dropped += 1;
            warnings.push(format!("Dropped reply {id}: unknown speaker"));
            continue;
        }
        if body.trim().is_empty() {
            dropped += 1;
            warnings.push(format!("Dropped reply {id}: empty body"));
            continue;
        }
        if !accepted_notes.contains(&parent_id) {
            dropped += 1;
            warnings.push(format!("Dropped reply {id}: parent note is missing"));
            continue;
        }
        let count = replies_by_parent.entry(parent_id.clone()).or_insert(0);
        if *count >= MAX_REPLIES_PER_NOTE {
            dropped += 1;
            warnings.push(format!(
                "Dropped reply {id}: cluster already has two replies"
            ));
            continue;
        }
        *count += 1;
        accepted.push(GuideInk::Reply {
            id,
            speaker_id,
            parent_id,
            body,
        });
    }

    GuideValidationOutcome {
        inks: accepted,
        dropped,
        warnings,
    }
}

fn recover_loose_ink(
    value: &Value,
    catalog: &HashMap<String, &GuideCatalogBlock>,
) -> Result<GuideInk, ()> {
    let object = value.as_object().ok_or(())?;
    let speaker_id = speaker_from_value(object).unwrap_or_else(|| "alin".to_string());
    let block_id = collect_strings(value)
        .into_iter()
        .find(|text| catalog.contains_key(text))
        .ok_or(())?;
    let body = string_field(
        object,
        &["body", "text", "content", "note", "prose", "comment"],
    );
    if let Some(body) = body {
        Ok(GuideInk::Note {
            id: string_field(object, &["id"]).unwrap_or_else(|| format!("note-{speaker_id}")),
            speaker_id,
            weight: "line".to_string(),
            anchor: GuideLocator {
                block_id,
                page_number: 0,
                block_type: "paragraph".to_string(),
                bbox: [0, 0, 0, 0],
            },
            body,
        })
    } else {
        Ok(GuideInk::Trace {
            id: string_field(object, &["id"]).unwrap_or_else(|| format!("trace-{speaker_id}")),
            speaker_id,
            anchor: GuideLocator {
                block_id,
                page_number: 0,
                block_type: "paragraph".to_string(),
                bbox: [0, 0, 0, 0],
            },
        })
    }
}

fn collect_strings(value: &Value) -> Vec<String> {
    let mut out = Vec::new();
    collect_strings_into(value, &mut out, 0);
    out
}

fn collect_strings_into(value: &Value, out: &mut Vec<String>, depth: usize) {
    if depth > 4 {
        return;
    }
    match value {
        Value::String(text) => {
            let text = text.trim();
            if !text.is_empty() {
                out.push(text.to_string());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_strings_into(item, out, depth + 1);
            }
        }
        Value::Object(object) => {
            for item in object.values() {
                collect_strings_into(item, out, depth + 1);
            }
        }
        _ => {}
    }
}

fn compact_json(value: &Value) -> String {
    let raw = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    if raw.chars().count() <= 180 {
        raw
    } else {
        let trimmed: String = raw.chars().take(180).collect();
        format!("{trimmed}…")
    }
}

fn accept_located_ink(
    ink: GuideInk,
    catalog: &HashMap<String, &GuideCatalogBlock>,
    notes_by_block: &mut HashSet<String>,
    accepted_notes: &mut HashSet<String>,
    dropped: &mut i64,
    warnings: &mut Vec<String>,
) -> Option<GuideInk> {
    let (id, speaker_id, anchor, is_note, weight, body) = match &ink {
        GuideInk::Trace {
            id,
            speaker_id,
            anchor,
        } => (
            id.clone(),
            speaker_id.clone(),
            anchor.clone(),
            false,
            None,
            None,
        ),
        GuideInk::Note {
            id,
            speaker_id,
            weight,
            anchor,
            body,
        } => (
            id.clone(),
            speaker_id.clone(),
            anchor.clone(),
            true,
            Some(weight.clone()),
            Some(body.clone()),
        ),
        GuideInk::Reply { .. } => return None,
    };
    if !is_known_speaker(&speaker_id) {
        *dropped += 1;
        warnings.push(format!("Dropped {id}: unknown speaker"));
        return None;
    }
    if is_note {
        let Some(weight) = weight.as_deref() else {
            return None;
        };
        if weight != "line" && weight != "short" {
            *dropped += 1;
            warnings.push(format!("Dropped note {id}: invalid weight"));
            return None;
        }
        if body.as_deref().is_some_and(|text| text.trim().is_empty()) {
            *dropped += 1;
            warnings.push(format!("Dropped note {id}: empty body"));
            return None;
        }
    }
    let Some(block) = catalog.get(&anchor.block_id) else {
        *dropped += 1;
        warnings.push(format!("Dropped {id}: unknown block {}", anchor.block_id));
        return None;
    };
    if is_note && !notes_by_block.insert(block.id.clone()) {
        *dropped += 1;
        warnings.push(format!(
            "Dropped note {id}: block {} already has a primary speaker",
            block.id
        ));
        return None;
    }
    if is_note {
        accepted_notes.insert(id.clone());
    }
    let snapped = GuideLocator {
        block_id: block.id.clone(),
        page_number: block.page_number,
        block_type: block.block_type.clone(),
        bbox: block.bbox,
    };
    if anchor.page_number != 0
        && anchor.page_number != block.page_number
        && bbox_iou(anchor.bbox, block.bbox) < ANCHOR_IOU
    {
        warnings.push(format!(
            "Snapped {id} onto OCR block {} despite a mismatched box",
            block.id
        ));
    }
    Some(match ink {
        GuideInk::Trace { id, speaker_id, .. } => GuideInk::Trace {
            id,
            speaker_id,
            anchor: snapped,
        },
        GuideInk::Note {
            id,
            speaker_id,
            weight,
            body,
            ..
        } => GuideInk::Note {
            id,
            speaker_id,
            weight,
            anchor: snapped,
            body,
        },
        reply => reply,
    })
}

fn bbox_iou(left: [i64; 4], right: [i64; 4]) -> f64 {
    let x0 = left[0].max(right[0]);
    let y0 = left[1].max(right[1]);
    let x1 = left[2].min(right[2]);
    let y1 = left[3].min(right[3]);
    let intersection = ((x1 - x0).max(0) * (y1 - y0).max(0)) as f64;
    let left_area = ((left[2] - left[0]).max(0) * (left[3] - left[1]).max(0)) as f64;
    let right_area = ((right[2] - right[0]).max(0) * (right[3] - right[1]).max(0)) as f64;
    let union = left_area + right_area - intersection;
    if union <= 0.0 {
        0.0
    } else {
        intersection / union
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn block(id: &str, page: i64, bbox: [i64; 4]) -> GuideCatalogBlock {
        GuideCatalogBlock {
            id: id.to_string(),
            page_number: page,
            block_index: 1,
            block_type: "paragraph".to_string(),
            bbox,
            excerpt: None,
        }
    }

    fn locator(block_id: &str, page: i64, bbox: [i64; 4]) -> Value {
        json!({
            "blockId": block_id,
            "pageNumber": page,
            "blockType": "paragraph",
            "bbox": bbox
        })
    }

    #[test]
    fn unknown_anchor_is_dropped() {
        let raw = json!([{
            "id": "n1",
            "kind": "note",
            "speakerId": "alin",
            "weight": "line",
            "anchor": locator("missing", 1, [10, 10, 100, 40]),
            "body": "ht is after the gate."
        }]);
        let outcome = validate_guide_inks(&raw, &[block("p-1", 1, [10, 10, 100, 40])]);
        assert!(outcome.inks.is_empty());
        assert_eq!(outcome.dropped, 1);
    }

    #[test]
    fn second_note_on_the_same_block_is_dropped() {
        let bbox = [10, 10, 200, 80];
        let raw = json!([
            {
                "id": "n1",
                "kind": "note",
                "speakerId": "alin",
                "weight": "short",
                "anchor": locator("p-1", 1, bbox),
                "body": "Primary explanation."
            },
            {
                "id": "n2",
                "kind": "note",
                "speakerId": "xiaxia",
                "weight": "line",
                "anchor": locator("p-1", 1, bbox),
                "body": "Duplicate speaker."
            },
            {
                "id": "t1",
                "kind": "trace",
                "speakerId": "laozhou",
                "anchor": locator("p-1", 1, bbox)
            }
        ]);
        let outcome = validate_guide_inks(&raw, &[block("p-1", 1, bbox)]);
        assert_eq!(outcome.inks.len(), 2);
        assert_eq!(outcome.dropped, 1);
        assert!(matches!(&outcome.inks[0], GuideInk::Note { id, .. } if id == "n1"));
        assert!(matches!(&outcome.inks[1], GuideInk::Trace { id, .. } if id == "t1"));
    }

    #[test]
    fn reply_without_parent_is_dropped() {
        let bbox = [10, 10, 200, 80];
        let raw = json!([{
            "id": "r1",
            "kind": "reply",
            "speakerId": "laozhou",
            "parentId": "missing",
            "body": "@阿林 one seed is not enough."
        }]);
        let outcome = validate_guide_inks(&raw, &[block("p-1", 1, bbox)]);
        assert!(outcome.inks.is_empty());
        assert_eq!(outcome.dropped, 1);
    }

    #[test]
    fn valid_empty_array_is_not_repaired() {
        let outcome = validate_guide_inks(&json!([]), &[]);
        assert!(outcome.inks.is_empty());
        assert_eq!(outcome.dropped, 0);
        assert!(outcome.warnings.is_empty());
    }

    #[test]
    fn envelope_accepts_inks_object_and_bare_array() {
        let array = decode_inks_envelope("[ ]").expect("bare");
        assert_eq!(array, json!([]));
        let wrapped = decode_inks_envelope(r#"{"inks":[]}"#).expect("wrapped");
        assert_eq!(wrapped, json!([]));
        let nested = decode_inks_envelope(r#"{"result":{"inks":[]}}"#).expect("nested");
        assert_eq!(nested, json!([]));
        assert_eq!(decode_inks_envelope(""), Err("empty".to_string()));
    }

    #[test]
    fn compact_catalog_caps_blocks_per_page() {
        let blocks = (0..20)
            .map(|index| GuideCatalogBlock {
                id: format!("b-{index}"),
                page_number: 1,
                block_index: index,
                block_type: "paragraph".to_string(),
                bbox: [10, 10, 200, 80],
                excerpt: Some("x".repeat((index as usize + 1) * 10)),
            })
            .collect::<Vec<_>>();
        let catalog = compact_locator_catalog(&blocks, 8, 240);
        let items = catalog.as_array().expect("array");
        assert_eq!(items.len(), 8);
        assert_eq!(items[0]["blockId"], json!("b-19"));
    }

    #[test]
    fn locator_catalog_includes_excerpt() {
        let mut item = block("p-1", 1, [10, 10, 200, 80]);
        item.excerpt = Some("The manifold is the thing to hold.".to_string());
        let catalog = locator_catalog_value(&[item]);
        assert_eq!(
            catalog[0]["excerpt"],
            json!("The manifold is the thing to hold.")
        );
        assert_eq!(catalog[0]["ref"], json!("p1-1"));
    }

    #[test]
    fn traces_only_can_publish() {
        let bbox = [10, 10, 200, 80];
        let raw = json!([{
            "id": "t1",
            "kind": "trace",
            "speakerId": "laozhou",
            "anchor": locator("p-1", 1, bbox)
        }]);
        let outcome = validate_guide_inks(&raw, &[block("p-1", 1, bbox)]);
        assert_eq!(outcome.inks.len(), 1);
        assert_eq!(outcome.dropped, 0);
    }

    #[test]
    fn matching_block_id_snaps_bbox_instead_of_dropping() {
        let catalog = [block("p-1", 1, [10, 10, 200, 80])];
        let raw = json!([{
            "id": "n1",
            "kind": "note",
            "speakerId": "alin",
            "weight": "line",
            "anchor": locator("p-1", 1, [800, 800, 900, 900]),
            "body": "Wrong box, right block."
        }]);
        let outcome = validate_guide_inks(&raw, &catalog);
        assert_eq!(outcome.dropped, 0);
        match &outcome.inks[0] {
            GuideInk::Note { anchor, .. } => {
                assert_eq!(anchor.block_id, "p-1");
                assert_eq!(anchor.bbox, [10, 10, 200, 80]);
            }
            other => panic!("expected note, got {other:?}"),
        }
    }

    #[test]
    fn accepts_model_aliases_that_used_to_drop_every_ink() {
        let catalog = [block("p-1", 2, [40, 80, 900, 160])];
        let fenced = "Here you go:\n```json\n{\"inks\":[{\"id\":\"n1\",\"kind\":\"note\",\"speaker\":\"阿林\",\"weight\":\"line\",\"anchor\":{\"id\":\"p-1\",\"page\":2,\"type\":\"paragraph\",\"bbox\":[40.2,80.7,900.0,160.4]},\"body\":\"The manifold is the thing to hold.\"}]}\n```";
        let raw = decode_inks_envelope(fenced).expect("fenced json");
        let outcome = validate_guide_inks(&raw, &catalog);
        assert_eq!(outcome.dropped, 0, "{:?}", outcome.warnings);
        assert_eq!(outcome.inks.len(), 1);
        match &outcome.inks[0] {
            GuideInk::Note {
                speaker_id,
                anchor,
                body,
                ..
            } => {
                assert_eq!(speaker_id, "alin");
                assert_eq!(anchor.block_id, "p-1");
                assert_eq!(anchor.page_number, 2);
                assert_eq!(anchor.bbox, [40, 80, 900, 160]);
                assert!(body.contains("manifold"));
            }
            other => panic!("expected note, got {other:?}"),
        }
    }

    #[test]
    fn accepts_evidence_ids_capitalized_kind_and_shorthand_ref() {
        let catalog = [block("uuid-1", 2, [40, 80, 900, 160])];
        let raw = json!([
            {
                "kind": "Note",
                "speakerId": "alin",
                "text": "Copy the catalog row.",
                "evidenceIds": ["uuid-1"]
            },
            {
                "type": "trace",
                "speaker": {"id": "老周"},
                "anchor": "p2-1"
            }
        ]);
        let outcome = validate_guide_inks(&raw, &catalog);
        assert_eq!(outcome.dropped, 0, "{:?}", outcome.warnings);
        assert_eq!(outcome.inks.len(), 2);
    }

    #[test]
    fn accepts_top_level_block_id_without_nested_anchor() {
        let catalog = [block("p-1", 1, [10, 10, 200, 80])];
        let raw = json!([{
            "id": "t1",
            "kind": "trace",
            "speakerId": "laozhou",
            "blockId": "p-1",
            "pageNumber": 1
        }]);
        let outcome = validate_guide_inks(&raw, &catalog);
        assert_eq!(outcome.inks.len(), 1);
        assert_eq!(outcome.dropped, 0);
    }

    #[test]
    fn reply_cluster_keeps_two_and_drops_the_rest() {
        let bbox = [10, 10, 200, 80];
        let raw = json!([
            {
                "id": "n1",
                "kind": "note",
                "speakerId": "alin",
                "weight": "line",
                "anchor": locator("p-1", 1, bbox),
                "body": "Primary."
            },
            {
                "id": "r1",
                "kind": "reply",
                "speakerId": "laozhou",
                "parentId": "n1",
                "body": "@阿林 no."
            },
            {
                "id": "r2",
                "kind": "reply",
                "speakerId": "xiaxia",
                "parentId": "n1",
                "body": "@老周 maybe."
            },
            {
                "id": "r3",
                "kind": "reply",
                "speakerId": "alin",
                "parentId": "n1",
                "body": "@小夏 extra."
            }
        ]);
        let outcome = validate_guide_inks(&raw, &[block("p-1", 1, bbox)]);
        assert_eq!(outcome.inks.len(), 3);
        assert_eq!(outcome.dropped, 1);
    }
}
