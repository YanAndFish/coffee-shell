#!/usr/bin/env python3
"""Run a command on a pseudo-terminal of a given size and print its final screen as text.

For checking the TUI (`coffee-tui`) or an SSH session (`ssh -p 2222 localhost`) without a
human at a terminal. The command runs on a real PTY; a small built-in VT100/xterm emulator
replays what it draws (cursor moves, erases, scroll regions, the alternate screen, wide CJK
characters), and the screen as it stands after the last step is printed as plain text.
Colours and attributes are dropped. Only the Python standard library is used (macOS, Linux).

Usage:
  scripts/pty-run.py [options] -- COMMAND [ARG...]

Examples:
  # The UI alone at 100x30, after it settles:
  python3 scripts/pty-run.py --size 100x30 -- target/debug/coffee-tui

  # Over SSH (with `pnpm dev` running): wait for text, press keys, resize, check text:
  python3 scripts/pty-run.py --size 160x50 \\
      --step 'until=ROAST/TTY' --step 'key=down' --step 'wait=0.5' \\
      --step 'resize=60x20' --step 'until=终端太小' --expect '终端太小' \\
      -- ssh -p 2222 -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null localhost

Steps (`--step`, repeatable, run in order after --settle):
  wait=SECONDS        let the program run
  until=TEXT          wait until TEXT is on screen (fails after --timeout)
  gone=TEXT           wait until TEXT is no longer on screen (fails after --timeout)
  text=STRING         type STRING (Python escapes work: \\r \\t \\x1b \\u00e9)
  key=NAME[*N]        press a named key N times: enter tab backtab esc space backspace
                      delete up down left right home end pageup pagedown f1..f12,
                      ctrl-<letter> (ctrl-c, ctrl-d, ...), alt-<char>
  resize=COLSxROWS    resize the terminal (the program gets SIGWINCH)
  exit=SECONDS        wait up to SECONDS for the program to exit (fails if it does not)
  snap                print the screen now, too (to stdout, before the final one)

Exit status: 0 when every step succeeded and every --expect text is on the final screen,
1 otherwise, 2 for bad arguments. A summary goes to stderr; the screen goes to stdout.
"""

from __future__ import annotations

import argparse
import codecs
import fcntl
import os
import pty
import re
import select
import signal
import struct
import sys
import termios
import time
import unicodedata

# --- Terminal emulator ---------------------------------------------------------------------

WIDE = ("W", "F")
CONTINUATION = ""  # the right half of a wide character


def char_width(ch: str) -> int:
  if unicodedata.combining(ch) or ch in "​‌‍︎️":
    return 0
  return 2 if unicodedata.east_asian_width(ch) in WIDE else 1


