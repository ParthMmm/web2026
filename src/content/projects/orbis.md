---
title: "Orbis"
description: "A private library for DJ sets and mixes, with native apps for iPhone, iPad, and Mac and a server I own."
pubDate: 2026-09-09
applicationCategory: "MultimediaApplication"
operatingSystem: "iOS, iPadOS, macOS"
---

## what and why

I listen to a lot of long music: DJ sets and mixes from YouTube and SoundCloud. those sites are built for browsing, not for keeping things. orbis is a private library for them. paste a link, file it with a title and tags, download the audio to a server I own, and play it from a native app on iPhone, iPad, or Mac.

it's built for one listener and one home server, and it makes firm choices to stay that way: no accounts, no public endpoint, no cloud.

## how it works

1. **save**
   - paste a link in the app, send it from the share sheet, or grab the current tab with Raycast
   - saving files a set in the library
2. **download**
   - a separate step fetches the audio through a self-hosted Cobalt instance and keeps it next to the database
3. **listen**
   - the native apps stream it with the system media frameworks and seek by byte range
   - Now Playing and CarPlay work like any other audio app
4. **resume**
   - the listening queue and every playback position live on the server, so a second device picks up exactly where the first one stopped

## private by default

the server only listens on loopback. the one way in is Tailscale Serve on my tailnet, and every request from off the machine needs an enrolled device token. the server only stores a digest of each token.

## how it's built

- **domain language first** — the terms (set, retained audio, playback position, listen) are defined once in `CONTEXT.md`, and the code uses them
- **decision records** — six ADRs cover the client platform, download jobs, the audio fetch path, service identity, metadata providers, and artwork sizes
- **one source for design tokens** — a single tokens file generates both the Swift colors and the CSS, and CI fails if they drift
- **native test lanes** — one command starts a throwaway server with its own database, pairs a device, and runs the unit tests and UI journeys against it

## tech stack

- Apple: one SwiftUI app for iOS and macOS, Swift 6 strict concurrency, a share extension, CarPlay
- Server: Bun, Effect v4, Bun SQLite, a persisted download worker
- Audio: self-hosted Cobalt
- Network: Tailscale Serve with per-device tokens
- Other clients: a Raycast extension, and an Electron desktop app that came first
- Tooling: Turborepo, Bun, xcodegen, swift-format, oxlint, oxfmt

## status

working today: saving and organizing sets, tags, and playlists, search and filters, downloads, streaming with a shared queue and resume, and Raycast capture. not built yet: shared libraries, collaborative playlists, and App Store release. for now I build and run it from Xcode.
