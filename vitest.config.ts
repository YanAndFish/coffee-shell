import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    // TUI 的 *.test.tsx 由 nsh test 运行。
    include: ['{apps/server,packages/*}/{src,test}/**/*.{test,spec}.ts', 'scripts/*.test.ts'],
    passWithNoTests: true,
  },
})
