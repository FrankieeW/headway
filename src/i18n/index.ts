import i18n from "i18next";
import { initReactI18next } from "react-i18next";

import en from "./locales/en.json";
import zh from "./locales/zh.json";

export type Language = "zh" | "en";

const LANGUAGE_STORAGE_KEY = "headway-language";

const DEFAULT_LANGUAGE: Language = "zh";

const isLanguage = (value: unknown): value is Language =>
  value === "zh" || value === "en";

const getInitialLanguage = (): Language => {
  if (typeof window !== "undefined") {
    try {
      const stored = window.localStorage.getItem(LANGUAGE_STORAGE_KEY);
      if (isLanguage(stored)) {
        return stored;
      }
    } catch (error) {
      console.warn("[i18n] Failed to read stored language preference", error);
    }
  }

  const navigatorLang =
    typeof navigator !== "undefined"
      ? (navigator.language?.toLowerCase() ??
        navigator.languages?.[0]?.toLowerCase())
      : undefined;

  if (navigatorLang?.startsWith("zh")) {
    return "zh";
  }

  if (navigatorLang?.startsWith("en")) {
    return "en";
  }

  // 认得出系统语言、但界面没有这种语言时用英文
  return navigatorLang ? "en" : DEFAULT_LANGUAGE;
};

const resources = {
  zh: {
    translation: zh,
  },
  en: {
    translation: en,
  },
};

i18n.use(initReactI18next).init({
  resources,
  lng: getInitialLanguage(), // 根据本地存储或系统语言选择默认语言
  fallbackLng: "en", // 缺少中文翻译时退回英文

  interpolation: {
    escapeValue: false, // React 已经默认转义
  },

  debug: false,
});

/** 切换界面语言，并写入本地存储 */
export const setLanguage = (language: Language): void => {
  if (!isLanguage(language)) {
    return;
  }

  if (i18n.language !== language) {
    void i18n.changeLanguage(language);
  }

  try {
    if (window.localStorage.getItem(LANGUAGE_STORAGE_KEY) !== language) {
      window.localStorage.setItem(LANGUAGE_STORAGE_KEY, language);
    }
  } catch (error) {
    console.warn("[i18n] Failed to persist language preference", error);
  }
};

/** 当前界面语言 */
export const getLanguage = (): Language =>
  isLanguage(i18n.language) ? i18n.language : DEFAULT_LANGUAGE;

export { LANGUAGE_STORAGE_KEY };

export default i18n;
