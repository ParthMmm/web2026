/* oxlint-disable max-classes-per-file -- Effect tagged errors and middleware declare one cohesive wire contract. */
/* oxlint-disable unicorn/throw-new-error -- Schema.TaggedError is a curried class factory, not an Error constructor. */
import { Schema } from "effect";
import {
  HttpApi,
  HttpApiEndpoint,
  HttpApiGroup,
  HttpApiMiddleware,
  HttpApiSecurity,
  HttpApiError,
  OpenApi,
} from "effect/unstable/httpapi";

export class Unauthorized extends Schema.TaggedError<Unauthorized>()(
  "Unauthorized",
  {},
  { httpApiStatus: 401 },
) {}

export class ProbeAuth extends HttpApiMiddleware.Service<ProbeAuth>()("ProbeAuth", {
  error: Unauthorized,
  security: { bearer: HttpApiSecurity.bearer },
}) {}

export const Photo = Schema.Struct({
  caption: Schema.optionalKey(Schema.String),
  filmSimulation: Schema.optionalKey(
    Schema.String.annotate({
      description: "Recorded camera film simulation, independent of rendering.",
    }),
  ),
  capturedAt: Schema.NullOr(Schema.DateTimeUtcFromString),
  id: Schema.String,
  state: Schema.Union([
    Schema.TaggedStruct("Draft", {}),
    Schema.TaggedStruct("Published", { url: Schema.String }),
  ]),
}).annotate({ identifier: "ProbePhoto" });

export class InvalidCursor extends Schema.TaggedError<InvalidCursor>()(
  "InvalidCursor",
  { cursor: Schema.String },
  { httpApiStatus: 400 },
) {}

export const Page = Schema.Struct({
  items: Schema.Array(Photo),
  nextCursor: Schema.NullOr(Schema.String),
}).annotate({ identifier: "PhotoPage" });

export const Api = HttpApi.make("PhotoContractProbe")
  .annotate(OpenApi.Title, "Photo contract probe")
  .add(
    HttpApiGroup.make("probe").add(
      HttpApiEndpoint.post("echoPhoto", "/v1/probe/photos", {
        error: HttpApiError.BadRequestNoContent,
        payload: Photo,
        success: Photo,
      }).middleware(ProbeAuth),
      HttpApiEndpoint.get("listPhotos", "/v1/probe/photos", {
        error: InvalidCursor,
        query: { cursor: Schema.optionalKey(Schema.String) },
        success: Page,
      }).middleware(ProbeAuth),
    ),
  );
