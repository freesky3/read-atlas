use crate::guide_character_settings::{
    GuideCharacter, GuideCharacterDraft, GuideCharacterStoreFile, PRESET_CHITANDA, PRESET_CONAN,
    PRESET_FRIEREN, PRESET_JOTARO, PRESET_OREKI,
};
use crate::guide_personas::{GUIDE_PERSONAS, PERSONA_ALIN, PERSONA_LAOZHOU, PERSONA_XIAXIA};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CAST_SNAPSHOT_SCHEMA: u32 = 1;
pub const CAST_RULES_VERSION: &str = "guide-cast-rules-v1";
pub const V1_HISTORICAL_PERSONAS_VERSION: &str = "reading-guide-v1-personas";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideCastMember {
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
    pub workspace_avatar_path: Option<String>,
    pub ink_color: String,
    pub personality: String,
    pub reading_habits: String,
    pub expression_style: String,
    pub avoidances: String,
    #[serde(default)]
    pub example_notes: Vec<String>,
    pub preset_id: Option<String>,
    pub preset_version: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuideCastSnapshot {
    pub schema_version: u32,
    pub characters: Vec<GuideCastMember>,
    pub order: Vec<String>,
    pub relation_hints: Vec<String>,
    pub rules_version: String,
    pub cast_digest: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoricalPersona {
    pub id: String,
    pub display_name: String,
    pub color: String,
}

pub fn v1_historical_personas() -> Vec<HistoricalPersona> {
    GUIDE_PERSONAS
        .iter()
        .map(|persona| HistoricalPersona {
            id: persona.id.to_string(),
            display_name: persona.display_name.to_string(),
            color: persona.color.to_string(),
        })
        .collect()
}

pub fn is_v1_speaker(id: &str) -> bool {
    matches!(id, "alin" | "laozhou" | "xiaxia")
}

pub fn v1_persona_display(id: &str) -> Option<HistoricalPersona> {
    let persona = match id {
        "alin" => PERSONA_ALIN,
        "laozhou" => PERSONA_LAOZHOU,
        "xiaxia" => PERSONA_XIAXIA,
        _ => return None,
    };
    Some(HistoricalPersona {
        id: persona.id.to_string(),
        display_name: persona.display_name.to_string(),
        color: persona.color.to_string(),
    })
}

pub fn snapshot_from_store(
    store: &GuideCharacterStoreFile,
    ordered_ids: &[String],
    workspace_avatar_paths: &[(String, String)],
) -> Result<GuideCastSnapshot, String> {
    if ordered_ids.is_empty() {
        return Err("至少选择一位批注角色".to_string());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut members = Vec::new();
    for id in ordered_ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        let character = store
            .characters
            .iter()
            .find(|item| item.id == *id)
            .ok_or_else(|| format!("阵容包含不可用人物：{id}"))?;
        let workspace_avatar_path = workspace_avatar_paths
            .iter()
            .find(|(asset_id, _)| character.avatar_asset_id.as_deref() == Some(asset_id.as_str()))
            .map(|(_, path)| path.clone());
        members.push(member_from_character(character, workspace_avatar_path));
    }
    if members.is_empty() {
        return Err("至少选择一位批注角色".to_string());
    }
    let order: Vec<String> = members.iter().map(|item| item.id.clone()).collect();
    let relation_hints = relation_hints_for(&order);
    let mut snapshot = GuideCastSnapshot {
        schema_version: CAST_SNAPSHOT_SCHEMA,
        characters: members,
        order,
        relation_hints,
        rules_version: CAST_RULES_VERSION.to_string(),
        cast_digest: String::new(),
    };
    snapshot.cast_digest = digest_snapshot(&snapshot);
    Ok(snapshot)
}

pub fn snapshot_from_drafts(drafts: &[GuideCharacterDraft]) -> Result<GuideCastSnapshot, String> {
    if drafts.is_empty() || drafts.len() > 3 {
        return Err("试写需要一至三位角色草稿".to_string());
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut members = Vec::new();
    for (index, draft) in drafts.iter().enumerate() {
        let id = draft
            .id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("preview:{}", index + 1));
        if !seen.insert(id.clone()) {
            continue;
        }
        members.push(GuideCastMember {
            id,
            revision: 0,
            display_name: draft.display_name.clone(),
            work_title: draft.work_title.clone(),
            character_version: draft.character_version.clone(),
            description: draft.description.clone(),
            avatar_asset_id: draft.avatar_asset_id.clone(),
            workspace_avatar_path: None,
            ink_color: draft.ink_color.clone(),
            personality: draft.personality.clone(),
            reading_habits: draft.reading_habits.clone(),
            expression_style: draft.expression_style.clone(),
            avoidances: draft.avoidances.clone(),
            example_notes: draft.example_notes.clone(),
            preset_id: None,
            preset_version: None,
        });
    }
    if members.is_empty() {
        return Err("试写至少需要一位角色草稿".to_string());
    }
    let order: Vec<String> = members.iter().map(|item| item.id.clone()).collect();
    let relation_hints = relation_hints_for(&order);
    let mut snapshot = GuideCastSnapshot {
        schema_version: CAST_SNAPSHOT_SCHEMA,
        characters: members,
        order,
        relation_hints,
        rules_version: CAST_RULES_VERSION.to_string(),
        cast_digest: String::new(),
    };
    snapshot.cast_digest = digest_snapshot(&snapshot);
    Ok(snapshot)
}

pub fn speaker_ids(snapshot: &GuideCastSnapshot) -> Vec<String> {
    snapshot.order.clone()
}

pub fn is_cast_speaker(snapshot: &GuideCastSnapshot, id: &str) -> bool {
    snapshot.order.iter().any(|item| item == id)
}

pub fn member_by_id<'a>(snapshot: &'a GuideCastSnapshot, id: &str) -> Option<&'a GuideCastMember> {
    snapshot.characters.iter().find(|item| item.id == id)
}

