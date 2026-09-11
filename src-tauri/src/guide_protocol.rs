use serde_json::{json, Value};

pub const GUIDE_PROTOCOL_VERSION: &str = "reading-guide-desktop-v1";
pub const GUIDE_PROMPT_VERSION: &str = "reading-guide-desktop-v1";
pub const GUIDE_JOB_KIND: &str = "reading_guide";
pub const GUIDE_LANGUAGE: &str = "en";
pub const GUIDE_BATCH_TARGET_PAGES: i64 = 6;
pub const GUIDE_BATCH_HARD_MAX_PAGES: i64 = 8;
pub const GUIDE_EXCERPT_CHARS: usize = 240;
pub const GUIDE_MAX_BLOCKS_PER_PAGE: usize = 8;
pub const GUIDE_PROTOCOL_V2: &str = "reading-guide-desktop-v2";
pub const GUIDE_PROMPT_V2: &str = "reading-guide-desktop-v2";
pub const GUIDE_MEMO_PROTOCOL: &str = "reading-guide-memo-v1";
pub const GUIDE_LANGUAGE_ZH: &str = "zh-CN";
pub const GUIDE_V2_BATCH_CHAR_BUDGET: usize = 24_000;
pub const GUIDE_V2_NOTE_MIN_BODY: usize = 8;
pub const GUIDE_STAGE1_MAX_CALLS: u32 = 2;
pub const GUIDE_BATCH_MAX_LOGIC_CALLS: u32 = 3;

pub fn is_v2_protocol(value: &str) -> bool {
    value == GUIDE_PROTOCOL_V2
}

pub fn context_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": true,
        "required": ["thesis", "sections"],
        "properties": {
            "thesis": {"type": "string"},
            "sections": {"type": "array"},
            "argumentFlow": {"type": "array"},
            "confusingPoints": {"type": "array"}
        }
    })
}

pub fn inks_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": true,
        "required": ["inks"],
        "properties": {
            "inks": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": true,
                    "required": ["id", "kind", "speakerId"],
                    "properties": {
                        "id": {"type": "string"},
                        "kind": {"type": "string"},
                        "speakerId": {"type": "string"},
                        "weight": {"type": "string"},
                        "parentId": {"type": "string"},
                        "body": {"type": "string"},
                        "blockId": {"type": "string"},
                        "pageNumber": {"type": "integer"},
                        "anchor": {
                            "type": "object",
                            "additionalProperties": true,
                            "properties": {
                                "blockId": {"type": "string"},
                                "pageNumber": {"type": "integer"},
                                "blockType": {"type": "string"},
                                "bbox": {
                                    "type": "array",
                                    "items": {"type": "number"}
                                }
                            }
                        }
                    }
                }
            }
        }
    })
}

pub fn memo_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["schemaVersion", "documentFocus", "spans", "observations", "connections", "pageHints", "limitations"],
        "properties": {
            "schemaVersion": {"type": "string"},
            "documentFocus": {"type": "string"},
            "spans": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["spanId", "heading", "pageStart", "pageEnd", "purposeMarkdown"],
                    "properties": {
                        "spanId": {"type": "string"},
                        "heading": {"type": "string"},
                        "pageStart": {"type": "integer"},
                        "pageEnd": {"type": "integer"},
                        "purposeMarkdown": {"type": "string"}
                    }
                }
            },
            "observations": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["observationId", "location", "observationMarkdown", "basis"],
                    "properties": {
                        "observationId": {"type": "string"},
                        "location": {"$ref": "#/$defs/location"},
                        "observationMarkdown": {"type": "string"},
                        "basis": {"type": "string", "enum": ["explicit", "inference", "reader_reaction"]}
                    }
                }
            },
            "connections": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["connectionId", "from", "to", "relationMarkdown"],
                    "properties": {
                        "connectionId": {"type": "string"},
                        "from": {"$ref": "#/$defs/location"},
                        "to": {"$ref": "#/$defs/location"},
                        "relationMarkdown": {"type": "string"}
                    }
                }
            },
            "pageHints": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["pageNumber", "kind", "note"],
                    "properties": {
                        "pageNumber": {"type": "integer"},
                        "kind": {"type": "string"},
                        "note": {"type": "string"}
                    }
                }
            },
            "limitations": {
                "type": "array",
                "items": {"type": "string"}
            }
        },
        "$defs": {
            "location": {
                "type": "object",
                "additionalProperties": false,
                "required": ["pageNumber", "blockIds"],
                "properties": {
                    "pageNumber": {"type": "integer"},
                    "blockIds": {
                        "type": "array",
                        "items": {"type": "string"}
                    }
                }
            }
        }
    })
}

pub fn inks_schema_v2(speaker_ids: &[String]) -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["inks"],
        "properties": {
            "inks": {
                "type": "array",
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["id", "kind", "speakerId", "blockId", "weight", "body", "parentId"],
                    "properties": {
                        "id": {"type": "string", "minLength": 1},
                        "kind": {"type": "string", "enum": ["trace", "note", "reply"]},
                        "speakerId": {"type": "string", "enum": speaker_ids},
                        "blockId": {"type": "string", "minLength": 1},
                        "weight": {"type": ["string", "null"], "enum": ["line", "short", null]},
                        "body": {"type": "string"},
                        "parentId": {"type": ["string", "null"]}
                    }
                }
            }
        }
    })
}
