import { act, renderHook } from "@testing-library/react";
import { beforeEach, expect, it } from "vitest";
import { setupI18n } from "@/test/renderWithProviders";
import { appFromForm, formFromApp } from "./appFormModel";
import { useAppForm } from "./useAppForm";
import { DEFAULT_UPDATE_POLICY, type TrackedApp } from "@/lib/api";

const imported: TrackedApp = {
  id: "editor",
  name: "Editor",
  source: "brew",
  package: "editor",
  cask: true,
  kind: "app",
  auto_update: false,
  update_policy: DEFAULT_UPDATE_POLICY,
  install_path: "/Applications/Custom Apps/Editor.app",
};
beforeEach(async () => {
  await setupI18n();
});

it("preserves an imported Homebrew location when saving unchanged identity", () => {
  // Given an imported cask in a custom directory.
  const form = formFromApp(imported);
  // When the existing form is saved.
  const saved = appFromForm(form);
  // Then the reported metadata and cask ownership survive the round trip.
  expect(saved).toEqual(imported);
});

it.each(["package", "cask"] as const)(
  "clears the reported location when the %s identity changes",
  (key) => {
    // Given a previously imported cask.
    const { result } = renderHook(() => useAppForm(imported));
    // When its package identity changes.
    act(() => {
      if (key === "package") result.current.update("package", "other");
      else result.current.update("cask", false);
    });
    // Then the prior location is not assigned to the new source.
    expect(appFromForm(result.current.form)).not.toHaveProperty("install_path");
  },
);
