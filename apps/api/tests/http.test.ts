import { Effect, Layer, Logger, Redacted } from "effect";
import { Digest } from "effect/crypto";
import { HttpRouter } from "effect/http";
import { afterEach, expect, it } from "vitest";

import { photosApp } from "../src/http/handlers.ts";
import { makeMemoryBlobs } from "../src/photos/blobs-memory.ts";
import { variantKinds } from "../src/photos/domain.ts";
import { Photos } from "../src/photos/photos.ts";
import { fixture } from "./support/fixture.ts";
import { migratedSqlite } from "./support/sqlite.ts";

const TOKEN = "owner-token-for-tests-only-0123456789";
const ORIGIN = "http://photos.test";

const disposers: Array<() => Promise<void>> = [];
afterEach(async () => {
  await Promise.all(disposers.splice(0).map((dispose) => dispose()));
});

const serve = () => {
  const lines: Array<string> = [];
  const capture = Logger.make((options) => {
    lines.push(Logger.formatJson.log(options));
  });
  const photos = Photos.layer.pipe(
    Layer.provide([makeMemoryBlobs().layer, migratedSqlite()]),
    Layer.provide(Digest.layerWebCrypto),
  );
  const { dispose, handler } = HttpRouter.toWebHandler(
    Layer.merge(photosApp(photos, Redacted.make(TOKEN)), Logger.layer([capture])),
  );
  disposers.push(dispose);
  const call = (path: string, init: RequestInit = {}, token: string | null = TOKEN) => {
    const headers = new Headers(init.headers);
    if (token !== null) {
      headers.set("authorization", `Bearer ${token}`);
    }
    return handler(new Request(`${ORIGIN}${path}`, { ...init, headers }));
  };
  return { call, lines };
};

const json = (body: unknown): RequestInit => ({
  body: JSON.stringify(body),
  headers: { "content-type": "application/json" },
  method: "POST",
});

const put = (body: Uint8Array): RequestInit => ({
  body: Uint8Array.from(body),
  headers: { "content-length": String(body.length), "content-type": "application/octet-stream" },
  method: "PUT",
});

const photo = () => Effect.runPromise(fixture().pipe(Effect.provide(Digest.layerWebCrypto)));

it("rejects a missing or wrong bearer token with 401 Unauthorized", async () => {
  const { call } = serve();
  const { photoId } = await photo();
  for (const token of [null, "x".repeat(TOKEN.length)]) {
    const response = await call(`/v1/photos/${photoId}`, {}, token);
    expect(response.status).toBe(401);
    expect(await response.json()).toEqual({ _tag: "Unauthorized" });
  }
});

it("rejects a photoId that is not the original's sha256 with 400", async () => {
  const { call } = serve();
  const { manifest } = await photo();
  const response = await call("/v1/photos", json({ manifest, photoId: "b".repeat(64) }));
  expect(response.status).toBe(400);
  const malformed = await call("/v1/photos/not-a-hash");
  expect(malformed.status).toBe(400);
});

it("uploads every object over HTTP and finalizes the photo into a draft", async () => {
  const { call } = serve();
  const { bodies, manifest, photoId } = await photo();

  const begun = await call("/v1/photos", json({ manifest, photoId }));
  expect(begun.status).toBe(200);
  expect(await begun.json()).toMatchObject({ _tag: "Uploading", missing: variantKinds });

  for (const kind of variantKinds) {
    const stored = await call(`/v1/photos/${photoId}/${kind}`, put(bodies[kind]));
    expect(stored.status).toBe(204);
  }

  const finalized = await call(`/v1/photos/${photoId}/finalize`, { method: "POST" });
  expect(finalized.status).toBe(200);
  const draft = await finalized.json();
  expect(draft).toMatchObject({ _tag: "Draft", manifest, photoId });

  const again = await call(`/v1/photos/${photoId}/finalize`, { method: "POST" });
  expect(await again.json()).toEqual(draft);
});

it("answers a body with the wrong hash with 422 HashMismatch", async () => {
  const { call } = serve();
  const { bodies, manifest, photoId } = await photo();
  await call("/v1/photos", json({ manifest, photoId }));
  const tampered = Uint8Array.from(bodies.w480);
  tampered[tampered.length - 1] ^= 0xff;

  const response = await call(`/v1/photos/${photoId}/w480`, put(tampered));
  expect(response.status).toBe(422);
  expect(await response.json()).toEqual({ _tag: "HashMismatch", kind: "w480" });
});

it("logs one request line per call with the operation and outcome, and never the token", async () => {
  const { call, lines } = serve();
  const { photoId } = await photo();
  await call(`/v1/photos/${photoId}`);

  const requests = lines.filter((line) => line.includes('"request"'));
  expect(requests).toHaveLength(1);
  expect(JSON.parse(requests[0] ?? "{}")).toMatchObject({
    annotations: { operation: "photos.get", photoId },
    message: ["request", { outcome: "PhotoNotFound" }],
  });
  expect(lines.join("\n")).not.toContain(TOKEN);
});
