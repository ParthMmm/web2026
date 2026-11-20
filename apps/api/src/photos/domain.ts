/* oxlint-disable max-classes-per-file -- one cohesive error vocabulary; each class is one wire contract. */
/* oxlint-disable unicorn/throw-new-error -- Schema.TaggedError is a curried class factory, not an Error constructor. */
import { Schema } from "effect";

const Hex64 = Schema.String.check(Schema.isPattern(/^[0-9a-f]{64}$/u));

export const Sha256 = Hex64.pipe(Schema.brand("Sha256"));
export type Sha256 = typeof Sha256.Type;

export const PhotoId = Hex64.pipe(Schema.brand("PhotoId"));
export type PhotoId = typeof PhotoId.Type;

/** The largest body the Worker takes. Derivatives go through the Worker, so none is larger. */
export const MAX_OBJECT_BYTES = 100 * 1024 * 1024;

/** The largest original. It goes straight to R2, whose limit for one PUT is 5 GiB. */
export const MAX_ORIGINAL_BYTES = 5_000_000_000;

export const ByteCount = Schema.Int.check(
  Schema.isBetween({ maximum: MAX_ORIGINAL_BYTES, minimum: 1 })
).pipe(Schema.brand("ByteCount"));
export type ByteCount = typeof ByteCount.Type;

const Original = Schema.Struct({ bytes: ByteCount, sha256: Sha256 });

const DerivativeByteCount = ByteCount.check(
  Schema.isLessThanOrEqualTo(MAX_OBJECT_BYTES)
);

const derivative = (longestEdge: number) => {
  const edge = Schema.Int.check(
    Schema.isBetween({ maximum: longestEdge, minimum: 1 })
  );
  return Schema.Struct({
    bytes: DerivativeByteCount,
    height: edge,
    sha256: Sha256,
    width: edge,
  });
};

// oxlint-disable-next-line sort-keys -- field order sets variantKinds and the stored JSON that ledger.markFinal compares as text
export const Manifest = Schema.Struct({
  original: Original,
  w480: derivative(480),
  w1200: derivative(1200),
  w2400: derivative(2400),
});
export type Manifest = typeof Manifest.Type;

export type VariantKind = keyof Manifest;
export type DerivativeKind = Exclude<VariantKind, "original">;

// SAFETY: a Struct's field names are exactly the keys of its Type, which VariantKind names.
export const variantKinds = Object.keys(
  Manifest.fields
) as readonly VariantKind[];
export const VariantKindSchema = Schema.Literals(variantKinds);

export const isDerivativeKind = (kind: VariantKind): kind is DerivativeKind =>
  kind !== "original";

export const derivativeKinds = variantKinds.filter(isDerivativeKind);

export const photoIdOf = (manifest: Manifest): PhotoId =>
  PhotoId.make(manifest.original.sha256);

const Uploading = Schema.TaggedStruct("Uploading", {
  manifest: Manifest,
  missing: Schema.Array(VariantKindSchema),
  photoId: PhotoId,
});

export const Draft = Schema.TaggedStruct("Draft", {
  finalizedAt: Schema.DateTimeUtcFromString,
  manifest: Manifest,
  photoId: PhotoId,
});
export type DraftView = typeof Draft.Type;

const boundedText = (maxLength: number) =>
  Schema.optionalKey(Schema.String.check(Schema.isMaxLength(maxLength)));

/** What a visitor sees beside a published photo. `capturedAt` is the camera's local date-time as text, with no zone. */
export const PhotoDetails = Schema.Struct({
  alt: boundedText(300),
  capturedAt: boundedText(40),
  film: boundedText(64),
});
export type PhotoDetails = typeof PhotoDetails.Type;

export const Published = Schema.TaggedStruct("Published", {
  details: PhotoDetails,
  finalizedAt: Schema.DateTimeUtcFromString,
  manifest: Manifest,
  photoId: PhotoId,
  publishedAt: Schema.DateTimeUtcFromString,
});
export type PublishedView = typeof Published.Type;

