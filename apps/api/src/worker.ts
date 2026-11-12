import * as Cloudflare from "@alchemy.run/cloudflare";
import { D1Layer } from "@alchemy.run/cloudflare/D1/SQL";
import { Config, Effect, Layer, Schema } from "effect";
import { HttpRouter } from "effect/http";

import { photosApp } from "./http/handlers.ts";
import { r2Blobs } from "./photos/blobs-r2.ts";
import { Blobs, PublicBlobs } from "./photos/blobs.ts";
import { PhotoDatabase, PrivateBucket, PublicBucket } from "./photos/infra.ts";
import { Photos } from "./photos/photos.ts";
import { PublicGallery } from "./photos/public-gallery.ts";
import { Publishing } from "./photos/publishing.ts";

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
  Effect.gen(function* makeWorker() {
    const privateBucket = yield* Cloudflare.R2.ReadWriteBucket(PrivateBucket);
    const publicBucket = yield* Cloudflare.R2.ReadWriteBucket(PublicBucket);
    const database = yield* Cloudflare.D1.QueryDatabase(PhotoDatabase);
    const ownerToken = yield* OwnerToken;
    const publicBlobs = Layer.succeed(PublicBlobs, r2Blobs(publicBucket));
    const owner = Layer.mergeAll(Photos.layer, Publishing.layer).pipe(
      Layer.provide([
        Layer.succeed(Blobs, r2Blobs(privateBucket)),
        publicBlobs,
        D1Layer(database),
      ])
    );
    const gallery = PublicGallery.layer.pipe(Layer.provide(publicBlobs));
    return {
      fetch: HttpRouter.toHttpEffect(photosApp(owner, gallery, ownerToken)),
    };
  }).pipe(
    Effect.provide(
      Layer.mergeAll(
        Cloudflare.R2.ReadWriteBucketBinding,
        Cloudflare.D1.QueryDatabaseBinding
      )
    )
  )
);
