import { Effect } from "effect";

import { UploadUrls } from "../../src/photos/upload-urls.ts";
import type { S3Endpoint } from "../../src/photos/upload-urls.ts";

export const TEST_S3: S3Endpoint = {
  accessKeyId: "test-access-key",
  bucketName: "private",
  endpoint: "http://r2.test",
  region: "auto",
  secretAccessKey: "test-secret-key",
};

/** Signs URLs for a bucket that no test sends bytes to. */
export const testUploadUrls = UploadUrls.layerS3(Effect.succeed(TEST_S3));
