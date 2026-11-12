import { expect, it } from "@effect/vitest";
import { OpenApi } from "effect/http-api";

import { Api } from "../src/http/contract.ts";

const spec = OpenApi.fromApi(Api);

const operations = [
  spec.paths["/v1/photos"]?.post,
  spec.paths["/v1/photos"]?.get,
  spec.paths["/v1/photos/{photoId}"]?.get,
  spec.paths["/v1/photos/{photoId}/{kind}"]?.put,
  spec.paths["/v1/photos/{photoId}/finalize"]?.post,
  spec.paths["/v1/photos/{photoId}/publish"]?.post,
  spec.paths["/v1/photos/{photoId}/unpublish"]?.post,
];

it("exports seven stable operations behind bearer security in the real OpenAPI dialect", () => {
  expect(spec.openapi).toBe("3.1.0");
  expect(operations.map((operation) => operation?.operationId)).toEqual([
    "photos.begin",
    "photos.list",
    "photos.get",
    "photos.put",
    "photos.finalize",
    "photos.publish",
    "photos.unpublish",
  ]);
  for (const operation of operations) {
    expect(operation?.security).toEqual([{ bearer: [] }]);
    expect(operation?.responses).toHaveProperty("401");
  }
  expect(spec.components.securitySchemes.bearer).toEqual({
    scheme: "Bearer",
    type: "http",
  });
});

it("takes each object as a raw octet-stream body", () => {
  const put = spec.paths["/v1/photos/{photoId}/{kind}"]?.put;
  expect(Object.keys(put?.requestBody?.content ?? {})).toEqual([
    "application/octet-stream",
  ]);
  expect(put?.responses).toHaveProperty("204");
  expect(put?.responses).toHaveProperty("422");
});

it("leaves the unauthenticated gallery reads out of the owner's contract", () => {
  expect(
    Object.keys(spec.paths).filter((path) => path.startsWith("/gallery"))
  ).toEqual([]);
});
