const token = crypto.randomUUID();
const env = { ...process.env, PORT: "0", PROBE_TOKEN: token };
const server = Bun.spawn(["bun", "scripts/serve.ts"], {
  env,
  stderr: "inherit",
  stdout: "pipe",
});
const timeout = setTimeout(() => server.kill(), 180_000);
try {
  const reader = server.stdout.getReader();
  const first = await reader.read();
  reader.releaseLock();
  const output = new TextDecoder().decode(first.value);
  const url = output.match(/http:\/\/127\.0\.0\.1:\d+\//u)?.[0];
  if (!url) {
    throw new Error(`Server did not start: ${output}`);
  }
  const client = Bun.spawn(
    [
      "cargo",
      "run",
      "--locked",
      "--manifest-path",
      "client/Cargo.toml",
      "--",
      url,
      ...process.argv.slice(2),
    ],
    {
      env,
      stderr: "inherit",
      stdout: "inherit",
    },
  );
  if ((await client.exited) !== 0) {
    throw new Error("Rust HTTP smoke failed");
  }
} finally {
  clearTimeout(timeout);
  server.kill();
  await server.exited;
}
