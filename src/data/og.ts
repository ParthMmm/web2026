import { projects, work } from "./site";

export type Tone = "green" | "orange";

export interface OgCard {
  /** Section the page belongs to, shown above the title. */
  kicker?: string;
  title: string;
  /** Continues the title a rung down in tone, like the homepage thesis. */
  quiet?: string;
  description?: string;
  tone: Tone;
}

const describe = (list: typeof projects, url: string): string =>
  list.find((item) => item.url === url)?.description ?? "";

/** Keyed by page path without slashes; "index" is the homepage. */
export const ogCards = {
  index: {
    quiet: "I build apps for the things I'm into.",
    title: "Hey! I'm Parth.",
    tone: "green",
  },
  projects: {
    description: "Möte, Versos, Orbis, Beacon, and Auriom.",
    title: "Projects",
    tone: "green",
  },
  "projects/auriom": {
    description: describe(projects, "/projects/auriom/"),
    kicker: "Projects",
    title: "Auriom",
    tone: "green",
  },
  "projects/beacon": {
    description: describe(projects, "/projects/beacon/"),
    kicker: "Projects",
    title: "Beacon",
    tone: "orange",
  },
  "projects/mote": {
    description: describe(projects, "/projects/mote/"),
    kicker: "Projects",
    title: "Möte",
    tone: "green",
  },
  "projects/orbis": {
    description: describe(projects, "/projects/orbis/"),
    kicker: "Projects",
    title: "Orbis",
    tone: "green",
  },
  "projects/versos": {
    description: describe(projects, "/projects/versos/"),
    kicker: "Projects",
    title: "Versos",
    tone: "orange",
  },
  work: {
    description:
      "Compliance SaaS, healthcare platforms, and landing pages for clients.",
    title: "Work",
    tone: "orange",
  },
  "work/carbon-law-group": {
    description: describe(work, "/work/carbon-law-group/"),
    kicker: "Work",
    title: "Carbon Law Group",
    tone: "green",
  },
  "work/embic": {
    description: describe(work, "/work/embic/"),
    kicker: "Work",
    title: "Embic",
    tone: "orange",
  },
} satisfies Record<string, OgCard>;

export type OgSlug = keyof typeof ogCards;

export const isOgSlug = (slug: string): slug is OgSlug =>
  Object.hasOwn(ogCards, slug);

const toSlug = (pathname: string): string =>
  pathname.replaceAll(/^\/+|\/+$/gu, "") || "index";

/* A missing card stops the build. The old fallback, the home page's card,
   let a new page ship with the wrong link preview and say nothing. */
const resolve = (pathname: string): OgSlug => {
  const slug = toSlug(pathname);
  if (!isOgSlug(slug)) {
    throw new Error(
      `No share card for ${pathname}. Its link previews would have no image of their own. Fix: add "${slug}" to \`ogCards\` in src/data/og.ts.`
    );
  }
  return slug;
};

/** The generated card for a page. Throws when the page has none. */
export const ogImagePath = (pathname: string): string =>
  `/og/${resolve(pathname)}.png`;

/** Alt text that reads out what the card shows. */
export const ogImageAlt = (pathname: string): string => {
  const { kicker, title, quiet, description }: OgCard =
    ogCards[resolve(pathname)];
  return [kicker, [title, quiet].filter(Boolean).join(" "), description]
    .filter(Boolean)
    .join(". ");
};