fn member_from_character(
    character: &GuideCharacter,
    workspace_avatar_path: Option<String>,
) -> GuideCastMember {
    GuideCastMember {
        id: character.id.clone(),
        revision: character.revision,
        display_name: character.display_name.clone(),
        work_title: character.work_title.clone(),
        character_version: character.character_version.clone(),
        description: character.description.clone(),
        avatar_asset_id: character.avatar_asset_id.clone(),
        workspace_avatar_path,
        ink_color: character.ink_color.clone(),
        personality: character.personality.clone(),
        reading_habits: character.reading_habits.clone(),
        expression_style: character.expression_style.clone(),
        avoidances: character.avoidances.clone(),
        example_notes: character.example_notes.clone(),
        preset_id: character.preset_id.clone(),
        preset_version: character.preset_version,
    }
}

pub fn relation_hints_for(order: &[String]) -> Vec<String> {
    let has = |id: &str| order.iter().any(|item| item == id);
    let mut hints = Vec::new();
    if has(PRESET_CHITANDA) && has(PRESET_OREKI) {
        hints.push(
            "千反田与折木可以自然互称；她抓住一个小地方时，折木偶尔接通，但两人都可独立落笔。"
                .to_string(),
        );
    }
    if has(PRESET_OREKI) && has(PRESET_CONAN) {
        hints.push(
            "折木找足够清楚的理解路径，柯南核更多证据并保留替代解释；不要重复写同一种逻辑评论。"
                .to_string(),
        );
    }
    if has(PRESET_FRIEREN) && order.len() > 1 {
        hints.push("芙莉莲可以在他人关注主结果时记下小设计，或平静补上被忽略的条件。".to_string());
    }
    hints.push("缺席人物不收到回复，也不要反复召唤未选择的角色。".to_string());
    hints.push("跨作品人物只有阅读搭档关系，不编造原作中不存在的相识历史。".to_string());
    hints
}

pub fn cast_prompt_block(snapshot: &GuideCastSnapshot) -> String {
    let mut lines = Vec::new();
    lines.push("本次阵容（只能使用下列 speakerId；显示名用于正文称呼）：".to_string());
    for member in &snapshot.characters {
        lines.push(format!(
            "- speakerId: {} / 显示名: {} / 作品: {} / 人物版本: {}",
            member.id,
            member.display_name,
            empty_dash(&member.work_title),
            empty_dash(&member.character_version)
        ));
        lines.push(format!("  一句话：{}", empty_dash(&member.description)));
        lines.push(format!("  核心性格：{}", member.personality));
        lines.push(format!("  阅读习惯：{}", member.reading_habits));
        lines.push(format!("  表达习惯：{}", member.expression_style));
        lines.push(format!("  避免：{}", member.avoidances));
        if !member.example_notes.is_empty() {
            lines.push("  风格样例（不是本文证据）：".to_string());
            for note in &member.example_notes {
                lines.push(format!("    - {note}"));
            }
        }
    }
    if !snapshot.relation_hints.is_empty() {
        lines.push("关系提示：".to_string());
        for hint in &snapshot.relation_hints {
            lines.push(format!("- {hint}"));
        }
    }
    lines.join("\n")
}

fn empty_dash(value: &str) -> &str {
    if value.trim().is_empty() {
        "—"
    } else {
        value
    }
}

fn digest_snapshot(snapshot: &GuideCastSnapshot) -> String {
    let payload = serde_json::json!({
        "schemaVersion": snapshot.schema_version,
        "characters": snapshot.characters,
        "order": snapshot.order,
        "relationHints": snapshot.relation_hints,
        "rulesVersion": snapshot.rules_version,
    });
    let bytes = serde_json::to_vec(&payload).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guide_character_settings::factory_store;

    #[test]
    fn snapshot_copies_full_cards_not_ids_only() {
        let store = factory_store();
        let snapshot = snapshot_from_store(
            &store,
            &[PRESET_CHITANDA.to_string(), PRESET_OREKI.to_string()],
            &[],
        )
        .unwrap();
        assert_eq!(snapshot.order.len(), 2);
        assert!(snapshot.characters[0].personality.contains("真诚"));
        assert!(!snapshot.cast_digest.is_empty());
        let mutated = {
            let mut store = store.clone();
            store.characters[0].personality = "changed later".into();
            store
        };
        let later = snapshot_from_store(
            &mutated,
            &[PRESET_CHITANDA.to_string(), PRESET_OREKI.to_string()],
            &[],
        )
        .unwrap();
        assert_ne!(later.cast_digest, snapshot.cast_digest);
        assert!(snapshot.characters[0].personality.contains("真诚"));
    }

    #[test]
    fn v1_ids_are_never_mapped_to_anime_presets() {
        assert!(is_v1_speaker("alin"));
        assert!(!is_v1_speaker(PRESET_CHITANDA));
        assert_eq!(v1_persona_display("alin").unwrap().display_name, "阿林");
        assert!(v1_persona_display(PRESET_CHITANDA).is_none());
    }

    #[test]
    fn unknown_cast_member_is_rejected() {
        let store = factory_store();
        let err = snapshot_from_store(&store, &["missing".into()], &[]).unwrap_err();
        assert!(err.contains("不可用"));
    }
}
