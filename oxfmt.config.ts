import { defineConfig } from "oxfmt";
import ultracite from "ultracite/oxfmt";

export default defineConfig({
  ...ultracite,
  ignorePatterns: [
    ...(ultracite.ignorePatterns ?? []),
    // shadcn owns src/components/ui; `shadcn add` overwrites local edits there.
    "src/components/ui/**",
    // `bun run openapi` writes this file and `--check` compares it byte for byte.
    "apps/api/openapi.json",
    // Frozen copy of a past openapi.json that oasdiff compares against.
    "apps/api/baseline/**",
    // The Rust desktop app sits outside the web toolchain.
    "apps/desktop/**",
  ],
});
