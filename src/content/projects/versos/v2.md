---
title: "Tracklister"
description: "A web application for DJs to create and format track listings from their mixes"
pubDate: 2024-01-15
updatedDate: 2024-01-20
---

<!-- # [Tracklister](https://tracklister.p11a.xyz/) <svg width="16" height="16" fill="none" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M5 3h6v2H5v14h14v-6h2v8H3V3h2zm8 0h8v8h-2V7h-2V5h-4V3zm0 8h-2v2H9v2h2v-2h2v-2zm4-4h-2v2h-2v2h2V9h2V7z" fill="currentColor"/></svg>

<h1>
  <a href="https://tracklister.p11a.xyz/">Tracklister</a>
  <svg width="16" height="16" fill="none" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" style="display: inline-block; vertical-align: middle; margin-left: 4px;">
    <path d="M5 3h6v2H5v14h14v-6h2v8H3V3h2zm8 0h8v8h-2V7h-2V5h-4V3zm0 8h-2v2H9v2h2v-2h2v-2zm4-4h-2v2h-2v2h2V9h2V7z" fill="currentColor"/>
  </svg>
</h1> -->

<!-- easily turn a tracklist into a playlist. -->

## what and why

i listen to a lot of DJ mixes and sets and the youtube comments will almost always have a timestamped tracklist. so I then would have to go through each song one by one and find it on spotify, soundcloud, or youtube. this was annoying to do manually

now with LLMs being great at handling messy text like this when given the proper prompt we can easily transform a tracklist into searchable queries.

```json
// Input
"[47:58] Juelz & JAWNS - Enter The World"

// Output
{
  "artist": "Juelz",
  "title": "Enter The World"
}

// Input
"40:56 Special Request - Pull Up (Tim Reaper Remix)"

// Output
{
  "artist": "Special Request",
  "title": "Pull Up (Tim Reaper Remix)"
}
```

there are definitely still cases where the LLM performs poorly though. honestly this one is just hard to parse anyways

```json
// Input
"[31:29] Joy Orbison & Vice Selex vs. Sage The Gemini & IAMSU! - Flight FM vs. Gas Pedal (RL Grime Edit)"

// Output
{
  "artist": "Joy Orbison & Vice Selex vs. Sage The Gemini & IAMSU!",
  "title": "Flight FM vs. Gas Pedal (RL Grime Edit)"
}
```

## how it works

1. **User Input**

   - Raw tracklist is submitted by user

2. **LLM Parse & Clean**

   - LLM processes raw input to standardize format
   - Extracts artist and title information
   - Cleans up common formatting issues and variations

3. **Embedding Creation**

   - Creates vector embeddings for each parsed track
   - Embeddings capture semantic meaning of track information

4. **Vector Database Search**

   - Queries vector database using created embeddings
   - Finds similar tracks based on semantic similarity
   - Calculates confidence scores for potential matches

5. **LLM Verification**

   - LLM evaluates matches from vector search
   - Uses vector search results and additional context
   - Makes intelligent matching decisions based on multiple factors

6. **Spotify Search**

   - Queries Spotify API with verified matches
   - Finds exact tracks in Spotify's catalog
   - Returns final track matches for playlist creation

7. **User Review & Playlist Creation**
   - User reviews results
   - Creates new Spotify playlists with accepted results

## tech stack

- Frontend: React, Next.js, React Query, TypeScript
- Backend: Hono, TypeScript, Drizzle, Cloudflare D1, Workers, Durable Objects, Vectorize,

after learning about all these Cloudflare services, I knew when I built this version of the app I wanted to try them all out. the whole process runs in a durable object and I use a server-sent event(SSE) to notify the frontend of the process state. so the loading bar actually means something!

I just learned about SSEs and they are very cool. it was very easy to set up with hono and a lot easier than webhooks.

this was a learning process for me so there is probably a lot of room for improvement especially in the whole llm, embedding, vector search area.

## Future Plans

- improve LLM performance
- mobile design
- more social features
- allow user to correct parsing mistakes
