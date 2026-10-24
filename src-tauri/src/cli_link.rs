//! Desktop adapters for the shared CLI-link API.
pub use headway_core::CliStatus;
use headway_core::{Headway, Result};
pub async fn status(hw: &Headway) -> Result<CliStatus> {
    hw.cli_status(None).await
}
pub async fn install(hw: &Headway) -> Result<CliStatus> {
    hw.fix_cli_link(None).await
}
