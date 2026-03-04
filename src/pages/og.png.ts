import { satoriAstroOG } from "satori-astro";
import { readFileSync } from "node:fs";
import { join } from "node:path";

export const prerender = true;

const fontBuffer = readFileSync(
	join(
		process.cwd(),
		"node_modules/@fontsource/geist-mono/files/geist-mono-latin-700-normal.woff",
	),
);
const fontData = fontBuffer.buffer.slice(
	fontBuffer.byteOffset,
	fontBuffer.byteOffset + fontBuffer.byteLength,
);

export const GET = async () => {
	return await satoriAstroOG({
		template: {
			type: "div",
			props: {
				style: {
					display: "flex",
					flexDirection: "column",
					justifyContent: "center",
					alignItems: "center",
					width: "100%",
					height: "100%",
					backgroundColor: "#010080",
				},
				children: {
					type: "span",
					props: {
						style: {
							fontFamily: "Geist Mono",
							fontSize: 72,
							fontWeight: 700,
							color: "#c8ff4a",
						},
						children: "parth mangrola",
					},
				},
			},
		},
		width: 1200,
		height: 630,
	}).toResponse({
		satori: {
			fonts: [
				{
					name: "Geist Mono",
					data: fontData,
					weight: 700,
					style: "normal",
				},
			],
		},
	});
};
