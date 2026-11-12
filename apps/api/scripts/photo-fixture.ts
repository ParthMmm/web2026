import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";

import { Effect } from "effect";
import { Digest } from "effect/crypto";

import { variantKinds } from "../src/photos/domain.ts";
import { declare, validBodies } from "../tests/support/fixture.ts";

/** Writes a photo's manifest and objects. `originalBytes` sizes the original, as a camera file would be. */
export const writePhotoFixture = async (
  dir: string,
  originalBytes?: number
) => {
  const bodies = validBodies();
  const original =
    originalBytes === undefined
      ? bodies.original
      : Uint8Array.from({ length: originalBytes }, (_, index) => index % 251);
  const { manifest } = await Effect.runPromise(
    declare({ ...bodies, original }).pipe(Effect.provide(Digest.layerWebCrypto))
  );
  mkdirSync(dir, { recursive: true });
  writeFileSync(path.join(dir, "manifest.json"), JSON.stringify(manifest));
  for (const kind of variantKinds) {
    writeFileSync(
      path.join(dir, kind),
      kind === "original" ? original : bodies[kind]
    );
  }
};

if (import.meta.main) {
  const [dir, originalBytes] = process.argv.slice(2);
  if (!dir) {
    throw new Error(
      "Usage: bun scripts/photo-fixture.ts <dir> [originalBytes]"
    );
  }
  await writePhotoFixture(
    dir,
    originalBytes === undefined ? undefined : Number(originalBytes)
  );
}
