import { expect, test } from "bun:test";

import { builtSite } from "./built-site";

const HEADING = /<h(?<level>[1-6])[\s>]/gu;
const REDIRECT_STUB = /http-equiv="refresh"/u;

const outlines = () =>
  builtSite()
    .pages.filter((page) => !REDIRECT_STUB.test(page.html))
    .map((page) => ({
      levels: [...page.html.matchAll(HEADING)].map((match) =>
        Number(match.groups?.level)
      ),
      url: page.url,
    }));

test("every page has exactly one h1", () => {
  const wrong = outlines()
    .map(({ levels, url }) => ({
      count: levels.filter((level) => level === 1).length,
      url,
    }))
    .filter(({ count }) => count !== 1)
    .map(({ count, url }) => `${url} has ${count}`);

  expect(outlines().length).toBeGreaterThan(0);
  expect(wrong).toEqual([]);
});

test("no page skips a heading level on the way down", () => {
  const skipped = outlines().flatMap(({ levels, url }) =>
    levels
      .map((level, index) => ({ level, previous: levels[index - 1] ?? 0 }))
      .filter(({ level, previous }) => level > previous + 1)
      .map(
        ({ level, previous }) => `${url} goes from h${previous} to h${level}`
      )
  );

  expect(skipped).toEqual([]);
});
