/* oxlint-disable max-classes-per-file -- the ledger's two failure modes belong with it. */
/* oxlint-disable unicorn/throw-new-error -- Schema.TaggedError is a curried class factory, not an Error constructor. */
import { Effect, Option, Schema } from "effect";
import type { DateTime } from "effect";
import { SqlClient, SqlSchema } from "effect/sql";

import { Manifest, PhotoDetails, PhotoId } from "../domain.ts";

export class LedgerUnavailable extends Schema.TaggedError<LedgerUnavailable>()(
  "LedgerUnavailable",
  { cause: Schema.Defect(), operation: Schema.String }
) {}

export class LedgerManifestConflict extends Schema.TaggedError<LedgerManifestConflict>()(
  "LedgerManifestConflict",
  { photoId: PhotoId }
) {}

export interface Uploading {
  readonly _tag: "Uploading";
  readonly photoId: PhotoId;
  readonly manifest: Manifest;
}

export interface Draft {
  readonly _tag: "Draft";
  readonly photoId: PhotoId;
  readonly manifest: Manifest;
  readonly finalizedAt: DateTime.Utc;
}

/** Published and Unpublishing rows carry the same facts; only the tag differs. */
export interface Public {
  readonly _tag: "Published" | "Unpublishing";
  readonly photoId: PhotoId;
  readonly manifest: Manifest;
  readonly finalizedAt: DateTime.Utc;
  readonly publishedAt: DateTime.Utc;
  readonly details: PhotoDetails;
}

export interface Published extends Public {
  readonly _tag: "Published";
}

export interface Unpublishing extends Public {
  readonly _tag: "Unpublishing";
}

export type Stored = Uploading | Draft | Published | Unpublishing;

const PhotoRow = Schema.Struct({
  details: Schema.NullOr(Schema.fromJsonString(PhotoDetails)),
  finalizedAt: Schema.NullOr(Schema.DateTimeUtcFromMillis),
  manifest: Schema.fromJsonString(Manifest),
  photoId: PhotoId,
  publication: Schema.NullOr(Schema.Literals(["Published", "Unpublishing"])),
  publishedAt: Schema.NullOr(Schema.DateTimeUtcFromMillis),
});

const Write = Schema.Struct({
  manifest: Schema.fromJsonString(Manifest),
  nowMillis: Schema.Int,
  photoId: PhotoId,
});

const Publish = Schema.Struct({
  details: Schema.fromJsonString(PhotoDetails),
  manifest: Schema.fromJsonString(Manifest),
  nowMillis: Schema.Int,
  photoId: PhotoId,
});

const Page = Schema.Struct({
  after: Schema.NullOr(PhotoId),
  limit: Schema.Int,
});

// The migration's checks keep publication, published_at and details all null or all set.
const toStored = ({
  details,
  finalizedAt,
  manifest,
  photoId,
  publication,
  publishedAt,
}: typeof PhotoRow.Type): Stored => {
  if (finalizedAt === null) {
    return { _tag: "Uploading", manifest, photoId };
  }
  if (publication === null || publishedAt === null || details === null) {
    return { _tag: "Draft", finalizedAt, manifest, photoId };
  }
  return {
    _tag: publication,
    details,
    finalizedAt,
    manifest,
    photoId,
    publishedAt,
  };
};

const isPublished = (stored: Stored): stored is Published =>
  stored._tag === "Published";

const isDraft = (stored: Stored): stored is Draft => stored._tag === "Draft";

const sameManifest = Schema.toEquivalence(Manifest);

const attempt =
  (operation: string) =>
  <A, E, R>(statement: Effect.Effect<A, E, R>) =>
    Effect.mapError(
      statement,
      (cause) => new LedgerUnavailable({ cause, operation })
    );

export interface Ledger {
  readonly find: (
    photoId: PhotoId
  ) => Effect.Effect<Option.Option<Stored>, LedgerUnavailable>;
  /** Up to `limit` rows after `after`, in photo ID order. */
  readonly page: (
    after: PhotoId | null,
    limit: number
  ) => Effect.Effect<readonly Stored[], LedgerUnavailable>;
  /** A new manifest reopens a draft; a public photo keeps its manifest and answers a conflict. */
  readonly declare: (
    photoId: PhotoId,
    manifest: Manifest,
    nowMillis: number
  ) => Effect.Effect<Stored, LedgerManifestConflict | LedgerUnavailable>;
  readonly markFinal: (
    photoId: PhotoId,
    verified: Manifest,
    nowMillis: number
  ) => Effect.Effect<Draft, LedgerManifestConflict | LedgerUnavailable>;
  /**
   * Marks a finalized photo of exactly `copied` published with `details`. A
   * published photo keeps its first publishedAt. None when the row no longer
   * holds that manifest, or is no longer finalized.
   */
  readonly markPublished: (
    photoId: PhotoId,
    copied: Manifest,
    details: PhotoDetails,
    nowMillis: number
  ) => Effect.Effect<Option.Option<Published>, LedgerUnavailable>;
  /** Published becomes Unpublishing; any other state is left as it is. */
  readonly markUnpublishing: (
    photoId: PhotoId
  ) => Effect.Effect<void, LedgerUnavailable>;
  /** Unpublishing becomes Draft. None when the row was not Unpublishing. */
  readonly markPrivate: (
    photoId: PhotoId
  ) => Effect.Effect<Option.Option<Draft>, LedgerUnavailable>;
  readonly published: Effect.Effect<readonly Published[], LedgerUnavailable>;
}

