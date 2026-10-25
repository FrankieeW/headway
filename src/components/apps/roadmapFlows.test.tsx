import { fireEvent, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { AddSources } from "./AddSources";
import { BuildCandidates } from "./BuildCandidates";
import { RecoverySection } from "./RecoverySection";
import { AutoUpdatePaused } from "./AutoUpdatePaused";
import { appFromForm, emptyForm, formFromApp } from "./appFormModel";
import {
  appsApi,
  DEFAULT_UPDATE_POLICY,
  opsApi,
  type AppView,
} from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";

beforeEach(async () => {
  vi.restoreAllMocks();
  await setupI18n();
});
const view: AppView = {
  app: {
    id: "tool",
    name: "Tool",
    source: "github",
    repo: "owner/tool",
    kind: "cli",
    auto_update: true,
    track: { mode: "commit" },
    install: { method: "source", build: "make", artifact: "bin/tool" },
  },
  status: "unchecked",
  pending_count: 0,
  busy: false,
};

it("applies the previewed candidate only after an explicit action and renders README as text", () => {
  // Given a command containing HTML-like untrusted text.
  const candidate = {
    id: "readme",
    source: "readme" as const,
    file: "README.md",
    line: 17,
    build: 'echo "<img src=x onerror=alert(1)>"',
    kind: "cli" as const,
    recommended: true,
    reason: "README build command",
  };
  const apply = vi.fn();
  const { container } = renderWithProviders(
    <BuildCandidates candidates={[candidate]} onApply={apply} />,
  );
  expect(apply).not.toHaveBeenCalled();
  expect(container.querySelector("img")).toBeNull();
  // When the user applies the selected command.
  fireEvent.click(screen.getByRole("button", { name: "Apply candidate" }));
  // Then exactly the previewed candidate is handed to the editable form.
  expect(apply).toHaveBeenCalledWith(candidate);
});

it("imports only eligible checked packages with automation disabled", async () => {
  // Given eligible, already tracked, and conflicting inventory entries.
  const app = {
    id: "jq",
    name: "jq",
    source: "brew" as const,
    package: "jq",
    cask: false,
    kind: "cli" as const,
    auto_update: true,
  };
  vi.spyOn(appsApi, "discover").mockResolvedValue({
    candidates: [
      { key: "jq", app, already_tracked: false, conflicts: [] },
      {
        key: "tracked",
        app: { ...app, id: "tracked", name: "Tracked" },
        already_tracked: true,
        conflicts: [],
      },
      {
        key: "conflict",
        app: { ...app, id: "conflict", name: "Conflict" },
        already_tracked: false,
        conflicts: ["Managed destination"],
      },
    ],
    errors: [],
  });
  const imported = vi.spyOn(appsApi, "import").mockResolvedValue([]);
  renderWithProviders(<AddSources onRecipe={() => {}} onImported={() => {}} />);
  fireEvent.click(
    screen.getByRole("button", { name: "Import installed software" }),
  );
  expect(
    await screen.findByRole("checkbox", { name: "Tracked · Homebrew Formula" }),
  ).toBeDisabled();
  expect(
    screen.getByRole("checkbox", { name: "Conflict · Homebrew Formula" }),
  ).toBeDisabled();
  fireEvent.click(
    screen.getByRole("checkbox", { name: "jq · Homebrew Formula" }),
  );
  // When explicitly importing the checked eligible item.
  fireEvent.click(screen.getByRole("button", { name: "Import selected (1)" }));
  // Then no updater is started and unsafe imported automation is reset.
  await waitFor(() =>
    expect(imported).toHaveBeenCalledWith([
      { ...app, auto_update: false, update_policy: DEFAULT_UPDATE_POLICY },
    ]),
  );
});

it("requires confirmation before restoring a cached artifact", async () => {
  // Given a real recovery component and one cached backup.
  vi.spyOn(opsApi, "backups").mockResolvedValue([
    {
      id: "backup",
      app_id: "tool",
      created_at: "2026-10-24T08:00:00Z",
      artifact_path: "/cache/tool",
    },
  ]);
  const restore = vi.spyOn(opsApi, "restoreBackup").mockResolvedValue({
    app_id: "tool",
    backup_id: "backup",
    restored_path: "/bin/tool",
  });
  renderWithProviders(<RecoverySection view={view} />);
  fireEvent.click(
    await screen.findByRole("button", { name: "Restore backup" }),
  );
  expect(restore).not.toHaveBeenCalled();
  // When the user confirms the replacement.
  const buttons = screen.getAllByRole("button", { name: "Restore backup" });
  const confirm = buttons[buttons.length - 1];
  if (!confirm) throw new Error("Confirmation missing");
  fireEvent.click(confirm);
  // Then the exact backup is restored and the outcome path is displayed.
  await waitFor(() => expect(restore).toHaveBeenCalledWith("tool", "backup"));
  expect(await screen.findByRole("status")).toHaveTextContent("/bin/tool");
});

it("never offers cached artifact recovery for package-manager sources", () => {
  // Given a Homebrew-managed install.
  const backups = vi.spyOn(opsApi, "backups");
  const { container } = renderWithProviders(
    <RecoverySection
      view={{
        ...view,
        app: {
          id: "brew",
          name: "Brew",
          kind: "cli",
          auto_update: false,
          source: "brew",
          package: "jq",
          cask: false,
        },
      }}
    />,
  );
  // When rendering its recovery surface, then no artifact endpoint or action exists.
  expect(container).toBeEmptyDOMElement();
  expect(backups).not.toHaveBeenCalled();
});

it("resumes the paused app explicitly", async () => {
  // Given a paused automation surface.
  const resume = vi.spyOn(opsApi, "resumeAutoUpdate").mockResolvedValue(view);
  renderWithProviders(<AutoUpdatePaused id="tool" busy={false} />);
  // When the user resumes it.
  fireEvent.click(
    screen.getByRole("button", { name: "Resume automatic updates" }),
  );
  // Then only that app is resumed.
  await waitFor(() => expect(resume).toHaveBeenCalledWith("tool"));
});

it("serializes legacy defaults and round-trips verification and bounded next policy", () => {
  // Given a legacy configuration and an explicitly customized configuration.
  const legacy = appFromForm(formFromApp(view.app));
  expect(legacy.update_policy).toEqual({
    mode: "latest",
    max_steps: 1,
    pause_on_failure: true,
  });
  const form = {
    ...emptyForm(),
    id: "tool",
    name: "Tool",
    repo: "owner/tool",
    autoUpdate: true,
    verifyCommand: " tool --version ",
    updatePolicy: {
      mode: "next" as const,
      max_steps: 3,
      pause_on_failure: false,
    },
  };
  // When serializing and loading the saved form.
  const saved = appFromForm(form);
  const loaded = formFromApp(saved);
  // Then policy bounds, opt-in and verification survive.
  expect(loaded.updatePolicy).toEqual(form.updatePolicy);
  expect(loaded.autoUpdate).toBe(true);
  expect(loaded.verifyCommand).toBe("tool --version");
});

it("never serializes notify-only as automatic installation", () => {
  // Given a legacy enabled flag with a newly selected notify-only policy.
  const form = {
    ...emptyForm(),
    autoUpdate: true,
    updatePolicy: { ...DEFAULT_UPDATE_POLICY, mode: "notify" as const },
  };
  // When serializing, then notification-only wins over the enabled flag.
  expect(appFromForm(form).auto_update).toBe(false);
});

it("does not offer recovery for self-installing GitHub builds", () => {
  // Given a source recipe whose build installs itself and has no artifact.
  const backups = vi.spyOn(opsApi, "backups");
  const { container } = renderWithProviders(
    <RecoverySection
      view={{
        ...view,
        app: {
          source: "github",
          repo: "owner/tool",
          id: "tool",
          name: "Tool",
          kind: "cli",
          auto_update: false,
          track: { mode: "commit" },
          install: { method: "source", build: "make install" },
        },
      }}
    />,
  );
  // When rendered, then no unsupported restore is offered or queried.
  expect(container).toBeEmptyDOMElement();
  expect(backups).not.toHaveBeenCalled();
});

it("keeps a failed restore error visible inside its confirmation dialog", async () => {
  // Given a cached backup whose verification will fail.
  vi.spyOn(opsApi, "backups").mockResolvedValue([
    {
      id: "b",
      app_id: "tool",
      created_at: "2026-10-24T08:00:00Z",
      artifact_path: "/cache/tool",
    },
  ]);
  vi.spyOn(opsApi, "restoreBackup").mockRejectedValue({
    code: "verify_failed",
    message: "verification failed",
  });
  const { queryClient } = renderWithProviders(<RecoverySection view={view} />);
  queryClient.setQueryDefaults(["app", "tool"], { gcTime: Infinity });
  queryClient.setQueryData(["app", "tool"], view);
  fireEvent.click(
    await screen.findByRole("button", { name: "Restore backup" }),
  );
  const confirmation = screen
    .getAllByRole("button", { name: "Restore backup" })
    .at(-1);
  if (!confirmation) throw new Error("Missing confirmation");
  // When a confirmed restore fails.
  fireEvent.click(confirmation);
  // Then its actionable error stays visible in the modal.
  await waitFor(() =>
    expect(screen.getByRole("dialog")).toHaveTextContent("verification failed"),
  );
  await waitFor(() =>
    expect(queryClient.getQueryState(["app", "tool"])?.isInvalidated).toBe(
      true,
    ),
  );
});
