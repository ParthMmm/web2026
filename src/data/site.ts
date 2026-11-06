export interface Record {
  title: string;
  description: string;
  /** Omitted when there is nowhere to go. The list renders those as plain text. */
  url?: string;
  /** The thing itself, when it lives somewhere other than the write-up. */
  site?: string;
}

/** The footer renders the label only, so these carry no description. */
export interface Link {
  title: string;
  url: string;
}

export const profile = {
  name: "Parth Mangrola",
  role: "Full-stack engineer",
  site: "https://www.parthm.dev",
};

export const work: Record[] = [
  {
    description:
      "Sole engineer on a full-stack SaaS for state filing compliance. Shipped a Chrome extension that automates 30+ form fields.",
    title: "Carbon Law Group",
    url: "/work/carbon-law-group",
  },
  {
    description:
      "Healthcare platform with an admin dashboard for medical organizations and patient records. Fixed production bugs affecting 125+ users.",
    title: "Embic",
    url: "/work/embic",
  },
  {
    description: "Landing page for a clinical research facility.",
    title: "Penovar",
  },
  {
    description: "Animated landing page for an events business.",
    title: "Rhythm Events",
  },
  {
    description: "Landing page for a clinical research facility.",
    title: "Vitality Clinical Research",
  },
];

export const projects: Record[] = [
  {
    description:
      "Runs Magic: The Gathering nights for a playgroup, formerly Mana Margherita. Swiss and Commander tournaments, ratings, and a live timer on the web, the TV, and iPhone.",
    site: "https://motehall.com/",
    title: "Möte",
    url: "/projects/mote",
  },
  {
    description:
      "Playlist builder for DJ sets, formerly Tracklister. Paste a tracklist or a link, and a durable import on Cloudflare parses it, matches each track, and builds an Apple Music playlist.",
    site: "https://versos.studio/",
    title: "Versos",
    url: "/projects/versos",
  },
  {
    description:
      "Shared library for DJ sets and mixes. Native apps for iPhone, iPad, Mac, and Apple TV, and a web app, on a Cloudflare API with an audio server I own.",
    site: "https://github.com/ParthMmm/orbis",
    title: "Orbis",
    url: "/projects/orbis",
  },
  {
    description:
      "Travel app for planning trips together. Shared trips, places, and itineraries on Expo and Cloudflare, and Claude or ChatGPT can help plan over MCP.",
    title: "Beacon",
    url: "/projects/beacon",
  },
  {
    description:
      "Social platform for music lovers. Built during Buildspace Nights and Weekends S2.",
    site: "https://github.com/ParthMmm/auriom",
    title: "Auriom",
    url: "/projects/auriom",
  },
];

/** "https://github.com/ParthMmm/orbis" reads as "github.com/ParthMmm/orbis". */
export const siteLabel = (url: string): string => {
  const { host, pathname } = new URL(url);
  return `${host.replace(/^www\./u, "")}${pathname}`.replace(/\/$/u, "");
};

export const elsewhere: Link[] = [
  { title: "GitHub", url: "https://github.com/ParthMmm" },
  { title: "LinkedIn", url: "https://www.linkedin.com/in/parthmangrola/" },
  {
    title: "Résumé",
    url: "https://drive.google.com/file/d/170kgQ0N4xIQd-kuW4DpQa2enXMxeLrfq/view?usp=sharing",
  },
];
