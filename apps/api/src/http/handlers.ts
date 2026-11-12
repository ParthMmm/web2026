import * as Http from "alchemy/Http";
import { Effect, Layer } from "effect";
import type { Redacted } from "effect";
import { Digest } from "effect/crypto";
import { HttpApiBuilder } from "effect/http-api";

import { Photos } from "../photos/photos.ts";
import type { PublicGallery } from "../photos/public-gallery.ts";
import { Publishing } from "../photos/publishing.ts";
import { Api } from "./contract.ts";
import { galleryRoutes } from "./gallery-routes.ts";
import { ownerTokenAuth } from "./owner-auth.ts";
import { requestLog } from "./request-log.ts";

const PhotosHandlers = HttpApiBuilder.group(
  Api,
  "photos",
  Effect.fn(function* handlePhotos(handlers) {
    const photos = yield* Photos;
    const publishing = yield* Publishing;
    return handlers.handleAll({
      begin: ({ payload }) => photos.begin(payload.manifest),
      finalize: ({ params }) => photos.finalize(params.photoId),
      get: ({ params }) => photos.get(params.photoId),
      list: ({ query }) => photos.list(query),
      publish: ({ params, payload }) =>
        publishing.publish(params.photoId, payload),
      put: ({ headers, params, payload }) =>
        photos.store({
          body: payload,
          contentLength: headers["content-length"],
          kind: params.kind,
          photoId: params.photoId,
        }),
      unpublish: ({ params }) => publishing.unpublish(params.photoId),
    });
  })
);

/**
 * The owner's API and the public gallery routes on one router. The gallery
 * gets only its own layer, which reads only the public bucket.
 */
export const photosApp = <E, G>(
  owner: Layer.Layer<Photos | Publishing, E, Digest.Digest>,
  gallery: Layer.Layer<PublicGallery, G, Digest.Digest>,
  ownerToken: Redacted.Redacted<string>
) =>
  Layer.mergeAll(
    HttpApiBuilder.layer(Api).pipe(
      Layer.provide(PhotosHandlers),
      Layer.provide([owner, ownerTokenAuth(ownerToken), requestLog])
    ),
    galleryRoutes.pipe(Layer.provide(gallery))
  ).pipe(Layer.provide([Digest.layerWebCrypto, Http.Platform]));
