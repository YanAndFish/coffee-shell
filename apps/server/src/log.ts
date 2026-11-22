import process from 'node:process'

/** One informational line on stdout, tagged with its source, e.g. `[server]`. */
export function logInfo(tag: string, message: string): void {
  process.stdout.write(`[${tag}] ${message}\n`)
}
