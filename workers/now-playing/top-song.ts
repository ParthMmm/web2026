/**
 * Apple Music's history lists songs without play times. So the object dates a
 * song itself, by when it saw the song take the top of the list, and counts it
 * as playing for its own length after that.
 */

/** Apple Music can list a song a little after it starts playing. */
const GRACE_MS = 60_000;

/** What the object knows about the song at the top of the history. */
export type TopSong =
  /** The object has not read the history yet. */
  | { _tag: "NeverSeen" }
  /** The first song the object read. It may be from yesterday, so it has no date and never plays. */
  | { _tag: "Undated"; id: string }
  /** A song the object saw replace another, at `since`. */
  | { _tag: "Dated"; id: string; since: number };

type Seen = Exclude<TopSong, { _tag: "NeverSeen" }>;

export interface Song {
  durationMs: number;
  id: string;
}

/** What one read of the history leaves the object knowing, and whether the song is playing. */
export interface Observation {
  known: Seen;
  playing: boolean;
}

/** Reads `song` at the top of the history at `now`. */
export const observe = (
  known: TopSong,
  song: Song,
  now: number
): Observation => {
  if (known._tag === "NeverSeen") {
    return { known: { _tag: "Undated", id: song.id }, playing: false };
  }
  if (known.id !== song.id) {
    return {
      known: { _tag: "Dated", id: song.id, since: now },
      playing: true,
    };
  }
  return {
    known,
    playing:
      known._tag === "Dated" && now - known.since <= song.durationMs + GRACE_MS,
  };
};

/**
 * The stored form is older than the tags and is kept, so a deploy needs no
 * migration: `since: 0` is `Undated`.
 */
export interface StoredTopSong {
  id: string;
  since: number;
}

export const fromStored = (stored: StoredTopSong | undefined): TopSong => {
  if (stored === undefined) {
    return { _tag: "NeverSeen" };
  }
  return stored.since === 0
    ? { _tag: "Undated", id: stored.id }
    : { _tag: "Dated", id: stored.id, since: stored.since };
};

export const toStored = (known: Seen): StoredTopSong => ({
  id: known.id,
  since: known._tag === "Dated" ? known.since : 0,
});