class Screen:
  """A character grid that understands the escape sequences TUIs (crossterm/ratatui) emit."""

  def __init__(self, cols: int, rows: int):
    self.cols, self.rows = cols, rows
    self.main = self._blank_grid()
    self.alt = self._blank_grid()
    self.grid = self.main
    self.x = self.y = 0
    self.saved = (0, 0)
    self.top, self.bottom = 0, rows - 1
    self.pending_wrap = False
    self.replies: list[bytes] = []
    # What the alternate screen showed when the program left it (a TUI's last frame).
    self.last_alt_frame: str | None = None
    self._decoder = codecs.getincrementaldecoder("utf-8")("replace")
    self._state = "ground"
    self._seq = ""

  # Grid helpers

  def _blank_row(self) -> list[str]:
    return [" "] * self.cols

  def _blank_grid(self) -> list[list[str]]:
    return [self._blank_row() for _ in range(self.rows)]

  def resize(self, cols: int, rows: int) -> None:
    on_alt = self.grid is self.alt
    for name in ("main", "alt"):
      grid = getattr(self, name)
      grid = [(row + [" "] * cols)[:cols] for row in grid[:rows]]
      grid += [[" "] * cols for _ in range(rows - len(grid))]
      setattr(self, name, grid)
    self.grid = self.alt if on_alt else self.main
    self.cols, self.rows = cols, rows
    self.top, self.bottom = 0, rows - 1
    self.x, self.y = min(self.x, cols - 1), min(self.y, rows - 1)
    self.pending_wrap = False

  def text(self) -> str:
    lines = ["".join(cell for cell in row).rstrip() for row in self.grid]
    return "\n".join(lines)

  def _scroll_up(self, n: int = 1) -> None:
    for _ in range(n):
      del self.grid[self.top]
      self.grid.insert(self.bottom, self._blank_row())

  def _scroll_down(self, n: int = 1) -> None:
    for _ in range(n):
      del self.grid[self.bottom]
      self.grid.insert(self.top, self._blank_row())

  def _line_feed(self) -> None:
    if self.y == self.bottom:
      self._scroll_up()
    elif self.y < self.rows - 1:
      self.y += 1

  def _put(self, ch: str) -> None:
    width = char_width(ch)
    if width == 0:
      return
    if self.pending_wrap or self.x + width > self.cols:
      self.x = 0
      self._line_feed()
      self.pending_wrap = False
    row = self.grid[self.y]
    # Writing over half of a wide character clears its other half.
    if row[self.x] == CONTINUATION and self.x > 0:
      row[self.x - 1] = " "
    end = self.x + width
    if end < self.cols and row[end] == CONTINUATION:
      row[end] = " "
    row[self.x] = ch
    if width == 2:
      row[self.x + 1] = CONTINUATION
    self.x += width
    if self.x >= self.cols:
      self.x = self.cols - 1
      self.pending_wrap = True

  def _erase(self, row: int, start: int, end: int) -> None:
    line = self.grid[row]
    for col in range(max(start, 0), min(end, self.cols)):
      line[col] = " "

  # Input

  def feed(self, data: bytes) -> None:
    for ch in self._decoder.decode(data):
      self._feed_char(ch)

  def _feed_char(self, ch: str) -> None:
    state = self._state
    if state == "ground":
      if ch == "\x1b":
        self._state, self._seq = "esc", ""
      elif ch == "\r":
        self.x, self.pending_wrap = 0, False
      elif ch in "\n\x0b\x0c":
        self._line_feed()
        self.pending_wrap = False
      elif ch == "\b":
        self.x, self.pending_wrap = max(self.x - 1, 0), False
      elif ch == "\t":
        self.x = min((self.x // 8 + 1) * 8, self.cols - 1)
      elif ch >= " " and ch != "\x7f":
        self._put(ch)
    elif state == "esc":
      if ch == "[":
        self._state = "csi"
      elif ch == "]":
        self._state = "osc"
      elif ch in "PX^_":
        self._state = "string"  # DCS, SOS, PM, APC: skipped up to ST
      elif ch in "()*+#%":
        self._state = "charset"
      else:
        self._state = "ground"
        self._esc(ch)
    elif state == "charset":
      self._state = "ground"
    elif state == "csi":
      self._seq += ch
      if "@" <= ch <= "~":
        self._state = "ground"
        self._csi(self._seq)
    elif state == "osc":
      if ch == "\x07":
        self._state = "ground"
        self._osc(self._seq)
      elif ch == "\x1b":
        self._state = "osc-esc"
      else:
        self._seq += ch
    elif state == "osc-esc":
      self._state = "ground"
      self._osc(self._seq)
    elif state == "string":
      if ch == "\x1b":
        self._state = "string-esc"
      elif ch == "\x07":
        self._state = "ground"
    elif state == "string-esc":
      self._state = "ground" if ch == "\\" else "string"

  def _esc(self, ch: str) -> None:
    if ch == "7":
      self.saved = (self.x, self.y)
    elif ch == "8":
      self.x, self.y = self.saved
    elif ch == "D":
      self._line_feed()
    elif ch == "E":
      self.x = 0
      self._line_feed()
    elif ch == "M":
      if self.y == self.top:
        self._scroll_down()
      elif self.y > 0:
        self.y -= 1
    elif ch == "c":
      self.__init__(self.cols, self.rows)  # type: ignore[misc]

  def _osc(self, body: str) -> None:
    # Answer colour queries (OSC 10/11) so a host that asks does not wait for a timeout.
    code, _, value = body.partition(";")
    if value == "?" and code in ("10", "11"):
      rgb = "ffff/ffff/ffff" if code == "10" else "0000/0000/0000"
      self.replies.append(f"\x1b]{code};rgb:{rgb}\x1b\\".encode())

  def _csi(self, seq: str) -> None:
    final, body = seq[-1], seq[:-1]
    private = body[:1] if body[:1] in "?<=>" else ""
    body = body[len(private):].rstrip(" !\"#$%&'*+,-./")
    params = [int(p) if p.isdigit() else 0 for p in re.split("[;:]", body)] if body else []

    def arg(i: int = 0, default: int = 1) -> int:
      value = params[i] if i < len(params) else 0
      return value or default

    self.pending_wrap = self.pending_wrap and final in "mhl"
    if private == "?":
      if final in "hl":
        on = final == "h"
        for mode in params:
          if mode in (47, 1047, 1049):
            target = self.alt if on else self.main
            if target is not self.grid:
              if on:
                if mode == 1049:
                  self.saved = (self.x, self.y)
                self.alt = self._blank_grid()
                target = self.alt
              else:
                self.last_alt_frame = self.text()
              self.grid = target
              if not on and mode == 1049:
                self.x, self.y = self.saved
      elif final == "u":
        pass  # kitty keyboard query: not answering means "unsupported"
      return
    if private in "<=>" and private:
      if final == "c":
        pass
      return
    if final in "Hf":
      self.y = min(arg(0) - 1, self.rows - 1)
      self.x = min(arg(1) - 1, self.cols - 1)
    elif final == "A":
      self.y = max(self.y - arg(), 0)
    elif final in "Be":
      self.y = min(self.y + arg(), self.rows - 1)
    elif final in "Ca":
      self.x = min(self.x + arg(), self.cols - 1)
    elif final == "D":
      self.x = max(self.x - arg(), 0)
    elif final == "E":
      self.x, self.y = 0, min(self.y + arg(), self.rows - 1)
    elif final == "F":
      self.x, self.y = 0, max(self.y - arg(), 0)
    elif final in "G`":
      self.x = min(arg() - 1, self.cols - 1)
    elif final == "d":
      self.y = min(arg() - 1, self.rows - 1)
    elif final == "J":
      mode = arg(0, 0)
      if mode == 0:
        self._erase(self.y, self.x, self.cols)
        for row in range(self.y + 1, self.rows):
          self._erase(row, 0, self.cols)
      elif mode == 1:
        self._erase(self.y, 0, self.x + 1)
        for row in range(self.y):
          self._erase(row, 0, self.cols)
      else:
        for row in range(self.rows):
          self._erase(row, 0, self.cols)
    elif final == "K":
      mode = arg(0, 0)
      if mode == 0:
        self._erase(self.y, self.x, self.cols)
      elif mode == 1:
        self._erase(self.y, 0, self.x + 1)
      else:
        self._erase(self.y, 0, self.cols)
    elif final == "X":
      self._erase(self.y, self.x, self.x + arg())
    elif final == "P":
      row = self.grid[self.y]
      n = min(arg(), self.cols - self.x)
      del row[self.x:self.x + n]
      row.extend([" "] * n)
    elif final == "@":
      row = self.grid[self.y]
      n = min(arg(), self.cols - self.x)
      row[self.x:self.x] = [" "] * n
      del row[self.cols:]
    elif final in "LM" and self.top <= self.y <= self.bottom:
      top = self.top
      self.top = self.y
      (self._scroll_down if final == "L" else self._scroll_up)(arg())
      self.top = top
    elif final == "S":
      self._scroll_up(arg())
    elif final == "T":
      self._scroll_down(arg())
    elif final == "r":
      self.top = arg(0) - 1
      self.bottom = min(arg(1, self.rows), self.rows) - 1
      if self.top >= self.bottom:
        self.top, self.bottom = 0, self.rows - 1
      self.x = self.y = 0
    elif final == "s":
      self.saved = (self.x, self.y)
    elif final == "u":
      self.x, self.y = self.saved
    elif final == "n" and arg(0, 0) == 6:
      self.replies.append(f"\x1b[{self.y + 1};{self.x + 1}R".encode())
    elif final == "c" and arg(0, 0) == 0:
      self.replies.append(b"\x1b[?62;22c")  # primary device attributes: a VT220 with colour
    elif final == "t" and arg(0, 0) == 18:
      self.replies.append(f"\x1b[8;{self.rows};{self.cols}t".encode())


# --- Keys ----------------------------------------------------------------------------------

KEYS = {
  "enter": b"\r", "return": b"\r", "tab": b"\t", "backtab": b"\x1b[Z", "esc": b"\x1b",
  "escape": b"\x1b", "space": b" ", "backspace": b"\x7f", "delete": b"\x1b[3~",
  "up": b"\x1b[A", "down": b"\x1b[B", "right": b"\x1b[C", "left": b"\x1b[D",
  "home": b"\x1b[H", "end": b"\x1b[F", "pageup": b"\x1b[5~", "pagedown": b"\x1b[6~",
  "insert": b"\x1b[2~",
  "f1": b"\x1bOP", "f2": b"\x1bOQ", "f3": b"\x1bOR", "f4": b"\x1bOS",
  "f5": b"\x1b[15~", "f6": b"\x1b[17~", "f7": b"\x1b[18~", "f8": b"\x1b[19~",
  "f9": b"\x1b[20~", "f10": b"\x1b[21~", "f11": b"\x1b[23~", "f12": b"\x1b[24~",
}


def key_bytes(spec: str) -> bytes:
  name, _, count = spec.partition("*")
  name = name.strip().lower()
  if name in KEYS:
    data = KEYS[name]
  elif name.startswith("ctrl-") and len(name) == 6 and "a" <= name[5] <= "z":
    data = bytes([ord(name[5]) - ord("a") + 1])
  elif name.startswith("alt-") and len(name) == 5:
    data = b"\x1b" + name[4].encode()
  else:
    raise ValueError(f"unknown key {name!r}")
  return data * (int(count) if count else 1)


def parse_size(value: str) -> tuple[int, int]:
  match = re.fullmatch(r"(\d+)x(\d+)", value.strip())
  if not match:
    raise argparse.ArgumentTypeError(f"expected COLSxROWS, got {value!r}")
  return int(match.group(1)), int(match.group(2))


# --- Driver --------------------------------------------------------------------------------


class Session:
  def __init__(self, command: list[str], cols: int, rows: int, env: dict[str, str]):
    self.screen = Screen(cols, rows)
    self.raw = bytearray()
    self.status: int | None = None
    pid, fd = pty.fork()
    if pid == 0:
      try:
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
        os.execvpe(command[0], command, env)
      except OSError as error:
        os.write(2, f"pty-run: cannot run {command[0]}: {error}\n".encode())
      os._exit(127)
    self.pid, self.fd = pid, fd

  def alive(self) -> bool:
    if self.status is None:
      done, status = os.waitpid(self.pid, os.WNOHANG)
      if done:
        self.status = status
    return self.status is None

  def pump(self, seconds: float) -> None:
    """Read and render output for `seconds` (or until the program closes the PTY)."""
    deadline = time.monotonic() + seconds
    while True:
      left = deadline - time.monotonic()
      if left <= 0:
        return
      ready, _, _ = select.select([self.fd], [], [], min(left, 0.05))
      if not ready:
        continue
      try:
        data = os.read(self.fd, 65536)
      except OSError:
        data = b""
      if not data:
        self.alive()
        time.sleep(min(left, 0.05))
        if not self.alive():
          return
        continue
      self.raw += data
      self.screen.feed(data)
      for reply in self.screen.replies:
        self.send(reply)
      self.screen.replies.clear()

  def send(self, data: bytes) -> None:
    try:
      os.write(self.fd, data)
    except OSError:
      pass

  def resize(self, cols: int, rows: int) -> None:
    self.screen.resize(cols, rows)
    fcntl.ioctl(self.fd, termios.TIOCSWINSZ, struct.pack("HHHH", rows, cols, 0, 0))
    try:
      os.killpg(os.getpgid(self.pid), signal.SIGWINCH)
    except OSError:
      pass

  def wait_until(self, check, timeout: float) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
      if check():
        return True
      self.pump(0.1)
    return check()

  def stop(self) -> None:
    """Hang up like a closed terminal would, then make sure the program is gone."""
    for sig, grace in ((signal.SIGHUP, 2.0), (signal.SIGKILL, 1.0)):
      if not self.alive():
        break
      try:
        os.killpg(os.getpgid(self.pid), sig)
      except OSError:
        pass
      self.wait_until(lambda: not self.alive(), grace)
    try:
      os.close(self.fd)
    except OSError:
      pass

  def describe_exit(self) -> str:
    if self.status is None:
      return "still running"
    if os.WIFEXITED(self.status):
      return f"exited with code {os.WEXITSTATUS(self.status)}"
    return f"killed by signal {os.WTERMSIG(self.status)}"


def main() -> int:
  parser = argparse.ArgumentParser(
    description=__doc__.split("\n\n")[0],
    epilog=__doc__.split("\n\n", 1)[1],
    formatter_class=argparse.RawDescriptionHelpFormatter,
  )
  parser.add_argument("--size", type=parse_size, default=(100, 30), metavar="COLSxROWS",
                      help="terminal size (default 100x30)")
  parser.add_argument("--settle", type=float, default=2.0, metavar="SECONDS",
                      help="time to let the program draw before the first step (default 2)")
  parser.add_argument("--timeout", type=float, default=15.0, metavar="SECONDS",
                      help="limit for until=/gone= steps (default 15)")
  parser.add_argument("--step", action="append", default=[], metavar="STEP",
                      help="a step to take (repeatable; see below)")
  parser.add_argument("--expect", action="append", default=[], metavar="TEXT",
                      help="text the final screen must contain (repeatable)")
  parser.add_argument("--term", default="xterm-256color", help="TERM for the program")
  parser.add_argument("--colorterm", default="truecolor",
                      help="COLORTERM for the program ('' to unset)")
  parser.add_argument("--env", action="append", default=[], metavar="NAME=VALUE",
                      help="extra environment for the program (repeatable)")
  parser.add_argument("--raw", metavar="FILE", help="also write the raw output bytes to FILE")
  parser.add_argument("--frame", action="store_true", help="draw a border around the screen")
  parser.add_argument("command", nargs=argparse.REMAINDER, help="the command, after --")
  args = parser.parse_args()

  command = args.command[1:] if args.command[:1] == ["--"] else args.command
  if not command:
    parser.error("no command given (put it after --)")

  env = dict(os.environ)
  env["TERM"] = args.term
  env.pop("COLORTERM", None)
  if args.colorterm:
    env["COLORTERM"] = args.colorterm
  for item in args.env:
    name, sep, value = item.partition("=")
    if not sep:
      parser.error(f"--env expects NAME=VALUE, got {item!r}")
    env[name] = value

  cols, rows = args.size
  session = Session(command, cols, rows, env)
  ok = True
  failures: list[str] = []

  def show(text: str, cols: int) -> None:
    lines = text.split("\n")
    if args.frame:
      print("┌" + "─" * cols + "┐")
      for line in lines:
        pad = cols - sum(char_width(c) for c in line)
        print("│" + line + " " * max(pad, 0) + "│")
      print("└" + "─" * cols + "┘")
    else:
      print("\n".join(lines).rstrip("\n"))
    sys.stdout.flush()

  try:
    session.pump(args.settle)
    for step in args.step:
      kind, _, value = step.partition("=")
      kind = kind.strip().lower()
      if kind == "wait":
        session.pump(float(value))
      elif kind == "until":
        if not session.wait_until(lambda: value in session.screen.text(), args.timeout):
          failures.append(f"step {step!r}: not on screen after {args.timeout:g} s")
          break
      elif kind == "gone":
        if not session.wait_until(lambda: value not in session.screen.text(), args.timeout):
          failures.append(f"step {step!r}: still on screen after {args.timeout:g} s")
          break
      elif kind == "text":
        session.send(value.encode("latin-1", "backslashreplace").decode("unicode_escape").encode())
        session.pump(0.2)
      elif kind == "key":
        session.send(key_bytes(value))
        session.pump(0.2)
      elif kind == "resize":
        session.resize(*parse_size(value))
        session.pump(0.3)
      elif kind == "exit":
        if not session.wait_until(lambda: not session.alive(), float(value)):
          failures.append(f"step {step!r}: the program is still running")
          break
      elif kind == "snap":
        print(f"--- snap after {len(session.raw)} bytes ---")
        show(session.screen.text(), session.screen.cols)
      else:
        parser.error(f"unknown step {step!r}")
  except ValueError as error:
    parser.error(str(error))
  except KeyboardInterrupt:
    failures.append("interrupted")
  finally:
    # Read the screen before hanging up: a TUI leaves the alternate screen as it exits.
    screen = session.screen
    was_running = session.alive()
    text = screen.text()
    note = ""
    if not was_running and screen.grid is screen.main and screen.last_alt_frame is not None:
      text, note = screen.last_alt_frame, " (showing its last alternate-screen frame)"
    session.stop()

  show(text, screen.cols)
  for needle in args.expect:
    if needle not in text:
      failures.append(f"expected text not on screen: {needle!r}")
  if args.raw:
    with open(args.raw, "wb") as file:
      file.write(session.raw)

  state = "running at the end (stopped with SIGHUP)" if was_running else session.describe_exit()
  print(f"pty-run: {screen.cols}x{screen.rows}, {len(session.raw)} bytes, program {state}{note}",
        file=sys.stderr)
  for failure in failures:
    ok = False
    print(f"pty-run: ✗ {failure}", file=sys.stderr)
  return 0 if ok else 1


if __name__ == "__main__":
  sys.exit(main())
