/* oxlint-disable max-classes-per-file -- the ledger's two failure modes belong with it. */
/* oxlint-disable unicorn/throw-new-error -- Schema.TaggedError is a curried class factory, not an Error constructor. */
import { Effect, Option, Schema } from "effect";
import type { DateTime } from "effect";
import { SqlClient, SqlSchema } from "effect/sql";

import { Manifest, PhotoId } from "../domain.ts";

export class LedgerUnavailable extends Schema.TaggedError<LedgerUnavailable>()(
  "LedgerUnavailable",
  { cause: Schema.Defect(), operation: Schema.String },
) {}

export class LedgerManifestConflict extends Schema.TaggedError<LedgerManifestConflict>()(
  "LedgerManifestConflict",
  { photoId: PhotoId },
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

export type Stored = Uploading | Draft;

const PhotoRow = Schema.Struct({
  finalizedAt: Schema.NullOr(Schema.DateTimeUtcFromMillis),
  manifest: Schema.fromJsonString(Manifest),
  photoId: PhotoId,
});

const Write = Schema.Struct({
  manifest: Schema.fromJsonString(Manifest),
  nowMillis: Schema.Int,
  photoId: PhotoId,
});

const toStored = ({ finalizedAt, manifest, photoId }: typeof PhotoRow.Type): Stored =>
  finalizedAt === null
    ? { _tag: "Uploading", manifest, photoId }
    : { _tag: "Draft", finalizedAt, manifest, photoId };

const sameManifest = Schema.toEquivalence(Manifest);

export interface Ledger {
  readonly find: (photoId: PhotoId) => Effect.Effect<Option.Option<Stored>, LedgerUnavailable>;
  readonly declare: (
    photoId: PhotoId,
    manifest: Manifest,
    nowMillis: number,
  ) => Effect.Effect<Stored, LedgerManifestConflict | LedgerUnavailable>;
  readonly markFinal: (
    photoId: PhotoId,
    verified: Manifest,
    nowMillis: number,
  ) => Effect.Effect<Draft, LedgerManifestConflict | LedgerUnavailable>;
}

export const makeLedger: Effect.Effect<Ledger, never, SqlClient.SqlClient> = Effect.gen(
  function* () {
    const sql = yield* SqlClient.SqlClient;
    const columns = sql.unsafe("photo_id AS photoId, manifest, finalized_at AS finalizedAt");

    const select = SqlSchema.findOneOption({
      execute: (photoId) => sql`SELECT ${columns} FROM photo WHERE photo_id = ${photoId}`,
      Request: PhotoId,
      Result: PhotoRow,
    });

    const upsert = SqlSchema.findOne({
      execute: ({ manifest, nowMillis, photoId }) => sql`
        INSERT INTO photo (photo_id, manifest, created_at)
        VALUES (${photoId}, ${manifest}, ${nowMillis})
        ON CONFLICT (photo_id) DO UPDATE SET manifest = CASE
          WHEN photo.finalized_at IS NULL THEN excluded.manifest
          ELSE photo.manifest
        END
        RETURNING ${columns}`,
      Request: Write,
      Result: PhotoRow,
    });

    const compareAndMark = SqlSchema.findOneOption({
      execute: ({ manifest, nowMillis, photoId }) => sql`
        UPDATE photo SET finalized_at = ${nowMillis}
        WHERE photo_id = ${photoId} AND finalized_at IS NULL AND manifest = ${manifest}
        RETURNING ${columns}`,
      Request: Write,
      Result: PhotoRow,
    });

    const attempt =
      (operation: string) =>
      <A, E, R>(statement: Effect.Effect<A, E, R>) =>
        Effect.mapError(statement, (cause) => new LedgerUnavailable({ cause, operation }));

    const find: Ledger["find"] = (photoId) =>
      select(photoId).pipe(attempt("ledger.find"), Effect.map(Option.map(toStored)));

    const declare: Ledger["declare"] = (photoId, manifest, nowMillis) =>
      Effect.gen(function* () {
        const stored = toStored(
          yield* upsert({ manifest, nowMillis, photoId }).pipe(attempt("ledger.declare")),
        );
        if (!sameManifest(stored.manifest, manifest)) {
          return yield* new LedgerManifestConflict({ photoId });
        }
        return stored;
      });

    const markFinal: Ledger["markFinal"] = (photoId, verified, nowMillis) =>
      Effect.gen(function* () {
        const marked = yield* compareAndMark({ manifest: verified, nowMillis, photoId }).pipe(
          attempt("ledger.markFinal"),
        );
        const stored = Option.isSome(marked) ? Option.map(marked, toStored) : yield* find(photoId);
        if (
          Option.isSome(stored) &&
          stored.value._tag === "Draft" &&
          sameManifest(stored.value.manifest, verified)
        ) {
          return stored.value;
        }
        return yield* new LedgerManifestConflict({ photoId });
      });

    return { declare, find, markFinal };
  },
);