/** Public removal has started and has not finished. Another unpublish finishes it. */
const Unpublishing = Schema.TaggedStruct("Unpublishing", {
  details: PhotoDetails,
  finalizedAt: Schema.DateTimeUtcFromString,
  manifest: Manifest,
  photoId: PhotoId,
});
type UnpublishingView = typeof Unpublishing.Type;

export type SettledView = DraftView | PublishedView | UnpublishingView;

export const PhotoView = Schema.Union([
  Uploading,
  Draft,
  Published,
  Unpublishing,
]);
export type PhotoView = typeof PhotoView.Type;

/**
 * A presigned R2 PUT for an original. Send the bytes to `url` with exactly
 * these `headers`, before `expiresAt`. R2 refuses bytes whose SHA-256 differs
 * from the signed checksum, and a request whose signed headers differ.
 */
export const OriginalUpload = Schema.Struct({
  expiresAt: Schema.DateTimeUtcFromString,
  headers: Schema.Record(Schema.String, Schema.String),
  method: Schema.Literal("PUT"),
  url: Schema.String,
});
export type OriginalUpload = typeof OriginalUpload.Type;

export const MAX_PAGE = 200;
export const DEFAULT_PAGE = 100;

export const PageLimit = Schema.Int.check(
  Schema.isBetween({ maximum: MAX_PAGE, minimum: 1 })
);

export const PhotoPage = Schema.Struct({
  next: Schema.NullOr(PhotoId),
  photos: Schema.Array(PhotoView),
});
export type PhotoPage = typeof PhotoPage.Type;

const Dimensions = Schema.Struct({ height: Schema.Int, width: Schema.Int });

export const Problem = Schema.Union([
  Schema.TaggedStruct("Missing", { kind: VariantKindSchema }),
  Schema.TaggedStruct("Corrupt", { kind: VariantKindSchema }),
  Schema.TaggedStruct("NotWebp", { kind: VariantKindSchema }),
  Schema.TaggedStruct("HasMetadata", { kind: VariantKindSchema }),
  Schema.TaggedStruct("Animated", { kind: VariantKindSchema }),
  Schema.TaggedStruct("DimensionsMismatch", {
    expected: Dimensions,
    found: Dimensions,
    kind: VariantKindSchema,
  }),
]);
export type Problem = typeof Problem.Type;

export class PhotoNotFound extends Schema.TaggedError<PhotoNotFound>()(
  "PhotoNotFound",
  { photoId: PhotoId },
  { httpApiStatus: 404 }
) {}

export class PhotoManifestConflict extends Schema.TaggedError<PhotoManifestConflict>()(
  "PhotoManifestConflict",
  { photoId: PhotoId },
  { httpApiStatus: 409 }
) {}

export class PhotoNotDraft extends Schema.TaggedError<PhotoNotDraft>()(
  "PhotoNotDraft",
  { photoId: PhotoId },
  { httpApiStatus: 409 }
) {}

export class PhotoAlreadyDraft extends Schema.TaggedError<PhotoAlreadyDraft>()(
  "PhotoAlreadyDraft",
  { photoId: PhotoId },
  { httpApiStatus: 409 }
) {}

export class LengthMismatch extends Schema.TaggedError<LengthMismatch>()(
  "LengthMismatch",
  { expected: ByteCount, kind: VariantKindSchema, received: Schema.Int },
  { httpApiStatus: 422 }
) {}

export class HashMismatch extends Schema.TaggedError<HashMismatch>()(
  "HashMismatch",
  { kind: VariantKindSchema },
  { httpApiStatus: 422 }
) {}

export class FinalizeRejected extends Schema.TaggedError<FinalizeRejected>()(
  "FinalizeRejected",
  { photoId: PhotoId, problems: Schema.NonEmptyArray(Problem) },
  { httpApiStatus: 422 }
) {}

export class StorageUnavailable extends Schema.TaggedError<StorageUnavailable>()(
  "StorageUnavailable",
  { operation: Schema.String },
  { httpApiStatus: 503 }
) {}
