import { expect, test } from "bun:test";

import type {
  Player,
  PlayerEvent,
} from "../../src/components/media/orbis-player";
import { positionOf, step } from "../../src/components/media/orbis-player";

const LENGTH = 3;

const EVENTS: PlayerEvent[] = [
  { _tag: "Toggle" },
  { _tag: "Tick" },
  { _tag: "Seek", at: 0 },
  { _tag: "Seek", at: 2 },
];

/** Every sequence of `count` events. */
const sequences = (count: number): PlayerEvent[][] =>
  count === 0
    ? [[]]
    : sequences(count - 1).flatMap((sequence) =>
        EVENTS.map((event) => [...sequence, event])
      );

test("playback stops when it reaches the end of the set", () => {
  const lastSecond: Player = { _tag: "Playing", position: LENGTH - 1 };

  const ended = step(lastSecond, { _tag: "Tick" }, LENGTH);

  expect(ended.player).toEqual({ _tag: "Ended" });
  expect(ended.say).toBe("Saved at 0:03 for every device");
  expect(step(ended.player, { _tag: "Tick" }, LENGTH).player).toEqual({
    _tag: "Ended",
  });
});

test("pressing play on a finished set starts it again from the beginning", () => {
  const replay = step({ _tag: "Ended" }, { _tag: "Toggle" }, LENGTH);

  expect(replay.player).toEqual({ _tag: "Playing", position: 0 });
  expect(replay.say).toBe("Playing from 0:00");
});

test("whatever the visitor does, the player is never playing at or past the end, and only a tick while playing moves it", () => {
  for (const sequence of sequences(6)) {
    let player: Player = { _tag: "Paused", position: 1 };
    for (const event of sequence) {
      const before = player;
      ({ player } = step(player, event, LENGTH));
      const position = positionOf(player, LENGTH);

      expect(position).toBeGreaterThanOrEqual(0);
      expect(position).toBeLessThanOrEqual(LENGTH);
      if (player._tag === "Playing") {
        expect(position).toBeLessThan(LENGTH);
      }
      if (event._tag === "Tick") {
        expect(position - positionOf(before, LENGTH)).toBe(
          before._tag === "Playing" ? 1 : 0
        );
      }
    }
  }
});
