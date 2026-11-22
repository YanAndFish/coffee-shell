// Writes `openapi.json` and `fixtures/**` from the zod schemas.
//   node scripts/generate.ts          write the files
//   node scripts/generate.ts --check  exit 1 if any committed file is out of date
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import process from 'node:process'
import { renderGeneratedFiles } from '../src/generate.ts'

const root = join(import.meta.dirname, '..')
const check = process.argv.includes('--check')
const stale: string[] = []

for (const [path, text] of renderGeneratedFiles()) {
  const file = join(root, path)
  let current: string | undefined
  try {
    current = readFileSync(file, 'utf8')
  }
  catch {}
  if (current === text)
    continue
  if (check) {
    stale.push(path)
    continue
  }
  mkdirSync(dirname(file), { recursive: true })
  writeFileSync(file, text)
  console.log(`wrote ${path}`)
}

if (stale.length > 0) {
  console.error(`Out of date (run \`pnpm -F @coffee/contract generate\`): ${stale.join(', ')}`)
  process.exit(1)
}
