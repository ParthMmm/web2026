import { expect, test } from "bun:test";

import { builtSite } from "./built-site";
import { classNames, scopedClassNames, splitUtilities } from "./css";

const CUSTOM_PROPERTY_DEFINITION =
  /(?:@property\s+|[{;"\s])(?<name>--[\w-]+)\s*[:{]/gu;
/** `var(--name)` with nothing after the name: no fallback to use instead. */
const CUSTOM_PROPERTY_READ = /var\(\s*(?<name>--[\w-]+)\s*\)/gu;
const DATA_URI_FONT = /url\(\s*["']?data:(?:font|application\/font)[^)]*\)/gu;

test("no stylesheet embeds a font, so a page downloads only the fonts it renders", () => {
  const embedded = [...builtSite().stylesheets].flatMap(([url, css]) =>
    (css.match(DATA_URI_FONT) ?? []).map(
      (match) => `${url}: ${match.length} bytes of base64 font`
    )
  );

  expect(embedded).toEqual([]);
});

test("Tailwind generates no utility under a class name a component styles itself", () => {
  const utilities = new Set<string>();
  const scoped = new Set<string>();
  for (const css of builtSite().stylesheets.values()) {
    const layers = splitUtilities(css);
    for (const name of classNames(layers.utilities)) {
      utilities.add(name);
    }
    for (const name of scopedClassNames(layers.rest)) {
      scoped.add(name);
    }
  }

  expect(scoped.size).toBeGreaterThan(0);
  expect([...utilities].filter((name) => scoped.has(name))).toEqual([]);
});

test("every custom property a page reads without a fallback is defined", () => {
  const { pages, stylesheets } = builtSite();
  const sources = [...stylesheets.values(), ...pages.map((page) => page.html)];
  const defined = new Set(
    sources.flatMap((source) =>
      [...source.matchAll(CUSTOM_PROPERTY_DEFINITION)].map(
        (match) => match.groups?.name
      )
    )
  );
  const read = new Set(
    sources.flatMap((source) =>
      [...source.matchAll(CUSTOM_PROPERTY_READ)].map(
        (match) => match.groups?.name
      )
    )
  );

  expect(read.size).toBeGreaterThan(0);
  expect([...read].filter((name) => !defined.has(name))).toEqual([]);
});
