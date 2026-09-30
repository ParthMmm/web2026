---
title: "Mana Margherita"
description: "Swiss tournaments for Magic: The Gathering nights with friends, on the web, on phones, and on the TV."
pubDate: 2026-01-19
applicationCategory: "GameApplication"
operatingSystem: "Web, iOS, tvOS"
---

## what and why

my friends and I run Magic events at home: prereleases and Commander nights. doing pairings and standings by hand means someone is always doing math instead of playing. mana margherita runs the event so everyone can just play.

it's built around the playgroup. a group owns its events, decides who's in it, and decides who gets to run things.

## features

- **swiss pairings** — points-based matching with rematch avoidance
- **real tiebreakers** — OMW%, GW%, and OGW%, with the 33% floor from the WotC rules
- **commander pods** — 3 and 4 player pods, optionally balanced by ELO, with power bracket tracking
- **synced timer** — the round clock comes from the server, so every phone and the TV show the same time
- **hidden ELO** — skill ratings tracked in the background, optionally used to seed pairings
- **deck tracking** — colors, commander (with Scryfall autocomplete), and power bracket
- **RSVPs** — yes, no, or maybe, with guests
- **achievements and share cards** — First Blood, Undefeated, Comeback King, and a results image to post after

## one backend, every screen

- **web** is where players RSVP, check their stats, and follow the event live
- **the Expo app** is for organizing and playing from your phone
- **the tvOS app** is the room display: timer, standings, pairings, and announcements. it's read-only, so nobody changes a result from the couch
- **native iOS** is next. a SwiftUI app with iOS 26 as the minimum will replace the Expo client

## tech stack

- Backend: Convex, Confect for Effect schemas on Convex, Better Auth
- Web: TanStack Start, React 19, Tailwind v4, shadcn with Base UI
- Mobile: Expo SDK 55, React Native, expo-router
- Apple: Swift 6, SwiftUI, Liquid Glass on tvOS 26, a shared ManaCore Swift package
- Hosting: Cloudflare Workers, Convex Cloud
- Tooling: Turborepo, pnpm, oxlint, oxfmt, Vitest, knip

## status

active development. the pairing, standings, and scoring logic is covered by tests, and the native iOS app is being designed now.
