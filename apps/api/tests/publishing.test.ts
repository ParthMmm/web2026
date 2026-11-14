import { expect, it } from "@effect/vitest";
import { Deferred, Effect, Fiber, Layer, Schema, Stream } from "effect";
import { Digest } from "effect/crypto";
import { SqlClient } from "effect/sql";
import { TestClock } from "effect/testing";

import { makeMemoryBlobs } from "../src/photos/blobs-memory.ts";
import { PublicBlobs } from "../src/photos/blobs.ts";
import type { BlobKey, BlobStore } from "../src/photos/blobs.ts";
import { derivativeKinds, variantKinds } from "../src/photos/domain.ts";
import type { PhotoDetails, PhotoId } from "../src/photos/domain.ts";
import { GalleryManifest } from "../src/photos/gallery-manifest.ts";
import { Photos } from "../src/photos/photos.ts";
import { PublicGallery } from "../src/photos/public-gallery.ts";
import { Publishing } from "../src/photos/publishing.ts";
import { declare, validBodies } from "./support/fixture.ts";
import { migratedSqlite } from "./support/sqlite.ts";

const MANIFEST_KEY = "v1.json";

/** Runs `before` and `after` around every put to the public bucket, and `holds` before every check of a public copy. */
interface PutHooks {
  readonly before: (key: BlobKey) => Effect.Effect<void>;
  readonly after: (key: BlobKey) => Effect.Effect<void>;
  readonly holds: (key: BlobKey) => Effect.Effect<void>;
}

const noHooks: PutHooks = {
  after: () => Effect.void,
  before: () => Effect.void,
  holds: () => Effect.void,
};

const hookPuts = (hooks: PutHooks) =>
  Layer.effect(
    PublicBlobs,
    Effect.gen(function* wrapPublicBlobs() {
      const store = yield* PublicBlobs;
      const put: BlobStore["put"] = (key, body, expected) =>
        hooks
          .before(key)
          .pipe(
            Effect.andThen(store.put(key, body, expected)),
            Effect.andThen(hooks.after(key))
          );
      const holds: BlobStore["holds"] = (key, expected) =>
        hooks.holds(key).pipe(Effect.andThen(store.holds(key, expected)));
      return { ...store, holds, put };
    })
  );

const harness = (hooks: PutHooks = noHooks) => {
  const privateBlobs = makeMemoryBlobs();
  const publicBlobs = makeMemoryBlobs();
  const layer = Layer.mergeAll(
    Photos.layer,
    Publishing.layer,
    PublicGallery.layer
  ).pipe(
    Layer.provideMerge(privateBlobs.layer),
    Layer.provideMerge(
      hookPuts(hooks).pipe(Layer.provideMerge(publicBlobs.publicLayer))
    ),
    Layer.provideMerge(migratedSqlite()),
    Layer.provideMerge(Digest.layerWebCrypto)
  );
  return { layer, privateBlobs, publicBlobs };
};

/** A draft of a distinct photo per seed. */
const draft = (seed: number) =>
  Effect.gen(function* makeDraft() {
    const photos = yield* Photos;
    const declared = yield* declare(validBodies(seed));
    yield* photos.begin(declared.manifest);
    for (const kind of variantKinds) {
      yield* photos.store({
        body: Stream.make(declared.bodies[kind]),
        contentLength: declared.bodies[kind].length,
        kind,
        photoId: declared.photoId,
      });
    }
    yield* photos.finalize(declared.photoId);
    return declared;
  });

const publicKeys = (objects: ReadonlyMap<BlobKey, unknown>) =>
  [...objects.keys()].toSorted();

const gallery = Effect.gen(function* readGallery() {
  const { body } = yield* PublicGallery.use((reader) => reader.manifest);
  const chunks = yield* Stream.runCollect(body);
  const text = new TextDecoder().decode(
    Uint8Array.from(chunks.flatMap((chunk) => [...chunk]))
  );
  return {
    manifest: Schema.decodeUnknownSync(Schema.fromJsonString(GalleryManifest))(
      text
    ),
    text,
  };
});

const listedIds = gallery.pipe(
  Effect.map(({ manifest }) => manifest.photos.map((photo) => photo.id))
);

const tagOf = (photoId: PhotoId) =>
  Photos.use((photos) => photos.get(photoId)).pipe(
    Effect.map((view) => view._tag)
  );

const refuseUpdates = (when: string) =>
  SqlClient.SqlClient.use((sql) =>
    sql.unsafe(
      `CREATE TRIGGER refuse BEFORE UPDATE OF publication ON photo WHEN ${when} BEGIN SELECT RAISE(ABORT, 'injected'); END`
    )
  );

