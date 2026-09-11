use crate::outline_catalog::OutlineCatalog;
use crate::outline_protocol::parse_json_object;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

pub const MAP_PROTOCOL: &str = "outline-map-v4";
pub const DRAFT_PROTOCOL: &str = "outline-draft-v4";
pub const REVIEW_PROTOCOL: &str = "outline-review-v4";
pub const DEEP_DIVE_PROTOCOL_V4: &str = "outline-deep-dive-v4";

pub const DIRECTION_DIRECTED: &str = "directed";
pub const DIRECTION_UNDIRECTED: &str = "undirected";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlineJobProtocol {
    V3,
    V4,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutlineReference {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_number: Option<i64>,
    pub purpose: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutlineNodeV4 {
    pub node_id: String,
    pub title: String,
    pub takeaway: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role_label: Option<String>,
    pub references: Vec<OutlineReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncertainty: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutlineEdgeV4 {
    pub edge_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub direction: String,
    pub label: String,
    pub rationale: String,
    pub references: Vec<OutlineReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uncertainty: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutlineGroupV4 {
    pub group_id: String,
    pub title: String,
    pub node_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutlineGapV4 {
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_node_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related_edge_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_pages: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutlineGraphV4 {
    pub title: String,
    pub summary: String,
    pub nodes: Vec<OutlineNodeV4>,
    pub edges: Vec<OutlineEdgeV4>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<OutlineGroupV4>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gaps: Vec<OutlineGapV4>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutlineDraftEnvelope {
    pub graph: OutlineGraphV4,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutlineReviewEnvelope {
    pub graph: OutlineGraphV4,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub review_notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineIssueV4 {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutlineMapOutcome {
    Valid {
        graph: OutlineGraphV4,
        review_notes: Vec<String>,
    },
    Invalid {
        issues: Vec<OutlineIssueV4>,
        graph: Option<OutlineGraphV4>,
        review_notes: Vec<String>,
    },
}

pub fn job_protocol(payload: &Value) -> Result<OutlineJobProtocol, String> {
    if payload
        .get("mapProtocol")
        .is_some_and(|value| !value.is_string())
    {
        return Err("地图协议字段无效，请重新计划。".into());
    }
    match payload.get("mapProtocol").and_then(Value::as_str) {
        Some(MAP_PROTOCOL)
        | Some(DRAFT_PROTOCOL)
        | Some(REVIEW_PROTOCOL)
        | Some(DEEP_DIVE_PROTOCOL_V4) => Ok(OutlineJobProtocol::V4),
        Some(other) => Err(format!(
            "不支持的地图协议 {other}。请重新计划后再生成，不能按最新默认猜测。"
        )),
        None => {
            if payload
                .get("prompts")
                .and_then(|prompts| prompts.get("extract"))
                .and_then(Value::as_str)
                .is_some()
                && payload
                    .get("prompts")
                    .and_then(|prompts| prompts.get("compose"))
                    .and_then(Value::as_str)
                    .is_some()
            {
                Ok(OutlineJobProtocol::V3)
            } else if payload
                .get("prompts")
                .and_then(|prompts| prompts.get("deepDive"))
                .and_then(Value::as_str)
                .is_some()
                && payload.get("nodeId").and_then(Value::as_str).is_some()
            {
                Ok(OutlineJobProtocol::V3)
            } else {
                Err("无法识别的地图任务。缺少 mapProtocol，且不符合已知的旧抽取任务形状。请重新计划。".to_string())
            }
        }
    }
}

pub fn is_map_v4_protocol(value: &str) -> bool {
    value == MAP_PROTOCOL
        || value == DRAFT_PROTOCOL
        || value == REVIEW_PROTOCOL
        || value == DEEP_DIVE_PROTOCOL_V4
}

fn reference_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["purpose"],
        "properties": {
            "blockId": {"type": ["string", "null"]},
            "pageNumber": {"type": ["integer", "null"]},
            "purpose": {"type": "string", "minLength": 1}
        }
    })
}

fn node_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["nodeId", "title", "takeaway", "references"],
        "properties": {
            "nodeId": {"type": "string", "minLength": 1},
            "title": {"type": "string", "minLength": 1},
            "takeaway": {"type": "string", "minLength": 1},
            "roleLabel": {"type": ["string", "null"]},
            "references": {
                "type": "array",
                "minItems": 1,
                "items": reference_schema()
            },
            "uncertainty": {"type": ["string", "null"]}
        }
    })
}

fn edge_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["edgeId", "sourceNodeId", "targetNodeId", "direction", "label", "rationale", "references"],
        "properties": {
            "edgeId": {"type": "string", "minLength": 1},
            "sourceNodeId": {"type": "string", "minLength": 1},
            "targetNodeId": {"type": "string", "minLength": 1},
            "direction": {"type": "string", "enum": ["directed", "undirected"]},
            "label": {"type": "string", "minLength": 1},
            "rationale": {"type": "string", "minLength": 1},
            "references": {
                "type": "array",
                "minItems": 1,
                "items": reference_schema()
            },
            "uncertainty": {"type": ["string", "null"]}
        }
    })
}

fn graph_schema_properties() -> Value {
    json!({
        "title": {"type": "string", "minLength": 1},
        "summary": {"type": "string", "minLength": 1},
        "nodes": {
            "type": "array",
            "minItems": 1,
            "items": node_schema()
        },
        "edges": {
            "type": "array",
            "items": edge_schema()
        },
        "groups": {
            "type": "array",
            "items": {
                "type": "object",
                "additionalProperties": false,
                "required": ["groupId", "title", "nodeIds"],
                "properties": {
                    "groupId": {"type": "string", "minLength": 1},
                    "title": {"type": "string", "minLength": 1},
                    "nodeIds": {"type": "array", "items": {"type": "string", "minLength": 1}},
                    "description": {"type": ["string", "null"]}
                }
            }
        },
        "gaps": {
            "type": "array",
            "items": {
                "type": "object",
                "additionalProperties": false,
                "required": ["description"],
                "properties": {
                    "description": {"type": "string", "minLength": 1},
                    "relatedNodeIds": {"type": "array", "items": {"type": "string"}},
                    "relatedEdgeIds": {"type": "array", "items": {"type": "string"}},
                    "suggestedPages": {"type": "array", "items": {"type": "integer"}}
                }
            }
        }
    })
}

pub fn draft_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["graph"],
        "properties": {
            "graph": {
                "type": "object",
                "additionalProperties": false,
                "required": ["title", "summary", "nodes", "edges"],
                "properties": graph_schema_properties()
            }
        }
    })
}

