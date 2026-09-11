//! Single-output document artifacts. Legacy Orientation Pack jobs keep their old handler.
use super::*;
use crate::reading_artifact_module::{DocumentFacts, ReadingArtifactModule};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DocumentArtifactKind {
    Brief,
    Glossary,
    SymbolTable,
    Metadata,
}

impl DocumentArtifactKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Brief => "brief",
            Self::Glossary => "glossary",
            Self::SymbolTable => "symbol_table",
            Self::Metadata => "metadata",
        }
    }

    pub(crate) fn slot(self) -> PromptSlotId {
        match self {
            Self::Brief => PromptSlotId::OrientationPack,
            Self::Glossary => PromptSlotId::Glossary,
            Self::SymbolTable => PromptSlotId::SymbolTable,
            Self::Metadata => PromptSlotId::Metadata,
        }
    }

    fn response_key(self) -> &'static str {
        if self == Self::SymbolTable {
            "symbolTable"
        } else {
            self.as_str()
        }
    }
}

const BRIEF_FIELDS: [&str; 9] = [
    "takeaway",
    "keywords",
    "classification",
    "context",
    "backgroundAndProblem",
    "coreMethod",
    "findings",
    "evaluation",
    "futureWork",
];

pub(crate) fn response_schema(kind: DocumentArtifactKind, textbook: bool) -> Value {
    let string = json!({"type": "string"});
    let nonempty = json!({"type": "string", "minLength": 1});
    let strings = json!({"type": "array", "items": string});
    let body = match kind {
        DocumentArtifactKind::Brief => {
            let mut properties = serde_json::Map::new();
            for key in BRIEF_FIELDS {
                properties.insert(
                    key.to_string(),
                    if key == "keywords" {
                        json!({"type": "array", "minItems": 1, "items": nonempty})
                    } else {
                        nonempty.clone()
                    },
                );
            }
            json!({"type": "object", "additionalProperties": false,
                "required": BRIEF_FIELDS, "properties": properties})
        }
        DocumentArtifactKind::Glossary => json!({"type": "array", "items": {
            "type": "object", "additionalProperties": false,
            "required": ["term", "definition", "aliases"],
            "properties": {"term": nonempty, "definition": nonempty, "aliases": strings}
        }}),
        DocumentArtifactKind::SymbolTable => json!({"type": "array", "items": {
            "type": "object", "additionalProperties": false,
            "required": ["symbol", "meaning", "scope"],
            "properties": {"symbol": nonempty, "meaning": nonempty, "scope": string}
        }}),
        DocumentArtifactKind::Metadata => {
            let mut properties = serde_json::Map::new();
            let fields = if textbook {
                vec!["title", "bookName", "chapterNumber", "isbn", "abstract"]
            } else {
                vec!["title", "venue", "doi", "abstract"]
            };
            for key in fields {
                properties.insert(key.to_string(), string.clone());
            }
            properties.insert("authors".to_string(), strings);
            properties.insert(
                "publicationYear".to_string(),
                json!({"type": ["integer", "null"]}),
            );
            let required: Vec<_> = properties.keys().cloned().collect();
            json!({"type": "object", "additionalProperties": false,
                "required": required, "properties": properties})
        }
    };
    let key = kind.response_key();
    json!({"name": format!("document_{}_v1", kind.as_str()), "strict": true,
        "schema": {"type": "object", "additionalProperties": false,
            "required": [key], "properties": {key: body}}})
}

