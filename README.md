# parthmangrola.com

Personal portfolio site.

## Stack

- [Astro](https://astro.build), fully static
- Tailwind CSS v4
- Cloudflare Workers static assets, deployed with [Alchemy](https://alchemy.run)
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