pub fn review_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["graph"],
        "properties": {
            "graph": {
                "type": "object",
                "additionalProperties": false,
                "required": ["title", "summary", "nodes", "edges"],
                "properties": graph_schema_properties()
            },
            "reviewNotes": {
                "type": "array",
                "items": {"type": "string"}
            }
        }
    })
}

pub fn deep_dive_schema() -> Value {
    draft_schema()
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn clean_optional_id(value: Option<String>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_reference_shape(reference: &mut OutlineReference) {
    reference.block_id = clean_optional_id(reference.block_id.take());
    if let Some(block_id) = reference.block_id.as_mut() {
        if let Some(stripped) = block_id.strip_prefix("block:") {
            *block_id = stripped.to_string();
        }
    }
    reference.purpose = reference.purpose.trim().to_string();
}

fn normalize_graph_shape(graph: &mut OutlineGraphV4) {
    graph.title = graph.title.trim().to_string();
    graph.summary = graph.summary.trim().to_string();
    for node in &mut graph.nodes {
        node.node_id = node.node_id.trim().to_string();
        node.title = node.title.trim().to_string();
        node.takeaway = node.takeaway.trim().to_string();
        node.role_label = blank_to_none(node.role_label.take());
        node.uncertainty = blank_to_none(node.uncertainty.take());
        for reference in &mut node.references {
            normalize_reference_shape(reference);
        }
    }
    for edge in &mut graph.edges {
        edge.edge_id = edge.edge_id.trim().to_string();
        edge.source_node_id = edge.source_node_id.trim().to_string();
        edge.target_node_id = edge.target_node_id.trim().to_string();
        edge.direction = edge.direction.trim().to_ascii_lowercase();
        edge.label = edge.label.trim().to_string();
        edge.rationale = edge.rationale.trim().to_string();
        edge.uncertainty = blank_to_none(edge.uncertainty.take());
        for reference in &mut edge.references {
            normalize_reference_shape(reference);
        }
    }
    for group in &mut graph.groups {
        group.group_id = group.group_id.trim().to_string();
        group.title = group.title.trim().to_string();
        group.description = blank_to_none(group.description.take());
        group.node_ids = group
            .node_ids
            .iter()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect();
    }
    for gap in &mut graph.gaps {
        gap.description = gap.description.trim().to_string();
    }
}

/// Parse a model draft/deep-dive envelope. Trims whitespace and `block:` prefixes
/// only; does not rewrite node/edge IDs or page numbers.
pub fn parse_draft(text: &str) -> Result<OutlineDraftEnvelope, String> {
    let value = parse_json_object(text)?;
    let mut payload: OutlineDraftEnvelope = serde_json::from_value(value)
        .map_err(|error| format!("地图构图结果不符合 schema：{error}"))?;
    normalize_graph_shape(&mut payload.graph);
    Ok(payload)
}

pub fn parse_review(text: &str) -> Result<OutlineReviewEnvelope, String> {
    let value = parse_json_object(text)?;
    let mut payload: OutlineReviewEnvelope = serde_json::from_value(value)
        .map_err(|error| format!("地图审稿结果不符合 schema：{error}"))?;
    normalize_graph_shape(&mut payload.graph);
    payload.review_notes = payload
        .review_notes
        .into_iter()
        .map(|note| note.trim().to_string())
        .filter(|note| !note.is_empty())
        .collect();
    Ok(payload)
}

/// Load a persisted v4 graph without a second normalization pass that could
/// drift IDs. Unexpected fields are rejected like other schema corruption.
pub fn load_graph(value: &Value) -> Result<OutlineGraphV4, String> {
    serde_json::from_value(value.clone())
        .map_err(|error| format!("已保存的地图无法按 v4 读取：{error}"))
}

pub fn load_graph_from_str(text: &str) -> Result<OutlineGraphV4, String> {
    let value: Value =
        serde_json::from_str(text).map_err(|error| format!("已保存的地图 JSON 无效：{error}"))?;
    load_graph(&value)
}

pub fn catalog_page_index(catalog: &OutlineCatalog) -> HashMap<String, i64> {
    catalog
        .entries
        .iter()
        .map(|entry| (entry.id.clone(), entry.page))
        .collect()
}

pub fn priority_pages_from_graph(
    graph: &OutlineGraphV4,
    node_id: &str,
    catalog: &OutlineCatalog,
) -> HashSet<i64> {
    let mut related = HashSet::from([node_id.to_string()]);
    for edge in &graph.edges {
        if edge.source_node_id == node_id {
            related.insert(edge.target_node_id.clone());
        }
        if edge.target_node_id == node_id {
            related.insert(edge.source_node_id.clone());
        }
    }
    let pages_by_block = catalog_page_index(catalog);
    let mut pages = HashSet::new();
    let mut collect = |references: &[OutlineReference]| {
        for reference in references {
            if let Some(block_id) = &reference.block_id {
                if let Some(page) = pages_by_block.get(block_id) {
                    pages.insert(*page);
                }
            }
            if let Some(page) = reference.page_number {
                pages.insert(page);
            }
        }
    };
    for node in &graph.nodes {
        if related.contains(&node.node_id) {
            collect(&node.references);
        }
    }
    for edge in &graph.edges {
        if related.contains(&edge.source_node_id) || related.contains(&edge.target_node_id) {
            collect(&edge.references);
        }
    }
    pages
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outline_catalog::{build_outline_catalog, CatalogSourceBlock};

    fn sample_graph_json() -> &'static str {
        r#"{
          "graph": {
            "title": "自适应采样的收益与成立条件",
            "summary": "呈现采样设计、误差观察与估计条件之间的联系。",
            "nodes": [
              {
                "nodeId": "n1",
                "title": "根据局部变化调整采样频率",
                "takeaway": "方法根据变化估计分配采样预算。",
                "roleLabel": "设计选择",
                "references": [{"blockId": "b_method", "pageNumber": 3, "purpose": "方法定义"}]
              },
              {
                "nodeId": "n2",
                "title": "收益依赖可靠的变化估计",
                "takeaway": "估计噪声较大时收益减弱。",
                "references": [{"blockId": "b_noise", "pageNumber": 7, "purpose": "噪声实验"}]
              }
            ],
            "edges": [
              {
                "edgeId": "e1",
                "sourceNodeId": "n2",
                "targetNodeId": "n1",
                "direction": "directed",
                "label": "限制其有效使用条件",
                "rationale": "噪声实验限定了自适应设计可稳定获益的条件。",
                "references": [
                  {"blockId": "b_method", "pageNumber": 3, "purpose": "被限定的设计"},
                  {"blockId": "b_noise", "pageNumber": 7, "purpose": "限制依据"}
                ]
              }
            ],
            "groups": [],
            "gaps": []
          }
        }"#
    }

    #[test]
    fn parse_keeps_free_relation_and_optional_role() {
        let parsed = parse_draft(sample_graph_json()).expect("parse");
        assert_eq!(parsed.graph.edges[0].label, "限制其有效使用条件");
        assert_eq!(
            parsed.graph.nodes[0].role_label.as_deref(),
            Some("设计选择")
        );
        assert_eq!(parsed.graph.nodes[1].role_label, None);
        let loaded = load_graph(&serde_json::to_value(&parsed.graph).unwrap()).expect("load");
        assert_eq!(loaded.nodes[0].node_id, "n1");
        assert_eq!(loaded.edges[0].edge_id, "e1");
    }

    #[test]
    fn model_contract_rejects_unknown_fields_but_keeps_optional_groups() {
        let mut value: Value = serde_json::from_str(sample_graph_json()).unwrap();
        value["graph"].as_object_mut().unwrap().remove("groups");
        value["graph"].as_object_mut().unwrap().remove("gaps");
        assert!(parse_draft(&value.to_string()).is_ok());
        value["graph"]["nodes"][0]["bbox"] = json!([1, 2, 3, 4]);
        assert!(parse_draft(&value.to_string()).is_err());
        assert!(
            job_protocol(&json!({"mapProtocol":7,"prompts":{"extract":"x","compose":"y"}}))
                .is_err()
        );
        assert!(!is_map_v4_protocol("outline-map-v40"));
    }

    #[test]
    fn parse_does_not_rewrite_page_numbers() {
        let text = r#"{"graph":{"title":"t","summary":"s","nodes":[{"nodeId":"n1","title":"A","takeaway":"B","references":[{"blockId":"b1","pageNumber":99,"purpose":"出处"}]}],"edges":[],"groups":[],"gaps":[]}}"#;
        let parsed = parse_draft(text).expect("parse");
        assert_eq!(parsed.graph.nodes[0].references[0].page_number, Some(99));
        assert_eq!(
            parsed.graph.nodes[0].references[0].block_id.as_deref(),
            Some("b1")
        );
    }

    #[test]
    fn unknown_future_protocol_is_rejected_before_guessing() {
        let err = job_protocol(&json!({"mapProtocol": "outline-map-v5"})).unwrap_err();
        assert!(err.contains("outline-map-v5"));
        assert!(err.contains("不能按最新默认猜测"));
    }

    #[test]
    fn missing_protocol_with_extract_shape_is_v3() {
        let protocol = job_protocol(&json!({
            "prompts": {"extract": "e", "compose": "c"}
        }))
        .expect("v3");
        assert_eq!(protocol, OutlineJobProtocol::V3);
    }

    #[test]
    fn missing_protocol_without_known_shape_is_an_error() {
        let err = job_protocol(&json!({"prompts": {"draft": "x"}})).unwrap_err();
        assert!(err.contains("无法识别"));
    }

    #[test]
    fn v4_payload_is_detected() {
        let protocol = job_protocol(&json!({"mapProtocol": MAP_PROTOCOL})).expect("v4");
        assert_eq!(protocol, OutlineJobProtocol::V4);
    }

    #[test]
    fn priority_pages_use_adjacent_references_not_narrative_tier() {
        let catalog = build_outline_catalog(&[
            CatalogSourceBlock {
                id: "b1".into(),
                page: 2,
                block_index: 0,
                block_type: "paragraph".into(),
                text_content: "method".into(),
                bbox: [0, 0, 10, 10],
            },
            CatalogSourceBlock {
                id: "b2".into(),
                page: 9,
                block_index: 0,
                block_type: "paragraph".into(),
                text_content: "proof".into(),
                bbox: [0, 0, 10, 10],
            },
        ]);
        let graph = OutlineGraphV4 {
            title: "t".into(),
            summary: "s".into(),
            nodes: vec![
                OutlineNodeV4 {
                    node_id: "n1".into(),
                    title: "A".into(),
                    takeaway: "a".into(),
                    role_label: None,
                    references: vec![OutlineReference {
                        block_id: Some("b1".into()),
                        page_number: Some(2),
                        purpose: "定义".into(),
                    }],
                    uncertainty: None,
                },
                OutlineNodeV4 {
                    node_id: "n2".into(),
                    title: "B".into(),
                    takeaway: "b".into(),
                    role_label: None,
                    references: vec![OutlineReference {
                        block_id: Some("b2".into()),
                        page_number: Some(9),
                        purpose: "附录证明".into(),
                    }],
                    uncertainty: None,
                },
            ],
            edges: vec![OutlineEdgeV4 {
                edge_id: "e1".into(),
                source_node_id: "n1".into(),
                target_node_id: "n2".into(),
                direction: DIRECTION_DIRECTED.into(),
                label: "其证明在附录".into(),
                rationale: "证明写在附录".into(),
                references: vec![OutlineReference {
                    block_id: Some("b2".into()),
                    page_number: Some(9),
                    purpose: "证明".into(),
                }],
                uncertainty: None,
            }],
            groups: vec![],
            gaps: vec![],
        };
        let pages = priority_pages_from_graph(&graph, "n1", &catalog);
        assert!(pages.contains(&2));
        assert!(pages.contains(&9));
    }
}
