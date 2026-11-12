import { Effect, Layer, Logger, Redacted } from "effect";
import { Digest } from "effect/crypto";
import { HttpRouter } from "effect/http";
import { afterEach, expect, it } from "vitest";

import type { BeginRequest } from "../src/http/contract.ts";
import { photosApp } from "../src/http/handlers.ts";
import { makeMemoryBlobs } from "../src/photos/blobs-memory.ts";
import { variantKinds } from "../src/photos/domain.ts";
import type { PhotoDetails } from "../src/photos/domain.ts";
import { Photos } from "../src/photos/photos.ts";
import { PublicGallery } from "../src/photos/public-gallery.ts";
import { Publishing } from "../src/photos/publishing.ts";
import { declare, fixture, validBodies } from "./support/fixture.ts";
import { migratedSqlite } from "./support/sqlite.ts";

const TOKEN = "owner-token-for-tests-only-0123456789";
const ORIGIN = "http://photos.test";

const disposers: (() => Promise<void>)[] = [];
afterEach(async () => {
  await Promise.all(disposers.splice(0).map((dispose) => dispose()));
});

const serve = () => {
  const lines: string[] = [];
  const capture = Logger.make((options) => {
    lines.push(Logger.formatJson.log(options));
  });
  const privateBlobs = makeMemoryBlobs();
  const publicBlobs = makeMemoryBlobs();
  const owner = Layer.mergeAll(Photos.layer, Publishing.layer).pipe(
    Layer.provide([
      privateBlobs.layer,
      publicBlobs.publicLayer,
      migratedSqlite(),
    ])
  );
  const gallery = PublicGallery.layer.pipe(
    Layer.provide(publicBlobs.publicLayer)
  );
  const { dispose, handler } = HttpRouter.toWebHandler(
    Layer.merge(
      photosApp(owner, gallery, Redacted.make(TOKEN)),
      Logger.layer([capture])
    )
  );
  disposers.push(dispose);
  const call = (
    path: string,
    init: RequestInit = {},
    token: string | null = TOKEN
  ) => {
    const headers = new Headers(init.headers);
    if (token !== null) {
      headers.set("authorization", `Bearer ${token}`);
    }
    return handler(new Request(`${ORIGIN}${path}`, { ...init, headers }));
  };
  const statusOf = (...args: Parameters<typeof call>) =>
    call(...args).then((response) => response.status);
  const jsonOf = (...args: Parameters<typeof call>) =>
    call(...args).then((response) => response.json());
  return { call, jsonOf, lines, statusOf };
};

const json = (body: typeof BeginRequest.Encoded): RequestInit => ({
  body: JSON.stringify(body),
  headers: { "content-type": "application/json" },
  method: "POST",
});

const put = (body: Uint8Array): RequestInit => ({
  body: Uint8Array.from(body),
  headers: {
    "content-length": String(body.length),
    "content-type": "application/octet-stream",
  },
  method: "PUT",
});

const photo = (seed?: number) =>
  Effect.runPromise(
    (seed === undefined ? fixture() : declare(validBodies(seed))).pipe(
      Effect.provide(Digest.layerWebCrypto)
    )
  );

const post = (body?: typeof PhotoDetails.Encoded): RequestInit =>
  body === undefined
    ? { method: "POST" }
    : {
        body: JSON.stringify(body),
        headers: { "content-type": "application/json" },
        method: "POST",
      };

type Call = ReturnType<typeof serve>["call"];

const uploadDraft = async (call: Call, seed?: number) => {
  const declared = await photo(seed);
  const { bodies, manifest, photoId } = declared;
  await call("/v1/photos", json({ manifest, photoId }));
  for (const kind of variantKinds) {
    // oxlint-disable-next-line no-await-in-loop -- uploads in order keep the test readable
    await call(`/v1/photos/${photoId}/${kind}`, put(bodies[kind]));
  }
  const finalized = await call(`/v1/photos/${photoId}/finalize`, post());
  expect(finalized.status).toBe(200);
  return declared;
};

const publicHeaders = (response: Response) => ({
  cacheControl: response.headers.get("cache-control"),
  cors: response.headers.get("access-control-allow-origin"),
  type: response.headers.get("content-type"),
});

