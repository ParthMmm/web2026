/**
 * Signs in to Apple Music once and prints the Music User Token for
 * APPLE_MUSIC_USER_TOKEN. Apple expires it after about six months; run this again then.
 *
 *   bun workers/now-playing/authorize.ts
 */
import { signDeveloperToken } from "./apple-music-token";

const PORT = 4399;

const required = (name: string): string => {
  const value = process.env[name];
  if (!value) {
    throw new Error(`Set ${name} in .env first.`);
  }
  return value;
};

const { token } = await signDeveloperToken(
  {
    keyId: required("APPLE_MUSIC_KEY_ID"),
    privateKey: required("APPLE_MUSIC_PRIVATE_KEY"),
    teamId: required("APPLE_MUSIC_TEAM_ID"),
  },
  Math.floor(Date.now() / 1000)
);

const page = `<!doctype html>
<meta charset="utf-8">
<title>Authorize Apple Music</title>
<p id="status">Loading MusicKit…</p>
<script src="https://js-cdn.music.apple.com/musickit/v3/musickit.js" async></script>
<script>
  document.addEventListener("musickitloaded", async () => {
    const status = document.getElementById("status");
    try {
      await MusicKit.configure({
        developerToken: ${JSON.stringify(token)},
        app: { name: "parthm.dev", build: "1" },
      });
      status.textContent = "Waiting for Apple sign-in…";
      const userToken = await MusicKit.getInstance().authorize();
      await fetch("/token", { method: "POST", body: userToken });
      status.textContent = "Done. The token is in your terminal; you can close this tab.";
    } catch (error) {
      status.textContent = "Sign-in failed: " + error;
    }
  });
</script>`;

const server = Bun.serve({
  async fetch(request) {
    const { pathname } = new URL(request.url);
    if (request.method === "POST" && pathname === "/token") {
      process.stdout.write(
        `\nAPPLE_MUSIC_USER_TOKEN=${await request.text()}\n`
      );
      setTimeout(() => server.stop(), 100);
      return new Response("ok");
    }
    return new Response(page, {
      headers: { "content-type": "text/html; charset=utf-8" },
    });
  },
  hostname: "localhost",
  port: PORT,
});

process.stdout.write(
  `Open http://localhost:${PORT} and sign in to Apple Music.\n`
);
