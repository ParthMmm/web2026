import { expect, test } from "bun:test";

import type { Photo } from "../../src/lib/gallery";
import {
  altText,
  caption,
  firstRowCount,
  galleryBase,
  idFromHash,
  sizesFor,
  srcset,
  step,
  swipeStep,
  tileSizes,
} from "../../src/lib/gallery";
import { parseManifest } from "../../src/lib/gallery-manifest";

const BASE = "https://photos.example/gallery";
const ID = "a".repeat(64);
const SHA = "b".repeat(64);

const size = (key: string, width: number, height: number) => ({
  height,
  path: `photos/${ID}/${key}-${SHA}.webp`,
  width,
});

/** Fields a test sets on a photo as it would arrive over the wire, including
    ones the contract forbids, such as a number for `alt`. */
interface WireFields {
  id?: string;
  alt?: string | number;
  capturedAt?: string;
  film?: string;
}

const rawPhoto = (extra: WireFields = {}) => ({
  id: ID,
  publishedAt: "2026-11-10T18:04:00.000Z",
  sizes: {
    w1200: size("w1200", 1200, 800),
    w2400: size("w2400", 2400, 1600),
    w480: size("w480", 480, 320),
  },
  ...extra,
});

const manifest = (photos: unknown[]) => ({
  generatedAt: "2026-11-10T18:05:00.000Z",
  photos,
  version: 1,
});

const photo = (extra: WireFields = {}): Photo => {
  const parsed = parseManifest(manifest([rawPhoto(extra)]));
  if (parsed === null) {
    throw new Error("The test photo should be valid.");
  }
  const [first] = parsed.photos;
  if (first === undefined) {
    throw new Error("The test manifest should hold one photo.");
  }
  return first;
};

test("a manifest that follows the contract parses, with an empty gallery as a normal state", () => {
  expect(parseManifest(manifest([]))?.photos).toEqual([]);
  expect(
    parseManifest(manifest([rawPhoto({ alt: "A pier", film: "Portra 400" })]))
      ?.photos
  ).toHaveLength(1);
});

test("anything that does not follow the contract is a failed load", () => {
  const broken: [string, unknown][] = [
    ["not an object", "<html>"],
    ["null", null],
    ["another version", { ...manifest([]), version: 2 }],
    ["no photos array", { generatedAt: "x", version: 1 }],
    ["a short id", manifest([rawPhoto({ id: "abc" })])],
    [
      "a missing size",
      manifest([{ ...rawPhoto(), sizes: { w480: size("w480", 480, 320) } }]),
    ],
    [
      "a zero width",
      manifest([
        {
          ...rawPhoto(),
          sizes: { ...rawPhoto().sizes, w480: size("w480", 0, 320) },
        },
      ]),
    ],
    [
      "an absolute path",
      manifest([
        {
          ...rawPhoto(),
          sizes: {
            ...rawPhoto().sizes,
            w480: {
              height: 320,
              path: "https://evil.example/x.webp",
              width: 480,
            },
          },
        },
      ]),
    ],
    [
      "a path outside the photo",
      manifest([
        {
          ...rawPhoto(),
          sizes: {
            ...rawPhoto().sizes,
            w480: {
              height: 320,
              path: `private/${ID}/original.jpg`,
              width: 480,
            },
          },
        },
      ]),
    ],
    [
      "a path that climbs out",
      manifest([
        {
          ...rawPhoto(),
          sizes: {
            ...rawPhoto().sizes,
            w480: {
              height: 320,
              path: `photos/${ID}/../../originals/w480-${SHA}.webp`,
              width: 480,
            },
          },
        },
      ]),
    ],
    [
      "a path for another size",
      manifest([
        {
          ...rawPhoto(),
          sizes: { ...rawPhoto().sizes, w480: size("w2400", 480, 320) },
        },
      ]),
    ],
    ["an alt that is not text", manifest([rawPhoto({ alt: 4 })])],
    ["a duplicate id", manifest([rawPhoto(), rawPhoto()])],
  ];

  const parsed = broken
    .filter(([, value]) => parseManifest(value) !== null)
    .map(([name]) => name);

  expect(parsed).toEqual([]);
});

test("srcset lists each distinct width once, so an unscaled small original is not offered twice", () => {
  const small = photo();
  const sizes = {
    w1200: size("w1200", 900, 600),
    w2400: { ...size("w2400", 900, 600) },
    w480: size("w480", 480, 320),
  };

  expect(srcset(BASE, { ...small, sizes })).toBe(
    `${BASE}/photos/${ID}/w480-${SHA}.webp 480w, ${BASE}/photos/${ID}/w1200-${SHA}.webp 900w`
  );
});

test("srcset can stop below the largest size", () => {
  expect(srcset(BASE, photo(), ["w480", "w1200"])).toBe(
    `${BASE}/photos/${ID}/w480-${SHA}.webp 480w, ${BASE}/photos/${ID}/w1200-${SHA}.webp 1200w`
  );
});

