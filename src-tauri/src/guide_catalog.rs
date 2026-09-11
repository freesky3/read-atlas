use crate::guide_protocol::{
    GUIDE_BATCH_HARD_MAX_PAGES, GUIDE_BATCH_TARGET_PAGES, GUIDE_V2_BATCH_CHAR_BUDGET,
};
use crate::guide_validate::GuideCatalogBlock;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideMaterialBlock {
    pub id: String,
    pub page_number: i64,
    pub block_index: i64,
    pub block_type: String,
    pub bbox: [i64; 4],
    pub text: String,
    pub parent_block_id: String,
    pub fragment_index: i64,
    pub fragment_count: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideBatchPlan {
    pub ordinal: i64,
    pub page_start: i64,
    pub page_end: i64,
    pub anchor_block_ids: Vec<String>,
    pub context_block_ids: Vec<String>,
    pub materials: Vec<GuideMaterialBlock>,
}

pub fn materials_from_catalog(
    blocks: &[GuideCatalogBlock],
    char_budget: usize,
) -> Vec<GuideMaterialBlock> {
    let mut materials = Vec::new();
    for block in blocks {
        let text = block.excerpt.clone().unwrap_or_default();
        let fragments = split_text(&text, char_budget.max(800));
        let fragment_count = fragments.len() as i64;
        for (index, fragment) in fragments.into_iter().enumerate() {
            materials.push(GuideMaterialBlock {
                id: block.id.clone(),
                page_number: block.page_number,
                block_index: block.block_index,
                block_type: block.block_type.clone(),
                bbox: block.bbox,
                text: fragment,
                parent_block_id: block.id.clone(),
                fragment_index: index as i64,
                fragment_count,
            });
        }
    }
    materials
}

fn split_text(text: &str, max_chars: usize) -> Vec<String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return vec![String::new()];
    }
    if trimmed.chars().count() <= max_chars {
        return vec![trimmed.to_string()];
    }
    let chars: Vec<char> = trimmed.chars().collect();
    chars
        .chunks(max_chars)
        .map(|chunk| chunk.iter().collect())
        .collect()
}

pub fn plan_material_batches(
    materials: &[GuideMaterialBlock],
    char_budget: usize,
) -> Vec<GuideBatchPlan> {
    plan_material_batches_with_limits(
        materials,
        char_budget,
        GUIDE_BATCH_TARGET_PAGES,
        GUIDE_BATCH_HARD_MAX_PAGES,
    )
}

pub fn plan_material_batches_with_limits(
    materials: &[GuideMaterialBlock],
    char_budget: usize,
    target_pages: i64,
    hard_max_pages: i64,
) -> Vec<GuideBatchPlan> {
    if materials.is_empty() {
        return Vec::new();
    }
    let budget = char_budget.max(2_000);
    let mut batches = Vec::new();
    let mut current: Vec<GuideMaterialBlock> = Vec::new();
    let mut chars = 0usize;
    for material in materials {
        let item_chars = material_cost(material);
        let next_pages = if current.is_empty() {
            1
        } else {
            let start = current[0].page_number;
            (material.page_number - start).unsigned_abs() as i64 + 1
        };
        let would_exceed = !current.is_empty()
            && (chars + item_chars > budget
                || next_pages > hard_max_pages
                || (next_pages > target_pages && chars >= budget / 3));
        if would_exceed {
            batches.push(finish_batch(batches.len() as i64 + 1, &current));
            current = Vec::new();
            chars = 0;
        }
        current.push(material.clone());
        chars += item_chars;
    }
    if !current.is_empty() {
        batches.push(finish_batch(batches.len() as i64 + 1, &current));
    }
    attach_neighbor_context(&mut batches, materials, budget);
    batches
}

fn finish_batch(ordinal: i64, current: &[GuideMaterialBlock]) -> GuideBatchPlan {
    let page_start = current.first().map(|item| item.page_number).unwrap_or(1);
    let page_end = current
        .last()
        .map(|item| item.page_number)
        .unwrap_or(page_start);
    let mut anchor_ids = Vec::new();
    for material in current {
        if !anchor_ids.contains(&material.parent_block_id) {
            anchor_ids.push(material.parent_block_id.clone());
        }
    }
    GuideBatchPlan {
        ordinal,
        page_start,
        page_end,
        anchor_block_ids: anchor_ids,
        context_block_ids: Vec::new(),
        materials: current.to_vec(),
    }
}

fn material_cost(material: &GuideMaterialBlock) -> usize {
    // Covers the wire catalog metadata as well as text; never budget OCR alone.
    serde_json::to_string(material)
        .unwrap_or_default()
        .chars()
        .count()
        + 128
}