it("rejects a missing or wrong bearer token with 401 Unauthorized", async () => {
  const { call } = serve();
  const { photoId } = await photo();
  const responses = await Promise.all(
    [null, "x".repeat(TOKEN.length)].map((token) =>
      call(`/v1/photos/${photoId}`, {}, token)
    )
  );
  for (const response of responses) {
    expect(response.status).toBe(401);
  }
  const bodies = await Promise.all(
    responses.map((response) => response.json())
  );
  expect(bodies).toEqual([{ _tag: "Unauthorized" }, { _tag: "Unauthorized" }]);
});

it("rejects a photoId that is not the original's sha256 with 400", async () => {
  const { call } = serve();
  const { manifest } = await photo();
  const response = await call(
    "/v1/photos",
    json({ manifest, photoId: "b".repeat(64) })
  );
  expect(response.status).toBe(400);
  const malformed = await call("/v1/photos/not-a-hash");
  expect(malformed.status).toBe(400);
});

it("uploads every object over HTTP and finalizes the photo into a draft", async () => {
  const { call } = serve();
  const { bodies, manifest, photoId } = await photo();

  const begun = await call("/v1/photos", json({ manifest, photoId }));
  expect(begun.status).toBe(200);
  expect(await begun.json()).toMatchObject({
    _tag: "Uploading",
    missing: variantKinds,
  });

  const stored = await Promise.all(
    variantKinds.map((kind) =>
      call(`/v1/photos/${photoId}/${kind}`, put(bodies[kind]))
    )
  );
  expect(stored.map((response) => response.status)).toEqual(
    variantKinds.map(() => 204)
  );

  const finalized = await call(`/v1/photos/${photoId}/finalize`, {
    method: "POST",
  });
  expect(finalized.status).toBe(200);
  const draft = await finalized.json();
  expect(draft).toMatchObject({ _tag: "Draft", manifest, photoId });

  const again = await call(`/v1/photos/${photoId}/finalize`, {
    method: "POST",
  });
  expect(await again.json()).toEqual(draft);
});

