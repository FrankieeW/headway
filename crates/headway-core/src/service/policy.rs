use super::{AppView, Headway};
use crate::policy::AutoUpdateMode;
use crate::state::AppState;
use crate::{CancellationToken, Error, EventSink, Result, Target, UpdateOutcome, UpdateStatus};

impl Headway {
    /// Applies one bounded background cycle, preserving failures as errors.
    pub async fn auto_update(
        &self,
        id: &str,
        sink: &dyn EventSink,
        cancel: &CancellationToken,
    ) -> Result<Vec<UpdateOutcome>> {
        let app = self.store.config()?.app(id)?.clone();
        let policy = app.update_policy;
        if !app.auto_update || self.get(id)?.auto_update_paused {
            return Ok(Vec::new());
        }
        let (target, steps) = match policy.mode {
            AutoUpdateMode::Notify => return Ok(Vec::new()),
            AutoUpdateMode::Next => (Target::Next, policy.max_steps),
            AutoUpdateMode::Latest => (Target::Latest, 1),
        };
        let result = async {
            let mut outcomes = Vec::new();
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
                let view = self.check(id).await?;
                if view.installed.is_none()
                    || !view
                        .last_check
                        .as_ref()
                        .is_some_and(|check| check.status == UpdateStatus::UpdateAvailable)
                {
                    break;
                }
                // Check can await the network; settings saved during it govern admission.
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
            Ok(outcomes)
        }
        .await;
        let current = self.store.config()?.app(id)?.clone();
        if let Err(error) = &result
            && current.update_policy.pause_on_failure
            && current.update_policy.mode != AutoUpdateMode::Notify
            && current.source == app.source
            && !matches!(error, Error::Cancelled | Error::Busy(_))
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
        result
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
