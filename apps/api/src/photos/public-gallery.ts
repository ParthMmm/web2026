import { Context, DateTime, Effect, Layer, Stream } from "effect";

import { PublicBlobs } from "./blobs.ts";
import type { BlobBody } from "./blobs.ts";
import type {
  DerivativeKind,
  PhotoId,
  Sha256,
  StorageUnavailable,
} from "./domain.ts";
import { encodeGalleryManifest, galleryManifest } from "./gallery-manifest.ts";
import { GALLERY_MANIFEST_KEY, publicKey } from "./internal/keys.ts";
import { storage } from "./internal/storage.ts";

/** What anyone may read. Its layer needs only the public bucket, so it can never reach a draft or an original. */
export class PublicGallery extends Context.Service<
  PublicGallery,
  {
    /** The gallery manifest; an empty gallery before the first publish. */
    readonly manifest: Effect.Effect<BlobBody, StorageUnavailable>;
    readonly derivative: (
      photoId: PhotoId,
      kind: DerivativeKind,
      sha256: Sha256
    ) => Effect.Effect<BlobBody | null, StorageUnavailable>;
  }
>()("@web2026/api/PublicGallery") {
  static readonly layer: Layer.Layer<PublicGallery, never, PublicBlobs> =
    Layer.effect(
      PublicGallery,
      Effect.gen(function* buildPublicGallery() {
        const blobs = yield* PublicBlobs;

        const emptyGallery = DateTime.now.pipe(
          Effect.map((now) => {
            const bytes = encodeGalleryManifest(galleryManifest([], now));
            return { body: Stream.make(bytes), size: bytes.length };
          })
        );

        const manifest = blobs.read(GALLERY_MANIFEST_KEY).pipe(
          Effect.flatMap((found) =>
            found === null ? emptyGallery : Effect.succeed(found)
          ),
          storage
        );

        const derivative = (
          photoId: PhotoId,
          kind: DerivativeKind,
          sha256: Sha256
        ) => blobs.read(publicKey(photoId, kind, sha256)).pipe(storage);

        return PublicGallery.of({ derivative, manifest });
      })
    );
}
