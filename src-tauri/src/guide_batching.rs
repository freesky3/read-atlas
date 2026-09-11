use crate::guide_protocol::{GUIDE_BATCH_HARD_MAX_PAGES, GUIDE_BATCH_TARGET_PAGES};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideBatch {
    pub ordinal: i64,
    pub page_start: i64,
    pub page_end: i64,
    pub block_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuidePageBlocks {
    pub page: i64,
    pub block_ids: Vec<String>,
}

pub fn plan_guide_batches(pages: &[GuidePageBlocks], section_starts: &[i64]) -> Vec<GuideBatch> {
    plan_guide_batches_with_limits(
        pages,
        section_starts,
        GUIDE_BATCH_TARGET_PAGES,
        GUIDE_BATCH_HARD_MAX_PAGES,
    )
}

pub fn plan_guide_batches_with_limits(
    pages: &[GuidePageBlocks],
    section_starts: &[i64],
    target_pages: i64,
    hard_max_pages: i64,
) -> Vec<GuideBatch> {
    let target = target_pages.max(1);
    let hard_max = hard_max_pages.max(target);
    if pages.is_empty() {
        return Vec::new();
    }
    let page_numbers: Vec<i64> = pages.iter().map(|page| page.page).collect();
    let starts: std::collections::HashSet<i64> = section_starts
        .iter()
        .copied()
        .filter(|page| *page > page_numbers[0])
        .collect();
    let groups = page_groups(&page_numbers, &starts, target, hard_max);
    groups
        .into_iter()
        .enumerate()
        .map(|(index, group)| {
            let page_start = *group.first().unwrap_or(&1);
            let page_end = *group.last().unwrap_or(&page_start);
            let block_ids = pages
                .iter()
                .filter(|page| group.contains(&page.page))
                .flat_map(|page| page.block_ids.clone())
                .collect();
            GuideBatch {
                ordinal: index as i64 + 1,
                page_start,
                page_end,
                block_ids,
            }
        })
        .collect()
}

pub fn batch_too_sparse(page_start: i64, page_end: i64, locatable: usize) -> bool {
    let pages = (page_end - page_start + 1).max(1);
    (locatable as i64) < (pages / 2).max(2)
}

fn page_groups(
    pages: &[i64],
    section_starts: &std::collections::HashSet<i64>,
    target_pages: i64,
    hard_max_pages: i64,
) -> Vec<Vec<i64>> {
    let mut result = Vec::new();
    let mut cursor = 0;
    while cursor < pages.len() {
        let page_start = pages[cursor];
        let mut hard_end = cursor;
        while hard_end + 1 < pages.len() && pages[hard_end + 1] - page_start < hard_max_pages {
            hard_end += 1;
        }
        let mut desired_end = cursor;
        while desired_end < hard_end && pages[desired_end + 1] - page_start < target_pages {
            desired_end += 1;
        }
        let mut boundary_ends = Vec::new();
        for index in (cursor + 1)
            ..=hard_end
                .saturating_add(1)
                .min(pages.len().saturating_sub(1))
        {
            if index >= pages.len() || !section_starts.contains(&pages[index]) {
                continue;
            }
            let candidate_end = index - 1;
            let span = pages[candidate_end] - page_start + 1;
            if span >= 2.max(target_pages - 2) && span <= hard_max_pages {
                boundary_ends.push(candidate_end);
            }
        }
        boundary_ends.sort_by_key(|end| ((pages[*end] - page_start + 1) - target_pages).abs());
        let end = boundary_ends.first().copied().unwrap_or(desired_end);
        result.push(pages[cursor..=end].to_vec());
        cursor = end + 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pages(numbers: &[i64]) -> Vec<GuidePageBlocks> {
        numbers
            .iter()
            .map(|page| GuidePageBlocks {
                page: *page,
                block_ids: vec![format!("b-{page}")],
            })
            .collect()
    }

    #[test]
    fn splits_on_six_page_target_without_cutting_inside_a_page() {
        let batches =
            plan_guide_batches_with_limits(&pages(&[1, 2, 3, 4, 5, 6, 7, 8, 9]), &[], 6, 8);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].page_start, 1);
        assert_eq!(batches[0].page_end, 6);
        assert_eq!(batches[1].page_start, 7);
        assert_eq!(batches[1].page_end, 9);
        assert_eq!(
            batches[0].block_ids,
            vec!["b-1", "b-2", "b-3", "b-4", "b-5", "b-6"]
        );
    }

    #[test]
    fn prefers_section_boundaries_inside_the_hard_cap() {
        let batches = plan_guide_batches_with_limits(&pages(&[1, 2, 3, 4, 5, 6, 7, 8]), &[7], 6, 8);
        assert_eq!(batches[0].page_end, 6);
        assert_eq!(batches[1].page_start, 7);
    }

    #[test]
    fn empty_pages_yield_no_batches() {
        assert!(plan_guide_batches(&[], &[]).is_empty());
    }

    #[test]
    fn six_page_batch_with_one_ink_is_too_sparse() {
        assert!(batch_too_sparse(1, 6, 1));
        assert!(!batch_too_sparse(1, 6, 3));
    }
}
