import { DurableObject } from "cloudflare:workers";

import { signDeveloperToken } from "./apple-music-token";
import type { NowPlayingMessage, Track } from "./message";
import type { Answer } from "./sources";
import {
  attempt,
  found,
  nothing,
  readAppleMusicRecent,
  readLastfm,
  readOrbis,
  unavailable,
} from "./sources";
import type { StoredTopSong } from "./top-song";
import { fromStored, observe, toStored } from "./top-song";

/** Every source is optional: an empty value turns it off. */
export interface Env {
  ALLOWED_ORIGINS: string;
  APPLE_MUSIC_KEY_ID: string;
  APPLE_MUSIC_PRIVATE_KEY: string;
  APPLE_MUSIC_TEAM_ID: string;
  APPLE_MUSIC_USER_TOKEN: string;
  LASTFM_API_KEY: string;
  LASTFM_USER: string;
  NOW_PLAYING: DurableObjectNamespace<NowPlaying>;
  ORBIS_API_KEY: string;
  ORBIS_API_URL: string;
  ORBIS_USERNAME: string;
}

const ROOM = "parth";
const MESSAGE_KEY = "message";
const LATEST_SONG_KEY = "appleMusicLatest";
const LIVE_POLL_MS = 15_000;
const TOKEN_REFRESH_MARGIN_SECONDS = 60;

const idle: NowPlayingMessage = { playing: null };

const toTrack = ({ artist, source, title, url }: Track): Track => ({
  artist,
  source,
  title,
  url,
});

export class NowPlaying extends DurableObject<Env> {
  private developerToken: { expiresAt: number; token: string } | null = null;

  override async fetch(request: Request): Promise<Response> {
    if (request.headers.get("upgrade") !== "websocket") {
      return Response.json(await this.message());
    }
    const { 0: client, 1: server } = new WebSocketPair();
    this.ctx.acceptWebSocket(server);
    server.send(JSON.stringify(await this.message()));
    if ((await this.ctx.storage.getAlarm()) === null) {
      await this.ctx.storage.setAlarm(Date.now());
    }
    return new Response(null, { status: 101, webSocket: client });
  }

  /** Runs every minute from the cron, so MusicKit song changes are dated even with nobody watching. */
  async refresh(): Promise<void> {
    await this.poll();
  }

  override async alarm(): Promise<void> {
    await this.poll();
    if (this.ctx.getWebSockets().length > 0) {
      await this.ctx.storage.setAlarm(Date.now() + LIVE_POLL_MS);
    }
  }

  private async message(): Promise<NowPlayingMessage> {
    return (await this.ctx.storage.get<NowPlayingMessage>(MESSAGE_KEY)) ?? idle;
  }

  private async poll(): Promise<void> {
    // In order of trust: the first source with a track is the one reported.
    const answers = await Promise.all(
      (
        [
          ["orbis", this.orbis()],
          ["lastfm", this.lastfm()],
          ["apple-music", this.appleMusic()],
        ] as const
      ).map(async ([source, answer]) => ({ answer: await answer, source }))
    );
    let playing: Track | null = null;
    for (const { answer, source } of answers) {
      if (answer._tag === "Unavailable") {
        // One line per failed poll. A source that stays down, such as an
        // expired Music User Token, then shows as a steady rate in the logs.
        console.warn({
          message: "A now-playing source is unavailable.",
          reason: answer.reason,
          source,
        });
      } else if (answer._tag === "Found") {
        playing ??= answer.value;
      }
    }
    const message: NowPlayingMessage = { playing };
    const next = JSON.stringify(message);
    if (next === JSON.stringify(await this.message())) {
      return;
    }
    await this.ctx.storage.put(MESSAGE_KEY, message);
    for (const socket of this.ctx.getWebSockets()) {
      try {
        socket.send(next);
      } catch {
        // A socket closing mid-broadcast is dropped by the runtime.
      }
    }
  }

  private orbis(): Promise<Answer<Track>> {
    const { ORBIS_API_KEY, ORBIS_API_URL, ORBIS_USERNAME } = this.env;
    if (!(ORBIS_API_KEY && ORBIS_API_URL && ORBIS_USERNAME)) {
      return Promise.resolve(nothing);
    }
    return readOrbis({
      apiKey: ORBIS_API_KEY,
      apiUrl: ORBIS_API_URL,
      username: ORBIS_USERNAME,
    });
  }

  private lastfm(): Promise<Answer<Track>> {
    const { LASTFM_API_KEY, LASTFM_USER } = this.env;
    if (!(LASTFM_API_KEY && LASTFM_USER)) {
      return Promise.resolve(nothing);
    }
    return readLastfm({ apiKey: LASTFM_API_KEY, user: LASTFM_USER });
  }

  /** The song at the top of Apple Music's history, when `observe` counts it as playing. */
  private async appleMusic(): Promise<Answer<Track>> {
    const {
      APPLE_MUSIC_KEY_ID: keyId,
      APPLE_MUSIC_PRIVATE_KEY: privateKey,
      APPLE_MUSIC_TEAM_ID: teamId,
      APPLE_MUSIC_USER_TOKEN: userToken,
    } = this.env;
    if (!(keyId && privateKey && teamId && userToken)) {
      return nothing;
    }
    const nowSeconds = Math.floor(Date.now() / 1000);
    if (
      !this.developerToken ||
      this.developerToken.expiresAt - TOKEN_REFRESH_MARGIN_SECONDS < nowSeconds
    ) {
      const signed = await attempt(async () =>
        found(
          await signDeveloperToken({ keyId, privateKey, teamId }, nowSeconds)
        )
      );
      if (signed._tag !== "Found") {
        // A key that is not valid PEM lands here. The other sources still answer.
        return signed._tag === "Unavailable"
          ? unavailable(
              `APPLE_MUSIC_PRIVATE_KEY cannot sign a developer token. ${signed.reason}`
            )
          : signed;
      }
      this.developerToken = signed.value;
    }
    const recent = await readAppleMusicRecent(
      this.developerToken.token,
      userToken
    );
    if (recent._tag !== "Found") {
      return recent;
    }
    const song = recent.value;
    const known = fromStored(
      await this.ctx.storage.get<StoredTopSong>(LATEST_SONG_KEY)
    );
    const next = observe(known, song, Date.now());
    if (next.known !== known) {
      await this.ctx.storage.put(LATEST_SONG_KEY, toStored(next.known));
    }
    return next.playing ? found(toTrack(song)) : nothing;
  }
}

export default {
  fetch(request, env) {
    const origin = request.headers.get("origin");
    if (origin !== null && !env.ALLOWED_ORIGINS.split(",").includes(origin)) {
      return new Response("Forbidden", { status: 403 });
    }
    return env.NOW_PLAYING.getByName(ROOM).fetch(request);
  },
  scheduled(_controller, env, ctx) {
    ctx.waitUntil(env.NOW_PLAYING.getByName(ROOM).refresh());
  },
} satisfies ExportedHandler<Env>;
