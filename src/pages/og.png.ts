import { readFileSync } from "node:fs";
import path from "node:path";

import { createElement } from "react";
import { satoriAstroOG } from "satori-astro";

export const prerender = true;

const fontBuffer = readFileSync(
  path.join(
    process.cwd(),
    "node_modules/@fontsource/geist-mono/files/geist-mono-latin-700-normal.woff"
  )
);
const fontData = fontBuffer.buffer.slice(
  fontBuffer.byteOffset,
  fontBuffer.byteOffset + fontBuffer.byteLength
);

export const GET = async () =>
  await satoriAstroOG({
    height: 630,
    template: createElement(
      "div",
      {
        style: {
          alignItems: "center",
          backgroundColor: "#010080",
          display: "flex",
          flexDirection: "column",
          height: "100%",
          justifyContent: "center",
          width: "100%",
        },
      },
      createElement(
        "span",
        {
          style: {
            color: "#c8ff4a",
            fontFamily: "Geist Mono",
            fontSize: 72,
            fontWeight: 700,
          },
        },
        "parth mangrola"
      )
    ),
    width: 1200,
  }).toResponse({
    satori: {
      fonts: [
        {
          data: fontData,
          name: "Geist Mono",
          style: "normal",
          weight: 700,
        },
      ],
    },
  });
