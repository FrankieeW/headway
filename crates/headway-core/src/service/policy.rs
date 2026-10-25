use super::{AppView, Headway};
use crate::policy::AutoUpdateMode;
use crate::state::AppState;
use crate::{
    CancellationToken, Error, ErrorInfo, EventSink, Result, Target, UpdateOutcome, UpdateStatus,
};

/// A bounded cycle retains successful installations even if a later step fails.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AutoUpdateReport {
    pub outcomes: Vec<UpdateOutcome>,
    pub error: Option<ErrorInfo>,
}

impl Headway {
    /// Compatibility API; use `auto_update_report` to retain partial successes.
    pub async fn auto_update(
        &self,
        id: &str,
        sink: &dyn EventSink,
        cancel: &CancellationToken,
    ) -> Result<Vec<UpdateOutcome>> {
        let (outcomes, result) = self.auto_update_cycle(id, sink, cancel).await;
        result.map(|()| outcomes)
    }

    pub async fn auto_update_report(
        &self,
        id: &str,
        sink: &dyn EventSink,
        cancel: &CancellationToken,
    ) -> AutoUpdateReport {
        let (outcomes, result) = self.auto_update_cycle(id, sink, cancel).await;
        AutoUpdateReport {
            outcomes,
            error: result.as_ref().err().map(ErrorInfo::from),
        }
    }

    async fn auto_update_cycle(
        &self,
        id: &str,
        sink: &dyn EventSink,
        cancel: &CancellationToken,
    ) -> (Vec<UpdateOutcome>, Result<()>) {
        let mut outcomes = Vec::new();
        let mut source = None;
        let result = async {
            let app = self.store.config()?.app(id)?.clone();
            source = Some(app.source.clone());
            let policy = app.update_policy;
            if !app.auto_update || self.get(id)?.auto_update_paused {
                return Ok(());
            }
            let (target, steps) = match policy.mode {
                AutoUpdateMode::Notify => return Ok(()),
                AutoUpdateMode::Next => (Target::Next, policy.max_steps),
                AutoUpdateMode::Latest => (Target::Latest, 1),
            };
            for _ in 0..steps {
                if cancel.is_cancelled() {
                    return Err(Error::Cancelled);
                }
                let current = self.get(id)?;
                if !current.app.auto_update
                    || current.auto_update_paused
                    || current.busy
                    || current.app.update_policy.mode != policy.mode
                    || current.app.source != app.source
                    || outcomes.len() >= usize::from(current.app.update_policy.max_steps)
                {
                    break;
                }
                let view = self.check_with_cancel(id, cancel).await?;
                if view.installed.is_none()
                    || !view
                        .last_check
                        .as_ref()
                        .is_some_and(|check| check.status == UpdateStatus::UpdateAvailable)
                {
                    break;
                }
                // A check can await the network; current settings govern admission.
                let admission = self.get(id)?;
                if !admission.app.auto_update
                    || admission.auto_update_paused
                    || admission.app.update_policy.mode != policy.mode
                    || admission.app.source != app.source
                    || outcomes.len() >= usize::from(admission.app.update_policy.max_steps)
                {
                    break;
                }
                let outcome = self.update(id, &target, sink, cancel).await?;
                let changed = outcome.changed;
                outcomes.push(outcome);
                if !changed {
                    break;
                }
            }
            Ok(())
        }
        .await;
        if let Err(error) = &result
            && !matches!(
                error,
                Error::Cancelled | Error::Busy(_) | Error::StoreBusy(_) | Error::ConfigChanged(_)
            )
        {
            let pause = (|| -> Result<()> {
                let current = self.store.config()?.app(id)?.clone();
                if current.update_policy.pause_on_failure
                    && current.update_policy.mode != AutoUpdateMode::Notify
                    && Some(&current.source) == source.as_ref()
                {
                    self.store.update_state(|state| {
                        Ok((
                            state.with_app(id, |state| AppState {
                                auto_update_paused: true,
                                ..state
                            }),
                            (),
                        ))
                    })?;
                }
                Ok(())
            })();
            if let Err(pause_error) = pause {
                eprintln!("headway: could not pause automatic updates: {pause_error}");
            }
        }
        (outcomes, result)
    }

    /// Explicitly re-enables automatic attempts after a failure.
    pub fn resume_auto_update(&self, id: &str) -> Result<AppView> {
        self.store.config()?.app(id)?;
        self.store.update_state(|state| {
            Ok((
                state.with_app(id, |state| AppState {
                    auto_update_paused: false,
                    ..state
                }),
                (),
            ))
        })?;
        self.get(id)
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
