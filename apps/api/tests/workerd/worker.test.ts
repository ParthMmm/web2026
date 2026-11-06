import { afterAll, beforeAll, expect, it } from "vitest";

import { bundleWorker, startWorkerd } from "../../scripts/workerd.ts";

const TOKEN = "t".repeat(48);
let workerd: Awaited<ReturnType<typeof startWorkerd>>;

beforeAll(async () => {
  workerd = await startWorkerd(await bundleWorker(), { OWNER_TOKEN: TOKEN });
}, 120_000);

afterAll(() => workerd?.dispose());

const UNKNOWN = `/v1/photos/${"a".repeat(64)}`;

it("runtime init with only runtime bindings answers an anonymous request with 401", async () => {
  const response = await workerd.dispatchFetch(`http://photos.test${UNKNOWN}`);
  expect(response.status).toBe(401);
  expect(await response.json()).toEqual({ _tag: "Unauthorized" });
});

it("an authenticated request for an unknown photo is a 404", async () => {
  const response = await workerd.dispatchFetch(`http://photos.test${UNKNOWN}`, {
    headers: { authorization: `Bearer ${TOKEN}` },
  });
  expect(response.status).toBe(404);
  expect(await response.json()).toMatchObject({ _tag: "PhotoNotFound" });
});
