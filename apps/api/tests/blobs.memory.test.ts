import { it } from "@effect/vitest";
import { Effect, Layer } from "effect";
import { Digest } from "effect/crypto";

import { makeMemoryBlobs } from "../src/photos/blobs-memory.ts";
import { blobsContract, contractKey } from "./support/blobs-contract.ts";

for (const contract of blobsContract) {
  it.effect(`memory: ${contract.name}`, () =>
    contract
      .run(contractKey("memory"))
      .pipe(
        Effect.provide(
          makeMemoryBlobs().layer.pipe(Layer.provide(Digest.layerWebCrypto))
        )
      )
  );
}
