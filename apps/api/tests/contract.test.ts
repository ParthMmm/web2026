import { afterAll, expect, it } from "@effect/vitest";

import fixtures from "../fixtures/contract.json";
import { makeHandler } from "../src/handler.ts";

const app = makeHandler("local-test-credential");

it.each(fixtures.invalidPhotos)(
  "rejects invalid JSON contract input %#",
  async (photo) => {
    const response = await app.handler(
      new Request("http://localhost/v1/probe/photos", {
        body: JSON.stringify(photo),
        headers: {
          authorization: "Bearer local-test-credential",
          "content-type": "application/json",
        },
        method: "POST",
      })
    );
    expect(response.status).toBe(400);
    expect(await response.text()).toBe("");
  }
);
afterAll(() => app.dispose());

it("round-trips a photo's optional and nullable fields through HTTP", async () => {
  const photo = {
    capturedAt: null,
    id: "photo-1",
    state: { _tag: "Draft" },
  };
  const response = await app.handler(
    new Request("http://localhost/v1/probe/photos", {
      body: JSON.stringify(photo),
      headers: {
        authorization: "Bearer local-test-credential",
        "content-type": "application/json",
      },
      method: "POST",
    })
  );
  expect(response.status).toBe(200);
  expect(await response.json()).toEqual(photo);
});

it("follows a page cursor and returns a structured error for an unknown cursor", async () => {
  const get = (suffix: string) =>
    app.handler(
      new Request(`http://localhost/v1/probe/photos${suffix}`, {
        headers: { authorization: "Bearer local-test-credential" },
      })
    );
  const first = await get("");
  expect(first.status).toBe(200);
  expect(await first.json()).toEqual({
    items: [{ capturedAt: null, id: "fixture-1", state: { _tag: "Draft" } }],
    nextCursor: "page-2",
  });
  const second = await get("?cursor=page-2");
  expect(await second.json()).toEqual({ items: [], nextCursor: null });
  const invalid = await get("?cursor=missing");
  expect(invalid.status).toBe(400);
  expect(await invalid.json()).toEqual({
    _tag: "InvalidCursor",
    cursor: "missing",
  });
});

it.each([undefined, "Bearer wrong-token"])(
  "rejects missing or incorrect credentials (%s)",
  async (authorization) => {
    const headers = new Headers({ "content-type": "application/json" });
    if (authorization) {
      headers.set("authorization", authorization);
    }
    const response = await app.handler(
      new Request("http://localhost/v1/probe/photos", {
        body: JSON.stringify({
          capturedAt: null,
          id: "photo-1",
          state: { _tag: "Draft" },
        }),
        headers,
        method: "POST",
      })
    );
    expect(response.status).toBe(401);
    expect(await response.json()).toEqual({ _tag: "Unauthorized" });
  }
);

it("round-trips a published photo and a timestamp without leaking Effect values", async () => {
  const photo = {
    caption: "Evening",
    capturedAt: "2025-03-23T18:30:00.000Z",
    id: "photo-2",
    state: {
      _tag: "Published",
      url: "https://images.example.test/photo-2.webp",
    },
  };
  const response = await app.handler(
    new Request("http://localhost/v1/probe/photos", {
      body: JSON.stringify(photo),
      headers: {
        authorization: "Bearer local-test-credential",
        "content-type": "application/json",
      },
      method: "POST",
    })
  );
  expect(response.status).toBe(200);
  expect(await response.json()).toEqual(photo);
});
