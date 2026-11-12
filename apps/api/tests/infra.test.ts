import * as Cloudflare from "@alchemy.run/cloudflare";
import { expect, it } from "@effect/vitest";
import { Stack } from "alchemy/Stack";
import { Stage } from "alchemy/Stage";
import { Effect } from "effect";

import {
  PhotoDatabase,
  PrivateBucket,
  PublicBucket,
} from "../src/photos/infra.ts";

const policies = Effect.all({
  PhotoDatabase: PhotoDatabase.pipe(Effect.map((db) => db.RemovalPolicy)),
  PrivateBucket: PrivateBucket.pipe(
    Effect.map((bucket) => bucket.RemovalPolicy)
  ),
  PublicBucket: PublicBucket.pipe(Effect.map((bucket) => bucket.RemovalPolicy)),
});

const removalPolicyIn = (stage: string) =>
  policies.pipe(
    // Declaring a resource never looks a provider up; only plan and deploy do.
    Effect.provideService(Cloudflare.Providers, {
      get: (): undefined => {},
      kind: "ProviderCollection",
      providers: {},
    }),
    Effect.provideService(Stage, stage),
    Effect.provideService(Stack, {
      actions: {},
      bindings: {},
      name: "web2026-photos",
      resources: {},
      stage,
    })
  );

it.effect(
  "retains every bucket and the database in prod, and destroys them in other stages",
  () =>
    Effect.gen(function* run() {
      expect(yield* removalPolicyIn("prod")).toEqual({
        PhotoDatabase: "retain",
        PrivateBucket: "retain",
        PublicBucket: "retain",
      });
      expect(yield* removalPolicyIn("photos-proof")).toEqual({
        PhotoDatabase: "destroy",
        PrivateBucket: "destroy",
        PublicBucket: "destroy",
      });
    })
);
