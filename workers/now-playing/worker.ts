import { DurableObject } from "cloudflare:workers";

import { signDeveloperToken } from "./apple-music-token";
import type { NowPlayingMessage, Track } from "./message";
import { readAppleMusicRecent, readLastfm, readOrbis } from "./sources";

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

interface LatestSong {
  id: string;
  since: number;
}

const ROOM = "parth";
const MESSAGE_KEY = "message";
const LATEST_SONG_KEY = "appleMusicLatest";
const LIVE_POLL_MS = 15_000;
/** Apple Music can list a song a little after it starts playing. */
const APPLE_MUSIC_GRACE_MS = 60_000;
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
    const [orbis, lastfm, appleMusic] = await Promise.all([
      this.orbis(),
      this.lastfm(),
      this.appleMusic(),
    ]);
    const message: NowPlayingMessage = {
      playing: orbis ?? lastfm ?? appleMusic,
    };
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

  private orbis(): Promise<Track | null> {
    const { ORBIS_API_KEY, ORBIS_API_URL, ORBIS_USERNAME } = this.env;
    if (!(ORBIS_API_KEY && ORBIS_API_URL && ORBIS_USERNAME)) {
      return Promise.resolve(null);
    }
    return readOrbis({
      apiKey: ORBIS_API_KEY,
      apiUrl: ORBIS_API_URL,
      username: ORBIS_USERNAME,
    });
  }

  private lastfm(): Promise<Track | null> {
    const { LASTFM_API_KEY, LASTFM_USER } = this.env;
    if (!(LASTFM_API_KEY && LASTFM_USER)) {
      return Promise.resolve(null);
    }
    return readLastfm({ apiKey: LASTFM_API_KEY, user: LASTFM_USER });
  }

  /**
   * Apple Music's history has no play times, so a song counts as playing for its
   * own length after it first reaches the top of the list.
   */
  private async appleMusic(): Promise<Track | null> {
    const {
      APPLE_MUSIC_KEY_ID: keyId,
      APPLE_MUSIC_PRIVATE_KEY: privateKey,
      APPLE_MUSIC_TEAM_ID: teamId,
      APPLE_MUSIC_USER_TOKEN: userToken,
    } = this.env;
    if (!(keyId && privateKey && teamId && userToken)) {
      return null;
    }
    const nowSeconds = Math.floor(Date.now() / 1000);
    if (
      !this.developerToken ||
      this.developerToken.expiresAt - TOKEN_REFRESH_MARGIN_SECONDS < nowSeconds
    ) {
      this.developerToken = await signDeveloperToken(
        { keyId, privateKey, teamId },
        nowSeconds
      );
    }
    const song = await readAppleMusicRecent(
      this.developerToken.token,
      userToken
    );
    if (!song) {
      return null;
    }
    const now = Date.now();
    const latest = await this.ctx.storage.get<LatestSong>(LATEST_SONG_KEY);
    if (latest?.id !== song.id) {
      // The first song this object ever sees may be from yesterday, so it starts undated.
      const since = latest === undefined ? 0 : now;
      await this.ctx.storage.put(LATEST_SONG_KEY, { id: song.id, since });
      return since === 0 ? null : toTrack(song);
    }
    return now - latest.since > song.durationMs + APPLE_MUSIC_GRACE_MS
      ? null
      : toTrack(song);
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
