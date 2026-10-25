use crate::{AppKind, Error, Headway, Install, LaunchOutcome, Result, process::Cmd};

impl Headway {
    /// Launches an independently installed app using its normal shared data.
    pub async fn launch_variant(&self, id: &str) -> Result<LaunchOutcome> {
        let _lock = crate::lock::AppLock::acquire_app_async(self.paths(), id).await?;
        let config = self.store().config()?;
        let app = config.app(id)?;
        let github = app.require_github("independent application launch")?;
        if github.options.variant.is_none() || app.kind != AppKind::App {
            return Err(Error::InvalidInput("launch requires an independent app bundle; CLI versions run from their installed path".into()));
        }
        let installed = self.store().state()?.app(id).installed;
        let configured = match &github.install {
            Install::Source(s) => s.dest.as_deref(),
            Install::Asset(s) => s.dest.as_deref(),
        };
        let path = configured
            .or_else(|| installed.as_ref().and_then(|i| i.path.as_deref()))
            .map(crate::paths::expand_tilde)
            .ok_or_else(|| Error::InvalidInput("independent app has not been installed".into()))?;
        if !path.is_dir() || !crate::install::is_bundle(&path) {
            return Err(Error::InvalidInput(
                "installed independent app bundle is missing".into(),
            ));
        }
        let env = self.env().await.with_var("HEADWAY_VARIANT_ID", id);
        let plist = path.join("Contents/Info.plist");
        let read = crate::probe::capture(
            &crate::CancellationToken::new(),
            &Cmd::new("/usr/bin/plutil", &env).args([
                "-extract",
                "CFBundleExecutable",
                "raw",
                "-o",
                "-",
                &plist.display().to_string(),
            ]),
        )
        .await?;
        if !read.success() {
            return Err(Error::InvalidInput(
                "app bundle has no readable executable name".into(),
            ));
        }
        let name = read.stdout.trim();
        if name.is_empty()
            || name.contains(['/', '\\'])
            || matches!(name, "." | "..")
            || name.contains(char::is_control)
        {
            return Err(Error::InvalidInput("invalid bundle executable name".into()));
        }
        let executable = path.join("Contents/MacOS").join(name);
        if !executable
            .canonicalize()
            .map_err(|e| Error::io_at("resolve app executable", &executable, e))?
            .starts_with(
                path.canonicalize()
                    .map_err(|e| Error::io_at("resolve app bundle", &path, e))?,
            )
        {
            return Err(Error::InvalidInput(
                "bundle executable points outside the app".into(),
            ));
        }
        let pid = Cmd::new(executable.display().to_string(), &env).spawn_detached()?;
        Ok(LaunchOutcome {
            app_id: id.into(),
            pid,
            path: path.display().to_string(),
        })
    }
}
