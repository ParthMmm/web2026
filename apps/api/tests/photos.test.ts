import { readFileSync } from "node:fs";

import { expect, it } from "@effect/vitest";
import { Effect, Layer, Stream } from "effect";
import { Digest } from "effect/crypto";
import { SqlClient } from "effect/sql";

import { makeMemoryBlobs } from "../src/photos/blobs-memory.ts";
import type { BlobKey } from "../src/photos/blobs.ts";
import { variantKinds } from "../src/photos/domain.ts";
import type { PhotoId, VariantKind } from "../src/photos/domain.ts";
import { Photos } from "../src/photos/photos.ts";
import type { Bodies } from "./support/fixture.ts";
import {
  animated,
  declare,
  EDGES,
  fixture,
  validBodies,
  webp,
  withExif,
} from "./support/fixture.ts";
import { migratedSqlite } from "./support/sqlite.ts";

const harness = () => {
  const blobs = makeMemoryBlobs();
  const layer = Photos.layer.pipe(
    Layer.provideMerge(blobs.layer),
    Layer.provideMerge(migratedSqlite()),
    Layer.provideMerge(Digest.layerWebCrypto)
  );
  return { ...blobs, layer };
};

const uploadAll = (
  photoId: PhotoId,
  bodies: Bodies,
  kinds: readonly VariantKind[] = variantKinds
) =>
  Effect.gen(function* uploadVariants() {
    const photos = yield* Photos;
    for (const kind of kinds) {
      yield* photos.store({
        body: Stream.make(bodies[kind]),
        contentLength: bodies[kind].length,
        kind,
        photoId,
      });
    }
  });

const missingOf = (photoId: PhotoId) =>
  Photos.use((photos) => photos.get(photoId)).pipe(
    Effect.map((view) => (view._tag === "Uploading" ? view.missing : []))
  );

const keysUnder = (objects: ReadonlyMap<BlobKey, unknown>, prefix: string) =>
  [...objects.keys()].filter((key) => key.startsWith(prefix));

const expireStaging = (objects: Map<BlobKey, unknown>) => {
  for (const key of keysUnder(objects, "staging/")) {
    objects.delete(key);
  }
};

const rejectedProblems = (photoId: PhotoId) =>
  Photos.use((photos) => photos.finalize(photoId)).pipe(
    Effect.flip,
    Effect.map((failure) =>
      failure._tag === "FinalizeRejected" ? failure.problems : failure
    )
  );