it("answers a body with the wrong hash with 422 HashMismatch", async () => {
  const { call } = serve();
  const { bodies, manifest, photoId } = await photo();
  await call("/v1/photos", json({ manifest, photoId }));
  const tampered = Uint8Array.from(bodies.w480);
  tampered[tampered.length - 1] = 0xff - (tampered.at(-1) ?? 0);

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

it("serves an empty gallery to anyone before the first publish", async () => {
  const { call } = serve();
  const response = await call("/gallery/v1.json", {}, null);
  expect(response.status).toBe(200);
  expect(publicHeaders(response)).toEqual({
    cacheControl: "public, max-age=60",
    cors: "*",
    type: "application/json",
  });
  expect(await response.json()).toMatchObject({ photos: [], version: 1 });
});

it("publishes a draft, serves its manifest and derivatives without a token, and unpublishes it", async () => {
  const { call, jsonOf } = serve();
  const { bodies, manifest, photoId } = await uploadDraft(call);
  const details = {
    alt: "Fog",
    capturedAt: "2026-10-03T07:12:00",
    film: "HP5",
  };

  const published = await call(`/v1/photos/${photoId}/publish`, post(details));
  expect(published.status).toBe(200);
  const view = await published.json();
  expect(view).toMatchObject({ _tag: "Published", details, photoId });
  const again = await call(`/v1/photos/${photoId}/publish`, post(details));
  expect(await again.json()).toEqual(view);

  const gallery = await call("/gallery/v1.json", {}, null);
  const listed = await gallery.json();
  expect(listed.photos).toHaveLength(1);
  const { path } = listed.photos[0].sizes.w480;
  expect(path).toBe(`photos/${photoId}/w480-${manifest.w480.sha256}.webp`);

  const image = await call(`/gallery/${path}`, {}, null);
  expect(image.status).toBe(200);
  expect(publicHeaders(image)).toEqual({
    cacheControl: "public, max-age=31536000, immutable",
    cors: "*",
    type: "image/webp",
  });
  expect(new Uint8Array(await image.arrayBuffer())).toEqual(bodies.w480);

  const unpublished = await call(`/v1/photos/${photoId}/unpublish`, post());
  expect(unpublished.status).toBe(200);
  expect(await unpublished.json()).toMatchObject({ _tag: "Draft", photoId });
  expect(await jsonOf("/gallery/v1.json", {}, null)).toMatchObject({
    photos: [],
  });
  const gone = await call(`/gallery/${path}`, {}, null);
  expect(gone.status).toBe(404);
  expect(gone.headers.get("cache-control")).toBe("no-store");
});

it("never serves an original, a draft's derivative, or a path outside the derivative scheme", async () => {
  const { call, statusOf } = serve();
  const draft = await uploadDraft(call, 1);
  const shown = await uploadDraft(call, 2);
  await call(`/v1/photos/${shown.photoId}/publish`, post({}));

  const paths = [
    `/gallery/photos/${shown.photoId}/original-${shown.manifest.original.sha256}.webp`,
    `/gallery/photos/${shown.photoId}/original-${shown.manifest.original.sha256}`,
    `/gallery/photos/${draft.photoId}/w480-${draft.manifest.w480.sha256}.webp`,
    `/gallery/photos/${shown.photoId}/w480-${shown.manifest.w480.sha256}`,
    `/gallery/photos/${shown.photoId}/w480-${shown.manifest.w1200.sha256}.webp`,
    "/gallery/photos/not-a-photo/w480-x.webp",
  ];
  const statuses = await Promise.all(
    paths.map((path) => statusOf(path, {}, null))
  );
  expect(statuses).toEqual(paths.map(() => 404));
  const shownImage = await call(
    `/gallery/photos/${shown.photoId}/w480-${shown.manifest.w480.sha256}.webp`,
    {},
    null
  );
  expect(shownImage.status).toBe(200);
});

it("guards publish and unpublish with the owner token, the photo's state and the details' limits", async () => {
  const { call } = serve();
  const draft = await uploadDraft(call, 1);
  const uploading = await photo(2);
  await call(
    "/v1/photos",
    json({ manifest: uploading.manifest, photoId: uploading.photoId })
  );

  const anonymous = await call(
    `/v1/photos/${draft.photoId}/publish`,
    post({}),
    null
  );
  expect(anonymous.status).toBe(401);
  const notDraft = await call(
    `/v1/photos/${uploading.photoId}/publish`,
    post({})
  );
  expect(notDraft.status).toBe(409);
  expect(await notDraft.json()).toEqual({
    _tag: "PhotoNotDraft",
    photoId: uploading.photoId,
  });
  const unpublishing = await call(
    `/v1/photos/${uploading.photoId}/unpublish`,
    post()
  );
  expect(unpublishing.status).toBe(409);
  const tooLong = await call(
    `/v1/photos/${draft.photoId}/publish`,
    post({ alt: "a".repeat(301) })
  );
  expect(tooLong.status).toBe(400);
  const longest = await call(
    `/v1/photos/${draft.photoId}/publish`,
    post({
      alt: "a".repeat(300),
      capturedAt: "c".repeat(40),
      film: "f".repeat(64),
    })
  );
  expect(longest.status).toBe(200);
  const unpublishedDraft = await call(
    `/v1/photos/${draft.photoId}/unpublish`,
    post(),
    null
  );
  expect(unpublishedDraft.status).toBe(401);
});

it("lists photos one page at a time and refuses a limit outside 1 to 200", async () => {
  const { call, jsonOf, statusOf } = serve();
  const ids = [];
  for (const seed of [1, 2, 3]) {
    // oxlint-disable-next-line no-await-in-loop -- each photo is declared in turn
    const { manifest, photoId } = await photo(seed);
    // oxlint-disable-next-line no-await-in-loop -- each photo is declared in turn
    await call("/v1/photos", json({ manifest, photoId }));
    ids.push(photoId);
  }
  ids.sort();

  const first = await jsonOf("/v1/photos?limit=2");
  expect(first.photos.map((view: { photoId: string }) => view.photoId)).toEqual(
    ids.slice(0, 2)
  );
  expect(first.next).toBe(ids[1]);
  const second = await jsonOf(`/v1/photos?limit=2&after=${first.next}`);
  expect(second).toMatchObject({ next: null, photos: [{ photoId: ids[2] }] });
  const all = await jsonOf("/v1/photos");
  expect(all.photos).toHaveLength(3);

  const refused = await Promise.all(
    ["?limit=0", "?limit=201", "?limit=two", "?after=nope"].map((query) =>
      statusOf(`/v1/photos${query}`)
    )
  );
  expect(refused).toEqual([400, 400, 400, 400]);
  expect(await statusOf("/v1/photos", {}, null)).toBe(401);
});
