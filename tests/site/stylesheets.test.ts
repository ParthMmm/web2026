import { expect, test } from "bun:test";

import { builtSite } from "./built-site";

const DATA_URI_FONT = /url\(\s*["']?data:(?:font|application\/font)[^)]*\)/gu;

test("no stylesheet embeds a font, so a page downloads only the fonts it renders", () => {
  const embedded = [...builtSite().stylesheets].flatMap(([url, css]) =>
    (css.match(DATA_URI_FONT) ?? []).map(
      (match) => `${url}: ${match.length} bytes of base64 font`
    )
  );

  expect(embedded).toEqual([]);
});
