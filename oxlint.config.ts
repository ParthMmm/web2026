import { defineConfig } from "oxlint";
import antiSlop from "ultracite/oxlint/anti-slop";
import astro from "ultracite/oxlint/astro";
import core from "ultracite/oxlint/core";

export default defineConfig({
  extends: [core, astro, antiSlop],
  // shadcn owns src/components/ui; `shadcn add` overwrites local edits there.
  ignorePatterns: [...(core.ignorePatterns ?? []), "src/components/ui/**"],
  overrides: [
    {
      files: [
        "src/components/**/*.astro",
        "src/icons/**/*.astro",
        "src/layouts/**/*.astro",
      ],
      rules: {
        // Astro components are PascalCase by convention, and the name is the
        // import name. Kebab-case stays allowed.
        "unicorn/filename-case": [
          "error",
          { cases: { kebabCase: true, pascalCase: true } },
        ],
      },
    },
    {
      files: [
        "apps/api/src/photos/internal/webp.ts",
        "apps/api/tests/support/fixture.ts",
      ],
      rules: {
        // These files read and write WebP header bit fields, so bitwise
        // operators are the format, not a typo for && or ||.
        "no-bitwise": "off",
      },
    },
    {
      files: ["src/lib/gallery-manifest.ts"],
      rules: {
        // This file is the parser at the boundary these rules ask for: it
        // turns the fetched manifest into typed photos. Effect Schema would
        // add about 24 kB gzipped to a page that ships no Effect otherwise.
        "anti-slop/no-runtime-typeof": "off",
        "anti-slop/no-unknown-parameters": "off",
        "anti-slop/no-unsafe-dictionary-type": "off",
      },
    },
    {
      files: ["apps/api/src/photos/blobs.ts", "apps/api/src/photos/domain.ts"],
      rules: {
        // Effect schemas export a value and its type under one name. The
        // core rule does not know types and values are separate namespaces.
        "no-redeclare": "off",
      },
    },
  ],
});
