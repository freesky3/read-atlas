//! Paper Lens v2: frozen schemas and local structural checks, not factual verification.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum LensProtocol {
    #[default]
    V1,
    V2,
}
impl LensProtocol {
    pub(crate) fn from_job(payload: &Value) -> Result<Self, String> {
        match payload.get("prompts").and_then(|p| p.get("lensProtocol")) {
            None => Ok(Self::V1),
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|e| format!("无法识别冻结的 Lens 协议：{e}")),
        }
    }
    pub(crate) fn document_prompt(
        self,
        document_kind: crate::library_paths::DocumentKind,
        kind: &str,
        repair: bool,
        language: &str,
    ) -> String {
        if document_kind == crate::library_paths::DocumentKind::Paper {
            return self.paper_prompt(kind, repair, language);
        }
        let slot = match (kind, repair) {
            ("formula", false) => crate::prompt_settings::PromptSlotId::LensFormula,
            ("figure", false) => crate::prompt_settings::PromptSlotId::LensFigure,
            ("table", false) => crate::prompt_settings::PromptSlotId::LensTable,
            ("formula", true) => crate::prompt_settings::PromptSlotId::LensRepairFormula,
            ("figure", true) => crate::prompt_settings::PromptSlotId::LensRepairFigure,
            ("table", true) => crate::prompt_settings::PromptSlotId::LensRepairTable,
            _ => unreachable!("Lens kind checked before selecting a prompt"),
        };
        let text = match self {
            Self::V1 => crate::textbook_contract::old_prompt(slot).to_string(),
            Self::V2 => crate::prompt_settings::default_text_for_kind(slot, document_kind),
        };
        text.replace("{output_language}", language)
    }
    pub(crate) fn paper_prompt(self, kind: &str, repair: bool, language: &str) -> String {
        let text = match (self, kind, repair) {
            (Self::V2, "formula", false) => include_str!("../prompts/lens-formula.paper.md"),
            (Self::V2, "figure", false) => include_str!("../prompts/lens-figure.paper.md"),
            (Self::V2, "table", false) => include_str!("../prompts/lens-table.paper.md"),
            (Self::V2, "formula", true) => include_str!("../prompts/lens-repair-formula.paper.md"),
            (Self::V2, "figure", true) => include_str!("../prompts/lens-repair-figure.paper.md"),
            (Self::V2, "table", true) => include_str!("../prompts/lens-repair-table.paper.md"),
            (Self::V1, "formula", false) => {
                include_str!("../prompts/history/lens-formula.paper.before-intuitive.md")
            }
            (Self::V1, "figure", false) => {
                include_str!("../prompts/history/lens-figure.paper.before-intuitive.md")
            }
            (Self::V1, "table", false) => {
                include_str!("../prompts/history/lens-table.paper.before-intuitive.md")
            }
            (Self::V1, "formula", true) => {
                include_str!("../prompts/history/lens-repair-formula.paper.before-intuitive.md")
            }
            (Self::V1, "figure", true) => {
                include_str!("../prompts/history/lens-repair-figure.paper.before-intuitive.md")
            }
            (Self::V1, "table", true) => {
                include_str!("../prompts/history/lens-repair-table.paper.before-intuitive.md")
            }
            _ => unreachable!("Lens kind was checked before selecting a prompt"),
        };
        text.replace("{output_language}", language)
    }
}
fn object(fields: &[(&str, Value)]) -> Value {
    let properties: serde_json::Map<String, Value> = fields
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect();
    json!({"type":"object","additionalProperties":false,
        "required":fields.iter().map(|(key,_)|*key).collect::<Vec<_>>(),"properties":properties})
}
fn text() -> Value {
    json!({"type":"string","minLength":1})
}
fn possibly_empty_text() -> Value {
    json!({"type":"string"})
}
fn array(item: Value) -> Value {
    json!({"type":"array","items":item})
}
fn evidence(block: &str) -> Value {
    let mut result = array(json!({"type":"string","enum":[format!("block:{block}")]}));
    result["uniqueItems"] = json!(true);
    result
}
pub(crate) fn schema(kind: &str, block: &str) -> Value {
    let specific = if kind == "formula" {
        object(&[
            ("whatItDoesMarkdown", possibly_empty_text()),
            ("startHereMarkdown", possibly_empty_text()),
            ("reconstructedLatex", possibly_empty_text()),
            (
                "symbols",
                array(object(&[
                    ("symbolLatex", text()),
                    ("meaningMarkdown", text()),
                    (
                        "provenance",
                        json!({"type":"string","enum":["paper_defined","standard","inferred","unresolved"]}),
                    ),
                    ("evidenceIds", evidence(block)),
                ])),
            ),
        ])
    } else {
        assert!(matches!(kind, "figure" | "table"));
        object(&[
            ("overallMarkdown", possibly_empty_text()),
            ("readingGuideMarkdown", possibly_empty_text()),
            (
                "focusPoints",
                array(object(&[
                    ("location", text()),
                    ("observationMarkdown", text()),
                    ("meaningMarkdown", text()),
                    ("evidenceIds", evidence(block)),
                ])),
            ),
        ])
    };
    let mut questions = array(text());
    questions["maxItems"] = json!(3);
    json!({"name":format!("{kind}_lens_v2"),"strict":true,"schema":object(&[
        ("status", json!({"type":"string","enum":["complete","partial","unavailable"]})),
        ("limitations", array(text())),
        ("quickTakeaway", object(&[("title",text()),("markdown",text())])),
        ("sections", array(object(&[
            ("sectionId",text()),("title",text()),("markdown",text()),("evidenceIds",evidence(block)),
        ]))),
        ("suggestedQuestions", questions), (kind,specific),
    ])})
}
pub(crate) fn validate(value: &Value, kind: &str, block: &str) -> Result<(), String> {
    crate::auxiliary_contract::validate(value, &schema(kind, block)["schema"], "")?;
    let sections = value["sections"].as_array().unwrap();
    let mut ids = HashSet::new();
    for section in sections {
        if !ids.insert(section["sectionId"].as_str().unwrap().trim()) {
            return Err("Lens /sections: sectionId 重复".into());
        }
    }
    let status = value["status"].as_str().unwrap();
    if status != "complete" && value["limitations"].as_array().unwrap().is_empty() {
        return Err("Lens partial / unavailable 必须说明具体材料局限".into());
    }
    let string_fields = if kind == "formula" {
        vec![
            "whatItDoesMarkdown",
            "startHereMarkdown",
            "reconstructedLatex",
        ]
    } else {
        vec!["overallMarkdown", "readingGuideMarkdown"]
    };
    if status == "unavailable" {
        if string_fields
            .iter()
            .any(|key| value[kind][key].as_str().unwrap() != "")
            || !value[kind][if kind == "formula" {
                "symbols"
            } else {
                "focusPoints"
            }]
            .as_array()
            .unwrap()
            .is_empty()
            || !sections.is_empty()
            || !value["suggestedQuestions"].as_array().unwrap().is_empty()
        {
            return Err(
                "Lens unavailable 不得包含原式、解释、符号、重点或追问；原因写入概览和局限".into(),
            );
        }
    } else {
        for key in string_fields {
            if status == "partial" && key == "reconstructedLatex" {
                continue;
            }
            if value[kind][key].as_str().unwrap().trim().is_empty() {
                return Err(format!("Lens /{kind}/{key}: 当前状态要求非空内容"));
            }
        }
        if kind == "formula" && sections.is_empty() {
            return Err("公式 Lens 必须包含帮助理解的解释 section".into());
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn fixture(kind: &str, status: &str) -> Value {
    let unavailable = status == "unavailable";
    let body = "先把它看成一个整体。\n\n1. 对照 $\\nu$。\n2. 保留条件。";
    let mut value = json!({"status":status,"limitations":if status == "complete" {vec![]} else {vec!["当前选区的右侧标签无法确认。"]},
        "quickTakeaway":{"title":"理解当前对象","markdown":"在可确认的范围内理解。"},
        "sections":[],"suggestedQuestions":[]});
    if kind == "formula" {
        value[kind] = json!({"whatItDoesMarkdown":if unavailable {""} else {body},
            "startHereMarkdown":if unavailable {""} else {"先看外层汇总，再理解内层计算。"},
            "reconstructedLatex":if unavailable {""} else {r"\nu = \frac{x}{2}"},"symbols":[]});
        if !unavailable {
            value[kind]["symbols"] = json!([{"symbolLatex":r"\nu","meaningMarkdown":"当前量。","provenance":"inferred","evidenceIds":[]}]);
            value["sections"] = json!([{"sectionId":"example","title":"一个简单例子","markdown":body,"evidenceIds":["block:block-1"]}]);
        }
    } else {
        value[kind] = json!({"overallMarkdown":if unavailable {""} else {body},
            "readingGuideMarkdown":if unavailable {""} else {"先确认单位，再对照同一设置。"},"focusPoints":[]});
        if !unavailable {
            value[kind]["focusPoints"] = json!([{"location":"左侧同一条件下的两组结果","observationMarkdown":"两组读数有差异。","meaningMarkdown":"差异只支持此条件下的比较。","evidenceIds":["block:block-1"]}]);
        }
    }
    value
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lens_v2_states_preserve_math_and_allow_honest_partial_objects() {
        for kind in ["formula", "figure", "table"] {
            for status in ["complete", "partial", "unavailable"] {
                let value = fixture(kind, status);
                let before = value.clone();
                validate(&value, kind, "block-1").unwrap();
                assert_eq!(value, before);
            }
        }
        let mut partial = fixture("formula", "partial");
        partial["formula"]["reconstructedLatex"] = json!("");
        validate(&partial, "formula", "block-1").unwrap();
        partial["status"] = json!("complete");
        assert!(validate(&partial, "formula", "block-1").is_err());
    }
    #[test]
    fn lens_v2_rejects_nested_errors_unavailable_fabrication_and_bad_whitelists() {
        for kind in ["formula", "figure", "table"] {
            let good = fixture(kind, "complete");
            for key in good.as_object().unwrap().keys() {
                let mut bad = good.clone();
                bad.as_object_mut().unwrap().remove(key);
                assert!(validate(&bad, kind, "block-1").is_err(), "missing {key}");
            }
            let item_path = if kind == "formula" {
                "/formula/symbols/0"
            } else if kind == "figure" {
                "/figure/focusPoints/0"
            } else {
                "/table/focusPoints/0"
            };
            for field in good.pointer(item_path).unwrap().as_object().unwrap().keys() {
                let mut bad = good.clone();
                bad.pointer_mut(item_path)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                assert!(
                    validate(&bad, kind, "block-1").is_err(),
                    "missing nested {field}"
                );
            }
            for invalid_ids in [
                json!(["block:unknown"]),
                json!(["block:block-1", "block:block-1"]),
                json!([1]),
                json!(null),
            ] {
                let mut bad = good.clone();
                bad.pointer_mut(item_path).unwrap()["evidenceIds"] = invalid_ids;
                assert!(validate(&bad, kind, "block-1").is_err());
            }
            let mut bad = good.clone();
            bad.pointer_mut(item_path).unwrap()["invented"] = json!("unused");
            assert!(validate(&bad, kind, "block-1").is_err());
            bad = good.clone();
            bad["suggestedQuestions"] = json!(["1", "2", "3", "4"]);
            assert!(validate(&bad, kind, "block-1").is_err());
            bad = good.clone();
            bad["suggestedQuestions"] = json!([" "]);
            assert!(validate(&bad, kind, "block-1").is_err());
            bad = good.clone();
            bad["sections"] = json!([
                {"sectionId":"same","title":"标题","markdown":"正文","evidenceIds":[]},
                {"sectionId":" same ","title":"标题","markdown":"正文","evidenceIds":[]}]);
            assert!(validate(&bad, kind, "block-1").is_err());
            for status in ["partial", "unavailable"] {
                bad = fixture(kind, status);
                bad["limitations"] = json!([]);
                assert!(validate(&bad, kind, "block-1").is_err());
            }
            bad = good.clone();
            bad["status"] = json!("unavailable");
            bad["limitations"] = json!(["缺口"]);
            assert!(validate(&bad, kind, "block-1").is_err());
        }
        let mut bad = fixture("formula", "complete");
        bad["formula"]["symbols"][0]["provenance"] = json!("verified");
        assert!(validate(&bad, "formula", "block-1").is_err());
    }
    #[test]
    fn frozen_lens_version_defaults_old_jobs_and_rejects_unknown_versions() {
        assert_eq!(
            LensProtocol::from_job(&json!({})).unwrap(),
            LensProtocol::V1
        );
        assert_eq!(
            LensProtocol::from_job(&json!({"prompts":{"lensProtocol":"v2"}})).unwrap(),
            LensProtocol::V2
        );
        for version in [json!("v3"), json!(2), json!(null)] {
            assert!(LensProtocol::from_job(&json!({"prompts":{"lensProtocol":version}})).is_err());
        }
    }
}
