#!/usr/bin/env node
// `pnpm dev`: the whole demo on this machine.
//
// 1. Builds `coffee-tui` (crate `coffee-host`) and `coffee-sshd` with Cargo.
// 2. Starts the backend (`pnpm --filter @coffee/server dev`) and waits for `/v1/health`.
// 3. Starts `coffee-sshd`, which runs one `coffee-tui` per SSH session on a PTY.
// 4. Prints how to connect, prefixes both logs, and stops both on Ctrl+C.
//
// Both processes share a random `COFFEE_INTERNAL_KEY` made for this run only (never printed).
// Options: `--skip-build` reuses the binaries already in Cargo's target directory.
// Environment: HOST, PORT (backend, default 127.0.0.1:3000), COFFEE_SSH_ADDR
// (default 127.0.0.1:2222), COFFEE_SSH_HOST_KEY (default .data/ssh_host_ed25519_key),
// COFFEE_COLORS (passed on to every session).

import { spawn, spawnSync } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync } from 'node:fs'
import { connect } from 'node:net'
import { dirname, join, resolve } from 'node:path'
import process from 'node:process'
import { createInterface } from 'node:readline'
import { setTimeout as sleep } from 'node:timers/promises'
import { fileURLToPath } from 'node:url'

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const color = process.stdout.isTTY && !process.env.NO_COLOR
function paint(code, text) {
  return color
    ? `\x1B[${code}m${text}\x1B[0m`
    : text
}
const tags = {
  dev: paint('1;33', '[dev]   '),
  server: paint('36', '[server]'),
  sshd: paint('35', '[sshd]  '),
}

function log(message) {
  console.log(`${tags.dev} ${message}`)
}

function fail(message, hint) {
  console.error(`${tags.dev} ${paint('31', `✗ ${message}`)}`)
  if (hint)
    console.error(`${tags.dev}   ${hint}`)
  process.exit(1)
}

const args = new Set(process.argv.slice(2))
const skipBuild = args.has('--skip-build')

const apiHost = process.env.HOST || '127.0.0.1'
const apiPort = Number(process.env.PORT || 3000)
const apiUrl = `http://${apiHost.includes(':')
  ? `[${apiHost}]`
  : apiHost}:${apiPort}`
const sshAddr = process.env.COFFEE_SSH_ADDR || '127.0.0.1:2222'
const sshPort = Number(sshAddr.slice(sshAddr.lastIndexOf(':') + 1))
const sshHost = sshAddr.slice(0, sshAddr.lastIndexOf(':')).replace(/^\[|\]$/g, '')
const hostKey = resolve(root, process.env.COFFEE_SSH_HOST_KEY || '.data/ssh_host_ed25519_key')
const internalKey = randomBytes(24).toString('base64url')

// --- Preconditions -------------------------------------------------------------------------

const serverManifest = join(root, 'apps/server/package.json')
const serverScripts = existsSync(serverManifest)
  ? JSON.parse(readFileSync(serverManifest, 'utf8')).scripts ?? {}
  : {}
if (!serverScripts.dev) {
  fail(
    'the backend is not ready: @coffee/server has no "dev" script.',
    'It lives in apps/server; run `pnpm install` once it exists.',
  )
}

if (!existsSync(join(root, 'node_modules')))
  fail('dependencies are not installed.', 'Run `pnpm install` first.')

const cargo = spawnSync('cargo', [
  'metadata',
  '--format-version',
  '1',
  '--no-deps',
], {
  cwd: root,
  encoding: 'utf8',
  maxBuffer: 64 * 1024 * 1024,
})
if (cargo.error || cargo.status !== 0) {
  fail(
    'cargo is not available or the workspace does not load.',
    'Install Rust with rustup; rust-toolchain.toml pins the version.',
  )
}
const targetDir = JSON.parse(cargo.stdout).target_directory
const exe = process.platform === 'win32'
  ? '.exe'
  : ''
const tuiBin = join(targetDir, 'debug', `coffee-tui${exe}`)
const sshdBin = join(targetDir, 'debug', `coffee-sshd${exe}`)

for (const [port, host, what, variable] of [
  [
    apiPort,
    apiHost,
    'the backend',
    'PORT',
  ],
  [
    sshPort,
    sshHost,
    'coffee-sshd',
    'COFFEE_SSH_ADDR',
  ],
]) {
  if (await listening(host, port))
    fail(`${host}:${port} is already in use, so ${what} cannot listen there.`, `Stop what holds it or set ${variable}.`)
}

// --- Build ---------------------------------------------------------------------------------

if (skipBuild) {
  for (const bin of [tuiBin, sshdBin]) {
    if (!existsSync(bin))
      fail(`${bin} is not built.`, 'Run `pnpm dev` without --skip-build, or `cargo build -p coffee-host -p coffee-sshd`.')
  }
}
else {
  log('building coffee-tui and coffee-sshd (cargo build -p coffee-host -p coffee-sshd)…')
  const build = spawnSync('cargo', [
    'build',
    '-p',
    'coffee-host',
    '-p',
    'coffee-sshd',
  ], {
    cwd: root,
    stdio: 'inherit',
  })
  if (build.status !== 0)
    fail('cargo build failed (see above).')
}

