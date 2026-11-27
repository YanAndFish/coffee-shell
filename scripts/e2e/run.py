#!/usr/bin/env python3
"""End-to-end flows of ROAST/TTY over the real stack (`pnpm test:e2e:real`).

For each flow: a copy of a freshly migrated and seeded SQLite database, the backend
(`node apps/server/src/main.ts`) and `tty-gateway` (`deploy/gateway.toml`, see harness.py) on
free ports, and stock `ssh` clients on pseudo-terminals driven by what their screens show.
After a flow: no session program may be left, the logs may hold no password, token or
internal key, and no screen or written configuration the internal key.

  python3 scripts/e2e/run.py                 # every flow once
  python3 scripts/e2e/run.py main money      # some flows
  python3 scripts/e2e/run.py --repeat 5      # stability: the whole set five times

Needs the debug binaries (`cargo build -p coffee-tui -p tty-gateway`; `pnpm test:e2e:real`
builds them), `pnpm install` done, `ssh` and `ssh-keygen`. Output (text screenshots, logs,
FAIL.txt on failure) goes to `.data/e2e/` or `--out`: a new or empty directory, or an earlier
report, whose own entries are replaced and whose other files are kept.
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from flows import DEFAULT, EXTERNAL, FLOWS  # noqa: E402
from harness import (ROOT, Failure, Stack, cargo_target_dir, check_sync_frames,  # noqa: E402
                     check_toml_writer, fail_report, log, make_template_db, wait_until)
from shop import PASSWORD  # noqa: E402

FULL_FINGERPRINT = re.compile(r"SHA256:[A-Za-z0-9+/]{43}")


def check_logs(stack: Stack) -> None:
  """Nothing secret in the backend's or the gateway's log, and the internal key nowhere in
  the flow's files (screens, the gateway's configuration). The gateway logs full key
  fingerprints by design (ADR 0001); the backend only shortened ones."""
  backend = stack.backend.log.read_text(errors="replace") if stack.backend else ""
  gateway = stack.gateway_log()
  for name, text in (("backend", backend), ("gateway", gateway)):
    for secret, what in ((PASSWORD, "a password"), ("wrong-pass-", "a password"),
                         (stack.internal_key, "the internal key"), ("Bearer", "a token")):
      if secret in text:
        raise Failure(f"the {name} log contains {what}")
  if FULL_FINGERPRINT.search(backend):
    raise Failure("the backend log contains a full key fingerprint")
  for path in sorted(stack.flow_dir.iterdir()):
    if path.is_file() and path.suffix in (".txt", ".toml"):
      if stack.internal_key in path.read_text(errors="replace"):
        raise Failure(f"{path.name} contains the internal key")


def run_flow(name: str, out: Path, work: Path, template: Path | None, bins: Path | None,
             gateway: tuple[str, int] | None) -> tuple[bool, float]:
  flow_dir = out / name
  if flow_dir.exists():
    shutil.rmtree(flow_dir)
  started = time.monotonic()
  stack = Stack(work, template, flow_dir, bins, gateway)
  ok = True
  try:
    with stack:
      FLOWS[name](stack)
      for term in stack.terms:
        term.close()
      if not wait_until(lambda: not stack.session_pids(), 5):
        raise Failure(f"session programs left running after every client left: "
                      f"{stack.session_pids()}")
    if not gateway:
      check_logs(stack)
  except Failure as failure:
    ok = False
    report = fail_report(failure, stack)
    (flow_dir / "FAIL.txt").write_text(report + "\n")
    print(report, file=sys.stderr)
  except Exception as error:  # a bug in a flow: still stop the stack and report it
    ok = False
    report = fail_report(Failure(f"{type(error).__name__}: {error}"), stack)
    (flow_dir / "FAIL.txt").write_text(report + "\n")
    print(report, file=sys.stderr)
    import traceback
    traceback.print_exc()
  finally:
    stack.close()
  screens = [term.session.screen for term in stack.terms]
  frames = sum(screen.frames for screen in screens)
  split = sum(screen.split_frames for screen in screens)
  if split:
    log(f"e2e:   {split} reads ended inside a frame ({frames} frames): read whole")
  return ok, time.monotonic() - started


MARKER = ".e2e-report"
MARKER_TEXT = (b"Written by scripts/e2e/run.py: the next run here replaces its "
               b"work/, flow and run-N/ directories and keeps everything else.\n")
RUN_DIR = re.compile(r"run-\d+")


def own_entry(name: str) -> bool:
  """判断顶层条目是否属于每次运行产生的输出。"""
  return name == "work" or name in FLOWS or bool(RUN_DIR.fullmatch(name))


def prepare_out(out: Path) -> None:
  """在空目录中创建报告标记，或清理已标记报告中的运行输出。"""
  if out.exists() and not out.is_dir():
    raise ValueError(f"--out {out} is not a directory")
  out.mkdir(parents=True, exist_ok=True)
  names = sorted(path.name for path in out.iterdir())
  marker = out / MARKER
  if not names:
    with marker.open("xb") as file:
      file.write(MARKER_TEXT)
    return
  marked = False
  if not marker.is_symlink() and marker.is_file():
    with marker.open("rb") as file:
      marked = file.read(len(MARKER_TEXT) + 1) == MARKER_TEXT
  if not marked:
    raise ValueError(f"--out {out} 已有文件，且缺少有效的报告标记。请选择空目录或新路径。")
  for name in names:
    if own_entry(name):
      path = out / name
      if path.is_dir() and not path.is_symlink():
        shutil.rmtree(path)
      else:
        path.unlink()


def main() -> int:
  parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0],
                                   epilog="Flows: " + ", ".join(DEFAULT) + " (default); "
                                   + ", ".join(EXTERNAL) + " (with --gateway)",
                                   formatter_class=argparse.RawDescriptionHelpFormatter)
  parser.add_argument("flows", nargs="*", help="flows to run (default: all)")
  parser.add_argument("--repeat", type=int, default=1, help="run the set N times")
  parser.add_argument("--out", type=Path, default=ROOT / ".data" / "e2e",
                      help="screenshots and logs (default .data/e2e)")
  parser.add_argument("--gateway", metavar="HOST:PORT",
                      help="run against a running gateway (only: " + ", ".join(EXTERNAL) + ")")
  args = parser.parse_args()
  unknown = [name for name in args.flows if name not in FLOWS]
  if unknown:
    parser.error(f"unknown flow(s) {unknown}; known: {', '.join(FLOWS)}")
  gateway = None
  if args.gateway:
    host, _, port = args.gateway.rpartition(":")
    if not host or not port.isdigit():
      parser.error("--gateway expects HOST:PORT")
    gateway = (host, int(port))
    names = args.flows or EXTERNAL
    if any(name not in EXTERNAL for name in names):
      parser.error(f"with --gateway only {', '.join(EXTERNAL)} can run")
  else:
    names = args.flows or DEFAULT

  tools = ["ssh", "ssh-keygen"] + ([] if gateway else ["node"])
  for tool in tools:
    if not shutil.which(tool):
      parser.error(f"{tool} is not on PATH")
  bins = None
  if not gateway:
    bins = Path(os.environ.get("E2E_BIN_DIR") or cargo_target_dir() / "debug")
    for binary in ("coffee-tui", "tty-gateway"):
      if not (bins / binary).is_file():
        parser.error(f"{bins / binary} is not built: cargo build -p coffee-tui -p tty-gateway")

  out = args.out.resolve()
  try:
    prepare_out(out)
  except ValueError as error:
    parser.error(str(error))
  work = out / "work"
  work.mkdir(parents=True)
  template = None
  check_sync_frames()
  if not gateway:
    check_toml_writer()
    log(f"e2e: preparing the template database in {work}")
    template = make_template_db(work)

  results: list[tuple[int, str, bool, float]] = []
  for run in range(1, args.repeat + 1):
    run_out = out if args.repeat == 1 else out / f"run-{run}"
    for name in names:
      log(f"e2e: [{run}/{args.repeat}] {name} …")
      ok, seconds = run_flow(name, run_out, work, template, bins, gateway)
      results.append((run, name, ok, seconds))
      log(f"e2e: [{run}/{args.repeat}] {name} {'ok' if ok else 'FAILED'} in {seconds:.1f} s")

  log("")
  log("| run | flow | result | seconds |")
  log("| --- | --- | --- | --- |")
  for run, name, ok, seconds in results:
    log(f"| {run} | {name} | {'pass' if ok else 'FAIL'} | {seconds:.1f} |")
  failed = sum(1 for *_, ok, _ in results if not ok)
  log(f"\ne2e: {len(results) - failed} passed, {failed} failed; screens and logs in {out}")
  return 1 if failed else 0


if __name__ == "__main__":
  sys.exit(main())
