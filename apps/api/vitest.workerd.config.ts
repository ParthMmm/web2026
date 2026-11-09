import { defineConfig } from "vitest/config";

export default defineConfig({
  test: { include: ["tests/workerd/**/*.test.ts"], testTimeout: 30_000 },
});
