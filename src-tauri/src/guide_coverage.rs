use crate::guide_validate::GuideInk;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuidePageCoverage {
    pub page_number: i64,
    pub note_count: usize,
    pub trace_count: usize,
    pub reply_count: usize,
    pub kind: String,
    pub below_target: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideCoverageReport {
    pub body_page_count: usize,
    pub annotatable_page_count: usize,
    pub note_count: usize,
    pub note_block_count: usize,
    pub trace_count: usize,
    pub reply_count: usize,
    pub failed_batches: i64,
    pub untreated_blocks: Vec<String>,
    pub material_gaps: Vec<String>,
    pub pages: Vec<GuidePageCoverage>,
    pub sparse_pages: Vec<i64>,
}

pub fn classify_page(text_chars: usize, chrome: bool) -> &'static str {
    if chrome {
        "chrome"
    } else if text_chars == 0 {
        "unreadable"
    } else if text_chars < 80 {
        "transition"
    } else if text_chars > 900 {
        "dense"
    } else {
        "body"
    }
}

pub fn target_notes(kind: &str) -> (usize, usize) {
    match kind {
        "dense" => (5, 7),
        "body" => (3, 5),
        "transition" => (1, 2),
        _ => (0, 0),
    }
}

pub fn count_notes(inks: &[GuideInk]) -> usize {
    inks.iter()
        .filter(|ink| matches!(ink, GuideInk::Note { .. }))
        .count()
}

pub fn build_coverage_report(
    pages: &[(i64, String, usize)],
    inks: &[GuideInk],
    failed_batches: i64,
    untreated_blocks: Vec<String>,
    material_gaps: Vec<String>,
) -> GuideCoverageReport {
    let mut notes_by_page: BTreeMap<i64, usize> = BTreeMap::new();
    let mut traces_by_page: BTreeMap<i64, usize> = BTreeMap::new();
    let note_pages: BTreeMap<&str, i64> = inks
        .iter()
        .filter_map(|ink| match ink {
            GuideInk::Note { id, anchor, .. } => Some((id.as_str(), anchor.page_number)),
            _ => None,
        })
        .collect();
    let mut replies_by_page: BTreeMap<i64, usize> = BTreeMap::new();
    let mut note_blocks = BTreeSet::new();
    let mut note_count = 0usize;
    let mut trace_count = 0usize;
    let mut reply_count = 0usize;
    for ink in inks {
        match ink {
            GuideInk::Note { anchor, .. } => {
                note_count += 1;
                *notes_by_page.entry(anchor.page_number).or_default() += 1;
                note_blocks.insert(anchor.block_id.clone());
            }
            GuideInk::Trace { anchor, .. } => {
                trace_count += 1;
                *traces_by_page.entry(anchor.page_number).or_default() += 1;
            }
            GuideInk::Reply { parent_id, .. } => {
                reply_count += 1;
                if let Some(page) = note_pages.get(parent_id.as_str()) {
                    *replies_by_page.entry(*page).or_default() += 1;
                }
            }
        }
    }
    let mut page_rows = Vec::new();
    let mut body_page_count = 0usize;
    let mut annotatable = 0usize;
    let mut sparse_pages = Vec::new();
    for (page, kind, _chars) in pages {
        let notes = *notes_by_page.get(page).unwrap_or(&0);
        let traces = *traces_by_page.get(page).unwrap_or(&0);
        if matches!(kind.as_str(), "body" | "dense" | "transition") {
            annotatable += 1;
        }
        if kind == "body" || kind == "dense" {
            body_page_count += 1;
        }
        let (low, _) = target_notes(kind);
        let below = notes < low && matches!(kind.as_str(), "body" | "dense" | "transition");
        let reason = if below {
            Some(format!("低于 {kind} 页目标下界 {low}"))
        } else if kind == "chrome" {
            Some("封面、目录或参考文献页".to_string())
        } else if kind == "unreadable" {
            Some("缺少可靠锚点".to_string())
        } else {
            None
        };
        if below {
            sparse_pages.push(*page);
        }
        page_rows.push(GuidePageCoverage {
            page_number: *page,
            note_count: notes,
            trace_count: traces,
            reply_count: *replies_by_page.get(page).unwrap_or(&0),
            kind: kind.clone(),
            below_target: below,
            reason,
        });
    }
    GuideCoverageReport {
        body_page_count,
        annotatable_page_count: annotatable,
        note_count,
        note_block_count: note_blocks.len(),
        trace_count,
        reply_count,
        failed_batches,
        untreated_blocks,
        material_gaps,
        pages: page_rows,
        sparse_pages,
    }
}

