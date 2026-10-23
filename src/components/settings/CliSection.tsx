import { Copy, Loader2, Terminal } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import {
  SettingsBlock,
  SettingsCard,
  SettingsRow,
} from "@/components/settings/SettingsLayout";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { useCliStatus, useInstallCli } from "@/hooks/useSettings";

const EXAMPLES = [
  "headway list",
  "headway check",
  "headway update <id> --next",
  "headway --ai list",
];

/** One command with a copy button. */
function Example({ command }: { command: string }) {
  const { t } = useTranslation();

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(command);
      toast.success(t("toast.copied", { value: command }));
    } catch {
      toast.error(t("toast.copyFailed"));
    }
  };

  return (
    <div className="flex items-center gap-2 rounded-control bg-subtle px-2 py-1.5">
      <code className="min-w-0 flex-1 truncate font-mono text-caption text-fg-1">
        {command}
      </code>
      <button
        type="button"
        onClick={() => void copy()}
        aria-label={t("settings.copyCommand")}
        title={t("settings.copyCommand")}
        className="inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-control text-fg-2 transition-colors hover:bg-surface hover:text-fg-1"
      >
        <Copy className="h-3.5 w-3.5" strokeWidth={1.75} />
      </button>
    </div>
  );
}

/**
 * The command line section: the `headway` CLI and how to drive it.
 */
export function CliSection() {
  const { t } = useTranslation();
  const { data: cli } = useCliStatus();
  const installCli = useInstallCli();

  return (
    <SettingsBlock title={t("settings.cli")}>
      <SettingsCard>
        <SettingsRow
          label={t("settings.cliStatus")}
          description={
            cli?.installed
              ? t("settings.cliInstalledDescription")
              : t("settings.cliMissingDescription")
          }
          control={
            <Button
              variant={cli?.installed ? "neutral" : "solid"}
              size="regular"
              onClick={() => installCli.mutate()}
              disabled={installCli.isPending}
            >
              {installCli.isPending ? (
                <Loader2 className="h-4 w-4 animate-spin" />
              ) : (
                <Terminal className="h-4 w-4" strokeWidth={1.75} />
              )}
              {cli?.installed && cli.points_to === cli.bundled_path
                ? t("settings.reinstallCli")
                : t("settings.installCli")}
            </Button>
          }
        >
          <p className="font-mono text-caption text-fg-3">
            {cli?.link_path ?? "—"}
            {cli?.points_to ? ` → ${cli.points_to}` : ""}
          </p>

          {cli && !cli.on_path ? (
            <Alert className="mt-2 border-warning/40 bg-warning-soft">
              <AlertDescription className="text-caption text-warning-text">
                {t("settings.cliNotOnPath", { dir: cli.link_path })}
              </AlertDescription>
            </Alert>
          ) : null}
        </SettingsRow>

        <SettingsRow
          label={t("settings.cliExamples")}
          description={t("settings.aiAgents")}
        >
          <div className="space-y-1.5">
            {EXAMPLES.map((command) => (
              <Example key={command} command={command} />
            ))}
          </div>
          <p className="mt-2 text-caption text-fg-3">
            {t("settings.aiAgentsDescription")}
          </p>
        </SettingsRow>
      </SettingsCard>
    </SettingsBlock>
  );
}
