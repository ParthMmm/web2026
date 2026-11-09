import { Cause, Clock, Effect, Exit, Layer, Schema } from "effect";
import type { HttpServerResponse } from "effect/http";
import { HttpRouter, HttpServerRequest } from "effect/http";

import { PhotoId, VariantKindSchema } from "../photos/domain.ts";
import type { VariantKind } from "../photos/domain.ts";
import { RequestLog } from "./contract.ts";

const isPhotoId = Schema.is(PhotoId);
const isKind = Schema.is(VariantKindSchema);
const isTagged = Schema.is(Schema.Struct({ _tag: Schema.String }));

// Effect.annotateLogs takes a string-keyed record.
interface RequestAnnotations extends Record<string, unknown> {
  readonly operation: string;
  readonly requestId: string;
  photoId?: PhotoId;
  kind?: VariantKind;
}

const outcomeOf = (
  exit: Exit.Exit<HttpServerResponse.HttpServerResponse, unknown>
) => {
  if (Exit.isSuccess(exit)) {
    return exit.value.status;
  }
  const failure = exit.cause.reasons.find(Cause.isFailReason)?.error;
  return isTagged(failure) ? failure._tag : "defect";
};

export const requestLog = Layer.succeed(
  RequestLog,
  RequestLog.of((httpEffect, { endpoint, group }) =>
    Effect.gen(function* logRequest() {
      const request = yield* HttpServerRequest.HttpServerRequest;
      const { params } = yield* HttpRouter.RouteContext;
      const started = yield* Clock.currentTimeMillis;
      const annotations: RequestAnnotations = {
        operation: `${group.identifier}.${endpoint.identifier}`,
        requestId: request.headers["cf-ray"] ?? crypto.randomUUID(),
      };
      if (isPhotoId(params.photoId)) {
        annotations.photoId = params.photoId;
      }
      if (isKind(params.kind)) {
        annotations.kind = params.kind;
      }
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
        Effect.annotateLogs(annotations)
      );
    })
  )
);
