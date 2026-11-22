import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    // Node packages only: apps/tui's *.test.tsx files belong to `nsh test`, not vitest.
    include: ['{apps/server,packages/*}/{src,test}/**/*.{test,spec}.ts'],
    passWithNoTests: true,
  },
})
