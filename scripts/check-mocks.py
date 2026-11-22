#!/usr/bin/env python3
"""Check the size of every terminal mock in docs/design.

A mock is a fenced code block announced by a `<!-- mock WxH -->` comment (blank lines and a
`<!-- prettier-ignore -->` comment may sit between the two). Every line of the block must be
exactly W terminal columns wide and the block exactly H lines high. Columns are counted as the
terminal does (`unicode-width`, which the TUI uses): East Asian wide and full-width characters
take 2 columns, combining marks and zero-width characters 0, everything else 1. Trailing spaces
count: they are part of the mock's width.

Usage: scripts/check-mocks.py [DIR_OR_FILE ...]     (default: docs/design, without raw/)
Exit status: 0 when every mock fits, 1 when one does not (each problem is listed), 2 when no
mock is found at all. Standard library only.
"""

import re
import sys
import unicodedata
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MOCK = re.compile(r"^<!--\s*mock\s+(\d+)\s*[x×]\s*(\d+)\s*-->\s*$")
FENCE = re.compile(r"^(```+|~~~+)")


def columns(text: str) -> int:
  """The terminal columns `text` takes."""
  width = 0
  for char in text:
    if unicodedata.combining(char) or unicodedata.category(char) in ("Mn", "Me", "Cf"):
      continue
    width += 2 if unicodedata.east_asian_width(char) in ("W", "F") else 1
  return width


def check(path: Path) -> tuple[int, list[str]]:
  """The number of mocks in `path` and the problems found in them."""
  lines = path.read_text(encoding="utf-8").split("\n")
  label = path.relative_to(ROOT) if path.is_relative_to(ROOT) else path
  count, problems = 0, []
  i = 0
  while i < len(lines):
    match = MOCK.match(lines[i].strip())
    if not match:
      i += 1
      continue
    count += 1
    want_w, want_h = int(match[1]), int(match[2])
    where = f"{label}:{i + 1}"
    j = i + 1
    while j < len(lines) and (not lines[j].strip() or lines[j].strip() == "<!-- prettier-ignore -->"):
      j += 1
    fence = FENCE.match(lines[j]) if j < len(lines) else None
    if not fence:
      problems.append(f"{where}: mock {want_w}x{want_h} is not followed by a fenced code block")
      i = j
      continue
    body, k = [], j + 1
    while k < len(lines) and not lines[k].startswith(fence[1]):
      body.append(lines[k])
      k += 1
    if k == len(lines):
      problems.append(f"{where}: the code block of mock {want_w}x{want_h} is not closed")
      break
    if len(body) != want_h:
      problems.append(f"{where}: mock {want_w}x{want_h} has {len(body)} lines, not {want_h}")
    for n, line in enumerate(body, start=j + 2):
      width = columns(line)
      if width != want_w:
        problems.append(f"{label}:{n}: {width} columns, not {want_w} (mock at line {i + 1})")
    i = k + 1
  return count, problems


def main(argv: list[str]) -> int:
  targets = [Path(arg).resolve() for arg in argv] or [ROOT / "docs" / "design"]
  files: list[Path] = []
  for target in targets:
    if target.is_dir():
      files += sorted(p for p in target.rglob("*.md") if "raw" not in p.relative_to(target).parts)
    else:
      files.append(target)
  total, problems = 0, []
  for path in files:
    count, found = check(path)
    total += count
    problems += found
  for problem in problems:
    print(problem)
  if total == 0:
    print("check-mocks: no `<!-- mock WxH -->` block found", file=sys.stderr)
    return 2
  status = "ok" if not problems else f"{len(problems)} problem(s)"
  print(f"check-mocks: {total} mocks in {len(files)} files: {status}", file=sys.stderr)
  return 1 if problems else 0


if __name__ == "__main__":
  sys.exit(main(sys.argv[1:]))
