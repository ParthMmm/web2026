/* The Orbis figure's player. A set is paused, playing, or finished, and the
   three are separate states: a finished set is not playing, and has no
   position to resume from. */

import { clock } from "../../lib/clock";

export type Player =
  | { _tag: "Paused"; position: number }
  | { _tag: "Playing"; position: number }
  | { _tag: "Ended" };

export type PlayerEvent =
  /** The play button. */
  | { _tag: "Toggle" }
  /** A cue in the tracklist. */
  | { _tag: "Seek"; at: number }
  /** One second of playback. */
  | { _tag: "Tick" };

export interface Step {
  player: Player;
  /** What the status line says about this step, or `null` to leave it. */
  say: string | null;
}

/** Seconds into the set. A finished set is at its end. */
export const positionOf = (player: Player, length: number): number =>
  player._tag === "Ended" ? length : player.position;

const saved = (position: number): string =>
  `Saved at ${clock(position)} for every device`;

const playFrom = (position: number): Step => ({
  player: { _tag: "Playing", position },
  say: `Playing from ${clock(position)}`,
});

const toggle = (player: Player): Step => {
  if (player._tag === "Playing") {
    return {
      player: { _tag: "Paused", position: player.position },
      say: saved(player.position),
    };
  }
  return player._tag === "Ended"
    ? playFrom(0)
    : {
        player: { _tag: "Playing", position: player.position },
        say: "Playing",
      };
};

const tick = (player: Player, length: number): Step => {
  if (player._tag !== "Playing") {
    return { player, say: null };
  }
  const position = player.position + 1;
  return position >= length
    ? { player: { _tag: "Ended" }, say: saved(length) }
    : { player: { _tag: "Playing", position }, say: null };
};

/** The player after `event`, in a set `length` seconds long. */
export const step = (
  player: Player,
  event: PlayerEvent,
  length: number
): Step => {
  if (event._tag === "Toggle") {
    return toggle(player);
  }
  return event._tag === "Tick" ? tick(player, length) : playFrom(event.at);
};
