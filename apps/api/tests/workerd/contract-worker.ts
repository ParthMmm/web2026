import type * as Cloudflare from "@alchemy.run/cloudflare";
import { makeHelpers } from "@alchemy.run/cloudflare/R2/BucketBinding";
import { makeReadWrite } from "@alchemy.run/cloudflare/R2/ReadWriteBucketBinding";
import type { D1Database, R2Bucket } from "@cloudflare/workers-types";
import { D1Client } from "@effect/sql-d1";
import { Cause, Effect, Exit, Layer } from "effect";
import { Digest } from "effect/crypto";

import { r2Blobs } from "../../src/photos/blobs-r2.ts";
import { Blobs } from "../../src/photos/blobs.ts";
import { Photos } from "../../src/photos/photos.ts";
import { blobsContract, contractKey } from "../support/blobs-contract.ts";
import { photosScenarios } from "./scenarios.ts";

interface Env {
  readonly Private: R2Bucket;
  readonly Db: D1Database;
}

// SAFETY: makeHelpers reads only `LogicalId` from the bucket resource, to find the binding in env.
const privateBucket = { LogicalId: "Private" } as Cloudflare.R2.Bucket;

const run = async (effect: Effect.Effect<void, unknown>) => {
  const exit = await Effect.runPromiseExit(effect);
  return Response.json(
    Exit.isSuccess(exit)
      ? { ok: true }
      : { error: Cause.pretty(exit.cause), ok: false }
  );
};

export default {
  fetch: (request: Request, env: Env) => {
    const url = new URL(request.url);
    const name = url.searchParams.get("case");
    const blobs = Layer.succeed(
      Blobs,
      r2Blobs(makeReadWrite(makeHelpers(env, privateBucket)))
    );

    const contract = blobsContract.find((entry) => entry.name === name);
    if (url.pathname === "/blobs" && contract !== undefined) {
      return run(
        contract
          .run(contractKey(crypto.randomUUID()))
          .pipe(Effect.provide(blobs))
      );
    }

    const scenario = photosScenarios.find((entry) => entry.name === name);
    if (url.pathname === "/photos" && scenario !== undefined) {
      const photos = Photos.layer.pipe(
        Layer.provideMerge(blobs),
        Layer.provide(D1Client.layer({ db: env.Db })),
        Layer.provideMerge(Digest.layerWebCrypto)
      );
      return run(scenario.run.pipe(Effect.provide(photos)));
    }

    return new Response(`unknown case ${name}`, { status: 404 });
  },
};
