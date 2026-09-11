use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const UI_LOCALE_SCHEMA: u32 = 1;
pub const UI_LOCALE_FILE: &str = "ui-locale.json";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum UiLocale {
    #[serde(rename = "zh-CN")]
    #[default]
    ZhCn,
    #[serde(rename = "en")]
    En,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiLocaleFile {
    pub schema_version: u32,
    pub locale: UiLocale,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiLocaleProjection {
    pub locale: Option<UiLocale>,
}

pub fn ui_locale_path(config_dir: &Path) -> PathBuf {
    config_dir.join(UI_LOCALE_FILE)
}

pub fn output_language(locale: UiLocale) -> &'static str {
    match locale {
        UiLocale::ZhCn => "zh-CN",
        UiLocale::En => "en",
    }
}

pub fn message(locale: Option<UiLocale>, zh: &str, en: &str) -> String {
    match locale {
        Some(UiLocale::ZhCn) => zh.to_string(),
        Some(UiLocale::En) => en.to_string(),
        None => format!("{zh} / {en}"),
    }
}

fn invalid_locale_file(detail: impl std::fmt::Display) -> String {
    format!(
        "{}: {detail}",
        message(None, "界面语言配置无效", "UI locale file is invalid")
    )
}

pub fn load_ui_locale(config_dir: &Path) -> Result<Option<UiLocale>, String> {
    let path = ui_locale_path(config_dir);
    crate::guide_character_assets::recover_interrupted_file(&path)?;
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(&path).map_err(|error| invalid_locale_file(error))?;
    if raw.trim().is_empty() {
        return Ok(None);
    }
    let file: UiLocaleFile =
        serde_json::from_str(&raw).map_err(|error| invalid_locale_file(error))?;
    if file.schema_version != UI_LOCALE_SCHEMA {
        return Err(invalid_locale_file(format!(
            "schemaVersion {}",
            file.schema_version
        )));
    }
    Ok(Some(file.locale))
}

pub fn save_ui_locale(config_dir: &Path, locale: UiLocale) -> Result<UiLocale, String> {
    fs::create_dir_all(config_dir).map_err(|error| format!("无法创建目录：{error}"))?;
    let path = ui_locale_path(config_dir);
    crate::guide_character_assets::recover_interrupted_file(&path)?;
    let file = UiLocaleFile {
        schema_version: UI_LOCALE_SCHEMA,
        locale,
    };
    let payload = serde_json::to_vec_pretty(&file)
        .map_err(|error| format!("无法序列化界面语言配置：{error}"))?;
    crate::guide_character_assets::atomic_write(&path, &payload)?;
    Ok(locale)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_is_none() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_ui_locale(dir.path()).unwrap(), None);
    }

    #[test]
    fn empty_file_is_none() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(ui_locale_path(dir.path()), "  \n").unwrap();
        assert_eq!(load_ui_locale(dir.path()).unwrap(), None);
    }

    #[test]
    fn round_trip_en() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            save_ui_locale(dir.path(), UiLocale::En).unwrap(),
            UiLocale::En
        );
        assert_eq!(load_ui_locale(dir.path()).unwrap(), Some(UiLocale::En));
        let raw = fs::read_to_string(ui_locale_path(dir.path())).unwrap();
        assert!(raw.contains("\"schemaVersion\": 1"));
        assert!(raw.contains("\"locale\": \"en\""));
    }

    #[test]
    fn reject_unknown_locale() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            ui_locale_path(dir.path()),
            r#"{"schemaVersion":1,"locale":"fr"}"#,
        )
        .unwrap();
        let err = load_ui_locale(dir.path()).unwrap_err();
        assert!(err.contains("界面语言配置无效 / UI locale file is invalid"));
    }

    #[test]
    fn bilingual_message_none_joins_with_slash() {
        assert_eq!(message(None, "甲", "A"), "甲 / A");
    }
}
