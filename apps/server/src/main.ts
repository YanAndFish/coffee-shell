import process from 'node:process'
import { createApp } from './app.ts'
import { createPrisma } from './db.ts'
import { loadEnv } from './env.ts'
import { logInfo } from './log.ts'
import { catalogIsEmpty, databaseProblem, missingDatabase } from './startup.ts'

/** One line on stderr, then exit 1: nothing listens. */
function fatal(message: string): never {
  process.stderr.write(`[server] ✗ ${message}\n`)
  process.exit(1)
}

async function main(): Promise<void> {
  const env = loadEnv()
  // A database the backend cannot use stops it here, before /v1/health answers.
  const missing = missingDatabase(env.databaseUrl)
  if (missing)
    fatal(missing)
  const prisma = createPrisma(env.databaseUrl)
  const problem = await databaseProblem(prisma, env.databaseUrl)
  if (problem) {
    await prisma.$disconnect().catch(() => {})
    fatal(problem)
  }
  if (await catalogIsEmpty(prisma))
    logInfo('server', 'the catalogue is empty: run `pnpm --filter @coffee/server db:seed` (pnpm dev does it)')
  const app = createApp({ prisma, internalKey: env.internalKey })

  const server = app.listen(env.port, env.host, () => {
    const address = server.address()
    const port = typeof address === 'object' && address
      ? address.port
      : env.port
    logInfo('server', `ROAST/TTY 演示后端 listening on http://${env.host}:${port}`)
    logInfo('server', `database ${env.databaseUrl}; internal key ${env.internalKey ? 'required' : 'off'}`)
  })
  server.on('error', (error: NodeJS.ErrnoException) => {
    fatal(error.code === 'EADDRINUSE'
      ? `${env.host}:${env.port} is already in use: stop what holds it or set PORT`
      : `cannot listen on ${env.host}:${env.port}: ${error.message}`)
  })

  let closing = false
  function shutdown(signal: string): void {
    if (closing)
      return
    closing = true
    logInfo('server', `${signal}: shutting down`)
    server.close(() => {
      prisma.$disconnect().finally(() => process.exit(0))
    })
    server.closeAllConnections()
    setTimeout(() => process.exit(1), 5000).unref()
  }

  process.on('SIGINT', () => shutdown('SIGINT'))
  process.on('SIGTERM', () => shutdown('SIGTERM'))
}

main().catch((error: unknown) => fatal(error instanceof Error
  ? error.message
  : String(error)))
