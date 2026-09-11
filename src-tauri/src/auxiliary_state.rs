//! Application projections are separate from immutable model output.
use crate::document_artifacts::DocumentArtifactKind;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;
fn items(v: &Value) -> &[Value] {
    v.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn hash(v: &Value) -> String {
    format!("{:x}", Sha256::digest(v.to_string().as_bytes()))
}
fn identity(row: &Value) -> String {
    let mut row = row.as_object().cloned().unwrap_or_default();
    row.retain(|k, _| !k.starts_with('_') && k != "sources");
    hash(&Value::Object(row))
}
fn row_id(row: &Value, index: usize) -> String {
    row["_id"]
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| format!("legacy-{index}"))
}
pub(crate) fn apply_legacy_table_overrides(content: &Value, overrides: &Value) -> Value {
    if overrides.as_object().is_none_or(|o| o.is_empty()) {
        return content.clone();
    }
    let mut content = content.clone();
    if content.get("_model").is_none() {
        content["_model"] = content["entries"].clone();
    }
    let mut rows = items(&content["entries"]).to_vec();
    let mut pins = items(&content["_pinned"]).to_vec();
    let mut unmatched = items(&content["_unmatchedOverrides"]).to_vec();
    for (key, value) in overrides.as_object().into_iter().flatten() {
        let matches = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row["_id"] == *key || row["term"] == *key || row["symbol"] == *key)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            let index = matches[0];
            let row = &mut rows[index];
            let id = row_id(row, index);
            let origin = row["_originKey"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| identity(row));
            for (field, value) in value.as_object().into_iter().flatten() {
                if !field.starts_with('_') {
                    row[field] = value.clone();
                }
            }
            row["_originKey"] = json!(origin);
            row["_id"] = json!(id);
            row["_manual"] = json!(true);
            if !pins.contains(&json!(id)) {
                pins.push(json!(id));
            }
        } else {
            let record = json!({"key":key,"value":value});
            if !unmatched.contains(&record) {
                unmatched.push(record);
            }
        }
    }
    content["entries"] = json!(rows);
    content["_pinned"] = json!(pins);
    content["_unmatchedOverrides"] = json!(unmatched);
    content
}
pub(crate) fn load_legacy_table_overrides(
    connection: &rusqlite::Connection,
    revision: &str,
    kind: &str,
    content: &Value,
) -> Result<Value, String> {
    let (table, key) = match kind {
        "glossary" => ("term_overrides", "term_key"),
        "symbol_table" => ("symbol_overrides", "symbol_key"),
        _ => return Ok(content.clone()),
    };
    let sql=format!("SELECT o.{key},o.value_json FROM {table} o JOIN artifact_heads h ON h.artifact_id=o.artifact_id JOIN artifacts a ON a.id=h.artifact_id WHERE a.revision_id=?1 AND h.kind=?2");
    let mut statement = connection.prepare(&sql).map_err(|e| e.to_string())?;
    let rows = statement
        .query_map(rusqlite::params![revision, kind], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?;
    let mut overrides = serde_json::Map::new();
    for row in rows {
        let (key, value) = row.map_err(|e| e.to_string())?;
        overrides.insert(
            key,
            serde_json::from_str(&value).map_err(|e| e.to_string())?,
        );
    }
    Ok(apply_legacy_table_overrides(content, &json!(overrides)))
}

pub(crate) fn generate_table(model: Value, previous: Option<&Value>) -> Value {
    let empty = json!({});
    let old = previous.unwrap_or(&empty);
    let mut entries = items(&model).to_vec();
    let hidden = items(&old["_hidden"]).to_vec();
    entries.retain(|row| !hidden.contains(&json!(identity(row))));
    for row in &mut entries {
        row["_id"] = json!(Uuid::new_v4().to_string());
        row["_originKey"] = json!(identity(row));
    }
    let mut pins = vec![];
    for (i, row) in items(&old["entries"]).iter().enumerate() {
        let id = row_id(row, i);
        let legacy_key = if row.get("term").is_some() {
            &row["term"]
        } else {
            &row["symbol"]
        };
        let pinned = items(&old["_pinned"]).contains(&json!(id))
            || items(&old["_pinned"]).contains(legacy_key);
        let origin = row["_originKey"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| identity(row));
        let matches = entries
            .iter()
            .enumerate()
            .filter(|(_, r)| r["_originKey"] == origin)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let old_count = items(&old["entries"])
            .iter()
            .filter(|r| {
                r["_originKey"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| identity(r))
                    == origin
            })
            .count();
        if pinned {
            let mut manual = row.clone();
            manual["_id"] = json!(id);
            manual["_originKey"] = json!(origin);
            manual["_manual"] = json!(true);
            if matches.len() == 1 && old_count == 1 {
                manual.as_object_mut().unwrap().remove("_unmatched");
                entries[matches[0]] = manual;
            } else {
                manual["_unmatched"] = json!(true);
                entries.push(manual);
            }
            pins.push(json!(id));
        } else if matches.len() == 1 && old_count == 1 {
            entries[matches[0]]["_id"] = json!(id);
        }
    }
    json!({"_format":"auxiliary-v2","_model":model,"entries":entries,"_pinned":pins,"_hidden":hidden,"_unmatchedOverrides":old.get("_unmatchedOverrides").cloned().unwrap_or(json!([]))})
}
pub(crate) fn edit_table(old: &Value, mut entries: Vec<Value>, pins: Vec<String>) -> Value {
    let mut next = old.clone();
    if !next.is_object() {
        next = json!({});
    }
    if next.get("_model").is_none() {
        next["_model"] = old["entries"].clone();
    }
    let mut hidden = items(&old["_hidden"]).to_vec();
    for (i, row) in items(&old["entries"]).iter().enumerate() {
        let id = row_id(row, i);
        if !entries.iter().enumerate().any(|(j, r)| row_id(r, j) == id) {
            let origin = row["_originKey"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| identity(row));
            if !hidden.contains(&json!(origin)) {
                hidden.push(json!(origin));
            }
        }
    }
    for (i, row) in entries.iter_mut().enumerate() {
        let id = row_id(row, i);
        row["_id"] = json!(id);
        let prior = items(&old["entries"])
            .iter()
            .enumerate()
            .find(|(j, r)| row_id(r, *j) == id)
            .map(|(_, r)| r);
        let origin = prior
            .map(|r| {
                r["_originKey"]
                    .as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| identity(r))
            })
            .unwrap_or_else(|| identity(row));
        row["_originKey"] = json!(origin);
        if prior.is_none_or(|p| identity(p) != identity(row)) {
            row["_manual"] = json!(true);
        }
    }
    next["entries"] = json!(entries);
    next["_pinned"] = json!(pins);
    next["_hidden"] = json!(hidden);
    next["_format"] = json!("auxiliary-v2");
    next
}
// Store original array elements as anchors. A name or an index alone is never identity.
fn anchors(raw: &Value, path: &str) -> Vec<Value> {
    let mut current = raw;
    let mut prefix = String::new();
    let mut result = vec![];
    for token in path.trim_start_matches('/').split('/') {
        if let Some(array) = current.as_array() {
            if let Ok(index) = token.parse::<usize>() {
                if let Some(item) = array.get(index) {
                    result.push(json!({"prefix":prefix,"token":token,"item":item}));
                }
            }
        }
        prefix.push('/');
        prefix.push_str(token);
        current = raw.pointer(&prefix).unwrap_or(&Value::Null);
    }
    result
}
fn rebase(raw: &Value, path: &str, anchors: &Value) -> Option<String> {
    let mut result = path.to_string();
    // Deepest first would leave parent indices wrong. Resolve from outermost, adjusting descendant prefixes.
    let mut changes: Vec<(String, String)> = vec![];
    for anchor in items(anchors) {
        let original_prefix = anchor["prefix"].as_str()?;
        let mut prefix = original_prefix.to_string();
        for (old, new) in &changes {
            if prefix == *old || prefix.starts_with(&format!("{old}/")) {
                prefix = prefix.replacen(old, new, 1);
            }
        }
        let array = raw.pointer(&prefix)?.as_array()?;
        let matches = array
            .iter()
            .enumerate()
            .filter(|(_, r)| *r == &anchor["item"])
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        if matches.len() != 1 {
            return None;
        }
        let old = format!("{prefix}/{}", anchor["token"].as_str()?);
        let new = format!("{prefix}/{}", matches[0]);
        if result == old || result.starts_with(&format!("{old}/")) {
            result = result.replacen(&old, &new, 1);
        }
        changes.push((
            format!("{original_prefix}/{}", anchor["token"].as_str()?),
            new,
        ));
    }
    raw.pointer(&result)?;
    Some(result)
}
pub(crate) fn generate_metadata(model: Value, previous: Option<&Value>) -> Value {
    let empty = json!({});
    let mut inherited = previous.cloned().unwrap_or_else(|| empty.clone());
    if inherited.get("_inheritedFromRevision").is_some() {
        let mut unmatched = items(&inherited["_unmatchedManual"]).to_vec();
        for (path, setting) in inherited["_manual"].as_object().into_iter().flatten() {
            unmatched.push(json!({"path":path,"setting":setting,"revisionId":inherited["_inheritedFromRevision"]}));
        }
        inherited["_unmatchedManual"] = json!(unmatched);
        inherited["_manual"] = json!({});
        inherited["_pinned"] = json!([]);
        inherited["_legacy"] = json!({"_inheritedRecord":previous,"_pinned":[]});
        for key in ["_yearSelection", "_abstractSelection"] {
            inherited.as_object_mut().unwrap().remove(key);
        }
    }
    let old = &inherited;
    let mut content = model.clone();
    content["_model"] = model.clone();
    content["_format"] = json!("auxiliary-v2");
    let legacy = if old.get("document").is_none() {
        old.clone()
    } else {
        old.get("_legacy").cloned().unwrap_or(json!({}))
    };
    content["_legacy"] = legacy;
    content["_manual"] = old.get("_manual").cloned().unwrap_or(json!({}));
    content["_pinned"] = old.get("_pinned").cloned().unwrap_or(json!([]));
    let mut unmatched = vec![];
    let mut rebased = serde_json::Map::new();
    for (path, setting) in old["_manual"].as_object().into_iter().flatten() {
        if let Some(next_path) = rebase(&model, path, &setting["anchors"]) {
            if let Some(target) = content.pointer_mut(&next_path) {
                *target = setting["value"].clone();
                let mut setting = setting.clone();
                setting["anchors"] = json!(anchors(&model, &next_path));
                rebased.insert(next_path, setting);
            }
        } else {
            unmatched.push(json!({"path":path,"setting":setting}));
        }
    }
    unmatched.extend(items(&old["_unmatchedManual"]).iter().cloned());
    content["_pinned"] = json!(rebased
        .keys()
        .map(|p| json!(p))
        .chain(
            items(&old["_pinned"])
                .iter()
                .filter(|p| p.as_str().is_some_and(|p| !p.starts_with('/')))
                .cloned()
        )
        .collect::<Vec<_>>());
    content["_manual"] = json!(rebased);
    content["_unmatchedManual"] = json!(unmatched);
    for key in ["_yearSelection", "_abstractSelection", "_preferredLanguage"] {
        if let Some(value) = old.get(key) {
            content[key] = value.clone();
        }
    }
    refresh_links(&mut content, Some(old));
    content["_display"] = display(&content);
    content
}
pub(crate) fn edit_metadata(
    old: &Value,
    incoming: &Value,
    pins: Vec<String>,
    artifact_id: &str,
) -> Value {
    if old.get("document").is_none() {
        let mut next = incoming.clone();
        next["_model"] = old.get("_model").cloned().unwrap_or_else(|| old.clone());
        next["_pinned"] = json!(pins);
        return next;
    }
    let raw = &old["_model"];
    let mut next = old.clone();
    let mut manual = serde_json::Map::new();
    for path in pins.iter().filter(|p| {
        p.starts_with('/')
            && !pins
                .iter()
                .any(|parent| parent != *p && p.starts_with(&format!("{parent}/")))
    }) {
        if let Some(value) = incoming.pointer(path) {
            let setting = if old.pointer(path) == Some(value) {
                old["_manual"].get(path).cloned()
            } else {
                None
            };
            manual.insert(path.clone(),setting.unwrap_or_else(||json!({"value":value,"anchors":anchors(raw,path),"sourceArtifactId":artifact_id})));
        }
    }
    for key in ["document", "container", "relatedVersions"] {
        next[key] = raw[key].clone();
    }
    for (path, setting) in &manual {
        if let Some(target) = next.pointer_mut(path) {
            *target = setting["value"].clone();
        }
    }
    next["_manual"] = json!(manual);
    next["_pinned"] = json!(manual
        .keys()
        .map(|p| json!(p))
        .chain(
            pins.iter()
                .filter(|p| !p.starts_with('/'))
                .map(|p| json!(p))
        )
        .collect::<Vec<_>>());
    for key in ["_yearSelection", "_abstractSelection", "_preferredLanguage"] {
        if let Some(value) = incoming.get(key) {
            next[key] = value.clone();
        }
    }
    // Flat compatibility pins are kept in their original namespace, never reinterpreted as model facts.
    if let Some(legacy) = incoming.get("_legacy") {
        next["_legacy"] = legacy.clone();
    }
    refresh_links(&mut next, Some(old));
    next["_display"] = display(&next);
    next
}
// Each source still describes the immutable original. Only exact surviving targets
// receive a link in the effective view; edits never inherit a verified association.
fn refresh_links(content: &mut Value, previous: Option<&Value>) {
    let raw = content["_model"].clone();
    let map_path = |path: &str| -> Option<String> {
        let next = rebase(content, path, &json!(anchors(&raw, path)))?;
        if raw.pointer(path) == content.pointer(&next) {
            Some(next)
        } else {
            None
        }
    };
    let mut issue_links = vec![];
    let mut effective_issues = vec![];
    for issue in items(&raw["issues"]) {
        let mapped = items(&issue["fieldPaths"])
            .iter()
            .map(|p| p.as_str().and_then(map_path))
            .collect::<Option<Vec<_>>>();
        if let Some(paths) = mapped {
            let mut effective = issue.clone();
            effective["fieldPaths"] = json!(paths);
            if let Some(path) = issue["textGap"]["segmentsPath"].as_str() {
                if let Some(next) = map_path(path) {
                    effective["textGap"]["segmentsPath"] = json!(next);
                } else {
                    effective["textGap"] = Value::Null;
                }
            }
            effective_issues.push(effective);
            issue_links.push(json!(true));
        } else {
            issue_links.push(json!(false));
        }
    }
    let source_links = items(&raw["sources"])
        .iter()
        .map(|source| {
            items(&source["supports"])
                .iter()
                .map(|support| support["fieldPath"].as_str().and_then(map_path))
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    content["_sourceLinks"] = json!(source_links);
    content["_issueLinks"] = json!(issue_links);
    content["_effectiveIssues"] = json!(effective_issues);
    // App-owned IDs are kept outside model arrays and move only with exact records.
    fn collect(v: &Value, path: &str, records: &mut Vec<(String, Value)>) {
        if let Some(array) = v.as_array() {
            for (i, item) in array.iter().enumerate() {
                let path = format!("{path}/{i}");
                if item.is_object() {
                    records.push((path.clone(), item.clone()));
                }
                collect(item, &path, records);
            }
        } else if let Some(object) = v.as_object() {
            for (k, item) in object {
                if !k.starts_with('_') {
                    collect(item, &format!("{path}/{k}"), records);
                }
            }
        }
    }
    let mut records = vec![];
    for key in ["document", "container", "relatedVersions"] {
        collect(&content[key], &format!("/{key}"), &mut records);
    }
    let mut identities = serde_json::Map::new();
    for (path, value) in records {
        let candidates = previous
            .into_iter()
            .flat_map(|p| p["_identities"].as_object().into_iter().flatten())
            .filter(|(prior_path, record)| {
                prior_path.rsplit_once('/').map(|(parent, _)| parent)
                    == path.rsplit_once('/').map(|(parent, _)| parent)
                    && record["value"] == value
            })
            .map(|(_, record)| record)
            .collect::<Vec<_>>();
        let same_position = previous
            .filter(|old| old["_model"] == raw)
            .and_then(|old| old["_identities"].get(&path))
            .filter(|_| {
                !content["_manual"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .any(|(p, setting)| {
                        setting["value"].is_array()
                            && (path == *p || path.starts_with(&format!("{p}/")))
                    })
            });
        let id = if let Some(record) = same_position {
            record["id"].clone()
        } else if candidates.len() == 1 {
            candidates[0]["id"].clone()
        } else {
            json!(Uuid::new_v4().to_string())
        };
        identities.insert(path, json!({"id":id,"value":value}));
    }
    content["_identities"] = json!(identities);
}
fn affected(content: &Value, path: &str) -> bool {
    items(
        content
            .get("_effectiveIssues")
            .unwrap_or(&content["issues"]),
    )
    .iter()
    .any(|issue| {
        ["ambiguous", "conflict"].contains(&issue["type"].as_str().unwrap_or(""))
            && items(&issue["fieldPaths"]).iter().any(|p| {
                p.as_str().is_some_and(|p| {
                    p == path
                        || p.starts_with(&format!("{path}/"))
                        || path.starts_with(&format!("{p}/"))
                })
            })
    })
}
fn manual(content: &Value, path: &str) -> bool {
    content["_manual"].as_object().is_some_and(|m| {
        m.keys()
            .any(|p| p == path || path.starts_with(&format!("{p}/")))
    })
}
fn legacy_pin(content: &Value, key: &str) -> bool {
    items(&content["_legacy"]["_pinned"]).contains(&json!(key))
        || items(&content["_pinned"]).contains(&json!(key))
}
fn choose(content: &Value, path: &str, legacy_key: &str, value: Value) -> (Value, String) {
    if manual(content, path)
        || content["_manual"]
            .as_object()
            .is_some_and(|m| m.keys().any(|p| p.starts_with(&format!("{path}/"))))
    {
        return (value, "人工修改".into());
    }
    if legacy_pin(content, legacy_key) {
        return (
            content["_legacy"][legacy_key].clone(),
            "旧记录中的人工值".into(),
        );
    }
    if affected(content, path) {
        return (Value::Null, "待核对".into());
    }
    if !value.is_null()
        && !value.as_str().is_some_and(|s| s.is_empty())
        && !value.as_array().is_some_and(Vec::is_empty)
    {
        return (value, "本次提取".into());
    }
    let old = &content["_legacy"][legacy_key];
    (
        old.clone(),
        if old.is_null() {
            "未提供"
        } else {
            "旧记录"
        }
        .into(),
    )
}
fn year_for(content: &Value, owner: &str) -> (Option<i64>, String, bool) {
    let prefix = format!("/{owner}/dates");
    for (event, label) in [
        ("publication", "出版"),
        ("print_publication", "印刷出版"),
        ("online_publication", "在线发表"),
        ("release", "发布"),
    ] {
        let values = items(&content[owner]["dates"])
            .iter()
            .enumerate()
            .filter(|(_, d)| d["event"] == event)
            .collect::<Vec<_>>();
        let unresolved = values.iter().any(|(i, _)| {
            let path = format!("{prefix}/{i}");
            affected(content, &path) && !manual(content, &path)
        });
        if unresolved {
            return (None, "年份待核对".into(), true);
        }
        let years = values
            .iter()
            .filter_map(|(_, d)| d["value"].as_str()?.get(..4)?.parse::<i64>().ok())
            .collect::<std::collections::BTreeSet<_>>();
        if years.is_empty() {
            continue;
        }
        if years.len() != 1 {
            return (None, "年份待核对".into(), true);
        }
        return (
            years.first().copied(),
            format!(
                "{}{label}",
                if owner == "container" {
                    "所属出版物·"
                } else {
                    ""
                }
            ),
            false,
        );
    }
    // An unresolved empty date collection cannot be bypassed with a legacy year.
    if affected(content, &prefix) && !manual(content, &prefix) {
        return (None, "年份待核对".into(), true);
    }
    (None, String::new(), false)
}
fn language_code(value: &str) -> &str {
    match value {
        "zh" | "zh-CN" | "zh-Hans" | "中文" | "汉语" | "简体中文" | "Chinese" | "chinese" => {
            "zh-CN"
        }
        "en" | "en-US" | "en-GB" | "英文" | "英语" | "English" | "english" => "en",
        other => other,
    }
}

pub(crate) fn display(content: &Value) -> Value {
    let doc = &content["document"];
    let (title, title_source) = choose(content, "/document/title", "title", doc["title"].clone());
    let authors = json!(items(&doc["contributors"])
        .iter()
        .filter(|c| c["role"] == "author")
        .filter_map(|c| c["name"].as_str())
        .collect::<Vec<_>>());
    let (authors, authors_source) = choose(content, "/document/contributors", "authors", authors);
    let (mut year, mut year_label, mut blocked) = year_for(content, "document");
    if year.is_none()
        && !blocked
        && ["chapter", "section", "article"].contains(&doc["type"].as_str().unwrap_or(""))
        && ["book", "proceedings"].contains(&content["container"]["type"].as_str().unwrap_or(""))
    {
        (year, year_label, blocked) = year_for(content, "container");
    }
    if let Some(selection) = content.get("_yearSelection").filter(|v| !v.is_null()) {
        if selection.get("value").is_some() {
            year = selection["value"].as_i64();
            year_label = "人工年份".into();
        } else if let Some(date) = selection.get("date") {
            let owner = selection["owner"].as_str().unwrap_or("document");
            let matches = items(&content[owner]["dates"])
                .iter()
                .filter(|d| *d == date)
                .collect::<Vec<_>>();
            if matches.len() == 1 {
                year = date["value"]
                    .as_str()
                    .and_then(|v| v.get(..4))
                    .and_then(|v| v.parse().ok());
                year_label = "人工选择的日期".into();
            } else {
                year = None;
                year_label = "所选日期待核对".into();
            }
        }
    } else if legacy_pin(content, "publicationYear") {
        year = content["_legacy"]["publicationYear"].as_i64();
        year_label = "旧记录中的人工年份".into();
    } else if year.is_none()
        && !blocked
        && !affected(content, "/document/dates")
        && content["_legacy"]["publicationYear"].is_i64()
    {
        year = content["_legacy"]["publicationYear"].as_i64();
        year_label = "旧记录·出版年份".into();
    }
    let (venue, venue_source) = choose(
        content,
        "/container/title",
        "venue",
        content["container"]["title"].clone(),
    );
    let (chapter, chapter_source) = choose(
        content,
        "/container/placement/chapterNumber",
        "chapterNumber",
        content["container"]["placement"]["chapterNumber"].clone(),
    );
    let language = content["_preferredLanguage"].as_str().unwrap_or("中文");
    let mut abstracts = items(&doc["abstracts"])
        .iter()
        .enumerate()
        .filter(|(_, a)| !items(&a["segments"]).is_empty())
        .collect::<Vec<_>>();
    abstracts.sort_by_key(|(i, a)| {
        (
            !items(&a["languages"]).iter().any(|l| {
                l.as_str()
                    .is_some_and(|v| language_code(v) == language_code(language))
            }),
            a["completeness"] != "complete",
            *i,
        )
    });
    let selection = content.get("_abstractSelection").filter(|v| !v.is_null());
    let selected = if let Some(s) = selection {
        items(&doc["abstracts"])
            .iter()
            .enumerate()
            .find(|(_, a)| *a == s)
    } else {
        abstracts.first().copied()
    };
    let abstract_text = selected.map(|(_, a)| {
        items(&a["segments"])
            .iter()
            .map(|s| {
                format!(
                    "{}{}{}",
                    s["heading"].as_str().unwrap_or(""),
                    if s["heading"].is_null() { "" } else { "\n" },
                    s["text"].as_str().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    });
    let (abstract_text, abstract_source) = if selection.is_some() {
        (
            json!(abstract_text),
            if selected.is_some() {
                "人工选择的原文摘要"
            } else {
                "所选摘要待核对"
            }
            .into(),
        )
    } else {
        choose(
            content,
            "/document/abstracts",
            "abstract",
            json!(abstract_text),
        )
    };
    let ids = items(&doc["identifiers"]);
    let doi = ids
        .iter()
        .filter(|id| id["type"] == "doi")
        .collect::<Vec<_>>();
    let (doi, doi_source) = choose(
        content,
        "/document/identifiers",
        "doi",
        if doi.len() == 1 {
            doi[0]["value"].clone()
        } else {
            Value::Null
        },
    );
    // This legacy column is only a current-document publication event. Display year is separate.
    let publication_year =
        if year_label == "出版" || year_label == "印刷出版" || year_label == "在线发表" {
            year
        } else {
            None
        };
    json!({"title":title,"titleSource":title_source,"authors":authors,"authorsSource":authors_source,"year":year,"yearLabel":year_label,"publicationYear":publication_year,"venue":venue,"venueSource":venue_source,"chapterNumber":chapter,"chapterSource":chapter_source,"abstract":abstract_text,"abstractSource":abstract_source,"abstractIndex":selected.map(|(i,_)|i),"doi":doi,"doiSource":doi_source})
}
pub(crate) fn generated(
    kind: DocumentArtifactKind,
    model: Value,
    previous: Option<&Value>,
) -> Value {
    match kind {
        DocumentArtifactKind::Glossary | DocumentArtifactKind::SymbolTable => {
            generate_table(model, previous)
        }
        DocumentArtifactKind::Metadata => generate_metadata(model, previous),
        _ => model,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample() -> Value {
        serde_json::from_str::<Value>(include_str!("../tests/fixtures/auxiliary-v2.metadata.json"))
            .unwrap()["metadata"]
            .clone()
    }

    #[test]
    fn legacy_overrides_only_attach_to_a_unique_meaning_and_survive_regeneration() {
        let raw = json!({"entries":[{"term":"X","definition":"概念甲","aliases":[]},{"term":"X","definition":"概念乙","aliases":[]}]});
        let unresolved =
            apply_legacy_table_overrides(&raw, &json!({"X":{"definition":"旧人工说明"}}));
        assert_eq!(unresolved["entries"], raw["entries"]);
        assert_eq!(
            unresolved["_unmatchedOverrides"].as_array().unwrap().len(),
            1
        );
        let unique = json!({"entries":[{"term":"X","definition":"概念甲","aliases":[]}]});
        let applied =
            apply_legacy_table_overrides(&unique, &json!({"X":{"definition":"旧人工说明"}}));
        let regenerated = generate_table(unique["entries"].clone(), Some(&applied));
        assert_eq!(regenerated["entries"][0]["definition"], "旧人工说明");
        assert_eq!(regenerated["_model"], unique["entries"]);
    }
    #[test]
    fn modified_abstract_invalidates_gap_links_and_manual_ids_stay_stable() {
        let old = generate_metadata(sample(), None);
        let mut incoming = old.clone();
        incoming["document"]["abstracts"][0]["segments"][0]["text"] = json!("人工补充的段落");
        let edited = edit_metadata(
            &old,
            &incoming,
            vec!["/document/abstracts/0/segments/0/text".into()],
            "old",
        );
        assert_eq!(edited["_effectiveIssues"], json!([]));
        assert_eq!(edited["_issueLinks"], json!([false]));
        assert_eq!(edited["_model"], old["_model"]);
        assert_eq!(
            edited["_identities"]["/document/abstracts/0"]["id"],
            old["_identities"]["/document/abstracts/0"]["id"]
        );
    }
    #[test]
    fn newer_pdf_never_silently_inherits_old_manual_facts() {
        let old = generate_metadata(sample(), None);
        let mut incoming = old.clone();
        incoming["document"]["title"] = json!("旧 PDF 人工题名");
        let mut inherited = edit_metadata(&old, &incoming, vec!["/document/title".into()], "old");
        inherited["_inheritedFromRevision"] = json!("old-revision");
        let new = generate_metadata(sample(), Some(&inherited));
        assert_eq!(new["document"]["title"], "Inference");
        assert_eq!(new["_unmatchedManual"].as_array().unwrap().len(), 1);
    }
    #[test]
    fn homonyms_can_be_edited_deleted_and_regenerated_independently() {
        let rows = json!([{"symbol":"x","meaning":"向量","scope":"第一节","sources":[]},{"symbol":"x","meaning":"标量","scope":"第二节","sources":[]}]);
        let old = generate_table(rows.clone(), None);
        let mut edited = old["entries"].as_array().unwrap().clone();
        let id = edited[1]["_id"].as_str().unwrap().to_string();
        edited[1]["meaning"] = json!("人工核对的标量");
        edited.remove(0);
        let next = edit_table(&old, edited, vec![id.clone()]);
        let refreshed = generate_table(rows.clone(), Some(&next));
        assert_eq!(refreshed["entries"].as_array().unwrap().len(), 1);
        assert_eq!(refreshed["entries"][0]["meaning"], "人工核对的标量");
        assert_eq!(refreshed["entries"][0]["_id"], id);
        assert_eq!(old["_model"], rows);
        assert_eq!(refreshed["_model"], rows);
    }
    #[test]
    fn manual_array_item_follows_exact_original_and_never_a_reused_name() {
        let mut model = sample();
        let first = model["document"]["contributors"][0].clone();
        let second = json!({"name":"B. Writer","role":"author","roleLabel":null,"order":2});
        model["document"]["contributors"] = json!([first, second]);
        let old = generate_metadata(model.clone(), None);
        let mut incoming = old.clone();
        incoming["document"]["contributors"][0]["name"] = json!("人工署名");
        let edited = edit_metadata(
            &old,
            &incoming,
            vec!["/document/contributors/0/name".into()],
            "artifact-1",
        );
        model["document"]["contributors"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        let next = generate_metadata(model.clone(), Some(&edited));
        assert_eq!(next["document"]["contributors"][1]["name"], "人工署名");
        assert_eq!(next["_model"], model);
        model["document"]["contributors"][1]["role"] = json!("editor");
        let unmatched = generate_metadata(model.clone(), Some(&edited));
        assert_eq!(
            unmatched["document"]["contributors"][1]["name"],
            "A. Reader"
        );
        assert_eq!(unmatched["_unmatchedManual"].as_array().unwrap().len(), 1);
    }
    #[test]
    fn year_uses_current_event_then_parent_and_never_copyright_or_ambiguous_fallback() {
        let mut model = sample();
        let content = generate_metadata(model.clone(), None);
        assert_eq!(content["_display"]["year"], 2024);
        assert_eq!(content["_display"]["publicationYear"], Value::Null);
        model["document"]["dates"] =
            json!([{"event":"copyright","eventLabel":null,"dateText":"2025","value":"2025"}]);
        assert_eq!(
            generate_metadata(model.clone(), None)["_display"]["year"],
            2024
        );
        model["document"]["dates"] = json!([{"event":"publication","eventLabel":null,"dateText":"2025","value":"2025"},{"event":"publication","eventLabel":null,"dateText":"2026","value":"2026"}]);
        assert_eq!(
            generate_metadata(model.clone(), None)["_display"]["year"],
            Value::Null
        );
    }
    #[test]
    fn explicit_clear_survives_regeneration_and_legacy_values_remain_labeled() {
        let model = sample();
        let legacy =
            json!({"title":"Old title","publicationYear":1999,"_pinned":["publicationYear"]});
        let old = generate_metadata(model.clone(), Some(&legacy));
        assert_eq!(old["_display"]["year"], 1999);
        assert_eq!(old["document"]["dates"], json!([]));
        let mut input = old.clone();
        input["document"]["title"] = Value::Null;
        let edited = edit_metadata(&old, &input, vec!["/document/title".into()], "original");
        let next = generate_metadata(model, Some(&edited));
        assert_eq!(next["document"]["title"], Value::Null);
        assert_eq!(next["_display"]["title"], Value::Null);
        assert_eq!(next["_display"]["titleSource"], "人工修改");
    }
}
