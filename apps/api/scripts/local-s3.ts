import { createHash } from "node:crypto";
import { once } from "node:events";
import { createServer } from "node:http";
import type { IncomingMessage, ServerResponse } from "node:http";

import * as SigV4 from "@distilled.cloud/aws4signer";
import { Effect, Redacted, Schema } from "effect";

/** The env binding Alchemy's `R2.S3Credentials(PrivateBucket, { access: "write" })` reads. */
export const PRIVATE_S3_BINDING = "R2_S3_CREDENTIALS_PRIVATE_WRITE";

const CREDENTIALS = {
  accessKeyId: "local-s3-access-key",
  bucketName: "Private",
  region: "auto",
  secretAccessKey: "local-s3-secret-key",
} as const;

const SIGNATURE = "X-Amz-Signature";

const ListeningAddress = Schema.Struct({ port: Schema.Number });

interface Bucket {
  readonly put: (
    key: string,
    value: Uint8Array,
    options: { readonly sha256: string }
  ) => Promise<object | null>;
}

const xmlError = (
  response: ServerResponse,
  status: number,
  code: string,
  message: string
) => {
  response.writeHead(status, { "content-type": "application/xml" });
  response.end(
    `<?xml version="1.0" encoding="UTF-8"?><Error><Code>${code}</Code><Message>${message}</Message></Error>`
  );
};

/** `YYYYMMDDTHHMMSSZ` as epoch milliseconds. */
const amzMillis = (value: string) =>
  Date.parse(
    `${value.slice(0, 4)}-${value.slice(4, 6)}-${value.slice(6, 8)}T${value.slice(9, 11)}:${value.slice(11, 13)}:${value.slice(13, 15)}Z`
  );

const expectedSignature = (request: IncomingMessage, url: URL) => {
  const unsigned = new URL(url);
  unsigned.searchParams.delete(SIGNATURE);
  const signedHeaders = (unsigned.searchParams.get("X-Amz-SignedHeaders") ?? "")
    .split(";")
    .filter((name) => name !== "host");
  const headers: Record<string, string> = {};
  for (const name of signedHeaders) {
    headers[name] = String(request.headers[name] ?? "");
  }
  return Effect.runPromise(
    SigV4.sign({
      accessKeyId: CREDENTIALS.accessKeyId,
      allHeaders: true,
      datetime: unsigned.searchParams.get("X-Amz-Date") ?? "",
      headers,
      method: request.method ?? "",
      region: CREDENTIALS.region,
      secretAccessKey: Redacted.make(CREDENTIALS.secretAccessKey),
      service: "s3",
      signQuery: true,
      url: unsigned.toString(),
    })
  ).then((signed) => new URL(signed.url).searchParams.get(SIGNATURE));
};

/**
 * A local stand-in for R2's S3 endpoint that answers presigned PUTs the way
 * R2 does: it checks the SigV4 query signature and expiry, refuses bytes
 * whose SHA-256 differs from `x-amz-checksum-sha256` (400 BadDigest), and
 * writes accepted bytes into the bucket Miniflare serves to the Worker.
 * Alchemy's own local S3 endpoint ignores flexible checksums, so it cannot
 * show a wrong-hash refusal.
 */
export const startLocalS3 = async () => {
  let bucket: Bucket | undefined;

  const handle = async (request: IncomingMessage, response: ServerResponse) => {
    const url = new URL(request.url ?? "/", `http://${request.headers.host}`);
    const prefix = `/${CREDENTIALS.bucketName}/`;
    if (request.method !== "PUT" || !url.pathname.startsWith(prefix)) {
      return xmlError(response, 405, "MethodNotAllowed", "Only object PUT");
    }
    const date = url.searchParams.get("X-Amz-Date") ?? "";
    const expires = Number(url.searchParams.get("X-Amz-Expires"));
    if (Date.now() > amzMillis(date) + expires * 1000) {
      return xmlError(response, 403, "AccessDenied", "Request has expired");
    }
    if (
      url.searchParams.get(SIGNATURE) !==
      (await expectedSignature(request, url))
    ) {
      return xmlError(
        response,
        403,
        "SignatureDoesNotMatch",
        "The request signature does not match"
      );
    }
    const hash = createHash("sha256");
    const chunks: Buffer[] = [];
    for await (const chunk of request) {
      hash.update(chunk);
      chunks.push(chunk);
    }
    const body = Buffer.concat(chunks);
    const digest = hash.digest();
    if (body.length !== Number(request.headers["content-length"])) {
      return xmlError(response, 400, "IncompleteBody", "Short body");
    }
    if (
      request.headers["x-amz-checksum-sha256"] !== digest.toString("base64")
    ) {
      return xmlError(
        response,
        400,
        "BadDigest",
        "The SHA256 you specified did not match the calculated checksum."
      );
    }
    if (bucket === undefined) {
      return xmlError(response, 503, "ServiceUnavailable", "No bucket yet");
    }
    await bucket.put(
      decodeURIComponent(url.pathname.slice(prefix.length)),
      body,
      {
        sha256: digest.toString("hex"),
      }
    );
    response.writeHead(200, { etag: `"${digest.toString("hex")}"` });
    response.end();
  };

  const server = createServer(async (request, response) => {
    try {
      await handle(request, response);
    } catch (error) {
      xmlError(response, 500, "InternalError", String(error));
    }
  });
  server.listen(0, "127.0.0.1");
  await once(server, "listening");
  const { port } = Schema.decodeUnknownSync(ListeningAddress)(server.address());
  return {
    /** Accepted bytes go here; until then every PUT answers 503. */
    attach: (target: Bucket) => {
      bucket = target;
    },
    /** Give this to the Worker so it signs URLs for this server. */
    binding: {
      [PRIVATE_S3_BINDING]: JSON.stringify({
        ...CREDENTIALS,
        endpoint: `http://127.0.0.1:${port}`,
      }),
    },
    close: async () => {
      server.close();
      await once(server, "close");
    },
  };
};
