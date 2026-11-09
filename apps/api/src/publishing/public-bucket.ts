import * as Cloudflare from "@alchemy.run/cloudflare";

export const PublicBucket = Cloudflare.R2.Bucket("Public");
