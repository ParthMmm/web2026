import {
  Array as Arr,
  Clock,
  Context,
  DateTime,
  Effect,
  Layer,
  Option,
  Stream,
} from "effect";
import type { SqlClient } from "effect/sql";

import { Blobs } from "./blobs.ts";
import type { BlobKey, BlobUnavailable } from "./blobs.ts";
import {
  DEFAULT_PAGE,
  FinalizeRejected,
  HashMismatch,
  isDerivativeKind,
  LengthMismatch,
  PhotoAlreadyDraft,
  PhotoManifestConflict,
  PhotoNotFound,
  photoIdOf,
  variantKinds,
} from "./domain.ts";
import type {
  DerivativeKind,
  Manifest,
  OriginalUpload,
  PhotoId,
  PhotoPage,
  PhotoView,
  Problem,
  SettledView,
  StorageUnavailable,
  VariantKind,
} from "./domain.ts";
import { finalKey, finalPrefix, stagingKey } from "./internal/keys.ts";
import { makeLedger } from "./internal/ledger.ts";
import type { Stored, Uploading } from "./internal/ledger.ts";
import { storage, toUnavailable } from "./internal/storage.ts";
import { settledView } from "./internal/views.ts";
import {
  CHUNK_HEADER_BYTES,
  isExtended,
  MAX_BODY_CHUNKS,
  parseWebpHeader,
  readBodyChunk,
  VP8X_BODY_START,
  WEBP_PREFIX_BYTES,
} from "./internal/webp.ts";
import { UploadUrls } from "./upload-urls.ts";

export interface PageRequest {
  readonly after?: PhotoId | undefined;
  readonly limit?: number | undefined;
}

export interface StoreInput {
  readonly photoId: PhotoId;
  readonly kind: VariantKind;
  readonly contentLength: number;
  readonly body: Stream.Stream<Uint8Array, unknown>;
}

type Location =
  | {
      readonly _tag: "Staged";
      readonly kind: VariantKind;
      readonly key: BlobKey;
    }
  | {
      readonly _tag: "Final";
      readonly kind: VariantKind;
      readonly key: BlobKey;
    }
  | { readonly _tag: "Missing"; readonly kind: VariantKind };

const missing = (kind: VariantKind): readonly Problem[] => [
  { _tag: "Missing", kind },
];

const rejectIfAny = (
  photoId: PhotoId,
  problems: readonly (readonly Problem[])[]
) => {
  const flat = problems.flat();
  return Arr.isReadonlyArrayNonEmpty(flat)
    ? Effect.fail(new FinalizeRejected({ photoId, problems: flat }))
    : Effect.void;
};

/** What the chunks after the VP8X header hold. `wellFormed` means one image chunk, and chunks that fill the file exactly. */
interface BodyScan {
  readonly animated: boolean;
  readonly hasMetadata: boolean;
  readonly wellFormed: boolean;
}

const SIMPLE_BODY: BodyScan = {
  animated: false,
  hasMetadata: false,
  wellFormed: true,
};

const webpProblems = (
  kind: DerivativeKind,
  declared: { readonly width: number; readonly height: number },
  prefix: Uint8Array,
  totalBytes: number,
  body: BodyScan
): readonly Problem[] => {
  const header = parseWebpHeader(prefix, totalBytes);
  if (header._tag === "NotWebp" || !body.wellFormed) {
    return [{ _tag: "NotWebp", kind }];
  }
  const problems: Problem[] = [];
  if (header.animated || body.animated) {
    problems.push({ _tag: "Animated", kind });
  }
  if (header.hasMetadata || body.hasMetadata) {
    problems.push({ _tag: "HasMetadata", kind });
  }
  if (header.width !== declared.width || header.height !== declared.height) {
    problems.push({
      _tag: "DimensionsMismatch",
      expected: { height: declared.height, width: declared.width },
      found: { height: header.height, width: header.width },
      kind,
    });
  }
  return problems;
};

