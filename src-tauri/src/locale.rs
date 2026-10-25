//! Backend locale for surfaces the renderer cannot translate: the menu-bar menu
//! and native notifications. The window itself is translated by i18next; this
//! table only has to cover the few strings the tray and notifications show.
//!
//! The language comes from `Settings::language` (`system`, `en`, `zh`). For
//! `system` we follow the process environment first, then the macOS preferred
//! language list (`defaults read -g AppleLanguages`), probed once at startup.

use std::sync::OnceLock;

use headway_core::Headway;
#[cfg(target_os = "macos")]
use headway_core::process::{self, Cmd};

/// Language of the backend-owned UI surfaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
    En,
    Zh,
}

static SYSTEM_LOCALE: OnceLock<Locale> = OnceLock::new();

impl Locale {
    /// Reads the persisted setting (`system`, `en`, `zh`); unknown values follow
    /// the system, so a config write from an older build keeps working.
    pub fn from_setting(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "en" | "en-us" | "en-gb" => Self::En,
            "zh" | "zh-cn" | "zh-hans" | "zh-hant" => Self::Zh,
            _ => system_locale(),
        }
    }

    /// The strings of this language.
    pub const fn strings(self) -> &'static Table {
        match self {
            Self::En => &EN,
            Self::Zh => &ZH,
        }
    }
}

/// The language the user's system prefers, as far as we can tell without a
/// bundle. The slow half (macOS preferred languages) is resolved once by
/// [`prime_system_locale`] at startup; this function only reads the cache.
fn system_locale() -> Locale {
    environment_locale()
        .or_else(|| SYSTEM_LOCALE.get().copied())
        .unwrap_or(Locale::En)
}

/// Resolves the system language once at startup. The tray path stays synchronous
/// and cheap; only this startup task may probe macOS preferences.
pub async fn prime_system_locale(hw: &Headway) {
    if hw
        .settings()
        .is_ok_and(|settings| settings.language.trim() != "system")
    {
        return;
    }
    if SYSTEM_LOCALE.get().is_some() {
        return;
    }
    let locale = match environment_locale() {
        Some(locale) => locale,
        None => apple_languages_locale().await.unwrap_or(Locale::En),
    };
    let _ = SYSTEM_LOCALE.set(locale);
}

fn environment_locale() -> Option<Locale> {
    for key in ["HEADWAY_LANG", "LC_ALL", "LC_MESSAGES", "LANG"] {
        let Ok(value) = std::env::var(key) else {
            continue;
        };
        let value = value.trim().to_ascii_lowercase();
        if value.starts_with("zh") {
            return Some(Locale::Zh);
        }
        if value.starts_with("en") {
            return Some(Locale::En);
        }
    }
    None
}

/// macOS keeps the preferred languages in the global preferences domain.
#[cfg(target_os = "macos")]
async fn apple_languages_locale() -> Option<Locale> {
    let env = headway_core::shell::ShellEnv::current();
    let cmd = Cmd::new("/usr/bin/defaults", &env).args([
        "-currentHost".to_owned(),
        "-g".to_owned(),
        "AppleLanguages".to_owned(),
    ]);
    // Same policy as every other subprocess: short, bounded, token-free via Cmd.
    let output = tokio::time::timeout(std::time::Duration::from_secs(3), process::capture(&cmd))
        .await
        .ok()?
        .ok()?;
    output
        .success()
        .then(|| apple_languages_from(&output.stdout))
}

/// Linux/Windows development builds have no `defaults` command.
#[cfg(not(target_os = "macos"))]
async fn apple_languages_locale() -> Option<Locale> {
    None
}

/// Parses the first entry of `defaults read -g AppleLanguages` output.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn apple_languages_from(output: &str) -> Locale {
    let first = output
        .split('"')
        .nth(1)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if first.starts_with("zh") {
        Locale::Zh
    } else {
        Locale::En
    }
}

/// Language of the backend-owned surfaces, from the persisted settings.
pub fn for_headway(hw: &Headway) -> Locale {
    hw.settings().map_or(Locale::En, |settings| {
        Locale::from_setting(&settings.language)
    })
}

/// Everything the tray and notifications say. One struct per language: a missing
/// translation is a compile error, not a silent English fallback in one spot.
#[derive(Debug)]
pub struct Table {
    pub tooltip: &'static str,
    pub no_apps: &'static str,
    pub up_to_date: &'static str,
    pub updates_available: &'static str,
    pub unit_commits: &'static str,
    pub unit_releases: &'static str,
    pub unit_tags: &'static str,
    pub unit_versions: &'static str,
    pub updating: &'static str,
    pub behind: &'static str,
    pub more: &'static str,
    pub check_now: &'static str,
    pub update_all: &'static str,
    pub open: &'static str,
    pub quit: &'static str,
    pub has_update: &'static str,
    pub have_updates: &'static str,
    pub updated_to: &'static str,
    pub update_failed: &'static str,
}

