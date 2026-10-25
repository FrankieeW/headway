import { screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import type { DoctorReport, PathsInfo } from "@/lib/api";
import { renderWithProviders, setupI18n } from "@/test/renderWithProviders";
import { DiagnosticsSection } from "./DiagnosticsSection";

const paths: PathsInfo = {
  config_file: "~/Library/Application Support/Headway/config.toml",
  state_file: "~/Library/Application Support/Headway/state.json",
  cache_dir: "~/Library/Caches/Headway",
  log_dir: "~/Library/Logs/Headway",
};

const doctor: DoctorReport = {
  ok: true,
  checks: [
    { name: "git", status: "ok", message: "git version 2.56.0" },
    {
      name: "app:zed",
      status: "warn",
      message: "cargo is not on PATH",
      hint: "install Rust",
    },
  ],
};

const refetch = vi.fn();
let doctorState: {
  data?: DoctorReport;
  error?: unknown;
  isError: boolean;
  isFetching: boolean;
} = { data: doctor, isError: false, isFetching: false };

vi.mock("@/hooks/useSettings", () => ({
  useDoctor: () => ({ ...doctorState, refetch }),
  usePaths: () => ({ data: paths }),
}));

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    systemApi: { ...actual.systemApi, appVersion: async () => "0.4.2" },
  };
});

describe("diagnostics", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
    doctorState = { data: doctor, isError: false, isFetching: false };
    await setupI18n();
  });

  it("translates path labels and doctor check names", async () => {
    renderWithProviders(<DiagnosticsSection />);

    expect(await screen.findByText("Config file")).toBeVisible();
    expect(screen.getByText("State file")).toBeVisible();
    expect(screen.getByText("Cache")).toBeVisible();
    expect(screen.getByText("Logs")).toBeVisible();
    // Known check names are translated; `app:<id>` keeps the app id as data.
    expect(screen.getByText("Git")).toBeVisible();
    expect(screen.getByText("App zed")).toBeVisible();
    // No raw snake_case label leaks into the UI.
    expect(screen.queryByText("config_file")).not.toBeInTheDocument();
    expect(screen.queryByText("bin_dir")).not.toBeInTheDocument();
  });

  it("shows a failed doctor run instead of silently doing nothing", async () => {
    doctorState = {
      error: { code: "io_error", message: "diagnostics crashed" },
      isError: true,
      isFetching: false,
    };
    renderWithProviders(<DiagnosticsSection />);

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("diagnostics crashed");
    expect(screen.getByText("Diagnostics failed")).toBeVisible();
    expect(screen.getByRole("button", { name: "Retry" })).toBeVisible();
  });
});
