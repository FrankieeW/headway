// Theme bootstrap: must put the saved theme on <html> before the first paint.
import "./lib/themeBootstrap";
import "./index.css";
import { setLanguage } from "./i18n";

import React from "react";
import ReactDOM from "react-dom/client";
import { QueryClientProvider } from "@tanstack/react-query";

import App from "./App";
import { ThemeProvider } from "@/components/theme-provider";
import { Toaster } from "@/components/ui/sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { queryClient } from "@/lib/query/queryClient";

function render(): void {
  ReactDOM.createRoot(document.getElementById("root")!).render(
    <React.StrictMode>
      <QueryClientProvider client={queryClient}>
        <ThemeProvider defaultTheme="system">
          <TooltipProvider delayDuration={0} skipDelayDuration={0}>
            <App />
            <Toaster />
          </TooltipProvider>
        </ThemeProvider>
      </QueryClientProvider>
    </React.StrictMode>,
  );
}

/** `?mock` in a dev build previews the UI in a browser with fixture data. */
async function boot(): Promise<void> {
  const params = new URLSearchParams(window.location.search);
  if (import.meta.env.DEV && params.has("mock")) {
    const { installMockBackend } = await import("./dev/mockBackend");
    installMockBackend();
    const theme = params.get("theme");
    if (theme === "dark" || theme === "light") {
      window.localStorage.setItem("headway-theme", theme);
      document.documentElement.classList.toggle("dark", theme === "dark");
      document.documentElement.classList.toggle("light", theme === "light");
    }
    const lang = params.get("lang");
    if (lang === "en" || lang === "zh") {
      setLanguage(lang);
    }
  }
  render();
}

void boot();
