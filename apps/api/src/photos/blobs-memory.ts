import { Effect, Layer, Stream } from "effect";
import { Digest } from "effect/crypto";

import {
  BlobMissing,
  BlobRejected,
  Blobs,
  BlobUnavailable,
  PublicBlobs,
} from "./blobs.ts";
import type { BlobKey, BlobStore, Expected } from "./blobs.ts";
import type { Sha256 } from "./domain.ts";

export type BlobOperation =
  | "put"
  | "copy"
  | "holds"
  | "range"
  | "read"
  | "list"
  | "remove";

interface StoredBlob {
  readonly bytes: Uint8Array;
  readonly sha256: Sha256;
}

const concat = (chunks: readonly Uint8Array[]): Uint8Array => {
  const out = new Uint8Array(
    chunks.reduce((total, chunk) => total + chunk.length, 0)
  );
  let at = 0;
  for (const chunk of chunks) {
    out.set(chunk, at);
    at += chunk.length;
  }
  return out;
};

export const makeMemoryBlobs = () => {
  const objects = new Map<BlobKey, StoredBlob>();
  const pending = new Map<
    BlobOperation,
    Effect.Effect<void, BlobUnavailable>
  >();

  const faults = {
    beforeNext: (
      operation: BlobOperation,
      effect: Effect.Effect<void, BlobUnavailable>
    ) => {
      pending.set(operation, effect);
    },
    corrupt: (key: BlobKey) => {
      const blob = objects.get(key);
      if (blob !== undefined) {
        const bytes = Uint8Array.from(blob.bytes);
        bytes[bytes.length - 1] = 0xff - (bytes.at(-1) ?? 0);
        objects.set(key, { ...blob, bytes });
      }
    },
    failNext: (operation: BlobOperation) => {
      pending.set(
        operation,
        Effect.fail(new BlobUnavailable({ cause: "injected fault", operation }))
      );
    },
  };

  const fault = (operation: BlobOperation) =>
    Effect.suspend(() => {
      const effect = pending.get(operation);
      pending.delete(operation);
      return effect ?? Effect.void;
    });

  const store: Effect.Effect<BlobStore, never, Digest.Digest> = Effect.gen(
    function* buildBlobs() {
      const digest = yield* Digest.Digest;

      const verified = (key: BlobKey, bytes: Uint8Array, expect: Expected) =>
        Effect.gen(function* verifyBlob() {
          const sha256 = yield* digest.digestHex(Digest.sha256, bytes);
          if (bytes.length !== expect.bytes || sha256 !== expect.sha256) {
            return yield* new BlobRejected({ key });
          }
          return { bytes, sha256: expect.sha256 };
        });

      return {
        copy: (from, to, expect) =>
          Effect.gen(function* copy() {
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
              return (
                blob?.bytes.length === expect.bytes &&
                blob.sha256 === expect.sha256
              );
            })
          ),
        list: (prefix) =>
          fault("list").pipe(
            Effect.map(() =>
              [...objects.keys()]
                .filter((key) => key.startsWith(prefix))
                .toSorted()
            )
          ),
        put: (key, body, expect) =>
          Effect.gen(function* put() {
            yield* fault("put");
            const bytes = concat(
              yield* Stream.runCollect(body).pipe(
                Effect.mapError(
                  (cause) => new BlobUnavailable({ cause, operation: "put" })
                )
              )
            );
            objects.set(key, yield* verified(key, bytes, expect));
          }),
        range: (key, offset, length) =>
          fault("range").pipe(
            Effect.map(
              () =>
                objects.get(key)?.bytes.slice(offset, offset + length) ?? null
            )
          ),
        read: (key) =>
          fault("read").pipe(
            Effect.map(() => {
              const blob = objects.get(key);
              return blob === undefined
                ? null
                : { body: Stream.make(blob.bytes), size: blob.bytes.length };
            })
          ),
        remove: (keys) =>
          fault("remove").pipe(
            Effect.andThen(
              Effect.sync(() => {
                for (const key of keys) {
                  objects.delete(key);
                }
              })
            )
          ),
      };
    }
  );

  return {
    faults,
    /** This store as the private bucket. */
    layer: Layer.effect(Blobs, store),
    objects,
    /** This store as the public bucket. */
    publicLayer: Layer.effect(PublicBlobs, store),
  };
};
