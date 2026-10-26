import { act, screen, waitFor } from "@testing-library/react";
import { QueryClient } from "@tanstack/react-query";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { appsApi } from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
import { packageMatches } from "@/test/matchFixtures";
import { SourceMatchesSection } from "./SourceMatchesSection";

beforeEach(setupI18n);
afterEach(() => vi.restoreAllMocks());

it("reuses matches on remount for five minutes before probing again", async () => {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { gcTime: Infinity } },
  });
  const match = vi
    .spyOn(appsApi, "matchSource")
    .mockResolvedValue(packageMatches);
  const now = Date.now();
  const clock = vi.spyOn(Date, "now").mockReturnValue(now);
  const mount = () =>
    renderWithProviders(
      <SourceMatchesSection
        input="acme/editor"
        from="github"
        onSelect={vi.fn()}
      />,
      { queryClient },
    );
  const first = mount();
  await screen.findByText("Cask · vendor/apps/editor");
  first.unmount();
  clock.mockReturnValue(now + 5 * 60_000 - 1);
  let second: ReturnType<typeof mount> | undefined;
  await act(async () => {
    second = mount();
  });
  await waitFor(() => expect(queryClient.isFetching()).toBe(0));
  expect(match).toHaveBeenCalledTimes(1);
  second?.unmount();
  clock.mockReturnValue(now + 5 * 60_000);
  const third = mount();
  await waitFor(() => expect(match).toHaveBeenCalledTimes(2));
  third.unmount();
  queryClient.clear();
});
