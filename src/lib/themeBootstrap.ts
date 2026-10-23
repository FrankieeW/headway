/**
 * 首屏主题引导：main.tsx 最先 import 本模块，
 * 在 React 渲染前把保存的主题写到 <html> 上，避免第一帧闪白。
 */

export type Theme = "light" | "dark" | "system";

export const THEME_STORAGE_KEY = "headway-theme";

const isTheme = (value: unknown): value is Theme =>
  value === "light" || value === "dark" || value === "system";

/** 读出已保存的主题；没有或非法时回落到 system */
export function getStoredTheme(): Theme {
  try {
    const stored = window.localStorage.getItem(THEME_STORAGE_KEY);
    if (isTheme(stored)) {
      return stored;
    }
  } catch (error) {
    console.warn("[theme] Failed to read stored theme preference", error);
  }
  return "system";
}

/** system 时看系统偏好 */
export function resolveIsDark(theme: Theme): boolean {
  if (theme === "dark") return true;
  if (theme === "light") return false;
  try {
    return (
      window.matchMedia?.("(prefers-color-scheme: dark)").matches ?? false
    );
  } catch {
    return false;
  }
}

/** 把主题写到 <html> 的 class 上（dark / light） */
export function applyStoredTheme(): void {
  try {
    const isDark = resolveIsDark(getStoredTheme());
    const root = window.document.documentElement;
    root.classList.toggle("dark", isDark);
    root.classList.toggle("light", !isDark);
  } catch (error) {
    console.warn("[theme] Failed to apply stored theme", error);
  }
}

applyStoredTheme();
