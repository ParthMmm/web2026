import * as SigV4 from "@distilled.cloud/aws4signer";
import { Clock, Context, Effect, Layer, Redacted } from "effect";

import { BlobUnavailable } from "./blobs.ts";
import type { BlobKey, Expected } from "./blobs.ts";

/** How long a presigned PUT stays valid. R2 checks it when the request starts. */
export const UPLOAD_URL_SECONDS = 15 * 60;

/** S3 credentials and endpoint for one bucket, the shape Alchemy's `R2.S3Credentials` delivers. */
export interface S3Endpoint {
  readonly endpoint: string;
  readonly bucketName: string;
  readonly region: string;
  readonly accessKeyId: string;
  readonly secretAccessKey: string;
}

export interface SignedPut {
  readonly url: string;
  /** Headers signed into the URL. The uploader must send each one exactly. */
  readonly headers: Readonly<Record<string, string>>;
  /** Epoch milliseconds. */
  readonly expiresAt: number;
}

const base64OfHex = (hex: string) => {
  let binary = "";
  for (let index = 0; index < hex.length; index += 2) {
    binary += String.fromCodePoint(
      Number.parseInt(hex.slice(index, index + 2), 16)
    );
  }
  return btoa(binary);
};

/** SigV4's `YYYYMMDDTHHMMSSZ`. */
const amzDate = (millis: number) =>
  new Date(millis).toISOString().replaceAll(/[:-]|\.\d{3}/gu, "");

/**
 * Presigned PUTs into the private bucket. The URL signs the length and the
 * SHA-256, so the storage service refuses any other bytes before it stores
 * them, as the Worker's own writes do.
 */
export class UploadUrls extends Context.Service<
  UploadUrls,
  {
    readonly signPut: (
      key: BlobKey,
      expect: Expected
    ) => Effect.Effect<SignedPut, BlobUnavailable>;
  }
>()("@web2026/api/UploadUrls") {
  /** Signs path-style URLs (`{endpoint}/{bucket}/{key}`) with the credentials, read on each call. */
  static readonly layerS3 = (
    credentials: Effect.Effect<S3Endpoint>
  ): Layer.Layer<UploadUrls> =>
    Layer.succeed(
      UploadUrls,
      UploadUrls.of({
        signPut: (key, expect) =>
          Effect.gen(function* signPut() {
            const s3 = yield* credentials;
            const now = yield* Clock.currentTimeMillis;
            const path = key.split("/").map(encodeURIComponent).join("/");
            const url = new URL(
              `${s3.endpoint.replace(/\/+$/u, "")}/${encodeURIComponent(s3.bucketName)}/${path}`
            );
            url.searchParams.set("X-Amz-Expires", String(UPLOAD_URL_SECONDS));
            const headers = {
              "content-length": String(expect.bytes),
              "x-amz-checksum-sha256": base64OfHex(expect.sha256),
            };
            const signed = yield* SigV4.sign({
              accessKeyId: s3.accessKeyId,
              allHeaders: true,
              datetime: amzDate(now),
              headers,
              method: "PUT",
              region: s3.region,
              secretAccessKey: Redacted.make(s3.secretAccessKey),
              service: "s3",
              signQuery: true,
              url: url.toString(),
            });
            return {
              expiresAt: now + UPLOAD_URL_SECONDS * 1000,
              headers,
              url: signed.url,
            };
          }).pipe(
            Effect.mapError(
              (cause) => new BlobUnavailable({ cause, operation: "presign" })
            )
          ),
      })
    );
}
