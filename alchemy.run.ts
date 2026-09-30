import * as Alchemy from "alchemy";
import * as Cloudflare from "alchemy/Cloudflare";
import { Effect } from "effect";

export default Alchemy.Stack(
  "web2026-site",
  { providers: Cloudflare.providers(), state: Alchemy.localState() },
  Effect.gen(function* site() {
    const website = yield* Cloudflare.Website.StaticSite("Website", {
      command: "bun run build",
      dev: { command: "bun run dev" },
      domain: {
        name: "www.parthm.dev",
        redirects: ["parthm.dev", "p11a.xyz", "parthm.me", "parthm.xyz"],
      },
      outdir: "dist",
      workersDev: { enabled: true, previewsEnabled: false },
    });
    return { url: website.url };
  })
);
