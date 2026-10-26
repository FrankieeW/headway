import { afterEach, describe, expect, it } from "vitest";

import i18n, { getLanguage, setLanguage } from "./index";
import en from "./locales/en.json";
import zh from "./locales/zh.json";

it("provides matching locale keys and interpolation parameters for source matching", () => {
  function leaves(value: object, prefix = ""): Record<string, string[]> {
    return Object.fromEntries(
      Object.entries(value).flatMap(([key, text]) => {
        const path = `${prefix}${key}`;
        return typeof text === "string"
          ? [
              [
                path,
                [...text.matchAll(/\{\{(\w+)\}\}/g)]
                  .map((match) => match[1])
                  .sort(),
              ],
            ]
          : Object.entries(leaves(text, `${path}.`));
      }),
    );
  }
  expect(leaves(zh.matches)).toEqual(leaves(en.matches));
});

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
