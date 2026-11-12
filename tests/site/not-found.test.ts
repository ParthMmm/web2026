import { expect, test } from "bun:test";
import { readFileSync } from "node:fs";
import path from "node:path";

import { builtSite } from "./built-site";

const notFoundPage = () =>
  builtSite().pages.find((page) => page.url === "/404.html")?.html ?? "";

test("an unknown address gets a page that says so and links home", () => {
  const html = notFoundPage();

  expect(html).toMatch(/<h1[^>]*>[^<]*not found[^<]*<\/h1>/iu);
  expect(html).toMatch(/<main[\s\S]*<a\s[^>]*href="\/"[\s\S]*<\/main>/u);
});

test("search engines are told not to index the not-found page", () => {
  const sitemap = readFileSync(
    path.join(builtSite().dir, "sitemap-0.xml"),
    "utf-8"
  );

  expect(notFoundPage()).toMatch(/<meta name="robots" content="noindex/u);
  expect(sitemap).not.toContain("404");
});