mkdirSync(dirname(hostKey), { recursive: true })

// --- Run -----------------------------------------------------------------------------------

/** @type {Map<string, import('node:child_process').ChildProcess>} */
const children = new Map()
let stopping = false

function start(name, command, commandArgs, env) {
  const child = spawn(command, commandArgs, {
    cwd: root,
    env: {
      ...process.env,
      ...env,
    },
    stdio: ['ignore', 'pipe', 'pipe'],
    // Its own process group, so stopping it also stops what it started (pnpm → node).
    detached: process.platform !== 'win32',
  })
  children.set(name, child)
  for (const stream of [child.stdout, child.stderr]) {
    createInterface({ input: stream }).on('line', (line) => {
      // While stopping, pnpm only reports that its child got SIGTERM: not worth showing.
      if (!(stopping && command === 'pnpm'))
        console.log(`${tags[name]} ${line}`)
    })
  }
  child.on('error', (error) => {
    console.error(`${tags[name]} ${paint('31', `✗ could not start: ${error.message}`)}`)
  })
  child.on('exit', (code, signal) => {
    children.delete(name)
    if (stopping)
      return
    const how = signal
      ? `signal ${signal}`
      : `code ${code}`
    console.error(`${tags.dev} ${paint('31', `✗ ${name} exited (${how}); stopping the rest.`)}`)
    void stop(1)
  })
  return child
}

function signalChild(child, signal) {
  try {
    if (process.platform === 'win32')
      child.kill(signal)
    else
      process.kill(-child.pid, signal)
  }
  catch {}
}

async function stop(code) {
  if (stopping)
    return
  stopping = true
  const running = [
    ...children.values(),
  ]
  if (running.length > 0)
    log('stopping…')
  running.forEach(child => signalChild(child, 'SIGTERM'))
  const deadline = Date.now() + 5000
  while (children.size > 0 && Date.now() < deadline)
    await sleep(100)
  for (const child of children.values())
    signalChild(child, 'SIGKILL')
  process.exit(code)
}

process.on('SIGINT', () => void stop(0))
process.on('SIGTERM', () => void stop(0))
process.on('SIGHUP', () => void stop(0))

await run()

async function run() {
  log(`starting the backend on ${apiUrl} (internal key: generated for this run)`)
  start('server', 'pnpm', ['--filter', '@coffee/server', 'dev'], {
    HOST: apiHost,
    PORT: String(apiPort),
    COFFEE_INTERNAL_KEY: internalKey,
  })
  if (!(await healthy(60_000))) {
    if (!stopping) {
      console.error(`${tags.dev} ${paint('31', `✗ the backend did not answer ${apiUrl}/v1/health within 60 s.`)}`)
      await stop(1)
    }
    return
  }
  log('✓ backend is healthy')

  log(`starting coffee-sshd on ${sshAddr}`)
  start('sshd', sshdBin, [], {
    COFFEE_SSH_ADDR: sshAddr,
    COFFEE_SSH_HOST_KEY: hostKey,
    COFFEE_TUI_BIN: tuiBin,
    COFFEE_API_URL: apiUrl,
    COFFEE_INTERNAL_KEY: internalKey,
  })
  if (!(await waitFor(() => listening(sshHost, sshPort), 30_000))) {
    if (!stopping) {
      console.error(`${tags.dev} ${paint('31', `✗ coffee-sshd is not listening on ${sshAddr} after 30 s.`)}`)
      await stop(1)
    }
    return
  }
  const local = [
    '127.0.0.1',
    'localhost',
    '::1',
    '0.0.0.0',
    '::',
  ].includes(sshHost)
    ? 'localhost'
    : sshHost
  console.log('')
  log(paint('1;32', `✓ ready — connect with:  ssh -p ${sshPort} ${local}`))
  log('  (or `pnpm ssh`; any user name, no password. Ctrl+C here stops both.)')
  console.log('')
}

// --- Helpers -------------------------------------------------------------------------------

function listening(host, port) {
  return new Promise((done) => {
    const socket = connect({ host, port })
    socket.setTimeout(500)
    socket.once('connect', () => {
      socket.destroy()
      done(true)
    })
    socket.once('timeout', () => {
      socket.destroy()
      done(false)
    })
    socket.once('error', () => done(false))
  })
}

async function waitFor(check, timeout) {
  const deadline = Date.now() + timeout
  while (Date.now() < deadline) {
    if (stopping)
      return false
    if (await check())
      return true
    await sleep(250)
  }
  return false
}

function healthy(timeout) {
  return waitFor(async () => {
    try {
      const response = await fetch(`${apiUrl}/v1/health`, {
        headers: { 'X-Internal-Key': internalKey },
        signal: AbortSignal.timeout(1000),
      })
      return response.ok
    }
    catch {
      return false
    }
  }, timeout)
}
