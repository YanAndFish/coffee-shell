"""The real stack for end-to-end flows: a backend on a fresh SQLite database, `tty-gateway`,
and stock `ssh` clients on pseudo-terminals whose screens are read as text.

Nothing here knows the shop: flows (`flows.py`) say what to press and what to wait for.
The terminal emulator is `scripts/pty-run.py`'s (imported, not copied). The gateway runs
`deploy/gateway.toml` with a few overrides (`gateway_config`), written to the flow's
directory as `gateway.toml`.

  stack = Stack(work_dir, template_db, flow_dir)
  with stack:                               # backend + gateway up, both stopped on exit
    term = stack.connect("buyer", key="k1") # ssh -i <work>/k1 guest@... (None: no key at all)
    term.wait("用户名")                     # until the text is on screen (or fail)
    term.type("abc"); term.key("enter")
    term.snap("receipt")                    # <flow_dir>/NN-buyer-receipt.txt

Every wait has a timeout; a timeout raises `Failure` carrying the session's last screen.
"""

from __future__ import annotations

import importlib.util
import json
import os
import re
import shutil
import signal
import socket
import sqlite3
import subprocess
import sys
import time
import tomllib
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any, Callable

ROOT = Path(__file__).resolve().parents[2]
GATEWAY_CONFIG = ROOT / "deploy" / "gateway.toml"

_spec = importlib.util.spec_from_file_location("pty_run", ROOT / "scripts" / "pty-run.py")
assert _spec and _spec.loader
pty_run = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(pty_run)

# How long a screen may take to show what a step waits for. Generous: CI runners are slow
# and a debug `coffee-tui` hashes nothing but still draws a lot.
WAIT = float(os.environ.get("E2E_WAIT", "20"))


class Failure(Exception):
  """A flow step that did not happen; `screen` is what the session showed last."""

  def __init__(self, message: str, screen: str = "", term: str = ""):
    super().__init__(message)
    self.screen = screen
    self.term = term


def free_port() -> int:
  with socket.socket() as probe:
    probe.bind(("127.0.0.1", 0))
    return probe.getsockname()[1]


def listening(port: int) -> bool:
  with socket.socket() as probe:
    probe.settimeout(0.3)
    return probe.connect_ex(("127.0.0.1", port)) == 0


def wait_until(check: Callable[[], bool], timeout: float, step: float = 0.1) -> bool:
  deadline = time.monotonic() + timeout
  while time.monotonic() < deadline:
    if check():
      return True
    time.sleep(step)
  return check()


def cargo_target_dir() -> Path:
  out = subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps"], cwd=ROOT,
                       capture_output=True, text=True, timeout=120, check=True).stdout
  return Path(json.loads(out)["target_directory"])


def make_template_db(work: Path) -> Path:
  """Migrates and seeds one database; each flow starts from a copy of it."""
  db = work / "template.db"
  for leftover in work.glob("template.db*"):
    leftover.unlink()
  env = {**os.environ, "DATABASE_URL": f"file:{db}", "PRISMA_HIDE_UPDATE_MESSAGE": "1",
         "CHECKPOINT_DISABLE": "1"}
  server = ROOT / "apps" / "server"
  for argv in ([str(server / "node_modules" / ".bin" / "prisma"), "migrate", "deploy"],
               ["node", "prisma/seed.ts"]):
    done = subprocess.run(argv, cwd=server, env=env, capture_output=True, text=True, timeout=180)
    if done.returncode != 0:
      raise Failure(f"preparing the template database failed: {' '.join(argv)}\n"
                    f"{done.stdout[-2000:]}{done.stderr[-2000:]}")
  return db


def make_key(work: Path, name: str) -> Path:
  """A throwaway ed25519 key (no passphrase) for `ssh -i`."""
  key = work / "keys" / name
  if not key.exists():
    key.parent.mkdir(parents=True, exist_ok=True)
    subprocess.run(["ssh-keygen", "-q", "-t", "ed25519", "-N", "", "-C", f"e2e-{name}", "-f",
                    str(key)], check=True, timeout=30)
  return key


# --- The gateway's configuration ------------------------------------------------------------

