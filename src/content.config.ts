import { defineCollection } from 'astro:content';
import { glob } from 'astro/loaders';
import { z } from 'astro/zod';

const projects = defineCollection({
  loader: glob({ pattern: "**/*.md", base: "./src/content/projects" }),

  schema: z.object({
    title: z.string(),
    description: z.string(),
    pubDate: z.coerce.date(),
    updatedDate: z.coerce.date().optional(),
    /** Set on each write-up of a project that has been rebuilt more than once. */
    version: z.string().optional(),
    period: z.string().optional(),
  })
});

const work = defineCollection({
  loader: glob({ pattern: "**/*.md", base: "./src/content/work" }),

  schema: z.object({
    title: z.string(),
    description: z.string(),
    pubDate: z.coerce.date(),
    updatedDate: z.coerce.date().optional(),
  })
});

export const collections = { projects, work };
