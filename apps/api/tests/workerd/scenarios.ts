import { deepStrictEqual, ok, strictEqual } from "node:assert";

import { Effect, Schema, Stream } from "effect";
import { Digest } from "effect/crypto";

import { Blobs, PublicBlobs } from "../../src/photos/blobs.ts";
import { variantKinds } from "../../src/photos/domain.ts";
import type {
  Manifest,
  PhotoId,
  VariantKind,
} from "../../src/photos/domain.ts";
import { GalleryManifest } from "../../src/photos/gallery-manifest.ts";
import { Photos } from "../../src/photos/photos.ts";
import { PublicGallery } from "../../src/photos/public-gallery.ts";
import { Publishing } from "../../src/photos/publishing.ts";
import type { Bodies } from "../support/fixture.ts";
import { declare, EDGES, validBodies, webp } from "../support/fixture.ts";

export interface PhotosScenario {
  readonly name: string;
  readonly run: Effect.Effect<
    void,
    unknown,
    Photos | Publishing | PublicGallery | Blobs | PublicBlobs
  >;
}

const declared = (seed: number) =>
  declare(validBodies(seed)).pipe(Effect.provide(Digest.layerWebCrypto));

const upload = (
  photoId: PhotoId,
  bodies: Bodies,
  kinds: readonly VariantKind[] = variantKinds
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
        })
      ),
    { discard: true }
  );

const asDraft = (seed: number) =>
  Effect.gen(function* makeDraft() {
    const declaredPhoto = yield* declared(seed);
    yield* Photos.use((photos) => photos.begin(declaredPhoto.manifest));
    yield* upload(declaredPhoto.photoId, declaredPhoto.bodies);
    yield* Photos.use((photos) => photos.finalize(declaredPhoto.photoId));
    return declaredPhoto;
  });

const listed = Effect.gen(function* readGallery() {
  const { body } = yield* PublicGallery.use((gallery) => gallery.manifest);
  const chunks = yield* Stream.runCollect(body);
  const manifest = Schema.decodeUnknownSync(
    Schema.fromJsonString(GalleryManifest)
  )(
    new TextDecoder().decode(
      Uint8Array.from(chunks.flatMap((chunk) => [...chunk]))
    )
  );
  return manifest.photos.map((photo) => photo.id);
});

