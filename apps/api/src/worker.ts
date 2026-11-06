import * as Cloudflare from "@alchemy.run/cloudflare";
import { D1Layer } from "@alchemy.run/cloudflare/D1/SQL";
import { Config, Effect, Layer, Schema } from "effect";
import { HttpRouter } from "effect/http";

import { photosApp } from "./http/handlers.ts";
import { r2Blobs } from "./photos/blobs-r2.ts";
import { PhotoDatabase, PrivateBucket } from "./photos/infra.ts";
import { Photos } from "./photos/photos.ts";

const MIN_TOKEN_CHARS = 32;

const OwnerToken = Config.schema(
  Schema.Redacted(Schema.String.check(Schema.isMinLength(MIN_TOKEN_CHARS))),
  "OWNER_TOKEN"
);

export default Cloudflare.Worker(
  "PhotosApi",
  {
    compatibility: { date: "2026-08-01" },
    main: import.meta.url,
    observability: {
      enabled: true,
      logs: { enabled: true, invocationLogs: false },
    },
    workersDev: { enabled: true, previewsEnabled: false },
  },
  Effect.gen(function* () {
    const bucket = yield* Cloudflare.R2.ReadWriteBucket(PrivateBucket);
    const database = yield* Cloudflare.D1.QueryDatabase(PhotoDatabase);
    const ownerToken = yield* OwnerToken;
    const photos = Photos.layer.pipe(
      Layer.provide([r2Blobs(bucket), D1Layer(database)])
    );
    return { fetch: HttpRouter.toHttpEffect(photosApp(photos, ownerToken)) };
  }).pipe(
    Effect.provide(
      Layer.mergeAll(
        Cloudflare.R2.ReadWriteBucketBinding,
        Cloudflare.D1.QueryDatabaseBinding
      )
    )
  )
);
