use crate::library_paths::DocumentKind;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const EXTRACT_PROTOCOL: &str = "outline-extract-v3";
pub const COMPOSE_PROTOCOL: &str = "outline-compose-v3";
pub const DEEP_DIVE_PROTOCOL: &str = "outline-deep-dive-v3";

const NODE_ROLES: &[&str] = &[
    "question_context",
    "claim_hypothesis",
    "concept_theory",
    "method_design",
    "mechanism_process",
    "evidence_evaluation",
    "result_finding",
    "conclusion_implication",
    "limitation_boundary",
    "other",
];

const TEXTBOOK_NODE_ROLES: &[&str] = &[
    "concept_intro",
    "definition",
    "example_illustration",
    "derivation",
    "algorithm_procedure",
    "exercise_practice",
    "caution_pitfall",
    "application_example",
    "summary_recap",
    "other",
];

const RELATION_CLASSES: &[&str] = &[
    "context",
    "structure",
    "dependency",
    "transformation",
    "verification",
    "comparison",
    "qualification",
    "sequence",
    "application",
    "reference",
    "other",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineUnit {
    pub unit_id: String,
    pub role_class: String,
    #[serde(default)]
    pub role_label: Option<String>,
    pub title: String,
    pub takeaway: String,
    #[serde(default)]
    pub importance: String,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineNode {
    pub node_id: String,
    pub role_class: String,
    #[serde(default)]
    pub role_label: Option<String>,
    pub title: String,
    pub takeaway: String,
    #[serde(default)]
    pub importance: String,
    #[serde(default)]
    pub source_unit_ids: Vec<String>,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
    #[serde(default)]
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineEdge {
    pub edge_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub tier: String,
    pub relation_class: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineGraph {
    pub title: String,
    pub summary: String,
    pub nodes: Vec<OutlineNode>,
    pub edges: Vec<OutlineEdge>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineExtractPayload {
    pub units: Vec<OutlineUnit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineComposePayload {
    pub title: String,
    pub summary: String,
    pub nodes: Vec<OutlineNode>,
    pub edges: Vec<OutlineEdge>,
}

pub fn normalize_role_class_for_kind(value: &str, kind: DocumentKind) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    let allowed = match kind {
        DocumentKind::Textbook => TEXTBOOK_NODE_ROLES,
        DocumentKind::Paper => NODE_ROLES,
    };
    if allowed.contains(&normalized.as_str()) {
        normalized
    } else {
        "other".to_string()
    }
}

pub fn normalize_relation_class(value: &str) -> String {
    let normalized = value.trim().to_ascii_lowercase();
    if RELATION_CLASSES.contains(&normalized.as_str()) {
        return normalized;
    }
    if NODE_ROLES.contains(&normalized.as_str()) {
        return "other".to_string();
    }
    "other".to_string()
}

pub fn normalize_evidence_id(value: &str) -> String {
    value
        .trim()
        .strip_prefix("block:")
        .unwrap_or(value.trim())
        .to_string()
}

pub fn extract_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["units"],
        "properties": {
            "units": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["unitId", "roleClass", "title", "takeaway", "evidenceIds"],
                    "properties": {
                        "unitId": {"type": "string", "minLength": 1},
                        "roleClass": {"type": "string"},
                        "roleLabel": {"type": "string"},
                        "title": {"type": "string", "minLength": 1},
                        "takeaway": {"type": "string", "minLength": 1},
                        "importance": {"type": "string"},
                        "evidenceIds": {
                            "type": "array",
                            "items": {"type": "string", "minLength": 1}
                        },
                        "confidence": {"type": "number"}
                    }
                }
            }
        }
    })
}

pub fn compose_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["title", "summary", "nodes", "edges"],
        "properties": {
            "title": {"type": "string"},
            "summary": {"type": "string"},
            "nodes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["nodeId", "roleClass", "title", "takeaway", "evidenceIds"],
                    "properties": {
                        "nodeId": {"type": "string", "minLength": 1},
                        "roleClass": {"type": "string"},
                        "roleLabel": {"type": "string"},
                        "title": {"type": "string"},
                        "takeaway": {"type": "string"},
                        "importance": {"type": "string"},
                        "sourceUnitIds": {"type": "array", "items": {"type": "string"}},
                        "evidenceIds": {"type": "array", "items": {"type": "string"}},
                        "confidence": {"type": "number"}
                    }
                }
            },
            "edges": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["edgeId", "sourceNodeId", "targetNodeId", "tier", "relationClass"],
                    "properties": {
                        "edgeId": {"type": "string"},
                        "sourceNodeId": {"type": "string"},
                        "targetNodeId": {"type": "string"},
                        "tier": {"type": "string"},
                        "relationClass": {"type": "string"},
                        "label": {"type": "string"},
                        "rationale": {"type": "string"},
                        "evidenceIds": {"type": "array", "items": {"type": "string"}}
                    }
                }
            }
        }
    })
}

