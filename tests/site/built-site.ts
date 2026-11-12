import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

const ROOT = path.resolve(import.meta.dir, "../..");
const CACHE = path.join(ROOT, "node_modules/.cache");

/** The fixture gallery the browser smoke check serves (tests/fixtures/gallery). */
export const FIXTURE_GALLERY_URL = "http://127.0.0.1:8787/gallery";

const INLINE_STYLE = /<style[^>]*>(?<css>[\s\S]*?)<\/style>/gu;

export interface Page {
  /** The URL path the host serves this file at, such as "/projects/mote/". */
  url: string;
  html: string;
}

export interface BuiltSite {
  dir: string;
  /** The URL path of every file the host can serve as it is. */
  files: Set<string>;
  pages: Page[];
  /** Every stylesheet file, and every inline `<style>` block, by where it is. */
  stylesheets: Map<string, string>;
}

const walk = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(dir, entry.name);
    return entry.isDirectory() ? walk(file) : [file];
  });

const toUrl = (out: string, file: string): string => {
  const relative = `/${path.relative(out, file).split(path.sep).join("/")}`;
  return relative.replace(/index\.html$/u, "");
};

const build = (galleryUrl: string): BuiltSite => {
  const OUT = path.join(
    CACHE,
    galleryUrl === "" ? "site-test" : "site-test-gallery"
  );
  const result = Bun.spawnSync(["bunx", "astro", "build", "--outDir", OUT], {
    cwd: ROOT,
    env: {
      ...process.env,
      // `bun test` sets NODE_ENV=test, and Astro then builds as if for dev.
      NODE_ENV: "production",
      // Set even when empty, so a developer's own .env cannot change the build.
      PUBLIC_GALLERY_URL: galleryUrl,
    },
    stderr: "pipe",
    stdout: "pipe",
  });
  if (result.exitCode !== 0) {
    throw new Error(`astro build failed:\n${result.stderr.toString()}`);
  }
  const files = walk(OUT);
  const toUrlIn = (file: string) => toUrl(OUT, file);
  const pages = files
    .filter((file) => file.endsWith(".html"))
    .map((file) => ({ html: readFileSync(file, "utf-8"), url: toUrlIn(file) }));
  return {
    dir: OUT,
    files: new Set(files.map(toUrlIn)),
    pages,
    stylesheets: new Map([
      ...files
        .filter((file) => file.endsWith(".css"))
        .map((file): [string, string] => [
          toUrlIn(file),
          readFileSync(file, "utf-8"),
        ]),
      ...pages.flatMap((page) =>
        [...page.html.matchAll(INLINE_STYLE)].map(
          (match, index): [string, string] => [
            `${page.url}#style-${index}`,
            match.groups?.css ?? "",
          ]
        )
      ),
    ]),
  };
};

const built = new Map<string, BuiltSite>();

/** The production build, made once per test run for each gallery address.
    With no address the build is the one production gets today: the gallery
    is not connected. */
export const builtSite = (galleryUrl = ""): BuiltSite => {
  const site = built.get(galleryUrl) ?? build(galleryUrl);
  built.set(galleryUrl, site);
  return site;
};
