//! One stable check document shared by the CLI and desktop.
use chrono::{DateTime, Utc};
use serde::Serialize;

use super::{AppStatus, AppView};
use crate::{ErrorInfo, Result};

#[derive(Debug, Clone, Serialize)]
pub struct CheckSummary {
    pub results: Vec<CheckRecord>,
    pub checked: usize,
    pub with_updates: usize,
    /// Compatibility projection used by desktop clients.
    pub failed: Vec<CheckFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckFailure {
    pub id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CheckLatest {
    pub id: String,
    pub title: String,
    pub date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum CheckRecord {
    Success {
        id: String,
        ok: bool,
        status: AppStatus,
        pending_count: u32,
        latest: Option<CheckLatest>,
    },
    Failure {
        id: String,
        ok: bool,
        error: ErrorInfo,
    },
}

impl CheckSummary {
    #[must_use]
    pub fn from_results(results: &[(String, Result<AppView>)]) -> Self {
        let mut failed = Vec::new();
        let mut with_updates = 0;
        let records = results
            .iter()
            .map(|(id, result)| match result {
                Ok(view) => {
                    with_updates += usize::from(view.has_update());
                    CheckRecord::Success {
                        id: id.clone(),
                        ok: true,
                        status: view.status,
                        pending_count: view.pending_count,
                        latest: view
                            .last_check
                            .as_ref()
                            .and_then(|check| check.latest.as_ref())
                            .map(|latest| CheckLatest {
                                id: latest.id.clone(),
                                title: latest.title.clone(),
                                date: latest.date,
                            }),
                    }
                }
                Err(error) => {
                    failed.push(CheckFailure {
                        id: id.clone(),
                        message: error.to_string(),
                    });
                    CheckRecord::Failure {
                        id: id.clone(),
                        ok: false,
                        error: ErrorInfo::from(error),
                    }
                }
            })
            .collect();
        Self {
            results: records,
            checked: results.len(),
            with_updates,
            failed,
        }
    }
}
