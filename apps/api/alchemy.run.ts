import * as Cloudflare from "@alchemy.run/cloudflare";
import * as Alchemy from "alchemy";
import { Effect } from "effect";

import { PublicBucket } from "./src/publishing/public-bucket.ts";
import PhotosApi from "./src/worker.ts";

export default Alchemy.Stack(
  "web2026-photos",
  { providers: Cloudflare.providers(), state: Alchemy.localState() },
  Effect.gen(function* photos() {
    const api = yield* PhotosApi;
    yield* PublicBucket;
    return { url: api.url };
  }),
);
