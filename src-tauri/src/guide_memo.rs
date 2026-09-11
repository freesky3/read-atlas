use crate::guide_protocol::GUIDE_MEMO_PROTOCOL;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuideLocation {
    pub page_number: i64,
    pub block_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuideMemoSpan {
    pub span_id: String,
    pub heading: String,
    pub page_start: i64,
    pub page_end: i64,
    pub purpose_markdown: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuideMemoObservation {
    pub observation_id: String,
    pub location: GuideLocation,
    pub observation_markdown: String,
    pub basis: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuideMemoConnection {
    pub connection_id: String,
    pub from: GuideLocation,
    pub to: GuideLocation,
    pub relation_markdown: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuideMemoPageHint {
    pub page_number: i64,
    pub kind: String,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuideMemo {
    pub schema_version: String,
    pub document_focus: String,
    pub spans: Vec<GuideMemoSpan>,
    pub observations: Vec<GuideMemoObservation>,
    pub connections: Vec<GuideMemoConnection>,
    pub page_hints: Vec<GuideMemoPageHint>,
    pub limitations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GuideMemoCacheKey {
    pub revision_id: String,
    pub pdf_digest: String,
    pub ocr_revision_id: String,
    pub catalog_digest: String,
    pub document_kind: String,
    pub memo_protocol: String,
    pub context_prompt_digest: String,
    pub root_prompt_digest: String,
    pub reader_digest: String,
    pub route_id: String,
    pub model: String,
}

pub fn parse_guide_memo(text: &str) -> Result<GuideMemo, String> {
    let parsed = decode_json(text)?;
    let object = if parsed.get("documentFocus").is_some() || parsed.get("schemaVersion").is_some() {
        parsed
    } else if let Some(inner) = parsed.get("result").or(parsed.get("data")) {
        inner.clone()
    } else {
        parsed
    };
    extra_fields_rejected(
        &object,
        &[
            "schemaVersion",
            "documentFocus",
            "spans",
            "observations",
            "connections",
            "pageHints",
            "limitations",
        ],
    )?;
    let memo: GuideMemo =
        serde_json::from_value(object).map_err(|error| format!("读后备忘结构无效：{error}"))?;
    if memo.schema_version != GUIDE_MEMO_PROTOCOL {
        return Err(format!(
            "读后备忘协议应为 {GUIDE_MEMO_PROTOCOL}，实际为 {}",
            memo.schema_version
        ));
    }
    if memo.document_focus.trim().is_empty() {
        return Err("读后备忘缺少 documentFocus".to_string());
    }
    for observation in &memo.observations {
        if !matches!(
            observation.basis.as_str(),
            "explicit" | "inference" | "reader_reaction"
        ) {
            return Err(format!("观察 {} 的 basis 无效", observation.observation_id));
        }
        if observation.location.page_number < 1 {
            return Err(format!("观察 {} 缺少可靠页码", observation.observation_id));
        }
    }
    for connection in &memo.connections {
        if connection.from.page_number < 1 || connection.to.page_number < 1 {
            return Err(format!("连接 {} 两端都需要页码", connection.connection_id));
        }
    }
    Ok(memo)
}

pub fn validate_memo_locations(
    memo: &GuideMemo,
    page_count: i64,
    catalog: &[crate::guide_validate::GuideCatalogBlock],
) -> Result<(), String> {
    let check_page = |page: i64| {
        if page >= 1 && page <= page_count {
            Ok(())
        } else {
            Err(format!("备忘页码超出文档：{page}"))
        }
    };
    let check_location = |loc: &GuideLocation| -> Result<(), String> {
        check_page(loc.page_number)?;
        for id in &loc.block_ids {
            if !catalog
                .iter()
                .any(|b| &b.id == id && b.page_number == loc.page_number)
            {
                return Err(format!("备忘块与页码不匹配：{id}"));
            }
        }
        Ok(())
    };
    for span in &memo.spans {
        check_page(span.page_start)?;
        check_page(span.page_end)?;
        if span.page_start > span.page_end {
            return Err("备忘跨度页码逆序".into());
        }
    }
    for observation in &memo.observations {
        check_location(&observation.location)?;
    }
    for connection in &memo.connections {
        check_location(&connection.from)?;
        check_location(&connection.to)?;
    }
    for hint in &memo.page_hints {
        check_page(hint.page_number)?;
    }
    Ok(())
}

fn extra_fields_rejected(value: &Value, allowed: &[&str]) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return Err("读后备忘必须是对象".to_string());
    };
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("读后备忘含有未允许字段：{key}"));
        }
    }
    Ok(())
}

fn decode_json(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    let candidate = if let Some(start) = trimmed.find('{') {
        let end = trimmed
            .rfind('}')
            .ok_or_else(|| "读后备忘不是 JSON 对象".to_string())?;
        &trimmed[start..=end]
    } else {
        trimmed
    };
    serde_json::from_str(candidate).map_err(|error| format!("无法解析读后备忘：{error}"))
}

pub fn memo_cache_digest(key: &GuideMemoCacheKey) -> String {
    let payload = serde_json::to_vec(key).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(payload);
    format!("{:x}", hasher.finalize())
}

pub fn digest_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.replace("\r\n", "\n").as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn select_memo_for_batch(memo: &GuideMemo, page_start: i64, page_end: i64) -> Value {
    let in_range = |page: i64| page >= page_start && page <= page_end;
    let observations: Vec<_> = memo
        .observations
        .iter()
        .filter(|item| in_range(item.location.page_number))
        .cloned()
        .collect();
    let connections: Vec<_> = memo
        .connections
        .iter()
        .filter(|item| in_range(item.from.page_number) || in_range(item.to.page_number))
        .cloned()
        .collect();
    let extra_pages: Vec<i64> = connections
        .iter()
        .flat_map(|item| [item.from.page_number, item.to.page_number])
        .filter(|page| !in_range(*page))
        .collect();
    let mut extra_observations = memo
        .observations
        .iter()
        .filter(|item| extra_pages.contains(&item.location.page_number))
        .cloned()
        .collect::<Vec<_>>();
    let mut observations = observations;
    observations.append(&mut extra_observations);
    let spans: Vec<_> = memo
        .spans
        .iter()
        .filter(|span| span.page_end >= page_start && span.page_start <= page_end)
        .cloned()
        .collect();
    let page_hints: Vec<_> = memo
        .page_hints
        .iter()
        .filter(|hint| in_range(hint.page_number))
        .cloned()
        .collect();
    json!({
        "documentFocus": memo.document_focus,
        "spans": spans,
        "observations": observations,
        "connections": connections,
        "pageHints": page_hints,
        "limitations": memo.limitations,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_arrays_nested_extras_and_out_of_range_evidence() {
        assert!(parse_guide_memo(
            r#"{"schemaVersion":"reading-guide-memo-v1","documentFocus":"x"}"#
        )
        .is_err());
        let mut value = json!({"schemaVersion":"reading-guide-memo-v1","documentFocus":"x","spans":[],"observations":[],"connections":[],"pageHints":[{"pageNumber":20,"kind":"body","note":"x"}],"limitations":[]});
        let memo = parse_guide_memo(&value.to_string()).unwrap();
        assert!(validate_memo_locations(&memo, 2, &[]).is_err());
        value["pageHints"][0]["extra"] = json!(1);
        assert!(parse_guide_memo(&value.to_string()).is_err());
    }

    #[test]
    fn rejects_unknown_fields_and_keeps_cross_page_connections() {
        let err = parse_guide_memo(r#"{"schemaVersion":"reading-guide-memo-v1","documentFocus":"x","spans":[],"observations":[],"connections":[],"pageHints":[],"limitations":[],"extra":1}"#).unwrap_err();
        assert!(err.contains("未允许"));
        let memo = parse_guide_memo(
            r#"{
              "schemaVersion":"reading-guide-memo-v1",
              "documentFocus":"方法章",
              "spans":[{"spanId":"s1","heading":"方法","pageStart":2,"pageEnd":4,"purposeMarkdown":"设定"}],
              "observations":[
                {"observationId":"o1","location":{"pageNumber":2,"blockIds":["b1"]},"observationMarkdown":"固定样本量","basis":"explicit"},
                {"observationId":"o2","location":{"pageNumber":8,"blockIds":["b9"]},"observationMarkdown":"后文才用到","basis":"inference"}
              ],
              "connections":[
                {"connectionId":"c1","from":{"pageNumber":2,"blockIds":["b1"]},"to":{"pageNumber":8,"blockIds":["b9"]},"relationMarkdown":"后文用这个限制解释差距"}
              ],
              "pageHints":[],
              "limitations":[]
            }"#,
        )
        .unwrap();
        let selected = select_memo_for_batch(&memo, 1, 3);
        let observations = selected["observations"].as_array().unwrap();
        assert_eq!(observations.len(), 2);
        let connections = selected["connections"].as_array().unwrap();
        assert_eq!(connections.len(), 1);
    }
}