export const photosScenarios: readonly PhotosScenario[] = [
  {
    name: "a fully uploaded photo becomes a draft once, and finalize again returns the same draft",
    run: Effect.gen(function* run() {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* declared(1);
      const begun = yield* photos.begin(manifest);
      deepStrictEqual(
        begun._tag === "Uploading" && begun.missing,
        variantKinds
      );
      yield* upload(photoId, bodies);
      const first = yield* photos.finalize(photoId);
      strictEqual(first._tag, "Draft");
      deepStrictEqual(yield* photos.finalize(photoId), first);
      deepStrictEqual(yield* photos.begin(manifest), first);
    }),
  },
  {
    name: "begin replaces the manifest while uploading, and reopens a draft whose finalize then lists and removes the superseded objects",
    run: Effect.gen(function* run() {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* declared(2);
      yield* photos.begin(manifest);
      const rerendered = yield* declare({
        ...bodies,
        w480: webp("VP8L", ...EDGES.w480),
      }).pipe(Effect.provide(Digest.layerWebCrypto));
      const replaced = yield* photos.begin(rerendered.manifest);
      deepStrictEqual(
        replaced._tag === "Uploading" && replaced.missing,
        variantKinds
      );
      yield* upload(photoId, rerendered.bodies);
      yield* photos.finalize(photoId);

      const reopened = yield* photos.begin(manifest);
      deepStrictEqual(reopened._tag === "Uploading" && reopened.missing, [
        "w480",
      ]);
      yield* upload(photoId, bodies, ["w480"]);
      const draft = yield* photos.finalize(photoId);
      deepStrictEqual(draft.manifest, manifest);
      const blobs = yield* Blobs;
      deepStrictEqual(
        yield* blobs.list(`photos/${photoId}/`),
        variantKinds
          .map((kind) => `photos/${photoId}/${kind}-${manifest[kind].sha256}`)
          .toSorted()
      );
    }),
  },
  {
    name: "a wrong-hash upload is HashMismatch and the variant stays missing",
    run: Effect.gen(function* run() {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* declared(3);
      yield* photos.begin(manifest);
      const tampered = Uint8Array.from(bodies.w480);
      tampered[40] = 0xff - (tampered[40] ?? 0);
      const failure = yield* Effect.flip(
        photos.store({
          body: Stream.make(tampered),
          contentLength: tampered.length,
          kind: "w480",
          photoId,
        })
      );
      strictEqual(failure._tag, "HashMismatch");
      const view = yield* photos.get(photoId);
      ok(view._tag === "Uploading" && view.missing.includes("w480"));
    }),
  },
  {
    name: "a body that ends early is LengthMismatch with the bytes R2 was given",
    run: Effect.gen(function* run() {
      const photos = yield* Photos;
      const { bodies, manifest, photoId } = yield* declared(4);
      yield* photos.begin(manifest);
      const failure = yield* Effect.flip(
        photos.store({
          body: Stream.make(bodies.original.subarray(0, 100)),
          contentLength: bodies.original.length,
          kind: "original",
          photoId,
        })
      );
      deepStrictEqual(
        failure._tag === "LengthMismatch" && {
          kind: failure.kind,
          received: failure.received,
        },
        { kind: "original", received: 100 }
      );
    }),
  },
  {
    name: "a manifest the ledger round-trips through D1 JSON compares equal",
    run: Effect.gen(function* run() {
      const photos = yield* Photos;
      const { manifest } = yield* declared(5);
      const view = yield* photos.begin(manifest);
      const stored: Manifest = view.manifest;
      deepStrictEqual(stored, manifest);
      deepStrictEqual(yield* photos.begin(manifest), view);
    }),
  },
  {
    name: "publish copies the derivatives to the public bucket and lists the photo, and a repeat changes only the details",
    run: Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const publicBlobs = yield* PublicBlobs;
      const { manifest, photoId } = yield* asDraft(6);
      const first = yield* publishing.publish(photoId, { alt: "One" });
      const again = yield* publishing.publish(photoId, { film: "HP5" });
      deepStrictEqual(again, { ...first, details: { film: "HP5" } });
      ok((yield* listed).includes(photoId));
      deepStrictEqual(yield* publicBlobs.list(`photos/${photoId}/`), [
        `photos/${photoId}/w1200-${manifest.w1200.sha256}.webp`,
        `photos/${photoId}/w2400-${manifest.w2400.sha256}.webp`,
        `photos/${photoId}/w480-${manifest.w480.sha256}.webp`,
      ]);
      deepStrictEqual(
        yield* Photos.use((photos) => photos.get(photoId)),
        again
      );
    }),
  },
  {
    name: "unpublish delists the photo, deletes its public copies, keeps its private objects, and repeats as the same draft",
    run: Effect.gen(function* run() {
      const publishing = yield* Publishing;
      const publicBlobs = yield* PublicBlobs;
      const privateBlobs = yield* Blobs;
      const { photoId } = yield* asDraft(7);
      yield* publishing.publish(photoId, {});
      const draft = yield* publishing.unpublish(photoId);
      strictEqual(draft._tag, "Draft");
      deepStrictEqual(yield* publishing.unpublish(photoId), draft);
      ok(!(yield* listed).includes(photoId));
      deepStrictEqual(yield* publicBlobs.list(`photos/${photoId}/`), []);
      strictEqual(
        (yield* privateBlobs.list(`photos/${photoId}/`)).length,
        variantKinds.length
      );
    }),
  },
  {
    name: "publish of an uploading photo is PhotoNotDraft and list pages through D1 in photo ID order",
    run: Effect.gen(function* run() {
      const photos = yield* Photos;
      const uploading = yield* declared(8);
      yield* photos.begin(uploading.manifest);
      const refused = yield* Effect.flip(
        Publishing.use((publishing) =>
          publishing.publish(uploading.photoId, {})
        )
      );
      strictEqual(refused._tag, "PhotoNotDraft");

      const seen: PhotoId[] = [];
      let after: PhotoId | undefined;
      do {
        const page = yield* photos.list({ after, limit: 2 });
        ok(page.photos.length <= 2);
        seen.push(...page.photos.map((view) => view.photoId));
        after = page.next ?? undefined;
      } while (after !== undefined);
      deepStrictEqual(seen, seen.toSorted());
      ok(seen.includes(uploading.photoId));
    }),
  },
];
