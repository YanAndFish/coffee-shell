"""测试端到端报告可以写入和清理哪些目录。"""

import tempfile
import unittest
from pathlib import Path

from run import MARKER, prepare_out


class ReportDirectoryTests(unittest.TestCase):
  def test_first_run_preserves_files_in_directories_named_like_output(self):
    for name in ("work", "main", "run-1", "notes"):
      with self.subTest(name=name), tempfile.TemporaryDirectory() as temp:
        out = Path(temp)
        note = out / name / "meeting.txt"
        note.parent.mkdir()
        note.write_text("会议记录")
        with self.assertRaisesRegex(ValueError, "请选择空目录"):
          prepare_out(out)
        self.assertEqual(note.read_text(), "会议记录")
        self.assertFalse((out / MARKER).exists())

  def test_marked_report_replaces_only_run_output(self):
    with tempfile.TemporaryDirectory() as temp:
      out = Path(temp) / "report"
      prepare_out(out)
      note = out / "notes.txt"
      note.write_text("会议记录")
      for name in ("work", "main", "run-1"):
        (out / name).mkdir()
        (out / name / "log.txt").touch()
      prepare_out(out)
      self.assertEqual(set(path.name for path in out.iterdir()), {MARKER, note.name})
      self.assertEqual(note.read_text(), "会议记录")

  def test_invalid_marker_preserves_existing_output(self):
    with tempfile.TemporaryDirectory() as temp:
      root = Path(temp)
      prepare_out(root / "report")
      out = root / "other"
      out.mkdir()
      note = out / "work"
      note.write_text("会议记录")
      marker = out / MARKER
      marker.write_text("其他用途")
      with self.assertRaises(ValueError):
        prepare_out(out)
      marker.unlink()
      marker.symlink_to(root / "report" / MARKER)
      with self.assertRaises(ValueError):
        prepare_out(out)
      self.assertEqual(note.read_text(), "会议记录")
