import { afterEach, expect, mock, setSystemTime, spyOn, test } from "bun:test";

import type { Upstream } from "./harness";
import { appleMusicEnv, appleMusicTop, failing, nowPlaying } from "./harness";

const START = Date.UTC(2026, 10, 12, 12);
/** The Worker's allowance for Apple Music listing a song late. */
const GRACE_MS = 60_000;

const SONGS = {
  long: { durationMs: 180_000, id: "long" },
  short: { durationMs: 60_000, id: "short" },
} as const;

type SongName = keyof typeof SONGS;

/** One poll: how long after the one before, and what Apple Music then answers. */
interface Step {
  afterMs: number;
  top: SongName | "down";
}

const GAPS_MS = [30_000, 120_000];
const TOPS = ["long", "short", "down"] as const;
const STEPS: Step[] = GAPS_MS.flatMap((afterMs) =>
  TOPS.map((top) => ({ afterMs, top }))
);

/** Every history of `length` polls. */
const histories = (length: number): Step[][] =>
  length === 0
    ? [[]]
    : histories(length - 1).flatMap((history) =>
        STEPS.map((step) => [...history, step])
      );

/**
 * The rule, stated without the Worker's bookkeeping. Apple Music's history has
 * no play times, so a song is playing when the object saw it take the top of
 * the list, no longer ago than the song's length plus the grace. The first
 * song the object ever sees took the top at an unknown time, so it never is.
 */
const expectedTitles = (history: Step[]): (string | null)[] => {
  const seen: { at: number; top: SongName }[] = [];
  let now = START;
  return history.map((step) => {
    now += step.afterMs;
    if (step.top === "down") {
      return null;
    }
    seen.push({ at: now, top: step.top });
    const run = seen.findLastIndex((poll) => poll.top !== step.top) + 1;
    const tookTheTopAt = seen[run]?.at ?? now;
    const song = SONGS[step.top];
    return run > 0 && now - tookTheTopAt <= song.durationMs + GRACE_MS
      ? `Song ${song.id}`
      : null;
  });
};

afterEach(() => {
  setSystemTime();
  mock.restore();
});

test("Apple Music: a song plays from when it takes the top of the history, for its length plus the grace", async () => {
  spyOn(console, "warn").mockImplementation(() => null);
  const env = await appleMusicEnv();

  for (const history of histories(4)) {
    let answer: () => Upstream = failing(500);
    const object = nowPlaying(env, { appleMusic: () => answer() });
    const titles: (string | null)[] = [];
    let now = START;
    for (const step of history) {
      now += step.afterMs;
      answer =
        step.top === "down"
          ? failing(500)
          : appleMusicTop(SONGS[step.top].id, SONGS[step.top].durationMs);
      // oxlint-disable-next-line no-await-in-loop -- each poll follows the one before it
      const message = await object.pollAt(now);
      titles.push(message.playing?.title ?? null);
    }

    expect({ history, titles }).toEqual({
      history,
      titles: expectedTitles(history),
    });
  }
});
