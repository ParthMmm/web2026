import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

import { expect, it } from "@effect/vitest";

const ROOT = new URL("../", import.meta.url).pathname;
const SELF = path.join("tests", "boundaries.test.ts");

const files = ["src", "tests", "scripts"].flatMap((dir) =>
  readdirSync(path.join(ROOT, dir), { encoding: "utf-8", recursive: true })
    .filter((entry) => entry.endsWith(".ts"))
    .map((entry) => path.join(dir, entry))
    .filter((file) => file !== SELF)
);

const offenders = (allowed: (file: string) => boolean, pattern: string) =>
  files.filter(
    (file) =>
      !allowed(file) &&
      readFileSync(path.join(ROOT, file), "utf-8").includes(pattern)
  );

it("keeps photos/internal reachable only from inside src/photos and the parser's own test", () => {
  const parserTest = path.join("tests", "webp.test.ts");
  const allowed = (file: string) =>
    file.startsWith(path.join("src", "photos") + path.sep) ||
    file === parserTest;
  expect(offenders(allowed, "/internal/")).toEqual([]);
});

it("builds blob keys only in the key scheme, the R2 adapter's list and the port's contract suite", () => {
  const allowed = new Set([
    path.join("src", "photos", "internal", "keys.ts"),
    path.join("src", "photos", "blobs-r2.ts"),
    path.join("tests", "support", "blobs-contract.ts"),
  ]);
  expect(offenders((file) => allowed.has(file), "BlobKey.make(")).toEqual([]);
});

it("keeps R2's error vocabulary inside the R2 adapter", () => {
  const adapter = path.join("src", "photos", "blobs-r2.ts");
  expect(offenders((file) => file === adapter, "R2Error")).toEqual([]);
  expect(offenders((file) => file === adapter, "10037")).toEqual([]);
});

it("never opens a SQL transaction, because the D1 client has none", () => {
  expect(offenders(() => false, "withTransaction")).toEqual([]);
});
