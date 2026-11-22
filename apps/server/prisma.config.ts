// Prisma CLI configuration. Prisma 7 does not load `.env`: set DATABASE_URL in the shell, or
// rely on the default SQLite file `apps/server/.data/coffee.db`.
import process from 'node:process'
import { defineConfig } from 'prisma/config'
import { ensureDatabaseDir, resolveDatabaseUrl } from './src/env.ts'

const url = resolveDatabaseUrl(process.env.DATABASE_URL || undefined)
ensureDatabaseDir(url)

export default defineConfig({
  schema: 'prisma/schema.prisma',
  migrations: {
    path: 'prisma/migrations',
    seed: 'node prisma/seed.ts',
  },
  datasource: { url },
})
