import { DateTime, Order, Schema } from "effect";

import { PhotoDetails, PhotoId } from "./domain.ts";
import type { DerivativeKind, Manifest } from "./domain.ts";
import { publicKey } from "./internal/keys.ts";

const GallerySize = Schema.Struct({
  height: Schema.Int,
  /** Relative to the gallery base URL, such as `/gallery/`. */
  path: Schema.String,
  width: Schema.Int,
});

export const GalleryPhoto = Schema.Struct({
  ...PhotoDetails.fields,
  id: PhotoId,
  publishedAt: Schema.DateTimeUtcFromString,
  // oxlint-disable-next-line sort-keys -- smallest first, in the Manifest's order
  sizes: Schema.Struct({
    w480: GallerySize,
    w1200: GallerySize,
    w2400: GallerySize,
  }),
});
type GalleryPhotoValue = typeof GalleryPhoto.Type;

/**
 * The public gallery, version 1. It holds the details the owner published and
 * the three derivatives; never the original, a private key, or other EXIF.
 */
export const GalleryManifest = Schema.Struct({
  generatedAt: Schema.DateTimeUtcFromString,
  photos: Schema.Array(GalleryPhoto),
  version: Schema.Literal(1),
});
export type Gallery = typeof GalleryManifest.Type;

export interface GalleryEntry {
  readonly photoId: PhotoId;
  readonly manifest: Manifest;
  readonly publishedAt: DateTime.Utc;
  readonly details: PhotoDetails;
}

const byCapturedAt: Order.Order<GalleryPhotoValue> = (self, that) => {
  if (self.capturedAt === that.capturedAt) {
    return 0;
  }
  if (self.capturedAt === undefined) {
    return 1;
  }
  if (that.capturedAt === undefined) {
    return -1;
  }
  return Order.flip(Order.String)(self.capturedAt, that.capturedAt);
};

/** Captured time newest first, photos without one after those with one; then published time newest first; then ID. */
const galleryOrder = Order.combineAll<GalleryPhotoValue>([
  byCapturedAt,
  Order.mapInput(Order.flip(DateTime.Order), (photo) => photo.publishedAt),
  Order.mapInput(Order.String, (photo) => photo.id),
]);

const photoOf = ({
  details,
  manifest,
  photoId,
  publishedAt,
}: GalleryEntry): GalleryPhotoValue => {
  const size = (kind: DerivativeKind) => ({
    height: manifest[kind].height,
    path: publicKey(photoId, kind, manifest[kind].sha256),
    width: manifest[kind].width,
  });
  return {
    ...details,
    id: photoId,
    publishedAt,
    sizes: { w1200: size("w1200"), w2400: size("w2400"), w480: size("w480") },
  };
};

export const galleryManifest = (
  entries: readonly GalleryEntry[],
  generatedAt: DateTime.Utc
): Gallery => ({
  generatedAt,
  photos: entries.map(photoOf).toSorted(galleryOrder),
  version: 1,
});

const encodeJson = Schema.encodeSync(Schema.fromJsonString(GalleryManifest));

export const encodeGalleryManifest = (manifest: Gallery): Uint8Array =>
  new TextEncoder().encode(encodeJson(manifest));