def merge(base: dict[str, Any], overrides: dict[str, Any]) -> dict[str, Any]:
  """`base` with `overrides` laid over it: tables merge key by key, anything else replaces."""
  out = dict(base)
  for key, value in overrides.items():
    if isinstance(value, dict) and isinstance(out.get(key), dict):
      out[key] = merge(out[key], value)
    else:
      out[key] = value
  return out


def gateway_config(overrides: dict[str, Any] | None = None) -> dict[str, Any]:
  """`deploy/gateway.toml` for a flow: no per-minute connection budget (flows connect often
  from one address), then `overrides`."""
  with open(GATEWAY_CONFIG, "rb") as file:
    config = tomllib.load(file)
  config = merge(config, {"limits": {"connections_per_minute": 0}})
  return merge(config, overrides or {})


_BARE_KEY = re.compile(r"[A-Za-z0-9_-]+")
_ESCAPES = {"\\": "\\\\", '"': '\\"', "\b": "\\b", "\t": "\\t", "\n": "\\n", "\f": "\\f",
            "\r": "\\r"}


def _toml_string(text: str) -> str:
  out = []
  for char in text:
    if char in _ESCAPES:
      out.append(_ESCAPES[char])
    elif ord(char) < 0x20 or ord(char) == 0x7F:
      out.append(f"\\u{ord(char):04X}")
    else:
      out.append(char)
  return '"' + "".join(out) + '"'


def _toml_key(key: str) -> str:
  return key if _BARE_KEY.fullmatch(key) else _toml_string(key)


def _toml_value(value: Any) -> str:
  if isinstance(value, bool):
    return "true" if value else "false"
  if isinstance(value, int):
    return str(value)
  if isinstance(value, str):
    return _toml_string(value)
  if isinstance(value, list):
    return "[" + ", ".join(_toml_value(item) for item in value) + "]"
  raise TypeError(f"no TOML for {type(value).__name__}: {value!r}")


def toml_dumps(data: dict[str, Any]) -> str:
  """TOML that `tomllib` reads back as `data`: strings, ints, booleans, lists of those and
  nested tables (enough for the gateway's file)."""
  lines: list[str] = []

  def table(path: list[str], body: dict[str, Any]) -> None:
    scalars = [(k, v) for k, v in body.items() if not isinstance(v, dict)]
    tables = [(k, v) for k, v in body.items() if isinstance(v, dict)]
    if path and (scalars or not tables):
      if lines:
        lines.append("")
      lines.append("[" + ".".join(_toml_key(part) for part in path) + "]")
    for key, value in scalars:
      lines.append(f"{_toml_key(key)} = {_toml_value(value)}")
    for key, value in tables:
      table([*path, key], value)

  table([], data)
  return "\n".join(lines) + "\n"


def write_toml(path: Path, data: dict[str, Any]) -> Path:
  """Writes `data` to `path`, checked: read back with `tomllib`, it must be `data`."""
  text = toml_dumps(data)
  if tomllib.loads(text) != data:
    raise Failure(f"the TOML written for {path.name} does not read back the same")
  path.write_text(text)
  return path


def check_toml_writer() -> None:
  """The emitter against awkward values: quotes, backslashes, control characters, CJK, keys
  that need quoting, nesting, empty tables and lists."""
  sample = {
    "top": "a \"quoted\" \\ back\\slash\n\ttab \x01\x7f ◆咖啡 ${env:X:-y}",
    "n": 0, "big": 2**40, "neg": -3, "yes": True, "no": False,
    "list": ["x", "y\"z", "\\"], "ints": [1, 2], "empty": [],
    "server": {"banner": "\n  ◆ROAST/TTY\n\n", "listen": "127.0.0.1:1"},
    "routes": {"a.b": {"command": ["/bin/x", "--flag"], "env": {"K": "${fingerprint}"},
                       "messages": {}},
               "c": {"env": {"with space": "v", "": "empty key"}}},
  }
  text = toml_dumps(sample)
  if tomllib.loads(text) != sample:
    raise Failure(f"the TOML writer does not round-trip:\n{text}")


