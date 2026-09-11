//! Translation protocols are immutable once a job has frozen their version.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum TranslationProtocol {
    #[default]
    V1,
    V2,
}

impl TranslationProtocol {
    pub(crate) fn from_job(payload: &Value) -> Result<Self, String> {
        match payload
            .get("prompts")
            .and_then(|p| p.get("translationProtocol"))
        {
            None => Ok(Self::V1),
            Some(value) => serde_json::from_value(value.clone())
                .map_err(|e| format!("无法识别冻结的翻译协议：{e}")),
        }
    }
    pub(crate) fn default_prompt(self, language: &str) -> String {
        let text = match self {
            Self::V1 => include_str!("../prompts/v1/translation.md"),
            Self::V2 => include_str!("../prompts/translation.md"),
        };
        text.replace("{output_language}", language)
    }
    pub(crate) fn schema(self) -> Value {
        let mut schema = legacy_schema();
        if self == Self::V2 {
            schema["name"] = json!("block_translation_v2");
            schema["schema"]["required"]
                .as_array_mut()
                .unwrap()
                .insert(0, json!("status"));
            let props = &mut schema["schema"]["properties"];
            props["status"] =
                json!({"type":"string","enum":["translated","unchanged","partial","unavailable"]});
            props["translation"] = json!({"type":"string"});
            props["notes"]["items"]["minLength"] = json!(1);
            for key in ["source", "target"] {
                props["terms"]["items"]["properties"][key]["minLength"] = json!(1);
            }
        }
        schema
    }
    pub(crate) fn validate(self, value: &Value, target: &str) -> Result<(), String> {
        if self == Self::V1 {
            // Preserve the historical acceptance contract for custom prompts and queued jobs.
            for key in ["translation", "targetLanguage"] {
                if value[key]
                    .as_str()
                    .is_none_or(|text| text.trim().is_empty())
                {
                    return Err(format!("翻译 /{key} 必须为非空字符串"));
                }
            }
            return Ok(());
        }
        crate::auxiliary_contract::validate(value, &self.schema()["schema"], "")?;
        if value["targetLanguage"] != target {
            return Err("翻译 /targetLanguage 与请求不一致".into());
        }
        let status = value["status"].as_str().unwrap();
        let translation = value["translation"].as_str().unwrap();
        let notes = value["notes"].as_array().unwrap();
        let terms = value["terms"].as_array().unwrap();
        if ["partial", "unavailable"].contains(&status) && notes.is_empty() {
            return Err("部分完成或无法翻译时，/notes 必须说明具体原因".into());
        }
        if status == "unavailable" {
            if !translation.is_empty() || !terms.is_empty() {
                return Err("无法翻译时，/translation 必须为空字符串，/terms 必须为空数组".into());
            }
        } else if translation.trim().is_empty() {
            return Err("当前翻译状态要求 /translation 非空".into());
        }
        if status == "partial" {
            // Reject only the explicit markers defined by the prompt. Semantic
            // completeness and arbitrary natural-language disclaimers remain model duties.
            let without_markers = translation
                .replace("[文本缺损]", "")
                .replace("[无法辨认]", "");
            if without_markers.trim().is_empty() {
                return Err("部分译文不能只包含缺损标记".into());
            }
        }
        Ok(())
    }
}
fn named_schema(name: &str, schema: Value) -> Value {
    json!({"name": name, "strict": true, "schema": schema})
}

