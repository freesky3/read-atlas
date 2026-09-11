use crate::guide_character_assets::{
    atomic_write, recover_interrupted_file, validate_managed_path,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

pub const CHARACTERS_DIR: &str = crate::library_paths::CHARACTERS_DIR;
const PENDING_STORE: &str = ".pending-store.json";
pub const GUIDE_CHARACTER_STORE_SCHEMA: u32 = 1;

pub const PRESET_CHITANDA: &str = "preset:chitanda";
pub const PRESET_OREKI: &str = "preset:oreki";
pub const PRESET_FRIEREN: &str = "preset:frieren";
pub const PRESET_JOTARO: &str = "preset:jotaro";
pub const PRESET_CONAN: &str = "preset:conan";

const PRESET_FILES: &[(&str, &str)] = &[
    (
        PRESET_CHITANDA,
        include_str!("../prompts/guide-characters/chitanda.json"),
    ),
    (
        PRESET_OREKI,
        include_str!("../prompts/guide-characters/oreki.json"),
    ),
    (
        PRESET_FRIEREN,
        include_str!("../prompts/guide-characters/frieren.json"),
    ),
    (
        PRESET_JOTARO,
        include_str!("../prompts/guide-characters/jotaro.json"),
    ),
    (
        PRESET_CONAN,
        include_str!("../prompts/guide-characters/conan.json"),
    ),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideCharacter {
    pub id: String,
    pub revision: u32,
    pub display_name: String,
    #[serde(default)]
    pub work_title: String,
    #[serde(default)]
    pub character_version: String,
    #[serde(default)]
    pub description: String,
    pub avatar_asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar_rel_path: Option<String>,
    pub ink_color: String,
    pub personality: String,
    pub reading_habits: String,
    pub expression_style: String,
    pub avoidances: String,
    #[serde(default)]
    pub example_notes: Vec<String>,
    pub preset_id: Option<String>,
    pub preset_version: Option<u32>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuidePresetCast {
    pub id: String,
    pub name: String,
    pub character_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideCharacterStoreFile {
    pub schema_version: u32,
    pub store_revision: u32,
    pub characters: Vec<GuideCharacter>,
    pub default_character_ids: Vec<String>,
    #[serde(default)]
    pub preset_casts: Vec<GuidePresetCast>,
    #[serde(default)]
    pub retired_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideCharacterSettingsProjection {
    pub schema_version: u32,
    pub store_revision: u32,
    pub characters: Vec<GuideCharacter>,
    pub default_character_ids: Vec<String>,
    pub preset_casts: Vec<GuidePresetCast>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideCharacterDraft {
    pub id: Option<String>,
    pub display_name: String,
    #[serde(default)]
    pub work_title: String,
    #[serde(default)]
    pub character_version: String,
    #[serde(default)]
    pub description: String,
    pub avatar_asset_id: Option<String>,
    pub ink_color: String,
    pub personality: String,
    pub reading_habits: String,
    pub expression_style: String,
    pub avoidances: String,
    #[serde(default)]
    pub example_notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresetFile {
    preset_id: String,
    preset_version: u32,
    id: String,
    display_name: String,
    #[serde(default)]
    work_title: String,
    #[serde(default)]
    character_version: String,
    #[serde(default)]
    description: String,
    ink_color: String,
    personality: String,
    reading_habits: String,
    expression_style: String,
    avoidances: String,
    #[serde(default)]
    example_notes: Vec<String>,
}

pub fn characters_dir(workspace_root: &Path) -> PathBuf {
    workspace_root.join(CHARACTERS_DIR)
}

pub fn character_settings_path(workspace_root: &Path) -> PathBuf {
    characters_dir(workspace_root)
}

pub fn folder_name_for_id(id: &str) -> String {
    id.replace(':', "-")
}

pub fn builtin_presets() -> Vec<GuideCharacter> {
    PRESET_FILES
        .iter()
        .map(|(_, json)| {
            let preset: PresetFile =
                serde_json::from_str(json).expect("guide character preset must parse");
            character_from_preset(&preset, now())
        })
        .collect()
}

fn character_from_preset(preset: &PresetFile, timestamp: String) -> GuideCharacter {
    GuideCharacter {
        id: preset.id.clone(),
        revision: 1,
        display_name: preset.display_name.clone(),
        work_title: preset.work_title.clone(),
        character_version: preset.character_version.clone(),
        description: preset.description.clone(),
        avatar_asset_id: None,
        avatar_rel_path: None,
        ink_color: normalize_color(&preset.ink_color).expect("preset color"),
        personality: preset.personality.clone(),
        reading_habits: preset.reading_habits.clone(),
        expression_style: preset.expression_style.clone(),
        avoidances: preset.avoidances.clone(),
        example_notes: preset.example_notes.clone(),
        preset_id: Some(preset.preset_id.clone()),
        preset_version: Some(preset.preset_version),
        created_at: timestamp.clone(),
        updated_at: timestamp,
    }
}

pub fn default_character_ids() -> Vec<String> {
    vec![
        PRESET_CHITANDA.to_string(),
        PRESET_OREKI.to_string(),
        PRESET_FRIEREN.to_string(),
    ]
}

pub fn factory_preset_casts() -> Vec<GuidePresetCast> {
    vec![
        GuidePresetCast {
            id: "daily".to_string(),
            name: "日常阅读".to_string(),
            character_ids: default_character_ids(),
        },
        GuidePresetCast {
            id: "evidence".to_string(),
            name: "实验与证据".to_string(),
            character_ids: vec![
                PRESET_CHITANDA.to_string(),
                PRESET_CONAN.to_string(),
                PRESET_JOTARO.to_string(),
            ],
        },
        GuidePresetCast {
            id: "classics".to_string(),
            name: "古典部双人".to_string(),
            character_ids: vec![PRESET_CHITANDA.to_string(), PRESET_OREKI.to_string()],
        },
    ]
}

pub fn factory_store() -> GuideCharacterStoreFile {
    GuideCharacterStoreFile {
        schema_version: GUIDE_CHARACTER_STORE_SCHEMA,
        store_revision: 1,
        characters: builtin_presets(),
        default_character_ids: default_character_ids(),
        preset_casts: factory_preset_casts(),
        retired_ids: Vec::new(),
    }
}

pub fn load_store(path: &Path) -> Result<GuideCharacterSettingsProjection, String> {
    let _lock = lock_store(path)?;
    Ok(project_store(&read_file(path)?))
}

pub fn save_character(
    path: &Path,
    expected_revision: u32,
    draft: GuideCharacterDraft,
) -> Result<GuideCharacterSettingsProjection, String> {
    let _lock = lock_store(path)?;
    let mut file = read_file(path)?;
    check_revision(&file, expected_revision)?;
    let name = require_name(&draft.display_name)?;
    let color = normalize_color(&draft.ink_color)?;
    let notes = sanitize_notes(&draft.example_notes);
    let now = now();
    if let Some(id) = draft
        .id
        .as_deref()
        .map(str::trim)
        .filter(|id| !id.is_empty())
    {
        if file.retired_ids.iter().any(|retired| retired == id) {
            return Err("该人物标识已停用，不能重新使用".to_string());
        }
        let Some(index) = file.characters.iter().position(|item| item.id == id) else {
            return Err("找不到要保存的人物".to_string());
        };
        let current = &file.characters[index];
        let mut next = current.clone();
        next.display_name = name;
        next.work_title = draft.work_title.trim().to_string();
        next.character_version = draft.character_version.trim().to_string();
        next.description = draft.description.trim().to_string();
        next.avatar_rel_path = None;
        next.avatar_asset_id = draft
            .avatar_asset_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
        next.ink_color = color;
        next.personality = draft.personality.trim().to_string();
        next.reading_habits = draft.reading_habits.trim().to_string();
        next.expression_style = draft.expression_style.trim().to_string();
        next.avoidances = draft.avoidances.trim().to_string();
        next.example_notes = notes;
        if character_substance(current) != character_substance(&next) {
            next.revision = current.revision.saturating_add(1);
            next.updated_at = now;
        }
        file.characters[index] = next;
    } else {
        let id = new_custom_id(&file);
        file.characters.push(GuideCharacter {
            id,
            revision: 1,
            display_name: name,
            work_title: draft.work_title.trim().to_string(),
            character_version: draft.character_version.trim().to_string(),
            description: draft.description.trim().to_string(),
            avatar_asset_id: draft
                .avatar_asset_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string),
            avatar_rel_path: None,
            ink_color: color,
            personality: draft.personality.trim().to_string(),
            reading_habits: draft.reading_habits.trim().to_string(),
            expression_style: draft.expression_style.trim().to_string(),
            avoidances: draft.avoidances.trim().to_string(),
            example_notes: notes,
            preset_id: None,
            preset_version: None,
            created_at: now.clone(),
            updated_at: now,
        });
    }
    bump_and_write(path, file)
}

pub fn duplicate_character(
    path: &Path,
    expected_revision: u32,
    character_id: &str,
) -> Result<GuideCharacterSettingsProjection, String> {
    let _lock = lock_store(path)?;
    let mut file = read_file(path)?;
    check_revision(&file, expected_revision)?;
    let Some(source) = file
        .characters
        .iter()
        .find(|item| item.id == character_id)
        .cloned()
    else {
        return Err("找不到要复制的人物".to_string());
    };
    let now = now();
    file.characters.push(GuideCharacter {
        id: new_custom_id(&file),
        revision: 1,
        display_name: format!("{} 副本", source.display_name),
        work_title: source.work_title,
        character_version: source.character_version,
        description: source.description,
        avatar_asset_id: source.avatar_asset_id,
        avatar_rel_path: source.avatar_rel_path,
        ink_color: source.ink_color,
        personality: source.personality,
        reading_habits: source.reading_habits,
        expression_style: source.expression_style,
        avoidances: source.avoidances,
        example_notes: source.example_notes,
        preset_id: None,
        preset_version: None,
        created_at: now.clone(),
        updated_at: now,
    });
    bump_and_write(path, file)
}

pub fn delete_character(
    path: &Path,
    expected_revision: u32,
    character_id: &str,
) -> Result<GuideCharacterSettingsProjection, String> {
    let _lock = lock_store(path)?;
    let mut file = read_file(path)?;
    check_revision(&file, expected_revision)?;
    let before = file.characters.len();
    file.characters.retain(|item| item.id != character_id);
    if file.characters.len() == before {
        return Err("找不到要删除的人物".to_string());
    }
    if !file.retired_ids.iter().any(|id| id == character_id) {
        file.retired_ids.push(character_id.to_string());
    }
    file.default_character_ids.retain(|id| id != character_id);
    for cast in &mut file.preset_casts {
        cast.character_ids.retain(|id| id != character_id);
    }
    bump_and_write(path, file)
}

pub fn restore_character_preset(
    path: &Path,
    expected_revision: u32,
    character_id: &str,
) -> Result<GuideCharacterSettingsProjection, String> {
    let _lock = lock_store(path)?;
    let mut file = read_file(path)?;
    check_revision(&file, expected_revision)?;
    let Some(index) = file
        .characters
        .iter()
        .position(|item| item.id == character_id)
    else {
        return Err("找不到要恢复的人物".to_string());
    };
    let preset_id = file.characters[index]
        .preset_id
        .clone()
        .ok_or_else(|| "该人物没有可恢复的出厂设定".to_string())?;
    let preset = builtin_presets()
        .into_iter()
        .find(|item| item.preset_id.as_deref() == Some(preset_id.as_str()))
        .ok_or_else(|| "找不到对应的出厂设定".to_string())?;
    let current = &file.characters[index];
    let mut restored = preset;
    restored.id = current.id.clone();
    restored.revision = current.revision.saturating_add(1);
    restored.created_at = current.created_at.clone();
    restored.updated_at = now();
    restored.avatar_asset_id = None;
    file.characters[index] = restored;
    bump_and_write(path, file)
}

pub fn save_default_cast(
    path: &Path,
    expected_revision: u32,
    character_ids: Vec<String>,
) -> Result<GuideCharacterSettingsProjection, String> {
    let _lock = lock_store(path)?;
    let mut file = read_file(path)?;
    check_revision(&file, expected_revision)?;
    file.default_character_ids = unique_existing_ids(&file, &character_ids)?;
    bump_and_write(path, file)
}

pub fn save_preset_casts(
    path: &Path,
    expected_revision: u32,
    preset_casts: Vec<GuidePresetCast>,
) -> Result<GuideCharacterSettingsProjection, String> {
    let _lock = lock_store(path)?;
    let mut file = read_file(path)?;
    check_revision(&file, expected_revision)?;

    let mut validated = Vec::new();
    let mut seen_ids = BTreeSet::new();

    for cast in preset_casts {
        let name = cast.name.trim();
        if name.is_empty() {
            return Err("阵容名称不能为空".to_string());
        }
        let id = cast.id.trim().to_string();
        let id = if id.is_empty() {
            new_preset_cast_id(&file)
        } else {
            id
        };
        if !seen_ids.insert(id.clone()) {
            return Err(format!("阵容标识重复：{id}"));
        }
        let character_ids = unique_existing_ids(&file, &cast.character_ids)?;
        validated.push(GuidePresetCast {
            id,
            name: name.to_string(),
            character_ids,
        });
    }

    file.preset_casts = validated;
    bump_and_write(path, file)
}

pub fn restore_factory_preset_casts(
    path: &Path,
    expected_revision: u32,
) -> Result<GuideCharacterSettingsProjection, String> {
    let _lock = lock_store(path)?;
    let mut file = read_file(path)?;
    check_revision(&file, expected_revision)?;

    let mut factory = factory_preset_casts();
    for cast in &mut factory {
        cast.character_ids
            .retain(|id| file.characters.iter().any(|c| c.id == *id));
    }
    factory.retain(|cast| !cast.character_ids.is_empty());
    if factory.is_empty() {
        return Err("当前角色库无法构成任何系统预设阵容".to_string());
    }

    file.preset_casts = factory;
    bump_and_write(path, file)
}

fn new_preset_cast_id(file: &GuideCharacterStoreFile) -> String {
    loop {
        let id = format!("cast_{}", uuid::Uuid::new_v4().simple());
        if file.preset_casts.iter().all(|item| item.id != id) {
            return id;
        }
    }
}

fn unique_existing_ids(
    file: &GuideCharacterStoreFile,
    character_ids: &[String],
) -> Result<Vec<String>, String> {
    let mut seen = BTreeSet::new();
    let mut ordered = Vec::new();
    for id in character_ids {
        let id = id.trim();
        if id.is_empty() {
            continue;
        }
        if !file.characters.iter().any(|item| item.id == id) {
            return Err(format!("阵容包含未知人物：{id}"));
        }
        if seen.insert(id.to_string()) {
            ordered.push(id.to_string());
        }
    }
    if ordered.is_empty() {
        return Err("至少选择一位批注角色".to_string());
    }
    Ok(ordered)
}

fn project_store(file: &GuideCharacterStoreFile) -> GuideCharacterSettingsProjection {
    GuideCharacterSettingsProjection {
        schema_version: file.schema_version,
        store_revision: file.store_revision,
        characters: file.characters.clone(),
        default_character_ids: file.default_character_ids.clone(),
        preset_casts: file.preset_casts.clone(),
    }
}

fn is_legacy_file_path(path: &Path) -> bool {
    path.is_file()
        || path
            .extension()
            .is_some_and(|extension| extension == "json")
}

fn lock_store(path: &Path) -> Result<fs::File, String> {
    let lock = if is_legacy_file_path(path) {
        path.with_file_name(format!(
            ".{}.store.lock",
            path.file_name().unwrap_or_default().to_string_lossy()
        ))
    } else {
        characters_dir_from(path).join(".store.lock")
    };
    crate::guide_character_assets::lock_file(&lock)
}

fn read_file(path: &Path) -> Result<GuideCharacterStoreFile, String> {
    if is_legacy_file_path(path) {
        return read_legacy_json(path);
    }
    let dir = characters_dir_from(path);
    validate_managed_path(dir.parent().ok_or("无效的角色目录")?, &dir)?;
    recover_pending_store(&dir)?;
    let cast_path = dir.join("cast.json");
    recover_interrupted_file(&cast_path)?;
    let file = load_characters_dir(&dir)?;
    if !cast_path.exists() && file.characters.is_empty() {
        let factory = factory_store();
        write_file(&dir, &factory)?;
        return Ok(factory);
    }
    Ok(file)
}

fn characters_dir_from(path: &Path) -> PathBuf {
    if path.file_name().and_then(|name| name.to_str()) == Some(CHARACTERS_DIR) {
        path.to_path_buf()
    } else {
        characters_dir(path)
    }
}

fn read_legacy_json(path: &Path) -> Result<GuideCharacterStoreFile, String> {
    recover_interrupted_file(path)?;
    if !path.exists() {
        return Err("角色配置文件缺失，且无法从备份恢复".to_string());
    }
    let raw = fs::read_to_string(path).map_err(|error| format!("无法读取角色配置：{error}"))?;
    let value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|error| format!("角色配置文件格式无效，已保留原文件：{error}"))?;
    let schema = value
        .get("schemaVersion")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if schema != u64::from(GUIDE_CHARACTER_STORE_SCHEMA) {
        return Err(format!("角色配置版本 {schema} 不受支持，已保留原文件"));
    }
    serde_json::from_value(value)
        .map_err(|error| format!("角色配置文件格式无效，已保留原文件：{error}"))
}

fn validate_store(file: &GuideCharacterStoreFile) -> Result<(), String> {
    if file.schema_version != GUIDE_CHARACTER_STORE_SCHEMA {
        return Err("不支持的角色配置版本，已保留原文件".to_string());
    }
    let mut ids = BTreeSet::new();
    let mut folders = BTreeSet::new();
    for character in &file.characters {
        let folder = folder_name_for_id(&character.id);
        crate::guide_character_assets::validate_folder_name(&folder)?;
        if !ids.insert(&character.id) || !folders.insert(folder.to_ascii_lowercase()) {
            return Err("角色配置包含重复的标识或目录，已保留原文件".to_string());
        }
        normalize_color(&character.ink_color)?;
        if let Some(asset) = character.avatar_asset_id.as_deref() {
            crate::guide_character_assets::validate_asset_id(asset)?;
        }
    }
    for id in &file.retired_ids {
        crate::guide_character_assets::validate_folder_name(&folder_name_for_id(id))?;
    }
    Ok(())
}

fn load_characters_dir(dir: &Path) -> Result<GuideCharacterStoreFile, String> {
    let cast_path = dir.join("cast.json");
    recover_interrupted_file(&cast_path)?;
    let mut file = if cast_path.is_file() {
        read_legacy_json(&cast_path)?
    } else {
        GuideCharacterStoreFile {
            schema_version: GUIDE_CHARACTER_STORE_SCHEMA,
            store_revision: 1,
            characters: Vec::new(),
            default_character_ids: Vec::new(),
            preset_casts: factory_preset_casts(),
            retired_ids: Vec::new(),
        }
    };
    let previous_order: Vec<String> = file.characters.iter().map(|item| item.id.clone()).collect();
    let mut loaded = Vec::new();
    let entries = fs::read_dir(dir).map_err(|error| format!("无法读取角色目录：{error}"))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("无法读取角色目录项：{error}"))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.starts_with('_') {
            continue;
        }
        // Ignore unrelated user folders without ever following or deleting them.
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_symlink()
        {
            return Err("角色目录不能使用符号链接".to_string());
        }
        validate_managed_path(dir, &path)?;
        if !path.is_dir() {
            continue;
        }
        let json_path = path.join("character.json");
        recover_interrupted_file(&json_path)?;
        if !json_path.is_file() {
            continue;
        }
        let raw = fs::read_to_string(&json_path)
            .map_err(|error| format!("无法读取角色 {name}：{error}"))?;
        let mut character: GuideCharacter = serde_json::from_str(&raw)
            .map_err(|error| format!("角色 {name} 格式无效，已保留原文件：{error}"))?;
        if folder_name_for_id(&character.id) != name {
            return Err(format!("角色 {name} 的标识与目录不一致，已保留原文件"));
        }
        if file.retired_ids.contains(&character.id) {
            continue;
        }
        character.avatar_rel_path = detect_avatar_rel(&path, &character.id)?;
        if let Some(relative) = character.avatar_rel_path.as_deref() {
            let workspace = dir.parent().ok_or("无效的工作区目录")?;
            // A user may replace avatar.png directly. Freeze the actual bytes, never a stale id.
            match crate::guide_character_assets::read_workspace_avatar(workspace, relative) {
                Ok((_, bytes)) => {
                    character.avatar_asset_id = Some(
                        crate::guide_character_assets::import_workspace_avatar(workspace, &bytes)?
                            .asset_id,
                    )
                }
                Err(_) => {
                    character.avatar_asset_id = None;
                    character.avatar_rel_path = None;
                }
            }
        } else {
            character.avatar_asset_id = None;
        }
        loaded.push(character);
    }
    loaded.sort_by(|left, right| {
        let position = |id: &str| {
            previous_order
                .iter()
                .position(|item| item == id)
                .or_else(|| PRESET_FILES.iter().position(|(preset, _)| *preset == id))
                .unwrap_or(usize::MAX)
        };
        position(&left.id)
            .cmp(&position(&right.id))
            .then_with(|| left.id.cmp(&right.id))
    });
    for id in &previous_order {
        if !file.retired_ids.contains(id) && !loaded.iter().any(|item| &item.id == id) {
            return Err(format!(
                "角色 {id} 的文件缺失，已保留配置；请恢复文件后重试"
            ));
        }
    }
    file.characters = loaded;
    validate_store(&file)?;
    Ok(file)
}

fn detect_avatar_rel(folder: &Path, character_id: &str) -> Result<Option<String>, String> {
    crate::guide_character_assets::validate_folder_name(&folder_name_for_id(character_id))?;
    for extension in ["png", "jpg", "jpeg", "webp", "gif"] {
        let path = folder.join(format!("avatar.{extension}"));
        validate_managed_path(folder, &path)?;
        if path.is_file() {
            return Ok(Some(format!(
                "{CHARACTERS_DIR}/{}/avatar.{extension}",
                folder_name_for_id(character_id)
            )));
        }
    }
    Ok(None)
}

fn recover_pending_store(dir: &Path) -> Result<(), String> {
    let pending = dir.join(PENDING_STORE);
    recover_interrupted_file(&pending)?;
    if pending.is_file() {
        let file = read_legacy_json(&pending)?;
        validate_store(&file)?;
        apply_store_files(dir, &file)?;
        fs::remove_file(&pending).map_err(|error| format!("无法完成角色配置恢复：{error}"))?;
    }
    Ok(())
}

fn write_file(path: &Path, file: &GuideCharacterStoreFile) -> Result<(), String> {
    validate_store(file)?;
    if is_legacy_file_path(path) {
        let payload = serde_json::to_vec_pretty(file).map_err(|error| error.to_string())?;
        return atomic_write(path, &payload);
    }
    let dir = characters_dir_from(path);
    validate_managed_path(dir.parent().ok_or("无效的角色目录")?, &dir)?;
    fs::create_dir_all(&dir).map_err(|error| format!("无法创建角色目录：{error}"))?;
    // Reject missing assets and unsafe file targets before starting a transaction.
    let workspace = dir.parent().ok_or("无效的工作区目录")?;
    for character in &file.characters {
        validate_managed_path(
            &dir,
            &dir.join(folder_name_for_id(&character.id))
                .join("character.json"),
        )?;
        if let Some(asset_id) = character.avatar_asset_id.as_deref() {
            crate::guide_character_assets::read_workspace_avatar(workspace, asset_id)?;
        }
    }
    // Journal the complete intended revision before changing any role file. Readers
    // finish an interrupted transaction before observing character/cast revisions.
    let payload =
        serde_json::to_vec_pretty(file).map_err(|error| format!("无法序列化角色配置：{error}"))?;
    atomic_write(&dir.join(PENDING_STORE), &payload)?;
    apply_store_files(&dir, file)?;
    fs::remove_file(dir.join(PENDING_STORE))
        .map_err(|error| format!("无法完成角色配置保存：{error}"))
}

fn apply_store_files(dir: &Path, file: &GuideCharacterStoreFile) -> Result<(), String> {
    let workspace = dir.parent().ok_or("无效的工作区目录")?;
    for character in &file.characters {
        let folder_name = folder_name_for_id(&character.id);
        let folder = dir.join(&folder_name);
        validate_managed_path(dir, &folder)?;
        fs::create_dir_all(&folder).map_err(|error| format!("无法创建人物目录：{error}"))?;
        let mut stored = character.clone();
        stored.avatar_rel_path = match character.avatar_asset_id.as_deref() {
            Some(asset_id) => crate::guide_character_assets::copy_asset_into_character_folder(
                workspace,
                &folder_name,
                asset_id,
            )?,
            None => None,
        };
        // Only remove exact managed avatar filenames, never a user directory.
        for extension in ["png", "jpg", "jpeg", "webp", "gif"] {
            let avatar = folder.join(format!("avatar.{extension}"));
            validate_managed_path(dir, &avatar)?;
            let relative = format!("{CHARACTERS_DIR}/{folder_name}/avatar.{extension}");
            if stored.avatar_rel_path.as_deref() != Some(relative.as_str()) && avatar.is_file() {
                fs::remove_file(&avatar).map_err(|error| format!("无法移除旧头像：{error}"))?;
            }
        }
        let payload = serde_json::to_vec_pretty(&stored)
            .map_err(|error| format!("无法序列化角色：{error}"))?;
        atomic_write(&folder.join("character.json"), &payload)?;
    }
    // Retired folders are intentionally retained. The retired-id list hides them
    // without deleting personal files that may share the Characters directory.
    let payload =
        serde_json::to_vec_pretty(file).map_err(|error| format!("无法序列化阵容：{error}"))?;
    atomic_write(&dir.join("cast.json"), &payload)
}

fn bump_and_write(
    path: &Path,
    mut file: GuideCharacterStoreFile,
) -> Result<GuideCharacterSettingsProjection, String> {
    file.store_revision = file.store_revision.saturating_add(1);
    file.schema_version = GUIDE_CHARACTER_STORE_SCHEMA;
    write_file(path, &file)?;
    Ok(project_store(&read_file(path)?))
}

fn check_revision(file: &GuideCharacterStoreFile, expected: u32) -> Result<(), String> {
    if file.store_revision != expected {
        return Err(format!(
            "角色配置已被其他窗口更新（当前版本 {}，提交版本 {expected}）",
            file.store_revision
        ));
    }
    Ok(())
}

fn require_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("显示名称不能为空".to_string());
    }
    Ok(name.to_string())
}

pub fn normalize_color(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);
    if hex.len() != 6 || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return Err("颜色须为 #RRGGBB".to_string());
    }
    Ok(format!("#{}", hex.to_ascii_uppercase()))
}

