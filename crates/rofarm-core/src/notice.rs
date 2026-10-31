use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeSeverity {
    Info,
    Success,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoticeAction {
    pub action_id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notice {
    pub id: Uuid,
    pub at: DateTime<Utc>,
    pub severity: NoticeSeverity,
    pub title: String,
    pub body: String,
    pub source: String,
    pub actions: Vec<NoticeAction>,
    pub read: bool,
    pub dedupe_key: Option<String>,
}

impl Notice {
    pub fn new(severity: NoticeSeverity, title: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4(),
            at: Utc::now(),
            severity,
            title: title.into(),
            body: body.into(),
            source: "rofarm".into(),
            actions: Vec::new(),
            read: false,
            dedupe_key: None,
        }
    }

    pub fn with_action(mut self, action_id: impl Into<String>, label: impl Into<String>) -> Self {
        self.actions.push(NoticeAction { action_id: action_id.into(), label: label.into() });
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = source.into();
        self
    }

    pub fn with_dedupe(mut self, key: impl Into<String>) -> Self {
        self.dedupe_key = Some(key.into());
        self
    }
}