const allowUpdates = SqlClient.SqlClient.use((sql) =>
  sql.unsafe("DROP TRIGGER refuse")
);

const details: PhotoDetails = {
  alt: "Fog over the bay",
  capturedAt: "2026-10-03T07:12:00",
  film: "Portra 400",
};

it.effect(
  "publish copies only the three derivatives, lists the photo in the manifest, and returns the published view",
  () => {
    const { layer, publicBlobs } = harness();
    return Effect.gen(function* run() {
      const { manifest, photoId } = yield* draft(1);
      const published = yield* Publishing.use((publishing) =>
        publishing.publish(photoId, details)
      );

      expect(published).toMatchObject({
        _tag: "Published",
        details,
        manifest,
        photoId,
      });
      expect(yield* Photos.use((photos) => photos.get(photoId))).toEqual(
        published
      );
      expect(publicKeys(publicBlobs.objects)).toEqual(
        [
          "v1.json",
          ...derivativeKinds.map(
            (kind) => `photos/${photoId}/${kind}-${manifest[kind].sha256}.webp`
          ),
        ].toSorted()
      );
      const { manifest: listed, text } = yield* gallery;
      expect(listed.photos).toEqual([
        {
          ...details,
          id: photoId,
          publishedAt: published.publishedAt,
          sizes: {
            w1200: {
              height: 800,
              path: `photos/${photoId}/w1200-${manifest.w1200.sha256}.webp`,
              width: 1200,
            },
            w2400: {
              height: 1600,
              path: `photos/${photoId}/w2400-${manifest.w2400.sha256}.webp`,
              width: 2400,
            },
            w480: {
              height: 320,
              path: `photos/${photoId}/w480-${manifest.w480.sha256}.webp`,
              width: 480,
            },
          },
        },
      ]);
      expect(text).not.toMatch(/original|staging\//u);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "publishing again replaces the details, keeps the first publish time, and copies nothing twice",
  () => {
    const { layer, publicBlobs } = harness();
    return Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const { photoId } = yield* draft(1);
      const first = yield* publishing.publish(photoId, details);
      const keys = publicKeys(publicBlobs.objects);

      publicBlobs.faults.failNext("put");
      yield* Effect.flip(publishing.publish(photoId, { alt: "Bay" }));
      const again = yield* publishing.publish(photoId, { alt: "Bay" });

      expect(again).toEqual({ ...first, details: { alt: "Bay" } });
      expect(publicKeys(publicBlobs.objects)).toEqual(keys);
      const { manifest } = yield* gallery;
      expect(manifest.photos).toMatchObject([{ alt: "Bay", id: photoId }]);
      expect(manifest.photos[0]).not.toHaveProperty("film");
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "publish and unpublish refuse an uploading photo with PhotoNotDraft and an unknown one with PhotoNotFound",
  () => {
    const { layer, publicBlobs } = harness();
    return Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const uploading = yield* declare(validBodies(2));
      yield* Photos.use((photos) => photos.begin(uploading.manifest));
      const unknown = yield* declare(validBodies(3));

      const onUploading: readonly Effect.Effect<unknown, unknown>[] = [
        publishing.publish(uploading.photoId, details),
        publishing.unpublish(uploading.photoId),
      ];
      for (const call of onUploading) {
        expect(yield* Effect.flip(call)).toEqual(
          expect.objectContaining({
            _tag: "PhotoNotDraft",
            photoId: uploading.photoId,
          })
        );
      }
      const onUnknown: readonly Effect.Effect<unknown, unknown>[] = [
        publishing.publish(unknown.photoId, details),
        publishing.unpublish(unknown.photoId),
      ];
      for (const call of onUnknown) {
        expect(yield* Effect.flip(call)).toEqual(
          expect.objectContaining({
            _tag: "PhotoNotFound",
            photoId: unknown.photoId,
          })
        );
      }
      expect(publicBlobs.objects.size).toBe(0);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "a publish whose copy fails stays a draft that no visitor can see, and publishing again finishes it",
  () => {
    const { layer, publicBlobs } = harness();
    return Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const { photoId } = yield* draft(1);

      publicBlobs.faults.failNext("put");
      expect(
        yield* Effect.flip(publishing.publish(photoId, details))
      ).toMatchObject({ _tag: "StorageUnavailable" });
      expect(yield* tagOf(photoId)).toBe("Draft");
      expect(yield* listedIds).toEqual([]);

      expect((yield* publishing.publish(photoId, details))._tag).toBe(
        "Published"
      );
      expect(yield* listedIds).toEqual([photoId]);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "a publish whose mark fails leaves unlisted copies; unpublishing the draft removes them and publishing again finishes it",
  () => {
    const { layer, publicBlobs } = harness();
    return Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const { photoId } = yield* draft(1);

      yield* refuseUpdates("1");
      expect(
        yield* Effect.flip(publishing.publish(photoId, details))
      ).toMatchObject({
        _tag: "StorageUnavailable",
        operation: "ledger.markPublished",
      });
      yield* allowUpdates;
      expect(yield* tagOf(photoId)).toBe("Draft");
      expect(yield* listedIds).toEqual([]);
      expect(publicBlobs.objects.size).toBe(derivativeKinds.length);

      expect((yield* publishing.unpublish(photoId))._tag).toBe("Draft");
      expect(publicKeys(publicBlobs.objects)).toEqual(["v1.json"]);

      expect((yield* publishing.publish(photoId, details))._tag).toBe(
        "Published"
      );
      expect(yield* listedIds).toEqual([photoId]);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "a publish whose manifest write fails is published but unlisted, and publishing again lists it",
  () => {
    const { layer, publicBlobs } = harness();
    return Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const { photoId } = yield* draft(1);
      yield* refuseUpdates("1");
      yield* Effect.flip(publishing.publish(photoId, details));
      yield* allowUpdates;

      // The copies exist now, so the next put is the manifest write.
      publicBlobs.faults.failNext("put");
      expect(
        yield* Effect.flip(publishing.publish(photoId, details))
      ).toMatchObject({ _tag: "StorageUnavailable", operation: "put" });
      expect(yield* tagOf(photoId)).toBe("Published");
      expect(yield* listedIds).toEqual([]);

      yield* publishing.publish(photoId, details);
      expect(yield* listedIds).toEqual([photoId]);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "unpublish delists the photo, deletes its public copies, keeps its private objects, and returns the draft",
  () => {
    const { layer, privateBlobs, publicBlobs } = harness();
    return Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const kept = yield* draft(1);
      const removed = yield* draft(2);
      yield* publishing.publish(kept.photoId, details);
      yield* publishing.publish(removed.photoId, details);
      const privateKeys = publicKeys(privateBlobs.objects);

      const unpublished = yield* publishing.unpublish(removed.photoId);

      expect(unpublished).toMatchObject({
        _tag: "Draft",
        manifest: removed.manifest,
        photoId: removed.photoId,
      });
      expect(
        yield* Photos.use((photos) => photos.get(removed.photoId))
      ).toEqual(unpublished);
      expect(yield* listedIds).toEqual([kept.photoId]);
      expect(
        publicKeys(publicBlobs.objects).filter((key) =>
          key.includes(removed.photoId)
        )
      ).toEqual([]);
      expect(publicKeys(privateBlobs.objects)).toEqual(privateKeys);
      expect(yield* publishing.unpublish(removed.photoId)).toEqual(unpublished);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "an unpublish whose manifest write fails is Unpublishing with the old listing intact, and unpublishing again finishes it",
  () => {
    const { layer, publicBlobs } = harness();
    return Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const { photoId } = yield* draft(1);
      yield* publishing.publish(photoId, details);

      publicBlobs.faults.failNext("put");
      expect(yield* Effect.flip(publishing.unpublish(photoId))).toMatchObject({
        _tag: "StorageUnavailable",
      });
      expect(yield* Photos.use((photos) => photos.get(photoId))).toMatchObject({
        _tag: "Unpublishing",
        details,
        photoId,
      });
      expect(yield* listedIds).toEqual([photoId]);
      expect(publicBlobs.objects.size).toBe(derivativeKinds.length + 1);

      expect((yield* publishing.unpublish(photoId))._tag).toBe("Draft");
      expect(yield* listedIds).toEqual([]);
      expect(publicKeys(publicBlobs.objects)).toEqual(["v1.json"]);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "an unpublish whose delete fails is delisted but still Unpublishing, never Draft, until unpublishing again deletes the copies",
  () => {
    const { layer, publicBlobs } = harness();
    return Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const { photoId } = yield* draft(1);
      yield* publishing.publish(photoId, details);

      for (const operation of ["list", "remove"] as const) {
        publicBlobs.faults.failNext(operation);
        expect(yield* Effect.flip(publishing.unpublish(photoId))).toMatchObject(
          { _tag: "StorageUnavailable", operation }
        );
        expect(yield* tagOf(photoId)).toBe("Unpublishing");
        expect(yield* listedIds).toEqual([]);
        expect(publicBlobs.objects.size).toBe(derivativeKinds.length + 1);
      }

      expect((yield* publishing.unpublish(photoId))._tag).toBe("Draft");
      expect(publicKeys(publicBlobs.objects)).toEqual(["v1.json"]);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "an unpublish whose final mark fails stays Unpublishing with its copies gone, and unpublishing again marks it Draft",
  () => {
    const { layer, publicBlobs } = harness();
    return Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const { photoId } = yield* draft(1);
      yield* publishing.publish(photoId, details);

      yield* refuseUpdates("NEW.publication IS NULL");
      expect(yield* Effect.flip(publishing.unpublish(photoId))).toMatchObject({
        _tag: "StorageUnavailable",
        operation: "ledger.markPrivate",
      });
      yield* allowUpdates;
      expect(yield* tagOf(photoId)).toBe("Unpublishing");
      expect(publicKeys(publicBlobs.objects)).toEqual(["v1.json"]);

      expect((yield* publishing.unpublish(photoId))._tag).toBe("Draft");
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "publishing an Unpublishing photo copies what the unpublish deleted and lists it again",
  () => {
    const { layer, publicBlobs } = harness();
    return Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const { photoId } = yield* draft(1);
      yield* publishing.publish(photoId, details);
      yield* refuseUpdates("NEW.publication IS NULL");
      yield* Effect.flip(publishing.unpublish(photoId));
      yield* allowUpdates;

      expect((yield* publishing.publish(photoId, details))._tag).toBe(
        "Published"
      );
      expect(yield* listedIds).toEqual([photoId]);
      expect(publicBlobs.objects.size).toBe(derivativeKinds.length + 1);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "a manifest put that lands after a newer one is written again, so the manifest lists exactly the Published rows",
  () => {
    const held = Deferred.makeUnsafe<true>();
    const release = Deferred.makeUnsafe<true>();
    let holdNextManifest = false;
    const { layer } = harness({
      ...noHooks,
      before: (key) =>
        Effect.suspend(() => {
          if (key !== MANIFEST_KEY || !holdNextManifest) {
            return Effect.void;
          }
          holdNextManifest = false;
          return Deferred.succeed(held, true).pipe(
            Effect.andThen(Deferred.await(release))
          );
        }),
    });
    return Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const kept = (yield* draft(1)).photoId;
      const removed = (yield* draft(2)).photoId;
      yield* publishing.publish(kept, details);
      yield* publishing.publish(removed, details);

      // Writer A reads both photos as Published, then its put waits.
      holdNextManifest = true;
      const writerA = yield* Effect.forkChild(
        publishing.publish(kept, { alt: "Bay" })
      );
      yield* Deferred.await(held);
      // Writer B delists a photo and its put lands first.
      yield* publishing.unpublish(removed);
      yield* Deferred.succeed(release, true);
      yield* Fiber.join(writerA);

      const publishedRows = yield* SqlClient.SqlClient.use(
        (sql) =>
          sql<{
            readonly photoId: string;
          }>`SELECT photo_id AS photoId FROM photo WHERE publication = 'Published'`
      );
      expect(publishedRows.map((row) => row.photoId)).toEqual([kept]);
      const { manifest } = yield* gallery;
      expect(manifest.photos).toMatchObject([{ alt: "Bay", id: kept }]);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "an unpublish between publish's copies and its mark leaves the photo Published with all three public copies",
  () => {
    const copied = Deferred.makeUnsafe<true>();
    const resume = Deferred.makeUnsafe<true>();
    let derivativePuts = 0;
    const { layer, publicBlobs } = harness({
      ...noHooks,
      after: (key) =>
        Effect.suspend(() => {
          if (key === MANIFEST_KEY) {
            return Effect.void;
          }
          derivativePuts += 1;
          // The third copy has landed; hold the copy step open so the mark waits.
          return derivativePuts === derivativeKinds.length
            ? Deferred.succeed(copied, true).pipe(
                Effect.andThen(Deferred.await(resume))
              )
            : Effect.void;
        }),
    });
    return Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const { manifest, photoId } = yield* draft(1);

      const publisher = yield* Effect.forkChild(
        publishing.publish(photoId, details)
      );
      yield* Deferred.await(copied);
      // The row is still Draft, so this unpublish deletes the copies and returns.
      expect((yield* publishing.unpublish(photoId))._tag).toBe("Draft");
      yield* Deferred.succeed(resume, true);
      yield* Fiber.join(publisher);

      expect(yield* tagOf(photoId)).toBe("Published");
      expect(yield* listedIds).toEqual([photoId]);
      expect(publicKeys(publicBlobs.objects)).toEqual(
        [
          "v1.json",
          ...derivativeKinds.map(
            (kind) => `photos/${photoId}/${kind}-${manifest[kind].sha256}.webp`
          ),
        ].toSorted()
      );
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "an unpublish that finishes between publish's mark and its second copy pass leaves the photo Draft with no public copies",
  () => {
    const marked = Deferred.makeUnsafe<true>();
    const resume = Deferred.makeUnsafe<true>();
    let checks = 0;
    const { layer, publicBlobs } = harness({
      ...noHooks,
      holds: () =>
        Effect.suspend(() => {
          checks += 1;
          // The first pass checks each derivative once; every later check belongs to the pass after the mark.
          if (checks <= derivativeKinds.length) {
            return Effect.void;
          }
          return Deferred.succeed(marked, true).pipe(
            Effect.andThen(Deferred.await(resume))
          );
        }),
    });
    return Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const { photoId } = yield* draft(1);

      const publisher = yield* Effect.forkChild(
        publishing.publish(photoId, details)
      );
      yield* Deferred.await(marked);
      expect(yield* tagOf(photoId)).toBe("Published");
      expect((yield* publishing.unpublish(photoId))._tag).toBe("Draft");
      yield* Deferred.succeed(resume, true);
      yield* Fiber.join(publisher);

      expect(yield* tagOf(photoId)).toBe("Draft");
      expect(yield* listedIds).toEqual([]);
      expect(publicKeys(publicBlobs.objects)).toEqual(["v1.json"]);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "begin refuses a new manifest while Published or Unpublishing, and store and finalize leave a published photo as it is",
  () => {
    const { layer } = harness();
    return Effect.gen(function* run() {
      const photos = yield* Photos;
      const publishing = yield* Publishing;
      const { bodies, manifest, photoId } = yield* draft(1);
      const published = yield* publishing.publish(photoId, details);
      const rerendered = {
        ...manifest,
        w480: { ...manifest.w480, height: 319 },
      };

      expect(yield* Effect.flip(photos.begin(rerendered))).toMatchObject({
        _tag: "PhotoManifestConflict",
      });
      expect(yield* photos.begin(manifest)).toEqual(published);
      expect(yield* photos.finalize(photoId)).toEqual(published);
      expect(
        yield* Effect.flip(
          photos.store({
            body: Stream.make(bodies.w480),
            contentLength: bodies.w480.length,
            kind: "w480",
            photoId,
          })
        )
      ).toMatchObject({ _tag: "PhotoAlreadyDraft" });

      yield* refuseUpdates("NEW.publication IS NULL");
      yield* Effect.flip(publishing.unpublish(photoId));
      yield* allowUpdates;
      expect(yield* Effect.flip(photos.begin(rerendered))).toMatchObject({
        _tag: "PhotoManifestConflict",
      });
      expect(yield* tagOf(photoId)).toBe("Unpublishing");
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "the manifest lists captured photos newest first, then photos without a capture time, newest published first",
  () => {
    const { layer } = harness();
    return Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const older = (yield* draft(1)).photoId;
      const newer = (yield* draft(2)).photoId;
      const uncapturedFirst = (yield* draft(3)).photoId;
      const uncapturedSecond = (yield* draft(4)).photoId;
      yield* publishing.publish(uncapturedFirst, {});
      yield* TestClock.adjust("1 millis");
      yield* publishing.publish(older, { capturedAt: "2025-01-02T03:04:05" });
      yield* TestClock.adjust("1 millis");
      yield* publishing.publish(uncapturedSecond, { alt: "No date" });
      yield* TestClock.adjust("1 millis");
      yield* publishing.publish(newer, { capturedAt: "2026-01-02T03:04:05" });

      expect(yield* listedIds).toEqual([
        newer,
        older,
        uncapturedSecond,
        uncapturedFirst,
      ]);
    }).pipe(Effect.provide(layer));
  }
);

it.effect(
  "the public reader answers an empty gallery before the first publish, and never a draft's derivative",
  () => {
    const { layer } = harness();
    return Effect.gen(function* run() {
      const { manifest } = yield* gallery;
      expect(manifest).toMatchObject({ photos: [], version: 1 });

      const unpublished = yield* draft(1);
      const reader = yield* PublicGallery;
      expect(
        yield* reader.derivative(
          unpublished.photoId,
          "w480",
          unpublished.manifest.w480.sha256
        )
      ).toBe(null);

      yield* Publishing.use((publishing) =>
        publishing.publish(unpublished.photoId, details)
      );
      const found = yield* reader.derivative(
        unpublished.photoId,
        "w480",
        unpublished.manifest.w480.sha256
      );
      expect(found?.size).toBe(unpublished.manifest.w480.bytes);
    }).pipe(Effect.provide(layer));
  }
);
