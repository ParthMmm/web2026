import type { Track } from "./message";

const ORBIS_TIMEOUT_MS = 5000;
const LASTFM_URL = "https://ws.audioscrobbler.com/2.0/";
const APPLE_MUSIC_RECENT_URL =
  "https://api.music.apple.com/v1/me/recent/played/tracks?limit=1&types=songs";

export interface OrbisConfig {
  apiKey: string;
  apiUrl: string;
  username: string;
}

export interface LastfmConfig {
  apiKey: string;
  user: string;
}

export interface RecentSong extends Track {
  durationMs: number;
  id: string;
}

interface OrbisSet {
  creator?: string | null;
  title?: string;
  url?: string | null;
}

interface OrbisPresence {
  set?: OrbisSet;
  username?: string;
}

interface OrbisPresenceEvent {
  kind?: string;
  presence?: OrbisPresence[];
}

interface LastfmArtist {
  "#text"?: string;
}

interface LastfmTrack {
  "@attr"?: { nowplaying?: string };
  artist?: LastfmArtist;
  name?: string;
  url?: string;
}

interface LastfmRecentBody {
  recenttracks?: {
    track?: LastfmTrack | LastfmTrack[];
  };
}

interface AppleMusicSong {
  attributes?: {
    artistName?: string;
    durationInMillis?: number;
    name?: string;
    url?: string;
  };
  id?: string;
}

interface AppleMusicRecentBody {
  data?: AppleMusicSong[];
}

/**
 * What one source said. `Nothing` is an answer: nothing is playing there.
 * `Unavailable` is the lack of one: the source failed, so the caller can say
 * why instead of reporting silence.
 */
export type Answer<Value> =
  | { _tag: "Found"; value: Value }
  | { _tag: "Nothing" }
  | { _tag: "Unavailable"; reason: string };

export const nothing = { _tag: "Nothing" } as const;

export const unavailable = (reason: string) =>
  ({ _tag: "Unavailable", reason }) as const;

export const found = <Value>(value: Value) =>
  ({ _tag: "Found", value }) as const;

/** Runs one read. A throw becomes `Unavailable`, with the error as the reason. */
export const attempt = async <Value>(
  read: () => Promise<Answer<Value>>
): Promise<Answer<Value>> => {
  try {
    return await read();
  } catch (error) {
    return unavailable(
      error instanceof Error ? `${error.name}: ${error.message}` : String(error)
    );
  }
};

const nonempty = (value: string | null | undefined): string | null =>
  value === undefined || value === "" ? null : value;

const presenceTrack = (
  frame: OrbisPresenceEvent,
  username: string
): Answer<Track> => {
  const mine = frame.presence?.find((person) => person.username === username);
  const title = nonempty(mine?.set?.title);
  return title
    ? found<Track>({
        artist: nonempty(mine?.set?.creator),
        source: "orbis",
        title,
        url: nonempty(mine?.set?.url),
      })
    : nothing;
};

const dataOf = (frame: string): string =>
  frame
    .split("\n")
    .filter((line) => line.startsWith("data:"))
    .map((line) => line.slice("data:".length).trimStart())
    .join("\n");

/**
 * Orbis only reports Presence on its event stream, and never a Person's own, so
 * this reads as a separate Person who can see `username`. The server sends a
 * presence frame as soon as the stream opens; the first one answers the question.
 */
export const readOrbis = async (
  config: OrbisConfig
): Promise<Answer<Track>> => {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), ORBIS_TIMEOUT_MS);
  const answer = await attempt(async () => {
    const response = await fetch(`${config.apiUrl}/events`, {
      headers: {
        accept: "text/event-stream",
        authorization: `Bearer ${config.apiKey}`,
      },
      signal: controller.signal,
    });
    if (!(response.ok && response.body)) {
      return unavailable(`HTTP ${response.status}`);
    }
    const reader = response.body
      .pipeThrough(new TextDecoderStream())
      .getReader();
    let pending = "";
    for (;;) {
      // oxlint-disable-next-line no-await-in-loop -- SSE frames arrive in sequence.
      const chunk = await reader.read();
      if (chunk.done) {
        return unavailable("the stream ended before a presence frame");
      }
      pending += chunk.value;
      let boundary = pending.indexOf("\n\n");
      while (boundary !== -1) {
        const data = dataOf(pending.slice(0, boundary));
        pending = pending.slice(boundary + 2);
        if (data !== "") {
          // SAFETY: Orbis SSE data is a QueueEvent; we only read presence frames.
          const frame = JSON.parse(data) as OrbisPresenceEvent;
          if (frame.kind === "presence") {
            return presenceTrack(frame, config.username);
          }
        }
        boundary = pending.indexOf("\n\n");
      }
    }
  });
  const timedOut = controller.signal.aborted;
  clearTimeout(timeout);
  controller.abort();
  return answer._tag === "Unavailable" && timedOut
    ? unavailable(`no presence frame within ${ORBIS_TIMEOUT_MS} ms`)
    : answer;
};

/** NepTunes scrobbles the laptop's Music app, and Last.fm flags the live track. */
export const readLastfm = async (
  config: LastfmConfig
): Promise<Answer<Track>> => {
  const url = new URL(LASTFM_URL);
  url.search = new URLSearchParams({
    api_key: config.apiKey,
    format: "json",
    limit: "1",
    method: "user.getrecenttracks",
    user: config.user,
  }).toString();
  const answer = await attempt(async () => {
    const response = await fetch(url);
    if (!response.ok) {
      return unavailable(`HTTP ${response.status}`);
    }
    // SAFETY: Last.fm's recenttracks payload; we read only the live flag and names.
    const body = (await response.json()) as LastfmRecentBody;
    const tracks = body.recenttracks?.track;
    const [track] = Array.isArray(tracks) ? tracks : [tracks];
    const title = nonempty(track?.name);
    if (!(title && track?.["@attr"]?.nowplaying === "true")) {
      return nothing;
    }
    return found<Track>({
      artist: nonempty(track.artist?.["#text"]),
      source: "apple-music",
      title,
      url: nonempty(track.url),
    });
  });
  // The request URL carries the API key, and an error may quote the URL.
  return answer._tag === "Unavailable"
    ? unavailable(answer.reason.replaceAll(config.apiKey, "[api key]"))
    : answer;
};

/**
 * The most recent song in Apple Music's history, from any device. The API gives
 * no play time, so whether it is still playing is the caller's call.
 */
export const readAppleMusicRecent = (
  developerToken: string,
  userToken: string
): Promise<Answer<RecentSong>> =>
  attempt(async () => {
    const response = await fetch(APPLE_MUSIC_RECENT_URL, {
      headers: {
        authorization: `Bearer ${developerToken}`,
        "music-user-token": userToken,
      },
    });
    if (!response.ok) {
      return unavailable(`HTTP ${response.status}`);
    }
    // SAFETY: MusicKit recent tracks; we read id, name, artist, duration, and url.
    const body = (await response.json()) as AppleMusicRecentBody;
    const [song] = body.data ?? [];
    const id = nonempty(song?.id);
    const title = nonempty(song?.attributes?.name);
    const durationMs = song?.attributes?.durationInMillis;
    if (!(id && title && durationMs !== undefined && durationMs > 0)) {
      return nothing;
    }
    return found<RecentSong>({
      artist: nonempty(song.attributes?.artistName),
      durationMs,
      id,
      source: "apple-music",
      title,
      url: nonempty(song.attributes?.url),
    });
  });