it.effect(
  "finalize makes a fully uploaded photo a draft, and every later call returns the same draft",
  () => {
    const { layer, objects } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* fixture();

      const begun = yield* photos.begin(manifest);
      expect(begun).toMatchObject({
        _tag: "Uploading",
        missing: variantKinds,
        photoId,
      });

      yield* uploadAll(photoId, bodies);
      expect(yield* missingOf(photoId)).toEqual([]);

      const first = yield* photos.finalize(photoId);
      expect(first).toMatchObject({ _tag: "Draft", manifest, photoId });
      expect(yield* photos.finalize(photoId)).toEqual(first);
      expect(yield* photos.get(photoId)).toEqual(first);
      expect(yield* photos.begin(manifest)).toEqual(first);
      expect(keysUnder(objects, "photos/")).toHaveLength(variantKinds.length);
      expect(keysUnder(objects, "staging/")).toEqual([]);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "store rejects bytes that do not hash to the declared sha256 and keeps nothing",
  () => {
    const { layer, objects } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* fixture();
      yield* photos.begin(manifest);

      const tampered = Uint8Array.from(bodies.original);
      tampered[0] = 0xff - (tampered[0] ?? 0);
      const failure = yield* photos
        .store({
          body: Stream.make(tampered),
          contentLength: tampered.length,
          kind: "original",
          photoId,
        })
        .pipe(Effect.flip);

      expect(failure).toMatchObject({ _tag: "HashMismatch", kind: "original" });
      expect(objects.size).toBe(0);
      expect(yield* missingOf(photoId)).toEqual(variantKinds);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "store rejects a declared length that differs before reading the body",
  () => {
    const { layer, objects } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* fixture();
      yield* photos.begin(manifest);

      const failure = yield* photos
        .store({
          body: Stream.die("the body must not be read"),
          contentLength: bodies.w480.length + 1,
          kind: "w480",
          photoId,
        })
        .pipe(Effect.flip);

      expect(failure).toMatchObject({
        _tag: "LengthMismatch",
        expected: bodies.w480.length,
        kind: "w480",
        received: bodies.w480.length + 1,
      });
      expect(objects.size).toBe(0);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "store names a body that ends early a LengthMismatch with the bytes counted",
  () => {
    const { layer, objects } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* fixture();
      yield* photos.begin(manifest);

      const failure = yield* photos
        .store({
          body: Stream.make(bodies.original.subarray(0, 100)),
          contentLength: bodies.original.length,
          kind: "original",
          photoId,
        })
        .pipe(Effect.flip);

      expect(failure).toMatchObject({
        _tag: "LengthMismatch",
        kind: "original",
        received: 100,
      });
      expect(objects.size).toBe(0);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "store, get and finalize answer PhotoNotFound for an undeclared photo",
  () => {
    const { layer } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const { bodies, photoId } = yield* fixture();
      const store = photos.store({
        body: Stream.make(bodies.original),
        contentLength: bodies.original.length,
        kind: "original",
        photoId,
      });
      const calls: readonly Effect.Effect<unknown, unknown>[] = [
        store,
        photos.get(photoId),
        photos.finalize(photoId),
      ];
      for (const call of calls) {
        expect(yield* Effect.flip(call)).toMatchObject({
          _tag: "PhotoNotFound",
          photoId,
        });
      }
    }).pipe(Effect.provide(layer));
  }
);

it.effect("store answers PhotoAlreadyDraft after finalize", () => {
  const { layer } = harness();
  return Effect.gen(function* run() {
    const photos = yield* Photos;
    const { bodies, manifest, photoId } = yield* fixture();
    yield* photos.begin(manifest);
    yield* uploadAll(photoId, bodies);
    yield* photos.finalize(photoId);

    const failure = yield* uploadAll(photoId, bodies, ["w480"]).pipe(
      Effect.flip
    );
    expect(failure).toMatchObject({ _tag: "PhotoAlreadyDraft", photoId });
  }).pipe(Effect.provide(layer));
});

it.effect(
  "finalize lists every missing variant and leaves the photo uploading",
  () => {
    const { layer } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* fixture();
      yield* photos.begin(manifest);
      yield* uploadAll(photoId, bodies, ["original", "w1200"]);

      expect(yield* rejectedProblems(photoId)).toEqual([
        { _tag: "Missing", kind: "w480" },
        { _tag: "Missing", kind: "w2400" },
      ]);
      expect((yield* photos.get(photoId))._tag).toBe("Uploading");
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "finalize rejects a derivative whose header declares other dimensions",
  () => {
    const { layer, objects } = harness();
    return Effect.gen(function* run() {
      const { bodies, manifest, photoId } = yield* declare({
        ...validBodies(),
        w480: webp("VP8 ", EDGES.w480[0] + 1, EDGES.w480[1]),
      });
      yield* Photos.use((photos) => photos.begin(manifest));
      yield* uploadAll(photoId, bodies);

      expect(yield* rejectedProblems(photoId)).toEqual([
        {
          _tag: "DimensionsMismatch",
          expected: { height: 320, width: 480 },
          found: { height: 320, width: 481 },
          kind: "w480",
        },
      ]);
      expect(keysUnder(objects, "photos/")).toEqual([]);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "finalize rejects derivatives that are not WebP, carry metadata, or are animated",
  () => {
    const { layer } = harness();
    return Effect.gen(function* run() {
      const { bodies, manifest, photoId } = yield* declare({
        ...validBodies(),
        w1200: webp("VP8X", ...EDGES.w1200, withExif),
        w2400: webp("VP8X", ...EDGES.w2400, animated),
        w480: Uint8Array.from(validBodies(1).original.subarray(0, 64)),
      });
      yield* Photos.use((photos) => photos.begin(manifest));
      yield* uploadAll(photoId, bodies);

      expect(yield* rejectedProblems(photoId)).toEqual([
        { _tag: "NotWebp", kind: "w480" },
        { _tag: "HasMetadata", kind: "w1200" },
        { _tag: "Animated", kind: "w2400" },
      ]);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "finalize reports a staged object whose bytes rotted as Corrupt and promotes nothing for it",
  () => {
    const { faults, layer, objects } = harness();
    return Effect.gen(function* run() {
      const { bodies, manifest, photoId } = yield* fixture();
      yield* Photos.use((photos) => photos.begin(manifest));
      yield* uploadAll(photoId, bodies);
      for (const staged of keysUnder(objects, `staging/${photoId}/w1200-`)) {
        faults.corrupt(staged);
      }

      expect(yield* rejectedProblems(photoId)).toEqual([
        { _tag: "Corrupt", kind: "w1200" },
      ]);
      expect((yield* Photos.use((photos) => photos.get(photoId)))._tag).toBe(
        "Uploading"
      );
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "begin replaces the manifest while uploading, and an upload for the old bytes no longer counts",
  () => {
    const { layer } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const first = yield* fixture();
      yield* photos.begin(first.manifest);
      yield* uploadAll(first.photoId, first.bodies);

      const rerendered = yield* declare({
        ...first.bodies,
        w480: webp("VP8L", ...EDGES.w480),
      });
      const view = yield* photos.begin(rerendered.manifest);

      expect(view).toMatchObject({
        _tag: "Uploading",
        manifest: rerendered.manifest,
        missing: ["w480"],
      });
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "begin with a re-rendered manifest reopens a draft, and the next finalize removes the superseded objects",
  () => {
    const { layer, objects } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const first = yield* fixture();
      yield* photos.begin(first.manifest);
      yield* uploadAll(first.photoId, first.bodies);
      yield* photos.finalize(first.photoId);

      const rerendered = yield* declare({
        ...first.bodies,
        w480: webp("VP8L", ...EDGES.w480),
      });
      expect(yield* photos.begin(rerendered.manifest)).toMatchObject({
        _tag: "Uploading",
        manifest: rerendered.manifest,
        missing: ["w480"],
      });
      yield* uploadAll(first.photoId, rerendered.bodies, ["w480"]);
      const draft = yield* photos.finalize(first.photoId);

      expect(draft).toMatchObject({
        _tag: "Draft",
        manifest: rerendered.manifest,
      });
      expect(keysUnder(objects, `photos/${first.photoId}/`).toSorted()).toEqual(
        variantKinds
          .map(
            (kind) =>
              `photos/${first.photoId}/${kind}-${rerendered.manifest[kind].sha256}`
          )
          .toSorted()
      );
      expect(keysUnder(objects, "staging/")).toEqual([]);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "a finalize whose cleanup of superseded objects fails still returns the draft, and finalize again removes them",
  () => {
    const { faults, layer, objects } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const first = yield* fixture();
      yield* photos.begin(first.manifest);
      yield* uploadAll(first.photoId, first.bodies);
      yield* photos.finalize(first.photoId);
      const rerendered = yield* declare({
        ...first.bodies,
        w2400: webp("VP8L", ...EDGES.w2400),
      });
      yield* photos.begin(rerendered.manifest);
      yield* uploadAll(first.photoId, rerendered.bodies, ["w2400"]);

      faults.failNext("list");
      const draft = yield* photos.finalize(first.photoId);
      expect(draft._tag).toBe("Draft");
      expect(keysUnder(objects, `photos/${first.photoId}/`)).toHaveLength(
        variantKinds.length + 1
      );

      expect(yield* photos.finalize(first.photoId)).toEqual(draft);
      expect(keysUnder(objects, `photos/${first.photoId}/`)).toHaveLength(
        variantKinds.length
      );
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "a finalize that fails part way through promotion converges when it runs again",
  () => {
    const { faults, layer, objects } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* fixture();
      yield* photos.begin(manifest);
      yield* uploadAll(photoId, bodies);

      faults.failNext("copy");
      expect(yield* Effect.flip(photos.finalize(photoId))).toMatchObject({
        _tag: "StorageUnavailable",
        operation: "copy",
      });
      expect((yield* photos.get(photoId))._tag).toBe("Uploading");

      expect((yield* photos.finalize(photoId))._tag).toBe("Draft");
      expect(keysUnder(objects, "photos/")).toHaveLength(variantKinds.length);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "a finalize that promoted everything but failed to mark converges after staging has expired",
  () => {
    const { layer, objects } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const sql = yield* SqlClient.SqlClient;
      const { bodies, manifest, photoId } = yield* fixture();
      yield* photos.begin(manifest);
      yield* uploadAll(photoId, bodies);

      yield* sql.unsafe(
        "CREATE TRIGGER refuse_mark BEFORE UPDATE OF finalized_at ON photo BEGIN SELECT RAISE(ABORT, 'injected'); END"
      );
      expect(yield* Effect.flip(photos.finalize(photoId))).toMatchObject({
        _tag: "StorageUnavailable",
        operation: "ledger.markFinal",
      });
      yield* sql.unsafe("DROP TRIGGER refuse_mark");
      expireStaging(objects);

      expect(yield* missingOf(photoId)).toEqual([]);
      expect((yield* photos.finalize(photoId))._tag).toBe("Draft");
      expect(keysUnder(objects, "photos/")).toHaveLength(variantKinds.length);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "store answers StorageUnavailable when storage fails before the body is read",
  () => {
    const { faults, layer } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* fixture();
      yield* photos.begin(manifest);
      faults.failNext("put");
      const failure = yield* Effect.flip(
        photos.store({
          body: Stream.make(bodies.w480),
          contentLength: bodies.w480.length,
          kind: "w480",
          photoId,
        })
      );
      expect(failure).toMatchObject({
        _tag: "StorageUnavailable",
        operation: "put",
      });
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "a finalize that loses a race to a finished finalize returns the draft",
  () => {
    const { faults, layer, objects } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* fixture();
      yield* photos.begin(manifest);
      yield* uploadAll(photoId, bodies);
      faults.beforeNext(
        "copy",
        photos.finalize(photoId).pipe(Effect.asVoid, Effect.orDie)
      );
      expect((yield* photos.finalize(photoId))._tag).toBe("Draft");
      expect(keysUnder(objects, "photos/")).toHaveLength(variantKinds.length);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "a final object promoted for an older manifest does not satisfy a re-declared one",
  () => {
    const { layer, objects } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const sql = yield* SqlClient.SqlClient;
      const first = yield* fixture();
      yield* photos.begin(first.manifest);
      yield* uploadAll(first.photoId, first.bodies);
      yield* sql.unsafe(
        "CREATE TRIGGER refuse_mark BEFORE UPDATE OF finalized_at ON photo BEGIN SELECT RAISE(ABORT, 'injected'); END"
      );
      yield* Effect.flip(photos.finalize(first.photoId));
      yield* sql.unsafe("DROP TRIGGER refuse_mark");
      expireStaging(objects);

      const rerendered = yield* declare({
        ...first.bodies,
        w2400: webp("VP8L", ...EDGES.w2400),
      });
      const view = yield* photos.begin(rerendered.manifest);

      expect(view).toMatchObject({ _tag: "Uploading", missing: ["w2400"] });
      expect(yield* rejectedProblems(first.photoId)).toEqual([
        { _tag: "Missing", kind: "w2400" },
      ]);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "a manifest re-declared between inspection and the mark fails finalize with PhotoManifestConflict",
  () => {
    const { faults, layer } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const first = yield* fixture();
      yield* photos.begin(first.manifest);
      yield* uploadAll(first.photoId, first.bodies);
      const rerendered = yield* declare({
        ...first.bodies,
        w480: webp("VP8L", ...EDGES.w480),
      });

      faults.beforeNext(
        "copy",
        photos.begin(rerendered.manifest).pipe(Effect.asVoid, Effect.orDie)
      );

      expect(yield* Effect.flip(photos.finalize(first.photoId))).toMatchObject({
        _tag: "PhotoManifestConflict",
        photoId: first.photoId,
      });
      expect(yield* photos.get(first.photoId)).toMatchObject({
        _tag: "Uploading",
        manifest: rerendered.manifest,
        missing: ["w480"],
      });
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "finalize rejects an extended WebP that hides an EXIF chunk behind a cleared header flag",
  () => {
    const { layer, objects } = harness();
    return Effect.gen(function* run() {
      const hidden = Uint8Array.from(
        readFileSync(new URL("fixtures/webp/exif.webp", import.meta.url))
      );
      // Byte 20 holds the VP8X feature flags. The EXIF chunk itself stays in the file.
      hidden[20] = 0;
      const { bodies, manifest, photoId } = yield* declare({
        ...validBodies(),
        w480: hidden,
      });
      yield* Photos.use((photos) => photos.begin(manifest));
      yield* uploadAll(photoId, bodies);

      expect(yield* rejectedProblems(photoId)).toEqual([
        { _tag: "HasMetadata", kind: "w480" },
      ]);
      expect(keysUnder(objects, "photos/")).toEqual([]);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "finalize accepts an extended WebP that holds only an alpha chunk and its image",
  () => {
    const { layer } = harness();
    return Effect.gen(function* run() {
      const { bodies, manifest, photoId } = yield* declare({
        ...validBodies(),
        w480: Uint8Array.from(
          readFileSync(new URL("fixtures/webp/alpha.webp", import.meta.url))
        ),
      });
      const photos = yield* Photos;
      yield* photos.begin(manifest);
      yield* uploadAll(photoId, bodies);

      expect((yield* photos.finalize(photoId))._tag).toBe("Draft");
    }).pipe(Effect.provide(layer));
  }
);
