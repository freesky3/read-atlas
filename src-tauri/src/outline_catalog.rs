use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

pub const BODY_EXCERPT_CHARS: usize = 80;
pub const TOKENS_PER_PAGE: i64 = 300;
pub const OUTPUT_RESERVE_TOKENS: i64 = 4_096;
const TOKENS_PER_ENTRY: i64 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogSourceBlock {
    pub id: String,
    pub page: i64,
    pub block_index: i64,
    pub block_type: String,
    pub text_content: String,
    pub bbox: [i64; 4],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineCatalogEntry {
    pub id: String,
    pub page: i64,
    #[serde(rename = "type")]
    pub block_type: String,
    pub block_index: i64,
    pub bbox: [i64; 4],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nearby_caption_block_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OutlineCatalog {
    pub entries: Vec<OutlineCatalogEntry>,
    pub digest: String,
    pub token_estimate: i64,
}

pub fn is_chrome_block(block_type: &str) -> bool {
    let normalized = block_type.to_ascii_lowercase();
    normalized.contains("header")
        || normalized.contains("footer")
        || normalized.contains("references")
}

pub fn is_formula_block(block_type: &str) -> bool {
    let normalized = block_type.to_ascii_lowercase();
    normalized.contains("formula") || normalized.contains("equation")
}

pub fn is_figure_block(block_type: &str) -> bool {
    let normalized = block_type.to_ascii_lowercase();
    normalized.contains("figure") || normalized.contains("image")
}

pub fn is_table_block(block_type: &str) -> bool {
    block_type.to_ascii_lowercase().contains("table")
}

pub fn is_caption_block(block_type: &str) -> bool {
    block_type.to_ascii_lowercase().contains("caption")
}

pub fn is_visual_object_block(block_type: &str) -> bool {
    is_figure_block(block_type) || is_table_block(block_type)
}

pub fn truncate_excerpt(text: &str, max_chars: usize) -> String {
    let trimmed = text.trim();
    let count = trimmed.chars().count();
    if count <= max_chars {
        return trimmed.to_string();
    }
    trimmed.chars().take(max_chars).collect()
}

pub fn outline_context_exceeds_window(
    page_count: i64,
    catalog_token_estimate: i64,
    input_token_limit: Option<i64>,
) -> bool {
    let Some(limit) = input_token_limit.filter(|value| *value > 0) else {
        return false;
    };
    let pdf_tokens = page_count.max(0).saturating_mul(TOKENS_PER_PAGE);
    pdf_tokens
        .saturating_add(catalog_token_estimate.max(0))
        .saturating_add(OUTPUT_RESERVE_TOKENS)
        > limit
}

pub fn build_outline_catalog(blocks: &[CatalogSourceBlock]) -> OutlineCatalog {
    let eligible = blocks
        .iter()
        .filter(|block| !is_chrome_block(&block.block_type))
        .cloned()
        .collect::<Vec<_>>();
    let captions = eligible
        .iter()
        .filter(|block| is_caption_block(&block.block_type))
        .cloned()
        .collect::<Vec<_>>();

    let mut entries = Vec::with_capacity(eligible.len());
    for block in &eligible {
        let nearby_caption_block_id = if is_visual_object_block(&block.block_type) {
            nearby_caption(block, &captions).map(|caption| caption.id.clone())
        } else {
            None
        };
        let excerpt = if is_visual_object_block(&block.block_type) {
            None
        } else if is_formula_block(&block.block_type) {
            let latex = block.text_content.trim();
            if latex.is_empty() {
                None
            } else {
                Some(latex.to_string())
            }
        } else {
            let excerpt = truncate_excerpt(&block.text_content, BODY_EXCERPT_CHARS);
            if excerpt.is_empty() {
                None
            } else {
                Some(excerpt)
            }
        };
        entries.push(OutlineCatalogEntry {
            id: block.id.clone(),
            page: block.page,
            block_type: block.block_type.to_ascii_lowercase(),
            block_index: block.block_index,
            bbox: block.bbox,
            excerpt,
            nearby_caption_block_id,
        });
    }

    entries.sort_by(|left, right| {
        left.page
            .cmp(&right.page)
            .then(left.block_index.cmp(&right.block_index))
            .then(left.id.cmp(&right.id))
    });

    let digest = catalog_digest(&entries);
    let token_estimate = estimate_catalog_tokens(&entries);
    OutlineCatalog {
        entries,
        digest,
        token_estimate,
    }
}

pub fn outline_local_pages(
    graph: &Value,
    node_id: &str,
    catalog: &OutlineCatalog,
) -> std::collections::HashSet<i64> {
    let nodes = graph
        .get("nodes")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let edges = graph
        .get("edges")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut neighbor_ids = std::collections::HashSet::from([node_id.to_string()]);
    for edge in &edges {
        if edge.get("tier").and_then(Value::as_str) != Some("narrative") {
            continue;
        }
        let source = edge
            .get("sourceNodeId")
            .and_then(Value::as_str)
            .unwrap_or("");
        let target = edge
            .get("targetNodeId")
            .and_then(Value::as_str)
            .unwrap_or("");
        if source == node_id {
            neighbor_ids.insert(target.to_string());
        }
        if target == node_id {
            neighbor_ids.insert(source.to_string());
        }
    }
    let mut pages = std::collections::HashSet::new();
    for node in nodes {
        let id = node.get("nodeId").and_then(Value::as_str).unwrap_or("");
        if !neighbor_ids.contains(id) {
            continue;
        }
        if let Some(ids) = node.get("evidenceIds").and_then(Value::as_array) {
            for evidence in ids {
                let Some(block_id) = evidence.as_str() else {
                    continue;
                };
                if let Some(entry) = catalog.entries.iter().find(|item| item.id == block_id) {
                    pages.insert(entry.page);
                }
            }
        }
    }
    pages
}

pub fn filter_catalog_to_pages(
    catalog: &OutlineCatalog,
    pages: &std::collections::HashSet<i64>,
) -> OutlineCatalog {
    let filtered = catalog
        .entries
        .iter()
        .filter(|entry| pages.contains(&entry.page))
        .cloned()
        .collect::<Vec<_>>();
    let digest = catalog_digest(&filtered);
    let token_estimate = estimate_catalog_tokens(&filtered);
    OutlineCatalog {
        entries: filtered,
        digest,
        token_estimate,
    }
}

fn nearby_caption<'a>(
    object: &CatalogSourceBlock,
    captions: &'a [CatalogSourceBlock],
) -> Option<&'a CatalogSourceBlock> {
    captions
        .iter()
        .filter(|caption| caption.page == object.page && caption.id != object.id)
        .filter(|caption| vertically_adjacent(object.bbox, caption.bbox))
        .filter(|caption| horizontal_overlap_ratio(object.bbox, caption.bbox) >= 0.3)
        .min_by_key(|caption| vertical_gap(object.bbox, caption.bbox))
}