// Checks precisely the vocabulary emitted by response_schema, including local
// non-whitespace checks. Provider-side strict mode alone is not validation.
fn validate(value: &Value, schema: &Value, path: &str) -> AppResult<()> {
    let valid = match schema["type"].as_str() {
        Some("object") => value.is_object(),
        Some("array") => value.is_array(),
        Some("string") => value.is_string(),
        _ => schema["type"] == json!(["integer", "null"]) && (value.is_i64() || value.is_null()),
    };
    if !valid {
        return Err(format!("{path}: invalid value type"));
    }
    if let Some(object) = value.as_object() {
        let properties = schema["properties"]
            .as_object()
            .ok_or("Missing schema properties")?;
        if object.keys().any(|key| !properties.contains_key(key)) {
            return Err(format!("{path}: unexpected field"));
        }
        for key in schema["required"]
            .as_array()
            .ok_or("Missing required fields")?
        {
            let key = key.as_str().ok_or("Invalid required field")?;
            validate(
                object
                    .get(key)
                    .ok_or_else(|| format!("{path}.{key}: missing field"))?,
                &properties[key],
                &format!("{path}.{key}"),
            )?;
        }
    }
    if let Some(items) = value.as_array() {
        if items.len() < schema["minItems"].as_u64().unwrap_or(0) as usize {
            return Err(format!("{path}: empty array"));
        }
        for (index, item) in items.iter().enumerate() {
            validate(item, &schema["items"], &format!("{path}[{index}]"))?;
        }
    }
    if schema["minLength"].as_u64().unwrap_or(0) > 0
        && value.as_str().is_some_and(|text| text.trim().is_empty())
    {
        return Err(format!("{path}: empty text"));
    }
    Ok(())
}

#[cfg(test)]
fn parse_output(raw: &str, kind: DocumentArtifactKind, textbook: bool) -> AppResult<Value> {
    let value: Value =
        serde_json::from_str(raw.trim()).map_err(|error| format!("独立成果 JSON 无效：{error}"))?;
    validate(&value, &response_schema(kind, textbook)["schema"], "$响应")?;
    Ok(value[kind.response_key()].clone())
}

pub(crate) fn brief_hints(artifact: &ArtifactProjection) -> Value {
    let mut hints = serde_json::Map::new();
    let keys = if artifact.content["briefProtocol"].as_str()
        == Some(crate::textbook_contract::BRIEF_PROTOCOL)
    {
        ["takeaway", "keywords", "learningScope", "motivation"]
    } else {
        [
            "takeaway",
            "keywords",
            "classification",
            "backgroundAndProblem",
        ]
    };
    for key in keys {
        if let Some(value) = artifact.content.get(key) {
            hints.insert(key.to_string(), value.clone());
        }
    }
    json!({"artifactId": artifact.id, "version": artifact.version,
        "revisionId": artifact.revision_id, "hints": hints})
}

fn task_input(kind: DocumentArtifactKind, textbook: bool, hints: Option<&Value>) -> String {
    let mut input = json!({"task": format!("直接依据完整 PDF，只生成 {}。", kind.response_key()),
        "documentKind": if textbook { "textbook" } else { "paper" }});
    if kind == DocumentArtifactKind::Glossary {
        if let Some(hints) = hints {
            input["briefTopicHints"] = hints.clone();
        }
    }
    input.to_string()
}

