//! Recording host double: IPC tests never open a browser, reveal in Finder, or
//! toggle the real login item.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard};

use crate::host::Host;

/// One call the backend made through the host adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HostCall {
    OpenUrl(String),
    Reveal(String),
}

pub(crate) struct RecordingHost {
    calls: Mutex<Vec<HostCall>>,
    autostart: AtomicBool,
}

impl RecordingHost {
    pub(crate) fn new() -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            autostart: AtomicBool::new(false),
        }
    }

    fn calls(&self) -> MutexGuard<'_, Vec<HostCall>> {
        match self.calls.lock() {
            Ok(calls) => calls,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub(crate) fn recorded(&self) -> Vec<HostCall> {
        self.calls().clone()
    }

    pub(crate) fn autostart(&self) -> bool {
        self.autostart.load(Ordering::SeqCst)
    }
}

impl Host for RecordingHost {
    fn open_url(&self, url: &str) -> Result<(), String> {
        self.calls().push(HostCall::OpenUrl(url.to_owned()));
        Ok(())
    }

    fn reveal(&self, path: &Path) -> Result<(), String> {
        self.calls()
            .push(HostCall::Reveal(path.display().to_string()));
        Ok(())
    }

    fn autostart_enabled(&self) -> Result<bool, String> {
        Ok(self.autostart())
    }

    fn set_autostart(&self, enabled: bool) -> Result<bool, String> {
        self.autostart.store(enabled, Ordering::SeqCst);
        Ok(enabled)
    }
}
