import { useState } from "react";
import { ExternalLink, Loader2 } from "lucide-react";
import { useTranslation } from "react-i18next";

import {
  SettingsBlock,
  SettingsCard,
  SettingsRow,
  SettingsSwitchRow,
} from "@/components/settings/SettingsLayout";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  useClearToken,
  useSaveSettings,
  useSetToken,
  useSettings,
  useTokenStatus,
} from "@/hooks/useSettings";
import { systemApi, type RateLimit } from "@/lib/api";
import { tokenSourceLabel } from "@/lib/format";

const TOKEN_URL = "https://github.com/settings/personal-access-tokens/new";

/** "4,987 / 5,000 requests left · resets in 23 min" */
function rateLimitText(
  rate: RateLimit | null | undefined,
  lang: string,
  t: (key: string, options?: Record<string, unknown>) => string,
): string {
  if (!rate) return "";

  const numbers = new Intl.NumberFormat(lang);
  const left = numbers.format(rate.remaining);
  const limit = numbers.format(rate.limit);

  const resetIn = Math.max(0, rate.reset * 1000 - Date.now());
  const minutes = Math.ceil(resetIn / 60_000);
  if (minutes <= 0) return t("settings.rateLimit", { left, limit });

  const time =
    minutes < 60
      ? t("settings.rateLimitMinutes", { count: minutes })
      : t("settings.rateLimitHours", { count: Math.ceil(minutes / 60) });

  return t("settings.rateLimitReset", { left, limit, time });
}

/**
 * The GitHub section: token, rate limit and the `gh` fallback.
 */
export function GitHubSection() {
  const { t, i18n } = useTranslation();
  const { data: settings } = useSettings();
  const { data: token, isLoading } = useTokenStatus();
  const setToken = useSetToken();
  const clearToken = useClearToken();
  const saveSettings = useSaveSettings();

  const [value, setValue] = useState("");
  const [confirmOpen, setConfirmOpen] = useState(false);

  const handleSaveToken = async () => {
    const token = value.trim();
    if (!token) return;
    try {
      await setToken.mutateAsync(token);
      // never keep a token in the interface
      setValue("");
    } catch {
      // the mutation already showed the error
    }
  };

  const handleToggleFallback = (checked: boolean) => {
    if (!settings) return;
    saveSettings.mutate({ ...settings, gh_fallback: checked });
  };

  return (
    <>
      <SettingsBlock title={t("settings.github")}>
        <SettingsCard>
          <SettingsRow
            label={t("settings.tokenStatus")}
            description={
              isLoading
                ? t("common.loading")
                : tokenSourceLabel(token?.source, t)
            }
            control={
              token?.login ? (
                <span className="font-mono text-caption text-fg-2">
                  @{token.login}
                </span>
              ) : null
            }
          >
            {token?.masked ? (
              <p className="font-mono text-caption text-fg-3">
                {token.masked}
              </p>
            ) : null}
            {token?.rate ? (
              <p className="text-caption text-fg-3">
                {rateLimitText(token.rate, i18n.language, t)}
              </p>
            ) : null}
            {token?.error ? (
              <Alert className="mt-2 border-danger/40 bg-danger-soft">
                <AlertDescription className="text-caption text-danger-text">
                  {token.error.message}
                </AlertDescription>
              </Alert>
            ) : null}
          </SettingsRow>

          <SettingsRow
            label={t("settings.saveToken")}
            description={t("settings.tokenHelp")}
          >
            <div className="flex items-center gap-2">
              <Input
                type="password"
                value={value}
                onChange={(event) => setValue(event.target.value)}
                placeholder="ghp_…"
                className="font-mono"
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    void handleSaveToken();
                  }
                }}
              />
              <Button
                variant="solid"
                size="regular"
                onClick={() => void handleSaveToken()}
                disabled={!value.trim() || setToken.isPending}
                className="shrink-0"
              >
                {setToken.isPending ? (
                  <Loader2 className="h-4 w-4 animate-spin" />
                ) : null}
                {t("settings.saveToKeychain")}
              </Button>
              <Button
                variant="neutral"
                size="regular"
                onClick={() => setConfirmOpen(true)}
                disabled={clearToken.isPending}
                className="shrink-0"
              >
                {t("settings.removeToken")}
              </Button>
            </div>

            <button
              type="button"
              onClick={() => void systemApi.openUrl(TOKEN_URL)}
              className="mt-2 inline-flex items-center gap-1 text-caption text-action-text transition-colors hover:underline"
            >
              <ExternalLink className="h-3 w-3" strokeWidth={1.75} />
              {t("settings.createToken")}
            </button>
          </SettingsRow>

          <SettingsSwitchRow
            label={t("settings.ghFallback")}
            description={t("settings.ghFallbackDescription")}
            checked={settings?.gh_fallback ?? false}
            onCheckedChange={handleToggleFallback}
            disabled={!settings || saveSettings.isPending}
          />
        </SettingsCard>
      </SettingsBlock>

      <ConfirmDialog
        isOpen={confirmOpen}
        title={t("settings.removeTokenTitle")}
        message={t("settings.removeTokenMessage")}
        confirmText={t("settings.removeToken")}
        variant="destructive"
        pending={clearToken.isPending}
        onConfirm={() => {
          clearToken.mutate(undefined, { onSuccess: () => setConfirmOpen(false) });
        }}
        onCancel={() => setConfirmOpen(false)}
      />
    </>
  );
}