export const makeLedger: Effect.Effect<Ledger, never, SqlClient.SqlClient> =
  Effect.gen(function* makeLedger() {
    const sql = yield* SqlClient.SqlClient;
    const columns = sql.unsafe(
      "photo_id AS photoId, manifest, finalized_at AS finalizedAt, publication, published_at AS publishedAt, details"
    );

    const select = SqlSchema.findOneOption({
      Request: PhotoId,
      Result: PhotoRow,
      execute: (photoId) =>
        sql`SELECT ${columns} FROM photo WHERE photo_id = ${photoId}`,
    });

    const selectPage = SqlSchema.findAll({
      Request: Page,
      Result: PhotoRow,
      execute: ({ after, limit }) =>
        after === null
          ? sql`SELECT ${columns} FROM photo ORDER BY photo_id LIMIT ${limit}`
          : sql`SELECT ${columns} FROM photo WHERE photo_id > ${after} ORDER BY photo_id LIMIT ${limit}`,
    });

    const selectPublished = SqlSchema.findAll({
      Request: Schema.Null,
      Result: PhotoRow,
      execute: () =>
        sql`SELECT ${columns} FROM photo WHERE publication = 'Published' ORDER BY photo_id`,
    });

    const upsert = SqlSchema.findOne({
      Request: Write,
      Result: PhotoRow,
      execute: ({ manifest, nowMillis, photoId }) => sql`
        INSERT INTO photo (photo_id, manifest, created_at)
        VALUES (${photoId}, ${manifest}, ${nowMillis})
        ON CONFLICT (photo_id) DO UPDATE SET
          finalized_at = CASE
            WHEN photo.manifest = excluded.manifest OR photo.publication IS NOT NULL
            THEN photo.finalized_at
          END,
          manifest = CASE
            WHEN photo.publication IS NULL THEN excluded.manifest
            ELSE photo.manifest
          END
        RETURNING ${columns}`,
    });

    const compareAndMark = SqlSchema.findOneOption({
      Request: Write,
      Result: PhotoRow,
      execute: ({ manifest, nowMillis, photoId }) => sql`
        UPDATE photo SET finalized_at = ${nowMillis}
        WHERE photo_id = ${photoId} AND finalized_at IS NULL AND manifest = ${manifest}
        RETURNING ${columns}`,
    });

    const compareAndPublish = SqlSchema.findOneOption({
      Request: Publish,
      Result: PhotoRow,
      execute: ({ details, manifest, nowMillis, photoId }) => sql`
        UPDATE photo SET
          publication = 'Published',
          published_at = CASE
            WHEN publication = 'Published' THEN published_at
            ELSE ${nowMillis}
          END,
          details = ${details}
        WHERE photo_id = ${photoId} AND finalized_at IS NOT NULL AND manifest = ${manifest}
        RETURNING ${columns}`,
    });

    const compareAndPrivate = SqlSchema.findOneOption({
      Request: PhotoId,
      Result: PhotoRow,
      execute: (photoId) => sql`
        UPDATE photo SET publication = NULL, published_at = NULL, details = NULL
        WHERE photo_id = ${photoId} AND publication = 'Unpublishing'
        RETURNING ${columns}`,
    });

    const find: Ledger["find"] = (photoId) =>
      select(photoId).pipe(
        attempt("ledger.find"),
        Effect.map(Option.map(toStored))
      );

    const page: Ledger["page"] = (after, limit) =>
      selectPage({ after, limit }).pipe(
        attempt("ledger.page"),
        Effect.map((rows) => rows.map(toStored))
      );

    const declare: Ledger["declare"] = (photoId, manifest, nowMillis) =>
      Effect.gen(function* declarePhoto() {
        const stored = toStored(
          yield* upsert({ manifest, nowMillis, photoId }).pipe(
            attempt("ledger.declare")
          )
        );
        if (!sameManifest(stored.manifest, manifest)) {
          return yield* new LedgerManifestConflict({ photoId });
        }
        return stored;
      });

    const markFinal: Ledger["markFinal"] = (photoId, verified, nowMillis) =>
      Effect.gen(function* markPhotoFinal() {
        const marked = yield* compareAndMark({
          manifest: verified,
          nowMillis,
          photoId,
        }).pipe(attempt("ledger.markFinal"));
        const stored = Option.isSome(marked)
          ? Option.some(toStored(marked.value))
          : yield* find(photoId);
        if (
          Option.isSome(stored) &&
          isDraft(stored.value) &&
          sameManifest(stored.value.manifest, verified)
        ) {
          return stored.value;
        }
        return yield* new LedgerManifestConflict({ photoId });
      });

    const markPublished: Ledger["markPublished"] = (
      photoId,
      copied,
      details,
      nowMillis
    ) =>
      compareAndPublish({ details, manifest: copied, nowMillis, photoId }).pipe(
        attempt("ledger.markPublished"),
        Effect.map(Option.map(toStored)),
        Effect.map(Option.filter(isPublished))
      );

    const markUnpublishing: Ledger["markUnpublishing"] = (photoId) =>
      sql`UPDATE photo SET publication = 'Unpublishing' WHERE photo_id = ${photoId} AND publication = 'Published'`.pipe(
        Effect.asVoid,
        attempt("ledger.markUnpublishing")
      );

    const markPrivate: Ledger["markPrivate"] = (photoId) =>
      compareAndPrivate(photoId).pipe(
        attempt("ledger.markPrivate"),
        Effect.map(Option.map(toStored)),
        Effect.map(Option.filter(isDraft))
      );

    const published: Ledger["published"] = selectPublished(null).pipe(
      attempt("ledger.published"),
      Effect.map((rows) => rows.map(toStored).filter(isPublished))
    );

    return {
      declare,
      find,
      markFinal,
      markPrivate,
      markPublished,
      markUnpublishing,
      page,
      published,
    };
  });
