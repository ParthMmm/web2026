import { readFileSync } from "node:fs";

import { Schema } from "effect";
import { describe, expect, it } from "vitest";

// A script that runs `alchemy deploy` or `alchemy destroy` with no --stage acts on `live_$USER`.
// Only the `prod` stage retains the photo bucket and database (src/photos/infra.ts), so a deploy
// there would hold real photos in resources that the next destroy deletes.

const Package = Schema.Struct({
  scripts: Schema.Record(Schema.String, Schema.String),
});

const scriptsOf = (file: string) =>
  Object.entries(
    Schema.decodeUnknownSync(Schema.fromJsonString(Package))(
      readFileSync(new URL(file, import.meta.url), "utf-8")
    ).scripts
  );

const stageChanging = /\balchemy (?:deploy|destroy)\b/u;

describe.each(["../package.json", "../../../package.json"])(
  "scripts in %s",
  (file) => {
    const scripts = scriptsOf(file).filter(([, command]) =>
      stageChanging.test(command)
    );

    it("has a deploy script to check", () => {
      expect(scripts.map(([name]) => name)).toContain("deploy");
    });

    it.each(scripts)("%s names the prod stage", (_name, command) => {
      expect(command).toMatch(/\balchemy (?:deploy|destroy) --stage prod\b/u);
    });

    it.each(scripts)("%s asks before it applies", (_name, command) => {
      expect(command).not.toMatch(/--yes\b|\s-y\b/u);
    });
  }
);
