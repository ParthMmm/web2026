import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

const ROOT = path.resolve(import.meta.dir, "../..");
const OUT = path.join(ROOT, "node_modules/.cache/site-test");

export interface Page {
  /** The URL path the host serves this file at, such as "/projects/mote/". */
  url: string;
  html: string;
}

export interface BuiltSite {
  dir: string;
  pages: Page[];
  /** Every stylesheet a page links, keyed by URL path. */
  stylesheets: Map<string, string>;
}

const walk = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const file = path.join(dir, entry.name);
    return entry.isDirectory() ? walk(file) : [file];
  });

const toUrl = (file: string): string => {
  const relative = `/${path.relative(OUT, file).split(path.sep).join("/")}`;
  return relative.replace(/index\.html$/u, "");
};

const build = (): BuiltSite => {
  const result = Bun.spawnSync(["bunx", "astro", "build", "--outDir", OUT], {
    cwd: ROOT,
    stderr: "pipe",
    stdout: "pipe",
  });
  if (result.exitCode !== 0) {
    throw new Error(`astro build failed:\n${result.stderr.toString()}`);
  }
  const files = walk(OUT);
  return {
    dir: OUT,
    pages: files
      .filter((file) => file.endsWith(".html"))
      .map((file) => ({ html: readFileSync(file, "utf-8"), url: toUrl(file) })),
    stylesheets: new Map(
      files
        .filter((file) => file.endsWith(".css"))
        .map((file) => [toUrl(file), readFileSync(file, "utf-8")])
    ),
  };
};

let built: BuiltSite | undefined;

/** The production build, made once per test run. */
export const builtSite = (): BuiltSite => {
  built ??= build();
  return built;
};
