//! Automatic update configuration shared by both front ends.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoUpdateMode {
    Notify,
    Next,
    #[default]
    Latest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdatePolicy {
    pub mode: AutoUpdateMode,
    pub max_steps: u8,
    pub pause_on_failure: bool,
}

impl Default for UpdatePolicy {
    fn default() -> Self {
        Self {
            mode: AutoUpdateMode::Latest,
            max_steps: 1,
            pause_on_failure: true,
        }
    }
}
