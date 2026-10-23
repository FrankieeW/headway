import { describe, expect, it } from "vitest";

import { cn } from "./utils";

describe("cn", () => {
  it("merges class names", () => {
    expect(cn("px-2", "py-1")).toBe("px-2 py-1");
    expect(cn("rounded-control", "border")).toBe("rounded-control border");
  });

  it("drops falsy values", () => {
    expect(cn("px-2", false, undefined, null, "py-1")).toBe("px-2 py-1");
  });

  it("resolves conflicting tailwind utilities", () => {
    expect(cn("px-2", "px-4")).toBe("px-4");
    expect(cn("bg-surface", "bg-subtle")).toBe("bg-subtle");
    expect(cn("rounded-control", "rounded-panel")).toBe("rounded-panel");
  });

  it("keeps the custom font-size roles apart from text colors", () => {
    // text-body 是字号角色，不能被当成颜色和 text-fg-1 互相覆盖
    expect(cn("text-body", "text-fg-1")).toBe("text-body text-fg-1");
  });
});
