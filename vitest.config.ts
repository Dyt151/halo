import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // sanitize 用 DOMParser，其余是纯函数；统一 jsdom 最省事。
    environment: "jsdom",
    include: ["src/**/*.test.ts"],
  },
});
