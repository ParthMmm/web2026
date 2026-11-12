// @ts-check

import react from "@astrojs/react";
import sitemap from "@astrojs/sitemap";
import tailwindcss from "@tailwindcss/vite";
import expressiveCode from "astro-expressive-code";
import { defineConfig } from "astro/config";

const FONT_FILE = /\.woff2?$/u;

// https://astro.build/config
export default defineConfig({
  integrations: [
    expressiveCode({
      styleOverrides: {
        borderColor: "rgb(255 255 255 / 11%)",
        borderRadius: "8px",
        codeBackground: "#151515",
        frames: {
          editorActiveTabBackground: "#151515",
          editorTabBarBackground: "#111111",
          shadowColor: "transparent",
          terminalBackground: "#151515",
          terminalTitlebarBackground: "#111111",
        },
      },
      // One theme in both appearances: a code block is a self-contained
      // inverted surface, not chrome that restates the page background.
      themeCssSelector: () => false,
      themes: ["github-dark-default"],
      useDarkModeMediaQuery: false,
    }),
    sitemap(),
    react(),
  ],
  output: "static",
  redirects: {
    "/projects/mana-margherita": "/projects/mote/",
    "/projects/tracklister": "/projects/versos/",
  },
  site: "https://www.parthm.dev",
  // The host serves a page at its directory URL and answers the slashless
  // form with a 307. Building, linking and the dev server all use the slash,
  // so a link that would redirect in production is a 404 in dev.
  trailingSlash: "always",
  vite: {
    build: {
      // Vite inlines any asset under 4 kB as base64. Four Geist Mono files
      // fall under that, so every page paid for Cyrillic Extended, Vietnamese
      // and symbol glyphs in its render-blocking stylesheet. A font stays a
      // file: the browser then fetches only the subsets the page renders.
      // `undefined` keeps Vite's default for everything else.
      assetsInlineLimit: (file) => (FONT_FILE.test(file) ? false : undefined),
    },
    plugins: [tailwindcss()],
  },
});
