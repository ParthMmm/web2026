import { Effect, Layer, Stream } from "effect";
import { Digest } from "effect/crypto";

import { BlobMissing, BlobRejected, Blobs, BlobUnavailable } from "./blobs.ts";
import type { BlobKey, Expected } from "./blobs.ts";
import type { Sha256 } from "./domain.ts";

export type BlobOperation = "put" | "copy" | "holds" | "prefix" | "remove";

interface StoredBlob {
  readonly bytes: Uint8Array;
  readonly sha256: Sha256;
}

const concat = (chunks: ReadonlyArray<Uint8Array>): Uint8Array => {
  const out = new Uint8Array(chunks.reduce((total, chunk) => total + chunk.length, 0));
  let at = 0;
  for (const chunk of chunks) {
    out.set(chunk, at);
    at += chunk.length;
  }
  return out;
};

export const makeMemoryBlobs = () => {
  const objects = new Map<BlobKey, StoredBlob>();
  const pending = new Map<BlobOperation, Effect.Effect<void, BlobUnavailable>>();

  const faults = {
    beforeNext: (operation: BlobOperation, effect: Effect.Effect<void, BlobUnavailable>) => {
      pending.set(operation, effect);
    },
    corrupt: (key: BlobKey) => {
      const blob = objects.get(key);
      if (blob !== undefined) {
        const bytes = Uint8Array.from(blob.bytes);
        bytes[bytes.length - 1] = (bytes.at(-1) ?? 0) ^ 0xff;
        objects.set(key, { ...blob, bytes });
      }
    },
    failNext: (operation: BlobOperation) => {
      pending.set(
        operation,
        Effect.fail(new BlobUnavailable({ cause: "injected fault", operation })),
      );
    },
  };

  const fault = (operation: BlobOperation) =>
    Effect.suspend(() => {
      const effect = pending.get(operation);
      pending.delete(operation);
      return effect ?? Effect.void;
    });

  const layer = Layer.effect(
    Blobs,
    Effect.gen(function* () {
      const digest = yield* Digest.Digest;

      const verified = (key: BlobKey, bytes: Uint8Array, expect: Expected) =>
        Effect.gen(function* () {
          const sha256 = yield* digest.digestHex(Digest.sha256, bytes);
          if (bytes.length !== expect.bytes || sha256 !== expect.sha256) {
            return yield* new BlobRejected({ key });
          }
          return { bytes, sha256: expect.sha256 };
        });

      return Blobs.of({
        copy: (from, to, expect) =>
          Effect.gen(function* () {
            yield* fault("copy");
            const source = objects.get(from);
            if (source === undefined) {
              return yield* new BlobMissing({ key: from });
            }
            objects.set(to, yield* verified(to, source.bytes, expect));
          }),
        holds: (key, expect) =>
          fault("holds").pipe(
            Effect.map(() => {
              const blob = objects.get(key);
              return blob?.bytes.length === expect.bytes && blob.sha256 === expect.sha256;
            }),
          ),
        prefix: (key, length) =>
          fault("prefix").pipe(Effect.map(() => objects.get(key)?.bytes.slice(0, length) ?? null)),
        put: (key, body, expect) =>
          Effect.gen(function* () {
            yield* fault("put");
            const bytes = concat(yield* Stream.runCollect(body));
            objects.set(key, yield* verified(key, bytes, expect));
          }),
        remove: (keys) =>
          fault("remove").pipe(
            Effect.map(() => {
              for (const key of keys) {
                objects.delete(key);
              }
            }),
          ),
      });
    }),
  );

  return { faults, layer, objects };
};