fn legacy_schema() -> Value {
    named_schema(
        "block_translation",
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["sourceLanguage", "targetLanguage", "translation", "notes", "terms"],
            "properties": {
                "sourceLanguage": {"type": "string", "minLength": 1},
                "targetLanguage": {"type": "string", "minLength": 1},
                "translation": {"type": "string", "minLength": 1},
                "notes": {"type": "array", "items": {"type": "string"}},
                "terms": {"type": "array", "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["source", "target", "note"],
                    "properties": {
                        "source": {"type": "string"},
                        "target": {"type": "string"},
                        "note": {"type": "string"}
                    }
                }}
            }
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn result(status: &str) -> Value {
        json!({"status":status,"sourceLanguage":"en","targetLanguage":"zh-CN",
            "translation":"在假设成立时，结果可能支持该解释。", "notes":[], "terms":[]})
    }
    #[test]
    fn accepts_four_states_and_preserves_markdown_and_term_ambiguity() {
        for status in ["translated", "unchanged", "partial", "unavailable"] {
            let mut value = result(status);
            if status == "unchanged" {
                value["sourceLanguage"] = json!("zh");
            }
            if status == "partial" {
                value["translation"] = json!("结果可能支持该解释。[文本缺损]");
                value["notes"] = json!(["输入文本末尾的比较对象无法辨认。"]);
            }
            if status == "unavailable" {
                value["sourceLanguage"] = json!("und");
                value["translation"] = json!("");
                value["notes"] = json!(["输入文本只有无法辨认的字符，不能形成可靠译文。"]);
            }
            TranslationProtocol::V2.validate(&value, "zh-CN").unwrap();
        }
        let mut value = result("translated");
        value["sourceLanguage"] = json!("mul");
        value["translation"] = json!("1. 域 $\\nu$\n2. 领域");
        value["terms"] = json!([
            {"source":"domain","target":"域","note":"公式中的定义域。"},
            {"source":"domain","target":"领域","note":"应用范围。"},
            {"source":"FFT","target":"FFT","note":""}
        ]);
        let before = value.clone();
        TranslationProtocol::V2.validate(&value, "zh-CN").unwrap();
        assert_eq!(value, before);
    }
    #[test]
    fn rejects_invalid_status_content_combinations_and_types() {
        let valid = result("translated");
        let mut invalid = vec![];
        for (key, replacement) in [
            ("status", json!("done")),
            ("status", json!(null)),
            ("translation", json!(" ")),
            ("targetLanguage", json!("en")),
            ("sourceLanguage", json!(" ")),
            ("notes", json!([""])),
            ("notes", json!([" \n "])),
            ("notes", json!("none")),
            ("terms", json!([{"source":"x","target":"","note":""}])),
            ("terms", json!([{"source":"","target":"变量","note":""}])),
            ("terms", json!([{"source":"x","target":"变量","note":null}])),
        ] {
            let mut value = valid.clone();
            value[key] = replacement;
            invalid.push(value);
        }
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove("status");
        invalid.push(missing);
        let mut extra = valid.clone();
        extra["sources"] = json!([]);
        invalid.push(extra);
        for status in ["partial", "unavailable"] {
            invalid.push(result(status));
        }
        let mut unavailable = result("unavailable");
        unavailable["translation"] = json!("");
        unavailable["notes"] = json!(["输入为空。"]);
        let mut whitespace = unavailable.clone();
        whitespace["translation"] = json!(" ");
        invalid.push(whitespace);
        unavailable["terms"] = json!([{"source":"x","target":"x","note":""}]);
        invalid.push(unavailable);
        let mut marker = result("partial");
        marker["notes"] = json!(["输入文本无法辨认。"]);
        marker["translation"] = json!("[文本缺损]\n[无法辨认]");
        invalid.push(marker);
        for value in invalid {
            assert!(
                TranslationProtocol::V2.validate(&value, "zh-CN").is_err(),
                "accepted {value}"
            );
        }
    }
    #[test]
    fn old_jobs_keep_the_five_field_contract_and_unknown_versions_fail() {
        assert_eq!(
            TranslationProtocol::from_job(&json!({"prompts":{"system":"old"}})).unwrap(),
            TranslationProtocol::V1
        );
        assert_eq!(
            TranslationProtocol::from_job(&json!({"prompts":{"translationProtocol":"v2"}}))
                .unwrap(),
            TranslationProtocol::V2
        );
        assert!(
            TranslationProtocol::from_job(&json!({"prompts":{"translationProtocol":"v3"}}))
                .is_err()
        );
        assert!(
            TranslationProtocol::from_job(&json!({"prompts":{"translationProtocol":null}}))
                .is_err()
        );
        let old = json!({"sourceLanguage":"en","targetLanguage":"zh-CN","translation":"译文","notes":[],"terms":[]});
        TranslationProtocol::V1.validate(&old, "zh-CN").unwrap();
        assert!(TranslationProtocol::V2.validate(&old, "zh-CN").is_err());
        assert!(TranslationProtocol::V1.schema()["schema"]["properties"]
            .get("status")
            .is_none());
        assert!(TranslationProtocol::V1
            .default_prompt("zh-CN")
            .starts_with("Translate only"));
        assert!(TranslationProtocol::V2
            .default_prompt("zh-CN")
            .starts_with("# 一、任务定义"));
    }
}
