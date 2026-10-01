import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["test/**/*.test.ts"],
    environment: "node",
    // The parity suite reads the Foundry artifacts and the Rust sources off disk. A stale cache
    // there would be a test passing against a build nobody has anymore, which is worse than slow.
    isolate: true,
  },
});