#[cfg(test)]
fn merge_pinned_entries(entries: Value, previous: Option<&Value>, key: &str) -> Value {
    let pinned = previous
        .and_then(|p| p.get("_pinned"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let old = previous
        .and_then(|p| p.get("entries"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut merged = entries.as_array().cloned().unwrap_or_default();
    for entry in old {
        if !pinned.contains(&entry[key]) {
            continue;
        }
        if let Some(index) = merged.iter().position(|new| new[key] == entry[key]) {
            merged[index] = entry;
        } else {
            merged.push(entry);
        }
    }
    json!({"entries": merged, "_pinned": pinned})
}

fn merge_pinned_metadata(
    connection: &Connection,
    revision_id: &str,
    mut metadata: Value,
) -> AppResult<Value> {
    let previous = connection
        .query_row(
            "SELECT title, authors_json, publication_year, venue, doi, abstract_text, metadata_json
         FROM paper_metadata WHERE revision_id = ?1",
            params![revision_id],
            |row| {
                let raw: String = row.get(6)?;
                let mut previous: Value = serde_json::from_str(&raw).unwrap_or(json!({}));
                if !previous.is_object() {
                    previous = json!({});
                }
                previous["title"] = json!(row.get::<_, String>(0)?);
                previous["authors"] =
                    serde_json::from_str(&row.get::<_, String>(1)?).unwrap_or(json!([]));
                previous["publicationYear"] = json!(row.get::<_, Option<i64>>(2)?);
                for (key, index) in [("venue", 3), ("doi", 4), ("abstract", 5)] {
                    previous[key] = json!(row.get::<_, Option<String>>(index)?.unwrap_or_default());
                }
                Ok(previous)
            },
        )
        .optional()
        .map_err(|error| error.to_string())?;
    if let Some(previous) = previous {
        if previous.get("_inheritedFromRevision").is_some() {
            metadata["_previousRevisionRecord"] = previous;
            return Ok(metadata);
        }
        if let Some(pinned) = previous["_pinned"].as_array() {
            for key in pinned.iter().filter_map(Value::as_str) {
                if let Some(value) = previous.get(key) {
                    metadata[key] = value.clone();
                }
            }
            metadata["_pinned"] = json!(pinned);
        }
    }
    Ok(metadata)
}

pub(crate) fn sync_metadata(
    connection: &Connection,
    revision: &RevisionRecord,
    metadata: &Value,
) -> AppResult<()> {
    let projection = metadata.get("_display").unwrap_or(metadata);
    let title = projection["title"].as_str().unwrap_or("");
    connection
        .execute(
            "UPDATE paper_metadata SET title = ?1, authors_json = ?2, publication_year = ?3,
            venue = ?4, doi = ?5, abstract_text = ?6, metadata_json = ?7, source = ?9
         WHERE revision_id = ?8",
            params![
                title,
                projection["authors"].to_string(),
                projection["publicationYear"].as_i64(),
                projection["venue"].as_str(),
                projection["doi"].as_str(),
                projection["abstract"].as_str(),
                metadata.to_string(),
                revision.id,
                if metadata["_manual"]
                    .as_object()
                    .is_some_and(|m| !m.is_empty())
                    || metadata["_pinned"]
                        .as_array()
                        .is_some_and(|p| !p.is_empty())
                {
                    "mixed"
                } else {
                    "model"
                }
            ],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn project_brief(value: Value, revision: &RevisionRecord, model: &str) -> AppResult<Value> {
    let generated: GeneratedBrief =
        serde_json::from_value(value).map_err(|error| error.to_string())?;
    let brief = Brief {
        revision_id: revision.id.clone(),
        version: 1,
        status: "ready".to_string(),
        takeaway: normalize_markdown_field(&generated.takeaway),
        keywords: generated.keywords,
        classification: normalize_markdown_field(&generated.classification),
        context: normalize_markdown_field(&generated.context),
        background_and_problem: normalize_markdown_field(&generated.background_and_problem),
        core_method: normalize_markdown_field(&generated.core_method),
        findings: normalize_markdown_field(&generated.findings),
        evaluation: normalize_markdown_field(&generated.evaluation),
        future_work: normalize_markdown_field(&generated.future_work),
        summary: normalize_markdown_field(&generated.takeaway),
        research_question: normalize_markdown_field(&generated.background_and_problem),
        method: normalize_markdown_field(&generated.core_method),
        limitations: String::new(),
        evidence: Vec::new(),
        model: model.to_string(),
        created_at: now(),
    };
    serde_json::to_value(brief).map_err(|error| error.to_string())
}

enum ExecutionError {
    Local(String),
    Provider(ProviderError),
}
impl From<String> for ExecutionError {
    fn from(message: String) -> Self {
        Self::Local(message)
    }
}
impl From<&str> for ExecutionError {
    fn from(message: &str) -> Self {
        Self::Local(message.to_string())
    }
}
impl From<ProviderError> for ExecutionError {
    fn from(error: ProviderError) -> Self {
        Self::Provider(error)
    }
}

pub(crate) async fn execute(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
) -> Result<(), ProviderError> {
    execute_inner(app, runtime, job, route)
        .await
        .map_err(|error| match error {
            ExecutionError::Local(message) => ProviderError::local_state(message),
            ExecutionError::Provider(error) => error,
        })
}

async fn execute_inner(
    app: &tauri::AppHandle,
    runtime: &Arc<WorkspaceRuntime>,
    job: &JobProjection,
    route: &BoundProviderRoute,
) -> Result<(), ExecutionError> {
    let kind: DocumentArtifactKind =
        serde_json::from_value(job.payload["documentArtifactKind"].clone())
            .map_err(|error| format!("Invalid document artifact kind: {error}"))?;
    let protocol = job.payload["documentArtifactProtocol"]
        .as_str()
        .unwrap_or("");
    if !["v1", "v2"].contains(&protocol) {
        return Err("Unsupported document artifact protocol".into());
    }
    let revision_id = job
        .revision_id
        .as_deref()
        .ok_or("Document artifact has no revision")?;
    let (mut revision, paper_id) = {
        let connection = open_db(&runtime.root)?;
        let revision = revision_record(&connection, revision_id)?;
        let paper_id: String = connection
            .query_row(
                "SELECT paper_id FROM document_revisions WHERE id = ?1",
                params![revision_id],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        (revision, paper_id)
    };
    revision.pdf_path = absolute_pdf(&runtime.root, &revision.pdf_path);
    let model = route.frozen().models().paper().to_string();
    let provider = route.frozen().provider_kind().as_str();
    let route_id = route.frozen().route_id().database_value();
    let textbook = job.payload["prompts"]["documentKind"] == "textbook";
    let textbook_brief = if kind == DocumentArtifactKind::Brief {
        crate::textbook_contract::from_job(&job.payload, textbook)?
    } else {
        false
    };
    let schema = job
        .payload
        .get("responseSchema")
        .cloned()
        .unwrap_or_else(|| {
            if textbook_brief {
                crate::textbook_contract::response_schema()
            } else if protocol == "v2" {
                crate::auxiliary_contract::response_schema(kind)
            } else {
                response_schema(kind, textbook)
            }
        });
    let module = ReadingArtifactModule::open(&runtime.root)?;
    let jobs = &runtime.job_module;
    let mut checkpoint = jobs.get_checkpoint(&job.id)?.unwrap_or(json!({}));
    if checkpoint.get("response").is_none() {
        let cancellation = CancellationFlag::default();
        app.state::<AppState>()
            .artifact_cancellations
            .lock()
            .map_err(|_| "Artifact cancellation lock poisoned")?
            .insert(job.id.clone(), cancellation.clone());
        let adapter = job_paper_model_adapter(
            route.adapter_arc(),
            jobs.clone(),
            job.id.clone(),
            cancellation,
        );
        let facts = DocumentFacts {
            paper_id: paper_id.clone(),
            revision_id: revision.id.clone(),
            revision_sha256: revision.sha256.clone(),
            title: revision.title.clone(),
            pdf_path: revision.pdf_path.clone(),
        };
        let system = frozen_prompt_field(&job.payload, "orientation")?;
        let root_instruction = frozen_prompt_field(&job.payload, "paperRoot")?;
        let mut source = module
            .ensure_root_for_route(
                &adapter,
                &facts,
                &route_id,
                provider,
                &model,
                Some(&root_instruction),
            )
            .await?;
        let mut retried = false;
        let response = loop {
            let request = PaperInteractionRequest {
                model: model.clone(),
                context_epoch: source.context_epoch.clone(),
                pdf_path: revision.pdf_path.clone(),
                display_name: revision.title.clone(),
                remote_file_id: Some(source.remote_file_id.clone()),
                previous_interaction_id: Some(source.remote_node_id.clone()),
                system_instruction: system.clone(),
                user_input: task_input(kind, textbook, job.payload.get("briefSource")),
                response_schema: Some(schema.clone()),
                inline_images: Vec::new(),
                kind: PaperInteractionKind::Artifact,
            };
            match adapter.interact(request).await {
                Ok(response) => break response,
                Err(error) if !retried && error.kind == ProviderErrorKind::StaleRemoteResource => {
                    retried = true;
                    module.invalidate_root_for_route_with_instruction(
                        &facts,
                        &route_id,
                        &model,
                        Some(&root_instruction),
                    )?;
                    source = module
                        .ensure_root_for_route(
                            &adapter,
                            &facts,
                            &route_id,
                            provider,
                            &model,
                            Some(&root_instruction),
                        )
                        .await?;
                }
                Err(error) => return Err(error.into()),
            }
        };
        // Retain the paid response before validation/publication. An explicit job
        // retry can recover locally without paying for the same response again.
        checkpoint = json!({"response": response, "contextEpoch": source.context_epoch,
            "parentNodeId": source.local_node_id});
        jobs.save_checkpoint(&job.id, "response_received", &checkpoint)?;
    }
    let response: PaperInteractionOutcome = serde_json::from_value(checkpoint["response"].clone())
        .map_err(|error| error.to_string())?;
    let epoch = checkpoint["contextEpoch"]
        .as_str()
        .ok_or("Missing response context epoch")?
        .to_string();
    // Receipts are retained even if strict validation fails.
    let recorded: bool = open_db(&runtime.root)?.query_row(
        "SELECT EXISTS(SELECT 1 FROM usage_receipts WHERE operation_id = ?1 AND provider_route_id = ?2)",
        params![job.id, route_id], |row| row.get(0)).map_err(|error| error.to_string())?;
    if !recorded {
        module.record_usage_scoped(&job.id, &route_id, &response.receipt)?;
    }
    open_db(&runtime.root)?.execute(
        "UPDATE usage_receipts SET job_id = ?1 WHERE operation_id = ?1 AND provider_route_id = ?2 AND job_id IS NULL",
        params![job.id, route_id]).map_err(|error| error.to_string())?;
    let mut content = if protocol == "v2" {
        let value: Value = serde_json::from_str(response.text.trim())
            .map_err(|e| format!("独立成果 JSON 无效：{e}"))?;
        crate::auxiliary_contract::validate(&value, &schema["schema"], "$响应")?;
        let body = value[kind.response_key()].clone();
        crate::auxiliary_contract::validate_relations(&body, kind, revision.page_count)?;
        body
    } else {
        let value: Value = serde_json::from_str(response.text.trim()).map_err(|e| e.to_string())?;
        validate(&value, &schema["schema"], "$响应")?;
        value[kind.response_key()].clone()
    };
    if checkpoint.get("providerNodeId").is_none() {
        let node = module.store_provider_node_for_route(
            &route_id,
            provider,
            &model,
            &epoch,
            &response.provider_node_id,
            checkpoint["parentNodeId"].as_str(),
            "complete",
        )?;
        checkpoint["providerNodeId"] = json!(node);
        jobs.save_checkpoint(&job.id, "validated", &checkpoint)?;
    }
    if kind == DocumentArtifactKind::Brief {
        content = if textbook_brief {
            let mut value = crate::textbook_contract::normalized_body(&content)?;
            value["briefProtocol"] = json!(crate::textbook_contract::BRIEF_PROTOCOL);
            value["documentKind"] = json!("textbook");
            value["revisionId"] = json!(revision.id);
            value["summary"] = value["takeaway"].clone();
            value["version"] = json!(1);
            value["status"] = json!("ready");
            value["model"] = json!(model);
            value["createdAt"] = json!(now());
            value
        } else {
            project_brief(content, &revision, &model)?
        };
    }
    let published=runtime.artifact_module.publish_prepared(ArtifactDraft {
        paper_id,revision_id:revision.id.clone(),ocr_revision_id:None,kind:kind.as_str().into(),object_key:String::new(),content,evidence:Vec::new(),
        dependency_snapshot:json!({"jobId":job.id,"jobCreatedAt":job.created_at,"protocol":format!("document-artifact-{protocol}"),"briefProtocol":job.payload.get("briefProtocol"),"documentKind":if textbook{"textbook"}else{"paper"},"revisionId":revision.id,"contextEpoch":epoch,"providerRouteId":route_id,"model":model,"briefSource":if kind==DocumentArtifactKind::Glossary{job.payload.get("briefSource")}else{None}}),
        provider_node_id:checkpoint["providerNodeId"].as_str().map(String::from),
    },|connection,draft| {
        let previous:Option<String>=connection.query_row("SELECT a.content_json FROM artifacts a JOIN artifact_heads h ON h.artifact_id=a.id WHERE a.revision_id=?1 AND a.kind=?2 AND a.object_key=''",params![revision.id,kind.as_str()],|r|r.get(0)).optional().map_err(|e|e.to_string())?;
        let previous=previous.or_else(||if kind==DocumentArtifactKind::Metadata {connection.query_row("SELECT metadata_json FROM paper_metadata WHERE revision_id=?1",params![revision.id],|r|r.get(0)).optional().ok().flatten()}else{None});
        let previous=previous.as_deref().map(serde_json::from_str::<Value>).transpose().map_err(|e|e.to_string())?;
        let previous=previous.as_ref().map(|old|crate::auxiliary_state::load_legacy_table_overrides(connection,&revision.id,kind.as_str(),old)).transpose()?;
        if protocol=="v2" || matches!(kind,DocumentArtifactKind::Glossary|DocumentArtifactKind::SymbolTable) {
            draft.content=crate::auxiliary_state::generated(kind,draft.content.clone(),previous.as_ref());
        } else if kind==DocumentArtifactKind::Metadata {
            let raw=draft.content.clone();
            if let Some(previous)=previous.as_ref().filter(|p|p.get("document").is_some()) {
                let mut retained=previous.clone();retained["_legacy"]=raw.clone();
                retained["_compatModel"]=raw;retained["_compatProtocol"]=json!("v1");
                retained["_display"]=crate::auxiliary_state::display(&retained);draft.content=retained;
            }else{
                draft.content=merge_pinned_metadata(connection,revision_id,draft.content.clone())?;
                draft.content["_model"]=raw;
            }
        }
        if kind==DocumentArtifactKind::Metadata && draft.dependency_snapshot["historicalOnly"]!=true {sync_metadata(connection,&revision,&draft.content)?;}
        Ok(())
    })?;
    checkpoint["artifactId"] = json!(published.id);
    checkpoint["artifactVersion"] = json!(published.version);
    jobs.save_checkpoint(&job.id, "published", &checkpoint)?;
    jobs.complete(&job.id)?;
    emit_job_event_for_runtime(app, runtime, &job.id);
    emit_read_event_for_runtime(
        app,
        runtime,
        ReadEvent {
            cursor: format!("{}:{}", now(), Uuid::new_v4().simple()),
            kind: "paper".to_string(),
            entity_id: Some(revision.id),
            delta: Some(kind.as_str().to_string()),
            status: None,
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brief() -> Value {
        json!({"takeaway": "复现未观察到提升。", "keywords": ["复现"], "classification": "实证复现",
            "context": "前人主张", "backgroundAndProblem": "验证稳健性", "coreMethod": "同设置复现",
            "findings": "未观察到差异", "evaluation": "仅支持所测设置", "futureWork": "原文未提出后续方向。"})
    }

    #[test]
    fn brief_accepts_negative_results_but_rejects_pack_extra_missing_and_empty_fields() {
        let valid = json!({"brief": brief()});
        assert!(parse_output(&valid.to_string(), DocumentArtifactKind::Brief, false).is_ok());
        let mut extra = valid.clone();
        extra["metadata"] = json!({});
        assert!(parse_output(&extra.to_string(), DocumentArtifactKind::Brief, false).is_err());
        let mut missing = valid.clone();
        missing["brief"]
            .as_object_mut()
            .unwrap()
            .remove("evaluation");
        assert!(parse_output(&missing.to_string(), DocumentArtifactKind::Brief, false).is_err());
        let mut empty = valid;
        empty["brief"]["coreMethod"] = json!("  ");
        assert!(parse_output(&empty.to_string(), DocumentArtifactKind::Brief, false).is_err());
    }

    #[test]
    fn independent_schemas_allow_absent_auxiliary_information() {
        for (kind, raw) in [
            (DocumentArtifactKind::Glossary, r#"{"glossary":[]}"#),
            (DocumentArtifactKind::SymbolTable, r#"{"symbolTable":[]}"#),
            (
                DocumentArtifactKind::Metadata,
                r#"{"metadata":{"title":"","authors":[],"publicationYear":null,"venue":"","doi":"","abstract":""}}"#,
            ),
        ] {
            assert!(parse_output(raw, kind, false).is_ok());
            assert!(parse_output(raw, DocumentArtifactKind::Brief, false).is_err());
        }
        assert!(parse_output(r#"{"metadata":{"title":"","authors":[],"publicationYear":null,"venue":"","doi":"","abstract":""}}"#,
            DocumentArtifactKind::Metadata, true).is_err());
    }

    #[test]
    fn only_glossary_receives_explicit_brief_hints() {
        let artifact = ArtifactProjection {
            id: "brief-7".into(),
            paper_id: "p".into(),
            revision_id: "r".into(),
            ocr_revision_id: None,
            kind: "brief".into(),
            object_key: String::new(),
            version: 7,
            status: "ready".into(),
            content: brief(),
            overrides: json!({}),
            evidence: vec![],
            dependency_snapshot: json!({}),
            provider_node_id: None,
            created_at: String::new(),
        };
        let hints = brief_hints(&artifact);
        assert_eq!(hints["version"], 7);
        assert_eq!(hints["artifactId"], "brief-7");
        assert!(hints["hints"].get("evaluation").is_none());
        assert!(hints["hints"].get("futureWork").is_none());
        for kind in [
            DocumentArtifactKind::Brief,
            DocumentArtifactKind::SymbolTable,
            DocumentArtifactKind::Metadata,
        ] {
            assert!(!task_input(kind, false, Some(&hints)).contains("briefTopicHints"));
        }
        assert!(
            task_input(DocumentArtifactKind::Glossary, false, Some(&hints)).contains("brief-7")
        );
        assert!(
            !task_input(DocumentArtifactKind::Glossary, false, None).contains("briefTopicHints")
        );
    }

    #[test]
    fn metadata_regeneration_preserves_pinned_columns_and_textbook_fields() {
        let connection = Connection::open_in_memory().unwrap();
        connection.execute_batch("CREATE TABLE paper_metadata(revision_id TEXT, title TEXT, authors_json TEXT,
            publication_year INTEGER, venue TEXT, doi TEXT, abstract_text TEXT, metadata_json TEXT);").unwrap();
        connection.execute("INSERT INTO paper_metadata VALUES ('r','用户标题','[\"用户作者\"]',2024,'','','',?1)",
            params![json!({"bookName":"用户书名", "_pinned":["title","authors","bookName"]}).to_string()]).unwrap();
        let merged = merge_pinned_metadata(
            &connection,
            "r",
            json!({"title":"模型标题","authors":["模型作者"],
            "bookName":"模型书名","publicationYear":2025}),
        )
        .unwrap();
        assert_eq!(merged["title"], "用户标题");
        assert_eq!(merged["authors"], json!(["用户作者"]));
        assert_eq!(merged["bookName"], "用户书名");
        assert_eq!(merged["publicationYear"], 2025);
    }

    #[test]
    fn regenerating_tables_keeps_replaced_and_omitted_pinned_entries() {
        let previous = json!({"entries": [{"term":"A","definition":"人工 A"},{"term":"B","definition":"人工 B"}], "_pinned":["A","B"]});
        let merged = merge_pinned_entries(
            json!([{"term":"A","definition":"模型 A"},{"term":"C","definition":"模型 C"}]),
            Some(&previous),
            "term",
        );
        assert_eq!(merged["entries"][0]["definition"], "人工 A");
        assert_eq!(merged["entries"][2]["definition"], "人工 B");
        assert_eq!(merged["entries"].as_array().unwrap().len(), 3);
    }
}