def check_sync_frames() -> None:
  """The emulator's synchronized updates: a frame fed in pieces changes nothing a reader sees
  (text, cursor) until its closing marker; without mode 2026 the screen is live; an update
  left open is shown as it stands after SYNC_TIMEOUT; DECRQM 2026 is answered."""
  def expect(what: bool, message: str) -> None:
    if not what:
      raise Failure(f"emulator self-check: {message}")

  screen = pty_run.Screen(20, 3)
  screen.feed(b"\x1b[?2026h\x1b[1;1Hold\x1b[2;5H\x1b[?2026l")
  frame = screen.frame
  expect(frame.text().startswith("old") and (frame.x, frame.y) == (4, 1), "the first frame")
  for half in (b"\x1b[?2026h\x1b[2J\x1b[1;1Hnew", b"\x1b[3;", b"9H\x1b[?20"):
    screen.feed(half)
    frame = screen.frame
    expect(screen.in_update and frame.text().startswith("old") and (frame.x, frame.y) == (4, 1),
           f"half a frame showed through after {half!r}: {frame.text()!r} at {(frame.x, frame.y)}")
  screen.feed(b"26l")
  frame = screen.frame
  expect(not screen.in_update and frame.text().startswith("new") and (frame.x, frame.y) == (8, 2),
         f"the completed frame: {frame.text()!r} at {(frame.x, frame.y)}")
  expect(screen.frames == 2 and screen.split_frames == 3,
         f"{screen.frames} frames, {screen.split_frames} split reads")
  screen.feed(b"\x1b[1;1Hlive")
  expect(screen.frame.text().startswith("live"), "output outside an update is not live")
  screen.feed(b"\x1b[?2026h\x1b[1;1Hstuck")
  expect(screen.frame.text().startswith("live"), "an open update showed through")
  screen._sync_since -= pty_run.SYNC_TIMEOUT + 0.1
  expect(screen.frame.text().startswith("stuck"), "an update left open is never shown")
  screen.feed(b"\x1b[?2026l\x1b[?25l")
  expect(not screen.frame.cursor_visible, "a hidden cursor reads as visible")
  screen.feed(b"\x1b[?2026$p")
  expect(screen.replies == [b"\x1b[?2026;2$y"], f"DECRQM 2026 answered {screen.replies}")


class Process:
  """A child in its own process group, its output in a log file."""

  def __init__(self, name: str, argv: list[str], env: dict[str, str], log: Path, cwd: Path = ROOT):
    self.name, self.log = name, log
    log.parent.mkdir(parents=True, exist_ok=True)
    self._out = open(log, "ab")
    self.proc = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                 stdout=self._out, stderr=subprocess.STDOUT,
                                 start_new_session=True)

  def alive(self) -> bool:
    return self.proc.poll() is None

  def stop(self, grace: float = 5.0) -> None:
    for sig, wait in ((signal.SIGTERM, grace), (signal.SIGKILL, 2.0)):
      if not self.alive():
        break
      try:
        os.killpg(self.proc.pid, sig)
      except ProcessLookupError:
        pass
      try:
        self.proc.wait(timeout=wait)
      except subprocess.TimeoutExpired:
        pass
    self._out.close()

  def tail(self, lines: int = 40) -> str:
    try:
      return "\n".join(self.log.read_text(errors="replace").splitlines()[-lines:])
    except OSError:
      return ""


