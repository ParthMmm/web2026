/* oxlint-disable max-classes-per-file -- one cohesive error vocabulary; each class is one wire contract. */
/* oxlint-disable unicorn/throw-new-error -- Schema.TaggedError is a curried class factory, not an Error constructor. */
import { Schema } from "effect";

const Hex64 = Schema.String.check(Schema.isPattern(/^[0-9a-f]{64}$/u));

export const Sha256 = Hex64.pipe(Schema.brand("Sha256"));
export type Sha256 = typeof Sha256.Type;

export const PhotoId = Hex64.pipe(Schema.brand("PhotoId"));
export type PhotoId = typeof PhotoId.Type;

export const MAX_OBJECT_BYTES = 100 * 1024 * 1024;

export const ByteCount = Schema.Int.check(
  Schema.isBetween({ maximum: MAX_OBJECT_BYTES, minimum: 1 })
).pipe(Schema.brand("ByteCount"));
export type ByteCount = typeof ByteCount.Type;

const Original = Schema.Struct({ bytes: ByteCount, sha256: Sha256 });

const derivative = (longestEdge: number) => {
  const edge = Schema.Int.check(
    Schema.isBetween({ maximum: longestEdge, minimum: 1 })
  );
  return Schema.Struct({
    bytes: ByteCount,
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

export const PhotoView = Schema.Union([Uploading, Draft]);
export type PhotoView = typeof PhotoView.Type;

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
