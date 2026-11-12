import { BlobKey } from "../blobs.ts";
import type { PhotoId, Sha256, VariantKind } from "../domain.ts";

export const STAGING_PREFIX = "staging/";
const FINAL_PREFIX = "photos/";

export const stagingKey = (
  photoId: PhotoId,
  kind: VariantKind,
  sha256: Sha256
): BlobKey => BlobKey.make(`${STAGING_PREFIX}${photoId}/${kind}-${sha256}`);

export const finalKey = (
  photoId: PhotoId,
  kind: VariantKind,
  sha256: Sha256
): BlobKey => BlobKey.make(`${FINAL_PREFIX}${photoId}/${kind}-${sha256}`);

/** Every final object of one photo, whichever manifest it was promoted for. */
export const finalPrefix = (photoId: PhotoId): string =>
  `${FINAL_PREFIX}${photoId}/`;
