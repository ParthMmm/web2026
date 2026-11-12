/* Serves the seeded gallery the way the API does, at
   http://127.0.0.1:8787/gallery, and logs every request so a smoke check can
   show what the page asked for. Only the manifest and the WebP sizes are
   served; anything else is a 404 and still logged.

     bun tests/fixtures/gallery/serve.ts
     PUBLIC_GALLERY_URL=http://127.0.0.1:8787/gallery bun run build */

import path from "node:path";

const ROOT = path.join(import.meta.dir, "public");
const PREFIX = "/gallery/";
const PORT = 8787;
const SERVED =
  /^(?:v1\.json|photos\/[0-9a-f]{64}\/w(?:480|1200|2400)-[0-9a-f]{64}\.webp)$/u;

const headers = (file: string) => ({
  "Access-Control-Allow-Origin": "*",
  "Cache-Control": file.endsWith(".webp")
    ? "public, max-age=31536000, immutable"
    : "public, max-age=60",
});

Bun.serve({
  fetch: async (request) => {
    const { pathname } = new URL(request.url);
    const relative = pathname.startsWith(PREFIX)
      ? pathname.slice(PREFIX.length)
      : "";
    const file = Bun.file(path.join(ROOT, relative));
    const found = SERVED.test(relative) && (await file.exists());
    console.log(`${found ? 200 : 404} ${request.method} ${pathname}`);
    return found
      ? new Response(file, { headers: headers(relative) })
      : new Response("Not found", { status: 404 });
  },
  hostname: "127.0.0.1",
  port: PORT,
});

console.log(`Serving ${ROOT} at http://127.0.0.1:${PORT}${PREFIX}`);
