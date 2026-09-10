import { Effect, FileSystem, Layer, Path, Redacted } from "effect";
import { Etag, HttpPlatform, HttpRouter } from "effect/unstable/http";
import { HttpApiBuilder } from "effect/unstable/httpapi";

import { Api, InvalidCursor, ProbeAuth, Unauthorized } from "./contract.ts";

export const makeHandler = (token: string) => {
  if (!token) {
    throw new Error("PROBE_TOKEN must be configured");
  }
  const auth = Layer.succeed(ProbeAuth)({
    bearer: (effect, { credential }) =>
      Redacted.value(credential) === token
        ? effect
        : Effect.fail(new Unauthorized()),
  });
  const group = HttpApiBuilder.group(Api, "probe", (handlers) =>
    handlers
      .handle("echoPhoto", ({ payload }) => Effect.succeed(payload))
      .handle("listPhotos", ({ query }) => {
        if (query.cursor === "page-2") {
          return Effect.succeed({ items: [], nextCursor: null });
        }
        if (query.cursor !== undefined) {
          return Effect.fail(new InvalidCursor({ cursor: query.cursor }));
        }
        return Effect.succeed({
          items: [
            {
              capturedAt: null,
              id: "fixture-1",
              state: { _tag: "Draft" as const },
            },
          ],
          nextCursor: "page-2",
        });
      })
  ).pipe(Layer.provide(auth));
  const platform = Layer.mergeAll(
    Path.layer,
    Etag.layerWeak,
    HttpPlatform.layer
  ).pipe(Layer.provideMerge(FileSystem.layerNoop({})));
  return HttpRouter.toWebHandler(
    HttpApiBuilder.layer(Api).pipe(
      Layer.provide(group),
      Layer.provide(platform)
    ),
    { disableLogger: true }
  );
};
