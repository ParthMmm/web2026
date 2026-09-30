import { readFileSync } from "node:fs";
import path from "node:path";

import type { APIRoute, GetStaticPaths } from "astro";
import type { CSSProperties, ReactNode } from "react";
import { createElement } from "react";
import { satoriAstroOG } from "satori-astro";

import type { OgCard, Tone } from "../../data/og";
import { isOgSlug, ogCards } from "../../data/og";
import { profile } from "../../data/site";

export const prerender = true;

/* Satori reads neither OKLCH nor color-mix, so these are the light theme's
   tokens resolved to sRGB against the cream canvas. */
const COLOR = {
  canvas: "#fcf6e9",
  ink1: "#1f1d17",
  ink3: "#615d56",
  ink4: "#837f76",
  line: "#e1dccf",
  tone: { green: "#009038", orange: "#ff8506" } satisfies Record<Tone, string>,
};

const WIDTH = 1200;
const HEIGHT = 630;
const HOST = new URL(profile.site).host.replace(/^www\./u, "");

const loadFont = (weight: 400 | 500) => {
  const buffer = readFileSync(
    path.join(
      process.cwd(),
      `node_modules/@fontsource/inter/files/inter-latin-${weight}-normal.woff`
    )
  );
  return {
    data: buffer.buffer.slice(
      buffer.byteOffset,
      buffer.byteOffset + buffer.byteLength
    ),
    name: "Inter",
    style: "normal" as const,
    weight,
  };
};

const fonts = [loadFont(400), loadFont(500)];

const h = (style: CSSProperties, ...children: ReactNode[]) =>
  createElement("div", { style: { display: "flex", ...style } }, ...children);

const marker = (color: string, size: number) =>
  h({
    backgroundColor: color,
    borderRadius: size * 0.2,
    flexShrink: 0,
    height: size,
    width: size,
  });

/* The four showcased projects' markers, in homepage order. */
const toneRow = () =>
  h(
    { gap: 10 },
    ...(["green", "orange", "green", "orange"] as const).map((tone) =>
      marker(COLOR.tone[tone], 14)
    )
  );

const header = () =>
  h(
    { alignItems: "center", fontSize: 26, gap: 14 },
    h({ color: COLOR.ink1, fontWeight: 500 }, profile.name),
    h({ color: COLOR.ink4 }, "·"),
    h({ color: COLOR.ink3 }, profile.role)
  );

const headline = (card: OgCard) => {
  if (card.quiet) {
    return h(
      {
        color: COLOR.ink1,
        flexWrap: "wrap",
        fontSize: 88,
        fontWeight: 500,
        letterSpacing: "-0.034em",
        lineHeight: 1.08,
        maxWidth: 980,
      },
      createElement("span", { style: { marginRight: 22 } }, card.title),
      createElement("span", { style: { color: COLOR.ink4 } }, card.quiet)
    );
  }

  return h(
    { flexDirection: "column", gap: 22 },
    card.kicker
      ? h({ color: COLOR.ink3, fontSize: 28, fontWeight: 500 }, card.kicker)
      : null,
    h(
      { alignItems: "center", gap: 26 },
      marker(COLOR.tone[card.tone], 28),
      h(
        {
          color: COLOR.ink1,
          fontSize: 80,
          fontWeight: 500,
          letterSpacing: "-0.03em",
          lineHeight: 1.05,
        },
        card.title
      )
    ),
    card.description
      ? h(
          {
            color: COLOR.ink3,
            fontSize: 32,
            lineHeight: 1.45,
            marginTop: 6,
            maxWidth: 940,
          },
          card.description
        )
      : null
  );
};

const footer = (slug: string) =>
  h(
    {
      alignItems: "center",
      borderTop: `2px solid ${COLOR.line}`,
      justifyContent: "space-between",
      paddingTop: 30,
    },
    h(
      { color: COLOR.ink3, fontSize: 26 },
      slug === "index" ? HOST : `${HOST}/${slug}`
    ),
    toneRow()
  );

const template = (slug: string, card: OgCard) =>
  h(
    {
      backgroundColor: COLOR.canvas,
      flexDirection: "column",
      fontFamily: "Inter",
      height: "100%",
      justifyContent: "space-between",
      padding: "72px 80px 64px",
      width: "100%",
    },
    header(),
    headline(card),
    footer(slug)
  );

export const getStaticPaths = (() =>
  Object.keys(ogCards).map((slug) => ({
    params: { slug },
  }))) satisfies GetStaticPaths;

export const GET: APIRoute = async ({ params }) => {
  const slug = params.slug ?? "index";
  if (!isOgSlug(slug)) {
    return new Response("Not found", { status: 404 });
  }
  return await satoriAstroOG({
    height: HEIGHT,
    template: template(slug, ogCards[slug]),
    width: WIDTH,
  }).toResponse({ satori: { fonts } });
};