/** Every method is idempotent, so the desktop can repeat any call after a lost response. */
export class Photos extends Context.Service<
  Photos,
  {
    readonly begin: (
      manifest: Manifest
    ) => Effect.Effect<PhotoView, PhotoManifestConflict | StorageUnavailable>;
    readonly get: (
      photoId: PhotoId
    ) => Effect.Effect<PhotoView, PhotoNotFound | StorageUnavailable>;
    /** Every photo in ID order, one page at a time; `next` is the `after` of the next page. */
    readonly list: (
      request: PageRequest
    ) => Effect.Effect<PhotoPage, StorageUnavailable>;
    /**
     * A presigned PUT that sends the original straight to its final key in
     * R2, which checks the signed length and SHA-256 as it writes.
     */
    readonly originalUpload: (
      photoId: PhotoId
    ) => Effect.Effect<
      OriginalUpload,
      PhotoNotFound | PhotoAlreadyDraft | StorageUnavailable
    >;
    readonly store: (
      input: StoreInput
    ) => Effect.Effect<
      void,
      | PhotoNotFound
      | PhotoAlreadyDraft
      | LengthMismatch
      | HashMismatch
      | StorageUnavailable
    >;
    readonly finalize: (
      photoId: PhotoId
    ) => Effect.Effect<
      SettledView,
      | PhotoNotFound
      | PhotoManifestConflict
      | FinalizeRejected
      | StorageUnavailable
    >;
  }
