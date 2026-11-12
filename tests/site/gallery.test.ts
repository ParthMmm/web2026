import { expect, test } from "bun:test";

import type { Photo } from "../../src/lib/gallery";
import {
  altText,
  caption,
  galleryBase,
  idFromHash,
  sizesFor,
  srcset,
  step,
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

test("next and previous stop at the ends instead of wrapping", () => {
  expect(step(0, 1, 3)).toBe(1);
  expect(step(2, 1, 3)).toBe(2);
  expect(step(0, -1, 3)).toBe(0);
  expect(step(2, -1, 3)).toBe(1);
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
