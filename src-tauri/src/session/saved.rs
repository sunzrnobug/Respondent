use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SessionTurn {
    pub transcript: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SavedSession {
    pub id: String,
    pub title: String,
    pub date: String,
    pub started_at: String,
    pub ended_at: String,
    pub turns: Vec<SessionTurn>,
    pub system_messages: Vec<String>,
}
