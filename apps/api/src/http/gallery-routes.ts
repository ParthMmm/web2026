import { Effect, Schema } from "effect";
import { HttpRouter, HttpServerResponse } from "effect/http";

import type { BlobBody } from "../photos/blobs.ts";
import { PhotoId, Sha256 } from "../photos/domain.ts";
import type { StorageUnavailable } from "../photos/domain.ts";
import { PublicGallery } from "../photos/public-gallery.ts";

const ANYONE = { "access-control-allow-origin": "*" };
const MANIFEST_MAX_AGE = "public, max-age=60";
const IMMUTABLE = "public, max-age=31536000, immutable";

const DERIVATIVE_FILE =
  /^(?<kind>w480|w1200|w2400)-(?<sha256>[0-9a-f]{64})\.webp$/u;
const isPhotoId = Schema.is(PhotoId);
const isSha256 = Schema.is(Sha256);
const isDerivativeKind = Schema.is(Schema.Literals(["w480", "w1200", "w2400"]));

const found = (
  { body, size }: BlobBody,
  contentType: string,
  cacheControl: string
) =>
  HttpServerResponse.stream(body, {
    contentLength: size,
    contentType,
    headers: { ...ANYONE, "cache-control": cacheControl },
  });

const refused = (status: 404 | 503) =>
  HttpServerResponse.empty({
    headers: { ...ANYONE, "cache-control": "no-store" },
    status,
  });

const orUnavailable = <A, R>(effect: Effect.Effect<A, StorageUnavailable, R>) =>
  effect.pipe(
    Effect.catchTag("StorageUnavailable", () => Effect.succeed(refused(503)))
  );

/** Unauthenticated reads of the public bucket. They never touch the private bucket or D1. */
export const galleryRoutes = HttpRouter.addAll(
  Effect.gen(function* buildGalleryRoutes() {
    const gallery = yield* PublicGallery;

    const manifest = gallery.manifest.pipe(
      Effect.map((body) => found(body, "application/json", MANIFEST_MAX_AGE)),
      orUnavailable
    );

    const derivative = Effect.gen(function* serveDerivative() {
      const { file, photoId } = yield* HttpRouter.params;
      const { kind, sha256 } = DERIVATIVE_FILE.exec(file ?? "")?.groups ?? {};
      if (!isPhotoId(photoId) || !isDerivativeKind(kind) || !isSha256(sha256)) {
        return refused(404);
      }
      const body = yield* gallery.derivative(photoId, kind, sha256);
      return body === null
        ? refused(404)
        : found(body, "image/webp", IMMUTABLE);
    }).pipe(orUnavailable);

    return [
      HttpRouter.route("GET", "/gallery/v1.json", manifest),
      HttpRouter.route("GET", "/gallery/photos/:photoId/:file", derivative),
    ];
  })
);