class Stack:
  """Backend + gateway for one flow, on ports of their own and a copy of the template
  database."""

  def __init__(self, work: Path, template_db: Path | None, flow_dir: Path, bins: Path | None,
               external: tuple[str, int] | None = None):
    """With `external` (host, port), flows run against a gateway someone else started (the
    compose pair): no backend, gateway or database of our own."""
    self.work, self.flow_dir, self.bins, self.external = work, flow_dir, bins, external
    self.db = flow_dir / "shop.db"
    flow_dir.mkdir(parents=True, exist_ok=True)
    if template_db:
      for leftover in flow_dir.glob("shop.db*"):
        leftover.unlink()
      shutil.copyfile(template_db, self.db)
    self.api_port = free_port()
    self.ssh_host, self.ssh_port = external or ("127.0.0.1", free_port())
    self.internal_key = os.urandom(24).hex()
    self.backend: Process | None = None
    self.gateway: Process | None = None
    self.config_path = flow_dir / "gateway.toml"
    self.terms: list[Term] = []
    self._snaps = 0

  # Lifecycle

  def __enter__(self) -> "Stack":
    if self.external:
      return self
    try:
      self.start_backend()
      self.start_gateway()
    except BaseException:
      self.close()
      raise
    return self

  def __exit__(self, *exc) -> None:
    self.close()

  def close(self) -> None:
    for term in self.terms:
      term.close()
    for proc in (self.gateway, self.backend):
      if proc:
        proc.stop()

  def start_backend(self) -> None:
    env = {**os.environ, "HOST": "127.0.0.1", "PORT": str(self.api_port),
           "DATABASE_URL": f"file:{self.db}", "COFFEE_INTERNAL_KEY": self.internal_key}
    self.backend = Process("backend", ["node", "src/main.ts"], env, self.flow_dir / "backend.log",
                           cwd=ROOT / "apps" / "server")
    if not wait_until(self.healthy, 30, 0.2):
      raise Failure(f"the backend did not answer /v1/health within 30 s:\n{self.backend.tail()}")

  def stop_backend(self) -> None:
    if self.backend:
      self.backend.stop()
    if not wait_until(lambda: not listening(self.api_port), 10):
      raise Failure("the backend's port is still open after stopping it")

  def healthy(self) -> bool:
    if self.backend and not self.backend.alive():
      raise Failure(f"the backend exited:\n{self.backend.tail()}")
    try:
      with urllib.request.urlopen(f"http://127.0.0.1:{self.api_port}/v1/health", timeout=1) as r:
        return r.status == 200
    except (urllib.error.URLError, OSError):
      return False

  def start_gateway(self, overrides: dict[str, Any] | None = None) -> None:
    """`tty-gateway --config <flow>/gateway.toml`: `deploy/gateway.toml` with `overrides`
    (see `gateway_config`). The address, host key and log filter come from its environment,
    as do the backend's URL and the internal key the shop's route reads (`${env:…}`)."""
    write_toml(self.config_path, gateway_config(overrides))
    env = {
      "PATH": os.environ.get("PATH", ""), "HOME": os.environ.get("HOME", ""),
      "TMPDIR": os.environ.get("TMPDIR", "/tmp"), "LANG": "C.UTF-8",
      "TTY_GATEWAY_ADDR": f"127.0.0.1:{self.ssh_port}",
      "TTY_GATEWAY_HOST_KEY": str(self.work / "ssh_host_ed25519_key"),
      "TTY_GATEWAY_LOG": "info,russh=warn",
      "COFFEE_API_URL": f"http://127.0.0.1:{self.api_port}",
      "COFFEE_INTERNAL_KEY": self.internal_key,
    }
    argv = [str(self.bins / "tty-gateway"), "--config", str(self.config_path)]
    self.gateway = Process("gateway", argv, env, self.flow_dir / "gateway.log")
    if not wait_until(lambda: listening(self.ssh_port) or not self.gateway.alive(), 30):
      raise Failure(f"tty-gateway is not listening after 30 s:\n{self.gateway.tail()}")
    if not self.gateway.alive():
      raise Failure(f"tty-gateway exited:\n{self.gateway.tail()}")

  def stop_gateway(self) -> None:
    if self.gateway:
      self.gateway.stop()

  def gateway_log(self) -> str:
    return self.gateway.log.read_text(errors="replace") if self.gateway else ""

  # Sessions

  def connect(self, name: str, key: str | None = None, size: tuple[int, int] = (130, 40),
              term: str = "xterm-256color", user: str = "guest") -> "Term":
    """`ssh user@gateway` as a stock client: with the throwaway key `key`, or with none. The
    user name picks the route; no route lists `guest`, so it gets the default (the shop)."""
    argv = ["ssh", "-F", "/dev/null", "-p", str(self.ssh_port),
            "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null",
            "-o", "LogLevel=ERROR", "-o", "IdentitiesOnly=yes", "-o", "IdentityAgent=none",
            "-o", "SendEnv=COLORTERM"]
    if key is None:
      argv += ["-o", "PubkeyAuthentication=no"]
    else:
      argv += ["-i", str(make_key(self.work, key))]
    argv += ["-tt", f"{user}@{self.ssh_host}"]
    session = Term(self, name, argv, size, term)
    self.terms.append(session)
    return session

  def next_snap(self) -> int:
    self._snaps += 1
    return self._snaps

  def session_pids(self) -> list[int]:
    """The live (non-zombie) children of this flow's gateway: its session programs, whatever
    the route runs."""
    if not self.gateway or not self.gateway.alive():
      return []
    out = subprocess.run(["ps", "-A", "-o", "pid=,ppid=,stat="], capture_output=True,
                         text=True, timeout=10).stdout
    pids = []
    for line in out.splitlines():
      parts = line.split()
      if (len(parts) == 3 and parts[1] == str(self.gateway.proc.pid)
          and not parts[2].startswith("Z")):
        pids.append(int(parts[0]))
    return pids

  def env_names(self, pid: int) -> set[str] | None:
    """The names in process `pid`'s environment (Linux: /proc; macOS: `ps -E`), or None when
    this system does not tell. Only names: values (the internal key) never leave here."""
    environ = Path(f"/proc/{pid}/environ")
    if environ.exists():
      try:
        entries = environ.read_bytes().split(b"\0")
      except OSError:
        return None
      return {entry.split(b"=", 1)[0].decode(errors="replace") for entry in entries if b"=" in entry}
    if sys.platform != "darwin":
      return None
    out = subprocess.run(["ps", "-E", "-ww", "-o", "command=", "-p", str(pid)],
                         capture_output=True, text=True, timeout=10).stdout
    names = set(re.findall(r"(?:^|\s)([A-Z_][A-Z0-9_]*)=", out))
    return names or None

  def sql(self, query: str) -> list[tuple]:
    """Reads the flow's database directly (the shop's truth, not the UI's)."""
    db = sqlite3.connect(f"file:{self.db}?mode=ro", uri=True, timeout=10)
    try:
      return db.execute(query).fetchall()
    finally:
      db.close()


