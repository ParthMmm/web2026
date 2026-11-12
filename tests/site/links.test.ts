import { expect, test } from "bun:test";

import { builtSite } from "./built-site";

const HREF = /<a\s[^>]*?href="(?<url>[^"]*)"/gu;
const META_REFRESH =
  /http-equiv="refresh"\s+content="[^"]*?url=(?<url>[^"]*)"/gu;
const ABSOLUTE = /https:\/\/www\.parthm\.dev(?<url>[^"'\s<>\\]*)/gu;

/** The path a link asks the host for, or `null` when it leaves the site. */
const internalPath = (url: string): string | null => {
  if (url.startsWith("//") || !(url.startsWith("/") || url === "")) {
    return null;
  }
  const [path = ""] = url.split(/[?#]/u);
  return path === "" ? "/" : path;
};

test("every link into the site names a file the host serves without a redirect", () => {
  const { files, pages } = builtSite();
  const redirected = pages.flatMap((page) =>
    [
      ...[...page.html.matchAll(HREF)].map((match) => match.groups?.url ?? ""),
      ...[...page.html.matchAll(META_REFRESH)].map(
        (match) => match.groups?.url ?? ""
      ),
      ...[...page.html.matchAll(ABSOLUTE)].map(
        (match) => match.groups?.url ?? ""
      ),
    ]
      .filter((url) => !url.startsWith("#"))
      .map(internalPath)
      .filter((path) => path !== null && !files.has(path))
      .map((path) => `${page.url} links to ${path}`)
  );

  expect(pages.length).toBeGreaterThan(0);
  expect(redirected).toEqual([]);
});
