// @ts-check
import { defineConfig } from "astro/config";

import sitemap from "@astrojs/sitemap";
import tailwindcss from "@tailwindcss/vite";
import expressiveCode from "astro-expressive-code";

import react from "@astrojs/react";

// https://astro.build/config
export default defineConfig({
  site: "https://www.parthm.dev",
  vite: {
    plugins: [tailwindcss()],
  },
  integrations: [expressiveCode({
    // One theme in both appearances: a code block is a self-contained
    // inverted surface, not chrome that restates the page background.
    themes: ["github-dark-default"],
    themeCssSelector: () => false,
    useDarkModeMediaQuery: false,
    styleOverrides: {
      borderRadius: "8px",
      borderColor: "rgb(255 255 255 / 11%)",
      codeBackground: "#151515",
      frames: {
        shadowColor: "transparent",
        editorTabBarBackground: "#111111",
        editorActiveTabBackground: "#151515",
        terminalBackground: "#151515",
        terminalTitlebarBackground: "#111111",
      },
    },
  }), sitemap(), react()],
  output: "static",
  redirects: {
    "/projects/tracklister": "/projects/versos",
  },
});