fn vertically_adjacent(object: [i64; 4], caption: [i64; 4]) -> bool {
    const SLOP: i64 = 24;
    caption[1] + SLOP >= object[3] || object[1] + SLOP >= caption[3]
}

fn vertical_gap(object: [i64; 4], caption: [i64; 4]) -> i64 {
    if caption[1] >= object[3] {
        caption[1] - object[3]
    } else if object[1] >= caption[3] {
        object[1] - caption[3]
    } else {
        0
    }
}

fn horizontal_overlap_ratio(left: [i64; 4], right: [i64; 4]) -> f64 {
    let overlap = (left[2].min(right[2]) - left[0].max(right[0])).max(0) as f64;
    let width = (left[2] - left[0]).min(right[2] - right[0]).max(1) as f64;
    overlap / width
}

fn catalog_digest(entries: &[OutlineCatalogEntry]) -> String {
    let mut hasher = Sha256::new();
    for entry in entries {
        let excerpt = entry.excerpt.as_deref().unwrap_or("");
        let caption = entry.nearby_caption_block_id.as_deref().unwrap_or("");
        let line = format!(
            "{}\t{}\t{}\t{}\t{},{},{},{}\t{}\t{}\n",
            entry.id,
            entry.page,
            entry.block_type,
            entry.block_index,
            entry.bbox[0],
            entry.bbox[1],
            entry.bbox[2],
            entry.bbox[3],
            excerpt,
            caption
        );
        hasher.update(line.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn estimate_catalog_tokens(entries: &[OutlineCatalogEntry]) -> i64 {
    entries.iter().fold(0_i64, |total, entry| {
        let excerpt_chars = entry
            .excerpt
            .as_deref()
            .map(|text| text.chars().count() as i64)
            .unwrap_or(0);
        total
            .saturating_add(TOKENS_PER_ENTRY)
            .saturating_add((excerpt_chars + 3) / 4)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(
        id: &str,
        page: i64,
        index: i64,
        block_type: &str,
        text: &str,
        bbox: [i64; 4],
    ) -> CatalogSourceBlock {
        CatalogSourceBlock {
            id: id.to_string(),
            page,
            block_index: index,
            block_type: block_type.to_string(),
            text_content: text.to_string(),
            bbox,
        }
    }

    #[test]
    fn catalog_drops_chrome_and_truncates_body_excerpt() {
        let long_body = "α".repeat(90) + " should not appear";
        let catalog = build_outline_catalog(&[
            block("h1", 1, 0, "page_header", "IEEE Trans.", [10, 10, 900, 40]),
            block("f1", 1, 1, "footer", "Page 1", [10, 960, 900, 990]),
            block(
                "r1",
                12,
                0,
                "references",
                "[1] Prior work",
                [40, 80, 900, 200],
            ),
            block("p1", 1, 2, "paragraph", &long_body, [40, 80, 900, 200]),
        ]);

        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(catalog.entries[0].id, "p1");
        assert_eq!(
            catalog.entries[0].excerpt.as_deref(),
            Some(long_body.chars().take(80).collect::<String>().as_str())
        );
        assert!(catalog.entries[0].nearby_caption_block_id.is_none());
        assert_eq!(catalog.digest.len(), 64);
        assert!(catalog.token_estimate > 0);
    }

    #[test]
    fn formula_excerpt_keeps_full_latex() {
        let catalog = build_outline_catalog(&[block(
            "eq1",
            2,
            0,
            "Formula",
            "z_i = h_i(t) + \\epsilon",
            [120, 200, 480, 260],
        )]);
        assert_eq!(catalog.entries[0].block_type, "formula");
        assert_eq!(
            catalog.entries[0].excerpt.as_deref(),
            Some("z_i = h_i(t) + \\epsilon")
        );
    }

    #[test]
    fn figure_without_caption_is_locator_only() {
        let catalog = build_outline_catalog(&[block(
            "fig1",
            3,
            1,
            "figure",
            "untrusted OCR dump of pixels",
            [80, 120, 700, 520],
        )]);
        assert_eq!(catalog.entries.len(), 1);
        assert_eq!(catalog.entries[0].id, "fig1");
        assert!(catalog.entries[0].excerpt.is_none());
        assert!(catalog.entries[0].nearby_caption_block_id.is_none());
    }

    #[test]
    fn same_page_caption_is_attached_to_figure() {
        let catalog = build_outline_catalog(&[
            block("fig1", 4, 0, "figure", "", [100, 120, 800, 500]),
            block(
                "cap1",
                4,
                1,
                "caption",
                "Figure 2. Architecture of the RNN simulator.",
                [120, 510, 780, 560],
            ),
            block(
                "cap-other",
                5,
                0,
                "caption",
                "Figure 3. Unrelated.",
                [120, 510, 780, 560],
            ),
        ]);
        let figure = catalog
            .entries
            .iter()
            .find(|entry| entry.id == "fig1")
            .expect("figure");
        assert_eq!(figure.nearby_caption_block_id.as_deref(), Some("cap1"));
        assert!(figure.excerpt.is_none());
        let caption = catalog
            .entries
            .iter()
            .find(|entry| entry.id == "cap1")
            .expect("caption stays in catalog");
        assert!(caption.excerpt.as_deref().unwrap().starts_with("Figure 2"));
    }

    #[test]
    fn digest_is_stable_for_the_same_blocks() {
        let blocks = [
            block("p1", 1, 0, "paragraph", "Hello world", [10, 10, 200, 40]),
            block("eq1", 1, 1, "equation", "E = mc^2", [10, 50, 200, 90]),
        ];
        let first = build_outline_catalog(&blocks);
        let second = build_outline_catalog(&blocks);
        assert_eq!(first.digest, second.digest);
        assert_eq!(first.token_estimate, second.token_estimate);
    }

    #[test]
    fn unknown_context_limit_never_fails_the_plan() {
        assert!(!outline_context_exceeds_window(500, 2_000, None));
        assert!(!outline_context_exceeds_window(500, 2_000, Some(0)));
    }

    #[test]
    fn long_pdf_plus_catalog_exceeds_a_small_window() {
        assert!(outline_context_exceeds_window(40, 1_000, Some(8_000)));
        assert!(!outline_context_exceeds_window(10, 200, Some(1_048_576)));
    }

    #[test]
    fn local_pages_include_node_and_narrative_neighbors_without_slack() {
        let catalog = OutlineCatalog {
            entries: vec![
                OutlineCatalogEntry {
                    id: "a".into(),
                    page: 5,
                    block_type: "paragraph".into(),
                    block_index: 0,
                    bbox: [0, 0, 1, 1],
                    excerpt: None,
                    nearby_caption_block_id: None,
                },
                OutlineCatalogEntry {
                    id: "b".into(),
                    page: 8,
                    block_type: "paragraph".into(),
                    block_index: 0,
                    bbox: [0, 0, 1, 1],
                    excerpt: None,
                    nearby_caption_block_id: None,
                },
                OutlineCatalogEntry {
                    id: "c".into(),
                    page: 9,
                    block_type: "paragraph".into(),
                    block_index: 0,
                    bbox: [0, 0, 1, 1],
                    excerpt: None,
                    nearby_caption_block_id: None,
                },
            ],
            digest: "d".into(),
            token_estimate: 0,
        };
        let graph = serde_json::json!({
            "nodes": [
                {"nodeId": "n1", "evidenceIds": ["a"]},
                {"nodeId": "n2", "evidenceIds": ["b"]},
                {"nodeId": "n3", "evidenceIds": ["c"]}
            ],
            "edges": [
                {"tier": "narrative", "sourceNodeId": "n1", "targetNodeId": "n2"},
                {"tier": "cross_link", "sourceNodeId": "n1", "targetNodeId": "n3"}
            ]
        });
        let pages = outline_local_pages(&graph, "n1", &catalog);
        assert_eq!(pages, std::collections::HashSet::from([5, 8]));
    }
}
