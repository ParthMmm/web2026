import { expect, test } from "bun:test";

import { ogImagePath } from "../../src/data/og";

test("a page with no share card fails the build instead of borrowing the home page's", () => {
  expect(() => ogImagePath("/projects/not-written-yet/")).toThrow(
    /No share card for \/projects\/not-written-yet\/.*src\/data\/og\.ts/su
  );
});
