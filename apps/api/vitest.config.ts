import { defineConfig } from "vitest/config";

export default defineConfig({
  test: { exclude: ["tests/workerd/**", "node_modules/**"], include: ["tests/**/*.test.ts"] },
});
