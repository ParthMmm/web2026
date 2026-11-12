import * as Cloudflare from "@alchemy.run/cloudflare";
import { RuntimeContext } from "alchemy/RuntimeContext";
import { Effect, Layer } from "effect";

import {
  BlobKey,
  BlobMissing,
  BlobRejected,
  Blobs,
  BlobUnavailable,
} from "./blobs.ts";
import type { Expected } from "./blobs.ts";

const R2_BAD_DIGEST = /\(10037\)/u;

const hex = (buffer: ArrayBuffer) =>
  Array.from(new Uint8Array(buffer), (byte) =>
    byte.toString(16).padStart(2, "0")
  ).join("");

const unavailable = (operation: string) => (cause: unknown) =>
  new BlobUnavailable({ cause, operation });

const rejectedOr = (key: BlobKey, operation: string) => (cause: unknown) =>
  cause instanceof Cloudflare.R2.R2Error && R2_BAD_DIGEST.test(cause.message)
    ? new BlobRejected({ key })
    : new BlobUnavailable({ cause, operation });

// The binding reads its env lazily and never touches RuntimeContext, so the phantom only satisfies the type.
const runtime = Effect.provide(RuntimeContext.phantom);

export const r2Blobs = (
  bucket: Cloudflare.R2.ReadWriteBucketClient
): Layer.Layer<Blobs> => {
  const write = (
    key: BlobKey,
    body: Parameters<typeof bucket.put>[1],
    expect: Expected
  ) =>
    bucket
      .put(key, body, { contentLength: expect.bytes, sha256: expect.sha256 })
      .pipe(runtime);

  return Layer.succeed(
    Blobs,
    Blobs.of({
      copy: (from, to, expect) =>
        Effect.gen(function* copy() {
          const source = yield* bucket
            .get(from)
            .pipe(runtime, Effect.mapError(unavailable("copy.get")));
          if (source === null) {
            return yield* new BlobMissing({ key: from });
          }
          if (source.size !== expect.bytes) {
            return yield* new BlobRejected({ key: from });
          }
          yield* write(to, source.body, expect).pipe(
            Effect.mapError(rejectedOr(to, "copy.put"))
          );
        }),
      holds: (key, expect) =>
        bucket.head(key).pipe(
          runtime,
          Effect.map(
            (object) =>
              object !== null &&
              object.size === expect.bytes &&
              object.checksums.sha256 !== undefined &&
              hex(object.checksums.sha256) === expect.sha256
          ),
          Effect.mapError(unavailable("head"))
        ),
      list: (prefix) =>
        Effect.gen(function* list() {
          const keys: BlobKey[] = [];
          let cursor: string | undefined;
          do {
            const page = yield* bucket
              .list(cursor === undefined ? { prefix } : { cursor, prefix })
              .pipe(runtime, Effect.mapError(unavailable("list")));
            for (const object of page.objects) {
              keys.push(BlobKey.make(object.key));
            }
            cursor = page.truncated ? page.cursor : undefined;
          } while (cursor !== undefined);
          return keys;
        }),
      put: (key, body, expect) =>
        write(key, body, expect).pipe(
          Effect.asVoid,
          Effect.mapError(rejectedOr(key, "put"))
        ),
      range: (key, offset, length) =>
        bucket.get(key, { range: { length, offset } }).pipe(
          runtime,
          Effect.flatMap((object) =>
            object === null ? Effect.succeed(null) : object.bytes()
          ),
          Effect.mapError(unavailable("range"))
        ),
      read: (key) =>
        bucket.get(key).pipe(
          runtime,
          Effect.map((object) =>
            object === null ? null : { body: object.body, size: object.size }
          ),
          Effect.mapError(unavailable("read"))
        ),
      remove: (keys) =>
        keys.length === 0
          ? Effect.void
          : bucket
              .delete([...keys])
              .pipe(runtime, Effect.mapError(unavailable("delete"))),
    })
  );
};
