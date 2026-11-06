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
  ],
});
