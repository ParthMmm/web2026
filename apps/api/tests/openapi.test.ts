import { expect, it } from "@effect/vitest";
import { OpenApi } from "effect/unstable/httpapi";

import { Api } from "../src/contract.ts";

it("exports stable operations, bearer security and the real OpenAPI dialect", () => {
  const spec = OpenApi.fromApi(Api);
  expect(spec.openapi).toBe("3.1.0");
  const operations = spec.paths["/v1/probe/photos"];
  expect(operations?.post?.operationId).toBe("probe.echoPhoto");
  expect(operations?.get?.operationId).toBe("probe.listPhotos");
  for (const operation of [operations?.post, operations?.get]) {
    expect(operation?.security).toEqual([{ bearer: [] }]);
    expect(operation?.responses).toHaveProperty("400");
    expect(operation?.responses).toHaveProperty("401");
  }
  expect(spec.components.securitySchemes.bearer).toEqual({
    scheme: "Bearer",
    type: "http",
  });
});

it("adds film simulation without changing the v1 required fields", () => {
  const photo = OpenApi.fromApi(Api).components.schemas.ProbePhoto;
  expect(photo).toMatchObject({
    properties: { filmSimulation: { type: "string" } },
    required: ["capturedAt", "id", "state"],
  });
});
