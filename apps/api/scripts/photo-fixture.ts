import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";

import { Effect } from "effect";
import { Digest } from "effect/crypto";

import { variantKinds } from "../src/photos/domain.ts";
import { fixture } from "../tests/support/fixture.ts";

export const writePhotoFixture = async (dir: string) => {
  const { bodies, manifest } = await Effect.runPromise(
    fixture().pipe(Effect.provide(Digest.layerWebCrypto))
  );
  mkdirSync(dir, { recursive: true });
  writeFileSync(path.join(dir, "manifest.json"), JSON.stringify(manifest));
  for (const kind of variantKinds) {
    writeFileSync(path.join(dir, kind), bodies[kind]);
  }
};

if (import.meta.main) {
  const [dir] = process.argv.slice(2);
  if (!dir) {
    throw new Error("Usage: bun scripts/photo-fixture.ts <dir>");
  }
  await writePhotoFixture(dir);
}
