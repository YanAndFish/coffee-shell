#!/usr/bin/env node
// `pnpm dev`: the whole demo on this machine.
//
// 1. Builds `coffee-tui` and `tty-gateway` with Cargo.
// 2. Starts the backend (`pnpm --filter @coffee/server dev`) and waits for `/v1/health`.
// 3. Checks `deploy/gateway.toml` (`tty-gateway --check`) and starts `tty-gateway` with it:
//    the SSH user name picks a route, each session runs `coffee-tui` on a PTY (`gallery@`
//    gets the UI gallery, any other name the shop).
// 4. Prints how to connect, prefixes both logs, and stops both on Ctrl+C.
//
// Both processes share a random `COFFEE_INTERNAL_KEY` made for this run only (never printed);
// the gateway's configuration hands it to the shop's sessions only, with COFFEE_API_URL.
// Options: `--skip-build` reuses the binaries already in Cargo's target directory.
// Environment: HOST, PORT (backend, default 127.0.0.1:3000); TTY_GATEWAY_ADDR (default
// 127.0.0.1:2222), TTY_GATEWAY_HOST_KEY (default .data/ssh_host_ed25519_key) and
// TTY_GATEWAY_CONFIG (default deploy/gateway.toml), all three passed to the gateway;
// COFFEE_COLORS (passed on to every session). The names COFFEE_SSH_ADDR and
// COFFEE_SSH_HOST_KEY of the old gateway are no longer read (a warning says so).
//
// Every phase (cargo metadata, the build, the check, the backend, the gateway) runs as a child in its own
// process group, never blocking this process: Ctrl+C, SIGTERM or SIGHUP at any moment stops
// whatever is running (the whole group, so cargo's rustc jobs and pnpm's node too) and exits.

import { Buffer } from 'node:buffer'
import { spawn } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync } from 'node:fs'
import { connect } from 'node:net'
import { dirname, join, relative, resolve } from 'node:path'
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
  dev: paint('1;33', '[dev]    '),
  server: paint('36', '[server] '),
  gateway: paint('35', '[gateway]'),
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

// --- Children and shutdown -----------------------------------------------------------------

/** @type {Map<string, import('node:child_process').ChildProcess>} */
const children = new Map()
let stopping = false

process.on('SIGINT', () => void stop(0))
process.on('SIGTERM', () => void stop(0))
process.on('SIGHUP', () => void stop(0))

/**
 * Starts `command` as child `name` in a process group of its own (so stopping it stops what
 * it started) and resolves with how it ended.
 * @returns {{ child: import('node:child_process').ChildProcess, done: Promise<{ code: number | null, signal: string | null, error?: Error }> }} the child and its end
 */
function launch(name, command, commandArgs, options) {
  const child = spawn(command, commandArgs, {
    cwd: root,
    detached: process.platform !== 'win32',
    ...options,
  })
  children.set(name, child)
  const done = new Promise((settle) => {
    child.once('error', (error) => {
      children.delete(name)
      settle({ code: null, signal: null, error })
    })
    child.once('exit', (code, signal) => {
      children.delete(name)
      settle({ code, signal })
    })
  })
  return { child, done }
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
    await sleep(50)
  for (const child of children.values())
    signalChild(child, 'SIGKILL')
  process.exit(code)
}

/** Parks the caller forever once a shutdown is under way: `stop` ends the process. */
async function unlessStopping() {
  if (stopping)
    await new Promise(() => {})
}

// --- Settings ------------------------------------------------------------------------------

const args = new Set(process.argv.slice(2))
const skipBuild = args.has('--skip-build')

const apiHost = process.env.HOST || '127.0.0.1'
const apiPort = Number(process.env.PORT || 3000)
const apiUrl = `http://${apiHost.includes(':')
  ? `[${apiHost}]`
  : apiHost}:${apiPort}`
for (const [old, now] of [
  [
    'COFFEE_SSH_ADDR',
    'TTY_GATEWAY_ADDR',
  ],
  [
    'COFFEE_SSH_HOST_KEY',
    'TTY_GATEWAY_HOST_KEY',
  ],
]) {
  if (process.env[old])
    log(paint('33', `! ${old} is no longer read: set ${now} instead.`))
}
const sshAddr = process.env.TTY_GATEWAY_ADDR || '127.0.0.1:2222'
const sshPort = Number(sshAddr.slice(sshAddr.lastIndexOf(':') + 1))
const sshHost = sshAddr.slice(0, sshAddr.lastIndexOf(':')).replace(/^\[|\]$/g, '')
const hostKey = resolve(root, process.env.TTY_GATEWAY_HOST_KEY || '.data/ssh_host_ed25519_key')
const gatewayConfig = resolve(root, process.env.TTY_GATEWAY_CONFIG || 'deploy/gateway.toml')
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

