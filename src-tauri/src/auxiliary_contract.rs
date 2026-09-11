//! Frozen v2 model contracts. Application IDs and manual edits never enter these schemas.
use crate::document_artifacts::DocumentArtifactKind;
use serde_json::{json, Value};
type Result<T> = std::result::Result<T, String>;
fn object(fields: &[(&str, Value)]) -> Value {
    let properties: serde_json::Map<String, Value> = fields
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    json!({"type":"object","additionalProperties":false,"required":fields.iter().map(|(k,_)|*k).collect::<Vec<_>>(),"properties":properties})
}
fn text() -> Value {
    json!({"type":"string","minLength":1})
}
fn nullable(v: Value) -> Value {
    json!({"anyOf":[v,{"type":"null"}]})
}
fn array(v: Value) -> Value {
    json!({"type":"array","items":v})
}
fn nonempty_array(v: Value) -> Value {
    json!({"type":"array","items":v,"minItems":1})
}
fn enumeration(values: &str) -> Value {
    json!({"type":"string","enum":values.split('|').collect::<Vec<_>>()})
}
fn integer(min: u64) -> Value {
    json!({"type":"integer","minimum":min})
}
fn contributors() -> Value {
    array(object(&[
        ("name", text()),
        (
            "role",
            enumeration("author|editor|translator|other|unknown"),
        ),
        ("roleLabel", nullable(text())),
        ("order", nullable(integer(1))),
    ]))
}
fn versions() -> Value {
    array(object(&[
        (
            "kind",
            enumeration("manuscript_label|revision|edition|printing|other"),
        ),
        ("label", text()),
    ]))
}
fn dates() -> Value {
    array(object(&[("event",enumeration("publication|online_publication|print_publication|release|submission|receipt|acceptance|revision|copyright|printing|other|unknown")),("eventLabel",nullable(text())),("dateText",text()),("value",nullable(text()))]))
}
fn identifiers() -> Value {
    array(object(&[
        ("type", enumeration("doi|isbn|issn|arxiv|other|unknown")),
        ("label", nullable(text())),
        ("identifierText", text()),
        ("value", nullable(text())),
    ]))
}
fn publishers() -> Value {
    array(object(&[
        ("name", text()),
        (
            "role",
            enumeration("publisher|imprint|distributor|issuing_body|other|unknown"),
        ),
        ("roleLabel", nullable(text())),
        ("places", array(text())),
        ("scope", nullable(text())),
    ]))
}
fn common() -> Vec<(&'static str, Value)> {
    vec![
        ("title", nullable(text())),
        ("alternateTitles", array(text())),
        ("contributors", contributors()),
        ("versionInfo", versions()),
        ("dates", dates()),
        ("identifiers", identifiers()),
        ("publishingEntities", publishers()),
    ]
}
fn container() -> Value {
    let mut fields = common();
    fields.push((
        "type",
        enumeration("book|journal|proceedings|series|other|unknown"),
    ));
    fields.push((
        "placement",
        object(
            &[
                "part",
                "volume",
                "issue",
                "chapterNumber",
                "sectionNumber",
                "pages",
                "articleNumber",
            ]
            .iter()
            .map(|k| (*k, nullable(text())))
            .collect::<Vec<_>>(),
        ),
    ));
    nullable(object(&fields))
}
fn paths() -> Value {
    let mut v = nonempty_array(text());
    v["uniqueItems"] = json!(true);
    v
}
fn source(supports: Value) -> Value {
    object(&[
        ("pageNumber", nullable(integer(1))),
        ("locator", json!({"type":"string"})),
        ("excerpt", json!({"type":"string"})),
        ("supports", supports),
    ])
}
pub(crate) fn response_schema(kind: DocumentArtifactKind) -> Value {
    let (key, body) = match kind {
        DocumentArtifactKind::Glossary => (
            "glossary",
            array(object(&[
                ("term", text()),
                ("aliases", array(text())),
                ("definition", text()),
                ("usage", text()),
                ("sources", array(source(text()))),
            ])),
        ),
        DocumentArtifactKind::SymbolTable => (
            "symbolTable",
            array(object(&[
                ("symbol", text()),
                ("meaning", text()),
                ("scope", text()),
                ("sources", array(source(text()))),
            ])),
        ),
        DocumentArtifactKind::Metadata => {
            let mut doc = common();
            doc.push((
                "type",
                enumeration("article|book|chapter|section|report|thesis|other|unknown"),
            ));
            doc.push(("coverage", enumeration("complete|partial|unknown")));
            doc.push((
                "abstracts",
                array(object(&[
                    ("label", nullable(text())),
                    ("languages", array(text())),
                    ("completeness", enumeration("complete|partial|unknown")),
                    (
                        "segments",
                        array(object(&[("heading", nullable(text())), ("text", text())])),
                    ),
                ])),
            ));
            let mut target = common();
            target.push(("container", container()));
            let related=array(object(&[("relativeTo",enumeration("document|container")),("relations",nonempty_array(object(&[("type",enumeration("earlier_version|later_version|translation|translation_source|other|unknown")),("relationText",nullable(text()))]))),("target",object(&target))]));
            let issues=array(object(&[("type",enumeration("not_found|not_applicable|unreadable|incomplete|ambiguous|conflict|normalization_limited|suspected_error|source_unlocated|other")),("fieldPaths",paths()),("explanation",text()),("candidates",array(object(&[("fieldPaths",paths()),("text",text()),("explanation",text())]))),("textGap",nullable(object(&[("segmentsPath",text()),("afterIndex",nullable(integer(0))),("beforeIndex",nullable(integer(0)))])))]));
            (
                "metadata",
                object(&[
                    ("document", object(&doc)),
                    ("container", container()),
                    ("relatedVersions", related),
                    (
                        "sources",
                        array(source(nonempty_array(object(&[
                            ("fieldPath", text()),
                            ("explanation", text()),
                        ])))),
                    ),
                    ("issues", issues),
                ]),
            )
        }
        DocumentArtifactKind::Brief => {
            return crate::document_artifacts::response_schema(kind, false)
        }
    };
    json!({"name":format!("document_{}_v2",kind.as_str()),"strict":true,"schema":object(&[(key,body)])})
}
/// The vocabulary is deliberately closed: unsupported schema keywords fail locally.
pub(crate) fn validate(value: &Value, schema: &Value, path: &str) -> Result<()> {
    for key in schema.as_object().ok_or("Invalid schema")?.keys() {
        if ![
            "type",
            "anyOf",
            "enum",
            "properties",
            "required",
            "additionalProperties",
            "items",
            "minItems",
            "maxItems",
            "minLength",
            "minimum",
            "uniqueItems",
        ]
        .contains(&key.as_str())
        {
            return Err(format!("{path}: unsupported schema keyword {key}"));
        }
    }
    if let Some(choices) = schema["anyOf"].as_array() {
        if choices.iter().any(|s| validate(value, s, path).is_ok()) {
            return Ok(());
        }
        return Err(format!("{path}: value matches no permitted type"));
    }
    let valid = match schema["type"].as_str() {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("integer") => value.is_i64() || value.is_u64(),
        Some("null") => value.is_null(),
        _ => false,
    };
    if !valid {
        return Err(format!("{path}: invalid value type"));
    }
    if let Some(allowed) = schema["enum"].as_array() {
        if !allowed.contains(value) {
            return Err(format!("{path}: invalid enum value"));
        }
    }
    if let Some(min) = schema["minimum"].as_i64() {
        if value.as_i64().is_none_or(|v| v < min) {
            return Err(format!("{path}: integer below minimum"));
        }
    }
    if let Some(fields) = value.as_object() {
        let props = schema["properties"]
            .as_object()
            .ok_or("Missing properties")?;
        for key in fields.keys() {
            if !props.contains_key(key) {
                return Err(format!("{path}/{key}: unexpected field"));
            }
        }
        for key in schema["required"].as_array().ok_or("Missing required")? {
            let key = key.as_str().ok_or("Invalid required")?;
            validate(
                fields
                    .get(key)
                    .ok_or_else(|| format!("{path}/{key}: missing field"))?,
                &props[key],
                &format!("{path}/{key}"),
            )?;
        }
    }
    if let Some(items) = value.as_array() {
        if items.len() < schema["minItems"].as_u64().unwrap_or(0) as usize {
            return Err(format!("{path}: empty array"));
        }
        if schema["maxItems"]
            .as_u64()
            .is_some_and(|max| items.len() > max as usize)
        {
            return Err(format!("{path}: too many items"));
        }
        for (i, item) in items.iter().enumerate() {
            if schema["uniqueItems"] == true && items[..i].contains(item) {
                return Err(format!("{path}/{i}: duplicate item"));
            }
            validate(item, &schema["items"], &format!("{path}/{i}"))?;
        }
    }
    if schema["minLength"].as_u64().unwrap_or(0) > 0
        && value.as_str().is_some_and(|v| v.trim().is_empty())
    {
        return Err(format!("{path}: empty text"));
    }
    Ok(())
}
fn pointer<'a>(root: &'a Value, path: &str, allowed: &[&str]) -> Result<(&'a Value, Vec<String>)> {
    if !path.starts_with('/') {
        return Err(format!("{path}: expected JSON Pointer"));
    }
    let mut current = root;
    let mut tokens = vec![];
    for token in path[1..].split('/') {
        let mut decoded = String::new();
        let mut chars = token.chars();
        while let Some(c) = chars.next() {
            if c == '~' {
                decoded.push(match chars.next() {
                    Some('0') => '~',
                    Some('1') => '/',
                    _ => return Err(format!("{path}: invalid escape")),
                });
            } else {
                decoded.push(c);
            }
        }
        current = match current {
            Value::Object(map) => map.get(&decoded),
            Value::Array(items) => {
                if decoded.is_empty()
                    || !decoded.bytes().all(|c| c.is_ascii_digit())
                    || (decoded.len() > 1 && decoded.starts_with('0'))
                {
                    return Err(format!("{path}: invalid array index"));
                }
                decoded.parse::<usize>().ok().and_then(|i| items.get(i))
            }
            _ => None,
        }
        .ok_or_else(|| format!("{path}: path does not exist"))?;
        tokens.push(decoded);
    }
    if !allowed.contains(&tokens[0].as_str()) {
        return Err(format!("{path}: prohibited target"));
    }
    Ok((current, tokens))
}
fn items(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn source_checks(source: &Value, pages: Option<i64>, path: &str) -> Result<()> {
    if let Some(page) = source["pageNumber"].as_i64() {
        if page < 1 || pages.is_some_and(|p| page > p) {
            return Err(format!("{path}/pageNumber: outside PDF page range"));
        }
    }
    if source["pageNumber"].is_null()
        && source["locator"].as_str().unwrap_or("").trim().is_empty()
        && source["excerpt"].as_str().unwrap_or("").trim().is_empty()
    {
        return Err(format!("{path}: source has no location or excerpt"));
    }
    Ok(())
}
fn normalized_dates(v: &Value, path: &str) -> Result<()> {
    if let Some(fields) = v.as_object() {
        if fields.contains_key("dateText") {
            if let Some(date) = v["value"].as_str() {
                let digits = date.bytes().enumerate().all(|(i, b)| {
                    if i == 4 || i == 7 {
                        b == b'-'
                    } else {
                        b.is_ascii_digit()
                    }
                });
                let valid = digits
                    && !date.starts_with("0000")
                    && match date.len() {
                        4 => date.bytes().all(|b| b.is_ascii_digit()) && date != "0000",
                        7 => {
                            chrono::NaiveDate::parse_from_str(&format!("{date}-01"), "%Y-%m-%d")
                                .is_ok()
                                && date.as_bytes()[4] == b'-'
                        }
                        10 => {
                            chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").is_ok()
                                && date.as_bytes()[4] == b'-'
                                && date.as_bytes()[7] == b'-'
                        }
                        _ => false,
                    };
                if !valid {
                    return Err(format!("{path}/value: invalid normalized date"));
                }
            }
        }
        for (k, child) in fields {
            normalized_dates(child, &format!("{path}/{k}"))?;
        }
    } else if let Some(values) = v.as_array() {
        for (i, child) in values.iter().enumerate() {
            normalized_dates(child, &format!("{path}/{i}"))?;
        }
    }
    Ok(())
}
pub(crate) fn validate_relations(
    body: &Value,
    kind: DocumentArtifactKind,
    pages: Option<i64>,
) -> Result<()> {
    if kind != DocumentArtifactKind::Metadata {
        for (i, row) in items(body).iter().enumerate() {
            for (j, source) in items(&row["sources"]).iter().enumerate() {
                source_checks(source, pages, &format!("/{i}/sources/{j}"))?;
            }
        }
        return Ok(());
    }
    normalized_dates(body, "")?;
    for (i, abstract_item) in items(&body["document"]["abstracts"]).iter().enumerate() {
        if items(&abstract_item["segments"]).is_empty() {
            let base = format!("/document/abstracts/{i}");
            let explained = items(&body["issues"]).iter().any(|issue| {
                items(&issue["fieldPaths"]).iter().any(|p| {
                    p.as_str().is_some_and(|p| {
                        p == base
                            || p.starts_with(&format!("{base}/"))
                            || base.starts_with(&format!("{p}/"))
                    })
                })
            });
            if abstract_item["completeness"] != "partial" || !explained {
                return Err(format!(
                    "{base}: unreadable abstract needs partial completeness and a documented issue"
                ));
            }
        }
    }
    for version in items(&body["relatedVersions"]) {
        if version["relativeTo"] == "container" && body["container"].is_null() {
            return Err("/relatedVersions: relativeTo container requires a container".into());
        }
    }
    for (i, source) in items(&body["sources"]).iter().enumerate() {
        source_checks(source, pages, &format!("/sources/{i}"))?;
        for support in items(&source["supports"]) {
            pointer(
                body,
                support["fieldPath"].as_str().unwrap(),
                &["document", "container", "relatedVersions", "issues"],
            )?;
        }
    }
    for (i, issue) in items(&body["issues"]).iter().enumerate() {
        let affected = items(&issue["fieldPaths"])
            .iter()
            .map(|p| {
                pointer(
                    body,
                    p.as_str().unwrap(),
                    &["document", "container", "relatedVersions"],
                )
                .map(|(_, t)| t)
            })
            .collect::<Result<Vec<_>>>()?;
        for candidate in items(&issue["candidates"]) {
            for path in items(&candidate["fieldPaths"]) {
                let (_, tokens) = pointer(
                    body,
                    path.as_str().unwrap(),
                    &["document", "container", "relatedVersions"],
                )?;
                if !affected.iter().any(|parent| tokens.starts_with(parent)) {
                    return Err(format!(
                        "/issues/{i}/candidates: path outside affected scope"
                    ));
                }
            }
        }
        if !issue["textGap"].is_null() {
            let gap = &issue["textGap"];
            let path = gap["segmentsPath"].as_str().unwrap();
            let (segments, tokens) = pointer(body, path, &["document"])?;
            if tokens.len() != 4
                || tokens[1] != "abstracts"
                || tokens[3] != "segments"
                || !items(&issue["fieldPaths"]).contains(&json!(path))
            {
                return Err(format!(
                    "/issues/{i}/textGap: expected affected abstract segments"
                ));
            }
            let abstract_item = &body["document"]["abstracts"][tokens[2].parse::<usize>().unwrap()];
            if abstract_item["completeness"] != "partial" {
                return Err(format!("/issues/{i}/textGap: abstract must be partial"));
            }
            let n = items(segments).len() as u64;
            let after = gap["afterIndex"].as_u64();
            let before = gap["beforeIndex"].as_u64();
            if after.is_some_and(|v| v >= n)
                || before.is_some_and(|v| v >= n)
                || match (after, before) {
                    (Some(a), Some(b)) => b != a + 1,
                    (None, Some(b)) => b != 0,
                    (Some(a), None) => a + 1 != n,
                    (None, None) => false,
                }
            {
                return Err(format!("/issues/{i}/textGap: inconsistent gap anchors"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Value {
        serde_json::from_str::<Value>(include_str!("../tests/fixtures/auxiliary-v2.metadata.json"))
            .unwrap()
    }
    fn accepts(v: &Value) -> Result<()> {
        validate(
            v,
            &response_schema(DocumentArtifactKind::Metadata)["schema"],
            "$",
        )?;
        validate_relations(&v["metadata"], DocumentArtifactKind::Metadata, Some(8))
    }
    #[test]
    fn chapter_fixture_and_nullable_container_are_valid() {
        let mut v = sample();
        assert!(accepts(&v).is_ok());
        v["metadata"]["container"] = Value::Null;
        assert!(accepts(&v).is_ok());
    }
    #[test]
    fn invalid_paths_gaps_dates_and_pages_are_rejected() {
        let changes = [
            ("/metadata/sources/0/pageNumber", json!(9)),
            (
                "/metadata/sources/0/supports/0/fieldPath",
                json!("/metadata/document/title"),
            ),
            (
                "/metadata/sources/0/supports/0/fieldPath",
                json!("/sources/0"),
            ),
            ("/metadata/issues/0/textGap/beforeIndex", json!(0)),
            (
                "/metadata/issues/0/fieldPaths/0",
                json!("/document/abstracts/00/segments"),
            ),
            ("/metadata/container/dates/0/value", json!("2024-02-30")),
            (
                "/metadata/document/abstracts/0/completeness",
                json!("complete"),
            ),
            ("/metadata/document/title", json!("  ")),
        ];
        for (path, value) in changes {
            let mut v = sample();
            *v.pointer_mut(path).unwrap() = value;
            assert!(accepts(&v).is_err(), "{path}");
        }
    }
    #[test]
    fn null_child_and_candidate_outside_scope_are_rejected() {
        let mut v = sample();
        v["metadata"]["container"] = Value::Null;
        v["metadata"]["sources"][0]["supports"][0]["fieldPath"] = json!("/container/title");
        assert!(accepts(&v).is_err());
        let mut v = sample();
        v["metadata"]["issues"][0]["candidates"] = json!([{"fieldPaths":["/document/title"],"text":"Different title","explanation":"候选"}]);
        assert!(accepts(&v).is_err());
    }
    #[test]
    fn source_support_types_and_homonyms_are_distinct() {
        let s = json!({"pageNumber":null,"locator":"Definition A","excerpt":"","supports":"定义此处的对象。"});
        let row = json!({"symbol":"x","meaning":"第一个对象","scope":"定义 A","sources":[s]});
        let mut second = row.clone();
        second["meaning"] = json!("另一对象");
        second["scope"] = json!("定义 B");
        let value = json!({"symbolTable":[row,second]});
        assert!(validate(
            &value,
            &response_schema(DocumentArtifactKind::SymbolTable)["schema"],
            "$"
        )
        .is_ok());
        let mut bad = value.clone();
        bad["symbolTable"][0]["sources"][0]["supports"] = json!([]);
        assert!(validate(
            &bad,
            &response_schema(DocumentArtifactKind::SymbolTable)["schema"],
            "$"
        )
        .is_err());
    }
    #[test]
    fn source_without_any_locator_is_rejected() {
        let mut v = sample();
        v["metadata"]["sources"][0]["pageNumber"] = Value::Null;
        v["metadata"]["sources"][0]["locator"] = json!("");
        v["metadata"]["sources"][0]["excerpt"] = json!("");
        assert!(accepts(&v).is_err());
    }
}
