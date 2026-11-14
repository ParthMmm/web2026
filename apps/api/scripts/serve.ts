import { startLocalS3 } from "./local-s3.ts";
import { bundleWorker, startWorkerd } from "./workerd.ts";

const token = process.env.OWNER_TOKEN;
if (!token) {
  throw new Error("Set a non-production OWNER_TOKEN of at least 32 characters");
}
const s3 = await startLocalS3();
const workerd = await startWorkerd(
  await bundleWorker(),
  { OWNER_TOKEN: token, ...s3.binding },
  Number(process.env.PORT ?? 8787)
);
s3.attach(await workerd.getR2Bucket("Private"));
console.log(`Local photos Worker in workerd: ${await workerd.ready}`);
process.once("SIGTERM", async () => {
  await workerd.dispose();
  await s3.close();
  process.exit(0);
});
