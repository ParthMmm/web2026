import { afterEach, expect, mock, setSystemTime, spyOn, test } from "bun:test";

import {
  failing,
  lastfmEnv,
  lastfmPlaying,
  nowPlaying,
  orbisEnv,
  orbisPresence,
} from "./harness";

const NOON = Date.UTC(2026, 10, 12, 12);

afterEach(() => {
  setSystemTime();
  mock.restore();
});

test("a malformed Apple Music key does not hide what Last.fm reports", async () => {
  const object = nowPlaying(
    {
      ...lastfmEnv,
      APPLE_MUSIC_KEY_ID: "KEYID",
      APPLE_MUSIC_PRIVATE_KEY: "-----BEGIN PRIVATE KEY-----\\nnot base64",
      APPLE_MUSIC_TEAM_ID: "TEAMID",
      APPLE_MUSIC_USER_TOKEN: "user-token",
    },
    { lastfm: lastfmPlaying("Odyssey", "camoufly") }
  );

  const message = await object.pollAt(NOON);

  expect(message.playing?.title).toBe("Odyssey");
});

test("an upstream that refuses the request is named in the logs, with the reason", async () => {
  const warn = spyOn(console, "warn").mockImplementation(() => null);
  const object = nowPlaying(lastfmEnv, { lastfm: failing(401) });

  const message = await object.pollAt(NOON);

  expect(message.playing).toBeNull();
  expect(warn).toHaveBeenCalledWith(
    expect.objectContaining({ reason: "HTTP 401", source: "lastfm" })
  );
});

test("Orbis is reported ahead of Last.fm when both have a track", async () => {
  const object = nowPlaying(
    { ...lastfmEnv, ...orbisEnv },
    {
      lastfm: lastfmPlaying("Odyssey", "camoufly"),
      orbis: orbisPresence({
        set: { creator: "Skin On Skin", title: "Intercell x Glitch 2026" },
        username: "parth",
      }),
    }
  );

  const message = await object.pollAt(NOON);

  expect(message.playing).toEqual({
    artist: "Skin On Skin",
    source: "orbis",
    title: "Intercell x Glitch 2026",
    url: null,
  });
});