pub const EN: Table = Table {
    tooltip: "Headway",
    no_apps: "No apps tracked yet",
    up_to_date: "Everything is up to date",
    updates_available: "{} updates available",
    unit_commits: "new commits",
    unit_releases: "new releases",
    unit_tags: "new tags",
    unit_versions: "new versions",
    updating: "{} — updating…",
    behind: "{} — {} {}",
    more: "+{} more",
    check_now: "Check now",
    update_all: "Update all",
    open: "Open Headway",
    quit: "Quit Headway",
    has_update: "{} has an update",
    have_updates: "{} apps have updates",
    updated_to: "Updated {} to {}",
    update_failed: "Updating {} failed: {}",
};

pub const ZH: Table = Table {
    tooltip: "Headway",
    no_apps: "尚未跟踪任何应用",
    up_to_date: "全部已是最新",
    updates_available: "{} 个应用有可用更新",
    unit_commits: "个新提交",
    unit_releases: "个新版本",
    unit_tags: "个新标签",
    unit_versions: "个新版本",
    updating: "{} — 正在更新…",
    behind: "{} — {} {}",
    more: "另有 {} 个",
    check_now: "立即检查",
    update_all: "全部更新",
    open: "打开 Headway",
    quit: "退出 Headway",
    has_update: "{} 有可用更新",
    have_updates: "{} 个应用有可用更新",
    updated_to: "已将 {} 更新到 {}",
    update_failed: "{} 更新失败：{}",
};

impl Table {
    pub fn updates_available(&self, count: usize) -> String {
        self.updates_available.replacen("{}", &count.to_string(), 1)
    }

    pub fn updating(&self, name: &str) -> String {
        self.updating.replacen("{}", name, 1)
    }

    pub fn behind(&self, name: &str, count: u32, unit: &str) -> String {
        self.behind
            .replacen("{}", name, 1)
            .replacen("{}", &count.to_string(), 1)
            .replacen("{}", unit, 1)
    }

    pub fn more(&self, count: usize) -> String {
        self.more.replacen("{}", &count.to_string(), 1)
    }

    pub fn has_update(&self, name: &str) -> String {
        self.has_update.replacen("{}", name, 1)
    }

    pub fn have_updates(&self, count: usize) -> String {
        self.have_updates.replacen("{}", &count.to_string(), 1)
    }

    pub fn updated_to(&self, name: &str, version: &str) -> String {
        self.updated_to
            .replacen("{}", name, 1)
            .replacen("{}", version, 1)
    }

    pub fn update_failed(&self, name: &str, error: &str) -> String {
        self.update_failed
            .replacen("{}", name, 1)
            .replacen("{}", error, 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_settings_win() {
        assert_eq!(Locale::from_setting("en"), Locale::En);
        assert_eq!(Locale::from_setting(" zh "), Locale::Zh);
        assert_eq!(Locale::from_setting("ZH-HANS"), Locale::Zh);
    }

    #[test]
    fn unknown_settings_follow_the_system() {
        // `system` and anything a future build might write resolve the same way.
        assert_eq!(
            Locale::from_setting("system"),
            Locale::from_setting("klingon")
        );
    }

    #[test]
    fn apple_languages_output_parses() {
        let output = "(\n    \"zh-Hans-CN\",\n    \"en-US\"\n)\n";
        assert_eq!(apple_languages_from(output), Locale::Zh);
        assert_eq!(apple_languages_from("(\n    \"en-GB\"\n)"), Locale::En);
        assert_eq!(apple_languages_from("(\n    \"fr-FR\"\n)"), Locale::En);
        assert_eq!(apple_languages_from(""), Locale::En);
    }

    #[test]
    fn both_tables_are_complete_and_translated() {
        for table in [&EN, &ZH] {
            for value in [
                table.no_apps,
                table.up_to_date,
                table.updates_available,
                table.unit_commits,
                table.unit_releases,
                table.unit_tags,
                table.unit_versions,
                table.updating,
                table.behind,
                table.more,
                table.check_now,
                table.update_all,
                table.open,
                table.quit,
                table.has_update,
                table.have_updates,
                table.updated_to,
                table.update_failed,
            ] {
                assert!(!value.trim().is_empty());
            }
        }
        assert_ne!(EN.check_now, ZH.check_now);
        assert_ne!(EN.updated_to, ZH.updated_to);
        assert!(!ZH.updates_available(3).contains("{}"));
        assert!(!ZH.behind("Zed", 2, ZH.unit_commits).contains("{}"));
        assert!(!EN.update_failed("zed", "boom").contains("{}"));
    }
}
