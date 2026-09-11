use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoadmapProgressRow {
    pub task_id: String,
    pub completed: bool,
    pub completed_at: Option<String>,
}

pub fn create_tables(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS roadmap_progress (
            paper_id     TEXT NOT NULL,
            roadmap_id   TEXT NOT NULL,
            task_id      TEXT NOT NULL,
            completed    INTEGER NOT NULL DEFAULT 0,
            completed_at TEXT,
            PRIMARY KEY (paper_id, roadmap_id, task_id)
        );",
    )?;
    Ok(())
}

pub fn set_task_progress(
    conn: &Connection,
    paper_id: &str,
    roadmap_id: &str,
    task_id: &str,
    completed: bool,
) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO roadmap_progress (paper_id, roadmap_id, task_id, completed, completed_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(paper_id, roadmap_id, task_id)
         DO UPDATE SET completed = ?4, completed_at = ?5",
        params![
            paper_id,
            roadmap_id,
            task_id,
            completed as i32,
            if completed { Some(now.as_str()) } else { None },
        ],
    )?;
    Ok(())
}

pub fn list_progress(
    conn: &Connection,
    paper_id: &str,
    roadmap_id: &str,
) -> rusqlite::Result<Vec<RoadmapProgressRow>> {
    let mut stmt = conn.prepare(
        "SELECT task_id, completed, completed_at
         FROM roadmap_progress
         WHERE paper_id = ?1 AND roadmap_id = ?2",
    )?;
    let rows = stmt.query_map(params![paper_id, roadmap_id], |row| {
        Ok(RoadmapProgressRow {
            task_id: row.get(0)?,
            completed: row.get::<_, i32>(1)? != 0,
            completed_at: row.get(2)?,
        })
    })?;
    let mut result = Vec::new();
    for row in rows {
        result.push(row?);
    }
    Ok(result)
}

