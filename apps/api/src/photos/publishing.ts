import {
  Clock,
  Context,
  DateTime,
  Effect,
  Layer,
  Option,
  Stream,
} from "effect";
import { Digest } from "effect/crypto";
import type { SqlClient } from "effect/sql";

import { Blobs, BlobUnavailable, PublicBlobs } from "./blobs.ts";
import {
  ByteCount,
  derivativeKinds,
  PhotoNotDraft,
  PhotoNotFound,
  Sha256,
} from "./domain.ts";
import type {
  DraftView,
  Manifest,
  PhotoDetails,
  PhotoId,
  PublishedView,
  StorageUnavailable,
} from "./domain.ts";
import { encodeGalleryManifest, galleryManifest } from "./gallery-manifest.ts";
import {
  finalKey,
  GALLERY_MANIFEST_KEY,
  publicKey,
  publicPrefix,
} from "./internal/keys.ts";
import { makeLedger } from "./internal/ledger.ts";
import type { Published } from "./internal/ledger.ts";
import { storage } from "./internal/storage.ts";
import { draftView, publishedView } from "./internal/views.ts";

/**
 * Publishing moves a draft's three verified derivatives into the public
 * bucket and lists them in the gallery manifest. D1 has no transactions and
 * neither bucket shares one with it, so each call orders its steps so that a
 * visitor never sees a photo whose copies are missing, and so that running
 * the same call again finishes whatever an earlier call left undone. The
 * manifest is derived from D1 and rewritten by every call; a writer repeats
 * its write until D1's gallery version holds still across it, so the last
 * manifest put always lists the current rows.
 *
 * Overlapping publish and unpublish calls on one photo end in the state of
 * whichever call marks the row last. The other caller still gets its own
 * success view, which may already be out of date.
 */
export class Publishing extends Context.Service<
  Publishing,
  {
    /** Copies, marks Published, keeps the copies only while the row stays Published, then rewrites the manifest. Publishing again replaces the details. */
    readonly publish: (
      photoId: PhotoId,
      details: PhotoDetails
    ) => Effect.Effect<
      PublishedView,
      PhotoNotFound | PhotoNotDraft | StorageUnavailable
    >;
    /** Marks Unpublishing, rewrites the manifest, deletes the public copies, then marks Draft. */
    readonly unpublish: (
      photoId: PhotoId
    ) => Effect.Effect<
      DraftView,
      PhotoNotFound | PhotoNotDraft | StorageUnavailable
    >;
  }
