/* Writes a seeded public gallery for the /photos/ smoke check: drawn WebP
   images at the three published sizes and the v1.json manifest that lists
   them. No real photos. Run from the repo root:

     bun tests/fixtures/gallery/generate.ts

   then serve it with `bun tests/fixtures/gallery/serve.ts`. */

import { createHash } from "node:crypto";
import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";

import sharp from "sharp";

const OUT = path.join(import.meta.dir, "public");
const WIDTHS = { w1200: 1200, w2400: 2400, w480: 480 } as const;

interface Seed {
  /** Size of the drawn "original". */
  width: number;
  height: number;
  /** Two oklch hues for the sky and the ground. */
  hues: [number, number];
  publishedAt: string;
  capturedAt?: string;
  alt?: string;
  film?: string;
}

const SEEDS: Seed[] = [
  {
    alt: "A low orange sun over a green field",
    capturedAt: "2026:10:03 07:41:12",
    film: "Portra 400",
    height: 2000,
    hues: [55, 148],
    publishedAt: "2026-11-10T18:04:00.000Z",
    width: 3000,
  },
  {
    capturedAt: "2026:09:21 18:02:45",
    film: "HP5 Plus",
    height: 3000,
    hues: [85, 85],
    publishedAt: "2026-11-09T09:30:00.000Z",
    width: 2000,
  },
  {
    alt: "A square of pale sky above a dark hill",
    height: 2400,
    hues: [230, 148],
    publishedAt: "2026-11-08T12:00:00.000Z",
    width: 2400,
  },
  {
    capturedAt: "2026:08:14 12:10:00",
    height: 1688,
    hues: [30, 55],
    publishedAt: "2026-11-07T15:45:00.000Z",
    width: 3000,
  },
  {
    film: "Ektar 100",
    height: 3000,
    hues: [300, 230],
    publishedAt: "2026-11-06T08:20:00.000Z",
    width: 2400,
  },
  {
    // A small original: never upscaled, so w1200 and w2400 are both 900 wide.
    alt: "A small frame of a green door",
    height: 600,
    hues: [148, 55],
    publishedAt: "2026-11-05T19:05:00.000Z",
    width: 900,
  },
  {
    // No description, date or film: the alt falls back to the publish date.
    height: 2000,
    hues: [200, 30],
    publishedAt: "2026-11-04T10:10:00.000Z",
    width: 3000,
  },
];

const sha256 = (data: string | Uint8Array): string =>
  createHash("sha256").update(data).digest("hex");

const drawing = (seed: Seed): Buffer => {
  const { width, height, hues } = seed;
  const [sky, ground] = hues;
  return Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}">
  <defs>
    <linearGradient id="sky" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="oklch(0.85 0.08 ${sky})"/>
      <stop offset="1" stop-color="oklch(0.65 0.14 ${sky})"/>
    </linearGradient>
  </defs>
  <rect width="100%" height="100%" fill="url(#sky)"/>
  <circle cx="${width * 0.68}" cy="${height * 0.42}" r="${Math.min(width, height) * 0.12}" fill="oklch(0.93 0.06 85)"/>
  <rect y="${height * 0.62}" width="100%" height="${height * 0.38}" fill="oklch(0.4 0.09 ${ground})"/>
</svg>`);
};

rmSync(OUT, { force: true, recursive: true });

const photos = await Promise.all(
  SEEDS.map(async (seed, index) => {
    const id = sha256(`gallery-fixture-${index}`);
    const dir = path.join(OUT, "photos", id);
    mkdirSync(dir, { recursive: true });
    const original = drawing(seed);
    const sizes = Object.fromEntries(
      await Promise.all(
        Object.entries(WIDTHS).map(async ([key, longEdge]) => {
          const { data, info } = await sharp(original)
            .resize(longEdge, longEdge, {
              fit: "inside",
              withoutEnlargement: true,
            })
            .webp({ quality: 60 })
            .toBuffer({ resolveWithObject: true });
          const file = `${key}-${sha256(data)}.webp`;
          writeFileSync(path.join(dir, file), data);
          return [
            key,
            {
              height: info.height,
              path: `photos/${id}/${file}`,
              width: info.width,
            },
          ] as const;
        })
      )
    );
    const { alt, capturedAt, film, publishedAt } = seed;
    return { alt, capturedAt, film, id, publishedAt, sizes };
  })
);

writeFileSync(
  path.join(OUT, "v1.json"),
  `${JSON.stringify(
    { generatedAt: "2026-11-10T18:05:00.000Z", photos, version: 1 },
    null,
    2
  )}\n`
);
