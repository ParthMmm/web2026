import * as Cloudflare from "@alchemy.run/cloudflare";
import * as Alchemy from "alchemy";
import { Config, Effect } from "effect";

export default Alchemy.Stack(
  "web2026-contract-probe",
  { providers: Cloudflare.providers(), state: Alchemy.localState() },
  Effect.gen(function* contractProbe() {
    const worker = yield* Cloudflare.Worker("Api", {
      compatibility: { date: "2026-09-09" },
      env: { PROBE_TOKEN: Config.Redacted("PROBE_TOKEN") },
      main: "./src/worker.ts",
      observability: { enabled: false },
      workersDev: { enabled: true, previewsEnabled: false },
    });
    return { url: worker.url };
  })
);
