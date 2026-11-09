import { deepStrictEqual, ok, strictEqual } from "node:assert";

import { Effect, Stream } from "effect";
import { Digest } from "effect/crypto";

import { variantKinds } from "../../src/photos/domain.ts";
import type { Manifest, PhotoId, VariantKind } from "../../src/photos/domain.ts";
import { Photos } from "../../src/photos/photos.ts";
import type { Bodies } from "../support/fixture.ts";
import { declare, EDGES, validBodies, webp } from "../support/fixture.ts";

export interface PhotosScenario {
  readonly name: string;
  readonly run: Effect.Effect<void, unknown, Photos>;
}

const declared = (seed: number) =>
  declare(validBodies(seed)).pipe(Effect.provide(Digest.layerWebCrypto));

const upload = (
  photoId: PhotoId,
  bodies: Bodies,
  kinds: ReadonlyArray<VariantKind> = variantKinds,
) =>
  Effect.forEach(
    kinds,
    (kind) =>
      Photos.use((photos) =>
        photos.store({
          body: Stream.make(bodies[kind]),
          contentLength: bodies[kind].length,
          kind,
          photoId,
        }),
      ),
    { discard: true },
  );

export const photosScenarios: ReadonlyArray<PhotosScenario> = [
  {
    name: "a fully uploaded photo becomes a draft once, and finalize again returns the same draft",
    run: Effect.gen(function* () {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* declared(1);
      const begun = yield* photos.begin(manifest);
      deepStrictEqual(begun._tag === "Uploading" && begun.missing, variantKinds);
      yield* upload(photoId, bodies);
      const first = yield* photos.finalize(photoId);
      strictEqual(first._tag, "Draft");
      deepStrictEqual(yield* photos.finalize(photoId), first);
      deepStrictEqual(yield* photos.begin(manifest), first);
    }),
  },
  {
    name: "begin replaces the manifest while uploading and refuses another manifest for a draft",
    run: Effect.gen(function* () {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* declared(2);
      yield* photos.begin(manifest);
      const rerendered = yield* declare({ ...bodies, w480: webp("VP8L", ...EDGES.w480) }).pipe(
        Effect.provide(Digest.layerWebCrypto),
      );
      const replaced = yield* photos.begin(rerendered.manifest);
      deepStrictEqual(replaced._tag === "Uploading" && replaced.missing, variantKinds);
      yield* upload(photoId, rerendered.bodies);
      yield* photos.finalize(photoId);
      const conflict = yield* Effect.flip(photos.begin(manifest));
      strictEqual(conflict._tag, "PhotoManifestConflict");
    }),
  },
  {
    name: "a wrong-hash upload is HashMismatch and the variant stays missing",
    run: Effect.gen(function* () {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* declared(3);
      yield* photos.begin(manifest);
      const tampered = Uint8Array.from(bodies.w480);
      tampered[40] = (tampered[40] ?? 0) ^ 0xff;
      const failure = yield* Effect.flip(
        photos.store({
          body: Stream.make(tampered),
          contentLength: tampered.length,
          kind: "w480",
          photoId,
        }),
      );
      strictEqual(failure._tag, "HashMismatch");
      const view = yield* photos.get(photoId);
      ok(view._tag === "Uploading" && view.missing.includes("w480"));
    }),
  },
  {
    name: "a body that ends early is LengthMismatch with the bytes R2 was given",
    run: Effect.gen(function* () {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* declared(4);
      yield* photos.begin(manifest);
      const failure = yield* Effect.flip(
        photos.store({
          body: Stream.make(bodies.original.subarray(0, 100)),
          contentLength: bodies.original.length,
          kind: "original",
          photoId,
        }),
      );
      deepStrictEqual(
        failure._tag === "LengthMismatch" && { kind: failure.kind, received: failure.received },
        { kind: "original", received: 100 },
      );
    }),
  },
  {
    name: "a manifest the ledger round-trips through D1 JSON compares equal",
    run: Effect.gen(function* () {
      const photos = yield* Photos;
      const { manifest } = yield* declared(5);
      const view = yield* photos.begin(manifest);
      const stored: Manifest = view.manifest;
      deepStrictEqual(stored, manifest);
      deepStrictEqual(yield* photos.begin(manifest), view);
    }),
  },
];
