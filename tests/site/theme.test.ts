import { expect, test } from "bun:test";
import { runInNewContext } from "node:vm";

import { builtSite } from "./built-site";

const INLINE_SCRIPT = /<script>(?<code>[\s\S]*?)<\/script>/gu;

/** The theme script exactly as the home page ships it. */
const themeScript = (): string => {
  const home = builtSite().pages.find((page) => page.url === "/")?.html ?? "";
  const script = [...home.matchAll(INLINE_SCRIPT)]
    .map((match) => match.groups?.code ?? "")
    .find((code) => code.includes("theme-override"));
  if (script === undefined) {
    throw new Error("The home page has no inline theme script.");
  }
  return script;
};

type Storage = "blocked" | Map<string, string>;

/** The one part of a click event the theme script reads. */
interface Click {
  target: { closest: (selector: string) => object | null };
}

/** Runs the theme script in a page that has only what the script touches. */
const openPage = (options: { storage: Storage; systemDark: boolean }) => {
  const classes = new Set<string>();
  const clicks: ((event: Click) => void)[] = [];
  const store = options.storage;
  const localStorage =
    store === "blocked"
      ? undefined
      : {
          getItem: (key: string) => store.get(key) ?? null,
          removeItem: (key: string) => store.delete(key),
          setItem: (key: string, value: string) => store.set(key, value),
        };
  const element = { setAttribute: () => null };
  const context = {
    document: {
      addEventListener: (type: string, listener: (event: Click) => void) => {
        if (type === "click") {
          clicks.push(listener);
        }
      },
      documentElement: {
        classList: {
          contains: (name: string) => classes.has(name),
          toggle: (name: string, on: boolean) =>
            on ? classes.add(name) : classes.delete(name),
        },
      },
      querySelector: () => element,
    },
    matchMedia: (query: string) => ({
      addEventListener: () => null,
      matches: query.includes("prefers-color-scheme") && options.systemDark,
    }),
  };
  Object.defineProperties(context, {
    // Chrome and Safari throw on the property read itself when site data is blocked.
    localStorage: {
      get: () => {
        if (localStorage === undefined) {
          throw new Error("SecurityError: The operation is insecure.");
        }
        return localStorage;
      },
    },
    window: { value: context },
  });
  runInNewContext(themeScript(), context);
  return {
    clickToggle: () => {
      for (const listener of clicks) {
        listener({ target: { closest: () => element } });
      }
    },
    isDark: () => classes.has("dark"),
  };
};

test("with site data blocked, the page still follows the system theme and the toggle still works", () => {
  const page = openPage({ storage: "blocked", systemDark: true });
  expect(page.isDark()).toBe(true);

  page.clickToggle();
  expect(page.isDark()).toBe(false);
});

test("a stored choice wins over the system theme, and toggling back to the system theme forgets it", () => {
  const storage = new Map([["theme-override", "light"]]);
  const page = openPage({ storage, systemDark: true });
  expect(page.isDark()).toBe(false);

  page.clickToggle();
  expect(page.isDark()).toBe(true);
  expect(storage.has("theme-override")).toBe(false);
});