fn attach_neighbor_context(
    batches: &mut [GuideBatchPlan],
    all: &[GuideMaterialBlock],
    budget: usize,
) {
    for batch in batches.iter_mut() {
        let mut context = Vec::new();
        let mut used: usize = batch.materials.iter().map(material_cost).sum();
        if let Some(before) = all
            .iter()
            .rev()
            .find(|item| item.page_number == batch.page_start - 1)
        {
            if !batch.anchor_block_ids.contains(&before.parent_block_id)
                && used + material_cost(before) <= budget
            {
                used += material_cost(before);
                context.push(before.parent_block_id.clone());
                batch.materials.insert(0, before.clone());
            }
        }
        if let Some(after) = all
            .iter()
            .find(|item| item.page_number == batch.page_end + 1)
        {
            if !batch.anchor_block_ids.contains(&after.parent_block_id)
                && used + material_cost(after) <= budget
            {
                context.push(after.parent_block_id.clone());
                batch.materials.push(after.clone());
            }
        }
        batch.context_block_ids = context;
    }
}

pub fn catalog_value(batch: &GuideBatchPlan) -> serde_json::Value {
    serde_json::Value::Array(
        batch
            .materials
            .iter()
            .map(|block| {
                let role = if batch.context_block_ids.contains(&block.parent_block_id) {
                    "context"
                } else {
                    "anchor"
                };
                serde_json::json!({
                    "blockId": block.id,
                    "parentBlockId": block.parent_block_id,
                    "ref": format!("p{}-{}", block.page_number, block.block_index),
                    "pageNumber": block.page_number,
                    "blockIndex": block.block_index,
                    "blockType": block.block_type,
                    "bbox": block.bbox,
                    "role": role,
                    "fragmentIndex": block.fragment_index,
                    "fragmentCount": block.fragment_count,
                    "text": block.text
                })
            })
            .collect(),
    )
}

pub fn default_char_budget() -> usize {
    GUIDE_V2_BATCH_CHAR_BUDGET
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(id: &str, page: i64, index: i64, text: &str) -> GuideCatalogBlock {
        GuideCatalogBlock {
            id: id.to_string(),
            page_number: page,
            block_index: index,
            block_type: "Text".into(),
            bbox: [0, 0, 10, 10],
            excerpt: Some(text.to_string()),
        }
    }

    #[test]
    fn long_page_and_neighbors_stay_within_wire_budget() {
        let blocks = vec![
            block("a", 1, 0, &"字".repeat(20000)),
            block("b", 2, 0, &"文".repeat(1800)),
        ];
        let materials = materials_from_catalog(&blocks, 800);
        let batches = plan_material_batches(&materials, 2000);
        assert!(batches.len() > 10);
        for batch in &batches {
            assert!(catalog_value(batch).to_string().chars().count() <= 2000);
        }
        let collected: String = batches
            .iter()
            .flat_map(|b| {
                b.materials
                    .iter()
                    .filter(|m| b.anchor_block_ids.contains(&m.parent_block_id))
            })
            .filter(|m| m.id == "a")
            .map(|m| m.text.as_str())
            .collect();
        assert_eq!(collected, "字".repeat(20000));
    }

    #[test]
    fn keeps_full_body_text_and_reading_order() {
        let long = "正文".repeat(200);
        let blocks = vec![
            block("a", 1, 0, "标题"),
            block("b", 1, 1, &long),
            block("c", 1, 2, "公式附近的说明"),
        ];
        let materials = materials_from_catalog(&blocks, 80);
        assert!(materials.iter().any(|item| item.text.contains("正文")));
        assert_eq!(materials[0].id, "a");
        let batches = plan_material_batches_with_limits(&materials, 500, 6, 8);
        assert!(!batches.is_empty());
        assert!(batches[0].anchor_block_ids.contains(&"b".to_string()));
    }

    #[test]
    fn split_page_does_not_duplicate_parent_anchor() {
        let blocks = (1..=12)
            .map(|page| block(&format!("p{page}"), page, 0, &"段".repeat(400)))
            .collect::<Vec<_>>();
        let materials = materials_from_catalog(&blocks, 120);
        let batches = plan_material_batches_with_limits(&materials, 800, 2, 3);
        assert!(batches.len() >= 2);
        let mut seen = std::collections::HashSet::new();
        for batch in &batches {
            for id in &batch.anchor_block_ids {
                assert!(seen.insert(id.clone()), "duplicate anchor {id}");
            }
        }
    }

    #[test]
    fn long_block_keeps_parent_id() {
        let blocks = vec![block("huge", 1, 0, &"字".repeat(3000))];
        let materials = materials_from_catalog(&blocks, 400);
        assert!(materials.len() > 1);
        assert!(materials.iter().all(|item| item.parent_block_id == "huge"));
    }
}
