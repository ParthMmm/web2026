import { expect, test } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";

import { builtSite } from "./built-site";

const ONE_YEAR_SECONDS = 31_536_000;
const HASHED_NAME = /\.[\w-]{5,}\.\w+$/u;

/** The headers a Cloudflare `_headers` file sets for one path pattern. */
const headersFor = (file: string, pattern: string): Map<string, string> => {
  const headers = new Map<string, string>();
  if (!existsSync(file)) {
    return headers;
  }
  let inRule = false;
  for (const line of readFileSync(file, "utf-8").split("\n")) {
    if (line.trim() === "" || line.trim().startsWith("#")) {
      continue;
    }
    if (!line.startsWith(" ")) {
      inRule = line.trim() === pattern;
    } else if (inRule) {
      const [name = "", ...value] = line.trim().split(":");
      headers.set(name.toLowerCase(), value.join(":").trim());
    }
  }
  return headers;
};

test("the host caches content-hashed files for a year without revalidating", () => {
  const { dir } = builtSite();

  const cacheControl =
    headersFor(path.join(dir, "_headers"), "/_astro/*").get("cache-control") ??
    "";
  const maxAge = Number(
    cacheControl.match(/max-age=(?<seconds>\d+)/u)?.groups?.seconds
  );

  expect(cacheControl).toContain("immutable");
  expect(maxAge).toBeGreaterThanOrEqual(ONE_YEAR_SECONDS);
});

test("every file cached forever has its content hash in its name", () => {
  const unhashed = [...builtSite().files].filter(
    (url) => url.startsWith("/_astro/") && !HASHED_NAME.test(url)
  );

  expect(unhashed).toEqual([]);
});
