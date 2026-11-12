import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";

const ROOT = path.resolve(import.meta.dir, "../..");
const OUT = path.join(ROOT, "node_modules/.cache/site-test");

const INLINE_STYLE = /<style[^>]*>(?<css>[\s\S]*?)<\/style>/gu;

export interface Page {
  /** The URL path the host serves this file at, such as "/projects/mote/". */
  url: string;
  html: string;
}

export interface BuiltSite {
  dir: string;
  pages: Page[];
  /** Every stylesheet file, and every inline `<style>` block, by where it is. */
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
  const pages = files
    .filter((file) => file.endsWith(".html"))
    .map((file) => ({ html: readFileSync(file, "utf-8"), url: toUrl(file) }));
  return {
    dir: OUT,
    pages,
    stylesheets: new Map([
      ...files
        .filter((file) => file.endsWith(".css"))
        .map((file): [string, string] => [
          toUrl(file),
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

let built: BuiltSite | undefined;

/** The production build, made once per test run. */
export const builtSite = (): BuiltSite => {
  built ??= build();
  return built;
};
