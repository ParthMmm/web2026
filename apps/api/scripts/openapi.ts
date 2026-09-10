import SwaggerParser from "@apidevtools/swagger-parser";
import { OpenApi } from "effect/unstable/httpapi";

import { Api } from "../src/contract.ts";

const document = OpenApi.fromApi(Api);
const text = `${JSON.stringify(document, null, 2)}\n`;
await SwaggerParser.validate(JSON.parse(text));
const path = new URL("../openapi.json", import.meta.url);
if (process.argv.includes("--check")) {
  if ((await Bun.file(path).text()) !== text) {
    throw new Error("OpenAPI drift: run bun run openapi");
  }
} else {
  await Bun.write(path, text);
}
console.log(`Validated OpenAPI ${document.openapi}`);
