import { bundleWorker, startWorkerd } from "./workerd.ts";

const token = process.env.OWNER_TOKEN;
if (!token) {
  throw new Error("Set a non-production OWNER_TOKEN of at least 32 characters");
}
const workerd = await startWorkerd(
  await bundleWorker(),
  { OWNER_TOKEN: token },
  Number(process.env.PORT ?? 8787),
);
console.log(`Local photos Worker in workerd: ${await workerd.ready}`);
process.once("SIGTERM", async () => {
  await workerd.dispose();
  process.exit(0);
});
