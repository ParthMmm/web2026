import { expect, test } from "bun:test";

import { builtSite, FIXTURE_GALLERY_URL } from "./built-site";

const PHOTOS_LINK = /<a\s[^>]*?href="\/photos\/"/u;
const NOINDEX = /<meta name="robots" content="noindex/u;

const photosPage = (galleryUrl?: string): string => {
  const page = builtSite(galleryUrl).pages.find((p) => p.url === "/photos/");
  if (page === undefined) {
    throw new Error("The build has no /photos/ page.");
  }
  return page.html;
};

const sitemap = (galleryUrl?: string): Promise<string> =>
  Bun.file(`${builtSite(galleryUrl).dir}/sitemap-0.xml`).text();

test("without a gallery address, the photos page builds, says so, and nothing points to it", async () => {
  const html = photosPage();

  expect(html).toContain("The gallery isn’t connected yet.");
  expect(html).not.toContain("data-gallery-base");
  expect(html).toMatch(NOINDEX);
  expect(
    builtSite()
      .pages.filter((page) => PHOTOS_LINK.test(page.html))
      .map((page) => page.url)
  ).toEqual([]);
  expect(await sitemap()).not.toContain("/photos/");
});

test("with a gallery address, every page links to the photos page and the page loads from that address", async () => {
  const { pages } = builtSite(FIXTURE_GALLERY_URL);
  const html = photosPage(FIXTURE_GALLERY_URL);

  expect(html).toContain(`data-gallery-base="${FIXTURE_GALLERY_URL}"`);
  expect(html).not.toMatch(NOINDEX);
  expect(
    pages
      .filter((page) => !page.html.includes('http-equiv="refresh"'))
      .filter((page) => !PHOTOS_LINK.test(page.html))
      .map((page) => page.url)
  ).toEqual([]);
  expect(await sitemap(FIXTURE_GALLERY_URL)).toContain("/photos/");
});
