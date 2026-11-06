import { mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { Effect } from "effect";
import { Digest } from "effect/crypto";

import { variantKinds } from "../src/photos/domain.ts";
import { fixture } from "../tests/support/fixture.ts";

const { bodies, manifest } = await Effect.runPromise(
  fixture().pipe(Effect.provide(Digest.layerWebCrypto)),
);
const dir = mkdtempSync(join(tmpdir(), "photo-smoke-"));
writeFileSync(join(dir, "manifest.json"), JSON.stringify(manifest));
for (const kind of variantKinds) {
  writeFileSync(join(dir, kind), bodies[kind]);
}

const env = { ...process.env, OWNER_TOKEN: crypto.randomUUID().repeat(2), PORT: "0" };
const server = Bun.spawn(["bun", "scripts/serve.ts"], { env, stderr: "inherit", stdout: "pipe" });
const timeout = setTimeout(() => server.kill(), 300_000);
try {
  const reader = server.stdout.getReader();
  const first = await reader.read();
  const output = new TextDecoder().decode(first.value);
  process.stdout.write(output);
  void (async () => {
    for (let chunk = await reader.read(); !chunk.done; chunk = await reader.read()) {
      process.stdout.write(chunk.value);
    }
  })();
  const url = output.match(/http:\/\/127\.0\.0\.1:\d+\//u)?.[0];
  if (!url) {
    throw new Error(`Server did not start: ${output}`);
  }
  const client = Bun.spawn(
    ["cargo", "run", "--locked", "--manifest-path", "client/Cargo.toml", "--", url, dir],
    { env, stderr: "inherit", stdout: "inherit" },
  );
  if ((await client.exited) !== 0) {
    throw new Error("Rust HTTP smoke failed");
  }
} finally {
  clearTimeout(timeout);
  server.kill();
  await server.exited;
  rmSync(dir, { force: true, recursive: true });
}
