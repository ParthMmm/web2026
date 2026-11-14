/* oxlint-disable max-classes-per-file -- the contract owns its middleware tags and its one transport error. */
/* oxlint-disable unicorn/throw-new-error -- Schema.TaggedError is a curried class factory, not an Error constructor. */
import { Schema } from "effect";
import {
  HttpApi,
  HttpApiEndpoint,
  HttpApiGroup,
  HttpApiMiddleware,
  HttpApiSchema,
  HttpApiSecurity,
  OpenApi,
} from "effect/http-api";

import {
  Draft,
  FinalizeRejected,
  HashMismatch,
  LengthMismatch,
  Manifest,
  MAX_OBJECT_BYTES,
  OriginalUpload,
  PhotoAlreadyDraft,
  PhotoId,
  PageLimit,
  PhotoDetails,
  PhotoManifestConflict,
  PhotoNotDraft,
  PhotoNotFound,
  PhotoPage,
  PhotoView,
  photoIdOf,
  Published,
  StorageUnavailable,
  VariantKindSchema,
} from "../photos/domain.ts";

export class Unauthorized extends Schema.TaggedError<Unauthorized>()(
  "Unauthorized",
  {},
  { httpApiStatus: 401 }
) {}

export class OwnerAuth extends HttpApiMiddleware.Service<OwnerAuth>()(
  "OwnerAuth",
  {
    error: Unauthorized,
    security: { bearer: HttpApiSecurity.bearer },
  }
) {}

export class RequestLog extends HttpApiMiddleware.Service<RequestLog>()(
  "RequestLog"
) {}

export const BeginRequest = Schema.Struct({
  manifest: Manifest,
  photoId: PhotoId,
}).check(
  Schema.makeFilter(({ manifest, photoId }) =>
    photoId === photoIdOf(manifest)
      ? undefined
      : {
          issue: "photoId must equal manifest.original.sha256",
          path: ["photoId"],
        }
  )
);

const PhotoParams = { photoId: PhotoId };

export const PhotosGroup = HttpApiGroup.make("photos")
  .add(
    HttpApiEndpoint.post("begin", "/v1/photos", {
      error: [PhotoManifestConflict, StorageUnavailable],
      payload: BeginRequest,
      success: PhotoView,
    }),
    HttpApiEndpoint.get("list", "/v1/photos", {
      error: [StorageUnavailable],
      query: {
        after: Schema.optionalKey(PhotoId),
        limit: Schema.optionalKey(
          Schema.NumberFromString.pipe(Schema.decodeTo(PageLimit))
        ),
      },
      success: PhotoPage,
    }),
    HttpApiEndpoint.get("get", "/v1/photos/:photoId", {
      error: [PhotoNotFound, StorageUnavailable],
      params: PhotoParams,
      success: PhotoView,
    }),
    HttpApiEndpoint.post(
      "originalUpload",
      "/v1/photos/:photoId/original/upload-url",
      {
        error: [PhotoNotFound, PhotoAlreadyDraft, StorageUnavailable],
        params: PhotoParams,
        success: OriginalUpload,
      }
    ),
    HttpApiEndpoint.put("put", "/v1/photos/:photoId/:kind", {
      error: [
        PhotoNotFound,
        PhotoAlreadyDraft,
        LengthMismatch,
        HashMismatch,
        StorageUnavailable,
      ],
      headers: { "content-length": Schema.FiniteFromString },
      params: { kind: VariantKindSchema, photoId: PhotoId },
      payload: HttpApiSchema.StreamUint8Array({
        contentType: "application/octet-stream",
        maxBytes: MAX_OBJECT_BYTES,
      }),
      success: HttpApiSchema.NoContent,
    }),
    HttpApiEndpoint.post("finalize", "/v1/photos/:photoId/finalize", {
      error: [
        PhotoNotFound,
        PhotoManifestConflict,
        FinalizeRejected,
        StorageUnavailable,
      ],
      params: PhotoParams,
      success: PhotoView,
    }),
    HttpApiEndpoint.post("publish", "/v1/photos/:photoId/publish", {
      error: [PhotoNotFound, PhotoNotDraft, StorageUnavailable],
      params: PhotoParams,
      payload: PhotoDetails,
      success: Published,
    }),
    HttpApiEndpoint.post("unpublish", "/v1/photos/:photoId/unpublish", {
      error: [PhotoNotFound, PhotoNotDraft, StorageUnavailable],
      params: PhotoParams,
      success: Draft,
    })
  )
  .middleware(OwnerAuth)
  .middleware(RequestLog);

export const Api = HttpApi.make("PhotoUpload")
  .annotate(OpenApi.Title, "Photo upload API")
  .add(PhotosGroup);