>()("@web2026/api/Publishing") {
  static readonly layer: Layer.Layer<
    Publishing,
    never,
    Blobs | PublicBlobs | SqlClient.SqlClient | Digest.Digest
  > = Layer.effect(
    Publishing,
    Effect.gen(function* buildPublishing() {
      const privateBlobs = yield* Blobs;
      const publicBlobs = yield* PublicBlobs;
      const digest = yield* Digest.Digest;
      const ledger = yield* makeLedger;

      const requireFinalized = Effect.fnUntraced(function* requireFinalized(
        photoId: PhotoId
      ) {
        const found = yield* ledger.find(photoId).pipe(storage);
        if (Option.isNone(found)) {
          return yield* new PhotoNotFound({ photoId });
        }
        if (found.value._tag === "Uploading") {
          return yield* new PhotoNotDraft({ photoId });
        }
        return found.value;
      });

      // Each copy is verified against the manifest's hash again on the way in; the original is never copied.
      const copyDerivatives = (photoId: PhotoId, manifest: Manifest) =>
        Effect.forEach(
          derivativeKinds,
          Effect.fnUntraced(function* copyDerivative(kind) {
            const facts = manifest[kind];
            const target = publicKey(photoId, kind, facts.sha256);
            if (yield* publicBlobs.holds(target, facts)) {
              return;
            }
            const source = yield* privateBlobs.read(
              finalKey(photoId, kind, facts.sha256)
            );
            if (source === null) {
              return yield* new BlobUnavailable({
                cause: `the draft's ${kind} is missing from the private bucket`,
                operation: "publish.read",
              });
            }
            yield* publicBlobs
              .put(target, source.body, facts)
              .pipe(
                Effect.catchTag("BlobRejected", (cause) =>
                  Effect.fail(
                    new BlobUnavailable({ cause, operation: "publish.copy" })
                  )
                )
              );
          }),
          { concurrency: 3, discard: true }
        );

      // Another call can change D1 between this read and this put, and its own put can land first; write again until no change slipped in.
      const writeManifest = Effect.gen(function* writeGalleryManifest() {
        let written: number;
        let current: number;
        do {
          written = yield* ledger.galleryVersion;
          const entries = yield* ledger.published;
          const bytes = encodeGalleryManifest(
            galleryManifest(entries, yield* DateTime.now)
          );
          const sha256 = Sha256.make(
            yield* digest.digestHex(Digest.sha256, bytes)
          );
          yield* publicBlobs.put(GALLERY_MANIFEST_KEY, Stream.make(bytes), {
            bytes: ByteCount.make(bytes.length),
            sha256,
          });
          current = yield* ledger.galleryVersion;
        } while (current !== written);
      }).pipe(
        Effect.catchTag("BlobRejected", (cause) =>
          Effect.fail(new BlobUnavailable({ cause, operation: "manifest.put" }))
        )
      );

      const removePublicCopies = (photoId: PhotoId) =>
        publicBlobs
          .list(publicPrefix(photoId))
          .pipe(Effect.flatMap((keys) => publicBlobs.remove(keys)));

      const publishedRow = (photoId: PhotoId) =>
        ledger
          .find(photoId)
          .pipe(
            storage,
            Effect.map(
              Option.filter((row): row is Published => row._tag === "Published")
            )
          );

      // An unpublish can delete the copies before publish marks the row, or finish after the mark and before these copies land; leave the bucket as the row is now.
      const keepCopiesWhilePublished = (
        photoId: PhotoId,
        manifest: Manifest
      ): Effect.Effect<void, StorageUnavailable> =>
        Effect.gen(function* keepPublicCopies() {
          yield* copyDerivatives(photoId, manifest).pipe(storage);
          if (Option.isSome(yield* publishedRow(photoId))) {
            return;
          }
          yield* removePublicCopies(photoId).pipe(storage);
          const current = yield* publishedRow(photoId);
          if (Option.isSome(current)) {
            // A later publish marked the photo again while the copies were being deleted.
            return yield* keepCopiesWhilePublished(
              photoId,
              current.value.manifest
            );
          }
        });

      const publish = (
        photoId: PhotoId,
        details: PhotoDetails
      ): Effect.Effect<
        PublishedView,
        PhotoNotFound | PhotoNotDraft | StorageUnavailable
      > =>
        Effect.gen(function* publishPhoto() {
          const stored = yield* requireFinalized(photoId);
          yield* copyDerivatives(photoId, stored.manifest).pipe(storage);
          const marked = yield* ledger
            .markPublished(
              photoId,
              stored.manifest,
              details,
              yield* Clock.currentTimeMillis
            )
            .pipe(storage);
          if (Option.isNone(marked)) {
            // The photo was reopened with another manifest since it was read; start from the row as it is now.
            return yield* publish(photoId, details);
          }
          yield* keepCopiesWhilePublished(photoId, marked.value.manifest);
          yield* writeManifest.pipe(storage);
          return publishedView(marked.value);
        }).pipe(
          Effect.withSpan("Publishing.publish"),
          Effect.annotateLogs({ photoId })
        );

      const unpublish = (
        photoId: PhotoId
      ): Effect.Effect<
        DraftView,
        PhotoNotFound | PhotoNotDraft | StorageUnavailable
      > =>
        Effect.gen(function* unpublishPhoto() {
          yield* requireFinalized(photoId);
          yield* ledger.markUnpublishing(photoId).pipe(storage);
          yield* writeManifest.pipe(storage);
          // A draft runs this too: a publish that copied but failed to mark can leave unlisted copies.
          yield* removePublicCopies(photoId).pipe(storage);
          const marked = yield* ledger.markPrivate(photoId).pipe(storage);
          if (Option.isSome(marked)) {
            return draftView(marked.value);
          }
          const current = yield* requireFinalized(photoId);
          if (current._tag === "Draft") {
            return draftView(current);
          }
          // A publish marked the photo again while its copies were being deleted.
          return yield* unpublish(photoId);
        }).pipe(
          Effect.withSpan("Publishing.unpublish"),
          Effect.annotateLogs({ photoId })
        );

      return Publishing.of({ publish, unpublish });
    })
  );
}
