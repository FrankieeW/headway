import { act, renderHook } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { appsApi, type RepoInspection } from "@/lib/api";
import { matchInspection, matchLookup } from "@/test/matchFixtures";
import { useAppForm } from "./useAppForm";

afterEach(() => vi.restoreAllMocks());

it("drops saved recipe identity after reset when switching to a matched repository", async () => {
  vi.spyOn(appsApi, "inspect").mockResolvedValue(matchInspection);
  const { result } = renderHook(() => useAppForm());
  act(() => {
    result.current.setName("Earlier edit");
    result.current.setId("earlier-id");
    result.current.update("kind", "app");
    result.current.reset({
      ...matchLookup.suggestion,
      name: "Recipe X",
      id: "recipe-x",
    });
  });
  await act(async () =>
    result.current.inspectSource({ source: "github", repo: "acme/editor" }),
  );
  expect(result.current.form).toMatchObject({
    name: "Editor",
    id: "editor",
    kind: "cli",
  });
  expect(result.current.idEdited).toBe(false);
});

it("derives identity from a name typed after a recipe when switching sources", async () => {
  vi.spyOn(appsApi, "inspect").mockResolvedValue(matchInspection);
  const { result } = renderHook(() => useAppForm());
  act(() => {
    result.current.reset({
      ...matchLookup.suggestion,
      name: "Recipe X",
      id: "recipe-x",
    });
    result.current.setName("My editor");
  });
  await act(async () =>
    result.current.inspectSource({ source: "github", repo: "acme/editor" }),
  );
  expect(result.current.form).toMatchObject({
    name: "My editor",
    id: "my-editor",
    kind: "cli",
  });
  expect(result.current.idEdited).toBe(false);
});

it("ignores a delayed old inspection after explicitly selecting a different source", async () => {
  // Given an in-flight GitHub inspection and a responsive Homebrew lookup.
  let resolve: ((inspection: RepoInspection) => void) | undefined;
  vi.spyOn(appsApi, "inspect").mockImplementation(
    () =>
      new Promise((done) => {
        resolve = done;
      }),
  );
  vi.spyOn(appsApi, "lookup").mockResolvedValue(matchLookup);
  const { result } = renderHook(() => useAppForm());
  let pending: Promise<void> | undefined;
  act(() => {
    pending = result.current.inspect("old/repository");
  });
  // When the user switches before the old response arrives.
  await act(async () =>
    result.current.inspectSource({
      source: "brew",
      package: "vendor/apps/editor",
      cask: true,
    }),
  );
  await act(async () => {
    resolve?.(matchInspection);
    await pending;
  });
  // Then only the explicitly chosen source can populate the form.
  expect(result.current.form).toMatchObject({
    source: "brew",
    package: "vendor/apps/editor",
    cask: true,
    repo: "",
    build: "",
  });
  expect(result.current.inspection).toBeNull();
  expect(result.current.lookup).toEqual(matchLookup);
});

it("drops old recipe edits while keeping explicit identity and automation on a match switch", async () => {
  // Given an edited source recipe with installation details unsuitable for a package.
  vi.spyOn(appsApi, "lookup").mockResolvedValue(matchLookup);
  const { result } = renderHook(() => useAppForm());
  act(() => {
    result.current.setName("My editor");
    result.current.setId("custom-editor");
    result.current.update("kind", "cli");
    result.current.update("build", "make custom");
    result.current.update("dest", "/tmp/old");
    result.current.update("installPath", "/Applications/Old.app");
    result.current.update("autoUpdate", true);
    result.current.update("updatePolicy", {
      mode: "next",
      max_steps: 2,
      pause_on_failure: true,
    });
  });
  // When using a Homebrew suggestion, then only compatible deliberate choices survive.
  await act(async () =>
    result.current.inspectSource({
      source: "brew",
      package: "vendor/apps/editor",
      cask: true,
    }),
  );
  expect(result.current.form).toMatchObject({
    source: "brew",
    cask: true,
    kind: "cli",
    name: "My editor",
    id: "custom-editor",
    autoUpdate: true,
    updatePolicy: { mode: "latest" },
    build: "",
    dest: "",
    installPath: "",
  });
});

it("keeps the selected target editable when its immediate inspection fails", async () => {
  // Given a populated Homebrew form and an unavailable GitHub repository.
  vi.spyOn(appsApi, "lookup").mockResolvedValue(matchLookup);
  vi.spyOn(appsApi, "inspect").mockRejectedValue({
    code: "remote_not_found",
    message: "Repository unavailable",
  });
  const { result } = renderHook(() => useAppForm());
  await act(async () =>
    result.current.inspectSource({
      source: "brew",
      package: "editor",
      cask: true,
    }),
  );
  // When choosing GitHub fails, then stale package metadata stays cleared.
  await act(async () =>
    result.current.inspectSource({ source: "github", repo: "acme/editor" }),
  );
  expect(result.current.form).toMatchObject({
    source: "github",
    repo: "acme/editor",
    package: "",
    cask: false,
  });
  expect(result.current.lookup).toBeNull();
  expect(result.current.inspection).toBeNull();
  expect(result.current.inspectionError).toBe("Repository unavailable");
  expect(result.current.inspecting).toBe(false);
});
