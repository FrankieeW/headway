import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { appsApi, type RepoInspection } from "@/lib/api";
import { useAppForm, type AppFormState } from "./useAppForm";

const inspected: RepoInspection = {
  repo: "owner/tool",
  default_branch: "main",
  private: false,
  archived: false,
  html_url: "https://github.com/owner/tool",
  binaries: [],
  notes: [],
  suggestion: {
    source: "github",
    id: "tool",
    name: "Suggested",
    kind: "app",
    repo: "owner/tool",
    auto_update: true,
    track: { mode: "release", prerelease: false },
    install: { method: "asset", pattern: "*.zip", dest: "/Applications" },
  },
};
afterEach(() => vi.restoreAllMocks());
const choices: Partial<AppFormState> = {
  build: "make custom",
  workdir: "desktop",
  artifact: "out/My.app",
  dest: "/tmp/My.app",
  installMethod: "source",
  trackMode: "commit",
  branch: "dev",
  verifyCommand: "my-tool --version",
  pattern: "*.custom",
  binary: "Custom.app",
  gitUrl: "ssh://custom/repo",
  prerelease: true,
  tagFilter: "^preview",
  presetId: "custom",
};

describe("inspection field ownership", () => {
  it.each(["before", "during"] as const)(
    "preserves every explicit choice made %s inspection",
    async (timing) => {
      // Given a delayed IPC inspection and independently edited configuration.
      let resolve: ((value: RepoInspection) => void) | undefined;
      vi.spyOn(appsApi, "inspect").mockImplementation(
        () =>
          new Promise((r) => {
            resolve = r;
          }),
      );
      const { result } = renderHook(() => useAppForm());
      const edit = () => {
        result.current.update("build", choices.build ?? "");
        result.current.update("workdir", choices.workdir ?? "");
        result.current.update("artifact", choices.artifact ?? "");
        result.current.update("dest", choices.dest ?? "");
        result.current.update("installMethod", "source");
        result.current.update("trackMode", "commit");
        result.current.update("branch", "dev");
        result.current.update("verifyCommand", "my-tool --version");
        result.current.update("pattern", "*.custom");
        result.current.update("binary", "Custom.app");
        result.current.update("gitUrl", "ssh://custom/repo");
        result.current.update("prerelease", true);
        result.current.update("tagFilter", "^preview");
        result.current.update("presetId", "custom");
      };
      if (timing === "before") act(edit);
      let pending: Promise<void> | undefined;
      act(() => {
        pending = result.current.inspect("owner/tool");
      });
      if (timing === "during") act(edit);
      // When the result arrives, only untouched fields may be inferred.
      await act(async () => {
        resolve?.(inspected);
        await pending;
      });
      // Then manual commands, paths, installation choices and tracking survive.
      expect(result.current.form).toMatchObject(choices);
      expect(result.current.form.name).toBe("Suggested");
      expect(result.current.form.autoUpdate).toBe(false);
    },
  );

  it("preserves an intentionally empty field and name during inspection", async () => {
    // Given a manual clearing of values.
    vi.spyOn(appsApi, "inspect").mockResolvedValue(inspected);
    const { result } = renderHook(() => useAppForm());
    act(() => {
      result.current.setName("");
      result.current.update("dest", "");
    });
    // When inspection returns nonempty suggestions.
    await act(async () => result.current.inspect("owner/tool"));
    // Then emptiness remains a deliberate choice.
    expect(result.current.form.name).toBe("");
    expect(result.current.form.dest).toBe("");
  });

  it("ignores an old repository result after changing repository", async () => {
    // Given an in-flight request for the old source.
    let resolve: ((value: RepoInspection) => void) | undefined;
    vi.spyOn(appsApi, "inspect").mockImplementation(
      () =>
        new Promise((r) => {
          resolve = r;
        }),
    );
    const { result } = renderHook(() => useAppForm());
    let pending: Promise<void> | undefined;
    act(() => {
      pending = result.current.inspect("owner/tool");
    });
    act(() => result.current.update("repo", "other/repo"));
    // When the stale response arrives.
    await act(async () => {
      resolve?.(inspected);
      await pending;
    });
    // Then no old candidates or suggestions are applied.
    expect(result.current.inspection).toBeNull();
    expect(result.current.form.repo).toBe("other/repo");
    expect(result.current.form.name).toBe("");
  });

  it("preserves an edited saved configuration even without new keystrokes", async () => {
    // Given an existing app with an explicit command and paths.
    const existing = { ...inspected.suggestion, name: "User name" };
    vi.spyOn(appsApi, "inspect").mockResolvedValue(inspected);
    const { result } = renderHook(() => useAppForm(existing));
    const before = result.current.form;
    // When the user re-inspects it.
    await act(async () => result.current.inspect("owner/tool"));
    // Then existing configuration is preserved exactly.
    expect(result.current.form).toEqual(before);
  });
});

it("uses the newly selected repository when editing an existing subscription", async () => {
  // Given a saved subscription and a new repository input.
  const suggestion = {
    ...inspected,
    repo: "new/repo",
    suggestion: {
      ...inspected.suggestion,
      source: "github" as const,
      repo: "new/repo",
      track: { mode: "commit" as const },
      install: { method: "source" as const, build: "suggested build" },
    },
  };
  vi.spyOn(appsApi, "inspect").mockResolvedValue(suggestion);
  const { result } = renderHook(() => useAppForm(inspected.suggestion));
  act(() => result.current.update("repo", "new/repo"));
  // When inspecting the changed repository.
  await act(async () => result.current.inspect("new/repo"));
  // Then query identity is updated but the saved installation choice is preserved.
  expect(result.current.form.repo).toBe("new/repo");
  expect(result.current.form.installMethod).toBe("asset");
});
