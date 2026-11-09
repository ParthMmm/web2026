import { deepStrictEqual, strictEqual } from "node:assert";

import { Effect, Exit, Stream } from "effect";
import { Digest } from "effect/crypto";

import { BlobKey, Blobs } from "../../src/photos/blobs.ts";
import { ByteCount, Sha256 } from "../../src/photos/domain.ts";

export interface BlobsCase {
  readonly name: string;
  readonly run: (
    key: (name: string) => BlobKey
  ) => Effect.Effect<void, unknown, Blobs>;
}

const bytesOf = (length: number) =>
  Uint8Array.from({ length }, (_, index) => (index * 7) % 256);

const expectFor = (bytes: Uint8Array) =>
  Effect.gen(function* digestExpected() {
    const digest = yield* Digest.Digest;
    return {
      bytes: ByteCount.make(bytes.length),
      sha256: Sha256.make(yield* digest.digestHex(Digest.sha256, bytes)),
    };
  }).pipe(Effect.provide(Digest.layerWebCrypto));

const WRONG_SHA = Sha256.make("00".repeat(32));

const chunked = (bytes: Uint8Array, size: number) =>
  Stream.fromIterable(
    Array.from({ length: Math.ceil(bytes.length / size) }, (_, index) =>
      bytes.subarray(index * size, (index + 1) * size)
    )
  );

export const blobsContract: readonly BlobsCase[] = [
  {
    name: "put keeps exactly the verified bytes, streamed in chunks",
    run: (key) =>
      Effect.gen(function* run() {
        const blobs = yield* Blobs;
        const bytes = bytesOf(70_000);
        const expected = yield* expectFor(bytes);
        yield* blobs.put(key("a"), chunked(bytes, 4096), expected);
        strictEqual(yield* blobs.holds(key("a"), expected), true);
        deepStrictEqual(yield* blobs.prefix(key("a"), 16), bytes.slice(0, 16));
      }),
  },
  {
    name: "put with a wrong sha256 is rejected and stores nothing",
    run: (key) =>
      Effect.gen(function* run() {
        const blobs = yield* Blobs;
        const bytes = bytesOf(5000);
        const wrong = {
          bytes: ByteCount.make(bytes.length),
          sha256: WRONG_SHA,
        };
        const failure = yield* blobs
          .put(key("b"), Stream.make(bytes), wrong)
          .pipe(Effect.flip);
        strictEqual(failure._tag, "BlobRejected");
        strictEqual(yield* blobs.prefix(key("b"), 1), null);
      }),
  },
  {
    name: "put with a body shorter than declared fails and stores nothing",
    run: (key) =>
      Effect.gen(function* run() {
        const blobs = yield* Blobs;
        const bytes = bytesOf(5000);
        const exit = yield* Effect.exit(
          blobs.put(
            key("c"),
            Stream.make(bytes.slice(0, 100)),
            yield* expectFor(bytes)
          )
        );
        strictEqual(Exit.isFailure(exit), true);
        strictEqual(yield* blobs.prefix(key("c"), 1), null);
      }),
  },
  {
    name: "holds is false for an absent key, another size, or another hash",
    run: (key) =>
      Effect.gen(function* run() {
        const blobs = yield* Blobs;
        const bytes = bytesOf(3000);
        const expected = yield* expectFor(bytes);
        strictEqual(yield* blobs.holds(key("d"), expected), false);
        yield* blobs.put(key("d"), Stream.make(bytes), expected);
        strictEqual(
          yield* blobs.holds(key("d"), { ...expected, sha256: WRONG_SHA }),
          false
        );
        strictEqual(
          yield* blobs.holds(key("d"), {
            ...expected,
            bytes: ByteCount.make(2999),
          }),
          false
        );
      }),
  },
  {
    name: "copy verifies again and leaves no destination on a wrong hash",
    run: (key) =>
      Effect.gen(function* run() {
        const blobs = yield* Blobs;
        const bytes = bytesOf(3000);
        const expected = yield* expectFor(bytes);
        yield* blobs.put(key("from"), Stream.make(bytes), expected);

        yield* blobs.copy(key("from"), key("to"), expected);
        strictEqual(yield* blobs.holds(key("to"), expected), true);

        const bad = yield* blobs
          .copy(key("from"), key("to-bad"), { ...expected, sha256: WRONG_SHA })
          .pipe(Effect.flip);
        strictEqual(bad._tag, "BlobRejected");
        strictEqual(yield* blobs.prefix(key("to-bad"), 1), null);

        const missing = yield* blobs
          .copy(key("absent"), key("to-2"), expected)
          .pipe(Effect.flip);
        strictEqual(missing._tag, "BlobMissing");
      }),
  },
  {
    name: "remove deletes keys and ignores absent ones",
    run: (key) =>
      Effect.gen(function* run() {
        const blobs = yield* Blobs;
        const bytes = bytesOf(10);
        const expected = yield* expectFor(bytes);
        yield* blobs.put(key("gone"), Stream.make(bytes), expected);
        yield* blobs.remove([key("gone"), key("never")]);
        strictEqual(yield* blobs.holds(key("gone"), expected), false);
      }),
  },
];

export const contractKey = (scope: string) => (name: string) =>
  BlobKey.make(`${scope}/${name}`);
