import * as Cloudflare from "@alchemy.run/cloudflare";
import * as RemovalPolicy from "alchemy/RemovalPolicy";
import { Stage } from "alchemy/Stage";
import { Effect } from "effect";

import { STAGING_PREFIX } from "./internal/keys.ts";

const SECONDS_PER_DAY = 86_400;
const STAGING_RETENTION_DAYS = 7;

const retainInProd = RemovalPolicy.retain(
  Effect.map(Stage, (stage) => stage === "prod")
);

export const PrivateBucket = Cloudflare.R2.Bucket("Private", {
  lifecycleRules: [
    {
      deleteObjectsTransition: {
        condition: {
          maxAge: STAGING_RETENTION_DAYS * SECONDS_PER_DAY,
          type: "Age",
        },
      },
      id: "expire-staging",
      prefix: STAGING_PREFIX,
    },
  ],
  publicAccess: false,
}).pipe(retainInProd);

export const PhotoDatabase = Cloudflare.D1.Database("Db", {
  migrations: "./migrations",
}).pipe(retainInProd);
