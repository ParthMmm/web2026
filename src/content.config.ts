import { glob } from "astro/loaders";
import { z } from "astro/zod";
import { defineCollection } from "astro:content";

const projects = defineCollection({
  loader: glob({ base: "./src/content/projects", pattern: "**/*.md" }),

  schema: z.object({
    /** schema.org SoftwareApplication fields for the write-up's structured data. */
    applicationCategory: z.string().optional(),
    description: z.string(),
    operatingSystem: z.string().optional(),
    period: z.string().optional(),
    pubDate: z.coerce.date(),
    title: z.string(),
    updatedDate: z.coerce.date().optional(),
    /** Set on each write-up of a project that has been rebuilt more than once. */
    version: z.string().optional(),
  }),
});

const work = defineCollection({
  loader: glob({ base: "./src/content/work", pattern: "**/*.md" }),

  schema: z.object({
    description: z.string(),
    pubDate: z.coerce.date(),
    title: z.string(),
    updatedDate: z.coerce.date().optional(),
  }),
});

export const collections = { projects, work };
