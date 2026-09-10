import worker from "../src/worker.ts";

const token = process.env.PROBE_TOKEN;
if (!token) {
  throw new Error("Set a non-production PROBE_TOKEN");
}
const server = Bun.serve({
  fetch: (request) => worker.fetch(request, { PROBE_TOKEN: token }),
  hostname: "127.0.0.1",
  port: Number(process.env.PORT ?? 8787),
});
console.log(`Local contract probe: ${server.url}`);
