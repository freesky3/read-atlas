#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuidePersona {
    pub id: &'static str,
    pub display_name: &'static str,
    pub color: &'static str,
}

pub const PERSONA_ALIN: GuidePersona = GuidePersona {
    id: "alin",
    display_name: "阿林",
    color: "#2458a8",
};

pub const PERSONA_LAOZHOU: GuidePersona = GuidePersona {
    id: "laozhou",
    display_name: "老周",
    color: "#b42318",
};

pub const PERSONA_XIAXIA: GuidePersona = GuidePersona {
    id: "xiaxia",
    display_name: "小夏",
    color: "#9a6700",
};

pub const GUIDE_PERSONAS: [GuidePersona; 3] = [PERSONA_ALIN, PERSONA_LAOZHOU, PERSONA_XIAXIA];

pub fn is_known_speaker(id: &str) -> bool {
    normalize_speaker(id).is_some()
}

pub fn normalize_speaker(id: &str) -> Option<&'static str> {
    let trimmed = id.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "阿林" {
        return Some("alin");
    }
    if trimmed == "老周" {
        return Some("laozhou");
    }
    if trimmed == "小夏" {
        return Some("xiaxia");
    }
    let compact = trimmed
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect::<String>();
    match compact.as_str() {
        "alin" | "alinn" => Some("alin"),
        "laozhou" | "lao" => Some("laozhou"),
        "xiaxia" | "xia" => Some("xiaxia"),
        _ => GUIDE_PERSONAS
            .iter()
            .map(|persona| persona.id)
            .find(|known| *known == compact),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn three_frozen_speakers_have_stable_ids() {
        assert_eq!(
            GUIDE_PERSONAS.map(|persona| persona.id),
            ["alin", "laozhou", "xiaxia"]
        );
        assert!(is_known_speaker("alin"));
        assert_eq!(normalize_speaker("阿林"), Some("alin"));
        assert_eq!(normalize_speaker("老周"), Some("laozhou"));
        assert_eq!(normalize_speaker("小夏"), Some("xiaxia"));
        assert!(!is_known_speaker("guest"));
    }
}
