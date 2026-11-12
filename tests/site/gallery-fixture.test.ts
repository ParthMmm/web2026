import { expect, test } from "bun:test";
import { createHash } from "node:crypto";
import path from "node:path";

import sharp from "sharp";

import { SIZE_KEYS } from "../../src/lib/gallery";
import { parseManifest } from "../../src/lib/gallery-manifest";

const FIXTURE = path.join(import.meta.dir, "../fixtures/gallery/public");
const FILE_HASH = /-(?<sha>[0-9a-f]{64})\.webp$/u;

test("the smoke check's seeded gallery follows the manifest contract, file for file", async () => {
  const manifest = parseManifest(
    await Bun.file(path.join(FIXTURE, "v1.json")).json()
  );
  if (manifest === null) {
    throw new Error(
      "tests/fixtures/gallery/public/v1.json does not follow the contract."
    );
  }

  const sizes = manifest.photos.flatMap((photo) =>
    SIZE_KEYS.map((key) => photo.sizes[key])
  );
  const wrong = await Promise.all(
    sizes.map(async (size) => {
      const data = new Uint8Array(
        await Bun.file(path.join(FIXTURE, size.path)).arrayBuffer()
      );
      const { format, width, height } = await sharp(data).metadata();
      const sha = createHash("sha256").update(data).digest("hex");
      return [
        format !== "webp" || width !== size.width || height !== size.height
          ? `${size.path} is ${format} ${width}x${height}`
          : null,
        sha === size.path.match(FILE_HASH)?.groups?.sha
          ? null
          : `${size.path} has hash ${sha}`,
      ];
    })
  );

  expect(manifest.photos.length).toBeGreaterThan(0);
  expect(wrong.flat().filter((problem) => problem !== null)).toEqual([]);
});