fn sanitize_notes(notes: &[String]) -> Vec<String> {
    notes
        .iter()
        .map(|note| note.trim().to_string())
        .filter(|note| !note.is_empty())
        .collect()
}

fn new_custom_id(file: &GuideCharacterStoreFile) -> String {
    loop {
        let id = uuid::Uuid::new_v4().to_string();
        if file.characters.iter().all(|item| item.id != id)
            && file.retired_ids.iter().all(|item| item != &id)
        {
            return id;
        }
    }
}

fn character_substance(character: &GuideCharacter) -> String {
    serde_json::json!({
        "displayName": character.display_name,
        "workTitle": character.work_title,
        "characterVersion": character.character_version,
        "description": character.description,
        "avatarAssetId": character.avatar_asset_id,
        "inkColor": character.ink_color,
        "personality": character.personality,
        "readingHabits": character.reading_habits,
        "expressionStyle": character.expression_style,
        "avoidances": character.avoidances,
        "exampleNotes": character.example_notes,
    })
    .to_string()
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

pub fn get_character<'a>(
    file: &'a GuideCharacterStoreFile,
    id: &str,
) -> Option<&'a GuideCharacter> {
    file.characters.iter().find(|item| item.id == id)
}

pub fn load_store_file(path: &Path) -> Result<GuideCharacterStoreFile, String> {
    let _lock = lock_store(path)?;
    read_file(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = character_settings_path(dir.path());
        (dir, path)
    }

    #[test]
    fn factory_has_five_presets_and_three_defaults() {
        let store = factory_store();
        assert_eq!(
            store
                .characters
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            [
                PRESET_CHITANDA,
                PRESET_OREKI,
                PRESET_FRIEREN,
                PRESET_JOTARO,
                PRESET_CONAN
            ]
        );
        assert_eq!(store.default_character_ids, default_character_ids());
        assert_eq!(store.characters[0].display_name, "千反田爱瑠");
        assert_eq!(store.characters[3].character_version, "第四部");
    }

    #[test]
    fn rename_does_not_mint_a_new_id() {
        let (_dir, path) = path();
        let loaded = load_store(&path).unwrap();
        let mut draft = draft_from(&loaded.characters[0]);
        draft.display_name = "千反田".to_string();
        let saved = save_character(&path, loaded.store_revision, draft).unwrap();
        assert_eq!(saved.characters[0].id, PRESET_CHITANDA);
        assert_eq!(saved.characters[0].display_name, "千反田");
        assert_eq!(saved.characters[0].revision, 2);
    }

    #[test]
    fn same_display_name_keeps_independent_ids() {
        let (_dir, path) = path();
        let loaded = load_store(&path).unwrap();
        let draft = GuideCharacterDraft {
            id: None,
            display_name: "千反田爱瑠".into(),
            work_title: "原创".into(),
            character_version: String::new(),
            description: "custom".into(),
            avatar_asset_id: None,
            ink_color: "#112233".into(),
            personality: "a".into(),
            reading_habits: "b".into(),
            expression_style: "c".into(),
            avoidances: "d".into(),
            example_notes: vec!["e".into()],
        };
        let saved = save_character(&path, loaded.store_revision, draft).unwrap();
        let custom = saved
            .characters
            .iter()
            .find(|item| item.id != PRESET_CHITANDA && item.display_name == "千反田爱瑠")
            .unwrap();
        assert_ne!(custom.id, PRESET_CHITANDA);
        assert!(custom.preset_id.is_none());
    }

    #[test]
    fn delete_does_not_reuse_id_and_drops_from_default_cast() {
        let (_dir, path) = path();
        let loaded = load_store(&path).unwrap();
        let saved = delete_character(&path, loaded.store_revision, PRESET_JOTARO).unwrap();
        assert!(saved.characters.iter().all(|item| item.id != PRESET_JOTARO));
        assert!(!saved
            .default_character_ids
            .contains(&PRESET_JOTARO.to_string()));
        let file = load_store_file(&path).unwrap();
        assert!(file.retired_ids.iter().any(|id| id == PRESET_JOTARO));
        let again = save_character(
            &path,
            saved.store_revision,
            GuideCharacterDraft {
                id: Some(PRESET_JOTARO.to_string()),
                display_name: "ghost".into(),
                work_title: String::new(),
                character_version: String::new(),
                description: String::new(),
                avatar_asset_id: None,
                ink_color: "#35538A".into(),
                personality: String::new(),
                reading_habits: String::new(),
                expression_style: String::new(),
                avoidances: String::new(),
                example_notes: Vec::new(),
            },
        );
        assert!(again.unwrap_err().contains("停用"));
    }

    #[test]
    fn restore_preset_only_touches_that_character() {
        let (_dir, path) = path();
        let loaded = load_store(&path).unwrap();
        let mut draft = draft_from(&loaded.characters[0]);
        draft.personality = "changed".into();
        let saved = save_character(&path, loaded.store_revision, draft).unwrap();
        let oreki_before = saved
            .characters
            .iter()
            .find(|item| item.id == PRESET_OREKI)
            .unwrap()
            .clone();
        let restored =
            restore_character_preset(&path, saved.store_revision, PRESET_CHITANDA).unwrap();
        let chitanda = restored
            .characters
            .iter()
            .find(|item| item.id == PRESET_CHITANDA)
            .unwrap();
        assert!(chitanda.personality.contains("真诚"));
        assert_eq!(
            restored
                .characters
                .iter()
                .find(|item| item.id == PRESET_OREKI)
                .unwrap()
                .personality,
            oreki_before.personality
        );
    }

    #[test]
    fn concurrent_revision_does_not_clobber() {
        let (_dir, path) = path();
        let loaded = load_store(&path).unwrap();
        let mut draft = draft_from(&loaded.characters[0]);
        draft.description = "first window".into();
        save_character(&path, loaded.store_revision, draft.clone()).unwrap();
        draft.description = "second window".into();
        let err = save_character(&path, loaded.store_revision, draft).unwrap_err();
        assert!(err.contains("其他窗口"));
        let latest = load_store(&path).unwrap();
        assert_eq!(latest.characters[0].description, "first window");
    }

    #[test]
    fn corrupt_file_is_not_reset() {
        let (_dir, path) = path();
        fs::write(&path, "{not-json").unwrap();
        let err = load_store(&path).unwrap_err();
        assert!(err.contains("已保留原文件"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "{not-json");
    }

    fn png(width: u32) -> Vec<u8> {
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&32u32.to_be_bytes());
        bytes
    }

    #[test]
    fn saving_and_retiring_roles_never_deletes_unrelated_directories() {
        let (_dir, path) = path();
        let loaded = load_store(&path).unwrap();
        let notes = path.join("my-reference-material");
        fs::create_dir_all(&notes).unwrap();
        fs::write(notes.join("notes.md"), "personal work").unwrap();
        let saved = delete_character(&path, loaded.store_revision, PRESET_CHITANDA).unwrap();
        assert_eq!(
            fs::read_to_string(notes.join("notes.md")).unwrap(),
            "personal work"
        );
        assert!(!saved
            .characters
            .iter()
            .any(|item| item.id == PRESET_CHITANDA));
        assert!(!load_store(&path)
            .unwrap()
            .characters
            .iter()
            .any(|item| item.id == PRESET_CHITANDA));
        assert!(path
            .join(folder_name_for_id(PRESET_CHITANDA))
            .join("character.json")
            .exists());
    }

    #[test]
    fn avatar_removal_and_external_replacement_keep_hash_and_display_consistent() {
        let (dir, path) = path();
        let loaded = load_store(&path).unwrap();
        let asset =
            crate::guide_character_assets::import_workspace_avatar(dir.path(), &png(32)).unwrap();
        let mut draft = draft_from(&loaded.characters[0]);
        draft.avatar_asset_id = Some(asset.asset_id.clone());
        let saved = save_character(&path, loaded.store_revision, draft).unwrap();
        let avatar = path
            .join(folder_name_for_id(PRESET_CHITANDA))
            .join("avatar.png");
        assert!(saved.characters[0].avatar_rel_path.is_some());
        assert_eq!(
            saved.characters[0].avatar_asset_id.as_deref(),
            Some(asset.asset_id.as_str())
        );
        fs::write(&avatar, png(64)).unwrap();
        let externally_changed = load_store(&path).unwrap();
        let replacement_hash = crate::guide_character_assets::asset_id_for(&png(64));
        assert_eq!(
            externally_changed.characters[0].avatar_asset_id.as_deref(),
            Some(replacement_hash.as_str())
        );
        let mut draft = draft_from(&externally_changed.characters[0]);
        draft.avatar_asset_id = None;
        let removed = save_character(&path, externally_changed.store_revision, draft).unwrap();
        assert!(removed.characters[0].avatar_asset_id.is_none());
        assert!(removed.characters[0].avatar_rel_path.is_none());
        assert!(!avatar.exists());
        assert!(load_store(&path).unwrap().characters[0]
            .avatar_rel_path
            .is_none());
        assert!(
            crate::guide_character_assets::read_workspace_avatar(dir.path(), &asset.asset_id)
                .is_ok()
        );
    }

    #[test]
    fn interrupted_multi_file_save_replays_one_complete_revision() {
        let (_dir, path) = path();
        let original = load_store_file(&path).unwrap();
        let mut next = original.clone();
        next.store_revision += 1;
        next.characters[0].description = "first changed role".into();
        next.characters[1].description = "second changed role".into();
        let pending = path.join(PENDING_STORE);
        atomic_write(&pending, &serde_json::to_vec_pretty(&next).unwrap()).unwrap();
        // Simulate an exit after only the first role was replaced, before cast.json.
        atomic_write(
            &path
                .join(folder_name_for_id(PRESET_CHITANDA))
                .join("character.json"),
            &serde_json::to_vec_pretty(&next.characters[0]).unwrap(),
        )
        .unwrap();
        let recovered = load_store_file(&path).unwrap();
        assert_eq!(recovered.store_revision, next.store_revision);
        assert_eq!(recovered.characters[0].description, "first changed role");
        assert_eq!(recovered.characters[1].description, "second changed role");
        assert!(!pending.exists());
        assert_eq!(load_store_file(&path).unwrap(), recovered);
    }

    #[test]
    fn legacy_backup_and_role_backup_restore_without_factory_reset() {
        let (dir, path) = path();
        let loaded = load_store_file(&path).unwrap();
        let mut legacy = loaded.clone();
        legacy.store_revision = 42;
        legacy.characters[0].description = "preserve my edits".into();
        let legacy_path = dir.path().join("guide-characters.json");
        fs::write(
            crate::guide_character_assets::backup_path(&legacy_path),
            serde_json::to_vec(&legacy).unwrap(),
        )
        .unwrap();
        assert_eq!(load_store_file(&legacy_path).unwrap(), legacy);
        let role_path = path
            .join(folder_name_for_id(PRESET_CHITANDA))
            .join("character.json");
        fs::rename(
            &role_path,
            crate::guide_character_assets::backup_path(&role_path),
        )
        .unwrap();
        let cast_path = path.join("cast.json");
        fs::rename(
            &cast_path,
            crate::guide_character_assets::backup_path(&cast_path),
        )
        .unwrap();
        assert_eq!(load_store_file(&path).unwrap(), loaded);
    }

    #[test]
    fn concurrent_editors_cannot_both_commit_the_same_revision() {
        let (_dir, path) = path();
        let loaded = load_store(&path).unwrap();
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut handles = Vec::new();
        for name in ["editor one", "editor two"] {
            let mut draft = draft_from(&loaded.characters[0]);
            draft.description = name.into();
            let path = path.clone();
            let barrier = barrier.clone();
            let revision = loaded.store_revision;
            handles.push(std::thread::spawn(move || {
                barrier.wait();
                save_character(&path, revision, draft)
            }));
        }
        barrier.wait();
        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert!(results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .all(|error| error.contains("其他窗口")));
        assert_eq!(
            load_store(&path).unwrap().store_revision,
            loaded.store_revision + 1
        );
    }

    #[test]
    fn missing_avatar_does_not_poison_the_next_store_read() {
        let (_dir, path) = path();
        let original = load_store(&path).unwrap();
        let mut draft = draft_from(&original.characters[0]);
        draft.avatar_asset_id = Some("a".repeat(64));
        assert!(save_character(&path, original.store_revision, draft).is_err());
        assert!(!path.join(PENDING_STORE).exists());
        assert_eq!(load_store(&path).unwrap(), original);
    }

    #[test]
    fn malformed_role_identifier_cannot_write_outside_characters() {
        let (dir, path) = path();
        load_store(&path).unwrap();
        let role_path = path
            .join(folder_name_for_id(PRESET_CHITANDA))
            .join("character.json");
        let mut value: serde_json::Value =
            serde_json::from_slice(&fs::read(&role_path).unwrap()).unwrap();
        value["id"] = serde_json::json!("../outside");
        fs::write(&role_path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(load_store(&path).is_err());
        assert!(!dir.path().join("outside").exists());
    }

    #[test]
    fn save_and_restore_preset_casts_works() {
        let (_dir, path) = path();
        let original = load_store(&path).unwrap();
        assert_eq!(original.preset_casts.len(), 3);

        let custom_cast = GuidePresetCast {
            id: "custom_thesis".to_string(),
            name: "论文答辩组".to_string(),
            character_ids: vec![PRESET_CHITANDA.to_string(), PRESET_CONAN.to_string()],
        };

        let updated =
            save_preset_casts(&path, original.store_revision, vec![custom_cast.clone()]).unwrap();
        assert_eq!(updated.preset_casts.len(), 1);
        assert_eq!(updated.preset_casts[0].name, "论文答辩组");

        // Empty name should fail
        let invalid = GuidePresetCast {
            id: "bad".to_string(),
            name: "   ".to_string(),
            character_ids: vec![PRESET_CHITANDA.to_string()],
        };
        assert!(save_preset_casts(&path, updated.store_revision, vec![invalid]).is_err());

        // Restore factory presets
        let restored = restore_factory_preset_casts(&path, updated.store_revision).unwrap();
        assert_eq!(restored.preset_casts.len(), 3);
        assert_eq!(restored.preset_casts[0].name, "日常阅读");
    }

    fn draft_from(character: &GuideCharacter) -> GuideCharacterDraft {
        GuideCharacterDraft {
            id: Some(character.id.clone()),
            display_name: character.display_name.clone(),
            work_title: character.work_title.clone(),
            character_version: character.character_version.clone(),
            description: character.description.clone(),
            avatar_asset_id: character.avatar_asset_id.clone(),
            ink_color: character.ink_color.clone(),
            personality: character.personality.clone(),
            reading_habits: character.reading_habits.clone(),
            expression_style: character.expression_style.clone(),
            avoidances: character.avoidances.clone(),
            example_notes: character.example_notes.clone(),
        }
    }
}

/// Translate unchanged factory fields only when freezing a new cast. Never write this view to disk.
pub fn localized_for_generation(
    store: &GuideCharacterStoreFile,
    locale: crate::ui_locale::UiLocale,
) -> GuideCharacterStoreFile {
    if locale != crate::ui_locale::UiLocale::En {
        return store.clone();
    }
    let mut next = store.clone();
    for character in &mut next.characters {
        let pair = match character.preset_id.as_deref().unwrap_or("") {
            "chitanda" => Some((
                include_str!("../prompts/guide-characters/chitanda.json"),
                include_str!("../prompts/guide-characters/en/chitanda.json"),
            )),
            "oreki" => Some((
                include_str!("../prompts/guide-characters/oreki.json"),
                include_str!("../prompts/guide-characters/en/oreki.json"),
            )),
            "frieren" => Some((
                include_str!("../prompts/guide-characters/frieren.json"),
                include_str!("../prompts/guide-characters/en/frieren.json"),
            )),
            "jotaro" => Some((
                include_str!("../prompts/guide-characters/jotaro.json"),
                include_str!("../prompts/guide-characters/en/jotaro.json"),
            )),
            "conan" => Some((
                include_str!("../prompts/guide-characters/conan.json"),
                include_str!("../prompts/guide-characters/en/conan.json"),
            )),
            _ => None,
        };
        let Some((zh, en)) = pair else {
            continue;
        };
        let (Ok(zh), Ok(en), Ok(mut value)) = (
            serde_json::from_str::<serde_json::Value>(zh),
            serde_json::from_str::<serde_json::Value>(en),
            serde_json::to_value(&*character),
        ) else {
            continue;
        };
        for key in [
            "displayName",
            "workTitle",
            "characterVersion",
            "description",
            "personality",
            "readingHabits",
            "expressionStyle",
            "avoidances",
            "exampleNotes",
        ] {
            if value[key] == zh[key] {
                value[key] = en[key].clone();
            }
        }
        if let Ok(localized) = serde_json::from_value(value) {
            *character = localized;
        }
    }
    next
}

#[cfg(test)]
mod locale_tests {
    use super::*;
    #[test]
    fn new_cast_localizes_factory_fields_without_changing_custom_or_stored_text() {
        let mut store = factory_store();
        store.characters[0].personality = "用户自定义性格".into();
        let original = store.clone();
        let localized = localized_for_generation(&store, crate::ui_locale::UiLocale::En);
        assert_eq!(localized.characters[0].display_name, "Eru Chitanda");
        assert_eq!(localized.characters[0].personality, "用户自定义性格");
        assert_eq!(store, original);
        assert_eq!(
            localized_for_generation(&store, crate::ui_locale::UiLocale::ZhCn),
            original
        );
    }
}
