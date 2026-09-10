import * as Alchemy from "alchemy";
import * as Cloudflare from "alchemy/Cloudflare";
import { Config, Effect } from "effect";

export default Alchemy.Stack(
  "web2026-contract-probe",
  { providers: Cloudflare.providers(), state: Alchemy.localState() },
  Effect.gen(function* contractProbe() {
    const worker = yield* Cloudflare.Worker("Api", {
      compatibility: { date: "2026-09-09" },
      env: { PROBE_TOKEN: Config.redacted("PROBE_TOKEN") },
      main: "./src/worker.ts",
      observability: { enabled: false },
      workersDev: { enabled: true, previewsEnabled: false },
    });
    return { url: worker.url };
  })
);
