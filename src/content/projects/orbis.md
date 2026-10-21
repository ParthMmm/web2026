---
title: "Orbis"
description: "A shared library for DJ sets and mixes, with native apps for iPhone, iPad, Mac, and Apple TV, a web app, and an audio server I own."
pubDate: 2026-09-09
updatedDate: 2026-10-21
applicationCategory: "MultimediaApplication"
operatingSystem: "iOS, iPadOS, macOS, tvOS, Web"
---

## what and why

I listen to a lot of long music: DJ sets and mixes from YouTube and SoundCloud. those sites are built for browsing, not for keeping things. orbis is a library for them. paste a link, file it with a title and tags, download the audio to a server I own, and play it from a native app or the web.

it started as a library for one listener on one home server. now it's for a small private group. each person has their own library, queue, and playback positions, and everyone shares the audio files.

## how it works

1. **save**
   - paste a link in the app or on the web, send it from the share sheet, grab the current tab with Raycast, or import a whole YouTube playlist
2. **download**
   - an audio node on my server fetches the audio with yt-dlp or a self-hosted Cobalt
   - downloads are scheduled fairly across people, and everyone who saved the same set shares one audio file
3. **listen**
   - the native apps stream it with the system media frameworks. Now Playing, the Lock Screen, and CarPlay work like any other audio app
   - sets with a tracklist from [Versos](/projects/versos) show each track as a cue, and tapping one seeks to it
4. **resume**
   - the queue and every playback position live in the API, so a second device picks up exactly where the first one stopped

## the group

- **people.** sign in by invite, a device link, a group passcode, or a sign-in code
- **friends' libraries.** see what other people in the group saved
- **collaborative playlists.** playlists that more than one person can edit
- **presence.** see who's around, from a live change feed

## how it's built

- **one Durable Object per group.** a Cloudflare Worker sends each request to a SQLite-backed Durable Object that holds the library, people, keys, the queue, and the download schedule
- **audio on my server.** my server opens a WebSocket out to the group, so it has no database and no API keys. it serves audio through signed stream grants
- **domain language first.** a glossary defines the terms (set, retained audio, playback position, listen) once, and the code uses them
- **decision records.** twenty ADRs. they cover the client platform, sign-in, collaborative playlists, tracklists, and the move to a Durable Object
- **one source for design tokens.** a single tokens file generates both the Swift colors and the CSS, and CI fails if they drift
- **observability.** a wide event for every audio request, plus playback diagnostics from the Apple apps

## tech stack

- Apple: one SwiftUI app for iOS, macOS, and tvOS, Swift 6 strict concurrency, a share extension, CarPlay, a Top Shelf image
- Web: TanStack Start, React 19, Tailwind v4, shadcn
- API: Cloudflare Workers and Durable Objects, Effect v4 HttpApi, Drizzle on SQLite, shared typed contracts
- Audio: a Bun audio node, yt-dlp, self-hosted Cobalt
- Deploy: Alchemy
- Tooling: Turborepo, Bun, xcodegen, swift-format, oxlint, oxfmt, Miniflare

## status

working today are libraries, tags, playlists and collaborative playlists, search, downloads, streaming with a shared queue and resume, tracklists, the web app, and the Apple TV app. the group gets builds through TestFlight. there's no App Store release.