>()("@web2026/api/Photos") {
  static readonly layer: Layer.Layer<
    Photos,
    never,
    Blobs | UploadUrls | SqlClient.SqlClient
  > = Layer.effect(
    Photos,
    Effect.gen(function* buildPhotos() {
      const blobs = yield* Blobs;
      const uploadUrls = yield* UploadUrls;
      const ledger = yield* makeLedger;

      const requirePhoto = (photoId: PhotoId) =>
        ledger.find(photoId).pipe(
          storage,
          Effect.flatMap(
            Option.match({
              onNone: () => Effect.fail(new PhotoNotFound({ photoId })),
              onSome: Effect.succeed,
            })
          )
        );

      const locate = (stored: Uploading, kind: VariantKind) =>
        Effect.gen(function* locateVariant() {
          const facts = stored.manifest[kind];
          const staged = stagingKey(stored.photoId, kind, facts.sha256);
          if (yield* blobs.holds(staged, facts)) {
            return { _tag: "Staged", key: staged, kind } as const;
          }
          const final = finalKey(stored.photoId, kind, facts.sha256);
          if (yield* blobs.holds(final, facts)) {
            return { _tag: "Final", key: final, kind } as const;
          }
          return { _tag: "Missing", kind } as const;
        });

      const locateAll = (stored: Uploading) =>
        Effect.forEach(
          variantKinds,
          (kind): Effect.Effect<Location, BlobUnavailable> =>
            locate(stored, kind),
          { concurrency: 4 }
        );

      const view = (
        stored: Stored
      ): Effect.Effect<PhotoView, StorageUnavailable> =>
        stored._tag === "Uploading"
          ? locateAll(stored).pipe(
              storage,
              Effect.map((locations) => ({
                _tag: "Uploading" as const,
                manifest: stored.manifest,
                missing: locations.flatMap((at) =>
                  at._tag === "Missing" ? [at.kind] : []
                ),
                photoId: stored.photoId,
              }))
            )
          : Effect.succeed(settledView(stored));

      const scanBody = (key: BlobKey, totalBytes: number) =>
        Effect.gen(function* scanBodyChunks() {
          let animated = false;
          let hasMetadata = false;
          let images = 0;
          let offset = VP8X_BODY_START;
          for (let read = 0; offset < totalBytes; read += 1) {
            const header =
              read < MAX_BODY_CHUNKS
                ? yield* blobs.range(key, offset, CHUNK_HEADER_BYTES)
                : null;
            if (header === null || header.length < CHUNK_HEADER_BYTES) {
              return { animated, hasMetadata, wellFormed: false };
            }
            const chunk = readBodyChunk(header, offset);
            if (chunk.kind === "Unknown") {
              return { animated, hasMetadata, wellFormed: false };
            }
            animated ||= chunk.kind === "Animation";
            hasMetadata ||= chunk.kind === "Metadata";
            images += chunk.kind === "Image" ? 1 : 0;
            offset = chunk.next;
          }
          // An animation keeps its frames inside ANMF chunks, so it has no top-level image chunk.
          const wellFormed =
            offset === totalBytes && (animated ? images === 0 : images === 1);
          return { animated, hasMetadata, wellFormed } satisfies BodyScan;
        });

      const inspect = (stored: Uploading, at: Location) =>
        Effect.gen(function* inspectVariant() {
          if (at._tag === "Missing") {
            return missing(at.kind);
          }
          if (!isDerivativeKind(at.kind)) {
            return [];
          }
          const facts = stored.manifest[at.kind];
          const prefix = yield* blobs.range(at.key, 0, WEBP_PREFIX_BYTES);
          if (prefix === null) {
            return missing(at.kind);
          }
          const body = isExtended(prefix)
            ? yield* scanBody(at.key, facts.bytes)
            : SIMPLE_BODY;
          return webpProblems(at.kind, facts, prefix, facts.bytes, body);
        });

      const promote = (stored: Uploading, at: Location) =>
        at._tag === "Staged"
          ? blobs
              .copy(
                at.key,
                finalKey(
                  stored.photoId,
                  at.kind,
                  stored.manifest[at.kind].sha256
                ),
                stored.manifest[at.kind]
              )
              .pipe(
                Effect.as<readonly Problem[]>([]),
                Effect.catchTags({
                  BlobMissing: () => Effect.succeed(missing(at.kind)),
                  BlobRejected: () =>
                    Effect.succeed<readonly Problem[]>([
                      { _tag: "Corrupt", kind: at.kind },
                    ]),
                })
              )
          : Effect.succeed<readonly Problem[]>([]);

      const begin = Effect.fn("Photos.begin")(function* beginPhoto(
        manifest: Manifest
      ) {
        const photoId = photoIdOf(manifest);
        const now = yield* Clock.currentTimeMillis;
        const stored = yield* ledger.declare(photoId, manifest, now).pipe(
          Effect.catchTag("LedgerManifestConflict", () =>
            Effect.fail(new PhotoManifestConflict({ photoId }))
          ),
          storage
        );
        return yield* view(stored);
      });

      const get = Effect.fn("Photos.get")(function* getPhoto(photoId: PhotoId) {
        return yield* view(yield* requirePhoto(photoId));
      });

      const store = ({ body, contentLength, kind, photoId }: StoreInput) =>
        Effect.gen(function* storeVariant() {
          const stored = yield* requirePhoto(photoId);
          if (stored._tag !== "Uploading") {
            return yield* new PhotoAlreadyDraft({ photoId });
          }
          const facts = stored.manifest[kind];
          if (contentLength !== facts.bytes) {
            return yield* new LengthMismatch({
              expected: facts.bytes,
              kind,
              received: contentLength,
            });
          }
          let received = 0;
          let ended = false;
          const metered = body.pipe(
            Stream.tap((chunk) =>
              Effect.sync(() => {
                received += chunk.length;
              })
            ),
            Stream.onEnd(
              Effect.sync(() => {
                ended = true;
              })
            )
          );
          // Only a body that overran, or ended short, is the client's fault; a store failure before the body ends is ours.
          const rejected = (
            otherwise: Effect.Effect<never, HashMismatch | StorageUnavailable>
          ) =>
            received > facts.bytes || (ended && received !== facts.bytes)
              ? Effect.fail(
                  new LengthMismatch({ expected: facts.bytes, kind, received })
                )
              : otherwise;
          yield* blobs
            .put(stagingKey(photoId, kind, facts.sha256), metered, facts)
            .pipe(
              Effect.catchTags({
                BlobRejected: () =>
                  rejected(Effect.fail(new HashMismatch({ kind }))),
                BlobUnavailable: (failure) => rejected(toUnavailable(failure)),
              })
            );
        }).pipe(
          Effect.withSpan("Photos.store"),
          Effect.annotateLogs({ kind, photoId })
        );

      // The final key, not staging: R2 has checked the bytes, and finalize only needs the original to exist.
      const originalUpload = Effect.fn("Photos.originalUpload")(
        function* signOriginalUpload(photoId: PhotoId) {
          const stored = yield* requirePhoto(photoId);
          if (stored._tag !== "Uploading") {
            return yield* new PhotoAlreadyDraft({ photoId });
          }
          const facts = stored.manifest.original;
          const signed = yield* uploadUrls
            .signPut(finalKey(photoId, "original", facts.sha256), facts)
            .pipe(storage);
          return {
            expiresAt: DateTime.makeUnsafe(signed.expiresAt),
            headers: signed.headers,
            method: "PUT" as const,
            url: signed.url,
          };
        }
      );

      // A reopened draft leaves the objects of its earlier manifest under its final prefix.
      const removeSuperseded = ({
        manifest,
        photoId,
      }: Exclude<Stored, Uploading>) =>
        blobs.list(finalPrefix(photoId)).pipe(
          Effect.flatMap((keys) => {
            const current = new Set(
              variantKinds.map((kind) =>
                finalKey(photoId, kind, manifest[kind].sha256)
              )
            );
            return blobs.remove(keys.filter((key) => !current.has(key)));
          }),
          Effect.catchTag("BlobUnavailable", ({ cause, operation }) =>
            Effect.logWarning(
              "superseded cleanup skipped; the next finalize retries it",
              { cause, operation }
            )
          )
        );

      const finalize = Effect.fn("Photos.finalize")(function* finalizePhoto(
        photoId: PhotoId
      ) {
        const stored = yield* requirePhoto(photoId);
        if (stored._tag !== "Uploading") {
          yield* removeSuperseded(stored);
          return settledView(stored);
        }
        const locations = yield* locateAll(stored).pipe(storage);
        yield* rejectIfAny(
          photoId,
          yield* Effect.forEach(locations, (at) => inspect(stored, at), {
            concurrency: 4,
          }).pipe(storage)
        );
        const promotionProblems = yield* Effect.forEach(
          locations,
          (at) => promote(stored, at),
          { concurrency: 2 }
        ).pipe(storage);
        if (promotionProblems.flat().length > 0) {
          const current = yield* requirePhoto(photoId);
          if (current._tag !== "Uploading") {
            return settledView(current);
          }
        }
        yield* rejectIfAny(photoId, promotionProblems);
        const now = yield* Clock.currentTimeMillis;
        const draft = yield* ledger
          .markFinal(photoId, stored.manifest, now)
          .pipe(
            Effect.catchTag("LedgerManifestConflict", () =>
              Effect.fail(new PhotoManifestConflict({ photoId }))
            ),
            storage
          );
        yield* blobs
          .remove(
            locations.flatMap((at) => (at._tag === "Staged" ? [at.key] : []))
          )
          .pipe(
            Effect.catchTag("BlobUnavailable", ({ cause, operation }) =>
              Effect.logWarning(
                "staging cleanup skipped; the lifecycle rule expires it",
                {
                  cause,
                  operation,
                }
              )
            )
          );
        yield* removeSuperseded(draft);
        return settledView(draft);
      });

      const list = Effect.fn("Photos.list")(function* listPhotos({
        after,
        limit = DEFAULT_PAGE,
      }: PageRequest) {
        const rows = yield* ledger.page(after ?? null, limit + 1).pipe(storage);
        const page = rows.slice(0, limit);
        const photos = yield* Effect.forEach(page, view, { concurrency: 2 });
        const last = page.at(-1);
        return {
          next: rows.length > limit && last !== undefined ? last.photoId : null,
          photos,
        };
      });

      return Photos.of({ begin, finalize, get, list, originalUpload, store });
    })
  );
}
