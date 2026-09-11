//! D-063 PR 5：批次费用预览。
//!
//! 预览只使用本地已验证 route、文档元数据和这份版本化价目，不向 Provider
//! 发请求。金额是十进制字符串；未知费用保持 unknown，不写成 0。
//! 多币种分开累计，不做汇率换算。最终收费以 Usage Receipt 为准。

use serde::{Deserialize, Serialize};

use crate::library_paths::LONG_PDF_PAGE_LIMIT;
use crate::model_settings::ProviderKind;

/// 价目版本。Start 时若与计划冻结的版本不同，必须重新预览。
pub(crate) const PRICE_CATALOG_VERSION: &str = "2026-09-01";
const USD: &str = "USD";
const MICROS: i64 = 1_000_000;
/// 本地价目：Mistral OCR 按页精确估算。这是预览假设，不是账单。
const MISTRAL_OCR_MICROS_PER_PAGE: i64 = 1_000;
/// Brief 工作量只能给区间：每页下限 / 上限（微美元）。
const BRIEF_MIN_MICROS_PER_PAGE: i64 = 500;
const BRIEF_MAX_MICROS_PER_PAGE: i64 = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CostConfidence {
    Exact,
    UpperBound,
    Estimate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CostEstimate {
    pub confidence: CostConfidence,
    pub currency: String,
    pub minimum: String,
    pub maximum: String,
    pub basis: String,
    pub price_catalog_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CostPreview {
    #[serde(default)]
    pub marginal_estimates: Vec<CostEstimate>,
    #[serde(default)]
    pub unknown_item_count: i64,
    #[serde(default)]
    pub unknown_reason_codes: Vec<String>,
    #[serde(default)]
    pub joined_existing_job_count: i64,
}

impl CostPreview {
    pub(crate) fn empty() -> Self {
        Self {
            marginal_estimates: Vec::new(),
            unknown_item_count: 0,
            unknown_reason_codes: Vec::new(),
            joined_existing_job_count: 0,
        }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.marginal_estimates.is_empty()
            && self.unknown_item_count == 0
            && self.joined_existing_job_count == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderWorkKind {
    Ocr,
    Brief,
}

pub(crate) fn is_long_pdf(page_count: Option<i64>) -> bool {
    page_count.is_some_and(|count| count >= LONG_PDF_PAGE_LIMIT)
}

/// 把一项边际工作量并入预览。`joined` 的共享 Job 不计新增费用。
pub(crate) fn add_item_estimate(
    preview: &mut CostPreview,
    kind: ProviderWorkKind,
    page_count: Option<i64>,
    paper_kind: Option<&ProviderKind>,
    joined: bool,
) {
    if joined {
        preview.joined_existing_job_count += 1;
        return;
    }
    match kind {
        ProviderWorkKind::Ocr => match page_count {
            Some(pages) if pages > 0 => add_or_merge(
                preview,
                CostConfidence::Exact,
                pages.saturating_mul(MISTRAL_OCR_MICROS_PER_PAGE),
                pages.saturating_mul(MISTRAL_OCR_MICROS_PER_PAGE),
                "mistral-ocr-latest · per page from local catalog",
            ),
            _ => mark_unknown(preview, "page_count_unknown"),
        },
        ProviderWorkKind::Brief => match paper_kind {
            Some(ProviderKind::OpenaiCompatible | ProviderKind::GeminiProxy) | None => {
                mark_unknown(preview, "custom_endpoint_unpriced")
            }
            Some(ProviderKind::Gemini | ProviderKind::Grok) => match page_count {
                Some(pages) if pages > 0 => add_or_merge(
                    preview,
                    CostConfidence::Estimate,
                    pages.saturating_mul(BRIEF_MIN_MICROS_PER_PAGE),
                    pages.saturating_mul(BRIEF_MAX_MICROS_PER_PAGE),
                    "orientation pack token range from page count · local catalog",
                ),
                _ => mark_unknown(preview, "page_count_unknown"),
            },
        },
    }
}

pub(crate) fn parse_usd_micros(value: &str) -> Option<i64> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (sign, rest) = match trimmed.strip_prefix('-') {
        Some(rest) => (-1_i64, rest),
        None => (1_i64, trimmed),
    };
    if rest.is_empty() || rest.chars().filter(|ch| *ch == '.').count() > 1 {
        return None;
    }
    let mut parts = rest.split('.');
    let whole = parts.next()?;
    let frac = parts.next().unwrap_or("0");
    if whole.is_empty() || !whole.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    if !frac.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }
    let whole: i64 = whole.parse().ok()?;
    let mut frac_digits = frac.to_string();
    if frac_digits.len() > 6 {
        frac_digits.truncate(6);
    }
    while frac_digits.len() < 6 {
        frac_digits.push('0');
    }
    let frac: i64 = frac_digits.parse().ok()?;
    Some(sign * (whole.saturating_mul(MICROS).saturating_add(frac)))
}

pub(crate) fn format_usd(micros: i64) -> String {
    let sign = if micros < 0 { "-" } else { "" };
    let micros = micros.abs();
    let dollars = micros / MICROS;
    let frac = micros % MICROS;
    if frac == 0 {
        return format!("{sign}{dollars}");
    }
    let mut frac_str = format!("{frac:06}");
    while frac_str.ends_with('0') {
        frac_str.pop();
    }
    format!("{sign}{dollars}.{frac_str}")
}

pub(crate) fn estimate_exceeds_ceiling(estimate_max: &str, ceiling: &str) -> bool {
    match (parse_usd_micros(estimate_max), parse_usd_micros(ceiling)) {
        (Some(max), Some(limit)) => max > limit,
        _ => true,
    }
}

fn mark_unknown(preview: &mut CostPreview, reason: &str) {
    preview.unknown_item_count += 1;
    if !preview
        .unknown_reason_codes
        .iter()
        .any(|code| code == reason)
    {
        preview.unknown_reason_codes.push(reason.to_string());
    }
}

fn add_or_merge(
    preview: &mut CostPreview,
    confidence: CostConfidence,
    minimum: i64,
    maximum: i64,
    basis: &str,
) {
    if let Some(existing) = preview.marginal_estimates.iter_mut().find(|estimate| {
        estimate.currency == USD
            && estimate.confidence == confidence
            && estimate.price_catalog_version == PRICE_CATALOG_VERSION
            && estimate.basis == basis
    }) {
        let min = parse_usd_micros(&existing.minimum)
            .unwrap_or(0)
            .saturating_add(minimum);
        let max = parse_usd_micros(&existing.maximum)
            .unwrap_or(0)
            .saturating_add(maximum);
        existing.minimum = format_usd(min);
        existing.maximum = format_usd(max);
        return;
    }
    preview.marginal_estimates.push(CostEstimate {
        confidence,
        currency: USD.to_string(),
        minimum: format_usd(minimum),
        maximum: format_usd(maximum),
        basis: basis.to_string(),
        price_catalog_version: PRICE_CATALOG_VERSION.to_string(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ocr_with_known_pages_is_exact_and_not_zero() {
        let mut preview = CostPreview::empty();
        add_item_estimate(&mut preview, ProviderWorkKind::Ocr, Some(12), None, false);
        assert_eq!(preview.unknown_item_count, 0);
        assert_eq!(preview.marginal_estimates.len(), 1);
        let estimate = &preview.marginal_estimates[0];
        assert_eq!(estimate.confidence, CostConfidence::Exact);
        assert_eq!(estimate.currency, "USD");
        assert_eq!(estimate.minimum, "0.012");
        assert_eq!(estimate.maximum, "0.012");
        assert_eq!(estimate.price_catalog_version, PRICE_CATALOG_VERSION);
    }

    #[test]
    fn missing_page_count_is_unknown_not_zero() {
        let mut preview = CostPreview::empty();
        add_item_estimate(&mut preview, ProviderWorkKind::Ocr, None, None, false);
        assert_eq!(preview.unknown_item_count, 1);
        assert!(preview.marginal_estimates.is_empty());
        assert_eq!(preview.unknown_reason_codes, ["page_count_unknown"]);
    }

    #[test]
    fn custom_endpoint_brief_is_unknown() {
        let mut preview = CostPreview::empty();
        add_item_estimate(
            &mut preview,
            ProviderWorkKind::Brief,
            Some(10),
            Some(&ProviderKind::OpenaiCompatible),
            false,
        );
        assert_eq!(preview.unknown_item_count, 1);
        assert_eq!(preview.unknown_reason_codes, ["custom_endpoint_unpriced"]);
        assert!(preview.marginal_estimates.is_empty());
    }

    #[test]
    fn joined_job_adds_no_marginal_cost() {
        let mut preview = CostPreview::empty();
        add_item_estimate(&mut preview, ProviderWorkKind::Ocr, Some(40), None, true);
        assert_eq!(preview.joined_existing_job_count, 1);
        assert!(preview.marginal_estimates.is_empty());
        assert_eq!(preview.unknown_item_count, 0);
    }

    #[test]
    fn ceiling_rejects_a_quiet_increase() {
        assert!(estimate_exceeds_ceiling("0.02", "0.012"));
        assert!(!estimate_exceeds_ceiling("0.012", "0.012"));
        assert!(!estimate_exceeds_ceiling("0.01", "0.012"));
    }

    #[test]
    fn long_pdf_uses_the_existing_page_limit() {
        assert!(!is_long_pdf(Some(79)));
        assert!(is_long_pdf(Some(80)));
        assert!(!is_long_pdf(None));
    }
}
