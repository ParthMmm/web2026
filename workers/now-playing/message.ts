/** What the masthead shows. Last.fm and MusicKit both report Apple Music listens. */
export interface Track {
  artist: string | null;
  source: "apple-music" | "orbis";
  title: string;
  url: string | null;
}

/** The one message the socket sends: on connect, then whenever it changes. */
export interface NowPlayingMessage {
  playing: Track | null;
}

const isTrack = (value: Track | null | undefined): value is Track =>
  value !== null &&
  value !== undefined &&
  (value.source === "apple-music" || value.source === "orbis") &&
  typeof value.title === "string" &&
  value.title !== "" &&
  (value.artist === null || typeof value.artist === "string") &&
  (value.url === null || typeof value.url === "string");

/** The Worker only sends this shape; a bad frame leaves the masthead on the idle line. */
export const parseNowPlayingMessage = (
  raw: string
): NowPlayingMessage | null => {
  try {
    // SAFETY: the Worker serializes NowPlayingMessage; fields are checked below.
    const message = JSON.parse(raw) as NowPlayingMessage;
    if (message.playing === null) {
      return { playing: null };
    }
    return isTrack(message.playing) ? { playing: message.playing } : null;
  } catch {
    return null;
  }
};