pub fn clear_progress(
    conn: &Connection,
    paper_id: &str,
    roadmap_id: &str,
) -> rusqlite::Result<usize> {
    conn.execute(
        "DELETE FROM roadmap_progress WHERE paper_id = ?1 AND roadmap_id = ?2",
        params![paper_id, roadmap_id],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roadmap_progress_lifecycle() {
        let conn = Connection::open_in_memory().unwrap();
        create_tables(&conn).unwrap();

        let initial = list_progress(&conn, "p1", "r1").unwrap();
        assert!(initial.is_empty());

        set_task_progress(&conn, "p1", "r1", "task-1", true).unwrap();
        let list1 = list_progress(&conn, "p1", "r1").unwrap();
        assert_eq!(list1.len(), 1);
        assert_eq!(list1[0].task_id, "task-1");
        assert!(list1[0].completed);
        assert!(list1[0].completed_at.is_some());

        // Toggle back to false
        set_task_progress(&conn, "p1", "r1", "task-1", false).unwrap();
        let list2 = list_progress(&conn, "p1", "r1").unwrap();
        assert_eq!(list2.len(), 1);
        assert!(!list2[0].completed);
        assert!(list2[0].completed_at.is_none());

        // Clear progress
        clear_progress(&conn, "p1", "r1").unwrap();
        let list3 = list_progress(&conn, "p1", "r1").unwrap();
        assert!(list3.is_empty());
    }
}

pub(crate) fn response_schema(kind: crate::library_paths::DocumentKind) -> Value {
    let one_chart = json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["label", "page", "reason"],
        "properties": {
            "label": {"type": "string", "minLength": 1},
            "page": {"type": "integer", "minimum": 1},
            "blockId": {"type": "string"},
            "reason": {"type": "string", "minLength": 1}
        }
    });
    let passes = json!({
        "type": "array",
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["passNumber", "title", "subtitle", "timeBudget", "exitCriteria", "tasks"],
            "properties": {
                "passNumber": {"type": "integer", "enum": [0, 1, 2, 3]},
                "title": {"type": "string", "minLength": 1},
                "subtitle": {"type": "string"},
                "timeBudget": {"type": "string"},
                "exitCriteria": {"type": "string"},
                "tasks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["id", "text", "timeMinutes", "required", "completionCriteria", "selfCheckQuestions", "evidence"],
                        "properties": {
                            "id": {"type": "string", "minLength": 1},
                            "text": {"type": "string", "minLength": 1},
                            "timeMinutes": {"type": "integer", "minimum": 1},
                            "required": {"type": "boolean"},
                            "completionCriteria": {"type": "string"},
                            "selfCheckQuestions": {"type": "array", "items": {"type": "string"}},
                            "evidence": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "additionalProperties": false,
                                    "required": ["label", "page"],
                                    "properties": {
                                        "label": {"type": "string", "minLength": 1},
                                        "page": {"type": "integer", "minimum": 1},
                                        "blockId": {"type": "string"}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    match kind {
        crate::library_paths::DocumentKind::Textbook => json!({
            "name": "reading_roadmap",
            "strict": true,
            "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["version", "paperTitle", "passes"],
                "properties": {
                    "version": {"type": "integer", "enum": [1]},
                    "paperTitle": {"type": "string", "minLength": 1},
                    "learningObjectives": {"type": "array", "items": {"type": "string"}},
                    "prerequisites": {"type": "array", "items": {"type": "string"}},
                    "elevatorPitch": {"type": "string"},
                    "oneChart": one_chart,
                    "passes": passes
                }
            }
        }),
        crate::library_paths::DocumentKind::Paper => json!({
            "name": "reading_roadmap",
            "strict": true,
            "schema": {
                "type": "object",
                "additionalProperties": false,
                "required": ["version", "paperTitle", "passes"],
                "properties": {
                    "version": {"type": "integer", "enum": [1]},
                    "paperTitle": {"type": "string", "minLength": 1},
                    "elevatorPitch": {"type": "string"},
                    "oneChart": one_chart,
                    "passes": passes
                }
            }
        }),
    }
}

pub(crate) fn parse_json(text: &str) -> Result<Value, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("the model returned an empty response".to_string());
    }
    if let Ok(parsed) = serde_json::from_str::<Value>(trimmed) {
        return Ok(parsed);
    }
    let bytes = trimmed.as_bytes();
    let start = bytes
        .iter()
        .position(|byte| *byte == b'{' || *byte == b'[')
        .ok_or_else(|| {
            let preview: String = trimmed.chars().take(80).collect();
            format!(
                "the model response does not start with a JSON object (starts with: {preview:?})"
            )
        })?;
    let open = bytes[start];
    let close = if open == b'{' { b'}' } else { b']' };
    let end = bytes
        .iter()
        .rposition(|byte| *byte == close)
        .filter(|end| *end >= start)
        .ok_or_else(|| "the model response has an incomplete JSON value".to_string())?;
    serde_json::from_str(&trimmed[start..=end])
        .map_err(|error| format!("the model response is not valid JSON: {error}"))
}

// Read every present field (including optional values), before any output is published.
fn validate_schema(value: &Value, schema: &Value, path: &str) -> Result<(), String> {
    let valid = match schema["type"].as_str() {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        Some("integer") => value.is_i64() || value.is_u64(),
        Some("boolean") => value.is_boolean(),
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
    if schema["minLength"].as_u64().unwrap_or(0) > 0
        && value.as_str().is_some_and(|s| s.trim().is_empty())
    {
        return Err(format!("{path}: empty text"));
    }
    if let Some(fields) = value.as_object() {
        let props = schema["properties"]
            .as_object()
            .ok_or("Missing schema properties")?;
        for required in schema["required"]
            .as_array()
            .ok_or("Missing schema required fields")?
        {
            let key = required.as_str().ok_or("Invalid schema required field")?;
            if !fields.contains_key(key) {
                return Err(format!("{path}/{key}: missing field"));
            }
        }
        for (key, item) in fields {
            let child = props
                .get(key)
                .ok_or_else(|| format!("{path}/{key}: unexpected field"))?;
            validate_schema(item, child, &format!("{path}/{key}"))?;
        }
    }
    if let Some(items) = value.as_array() {
        for (i, item) in items.iter().enumerate() {
            validate_schema(item, &schema["items"], &format!("{path}/{i}"))?;
        }
    }
    Ok(())
}

pub(crate) fn validate(
    content: &Value,
    schema: &Value,
    page_count: Option<i64>,
) -> Result<(), String> {
    validate_schema(content, &schema["schema"], "roadmap")?;
    let mut passes = std::collections::HashSet::new();
    let mut ids = std::collections::HashSet::new();
    let location = |value: &Value| -> Result<(), String> {
        let page = value["page"].as_i64().ok_or("Invalid roadmap page")?;
        if page < 1 || page_count.filter(|n| *n > 0).is_some_and(|max| page > max) {
            return Err("Roadmap page is outside the PDF".into());
        }
        // This generator supplies the native PDF, but no OCR catalog.
        if value.get("blockId").is_some() {
            return Err("Roadmap has no supplied OCR whitelist; omit blockId".into());
        }
        Ok(())
    };
    for pass in content["passes"]
        .as_array()
        .ok_or("Missing roadmap passes")?
    {
        if !passes.insert(pass["passNumber"].as_i64()) {
            return Err("Duplicate roadmap pass number".into());
        }
        for task in pass["tasks"].as_array().ok_or("Missing roadmap tasks")? {
            let id = task["id"].as_str().ok_or("Invalid roadmap task id")?;
            if !ids.insert(id.trim()) {
                return Err("Duplicate roadmap task id".into());
            }
            for evidence in task["evidence"]
                .as_array()
                .ok_or("Invalid roadmap evidence")?
            {
                location(evidence)?;
            }
        }
    }
    if let Some(object) = content.get("oneChart") {
        location(object)?;
    }
    Ok(())
}

pub(crate) fn task_input(brief: Option<&Value>, reader: Option<&str>, language: &str) -> String {
    let mut input = json!({
        "task": "为当前文档设计导师式精读路线，指导读者按合理顺序亲自阅读原文，逐步深入。",
        "outputLanguage": language,
        "locatorPolicy": "本次没有提供 OCR 定位目录；省略所有 blockId，仅使用可确认的 PDF 物理页码。"
    });
    if let Some(brief) = brief {
        input["briefSource"] = brief.clone();
    }
    crate::reader_context::prepend_reader_context(&input.to_string(), reader)
}

pub(crate) struct GenerationRequest {
    pub(crate) job_id: String,
    pub(crate) facts: crate::reading_artifact_module::DocumentFacts,
    pub(crate) page_count: Option<i64>,
    pub(crate) provider: String,
    pub(crate) route_id: String,
    pub(crate) model: String,
    pub(crate) call: crate::reading_artifact_module::PaperRootCall,
}

pub(crate) async fn generate(
    workspace_root: &std::path::Path,
    port: &dyn crate::provider_ports::PaperModelPort,
    jobs: &crate::job_module::JobModule,
    request: GenerationRequest,
) -> Result<crate::artifact_module::ArtifactProjection, String> {
    use crate::artifact_module::{ArtifactDraft, ArtifactModule};
    use crate::reading_artifact_module::{ModelCall, ReadingArtifactModule};
    let database_path = workspace_root.join(".read-desktop/workspace.sqlite3");
    let existing: Option<String> = crate::db::open(&database_path)
        .map_err(|e| e.to_string())?
        .query_row(
            "SELECT id FROM artifacts WHERE revision_id=?1 AND kind='reading_roadmap'
         AND json_extract(dependency_snapshot_json, '$.jobId')=?2 LIMIT 1",
            params![request.facts.revision_id, request.job_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(id) = existing {
        return ArtifactModule::open(&database_path)?.get(&id);
    }
    let module = ReadingArtifactModule::open(workspace_root)?;
    let schema = request.call.response_schema.clone();
    let mut checkpoint = jobs.get_checkpoint(&request.job_id)?.unwrap_or(json!({}));
    if checkpoint.get("response").is_none() {
        let response = module
            .call_from_paper_root(port, &request.facts, &request.route_id, request.call)
            .await
            .map_err(|e| e.to_string())?;
        checkpoint = json!({"response": response});
        jobs.save_checkpoint(&request.job_id, "response_received", &checkpoint)?;
    }
    let response: ModelCall =
        serde_json::from_value(checkpoint["response"].clone()).map_err(|e| e.to_string())?;
    let database_path = workspace_root.join(".read-desktop/workspace.sqlite3");
    let connection = crate::db::open(&database_path).map_err(|e| e.to_string())?;
    let recorded: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM usage_receipts WHERE operation_id=?1 AND provider_route_id=?2)",
        params![request.job_id, request.route_id], |row| row.get(0)
    ).map_err(|e| e.to_string())?;
    if !recorded {
        for receipt in &response.receipts {
            module
                .record_usage_scoped(&request.job_id, &request.route_id, receipt)
                .map_err(|e| e.to_string())?;
        }
    }
    connection.execute(
        "UPDATE usage_receipts SET job_id=?1 WHERE operation_id=?1 AND provider_route_id=?2 AND job_id IS NULL",
        params![request.job_id, request.route_id]
    ).map_err(|e| e.to_string())?;
    let content = parse_json(&response.text)?;
    validate(&content, &schema, request.page_count)?;
    if checkpoint.get("providerNodeId").is_none() {
        let node = module
            .store_provider_node_for_route(
                &request.route_id,
                &request.provider,
                &request.model,
                &response.context_epoch,
                &response.remote_node_id,
                response.parent_local_node_id.as_deref(),
                "complete",
            )
            .map_err(|e| e.to_string())?;
        checkpoint["providerNodeId"] = json!(node);
        jobs.save_checkpoint(&request.job_id, "validated", &checkpoint)?;
    }
    ArtifactModule::open(&database_path)?.publish(ArtifactDraft {
        paper_id: request.facts.paper_id,
        revision_id: request.facts.revision_id,
        ocr_revision_id: None,
        kind: "reading_roadmap".into(),
        object_key: "roadmap".into(),
        content,
        evidence: Vec::new(),
        dependency_snapshot: json!({"provider": request.provider, "providerRouteId": request.route_id,
            "model": request.model, "jobId": request.job_id, "contextEpoch": response.context_epoch}),
        provider_node_id: checkpoint["providerNodeId"].as_str().map(str::to_owned),
    })
}

#[cfg(test)]
mod contract_tests {
    use super::*;
    use crate::library_paths::DocumentKind;

    fn content() -> Value {
        json!({"version":1,"paperTitle":"定理阅读", "passes":[{
            "passNumber":1,"title":"建立方向","subtitle":"先辨认对象","timeBudget":"约 10 分钟",
            "exitCriteria":"能指出前提与结论", "tasks":[{
                "id":"p1-t1","text":"先读 $x > 0$ 的条件。\n\n再看定理。","timeMinutes":5,"required":true,
                "completionCriteria":"能辨认前提","selfCheckQuestions":[],"evidence":[{"label":"定理 1","page":2}]
            }]
        }]})
    }

    #[test]
    fn roadmap_validation_checks_optional_fields_ids_pages_and_preserves_content() {
        let schema = response_schema(DocumentKind::Paper);
        let base = content();
        validate(&base, &schema, Some(8)).unwrap();
        let mut with_object = base.clone();
        with_object["oneChart"] =
            json!({"label":"定理 1","page":2,"reason":"先读条件，再返回核对证明"});
        with_object["elevatorPitch"] = json!("用自己的话解释条件和结论之间的联系");
        validate(&with_object, &schema, Some(8)).unwrap();
        let raw = with_object.to_string();
        assert_eq!(parse_json(&raw).unwrap(), with_object);
        for (path, value) in [
            ("/version", json!(2)),
            ("/paperTitle", json!("  ")),
            ("/passes/0/tasks/0/required", json!("true")),
            ("/passes/0/tasks/0/timeMinutes", json!(0)),
            ("/passes/0/tasks/0/selfCheckQuestions", Value::Null),
            ("/passes/0/tasks/0/evidence/0/page", json!(9)),
            ("/passes/0/tasks/0/evidence/0/page", json!(0)),
            ("/oneChart", Value::Null),
            ("/oneChart/page", json!(9)),
            ("/oneChart/reason", json!([])),
            ("/elevatorPitch", json!(3)),
        ] {
            let mut bad = with_object.clone();
            *bad.pointer_mut(path).unwrap() = value;
            assert!(validate(&bad, &schema, Some(8)).is_err(), "{path}");
        }
        let mut bad = base.clone();
        bad["passes"][0]["tasks"][0]["evidence"][0]["blockId"] = json!("invented");
        assert!(validate(&bad, &schema, Some(8)).is_err());
        bad = with_object.clone();
        bad["oneChart"]["blockId"] = json!("invented");
        assert!(validate(&bad, &schema, Some(8)).is_err());
        bad = base.clone();
        bad["passes"][0]["tasks"]
            .as_array_mut()
            .unwrap()
            .push(base["passes"][0]["tasks"][0].clone());
        assert!(validate(&bad, &schema, Some(8)).is_err());
        bad = base.clone();
        bad["passes"]
            .as_array_mut()
            .unwrap()
            .push(base["passes"][0].clone());
        assert!(validate(&bad, &schema, Some(8)).is_err());
        bad = base.clone();
        bad["answers"] = json!([]);
        assert!(validate(&bad, &schema, Some(8)).is_err());
        let unavailable = json!({"version":1,"paperTitle":"当前文档","passes":[{
            "passNumber":0,"title":"暂无法制定可靠路线","subtitle":"缺少可辨认的正文",
            "timeBudget":"暂不估时","exitCriteria":"需要可读的正文","tasks":[]
        }]});
        validate(&unavailable, &schema, None).unwrap();
    }

    #[test]
    fn roadmap_input_keeps_reader_and_brief_as_optional_task_materials() {
        let input = task_input(None, None, "zh-CN");
        let value: Value = serde_json::from_str(&input).unwrap();
        assert!(value.get("briefSource").is_none());
        assert_eq!(value["outputLanguage"], "zh-CN");
        let brief = json!({"artifactId":"brief-1","content":{"takeaway":"FROZEN_BRIEF"}});
        let input = task_input(Some(&brief), Some("FROZEN_READER"), "zh-CN");
        assert!(input.contains("FROZEN_BRIEF"));
        assert!(input.contains("FROZEN_READER"));
        assert!(input.contains("省略所有 blockId"));
    }
}
