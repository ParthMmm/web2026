import * as Http from "alchemy/Http";
import { Effect, Layer } from "effect";
import type { Redacted } from "effect";
import { Digest } from "effect/crypto";
import { HttpApiBuilder } from "effect/http-api";

import { Photos } from "../photos/photos.ts";
import { Api } from "./contract.ts";
import { ownerTokenAuth } from "./owner-auth.ts";
import { requestLog } from "./request-log.ts";

const PhotosHandlers = HttpApiBuilder.group(
  Api,
  "photos",
  Effect.fn(function* handlePhotos(handlers) {
    const photos = yield* Photos;
    return handlers.handleAll({
      begin: ({ payload }) => photos.begin(payload.manifest),
      finalize: ({ params }) => photos.finalize(params.photoId),
      get: ({ params }) => photos.get(params.photoId),
      put: ({ headers, params, payload }) =>
        photos.store({
          body: payload,
          contentLength: headers["content-length"],
          kind: params.kind,
          photoId: params.photoId,
        }),
    });
  })
);

export const photosApp = <E>(
  photos: Layer.Layer<Photos, E>,
  ownerToken: Redacted.Redacted<string>
) =>
  HttpApiBuilder.layer(Api).pipe(
    Layer.provide(PhotosHandlers),
    Layer.provide([photos, ownerTokenAuth(ownerToken), requestLog]),
    Layer.provide([Digest.layerWebCrypto, Http.Platform])
  );
