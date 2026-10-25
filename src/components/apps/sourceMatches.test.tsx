import { fireEvent, screen, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
import { packageMatches, repositoryMatches } from "@/test/matchFixtures";
import { SourceMatchList } from "./SourceMatchList";

beforeEach(setupI18n);

it.each([
  ["high", "Verified"],
  ["medium", "Likely"],
  ["low", "Unconfirmed"],
] as const)(
  "exposes %s package confidence as visible text",
  (confidence, label) => {
    // Given a package suggestion at this confidence level.
    renderWithProviders(
      <SourceMatchList
        matches={{
          ...packageMatches,
          packages: packageMatches.packages.map((match) => ({
            ...match,
            confidence,
          })),
        }}
        onSelect={vi.fn()}
      />,
    );
    // When its accessible row is read, then confidence is available without color.
    const row = screen.getByRole("listitem", {
      name: "Cask · vendor/apps/editor",
    });
    expect(within(row).getByText(label)).toBeVisible();
  },
);

it("selects a formula without changing it into a cask", () => {
  // Given a matching formula, when the user chooses Homebrew.
  const onSelect = vi.fn();
  renderWithProviders(
    <SourceMatchList
      matches={{
        ...packageMatches,
        packages: packageMatches.packages.map((match) => ({
          ...match,
          cask: false,
          installed: null,
        })),
      }}
      onSelect={onSelect}
    />,
  );
  fireEvent.click(screen.getByRole("button", { name: "Use Homebrew" }));
  // Then formula identity is preserved, including the tap-qualified package.
  expect(screen.getByText("Formula · vendor/apps/editor")).toBeVisible();
  expect(onSelect).toHaveBeenCalledWith({
    source: "brew",
    package: "vendor/apps/editor",
    cask: false,
  });
  expect(screen.queryByText(/Installed:/)).toBeNull();
});

it.each([
  ["high", "Verified"],
  ["medium", "Likely"],
  ["low", "Unconfirmed"],
] as const)(
  "renders %s repository evidence as text and selects only on click",
  (confidence, label) => {
    // Given untrusted metadata in an advisory repository match.
    const onSelect = vi.fn();
    const { container } = renderWithProviders(
      <SourceMatchList
        matches={{
          ...repositoryMatches,
          repositories: repositoryMatches.repositories.map((match) => ({
            ...match,
            confidence,
            description: "<img src=x onerror=alert(1)>",
          })),
        }}
        onSelect={onSelect}
      />,
    );
    expect(screen.getByText(label)).toBeVisible();
    expect(screen.getByText(/Download URL/)).toBeVisible();
    expect(screen.getByText("<img src=x onerror=alert(1)>")).toBeVisible();
    expect(container.querySelector("img")).toBeNull();
    expect(onSelect).not.toHaveBeenCalled();
    // When explicitly choosing the suggestion, then its source is handed off.
    fireEvent.click(screen.getByRole("button", { name: "Track from GitHub" }));
    expect(onSelect).toHaveBeenCalledWith({
      source: "github",
      repo: "acme/editor",
    });
  },
);

it("disables tracked packages and shows installed versions and tracking ids", () => {
  // Given a package managed by two subscriptions.
  const onSelect = vi.fn();
  renderWithProviders(
    <SourceMatchList
      matches={{
        ...packageMatches,
        packages: packageMatches.packages.map((match) => ({
          ...match,
          tracked_by: ["editor-main", "editor-beta"],
        })),
      }}
      onSelect={onSelect}
    />,
  );
  // When its row is displayed, then it cannot be chosen again.
  const row = screen.getByRole("listitem", {
    name: "Cask · vendor/apps/editor",
  });
  expect(row).toHaveTextContent("Cask · vendor/apps/editor");
  expect(row).toHaveTextContent("Installed: 2.0");
  expect(row).toHaveTextContent("editor-main, editor-beta");
  fireEvent.click(within(row).getByRole("button", { name: "Use Homebrew" }));
  expect(onSelect).not.toHaveBeenCalled();
});

it("disables tracked repositories", () => {
  renderWithProviders(
    <SourceMatchList
      matches={{
        ...repositoryMatches,
        repositories: repositoryMatches.repositories.map((match) => ({
          ...match,
          tracked_by: ["editor"],
        })),
      }}
      onSelect={vi.fn()}
    />,
  );
  expect(
    screen.getByRole("button", { name: "Track from GitHub" }),
  ).toBeDisabled();
  expect(screen.getByText("Tracked by: editor")).toBeVisible();
});

it("keeps matches selectable alongside partial errors and notes", () => {
  // Given usable results alongside a failed provider.
  renderWithProviders(
    <SourceMatchList
      matches={{
        ...packageMatches,
        notes: ["Search was limited."],
        errors: [
          {
            source: "github",
            error: { code: "rate_limited", message: "Try again later" },
          },
        ],
      }}
      onSelect={vi.fn()}
    />,
  );
  // Then the warning does not disable the successful result.
  expect(screen.getByRole("alert")).toHaveTextContent(
    "github: Try again later",
  );
  expect(screen.getByText("Search was limited.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Use Homebrew" })).toBeEnabled();
});

it("shows the empty match state", () => {
  renderWithProviders(
    <SourceMatchList
      matches={{ ...repositoryMatches, repositories: [] }}
      onSelect={vi.fn()}
    />,
  );
  expect(screen.getByText("No match found.")).toBeVisible();
});
