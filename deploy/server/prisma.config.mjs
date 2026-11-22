// The Prisma CLI's settings inside the backend image (deploy/server.Dockerfile), where
// apps/server/prisma.config.ts cannot load: it imports the TypeScript sources, which the
// image leaves out. Same schema and migrations; the database is DATABASE_URL (an absolute
// `file:` URL on the data volume).
import process from 'node:process'
import { defineConfig } from 'prisma/config'

export default defineConfig({
  schema: 'prisma/schema.prisma',
  migrations: {
    path: 'prisma/migrations',
  },
  datasource: {
    url: process.env.DATABASE_URL,
  },
})