class Term:
  """One `ssh` client on a PTY, its screen replayed by pty-run's emulator."""

  def __init__(self, stack: Stack, name: str, argv: list[str], size: tuple[int, int], term: str):
    env = dict(os.environ)
    env["TERM"] = term
    env["COLORTERM"] = "truecolor"
    self.stack, self.name = stack, name
    self.session = pty_run.Session(argv, size[0], size[1], env)
    self.closed = False

  # Reading

  @property
  def screen(self) -> "pty_run.Frame":
    """The last completed frame: the host draws each frame inside a synchronized update, and
    one that arrives over several reads is not seen until it is whole (a half-painted frame
    moves the cursor and the status bar before the focus has really moved)."""
    return self.session.screen.frame

  def text(self) -> str:
    return self.screen.text()

  def row(self, index: int) -> str:
    return "".join(self.screen.grid[index]).rstrip()

  def footer(self) -> str:
    """The status bar: the bottom row."""
    return self.row(self.screen.rows - 1)

  def cursor(self) -> tuple[int, int] | None:
    """(column, row) of the terminal cursor, where a focused text field puts it; None while
    it is hidden (its position then only says where the last frame's drawing ended)."""
    screen = self.screen
    return (screen.x, screen.y) if screen.cursor_visible else None

  def find(self, needle: str) -> tuple[int, int] | None:
    """(row, cell column) of the first `needle` on screen. Columns count cells: a CJK
    character is two."""
    return self.find_re(re.escape(needle))

  def find_re(self, pattern: str) -> tuple[int, int] | None:
    """Like `find` for a regular expression; with a group, where the first group starts."""
    for y, cells in enumerate(self.screen.grid):
      text, starts = "", []
      for x, cell in enumerate(cells):
        if cell:  # the right half of a wide character is an empty cell
          starts.append(x)
          text += cell
      match = re.search(pattern, text)
      if match:
        return y, starts[match.start(1) if match.groups() else match.start()]
    return None

  def columns(self, start: int, end: int | None = None) -> list[str]:
    """Every row cut to the cells [start, end): one panel of a multi-column layout."""
    return ["".join(cells[start:end]).rstrip() for cells in self.screen.grid]

  def alive(self) -> bool:
    return self.session.alive()

  def fail(self, message: str) -> Failure:
    return Failure(f"[{self.name}] {message}", self.text(), self.name)

  # Waiting

  def pump(self, seconds: float) -> None:
    """Renders this session's output for `seconds`, keeping the other open sessions drained
    (a client nobody reads would stall its server side)."""
    deadline = time.monotonic() + seconds
    while True:
      for other in self.stack.terms:
        if other is not self and not other.closed:
          other.session.pump(0.001)
      left = deadline - time.monotonic()
      if left <= 0:
        return
      self.session.pump(min(left, 0.05))

  def wait_for(self, check: Callable[[], bool], what: str, timeout: float = WAIT) -> None:
    deadline = time.monotonic() + timeout
    while not check():
      if time.monotonic() >= deadline:
        raise self.fail(f"{what}: not within {timeout:g} s")
      self.pump(0.1)

  def wait(self, *needles: str, timeout: float = WAIT) -> str:
    """Until every needle is on screen; returns the screen."""
    self.wait_for(lambda: all(n in self.text() for n in needles),
                  "waiting for " + " + ".join(repr(n) for n in needles), timeout)
    return self.text()

  def wait_any(self, *needles: str, timeout: float = WAIT) -> str:
    """Until one of the needles is on screen; returns the one seen."""
    found: list[str] = []

    def check() -> bool:
      text = self.text()
      found[:] = [n for n in needles if n in text]
      return bool(found)

    self.wait_for(check, "waiting for any of " + ", ".join(repr(n) for n in needles), timeout)
    return found[0]

  def gone(self, needle: str, timeout: float = WAIT) -> None:
    self.wait_for(lambda: needle not in self.text(), f"waiting for {needle!r} to go", timeout)

  def wait_footer(self, needle: str, timeout: float = WAIT) -> None:
    self.wait_for(lambda: needle in self.footer(), f"status bar showing {needle!r}", timeout)

  def pause(self, seconds: float) -> None:
    self.pump(seconds)

  def exited(self, timeout: float = WAIT) -> None:
    self.wait_for(lambda: not self.alive(), "waiting for ssh to exit", timeout)

  # Input

  def type(self, text: str) -> None:
    self.session.send(text.encode())
    self.pump(0.15)

  def key(self, name: str, times: int = 1) -> None:
    """Presses a named key (pty-run's names). After Esc it pauses, so the next key is not
    read as Alt+key."""
    for _ in range(times):
      self.session.send(pty_run.key_bytes(name))
      self.pump(0.4 if name in ("esc", "escape") else 0.12)

  def resize(self, cols: int, rows: int) -> None:
    self.session.resize(cols, rows)
    self.pump(0.3)

  # Records

  def snap(self, label: str) -> Path:
    """Writes the screen to `<flow_dir>/NN-<term>-<label>.txt`."""
    path = self.stack.flow_dir / f"{self.stack.next_snap():02d}-{self.name}-{label}.txt"
    path.write_text(self.text() + "\n")
    return path

  # Leaving

  def quit(self) -> None:
    """Ctrl+C twice, as a person leaves; ssh must exit."""
    self.session.send(b"\x03")
    self.wait("再按一次")
    self.session.send(b"\x03")
    self.exited()
    self.close()

  def kill(self) -> None:
    """The client dies (no goodbye): SIGKILL to ssh."""
    try:
      os.kill(self.session.pid, signal.SIGKILL)
    except ProcessLookupError:
      pass
    self.session.wait_until(lambda: not self.alive(), 5)
    self.close()

  def close(self) -> None:
    if not self.closed:
      self.closed = True
      self.session.stop()


def log(message: str) -> None:
  print(message, flush=True)


def fail_report(failure: Failure, stack: Stack | None) -> str:
  parts = [f"FAIL: {failure}"]
  if failure.screen:
    parts += [f"--- last screen of {failure.term or 'the session'} ---", failure.screen]
  if stack:
    for term in stack.terms:
      if term.name != failure.term and not term.closed:
        parts += [f"--- last screen of {term.name} ---", term.text()]
    for proc in (stack.backend, stack.gateway):
      if proc:
        parts += [f"--- {proc.name} log (tail) ---", proc.tail(25)]
  return "\n".join(parts)


if __name__ == "__main__":
  sys.exit("run scripts/e2e/run.py")
