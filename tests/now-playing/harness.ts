import { mock, setSystemTime } from "bun:test";

import type { NowPlayingMessage } from "../../workers/now-playing/message";
import { parseNowPlayingMessage } from "../../workers/now-playing/message";
import type { Env } from "../../workers/now-playing/worker";

type Variables = Omit<Env, "NOW_PLAYING">;

/** The two values the object keeps: its last message and the last Apple Music song. */
type Stored = NowPlayingMessage | { id: string; since: number };

/** The part of a Durable Object's state the object uses. */
interface State {
  getWebSockets: () => WebSocket[];
  storage: {
    get: (key: string) => Promise<Stored | undefined>;
    getAlarm: () => Promise<number | null>;
    put: (key: string, value: Stored) => Promise<void>;
    setAlarm: (time: number) => Promise<void>;
  };
}

/** `cloudflare:workers` exists only inside workerd; the class keeps `ctx` and `env`. */
mock.module("cloudflare:workers", () => ({
  DurableObject: class {
    ctx: State;
    env: Variables;
    constructor(ctx: State, env: Variables) {
      this.ctx = ctx;
      this.env = env;
    }
  },
}));

const { NowPlaying } = await import("../../workers/now-playing/worker");

/** What one upstream API answers for a poll. */
export type Upstream = Response | Error;

export interface Upstreams {
  appleMusic?: () => Upstream;
  lastfm?: () => Upstream;
  orbis?: () => Upstream;
}

const HOSTS = {
  appleMusic: "api.music.apple.com",
  lastfm: "ws.audioscrobbler.com",
  orbis: "orbis.test",
} as const;

const empty: Variables = {
  ALLOWED_ORIGINS: "",
  APPLE_MUSIC_KEY_ID: "",
  APPLE_MUSIC_PRIVATE_KEY: "",
  APPLE_MUSIC_TEAM_ID: "",
  APPLE_MUSIC_USER_TOKEN: "",
  LASTFM_API_KEY: "",
  LASTFM_USER: "",
  ORBIS_API_KEY: "",
  ORBIS_API_URL: "",
  ORBIS_USERNAME: "",
};

export const lastfmEnv = { LASTFM_API_KEY: "key", LASTFM_USER: "parth" };
export const orbisEnv = {
  ORBIS_API_KEY: "key",
  ORBIS_API_URL: `https://${HOSTS.orbis}/api`,
  ORBIS_USERNAME: "parth",
};

/** A P-256 key in the PEM form of a MusicKit `.p8` file. */
export const appleMusicEnv = async () => {
  const pair = await crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["sign"]
  );
  const pkcs8 = await crypto.subtle.exportKey("pkcs8", pair.privateKey);
  const body = Buffer.from(pkcs8).toString("base64");
  return {
    APPLE_MUSIC_KEY_ID: "KEYID",
    APPLE_MUSIC_PRIVATE_KEY: `-----BEGIN PRIVATE KEY-----\n${body}\n-----END PRIVATE KEY-----`,
    APPLE_MUSIC_TEAM_ID: "TEAMID",
    APPLE_MUSIC_USER_TOKEN: "user-token",
  };
};

/** One Durable Object with in-memory storage, a clock, and scripted upstreams. */
export const nowPlaying = (env: Partial<Variables>, upstreams: Upstreams) => {
  const stored = new Map<string, Stored>();
  const ctx: State = {
    getWebSockets: () => [],
    storage: {
      get: (key) => Promise.resolve(stored.get(key)),
      getAlarm: () => Promise.resolve(null),
      put: (key, value) => {
        stored.set(key, structuredClone(value));
        return Promise.resolve();
      },
      setAlarm: () => Promise.resolve(),
    },
  };
  const answer = (url: URL): Upstream => {
    const source = (["appleMusic", "lastfm", "orbis"] as const).find(
      (name) => HOSTS[name] === url.hostname
    );
    const upstream = source && upstreams[source];
    return upstream ? upstream() : new Error(`Unexpected request to ${url}`);
  };
  const fetched = mock((input: Request | string | URL) => {
    const response = answer(
      new URL(input instanceof Request ? input.url : input)
    );
    return response instanceof Error
      ? Promise.reject(response)
      : Promise.resolve(response);
  });
  Object.assign(globalThis, { fetch: fetched });

  // SAFETY: the object reads only the state faked above, and no namespace.
  const object = new NowPlaying(ctx as never, { ...empty, ...env } as never);
  return {
    /** Sets the clock, runs the cron's refresh, and returns what a reader then gets. */
    pollAt: async (time: number): Promise<NowPlayingMessage> => {
      setSystemTime(new Date(time));
      await object.refresh();
      const response = await object.fetch(new Request("https://now.test/"));
      const message = parseNowPlayingMessage(await response.text());
      if (message === null) {
        throw new Error("The object answered with something a reader rejects.");
      }
      return message;
    },
  };
};

export const lastfmPlaying = (name: string, artist: string) => () =>
  Response.json({
    recenttracks: {
      track: [
        {
          "@attr": { nowplaying: "true" },
          artist: { "#text": artist },
          name,
          url: `https://www.last.fm/music/${artist}/_/${name}`,
        },
      ],
    },
  });

export const appleMusicTop = (id: string, durationInMillis: number) => () =>
  Response.json({
    data: [
      {
        attributes: {
          artistName: `Artist ${id}`,
          durationInMillis,
          name: `Song ${id}`,
          url: `https://music.apple.com/song/${id}`,
        },
        id,
      },
    ],
  });

export const failing = (status: number) => () =>
  new Response("upstream failed", { status });

/** The presence frame Orbis sends as soon as its event stream opens. */
export const orbisPresence =
  (person: { set: { creator: string; title: string }; username: string }) =>
  () =>
    new Response(
      `data: ${JSON.stringify({ kind: "presence", presence: [person] })}\n\n`,
      { headers: { "content-type": "text/event-stream" } }
    );
