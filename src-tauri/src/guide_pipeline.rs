//! V2 batch execution, shared by the worker and fake-provider regression tests.
use crate::guide_cast::GuideCastSnapshot;
use crate::guide_catalog::GuideBatchPlan;
use crate::guide_generation::{call_once, limited_memo, merge_validated_batch, page_coverage};
use crate::guide_memo::GuideMemo;
use crate::guide_validate::{GuideCatalogBlock, GuideInk};
use crate::provider_ports::{
    PaperInteractionKind, PaperModelPort, ProviderError, TextInteractionOutcome,
    TextInteractionRequest,
};
use serde_json::{json, Value};

pub struct BatchInput<'a> {
    pub batch: &'a GuideBatchPlan,
    pub cast: &'a GuideCastSnapshot,
    pub memo: &'a GuideMemo,
    pub catalog: &'a [GuideCatalogBlock],
    pub model: &'a str,
    pub epoch: &'a str,
    pub prompt: &'a str,
    pub language: &'a str,
    pub reader: Option<&'a str>,
    pub input_limit: usize,
}

pub struct BatchResult {
    pub inks: Vec<GuideInk>,
    pub warnings: Vec<String>,
    pub dropped: i64,
}

pub async fn run_batch<P: FnMut(&Value) -> Result<(), String>>(
    port: &dyn PaperModelPort,
    input: &BatchInput<'_>,
    existing: &[GuideInk],
    checkpoint: &mut Value,
    mut persist: P,
) -> Result<BatchResult, ProviderError> {
    let mut accepted = existing.to_vec();
    let mut warnings = Vec::new();
    let mut dropped = 0;
    let mut repair = false;
    let mut previous = String::new();
    for phase in ["initial", "repair", "supplement"] {
        if phase == "repair" && !repair {
            continue;
        }
        let mut report = page_coverage(
            input.catalog,
            input.batch.page_end,
            Some(input.memo),
            &accepted,
            0,
        );
        report
            .pages
            .retain(|p| p.page_number >= input.batch.page_start);
        report.sparse_pages.retain(|p| *p >= input.batch.page_start);
        if phase == "supplement" && !crate::guide_coverage::needs_supplement(&report) {
            continue;
        }
        let summary: Vec<Value> = accepted
            .iter()
            .filter_map(|ink| match ink {
                GuideInk::Note { id, anchor, .. }
                    if input.batch.anchor_block_ids.contains(&anchor.block_id) =>
                {
                    Some(json!({"id":id,"blockId":anchor.block_id}))
                }
                _ => None,
            })
            .collect();
        let hint = match phase {
            "repair" => Some(crate::guide_generation::chinese_repair_hint(&warnings)),
            "supplement" => Some(crate::guide_generation::chinese_supplement_hint(&report)),
            _ => None,
        };
        let mut user: Value = serde_json::from_str(&crate::guide_generation::annotate_user_input(
            input.cast,
            None,
            input.batch,
            phase,
            hint.as_deref(),
            Some(&json!(summary)),
            input.language,
        ))
        .map_err(|e| ProviderError::local_state(e.to_string()))?;
        user["memo"] = limited_memo(input.memo, input.batch, 6000);
        if phase == "repair" {
            user["previousOutput"] = json!(previous);
        }
        let user = crate::reader_context::prepend_reader_context(&user.to_string(), input.reader);
        if input.prompt.chars().count() + user.chars().count() > input.input_limit {
            warnings.push(format!(
                "第 {} 批 {phase} 输入超过冻结预算，未发起模型调用",
                input.batch.ordinal
            ));
            continue;
        }
        let slot = format!("batch:{}:{phase}", input.batch.ordinal);
        let response: Result<TextInteractionOutcome, _> = call_once(
            checkpoint,
            &slot,
            &mut persist,
            port.interact_text(TextInteractionRequest {
                model: input.model.into(),
                context_epoch: input.epoch.into(),
                system_instruction: input.prompt.into(),
                user_input: user,
                response_schema: Some(crate::guide_protocol::inks_schema_v2(&input.cast.order)),
                kind: PaperInteractionKind::Artifact,
            }),
        )
        .await;
        match response {
            Ok(response) => {
                previous = response.text;
                match crate::guide_validate::decode_inks_envelope(&previous) {
                    Ok(raw) => {
                        let ns = if phase == "supplement" {
                            format!("b{}s", input.batch.ordinal)
                        } else {
                            format!("b{}", input.batch.ordinal)
                        };
                        let outcome = crate::guide_validate_v2::validate_guide_inks_v2(
                            &raw,
                            input.catalog,
                            &input.cast.order,
                            &input.batch.anchor_block_ids,
                            &ns,
                        );
                        repair = outcome.dropped > 0;
                        dropped += outcome.dropped;
                        warnings.extend(outcome.warnings);
                        merge_validated_batch(&mut accepted, outcome.inks, &Default::default());
                    }
                    Err(error) => {
                        repair = true;
                        warnings.push(error);
                    }
                }
            }
            Err(error) => {
                // A transport error is not a malformed response; never invent a repair request for it.
                warnings.push(format!("第 {} 批 {phase}：{error}", input.batch.ordinal));
                if phase == "initial" {
                    break;
                }
            }
        }
    }
    Ok(BatchResult {
        inks: accepted,
        warnings,
        dropped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_ports::*;
    use std::sync::Mutex;
    struct Fake {
        requests: Mutex<Vec<TextInteractionRequest>>,
    }
    #[async_trait::async_trait]
    impl PaperModelPort for Fake {
        fn capabilities(&self, _: &str) -> PaperModelCapabilities {
            PaperModelCapabilities {
                native_pdf: true,
                interactions: true,
                structured_output: true,
                streaming: false,
            }
        }
        async fn interact(
            &self,
            _: PaperInteractionRequest,
        ) -> Result<PaperInteractionOutcome, ProviderError> {
            panic!("batch cannot attach PDF")
        }
        async fn interact_stream(
            &self,
            _: PaperStreamRequest,
        ) -> Result<PaperInteractionOutcome, ProviderError> {
            panic!("no stream")
        }
        async fn interact_text(
            &self,
            r: TextInteractionRequest,
        ) -> Result<TextInteractionOutcome, ProviderError> {
            self.requests.lock().unwrap().push(r);
            let n = self.requests.lock().unwrap().len();
            let text = if n == 1 {
                "broken JSON".into()
            } else {
                json!({"inks":[{"id":"n1","kind":"note","speakerId":"preset:chitanda","blockId":"b1","weight":"line","body":"这里的前提不能丢。","parentId":null}]}).to_string()
            };
            Ok(TextInteractionOutcome {
                text,
                provider_node_id: format!("call-{n}"),
                receipt: UsageEnvelope {
                    provider: "fake".into(),
                    model: "fake".into(),
                    ..Default::default()
                },
            })
        }
        async fn delete_remote(&self, _: &RemoteResource) -> Result<(), ProviderError> {
            Ok(())
        }
    }
    #[tokio::test]
    async fn real_batch_core_repairs_once_supplements_once_and_resumes_without_repaying() {
        let port = Fake {
            requests: Mutex::new(Vec::new()),
        };
        let cast = crate::guide_cast::snapshot_from_store(
            &crate::guide_character_settings::factory_store(),
            &["preset:chitanda".into()],
            &[],
        )
        .unwrap();
        let catalog = vec![GuideCatalogBlock {
            id: "b1".into(),
            page_number: 1,
            block_index: 0,
            block_type: "Text".into(),
            bbox: [0, 0, 1, 1],
            excerpt: Some("完整原文后半也保留".repeat(50)),
        }];
        let batch = crate::guide_catalog::plan_material_batches(
            &crate::guide_catalog::materials_from_catalog(&catalog, 800),
            4000,
        )
        .remove(0);
        let memo=crate::guide_memo::parse_guide_memo(r#"{"schemaVersion":"reading-guide-memo-v1","documentFocus":"前提","spans":[],"observations":[],"connections":[],"pageHints":[],"limitations":[]}"#).unwrap();
        let input = BatchInput {
            batch: &batch,
            cast: &cast,
            memo: &memo,
            catalog: &catalog,
            model: "fake",
            epoch: "test",
            prompt: "规则",
            language: "en",
            reader: None,
            input_limit: 20000,
        };
        let mut checkpoint = json!({});
        let first = run_batch(&port, &input, &[], &mut checkpoint, |_| Ok(()))
            .await
            .unwrap();
        assert_eq!(crate::guide_coverage::count_notes(&first.inks), 1);
        assert_eq!(port.requests.lock().unwrap().len(), 3);
        for request in port.requests.lock().unwrap().iter() {
            let value: Value = serde_json::from_str(&request.user_input).unwrap();
            assert_eq!(value["language"], "en");
        }
        let requests = port.requests.lock().unwrap();
        assert!(requests[0].user_input.contains("完整原文后半也保留"));
        assert!(requests[1].user_input.contains("broken JSON"));
        assert!(requests[2].user_input.contains("acceptedSummary"));
        drop(requests);
        let second = run_batch(&port, &input, &[], &mut checkpoint, |_| Ok(()))
            .await
            .unwrap();
        assert_eq!(first.inks, second.inks);
        assert_eq!(port.requests.lock().unwrap().len(), 3);
    }
}
