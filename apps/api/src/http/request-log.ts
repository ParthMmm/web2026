import { Cause, Clock, Effect, Exit, Layer, Predicate, Schema } from "effect";
import type { HttpServerResponse } from "effect/http";
import { HttpRouter, HttpServerRequest } from "effect/http";

import { PhotoId, VariantKindSchema } from "../photos/domain.ts";
import { RequestLog } from "./contract.ts";

const isPhotoId = Schema.is(PhotoId);
const isKind = Schema.is(VariantKindSchema);

const outcomeOf = (
  exit: Exit.Exit<HttpServerResponse.HttpServerResponse, unknown>
) => {
  if (Exit.isSuccess(exit)) {
    return exit.value.status;
  }
  const failure = exit.cause.reasons.find(Cause.isFailReason)?.error;
  return Predicate.hasProperty(failure, "_tag") &&
    typeof failure._tag === "string"
    ? failure._tag
    : "defect";
};

export const requestLog = Layer.succeed(
  RequestLog,
  RequestLog.of((httpEffect, { endpoint, group }) =>
    Effect.gen(function* () {
      const request = yield* HttpServerRequest.HttpServerRequest;
      const { params } = yield* HttpRouter.RouteContext;
      const started = yield* Clock.currentTimeMillis;
      return yield* httpEffect.pipe(
        Effect.onExit((exit) =>
          Clock.currentTimeMillis.pipe(
            Effect.flatMap((ended) =>
              Effect.logInfo("request", {
                durationMs: ended - started,
                outcome: outcomeOf(exit),
              })
            )
          )
        ),
        Effect.annotateLogs({
          operation: `${group.identifier}.${endpoint.identifier}`,
          requestId: request.headers["cf-ray"] ?? crypto.randomUUID(),
          ...(isPhotoId(params.photoId) ? { photoId: params.photoId } : {}),
          ...(isKind(params.kind) ? { kind: params.kind } : {}),
        })
      );
    })
  )
);