const targetDir = await cargoTargetDir()
const exe = process.platform === 'win32'
  ? '.exe'
  : ''
const tuiBin = join(targetDir, 'debug', `coffee-tui${exe}`)
const gatewayBin = join(targetDir, 'debug', `tty-gateway${exe}`)

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
    'tty-gateway',
    'TTY_GATEWAY_ADDR',
  ],
]) {
  if (await listening(host, port))
    fail(`${host}:${port} is already in use, so ${what} cannot listen there.`, `Stop what holds it or set ${variable}.`)
}
await unlessStopping()

// --- Build ---------------------------------------------------------------------------------

if (skipBuild) {
  for (const bin of [tuiBin, gatewayBin]) {
    if (!existsSync(bin))
      fail(`${bin} is not built.`, 'Run `pnpm dev` without --skip-build, or `cargo build -p coffee-tui -p tty-gateway`.')
  }
}
else {
  log('building coffee-tui and tty-gateway (cargo build -p coffee-tui -p tty-gateway)…')
  const build = await launch('build', 'cargo', [
    'build',
    '-p',
    'coffee-tui',
    '-p',
    'tty-gateway',
  ], {
    stdio: ['ignore', 'inherit', 'inherit'],
  }).done
  await unlessStopping()
  if (build.error || build.code !== 0)
    fail('cargo build failed (see above).')
}

mkdirSync(dirname(hostKey), { recursive: true })

// The gateway's environment: the configuration reads COFFEE_API_URL and COFFEE_INTERNAL_KEY
// (`${env:…}`) for the shop's sessions; `coffee-tui` is found next to `tty-gateway`.
const gatewayEnv = {
  TTY_GATEWAY_ADDR: sshAddr,
  TTY_GATEWAY_HOST_KEY: hostKey,
  COFFEE_API_URL: apiUrl,
  COFFEE_INTERNAL_KEY: internalKey,
}

// --- Check the gateway's configuration -----------------------------------------------------

{
  const { child, done } = launch('check', gatewayBin, [
    '--check',
    '--config',
    gatewayConfig,
  ], {
    env: {
      ...process.env,
      ...gatewayEnv,
    },
    stdio: ['ignore', 'ignore', 'pipe'],
  })
  const chunks = []
  child.stderr.on('data', chunk => chunks.push(chunk))
  const { code, error } = await done
  await unlessStopping()
  const said = Buffer.concat(chunks).toString('utf8').trim()
  if (error || code !== 0)
    fail(`${gatewayConfig} does not check out (tty-gateway --check):`, error?.message ?? said)
  for (const line of said.split('\n').filter(Boolean))
    log(paint('33', `! ${line}`))
}

// --- Run -----------------------------------------------------------------------------------

function start(name, command, commandArgs, env) {
  const { child, done } = launch(name, command, commandArgs, {
    env: {
      ...process.env,
      ...env,
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  })
  for (const stream of [child.stdout, child.stderr]) {
    createInterface({ input: stream }).on('line', (line) => {
      // While stopping, pnpm only reports that its child got SIGTERM: not worth showing.
      if (!(stopping && command === 'pnpm'))
        console.log(`${tags[name]} ${line}`)
    })
  }
  void done.then(({ code, signal, error }) => {
    if (error)
      console.error(`${tags[name]} ${paint('31', `✗ could not start: ${error.message}`)}`)
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

  log(`starting tty-gateway on ${sshAddr} (${relative(root, gatewayConfig) || gatewayConfig})`)
  start('gateway', gatewayBin, [
    '--config',
    gatewayConfig,
  ], gatewayEnv)
  if (!(await waitFor(() => listening(sshHost, sshPort), 30_000))) {
    if (!stopping) {
      console.error(`${tags.dev} ${paint('31', `✗ tty-gateway is not listening on ${sshAddr} after 30 s.`)}`)
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
  log(`  the shop: any user name but gallery, no password (or \`pnpm ssh\`)`)
  log(`  the UI gallery:  ssh -p ${sshPort} gallery@${local}`)
  log('  Ctrl+C here stops both.')
  console.log('')
}

// --- Helpers -------------------------------------------------------------------------------

/** Cargo's target directory, from `cargo metadata` (run like every other child). */
async function cargoTargetDir() {
  const { child, done } = launch('metadata', 'cargo', [
    'metadata',
    '--format-version',
    '1',
    '--no-deps',
  ], {
    stdio: ['ignore', 'pipe', 'ignore'],
  })
  const chunks = []
  child.stdout.on('data', chunk => chunks.push(chunk))
  const { code, error } = await done
  await unlessStopping()
  if (error || code !== 0) {
    fail(
      'cargo is not available or the workspace does not load.',
      'Install Rust with rustup; rust-toolchain.toml pins the version.',
    )
  }
  return JSON.parse(Buffer.concat(chunks).toString('utf8')).target_directory
}

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
