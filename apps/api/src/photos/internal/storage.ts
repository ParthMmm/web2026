import { Effect, Predicate } from "effect";

import type { BlobUnavailable } from "../blobs.ts";
import { StorageUnavailable } from "../domain.ts";
import type { LedgerUnavailable } from "./ledger.ts";

const isStorageFailure = <E>(
  error: E
): error is Extract<E, BlobUnavailable | LedgerUnavailable> =>
  Predicate.isTagged(error, "BlobUnavailable") ||
  Predicate.isTagged(error, "LedgerUnavailable");

export const toUnavailable = ({
  cause,
  operation,
}: BlobUnavailable | LedgerUnavailable) =>
  Effect.logWarning("storage failure", { cause, operation }).pipe(
    Effect.andThen(Effect.fail(new StorageUnavailable({ operation })))
  );

/** Logs a blob or ledger failure with its cause, and answers the caller a bare StorageUnavailable. */
export const storage = <A, E, R>(effect: Effect.Effect<A, E, R>) =>
  Effect.catchIf(effect, isStorageFailure<E>, (failure) =>
    toUnavailable(failure)
  );