pub fn needs_supplement(report: &GuideCoverageReport) -> bool {
    consecutive_empty_body_pages(report)
        || !report.sparse_pages.is_empty()
        || notes_clustered(report)
}

fn consecutive_empty_body_pages(report: &GuideCoverageReport) -> bool {
    let mut run = 0usize;
    for page in &report.pages {
        if matches!(page.kind.as_str(), "body" | "dense") && page.note_count == 0 {
            run += 1;
            if run >= 2 {
                return true;
            }
        } else if matches!(page.kind.as_str(), "body" | "dense") {
            run = 0;
        }
    }
    false
}

fn notes_clustered(report: &GuideCoverageReport) -> bool {
    let body: Vec<_> = report
        .pages
        .iter()
        .filter(|page| matches!(page.kind.as_str(), "body" | "dense"))
        .collect();
    if body.len() < 3 {
        return false;
    }
    let total: usize = body.iter().map(|page| page.note_count).sum();
    if total == 0 {
        return false;
    }
    let first = body[0].note_count;
    first * 2 >= total && body.iter().skip(1).any(|page| page.note_count == 0)
}

pub fn traces_do_not_count_as_notes(inks: &[GuideInk]) -> bool {
    count_notes(inks) == 0 && inks.iter().any(|ink| matches!(ink, GuideInk::Trace { .. }))
}

pub fn coverage_value(report: &GuideCoverageReport) -> Value {
    json!(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guide_validate::GuideLocator;

    fn note(id: &str, page: i64, block: &str) -> GuideInk {
        GuideInk::Note {
            id: id.into(),
            speaker_id: "preset:chitanda".into(),
            weight: "line".into(),
            anchor: GuideLocator {
                block_id: block.into(),
                page_number: page,
                block_type: "Text".into(),
                bbox: [0, 0, 1, 1],
            },
            body: "具体提醒".into(),
        }
    }

    fn trace(id: &str, page: i64, block: &str) -> GuideInk {
        GuideInk::Trace {
            id: id.into(),
            speaker_id: "preset:oreki".into(),
            anchor: GuideLocator {
                block_id: block.into(),
                page_number: page,
                block_type: "Text".into(),
                bbox: [0, 0, 1, 1],
            },
        }
    }

    #[test]
    fn traces_cannot_satisfy_text_density() {
        let inks = vec![trace("t1", 1, "b1"), trace("t2", 1, "b2")];
        assert!(traces_do_not_count_as_notes(&inks));
        let pages = vec![(1, "body".into(), 400)];
        let report = build_coverage_report(&pages, &inks, 0, Vec::new(), Vec::new());
        assert_eq!(report.note_count, 0);
        assert!(report.pages[0].below_target);
    }

    #[test]
    fn six_pages_with_notes_only_on_first_is_sparse() {
        let mut inks = Vec::new();
        for i in 0..6 {
            inks.push(note(&format!("n{i}"), 1, &format!("b{i}")));
        }
        let pages = (1..=6)
            .map(|page| (page, "body".into(), 500))
            .collect::<Vec<_>>();
        let report = build_coverage_report(&pages, &inks, 0, Vec::new(), Vec::new());
        assert!(needs_supplement(&report));
        assert!(report.sparse_pages.contains(&2));
        assert_eq!(report.note_count, 6);
    }
}
