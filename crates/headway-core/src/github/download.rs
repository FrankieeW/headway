//! Streaming release-asset downloads with a hard size cap and a total deadline.

use std::path::Path;

use sha2::{Digest as _, Sha256};
use tokio::io::AsyncWriteExt as _;

use super::http::{ASSET_DEADLINE, MAX_ASSET_BYTES, deadline};
use super::redact::{display_text, network};
use super::{Asset, GitHub};
use crate::error::{Error, Result};
use crate::repo::RepoId;

impl GitHub {
    /// Streams a release asset to `dest`, reporting `(bytes_done, bytes_total)`.
    /// Returns the SHA-256 of the downloaded bytes as lowercase hex.
    ///
    /// The stream is capped by a total deadline and a maximum size, so a hostile or
    /// broken asset cannot fill the disk or hold the update open indefinitely.
    pub async fn download_asset(
        &self,
        repo: &RepoId,
        asset: &Asset,
        dest: &Path,
        on_progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
    ) -> Result<String> {
        let label = format!("download of `{}`", display_text(&asset.name));
        let download = self.download_asset_bounded(repo, asset, dest, on_progress, MAX_ASSET_BYTES);
        deadline(ASSET_DEADLINE, &label, download).await
    }

    /// `download_asset` with an explicit byte limit, for tests and future callers.
    pub(super) async fn download_asset_bounded(
        &self,
        repo: &RepoId,
        asset: &Asset,
        dest: &Path,
        on_progress: &mut (dyn FnMut(u64, Option<u64>) + Send),
        limit: u64,
    ) -> Result<String> {
        let endpoint = format!("/repos/{repo}/releases/assets/{}", asset.id);
        let what = format!("asset {}", display_text(&asset.name));
        let mut response = self
            .send(
                || self.request(&endpoint, "application/octet-stream"),
                &what,
            )
            .await?;
        let declared = response.content_length();
        if declared.is_some_and(|length| length > limit) {
            return Err(too_large(asset, limit));
        }
        let total = declared.or_else(|| (asset.size > 0).then_some(asset.size));
        let mut file = tokio::fs::File::create(dest)
            .await
            .map_err(|err| Error::io_at("create", dest, err))?;
        let mut hasher = Sha256::new();
        let mut done = 0_u64;
        while let Some(chunk) = response.chunk().await.map_err(network)? {
            let next = done.saturating_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX));
            if next > limit {
                drop(file);
                let _ = tokio::fs::remove_file(dest).await;
                return Err(too_large(asset, limit));
            }
            file.write_all(&chunk)
                .await
                .map_err(|err| Error::io_at("write", dest, err))?;
            hasher.update(&chunk);
            done = next;
            on_progress(done, total);
        }
        file.flush()
            .await
            .map_err(|err| Error::io_at("write", dest, err))?;
        Ok(super::to_hex(&hasher.finalize()))
    }
}

fn too_large(asset: &Asset, limit: u64) -> Error {
    Error::NoAsset(format!(
        "asset `{}` is larger than the {} GiB download limit",
        display_text(&asset.name),
        limit / (1024 * 1024 * 1024)
    ))
}
