---
title: "Beacon"
description: "Smart travel companion for collaborative trip planning"
pubDate: 2026-03-03
---

## what and why

trip planning is scattered across group chats, shared docs, screenshots of Google Maps, and half-finished spreadsheets. beacon pulls it all into one place, a real-time shared workspace for building trips together.

depending on the country, Apple Maps or Google Maps is better, so beacon lets you switch between them seamlessly.

your plans aren't locked in. markdown import/export means you can plan with any tool, any LLM, or just a text editor. the app is the interface, not the cage.

## features

- **collaborative itineraries** — build and edit trip plans together in real-time
- **location management** — save, organize, and share places
- **markdown interop** — import/export plans as markdown, bring your own tools
- **live sync** — everyone sees the same thing, always up to date
- **map provider switching** — seamlessly switch between Apple Maps and Google Maps
- **Apple Sign-In** — one tap to get started

## tech stack

- **Expo + React Native** — native iOS app with file-based routing (Expo Router)
- **Convex** — real-time serverless backend, live-syncing data, no manual cache invalidation
- **Better Auth** — authentication with Apple Sign-In
- **Jotai** — minimal client-side state management
- **NativeWind** — Tailwind CSS for React Native
- **Zod** — shared validation schemas across client and server
- **Turborepo + Bun** — monorepo tooling

## status

active development. building out itinerary planning and markdown interop.
