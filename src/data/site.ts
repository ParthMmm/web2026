export type Record = {
	title: string;
	description: string;
	/** Omitted when there is nowhere to go. The list renders those as plain text. */
	url?: string;
};

/** The footer renders the label only, so these carry no description. */
export type Link = {
	title: string;
	url: string;
};

export const profile = {
	name: "Parth Mangrola",
	role: "Full-stack engineer",
	status: "Open to new roles",
	site: "https://www.parthm.dev",
};

export const work: Record[] = [
	{
		title: "Carbon Law Group",
		description:
			"Sole engineer on a full-stack SaaS for state filing compliance. Shipped a Chrome extension that automates 30+ form fields.",
		url: "/work/carbon-law-group",
	},
	{
		title: "Embic",
		description:
			"Healthcare platform with an admin dashboard for medical organizations and patient records. Fixed production bugs affecting 125+ users.",
		url: "/work/embic",
	},
	{
		title: "Penovar",
		description: "Landing page for a clinical research facility.",
	},
	{
		title: "Rhythm Events",
		description: "Animated landing page for an events business.",
	},
	{
		title: "Vitality Clinical Research",
		description: "Landing page for a clinical research facility.",
	},
];

export const projects: Record[] = [
	{
		title: "Beacon",
		description:
			"Real-time shared workspace for planning trips together. Built on Expo and Convex, with markdown import and export.",
		url: "/projects/beacon",
	},
	{
		title: "Versos",
		description:
			"Playlist builder for DJ sets, formerly Tracklister. Gemini parses the tracklist, then a fuzzy matching engine finds each track on Apple Music.",
		url: "/projects/versos",
	},
	{
		title: "Orbis",
		description:
			"Self-hosted library for DJ sets and mixes. A SwiftUI app for iPhone, iPad, and Mac, backed by a Bun and Effect server over Tailscale.",
		url: "https://github.com/ParthMmm/orbis",
	},
	{
		title: "Mana Margherita",
		description:
			"Swiss tournament app for Magic: The Gathering nights with friends. Pairings, Commander pods, a synced timer, and ELO, on Convex.",
		url: "https://mana-margherita.p11a.xyz/",
	},
	{
		title: "Auriom",
		description:
			"Social platform for music lovers. Built during Buildspace Nights and Weekends S2.",
		url: "https://github.com/ParthMmm/auriom",
	},
];

export const elsewhere: Link[] = [
	{ title: "GitHub", url: "https://github.com/ParthMmm" },
	{ title: "LinkedIn", url: "https://www.linkedin.com/in/parthmangrola/" },
	{
		title: "Résumé",
		url: "https://drive.google.com/file/d/170kgQ0N4xIQd-kuW4DpQa2enXMxeLrfq/view?usp=sharing",
	},
];