pub fn parse_json_object(text: &str) -> Result<Value, String> {
    let start = text
        .find('{')
        .ok_or_else(|| "Outline response is not a JSON object".to_string())?;
    let end = text
        .rfind('}')
        .ok_or_else(|| "Outline response is not a JSON object".to_string())?;
    serde_json::from_str(&text[start..=end])
        .map_err(|error| format!("Outline response is not valid JSON: {error}"))
}

pub fn parse_extract(text: &str, kind: DocumentKind) -> Result<OutlineExtractPayload, String> {
    let value = parse_json_object(text)?;
    let mut payload: OutlineExtractPayload = serde_json::from_value(value)
        .map_err(|error| format!("Outline extract failed schema validation: {error}"))?;
    for unit in &mut payload.units {
        unit.role_class = normalize_role_class_for_kind(&unit.role_class, kind);
        if unit.importance.trim().is_empty() {
            unit.importance = "supporting".to_string();
        }
        unit.evidence_ids = unit
            .evidence_ids
            .iter()
            .map(|id| normalize_evidence_id(id))
            .filter(|id| !id.is_empty())
            .collect();
    }
    Ok(payload)
}

pub fn parse_compose(text: &str, kind: DocumentKind) -> Result<OutlineComposePayload, String> {
    let value = parse_json_object(text)?;
    let mut payload: OutlineComposePayload = serde_json::from_value(value)
        .map_err(|error| format!("Outline compose failed schema validation: {error}"))?;
    for node in &mut payload.nodes {
        node.role_class = normalize_role_class_for_kind(&node.role_class, kind);
        if node.importance.trim().is_empty() {
            node.importance = "supporting".to_string();
        }
        node.evidence_ids = node
            .evidence_ids
            .iter()
            .map(|id| normalize_evidence_id(id))
            .filter(|id| !id.is_empty())
            .collect();
    }
    for edge in &mut payload.edges {
        edge.relation_class = normalize_relation_class(&edge.relation_class);
        if edge.tier != "narrative" && edge.tier != "cross_link" {
            edge.tier = "cross_link".to_string();
        }
        edge.evidence_ids = edge
            .evidence_ids
            .iter()
            .map(|id| normalize_evidence_id(id))
            .filter(|id| !id.is_empty())
            .collect();
    }
    Ok(payload)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn role_normalization_forks_by_document_kind() {
        assert_eq!(
            normalize_role_class_for_kind("claim_hypothesis", DocumentKind::Paper),
            "claim_hypothesis"
        );
        assert_eq!(
            normalize_role_class_for_kind("claim_hypothesis", DocumentKind::Textbook),
            "other"
        );
        assert_eq!(
            normalize_role_class_for_kind("concept_intro", DocumentKind::Textbook),
            "concept_intro"
        );
        assert_eq!(
            normalize_role_class_for_kind("concept_intro", DocumentKind::Paper),
            "other"
        );
        assert_eq!(
            normalize_role_class_for_kind("method_design", DocumentKind::Paper),
            "method_design"
        );
        assert_eq!(
            normalize_role_class_for_kind("unknown_role", DocumentKind::Textbook),
            "other"
        );
    }

    #[test]
    fn textbook_parse_keeps_teaching_roles_and_maps_paper_roles_to_other() {
        let payload = parse_extract(
            r#"{"units":[{"unitId":"u1","roleClass":"concept_intro","title":"梯度","takeaway":"引入梯度概念","evidenceIds":["b1"]},{"unitId":"u2","roleClass":"claim_hypothesis","title":"旧角色","takeaway":"t","evidenceIds":["b2"]}]}"#,
            DocumentKind::Textbook,
        )
        .expect("parse");
        assert_eq!(payload.units[0].role_class, "concept_intro");
        assert_eq!(payload.units[1].role_class, "other");

        let paper = parse_extract(
            r#"{"units":[{"unitId":"u1","roleClass":"method_design","title":"RNN","takeaway":"sim","evidenceIds":["b1"]}]}"#,
            DocumentKind::Paper,
        )
        .expect("parse paper");
        assert_eq!(paper.units[0].role_class, "method_design");
    }
}
