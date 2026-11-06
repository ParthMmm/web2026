import {
  Array as Arr,
  Clock,
  Context,
  Effect,
  Layer,
  Option,
  Predicate,
  Stream,
} from "effect";
import type { SqlClient } from "effect/sql";

import { Blobs } from "./blobs.ts";
import { BlobUnavailable } from "./blobs.ts";
import type { BlobKey } from "./blobs.ts";
import {
  FinalizeRejected,
  HashMismatch,
  isDerivativeKind,
  LengthMismatch,
  PhotoAlreadyDraft,
  PhotoManifestConflict,
  PhotoNotFound,
  photoIdOf,
  StorageUnavailable,
  variantKinds,
} from "./domain.ts";
import type {
  DerivativeKind,
  DraftView,
  Manifest,
  PhotoId,
  PhotoView,
  Problem,
  VariantKind,
} from "./domain.ts";
import { finalKey, stagingKey } from "./internal/keys.ts";
import { makeLedger } from "./internal/ledger.ts";
import type {
  Draft,
  LedgerUnavailable,
  Stored,
  Uploading,
} from "./internal/ledger.ts";
import { parseWebpHeader, WEBP_PREFIX_BYTES } from "./internal/webp.ts";

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

const isStorageFailure = <E>(
  error: E
): error is Extract<E, BlobUnavailable | LedgerUnavailable> =>
  Predicate.isTagged(error, "BlobUnavailable") ||
  Predicate.isTagged(error, "LedgerUnavailable");

const missing = (kind: VariantKind): ReadonlyArray<Problem> => [
  { _tag: "Missing", kind },
];

const toUnavailable = ({
  cause,
  operation,
}: BlobUnavailable | LedgerUnavailable) =>
  Effect.logWarning("storage failure", { cause, operation }).pipe(
    Effect.andThen(Effect.fail(new StorageUnavailable({ operation })))
  );

const draftView = ({ finalizedAt, manifest, photoId }: Draft): DraftView => ({
  _tag: "Draft",
  finalizedAt,
  manifest,
  photoId,
});

const webpProblems = (
  kind: DerivativeKind,
  declared: { readonly width: number; readonly height: number },
  prefix: Uint8Array,
  totalBytes: number
): ReadonlyArray<Problem> => {
  const header = parseWebpHeader(prefix, totalBytes);
  if (header._tag === "NotWebp") {
    return [{ _tag: "NotWebp", kind }];
  }
  const problems: Array<Problem> = [];
  if (header.animated) {
    problems.push({ _tag: "Animated", kind });
  }
  if (header.hasMetadata) {
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
      DraftView,
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
    Blobs | SqlClient.SqlClient
  > = Layer.effect(
    Photos,
    Effect.gen(function* () {
      const blobs = yield* Blobs;
      const ledger = yield* makeLedger;

      const storage = <A, E, R>(effect: Effect.Effect<A, E, R>) =>
        Effect.catchIf(effect, isStorageFailure<E>, (failure) =>
          toUnavailable(failure)
        );

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
        Effect.gen(function* () {
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
        stored._tag === "Draft"
          ? Effect.succeed(draftView(stored))
          : locateAll(stored).pipe(
              storage,
              Effect.map((locations) => ({
                _tag: "Uploading" as const,
                manifest: stored.manifest,
                missing: locations.flatMap((at) =>
                  at._tag === "Missing" ? [at.kind] : []
                ),
                photoId: stored.photoId,
              }))
            );

      const inspect = (stored: Uploading, at: Location) =>
        Effect.gen(function* () {
          if (at._tag === "Missing") {
            return missing(at.kind);
          }
          if (!isDerivativeKind(at.kind)) {
            return [];
          }
          const facts = stored.manifest[at.kind];
          const prefix = yield* blobs.prefix(at.key, WEBP_PREFIX_BYTES);
          return prefix === null
            ? missing(at.kind)
            : webpProblems(at.kind, facts, prefix, facts.bytes);
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
                Effect.as<ReadonlyArray<Problem>>([]),
                Effect.catchTags({
                  BlobMissing: () => Effect.succeed(missing(at.kind)),
                  BlobRejected: () =>
                    Effect.succeed<ReadonlyArray<Problem>>([
                      { _tag: "Corrupt", kind: at.kind },
                    ]),
                })
              )
          : Effect.succeed<ReadonlyArray<Problem>>([]);

      const rejectIfAny = (
        photoId: PhotoId,
        problems: ReadonlyArray<ReadonlyArray<Problem>>
      ) => {
        const flat = problems.flat();
        return Arr.isReadonlyArrayNonEmpty(flat)
          ? Effect.fail(new FinalizeRejected({ photoId, problems: flat }))
          : Effect.void;
      };

      const begin = Effect.fn("Photos.begin")(function* (manifest: Manifest) {
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
          if (stored._tag === "Draft") {
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

      const finalize = Effect.fn("Photos.finalize")(function* finalizePhoto(
        photoId: PhotoId
      ) {
        const stored = yield* requirePhoto(photoId);
        if (stored._tag === "Draft") {
          return draftView(stored);
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
          if (current._tag === "Draft") {
            return draftView(current);
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
        return draftView(draft);
      });

      return Photos.of({ begin, finalize, get, store });
    })
  );
}
