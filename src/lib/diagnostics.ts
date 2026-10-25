import type { TranslateFn } from "@/lib/format";

/** The four directories `get_paths` reports. */
export type DiagnosticsPathKey =
  | "config_file"
  | "state_file"
  | "cache_dir"
  | "log_dir";

export const DIAGNOSTICS_PATH_KEYS: readonly DiagnosticsPathKey[] = [
  "config_file",
  "state_file",
  "cache_dir",
  "log_dir",
];

/** i18n key suffix for a reported path. */
export function diagnosticsPathLabelKey(key: DiagnosticsPathKey): string {
  switch (key) {
    case "config_file":
      return "settings.diagnosticsPaths.config";
    case "state_file":
      return "settings.diagnosticsPaths.state";
    case "cache_dir":
      return "settings.diagnosticsPaths.cache";
    case "log_dir":
      return "settings.diagnosticsPaths.logs";
  }
}

/** Translate a path label; unknown keys fall back to the raw key. */
export function diagnosticsPathLabel(
  key: DiagnosticsPathKey,
  t: TranslateFn,
): string {
  return t(diagnosticsPathLabelKey(key));
}

/**
 * Translate a `doctor` check name. `app:<id>` names the tracked app, which
 * stays user data; unknown future checks keep their raw name.
 */
export function diagnosticsCheckLabel(name: string, t: TranslateFn): string {
  switch (name) {
    case "config":
      return t("settings.diagnosticsChecks.config");
    case "git":
      return t("settings.diagnosticsChecks.git");
    case "github":
      return t("settings.diagnosticsChecks.github");
    case "bin_dir":
      return t("settings.diagnosticsChecks.binDir");
    case "apps_dir":
      return t("settings.diagnosticsChecks.appsDir");
    default:
      if (name.startsWith("app:")) {
        return t("settings.diagnosticsChecks.app", { id: name.slice(4) });
      }
      return name;
  }
}
