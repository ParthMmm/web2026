import { readdirSync, readFileSync } from "node:fs";

import cloudflare from "@alchemy.run/cloudflare-runtime/rolldown";
import { makeEffectVirtualEntry } from "@alchemy.run/cloudflare/Workers/Sources/Rolldown";
import { splitSqliteStatements } from "@alchemy.run/sql/SqlFile";
import { Miniflare } from "miniflare";
import { rolldown } from "rolldown";
import type { Plugin } from "rolldown";

export const COMPATIBILITY_DATE = "2026-08-01";
export const COMPATIBILITY_FLAGS = ["nodejs_compat"];

const ROOT = new URL("../", import.meta.url).pathname;
const MIGRATIONS = new URL("../migrations/", import.meta.url);
const VIRTUAL_ENTRY = "\0workerd-entry";

const virtualEntry = (source: string): Plugin => ({
  load: (id) => (id === VIRTUAL_ENTRY ? source : null),
  name: "workerd-entry",
  resolveId: (id) => (id === VIRTUAL_ENTRY ? id : null),
});

export const bundle = async (entry: { readonly path: string } | { readonly source: string }) => {
  const build = await rolldown({
    checks: { ineffectiveDynamicImport: false, unresolvedImport: false },
    cwd: ROOT,
    experimental: { lazyBarrel: true },
    external: ["lightningcss", "fsevents"],
    input: "path" in entry ? entry.path : VIRTUAL_ENTRY,
    moduleTypes: { ".sql": "text" },
    plugins: [
      cloudflare({
        compatibilityDate: COMPATIBILITY_DATE,
        compatibilityFlags: COMPATIBILITY_FLAGS,
      }),
      "source" in entry ? virtualEntry(entry.source) : null,
    ],
    transform: { define: { "globalThis.__ALCHEMY_RUNTIME__": "true" } },
  });
  const { output } = await build.generate({ codeSplitting: false, format: "esm" });
  await build.close();
  return output.flatMap((chunk) =>
    chunk.type === "chunk"
      ? [{ contents: chunk.code, path: `${ROOT}${chunk.fileName}`, type: "ESModule" as const }]
      : [],
  );
};

export const bundleWorker = () =>
  bundle({
    source: makeEffectVirtualEntry(
      {},
      { name: "web2026-photos", stage: "test" },
    )(`${ROOT}src/worker.ts`),
  });

export const startWorkerd = async (
  modules: Awaited<ReturnType<typeof bundle>>,
  bindings: Record<string, string> = {},
  port?: number,
) => {
  const miniflare = new Miniflare({
    bindings,
    compatibilityDate: COMPATIBILITY_DATE,
    host: "127.0.0.1",
    port,
    compatibilityFlags: COMPATIBILITY_FLAGS,
    d1Databases: ["Db"],
    modules,
    modulesRoot: ROOT,
    r2Buckets: ["Private"],
  });
  const db = await miniflare.getD1Database("Db");
  for (const file of readdirSync(MIGRATIONS).sort()) {
    for (const statement of splitSqliteStatements(
      readFileSync(new URL(file, MIGRATIONS), "utf8"),
    )) {
      await db.prepare(statement).run();
    }
  }
  return miniflare;
};
