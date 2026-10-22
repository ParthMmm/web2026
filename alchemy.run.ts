import * as Alchemy from "alchemy";
import * as Cloudflare from "alchemy/Cloudflare";
import { Config, Effect, Redacted } from "effect";

/** Unset means that source is off; the Worker skips empty values. */
const optionalSecret = (name: string) =>
  Config.Redacted(name).pipe(Config.withDefault(Redacted.make("")));
const optionalString = (name: string) =>
  Config.String(name).pipe(Config.withDefault(""));

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

    const nowPlaying = yield* Cloudflare.Worker("NowPlaying", {
      compatibility: { date: "2026-09-29" },
      crons: ["* * * * *"],
      domain: "now.parthm.dev",
      env: {
        ALLOWED_ORIGINS: "https://www.parthm.dev,http://localhost:4321",
        APPLE_MUSIC_KEY_ID: optionalString("APPLE_MUSIC_KEY_ID"),
        APPLE_MUSIC_PRIVATE_KEY: optionalSecret("APPLE_MUSIC_PRIVATE_KEY"),
        APPLE_MUSIC_TEAM_ID: optionalString("APPLE_MUSIC_TEAM_ID"),
        APPLE_MUSIC_USER_TOKEN: optionalSecret("APPLE_MUSIC_USER_TOKEN"),
        LASTFM_API_KEY: optionalSecret("LASTFM_API_KEY"),
        LASTFM_USER: optionalString("LASTFM_USER"),
        NOW_PLAYING: Cloudflare.DurableObject("NowPlaying", {
          className: "NowPlaying",
        }),
        ORBIS_API_KEY: optionalSecret("ORBIS_API_KEY"),
        ORBIS_API_URL: "https://vanta.tail01d084.ts.net:10000/api",
        ORBIS_USERNAME: optionalString("ORBIS_USERNAME"),
      },
      main: "./workers/now-playing/worker.ts",
      observability: { enabled: true },
    });

    return { nowPlaying: nowPlaying.url, url: website.url };
  })
);
