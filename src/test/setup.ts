import "@testing-library/jest-dom/vitest";

/**
 * Some test environments have no `localStorage`. The app reads the theme,
 * the language and the selected app from it — a tiny in-memory shim keeps
 * those code paths working (and quiet) in tests.
 */
if (typeof window !== "undefined" && !window.localStorage) {
  const store = new Map<string, string>();

  const shim = {
    getItem: (key: string): string | null =>
      store.has(key) ? (store.get(key) as string) : null,
    setItem: (key: string, value: string): void => {
      store.set(key, String(value));
    },
    removeItem: (key: string): void => {
      store.delete(key);
    },
    clear: (): void => {
      store.clear();
    },
    key: (index: number): string | null =>
      Array.from(store.keys())[index] ?? null,
    get length(): number {
      return store.size;
    },
  };

  Object.defineProperty(window, "localStorage", {
    value: shim,
    configurable: true,
    writable: true,
  });
}
