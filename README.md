# parthmangrola.com

Personal portfolio site.

## Stack

- [Astro](https://astro.build), fully static
- Tailwind CSS v4
- Cloudflare Workers static assets, deployed with [Alchemy](https://alchemy.run)
- A separate Worker at `now.parthm.dev` that reports what is playing now
- Bun

## Development

```sh
bun install
bun run dev
```

## Build

```sh
bun run build
bun run preview
```

## Deploy

Alchemy's `default` profile authenticates the deployment. Run `bun alchemy login` if needed.

```sh
bun alchemy plan --stage prod
bun run deploy
```

State lives in `.alchemy/` (ignored). Don't delete it while the Worker is deployed; it tracks ownership.

## Now playing

The Worker serves what is playing now as JSON. Sources, in order: Orbis Presence, Last.fm (NepTunes on the laptop), then MusicKit recently played. Copy `.env.example` to `.env` and fill in the ones you want. MusicKit needs a one-time Apple sign-in:

```sh
bun workers/now-playing/authorize.ts
```

That prints `APPLE_MUSIC_USER_TOKEN`. Apple expires it after about six months; run the script again then. Orbis Presence never includes your own listens, so `ORBIS_API_KEY` must belong to a different Person who can see you.
