// Dead-path check for tracked Markdown (PAL-2652): fails when a Markdown file names a repo path
// that does not exist. The code is the source of truth, and a doc that points at a moved or
// deleted file misleads agents and people.
//
// Self-contained: no imports from this repo, so another repo can copy this file unchanged.
// It reads `git ls-files`, so untracked and ignored files never count.
//
// It checks each backticked token and each relative Markdown link target that looks like a repo
// path: it has a "/" and ends in a file extension. Bare directory names are not checked. Tokens
// with a glob, a placeholder (`<`, `{`, `$`), an `@` package or SSH address, a URL, a home path
// (`~`) or an absolute path are not repo paths and are skipped. A path resolves from the repo
// root, or from the folder of the file that names it. Edit the lists below for the repo.
//
//   bun tools/check-doc-paths.ts
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";

/** Markdown files under these prefixes are not checked: dated records. */
const skipPrefixes = ["docs/adr/"];
/** Dated records that name paths that were true on their date. Name each file. */
const skipFiles = new Set<string>();

const git = (...args: string[]): string =>
  execFileSync("git", args, {
    encoding: "utf-8",
    maxBuffer: 256 * 1024 * 1024,
  });

const root = git("rev-parse", "--show-toplevel").trim();
const tracked = new Set(git("ls-files", "-z").split("\0").filter(Boolean));

const extension = /\.[A-Za-z0-9]{1,8}$/u;
const notAPath = /[*<>{}$~|@\s]|^(?:[a-z]+:)?\/\/|^\/|^#|^[a-z]+:/u;

const candidate = (raw: string): string | undefined => {
  const token = raw
    .trim()
    .replace(/[#?].*$/u, "")
    .replace(/:\d+(?:[-:]\d+)*$/u, "")
    .replace(/^\.\//u, "");
  if (token === "" || notAPath.test(token) || !token.includes("/")) {
    return undefined;
  }
  return extension.test(token) && !token.includes("node_modules/")
    ? token
    : undefined;
};

const exists = (token: string, from: string): boolean =>
  [token, path.posix.join(from, token)].some((p) =>
    tracked.has(path.posix.normalize(p))
  );

const tokensOf = (line: string): string[] => [
  ...[...line.matchAll(/`(?<code>[^`]+)`/gu)].map((m) => m.groups?.code ?? ""),
  ...[...line.matchAll(/\]\((?<href>[^)\s]+)\)/gu)].map(
    (m) => m.groups?.href ?? ""
  ),
];

const problemsIn = (file: string): string[] => {
  const from = path.posix.dirname(file);
  const found: string[] = [];
  let fenced = false;
  const lines = readFileSync(path.join(root, file), "utf-8").split("\n");
  for (const [index, line] of lines.entries()) {
    if (/^\s*(?:```|~~~)/u.test(line)) {
      fenced = !fenced;
    } else if (!fenced) {
      for (const raw of tokensOf(line)) {
        const token = candidate(raw);
        if (token !== undefined && !exists(token, from)) {
          found.push(`${file}:${index + 1}: ${token}`);
        }
      }
    }
  }
  return found;
};

const problems = [...tracked]
  .filter(
    (file) =>
      file.endsWith(".md") &&
      !skipFiles.has(file) &&
      !skipPrefixes.some((prefix) => file.startsWith(prefix))
  )
  .flatMap(problemsIn);

if (problems.length > 0) {
  console.error(
    `${problems.length} Markdown path(s) name a file that does not exist. Fix or delete the doc, or skip a dated record in tools/check-doc-paths.ts:`
  );
  console.error(problems.map((problem) => `  ${problem}`).join("\n"));
  process.exit(1);
}
console.log("check-doc-paths: every path a Markdown file names exists");
