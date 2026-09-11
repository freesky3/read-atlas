//! Hub `chapter` 排序键：与前端 `hubSort.ts` 的 `chapterSortKey` 同一算法。
//! 可解析的章节号按段零填充；无法解析的排在后面。

pub(crate) fn chapter_sort_key(raw: Option<&str>) -> String {
    match parse_chapter_number(raw.unwrap_or("")) {
        Some(parts) => parts
            .into_iter()
            .map(|n| format!("{n:08}"))
            .collect::<Vec<_>>()
            .join("."),
        None => format!("~\t{}", raw.unwrap_or("")),
    }
}

fn parse_chapter_number(raw: &str) -> Option<Vec<i64>> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for part in trimmed.split(['.', '-']) {
        if part.is_empty() || !part.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        parts.push(part.parse().ok()?);
    }
    Some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_one_dot_two_before_one_dot_ten() {
        assert!(chapter_sort_key(Some("1.2")) < chapter_sort_key(Some("1.10")));
        assert!(chapter_sort_key(Some("1")) < chapter_sort_key(Some("附录")));
    }
}
