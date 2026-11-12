/* oxlint-disable max-classes-per-file -- the port and its three failure modes are one contract. */
/* oxlint-disable unicorn/throw-new-error -- Schema.TaggedError is a curried class factory, not an Error constructor. */
import { Context, Schema } from "effect";
import type { Effect, Stream } from "effect";

import type { ByteCount, Sha256 } from "./domain.ts";

export const BlobKey = Schema.String.pipe(Schema.brand("BlobKey"));
export type BlobKey = typeof BlobKey.Type;

export interface BlobBody {
  readonly size: number;
  readonly body: Stream.Stream<Uint8Array, unknown>;
}

export interface Expected {
  readonly bytes: ByteCount;
  readonly sha256: Sha256;
}

export class BlobMissing extends Schema.TaggedError<BlobMissing>()(
  "BlobMissing",
  {
    key: BlobKey,
  }
) {}

export class BlobRejected extends Schema.TaggedError<BlobRejected>()(
  "BlobRejected",
  {
    key: BlobKey,
  }
) {}

export class BlobUnavailable extends Schema.TaggedError<BlobUnavailable>()(
  "BlobUnavailable",
  {
    cause: Schema.Defect(),
    operation: Schema.String,
  }
) {}

/**
 * Every implementation keeps one invariant: an object exists at a key only if
 * a `put` or `copy` saw exactly `expect.bytes` bytes hashing to `expect.sha256`,
 * and the store records that hash. A rejected write leaves nothing behind.
 */
export class Blobs extends Context.Service<
  Blobs,
  {
    readonly put: (
      key: BlobKey,
      body: Stream.Stream<Uint8Array, unknown>,
      expect: Expected
    ) => Effect.Effect<void, BlobRejected | BlobUnavailable>;
    readonly copy: (
      from: BlobKey,
      to: BlobKey,
      expect: Expected
    ) => Effect.Effect<void, BlobMissing | BlobRejected | BlobUnavailable>;
    readonly holds: (
      key: BlobKey,
      expect: Expected
    ) => Effect.Effect<boolean, BlobUnavailable>;
    /** Up to `length` bytes from `offset`, or null when the key holds nothing. */
    readonly range: (
      key: BlobKey,
      offset: number,
      length: number
    ) => Effect.Effect<Uint8Array | null, BlobUnavailable>;
    /** The whole object, or null when the key holds nothing. */
    readonly read: (
      key: BlobKey
    ) => Effect.Effect<BlobBody | null, BlobUnavailable>;
    /** Every key that starts with `prefix`, in key order. */
    readonly list: (
      prefix: string
    ) => Effect.Effect<readonly BlobKey[], BlobUnavailable>;
    readonly remove: (
      keys: readonly BlobKey[]
    ) => Effect.Effect<void, BlobUnavailable>;
  }
>()("@web2026/api/Blobs") {}
