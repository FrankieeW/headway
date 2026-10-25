import { fireEvent, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import i18n from "@/i18n";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
import {
  matchInventory,
  packageMatches,
  repositoryMatches,
} from "@/test/matchFixtures";
import { SourceMatchList } from "./SourceMatchList";
import { InventoryCandidateRow } from "./InventoryCandidateRow";

afterEach(setupI18n);

it.each([
  ["en", "Use Homebrew", "Track from GitHub", "Use GitHub", ": "],
  ["zh", "改用 Homebrew", "改为跟踪 GitHub", "使用 GitHub", "："],
])(
  "names each match action by its target in %s without changing visible text",
  async (language, brew, github, imported, separator) => {
    await i18n.changeLanguage(language);
    const select = vi.fn();
    const useRepository = vi.fn();
    const candidates = ["acme/editor", "other/editor"];
    const repositories = candidates.flatMap((repo) =>
      repositoryMatches.repositories.map((match) => ({ ...match, repo })),
    );
    renderWithProviders(
      <>
        <SourceMatchList
          matches={{ ...repositoryMatches, repositories }}
          onSelect={select}
        />
        <SourceMatchList
          matches={{
            ...packageMatches,
            packages: ["editor", "editor-beta"].flatMap((pkg) =>
              packageMatches.packages.map((match) => ({
                ...match,
                package: pkg,
              })),
            ),
          }}
          onSelect={select}
        />
        <InventoryCandidateRow
          candidate={{ ...matchInventory, repositories }}
          selected={false}
          importing={false}
          onSelectedChange={vi.fn()}
          onUseRepository={useRepository}
        />
      </>,
    );
    for (const repo of candidates) {
      const track = screen.getByRole("button", {
        name: `${github}${separator}${repo}`,
      });
      expect(track).toHaveTextContent(github);
      fireEvent.click(track);
      expect(select).toHaveBeenLastCalledWith({ source: "github", repo });
      const use = screen.getByRole("button", {
        name: `${imported}${separator}${repo}`,
      });
      expect(use).toHaveTextContent(imported);
      fireEvent.click(use);
      expect(useRepository).toHaveBeenLastCalledWith(repo);
    }
    for (const pkg of ["editor", "editor-beta"]) {
      const use = screen.getByRole("button", {
        name: `${brew}${separator}Cask · ${pkg}`,
      });
      expect(use).toHaveTextContent(brew);
      fireEvent.click(use);
      expect(select).toHaveBeenLastCalledWith({
        source: "brew",
        package: pkg,
        cask: true,
      });
    }
  },
);
