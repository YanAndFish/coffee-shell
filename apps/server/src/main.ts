import process from 'node:process'
import { createApp } from './app.ts'
import { createPrisma } from './db.ts'
import { loadEnv } from './env.ts'
import { logInfo } from './log.ts'

const env = loadEnv()
const prisma = createPrisma(env.databaseUrl)
const app = createApp({ prisma, internalKey: env.internalKey })

const server = app.listen(env.port, env.host, () => {
  const address = server.address()
  const port = typeof address === 'object' && address
    ? address.port
    : env.port
  logInfo('server', `ROAST/TTY 演示后端 listening on http://${env.host}:${port}`)
  logInfo('server', `database ${env.databaseUrl}; internal key ${env.internalKey ? 'required' : 'off'}`)
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
