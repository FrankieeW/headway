import { afterEach, expect, it } from "vitest";
import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { appsApi } from "@/lib/api";
import { handleMockCommand } from "./mockBackend";

afterEach(clearMocks);

it("binds matchSource to the exact command payload with default and explicit offline flags", async () => {
  const calls: unknown[] = [];
  mockIPC((cmd, args) => {
    calls.push([cmd, args]);
    return handleMockCommand(
      cmd,
      args && "input" in args
        ? {
            input: args.input,
            offline: "offline" in args ? args.offline : undefined,
          }
        : undefined,
    );
  });
  await appsApi.matchSource("acme/editor");
  await appsApi.matchSource("cask:editor", true);
  expect(calls).toEqual([
    ["match_source", { input: "acme/editor", offline: false }],
    ["match_source", { input: "cask:editor", offline: true }],
  ]);
});

it("returns snake_case package matches for GitHub inputs through the mock backend", () => {
  expect(
    handleMockCommand("match_source", { input: "acme/editor", offline: false }),
  ).toMatchObject({
    query: "acme/editor",
    from: "github",
    repositories: [],
    notes: [],
    errors: [],
    packages: [
      {
        package: "editor",
        cask: true,
        repo: "acme/editor",
        confidence: "high",
        tracked_by: [],
        evidence: [{ kind: "download_url", value: expect.any(String) }],
        installed: expect.any(String),
        description: null,
        homepage: null,
      },
    ],
  });
});

it.each(["brew:editor", "cask:editor"])(
  "returns repository matches for %s through the mock backend",
  (input) => {
    expect(
      handleMockCommand("match_source", { input, offline: true }),
    ).toMatchObject({
      from: "brew",
      packages: [],
      notes: [],
      errors: [],
      repositories: [
        {
          repo: "acme/editor",
          html_url: "https://github.com/acme/editor",
          confidence: "high",
          tracked_by: [],
          description: null,
          stars: null,
          evidence: [{ kind: expect.any(String), value: expect.any(String) }],
        },
      ],
    });
  },
);

it("includes local repository evidence in a brew inventory fixture", () => {
  expect(handleMockCommand("discover_apps", undefined)).toMatchObject({
    candidates: expect.arrayContaining([
      expect.objectContaining({
        app: expect.objectContaining({ source: "brew" }),
        repositories: [
          expect.objectContaining({
            repo: "zed-industries/zed",
            confidence: "high",
          }),
        ],
      }),
    ]),
  });
});
