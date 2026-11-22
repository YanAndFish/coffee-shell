import { defineConfig } from 'tsdown'

export default defineConfig({
  entry: ['src/main.ts'],
  format: 'esm',
  platform: 'node',
  // npm dependencies stay external (installed next to dist/); the workspace contract is
  // TypeScript source, so it is bundled.
  deps: {
    alwaysBundle: ['@coffee/contract'],
  },
})
