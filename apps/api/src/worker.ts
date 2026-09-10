import { makeHandler } from "./handler.ts";

export default {
  async fetch(
    request: Request,
    env: { PROBE_TOKEN: string }
  ): Promise<Response> {
    // Keep Effect scopes within one workerd request; no cross-request I/O promises.
    const app = makeHandler(env.PROBE_TOKEN);
    try {
      return await app.handler(request);
    } finally {
      await app.dispose();
    }
  },
};
