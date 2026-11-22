// In-memory rate limits of register and login (docs/design/decisions.md P-auth-3). Counters
// live in the process: a restart clears them. Every method takes the time explicitly, so
// tests drive them with the app's clock (`deps.now`).

/** The window of both limits: 10 minutes. */
export const RATE_WINDOW_MS = 10 * 60 * 1000

/** Attempts (register) or failures (login) allowed per key within {@link RATE_WINDOW_MS}. */
export const RATE_MAX = 5

/** Beyond this many keys, stale ones are swept before a new key is added. */
const SWEEP_AT = 10_000

/** Seconds until `until` (at least 1), for `details.retryAfter` and `Retry-After`. */
function secondsUntil(until: number, now: number): number {
  return Math.max(1, Math.ceil((until - now) / 1000))
}

/** Timestamps within the window ending at `now`. */
function recent(times: readonly number[], now: number): number[] {
  return times.filter(t => now - t < RATE_WINDOW_MS)
}

/**
 * At most {@link RATE_MAX} attempts per key in any {@link RATE_WINDOW_MS}: register, keyed by
 * the SSH fingerprint, else the client IP.
 */
export class AttemptLimiter {
  readonly #attempts = new Map<string, number[]>()

  /**
   * Records an attempt for `key` at `now` and returns `null`, or, when the key already used
   * its attempts, records nothing and returns the seconds to wait.
   */
  take(key: string, now: Date): number | null {
    const t = now.getTime()
    const times = recent(this.#attempts.get(key) ?? [], t)
    if (times.length >= RATE_MAX)
      return secondsUntil(times[0]! + RATE_WINDOW_MS, t)
    if (!this.#attempts.has(key) && this.#attempts.size >= SWEEP_AT)
      this.#sweep(t)
    times.push(t)
    this.#attempts.set(key, times)
    return null
  }

  /** Keys tracked now (tests). */
  get size(): number {
    return this.#attempts.size
  }

  #sweep(now: number): void {
    for (const [key, times] of this.#attempts) {
      if (recent(times, now).length === 0)
        this.#attempts.delete(key)
    }
  }
}

interface FailureState {
  failures: number[]
  lockedUntil: number
}

/**
 * Login failures per key (the username key): {@link RATE_MAX} failures within
 * {@link RATE_WINDOW_MS} lock the key for {@link RATE_WINDOW_MS}. A success clears the key.
 */
export class FailureLimiter {
  readonly #keys = new Map<string, FailureState>()

  /** Seconds to wait when `key` is locked at `now`, else `null`. */
  lockedFor(key: string, now: Date): number | null {
    const state = this.#keys.get(key)
    const t = now.getTime()
    if (state && state.lockedUntil > t)
      return secondsUntil(state.lockedUntil, t)
    return null
  }

  /** Records a failure; the one that reaches {@link RATE_MAX} starts the lock. */
  fail(key: string, now: Date): void {
    const t = now.getTime()
    if (!this.#keys.has(key) && this.#keys.size >= SWEEP_AT)
      this.#sweep(t)
    const state = this.#keys.get(key) ?? { failures: [], lockedUntil: 0 }
    state.failures = recent(state.failures, t)
    state.failures.push(t)
    if (state.failures.length >= RATE_MAX) {
      state.lockedUntil = t + RATE_WINDOW_MS
      state.failures = []
    }
    this.#keys.set(key, state)
  }

  /** Forgets `key` (after a successful login). */
  clear(key: string): void {
    this.#keys.delete(key)
  }

  /** Keys tracked now (tests). */
  get size(): number {
    return this.#keys.size
  }

  #sweep(now: number): void {
    for (const [key, state] of this.#keys) {
      if (state.lockedUntil <= now && recent(state.failures, now).length === 0)
        this.#keys.delete(key)
    }
  }
}

/** The limits of one app instance (`routes/auth.ts` makes one per router). */
export interface AuthLimits {
  register: AttemptLimiter
  login: FailureLimiter
}

export function createAuthLimits(): AuthLimits {
  return {
    register: new AttemptLimiter(),
    login: new FailureLimiter(),
  }
}