test("the larger view sizes the image to whichever of the viewport's width or height limits it", () => {
  expect(sizesFor(photo())).toBe("min(100vw - 2rem, (100dvh - 7rem) * 1.5)");
});

test("a grid tile asks for about one and a half rows of its own width, and never more than the column", () => {
  expect(tileSizes(photo())).toBe(
    "(min-width: 48rem) min(60rem, 1.5 * 22.5rem), min(100vw - 3rem, 1.5 * 16.5rem)"
  );
});

test("alt text is the photo's own description when it has one", () => {
  expect(altText(photo({ alt: "  Fog over the Golden Gate  " }))).toBe(
    "Fog over the Golden Gate"
  );
});

test("without a description, alt text comes from the capture date and film, never the file", () => {
  const cases = [
    altText(photo({ capturedAt: "2024:05:03 14:22:01", film: "Portra 400" })),
    altText(photo({ capturedAt: "2024-05-03T14:22:01" })),
    altText(photo({ film: "HP5 Plus" })),
    altText(photo({ alt: "   " })),
    altText(photo({ capturedAt: "not a date" })),
  ];

  expect(cases).toEqual([
    "Photo taken 3 May 2024 on Portra 400",
    "Photo taken 3 May 2024",
    "Photo on HP5 Plus",
    "Photo published 10 November 2026",
    "Photo published 10 November 2026",
  ]);
  expect(cases.join(" ")).not.toContain(ID.slice(0, 8));
});

test("the caption names the date and film when the photo has them", () => {
  expect(
    caption(photo({ capturedAt: "2024:05:03 14:22:01", film: "Portra 400" }))
  ).toBe("3 May 2024 · Portra 400");
  expect(caption(photo())).toBe("");
});

test("next and previous stop at the ends instead of wrapping, and Home and End go to them", () => {
  expect(step(0, 1, 3)).toBe(1);
  expect(step(2, 1, 3)).toBe(2);
  expect(step(0, -1, 3)).toBe(0);
  expect(step(2, -1, 3)).toBe(1);
  expect(step(1, Number.POSITIVE_INFINITY, 3)).toBe(2);
  expect(step(1, Number.NEGATIVE_INFINITY, 3)).toBe(0);
});

test("a shared link opens the photo it names, and other hashes open nothing", () => {
  expect(idFromHash(`#${ID}`)).toBe(ID);
  expect(idFromHash("#main")).toBeNull();
  expect(idFromHash("")).toBeNull();
});

test("the gallery address is used only when it is set, without a trailing slash", () => {
  expect(galleryBase()).toBeNull();
  expect(galleryBase("  ")).toBeNull();
  expect(galleryBase(`${BASE}/`)).toBe(BASE);
  expect(galleryBase("not a url")).toBeNull();
});

const START = { x: 200, y: 400 };

test("a swipe to the left moves to the next photo", () => {
  expect(swipeStep(START, { x: 120, y: 410 })).toBe(1);
});

test("a swipe to the right moves to the previous photo", () => {
  expect(swipeStep(START, { x: 280, y: 390 })).toBe(-1);
});

test("a tap or a drag under the threshold does not move", () => {
  expect(swipeStep(START, START)).toBe(0);
  expect(swipeStep(START, { x: 170, y: 400 })).toBe(0);
  expect(swipeStep(START, { x: 230, y: 400 })).toBe(0);
});

test("a mostly vertical drag does not move", () => {
  expect(swipeStep(START, { x: 140, y: 700 })).toBe(0);
  expect(swipeStep(START, { x: 260, y: 100 })).toBe(0);
});

/* 16px rem; a phone's column is its width less the page's 1.5rem margins. */
const PHONE = { column: 342, rem: 16, viewport: 390 };
const DESKTOP = { column: 960, rem: 16, viewport: 1440 };
const WIDE = 1.5;
const TALL = 0.667;

test("the first row holds fewer wide photos on a phone than tall ones on a desktop", () => {
  const wide = Array.from({ length: 12 }, () => WIDE);
  const tall = Array.from({ length: 12 }, () => TALL);
  /* 1.5 × 176px is 264px, so a second wide photo cannot fit beside it. */
  expect(firstRowCount(wide, PHONE)).toBe(1);
  /* 0.667 × 240px is 160px; six with five 8px gaps take 1000px > 960px. */
  expect(firstRowCount(tall, DESKTOP)).toBe(5);
});

test("the first row follows the width and the shapes of the photos", () => {
  const tall = Array.from({ length: 12 }, () => TALL);
  const wide = Array.from({ length: 12 }, () => WIDE);
  /* 0.667 × 176px is about 117px; two and a gap fit 342px, three do not. */
  expect(firstRowCount(tall, PHONE)).toBe(2);
  expect(firstRowCount(wide, DESKTOP)).toBe(2);
  expect(firstRowCount([TALL, TALL, WIDE, TALL], DESKTOP)).toBe(4);
});

test("the first row holds one tile however wide it is, and none of none", () => {
  expect(firstRowCount([10], PHONE)).toBe(1);
  expect(firstRowCount([], PHONE)).toBe(0);
});
