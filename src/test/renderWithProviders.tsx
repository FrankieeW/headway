import type { ReactElement, ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, type RenderOptions } from "@testing-library/react";

import { TooltipProvider } from "@/components/ui/tooltip";
import i18n from "@/i18n";

/** A query client that never retries — a failing test should fail fast. */
export function createTestQueryClient(): QueryClient {
  return new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0 },
      mutations: { retry: false },
    },
  });
}

/** Switch the shared i18n instance to English for stable assertions. */
export async function setupI18n(): Promise<void> {
  await i18n.changeLanguage("en");
}

interface ProvidersOptions extends Omit<RenderOptions, "wrapper"> {
  queryClient?: QueryClient;
}

/**
 * Render a component with the providers it needs: i18n (English) and a
 * fresh query client.
 */
export function renderWithProviders(
  ui: ReactElement,
  options: ProvidersOptions = {},
) {
  const { queryClient = createTestQueryClient(), ...rest } = options;

  const Wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={queryClient}>
      <TooltipProvider delayDuration={0} skipDelayDuration={0}>
        {children}
      </TooltipProvider>
    </QueryClientProvider>
  );

  return {
    queryClient,
    ...render(ui, { wrapper: Wrapper, ...rest }),
  };
}
