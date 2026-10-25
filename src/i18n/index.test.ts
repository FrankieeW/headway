import { afterEach, describe, expect, it } from "vitest";

import i18n, { getLanguage, setLanguage } from "./index";

describe("document language", () => {
  afterEach(() => {
    setLanguage("en");
  });

  it("tracks the UI language on <html lang>", () => {
    setLanguage("zh");
    expect(i18n.language).toBe("zh");
    expect(document.documentElement.lang).toBe("zh-CN");

    setLanguage("en");
    expect(document.documentElement.lang).toBe("en");
  });

  it("reacts to direct i18n changes as well", async () => {
    await i18n.changeLanguage("zh");
    expect(document.documentElement.lang).toBe("zh-CN");
    expect(getLanguage()).toBe("zh");
  });
});
