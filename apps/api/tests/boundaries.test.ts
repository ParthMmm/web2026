import { readdirSync, readFileSync } from "node:fs";
import { join, sep } from "node:path";

import { expect, it } from "@effect/vitest";

const ROOT = new URL("../", import.meta.url).pathname;
const SELF = join("tests", "boundaries.test.ts");

const files = ["src", "tests", "scripts"].flatMap((dir) =>
  readdirSync(join(ROOT, dir), { encoding: "utf8", recursive: true })
    .filter((path) => path.endsWith(".ts"))
    .map((path) => join(dir, path))
    .filter((path) => path !== SELF),
);

const offenders = (allowed: (file: string) => boolean, pattern: string) =>
  files.filter(
    (file) => !allowed(file) && readFileSync(join(ROOT, file), "utf8").includes(pattern),
  );

it("keeps photos/internal reachable only from inside src/photos", () => {
  expect(offenders((file) => file.startsWith(join("src", "photos") + sep), "/internal/")).toEqual(
    [],
  );
});

it("builds blob keys only in the key scheme and the port's contract suite", () => {
  const allowed = new Set([
    join("src", "photos", "internal", "keys.ts"),
    join("tests", "support", "blobs-contract.ts"),
  ]);
  expect(offenders((file) => allowed.has(file), "BlobKey.make(")).toEqual([]);
});

it("never opens a SQL transaction, because the D1 client has none", () => {
  expect(offenders(() => false, "withTransaction")).toEqual([]);
});
