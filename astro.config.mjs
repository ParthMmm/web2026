// @ts-check

import react from "@astrojs/react";
import sitemap from "@astrojs/sitemap";
import tailwindcss from "@tailwindcss/vite";
import expressiveCode from "astro-expressive-code";
import { defineConfig } from "astro/config";

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
    "/projects/mana-margherita": "/projects/mote",
    "/projects/tracklister": "/projects/versos",
  },
  site: "https://www.parthm.dev",
  vite: {
    plugins: [tailwindcss()],
  },
});
