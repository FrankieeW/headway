import { toast } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { systemApi } from "@/lib/api";
import { openUrl, revealPath } from "./systemActions";

vi.mock("sonner", () => ({
  toast: { error: vi.fn(), success: vi.fn() },
}));

vi.mock("@/lib/api", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/api")>();
  return {
    ...actual,
    systemApi: {
      ...actual.systemApi,
      openUrl: vi.fn(),
      revealPath: vi.fn(),
    },
  };
});

describe("system actions", () => {
  beforeEach(() => {
    vi.mocked(toast.error).mockClear();
    vi.mocked(systemApi.openUrl).mockReset();
    vi.mocked(systemApi.revealPath).mockReset();
  });

  it("surfaces a rejected open_url instead of dropping the promise", async () => {
    vi.mocked(systemApi.openUrl).mockRejectedValue({
      code: "invalid_input",
      message: "only http(s) links can be opened",
    });

    openUrl("file:///etc/passwd");

    await vi.waitFor(() => expect(toast.error).toHaveBeenCalledOnce());
    expect(toast.error).toHaveBeenCalledWith(
      "only http(s) links can be opened",
    );
  });

  it("does not toast when the link opens", async () => {
    vi.mocked(systemApi.openUrl).mockResolvedValue(undefined);
    openUrl("https://github.com/FrankieeW/headway");
    await vi.waitFor(() => expect(systemApi.openUrl).toHaveBeenCalledOnce());
    expect(toast.error).not.toHaveBeenCalled();
  });

  it("surfaces a rejected reveal_path", async () => {
    vi.mocked(systemApi.revealPath).mockRejectedValue({
      code: "invalid_input",
      message: "path does not exist",
    });

    revealPath("/nope");

    await vi.waitFor(() => expect(toast.error).toHaveBeenCalledOnce());
    expect(toast.error).toHaveBeenCalledWith("path does not exist");
  });
});
