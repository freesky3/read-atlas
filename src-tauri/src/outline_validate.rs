use crate::outline_catalog::OutlineCatalog;
use crate::outline_map::{
    catalog_page_index, OutlineGraphV4, OutlineIssueV4, OutlineMapOutcome, OutlineReference,
    DIRECTION_DIRECTED, DIRECTION_UNDIRECTED,
};
use crate::outline_protocol::{
    normalize_evidence_id, normalize_relation_class, OutlineComposePayload, OutlineGraph,
    OutlineUnit,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutlineIssue {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OutlineGraphOutcome {
    Valid {
        graph: OutlineGraph,
        warnings: Vec<String>,
    },
    Partial {
        units: Vec<OutlineUnit>,
        issues: Vec<OutlineIssue>,
    },
}

pub fn retain_known_ids(ids: &[String], allowed: &HashSet<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut kept = Vec::new();
    for id in ids {
        let normalized = normalize_evidence_id(id);
        if allowed.contains(&normalized) && seen.insert(normalized.clone()) {
            kept.push(normalized);
        }
    }
    kept
}

pub fn sanitize_units(units: Vec<OutlineUnit>, allowed: &HashSet<String>) -> Vec<OutlineUnit> {
    units
        .into_iter()
        .map(|mut unit| {
            unit.evidence_ids = retain_known_ids(&unit.evidence_ids, allowed);
            unit
        })
        .filter(|unit| !unit.unit_id.trim().is_empty() && !unit.evidence_ids.is_empty())
        .collect()
}

pub fn reject_empty_primary(count: usize, label: &str) -> Result<(), String> {
    if count == 0 {
        Err(format!(
            "Empty repaired {label} are not a successful Outline result"
        ))
    } else {
        Ok(())
    }
}

pub fn validate_extract(units: &[OutlineUnit]) -> Result<(), String> {
    reject_empty_primary(units.len(), "units")
}

pub fn finish_overview(
    units: Vec<OutlineUnit>,
    compose: OutlineComposePayload,
    allowed: &HashSet<String>,
) -> OutlineGraphOutcome {
    let mut issues = Vec::new();
    if units.is_empty() {
        issues.push(OutlineIssue {
            code: "empty_units".to_string(),
            message: "An Outline extract requires at least one evidence-backed unit.".to_string(),
        });
        return OutlineGraphOutcome::Partial { units, issues };
    }

    let unit_ids = units
        .iter()
        .map(|unit| unit.unit_id.clone())
        .collect::<HashSet<_>>();
    let mut node_ids = HashSet::new();
    let mut nodes = Vec::new();
    for mut node in compose.nodes {
        if !node_ids.insert(node.node_id.clone()) {
            issues.push(OutlineIssue {
                code: "duplicate_node_id".to_string(),
                message: "Argument node ids must be unique.".to_string(),
            });
            continue;
        }
        node.evidence_ids = retain_known_ids(&node.evidence_ids, allowed);
        node.source_unit_ids.retain(|id| unit_ids.contains(id));
        if node.evidence_ids.is_empty() {
            issues.push(OutlineIssue {
                code: "missing_node_evidence".to_string(),
                message: "Every argument node requires catalog evidence.".to_string(),
            });
            continue;
        }
        nodes.push(node);
    }

    if nodes.is_empty() {
        issues.push(OutlineIssue {
            code: "empty_graph".to_string(),
            message: "An argument graph requires at least one evidence-backed node.".to_string(),
        });
        return OutlineGraphOutcome::Partial { units, issues };
    }

    let live_nodes = nodes
        .iter()
        .map(|node| node.node_id.clone())
        .collect::<HashSet<_>>();
    let mut edge_ids = HashSet::new();
    let mut edges = Vec::new();
    for mut edge in compose.edges {
        edge.relation_class = normalize_relation_class(&edge.relation_class);
        if !edge_ids.insert(edge.edge_id.clone()) {
            issues.push(OutlineIssue {
                code: "duplicate_edge_id".to_string(),
                message: "Argument edge ids must be unique.".to_string(),
            });
            continue;
        }
        if !live_nodes.contains(&edge.source_node_id) || !live_nodes.contains(&edge.target_node_id)
        {
            issues.push(OutlineIssue {
                code: "unknown_edge_endpoint".to_string(),
                message: "Argument relations must reference existing nodes.".to_string(),
            });
            continue;
        }
        if edge.source_node_id == edge.target_node_id {
            issues.push(OutlineIssue {
                code: "self_edge".to_string(),
                message: "Argument relations cannot connect a node to itself.".to_string(),
            });
            continue;
        }
        edge.evidence_ids = retain_known_ids(&edge.evidence_ids, allowed);
        edges.push(edge);
    }

    let narrative = edges
        .iter()
        .filter(|edge| edge.tier == "narrative")
        .cloned()
        .collect::<Vec<_>>();
    if has_narrative_cycle(&live_nodes, &narrative) {
        issues.push(OutlineIssue {
            code: "narrative_cycle".to_string(),
            message: "Narrative relations must form an acyclic reading flow.".to_string(),
        });
    }
    if !is_weakly_connected(&live_nodes, &narrative) {
        issues.push(OutlineIssue {
            code: "narrative_disconnected".to_string(),
            message: "Narrative relations must form one weakly connected map.".to_string(),
        });
    }

    let blocking = issues.iter().any(|issue| {
        matches!(
            issue.code.as_str(),
            "narrative_cycle" | "narrative_disconnected" | "empty_graph"
        )
    });
    if blocking {
        return OutlineGraphOutcome::Partial { units, issues };
    }

    let mut warnings = coverage_warnings(&nodes, allowed);
    let other_count = nodes
        .iter()
        .filter(|node| node.role_class == "other")
        .count();
    if other_count > 0 {
        warnings.push(format!(
            "{other_count} 个节点未匹配到论证/教学角色，已归为 other"
        ));
    }
    OutlineGraphOutcome::Valid {
        graph: OutlineGraph {
            title: compose.title,
            summary: compose.summary,
            nodes,
            edges,
        },
        warnings,
    }
}

fn has_narrative_cycle(
    node_ids: &HashSet<String>,
    narrative: &[crate::outline_protocol::OutlineEdge],
) -> bool {
    let mut incoming: HashMap<String, i64> = node_ids.iter().cloned().map(|id| (id, 0)).collect();
    let mut next: HashMap<String, Vec<String>> = node_ids
        .iter()
        .cloned()
        .map(|id| (id, Vec::new()))
        .collect();
    for edge in narrative {
        *incoming.entry(edge.target_node_id.clone()).or_insert(0) += 1;
        next.entry(edge.source_node_id.clone())
            .or_default()
            .push(edge.target_node_id.clone());
    }
    let mut queue = incoming
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    let mut visited = 0;
    while let Some(current) = queue.pop() {
        visited += 1;
        for nxt in next.get(&current).cloned().unwrap_or_default() {
            if let Some(count) = incoming.get_mut(&nxt) {
                *count -= 1;
                if *count == 0 {
                    queue.push(nxt);
                }
            }
        }
    }
    visited != node_ids.len()
}

fn is_weakly_connected(
    node_ids: &HashSet<String>,
    narrative: &[crate::outline_protocol::OutlineEdge],
) -> bool {
    if node_ids.is_empty() {
        return false;
    }
    let mut adjacent: HashMap<String, Vec<String>> = node_ids
        .iter()
        .cloned()
        .map(|id| (id, Vec::new()))
        .collect();
    for edge in narrative {
        adjacent
            .entry(edge.source_node_id.clone())
            .or_default()
            .push(edge.target_node_id.clone());
        adjacent
            .entry(edge.target_node_id.clone())
            .or_default()
            .push(edge.source_node_id.clone());
    }
    let start = node_ids.iter().next().cloned().unwrap();
    let mut seen = HashSet::new();
    let mut stack = vec![start];
    while let Some(current) = stack.pop() {
        if !seen.insert(current.clone()) {
            continue;
        }
        stack.extend(adjacent.get(&current).cloned().unwrap_or_default());
    }
    seen.len() == node_ids.len()
}

fn coverage_warnings(
    nodes: &[crate::outline_protocol::OutlineNode],
    allowed: &HashSet<String>,
) -> Vec<String> {
    let cited = nodes
        .iter()
        .flat_map(|node| node.evidence_ids.iter().cloned())
        .collect::<HashSet<_>>();
    let missing = allowed.len().saturating_sub(cited.len());
    if missing == 0 {
        Vec::new()
    } else {
        vec![format!(
            "{missing} catalog blocks are not cited by any Overview node"
        )]
    }
}

pub fn validate_map_v4(
    graph: OutlineGraphV4,
    catalog: &OutlineCatalog,
    page_count: i64,
    review_notes: Vec<String>,
) -> OutlineMapOutcome {
    let mut issues = Vec::new();
    if graph.title.trim().is_empty() {
        issues.push(issue("empty_title", "地图标题不能为空。"));
    }
    if graph.summary.trim().is_empty() {
        issues.push(issue("empty_summary", "地图摘要不能为空。"));
    }

    let pages_by_block = catalog_page_index(catalog);
    let mut node_ids = HashSet::new();
    let mut live_nodes = Vec::new();
    for node in graph.nodes {
        if node.node_id.is_empty() {
            issues.push(issue("empty_node_id", "节点标识不能为空。"));
            continue;
        }
        if !node_ids.insert(node.node_id.clone()) {
            issues.push(issue(
                "duplicate_node_id",
                &format!("节点标识重复：{}", node.node_id),
            ));
            continue;
        }
        if node.title.is_empty() || node.takeaway.is_empty() {
            issues.push(issue(
                "empty_node_text",
                &format!("节点 {} 的标题和认识说明都不能为空。", node.node_id),
            ));
        }
        validate_references(
            &node.references,
            &pages_by_block,
            page_count,
            &format!("节点 {}", node.node_id),
            &mut issues,
        );
        live_nodes.push(node);
    }

    if live_nodes.is_empty() {
        issues.push(issue(
            "empty_graph",
            "地图至少需要一个带有效定位的节点，才能作为正式结果发布。",
        ));
        return OutlineMapOutcome::Invalid {
            issues,
            graph: None,
            review_notes,
        };
    }

    let live_ids: HashSet<String> = live_nodes.iter().map(|node| node.node_id.clone()).collect();
    let mut edge_ids = HashSet::new();
    let mut live_edges = Vec::new();
    for edge in graph.edges {
        if edge.edge_id.is_empty() {
            issues.push(issue("empty_edge_id", "关系标识不能为空。"));
            continue;
        }
        if !edge_ids.insert(edge.edge_id.clone()) {
            issues.push(issue(
                "duplicate_edge_id",
                &format!("关系标识重复：{}", edge.edge_id),
            ));
            continue;
        }
        if !live_ids.contains(&edge.source_node_id) || !live_ids.contains(&edge.target_node_id) {
            issues.push(issue(
                "unknown_edge_endpoint",
                &format!("关系 {} 引用了不存在的节点。", edge.edge_id),
            ));
            continue;
        }
        if edge.direction != DIRECTION_DIRECTED && edge.direction != DIRECTION_UNDIRECTED {
            issues.push(issue(
                "invalid_direction",
                &format!(
                    "关系 {} 的方向只能是 directed 或 undirected。",
                    edge.edge_id
                ),
            ));
        }
        if edge.label.is_empty() || edge.rationale.is_empty() {
            issues.push(issue(
                "empty_edge_text",
                &format!("关系 {} 的标签和理由都不能为空。", edge.edge_id),
            ));
        }
        validate_references(
            &edge.references,
            &pages_by_block,
            page_count,
            &format!("关系 {}", edge.edge_id),
            &mut issues,
        );
        live_edges.push(edge);
    }

    let mut live_groups = Vec::new();
    let mut group_ids = HashSet::new();
    for group in graph.groups {
        if group.group_id.is_empty() || !group_ids.insert(group.group_id.clone()) {
            issues.push(issue("invalid_group", "分组标识必须唯一且非空。"));
            continue;
        }
        if group.title.is_empty() {
            issues.push(issue(
                "empty_group_title",
                &format!("分组 {} 的标题不能为空。", group.group_id),
            ));
        }
        if group.node_ids.iter().any(|id| !live_ids.contains(id)) {
            issues.push(issue(
                "unknown_group_node",
                &format!("分组 {} 引用了不存在的节点。", group.group_id),
            ));
        }
        live_groups.push(group);
    }

    let mut live_gaps = Vec::new();
    for gap in graph.gaps {
        if gap.description.trim().is_empty() {
            issues.push(issue("empty_gap", "材料缺口必须说明缺少什么。"));
            continue;
        }
        if gap.related_node_ids.iter().any(|id| !live_ids.contains(id))
            || gap.related_edge_ids.iter().any(|id| !edge_ids.contains(id))
        {
            issues.push(issue(
                "unknown_gap_target",
                "材料缺口引用了不存在的节点或关系。",
            ));
        }
        if gap
            .suggested_pages
            .iter()
            .any(|page| *page < 1 || *page > page_count)
        {
            issues.push(issue(
                "gap_page_out_of_range",
                "材料缺口建议页超出本文范围。",
            ));
        }
        live_gaps.push(gap);
    }

    let cleaned = OutlineGraphV4 {
        title: graph.title,
        summary: graph.summary,
        nodes: live_nodes,
        edges: live_edges,
        groups: live_groups,
        gaps: live_gaps,
    };
    let blocking = !issues.is_empty();
    if blocking {
        OutlineMapOutcome::Invalid {
            issues,
            graph: Some(cleaned),
            review_notes,
        }
    } else {
        OutlineMapOutcome::Valid {
            graph: cleaned,
            review_notes,
        }
    }
}

fn issue(code: &str, message: &str) -> OutlineIssueV4 {
    OutlineIssueV4 {
        code: code.to_string(),
        message: message.to_string(),
    }
}

fn validate_references(
    references: &[OutlineReference],
    pages_by_block: &HashMap<String, i64>,
    page_count: i64,
    owner: &str,
    issues: &mut Vec<OutlineIssueV4>,
) {
    if references.is_empty() {
        issues.push(issue(
            "missing_locator",
            &format!("{owner} 至少需要一处可用的原文定位。"),
        ));
        return;
    }
    for reference in references {
        if reference.purpose.trim().is_empty() {
            issues.push(issue(
                "empty_purpose",
                &format!("{owner} 的引用必须说明这段原文在此提供什么。"),
            ));
        }
        match (
            reference.block_id.as_deref().filter(|id| !id.is_empty()),
            reference.page_number,
        ) {
            (None, None) => issues.push(issue(
                "missing_locator",
                &format!("{owner} 的引用缺少 blockId 或页码。"),
            )),
            (Some(block_id), page) => match pages_by_block.get(block_id) {
                None => issues.push(issue(
                    "unknown_block",
                    &format!("{owner} 引用了目录外的块 {block_id}。"),
                )),
                Some(actual_page) => {
                    if let Some(page) = page {
                        if page != *actual_page {
                            issues.push(issue(
                                "page_block_mismatch",
                                &format!(
                                    "{owner} 的块 {block_id} 属于第 {actual_page} 页，不能改写成第 {page} 页。"
                                ),
                            ));
                        }
                    }
                }
            },
            (None, Some(page)) => {
                if page < 1 || page > page_count {
                    issues.push(issue(
                        "page_out_of_range",
                        &format!("{owner} 的页码 {page} 不在 1–{page_count}。"),
                    ));
                }
            }
        }
    }
}

pub fn format_validation_issues(issues: &[OutlineIssueV4]) -> String {
    issues
        .iter()
        .map(|issue| format!("{}：{}", issue.code, issue.message))
        .collect::<Vec<_>>()
        .join("；")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library_paths::DocumentKind;
    use crate::outline_protocol::{
        normalize_relation_class, parse_compose, parse_extract, OutlineEdge, OutlineNode,
    };

    fn allowed() -> HashSet<String> {
        ["b1", "b2", "b3"].into_iter().map(str::to_string).collect()
    }

    fn unit(id: &str, evidence: &[&str]) -> OutlineUnit {
        OutlineUnit {
            unit_id: id.to_string(),
            role_class: "claim_hypothesis".to_string(),
            role_label: None,
            title: id.to_string(),
            takeaway: "takeaway".to_string(),
            importance: "core".to_string(),
            evidence_ids: evidence.iter().map(|item| item.to_string()).collect(),
            confidence: 0.8,
        }
    }

    fn node(id: &str, evidence: &[&str]) -> OutlineNode {
        OutlineNode {
            node_id: id.to_string(),
            role_class: "method_design".to_string(),
            role_label: None,
            title: id.to_string(),
            takeaway: "takeaway".to_string(),
            importance: "core".to_string(),
            source_unit_ids: vec![id.to_string()],
            evidence_ids: evidence.iter().map(|item| item.to_string()).collect(),
            confidence: 0.7,
        }
    }

    fn edge(id: &str, from: &str, to: &str, tier: &str) -> OutlineEdge {
        OutlineEdge {
            edge_id: id.to_string(),
            source_node_id: from.to_string(),
            target_node_id: to.to_string(),
            tier: tier.to_string(),
            relation_class: "dependency".to_string(),
            label: "then".to_string(),
            rationale: String::new(),
            evidence_ids: vec!["b1".to_string()],
        }
    }

    #[test]
    fn unknown_catalog_ids_are_dropped() {
        let units = sanitize_units(
            vec![
                unit("u1", &["b1", "block:forged", "b1"]),
                unit("u2", &["missing"]),
            ],
            &allowed(),
        );
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].evidence_ids, vec!["b1".to_string()]);
    }

    #[test]
    fn empty_repair_is_rejected() {
        assert!(reject_empty_primary(0, "units").is_err());
        assert!(reject_empty_primary(2, "nodes").is_ok());
    }

    #[test]
    fn node_role_filled_as_relation_class_maps_to_other() {
        assert_eq!(normalize_relation_class("mechanism_process"), "other");
        assert_eq!(normalize_relation_class("comparison"), "comparison");
    }

    #[test]
    fn narrative_cycle_publishes_partial_arguments() {
        let outcome = finish_overview(
            vec![unit("a", &["b1"]), unit("b", &["b2"])],
            OutlineComposePayload {
                title: "Map".to_string(),
                summary: "Cycle".to_string(),
                nodes: vec![node("a", &["b1"]), node("b", &["b2"])],
                edges: vec![
                    edge("e1", "a", "b", "narrative"),
                    edge("e2", "b", "a", "narrative"),
                ],
            },
            &allowed(),
        );
        match outcome {
            OutlineGraphOutcome::Partial { issues, .. } => {
                assert!(issues.iter().any(|issue| issue.code == "narrative_cycle"));
            }
            OutlineGraphOutcome::Valid { .. } => panic!("cycle must not publish a graph"),
        }
    }

    #[test]
    fn valid_dag_is_published_and_unknown_ids_do_not_block() {
        let outcome = finish_overview(
            vec![unit("a", &["b1"]), unit("b", &["b2"])],
            OutlineComposePayload {
                title: "RNN map".to_string(),
                summary: "A then B".to_string(),
                nodes: vec![node("a", &["b1", "ghost"]), node("b", &["block:b2"])],
                edges: vec![edge("e1", "a", "b", "narrative")],
            },
            &allowed(),
        );
        match outcome {
            OutlineGraphOutcome::Valid { graph, .. } => {
                assert_eq!(graph.nodes.len(), 2);
                assert_eq!(graph.edges.len(), 1);
                assert_eq!(graph.nodes[0].evidence_ids, vec!["b1".to_string()]);
            }
            OutlineGraphOutcome::Partial { issues, .. } => {
                panic!("expected valid graph, got {issues:?}")
            }
        }
    }

    #[test]
    fn extract_parser_keeps_latex_and_block_prefix() {
        let payload = parse_extract(
            r#"{"units":[{"unitId":"u1","roleClass":"method_design","title":"RNN","takeaway":"sim","evidenceIds":["block:b1"]}]}"#,
            DocumentKind::Paper,
        )
        .expect("parse");
        assert_eq!(payload.units[0].evidence_ids, vec!["b1".to_string()]);
        let compose = parse_compose(
            r#"{"title":"t","summary":"s","nodes":[{"nodeId":"n1","roleClass":"method_design","title":"RNN","takeaway":"sim","evidenceIds":["b1"]}],"edges":[{"edgeId":"e1","sourceNodeId":"n1","targetNodeId":"n1","tier":"narrative","relationClass":"mechanism_process"}]}"#,
            DocumentKind::Paper,
        )
        .expect("parse compose");
        assert_eq!(compose.edges[0].relation_class, "other");
    }

    fn v4_catalog() -> OutlineCatalog {
        crate::outline_catalog::build_outline_catalog(&[
            crate::outline_catalog::CatalogSourceBlock {
                id: "b1".into(),
                page: 1,
                block_index: 0,
                block_type: "paragraph".into(),
                text_content: "one".into(),
                bbox: [0, 0, 10, 10],
            },
            crate::outline_catalog::CatalogSourceBlock {
                id: "b2".into(),
                page: 2,
                block_index: 0,
                block_type: "paragraph".into(),
                text_content: "two".into(),
                bbox: [0, 0, 10, 10],
            },
        ])
    }

    fn v4_node(id: &str, block: &str, page: i64) -> crate::outline_map::OutlineNodeV4 {
        crate::outline_map::OutlineNodeV4 {
            node_id: id.into(),
            title: id.into(),
            takeaway: format!("{id} takeaway"),
            role_label: None,
            references: vec![crate::outline_map::OutlineReference {
                block_id: Some(block.into()),
                page_number: Some(page),
                purpose: "出处".into(),
            }],
            uncertainty: None,
        }
    }

    fn v4_edge(
        id: &str,
        from: &str,
        to: &str,
        direction: &str,
    ) -> crate::outline_map::OutlineEdgeV4 {
        crate::outline_map::OutlineEdgeV4 {
            edge_id: id.into(),
            source_node_id: from.into(),
            target_node_id: to.into(),
            direction: direction.into(),
            label: "揭示了两种定义的差别".into(),
            rationale: "两端原文对照后可以核对这一结构联系。".into(),
            references: vec![crate::outline_map::OutlineReference {
                block_id: Some("b1".into()),
                page_number: Some(1),
                purpose: "对照".into(),
            }],
            uncertainty: None,
        }
    }

    #[test]
    fn v4_never_silently_discards_invalid_relations_groups_or_gap_targets() {
        let graph = crate::outline_map::OutlineGraphV4 {
            title: "t".into(),
            summary: "s".into(),
            nodes: vec![v4_node("a", "b1", 1)],
            edges: vec![v4_edge("", "a", "a", "directed")],
            groups: vec![crate::outline_map::OutlineGroupV4 {
                group_id: "g".into(),
                title: "g".into(),
                node_ids: vec!["missing".into()],
                description: None,
            }],
            gaps: vec![crate::outline_map::OutlineGapV4 {
                description: "材料缺口".into(),
                related_node_ids: vec!["missing".into()],
                related_edge_ids: vec!["noedge".into()],
                suggested_pages: vec![99],
            }],
        };
        let OutlineMapOutcome::Invalid { issues, .. } =
            validate_map_v4(graph, &v4_catalog(), 2, vec![])
        else {
            panic!("invalid structure must not publish")
        };
        for code in [
            "empty_edge_id",
            "unknown_group_node",
            "unknown_gap_target",
            "gap_page_out_of_range",
        ] {
            assert!(
                issues.iter().any(|i| i.code == code),
                "missing issue: {code}"
            );
        }
    }

    #[test]
    fn v4_accepts_cycle_disconnected_self_loop_and_undirected() {
        let catalog = v4_catalog();
        let graph = crate::outline_map::OutlineGraphV4 {
            title: "开放结构".into(),
            summary: "允许回路与独立组。".into(),
            nodes: vec![
                v4_node("a", "b1", 1),
                v4_node("b", "b2", 2),
                v4_node("c", "b1", 1),
            ],
            edges: vec![
                v4_edge("e1", "a", "b", "directed"),
                v4_edge("e2", "b", "a", "directed"),
                v4_edge("e3", "a", "a", "directed"),
                crate::outline_map::OutlineEdgeV4 {
                    edge_id: "e4".into(),
                    source_node_id: "a".into(),
                    target_node_id: "b".into(),
                    direction: "undirected".into(),
                    label: "对照".into(),
                    rationale: "两者是并列定义，不是推出。".into(),
                    references: vec![crate::outline_map::OutlineReference {
                        block_id: Some("b2".into()),
                        page_number: Some(2),
                        purpose: "对照材料".into(),
                    }],
                    uncertainty: None,
                },
            ],
            groups: vec![],
            gaps: vec![],
        };
        match validate_map_v4(graph, &catalog, 8, Vec::new()) {
            OutlineMapOutcome::Valid { graph, .. } => {
                assert_eq!(graph.nodes.len(), 3);
                assert_eq!(graph.edges.len(), 4);
                assert!(graph.edges.iter().any(|edge| edge.edge_id == "e3"));
            }
            OutlineMapOutcome::Invalid { issues, .. } => {
                panic!("open structure must be valid, got {issues:?}")
            }
        }
    }

    #[test]
    fn v4_page_block_mismatch_is_an_error_not_rewritten() {
        let catalog = v4_catalog();
        let mut node = v4_node("a", "b1", 2);
        node.references[0].page_number = Some(2);
        let graph = crate::outline_map::OutlineGraphV4 {
            title: "t".into(),
            summary: "s".into(),
            nodes: vec![node],
            edges: vec![],
            groups: vec![],
            gaps: vec![],
        };
        match validate_map_v4(graph, &catalog, 8, Vec::new()) {
            OutlineMapOutcome::Invalid { issues, graph, .. } => {
                assert!(issues
                    .iter()
                    .any(|issue| issue.code == "page_block_mismatch"));
                assert_eq!(graph.unwrap().nodes[0].references[0].page_number, Some(2));
            }
            OutlineMapOutcome::Valid { .. } => panic!("mismatch must not be rewritten as valid"),
        }
    }

    #[test]
    fn v4_page_only_locator_is_allowed_inside_pdf() {
        let catalog = v4_catalog();
        let graph = crate::outline_map::OutlineGraphV4 {
            title: "t".into(),
            summary: "s".into(),
            nodes: vec![crate::outline_map::OutlineNodeV4 {
                node_id: "solo".into(),
                title: "单节点".into(),
                takeaway: "没有边也合法。".into(),
                role_label: Some("识别策略".into()),
                references: vec![crate::outline_map::OutlineReference {
                    block_id: None,
                    page_number: Some(3),
                    purpose: "OCR 无块时的页级定位".into(),
                }],
                uncertainty: None,
            }],
            edges: vec![],
            groups: vec![],
            gaps: vec![],
        };
        match validate_map_v4(graph, &catalog, 8, Vec::new()) {
            OutlineMapOutcome::Valid { graph, .. } => {
                assert_eq!(graph.nodes.len(), 1);
                assert!(graph.edges.is_empty());
            }
            OutlineMapOutcome::Invalid { issues, .. } => panic!("{issues:?}"),
        }
    }
}
