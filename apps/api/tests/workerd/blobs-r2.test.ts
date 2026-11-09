import { afterAll, beforeAll, expect, it } from "@effect/vitest";
import type { Miniflare } from "miniflare";

import { blobsContract } from "../support/blobs-contract.ts";
import { bundle, startWorkerd } from "../../scripts/workerd.ts";
import { photosScenarios } from "./scenarios.ts";

let miniflare: Miniflare;

beforeAll(async () => {
  miniflare = await startWorkerd(
    await bundle({ path: new URL("contract-worker.ts", import.meta.url).pathname }),
  );
}, 60_000);

afterAll(() => miniflare.dispose());

const runCase = async (suite: string, name: string) => {
  const response = await miniflare.dispatchFetch(
    `http://workerd.test/${suite}?case=${encodeURIComponent(name)}`,
  );
  return response.json();
};

it.each(blobsContract.map(({ name }) => name))("r2 adapter in workerd: %s", async (name) => {
  expect(await runCase("blobs", name)).toEqual({ ok: true });
});

it.each(photosScenarios.map(({ name }) => name))(
  "photos on R2 and D1 in workerd: %s",
  async (name) => {
    expect(await runCase("photos", name)).toEqual({ ok: true });
  },
);
