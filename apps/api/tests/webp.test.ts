import { readFileSync } from "node:fs";

import { expect, it } from "vitest";

import { parseWebpHeader, WEBP_PREFIX_BYTES } from "../src/photos/internal/webp.ts";

const readFixture = (name: string) =>
  readFileSync(new URL(`fixtures/webp/${name}.webp`, import.meta.url));

const parseFixture = (name: string) => {
  const bytes = readFixture(name);
  return parseWebpHeader(bytes.subarray(0, WEBP_PREFIX_BYTES), bytes.length);
};

const still = { animated: false, hasMetadata: false };

it.each([
  ["lossy", { ...still, height: 320, width: 480 }],
  ["lossless", { ...still, height: 320, width: 480 }],
  ["alpha", { ...still, height: 320, width: 480 }],
  ["odd", { ...still, height: 321, width: 481 }],
  ["big", { ...still, height: 16, width: 16_383 }],
  ["anim", { animated: true, hasMetadata: false, height: 320, width: 480 }],
  ["exif", { ...still, hasMetadata: true, height: 320, width: 480 }],
  ["icc", { ...still, hasMetadata: true, height: 320, width: 480 }],
  ["xmp", { ...still, hasMetadata: true, height: 320, width: 480 }],
])("reads the cwebp file %s as %o", (name, expected) => {
  expect(parseFixture(name)).toEqual({ _tag: "Webp", ...expected });
});

it.each(["trunc_fixedriff", "vp8l_badver"])("refuses the malformed file %s", (name) => {
  expect(parseFixture(name)).toEqual({ _tag: "NotWebp" });
});

it("refuses a valid file whose stored size differs from its RIFF size", () => {
  const bytes = readFixture("lossy");
  expect(parseWebpHeader(bytes.subarray(0, WEBP_PREFIX_BYTES), bytes.length + 2)).toEqual({
    _tag: "NotWebp",
  });
});
