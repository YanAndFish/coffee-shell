"""The real stack for end-to-end flows: a backend on a fresh SQLite database, `coffee-sshd`,
and stock `ssh` clients on pseudo-terminals whose screens are read as text.

Nothing here knows the shop: flows (`flows.py`) say what to press and what to wait for.
The terminal emulator is `scripts/pty-run.py`'s (imported, not copied).

  stack = Stack(work_dir, template_db, flow_dir)
  with stack:                               # backend + sshd up, both stopped on exit
    term = stack.connect("buyer", key="k1") # ssh -i <work>/k1 ... (None: no key at all)
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
import urllib.error
import urllib.request
from pathlib import Path
from typing import Callable

ROOT = Path(__file__).resolve().parents[2]

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
  """Backend + sshd for one flow, on ports of their own and a copy of the template database."""

  def __init__(self, work: Path, template_db: Path | None, flow_dir: Path, bins: Path | None,
               gateway: tuple[str, int] | None = None):
    """With `gateway` (host, port), flows run against a gateway someone else started (the
    compose pair): no backend, sshd or database of our own."""
    self.work, self.flow_dir, self.bins, self.gateway = work, flow_dir, bins, gateway
    self.db = flow_dir / "shop.db"
    flow_dir.mkdir(parents=True, exist_ok=True)
    if template_db:
      for leftover in flow_dir.glob("shop.db*"):
        leftover.unlink()
      shutil.copyfile(template_db, self.db)
    self.api_port = free_port()
    self.ssh_host, self.ssh_port = gateway or ("127.0.0.1", free_port())
    self.internal_key = os.urandom(24).hex()
    self.backend: Process | None = None
    self.sshd: Process | None = None
    self.terms: list[Term] = []
    self._snaps = 0

  # Lifecycle

  def __enter__(self) -> "Stack":
    if self.gateway:
      return self
    try:
      self.start_backend()
      self.start_sshd()
    except BaseException:
      self.close()
      raise
    return self

  def __exit__(self, *exc) -> None:
    self.close()

  def close(self) -> None:
    for term in self.terms:
      term.close()
    for proc in (self.sshd, self.backend):
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

  def start_sshd(self, **overrides: str) -> None:
    env = {
      "PATH": os.environ.get("PATH", ""), "HOME": os.environ.get("HOME", ""),
      "TMPDIR": os.environ.get("TMPDIR", "/tmp"), "LANG": "C.UTF-8",
      "COFFEE_SSH_ADDR": f"127.0.0.1:{self.ssh_port}",
      "COFFEE_SSH_HOST_KEY": str(self.work / "ssh_host_ed25519_key"),
      "COFFEE_TUI_BIN": str(self.bins / "coffee-tui"),
      "COFFEE_API_URL": f"http://127.0.0.1:{self.api_port}",
      "COFFEE_INTERNAL_KEY": self.internal_key,
      # Flows connect often from one address: no per-minute connection budget here.
      "COFFEE_SSH_CONNECTIONS_PER_MINUTE": "0",
      "COFFEE_SSH_LOG": "info,russh=warn",
      **overrides,
    }
    self.sshd = Process("sshd", [str(self.bins / "coffee-sshd")], env, self.flow_dir / "sshd.log")
    if not wait_until(lambda: listening(self.ssh_port) or not self.sshd.alive(), 30):
      raise Failure(f"coffee-sshd is not listening after 30 s:\n{self.sshd.tail()}")
    if not self.sshd.alive():
      raise Failure(f"coffee-sshd exited:\n{self.sshd.tail()}")

  def stop_sshd(self) -> None:
    if self.sshd:
      self.sshd.stop()

  # Sessions

  def connect(self, name: str, key: str | None = None, size: tuple[int, int] = (130, 40),
              term: str = "xterm-256color") -> "Term":
    """`ssh` to the gateway as a stock client: with the throwaway key `key`, or with none."""
    argv = ["ssh", "-F", "/dev/null", "-p", str(self.ssh_port),
            "-o", "StrictHostKeyChecking=no", "-o", "UserKnownHostsFile=/dev/null",
            "-o", "LogLevel=ERROR", "-o", "IdentitiesOnly=yes", "-o", "IdentityAgent=none",
            "-o", "SendEnv=COLORTERM"]
    if key is None:
      argv += ["-o", "PubkeyAuthentication=no"]
    else:
      argv += ["-i", str(make_key(self.work, key))]
    argv += ["-tt", f"guest@{self.ssh_host}"]
    session = Term(self, name, argv, size, term)
    self.terms.append(session)
    return session

  def next_snap(self) -> int:
    self._snaps += 1
    return self._snaps

  def tui_pids(self) -> list[int]:
    """`coffee-tui` processes whose parent is this flow's sshd."""
    if not self.sshd:
      return []
    out = subprocess.run(["ps", "-A", "-o", "pid=,ppid=,stat=,comm="], capture_output=True,
                         text=True, timeout=10).stdout
    pids = []
    for line in out.splitlines():
      parts = line.split(None, 3)
      if (len(parts) == 4 and parts[1] == str(self.sshd.proc.pid) and not parts[2].startswith("Z")
          and parts[3].endswith("coffee-tui")):
        pids.append(int(parts[0]))
    return pids

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
  def screen(self):
    return self.session.screen

  def text(self) -> str:
    return self.screen.text()

  def row(self, index: int) -> str:
    return "".join(self.screen.grid[index]).rstrip()

  def footer(self) -> str:
    """The status bar: the bottom row."""
    return self.row(self.screen.rows - 1)

  def cursor(self) -> tuple[int, int]:
    """(column, row) of the terminal cursor: where a focused text field puts it."""
    return self.screen.x, self.screen.y

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
    for proc in (stack.backend, stack.sshd):
      if proc:
        parts += [f"--- {proc.name} log (tail) ---", proc.tail(25)]
  return "\n".join(parts)


if __name__ == "__main__":
  sys.exit("run scripts/e2e/run.py")
