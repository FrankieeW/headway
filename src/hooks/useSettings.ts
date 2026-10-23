import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";

import { errorMessage, settingsApi, type Settings } from "@/lib/api";
import { queryKeys } from "@/lib/query/keys";

/** Persisted preferences. */
export function useSettings() {
  return useQuery({
    queryKey: queryKeys.settings,
    queryFn: () => settingsApi.get(),
  });
}

/** Save preferences. */
export function useSaveSettings() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (settings: Settings) => settingsApi.save(settings),
    onSuccess: async (saved) => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.settings });
      // gh_fallback decides which token is used; bin_dir moves the CLI link
      await queryClient.invalidateQueries({ queryKey: queryKeys.token });
      await queryClient.invalidateQueries({ queryKey: queryKeys.cli });
      toast.success(t("toast.settingsSaved"));
      return saved;
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
}

/** Where the GitHub token comes from and how much API quota is left. */
export function useTokenStatus() {
  return useQuery({
    queryKey: queryKeys.token,
    queryFn: () => settingsApi.tokenStatus(),
    // the token rarely changes; the rate limit is refreshed on demand
    staleTime: 60_000,
  });
}

/** Store a token in the Keychain. */
export function useSetToken() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (token: string) => settingsApi.setToken(token),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.token });
      toast.success(t("toast.tokenSaved"));
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
}

/** Remove the stored token. */
export function useClearToken() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => settingsApi.clearToken(),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.token });
      toast.success(t("toast.tokenRemoved"));
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
}

/** Config, state, cache and log directories. */
export function usePaths() {
  return useQuery({
    queryKey: queryKeys.paths,
    queryFn: () => settingsApi.paths(),
  });
}

/** Whether the `headway` CLI is installed and on the PATH. */
export function useCliStatus() {
  return useQuery({
    queryKey: queryKeys.cli,
    queryFn: () => settingsApi.cliStatus(),
  });
}

/** Install the `headway` CLI into `bin_dir`. */
export function useInstallCli() {
  const { t } = useTranslation();
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: () => settingsApi.installCli(),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.cli });
      toast.success(t("toast.cliInstalled"));
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
}

/** Launch Headway at login. */
export function useAutostart() {
  return useQuery({
    queryKey: queryKeys.autostart,
    queryFn: () => settingsApi.getAutostart(),
  });
}

/** Turn launch at login on or off. */
export function useSetAutostart() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (enabled: boolean) => settingsApi.setAutostart(enabled),
    onSuccess: async (enabled) => {
      await queryClient.invalidateQueries({ queryKey: queryKeys.autostart });
      return enabled;
    },
    onError: (error) => toast.error(errorMessage(error)),
  });
}

/**
 * Environment diagnostics. Never fetched automatically — the user has to
 * press "Run checks".
 */
export function useDoctor() {
  return useQuery({
    queryKey: queryKeys.doctor,
    queryFn: () => settingsApi.doctor(),
    enabled: false,
    retry: false,
  });
}
