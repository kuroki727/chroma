"""Isolated writer safety tests; never address the live desktop."""

import fcntl
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("candidate", Path(__file__).with_name("waybar-candidate.py"))
candidate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(candidate)


class CandidateSafety(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.review = self.root / "review"
        self.review.mkdir(mode=0o700)
        self.active = self.root / "waybar/style.css"
        self.active.parent.mkdir()
        self.active.write_bytes(b"original\n")
        self.active.chmod(0o640)
        self.dependency = self.root / "colors.css"
        self.dependency.write_bytes(b"palette\n")
        for name, value in (("original.css", b"original\n"), ("candidate.css", b"candidate\n")):
            path = self.review / name
            path.write_bytes(value)
            path.chmod(0o600)
        self.plan = {"version": 1, "active": str(self.active),
                     "original": candidate.metadata(self.active),
                     "candidate_sha256": candidate.digest(b"candidate\n"),
                     "guards": {str(self.dependency): candidate.metadata(self.dependency)}}
        self.save_plan()

    def save_plan(self):
        manifest = self.review / "manifest.json"
        manifest.write_text(json.dumps(self.plan))
        manifest.chmod(0o600)
        self.pin = hashlib.sha256(manifest.read_bytes()).hexdigest()

    def run_action(self, action):
        candidate.run(action, self.review, self.pin)

    def test_round_trip_preserves_mode_backups_and_dependencies(self):
        self.run_action("check")
        self.assertEqual(self.active.read_bytes(), b"original\n")
        self.run_action("install")
        self.assertEqual(self.active.read_bytes(), b"candidate\n")
        self.assertEqual(self.active.stat().st_mode & 0o777, 0o640)
        self.dependency.write_bytes(b"later palette\n")
        self.run_action("reverse")
        self.assertEqual(self.active.read_bytes(), b"original\n")
        self.assertEqual((self.review / "original.css").read_bytes(), b"original\n")
        self.assertEqual(self.dependency.read_bytes(), b"later palette\n")
        with self.assertRaises(RuntimeError):
            self.run_action("reverse")

    def test_later_edits_block_install_and_reversal(self):
        self.active.write_bytes(b"user edit\n")
        with self.assertRaises(RuntimeError):
            self.run_action("install")
        self.active.write_bytes(b"original\n")
        self.run_action("install")
        self.active.write_bytes(b"later user edit\n")
        with self.assertRaises(RuntimeError):
            self.run_action("reverse")
        self.assertEqual(self.active.read_bytes(), b"later user edit\n")

    def test_changed_dependency_candidate_backup_and_manifest_refused(self):
        for path in (self.dependency, self.review / "candidate.css", self.review / "original.css", self.review / "manifest.json"):
            with self.subTest(path=path):
                before = path.read_bytes()
                path.write_bytes(b"changed")
                with self.assertRaises(RuntimeError):
                    self.run_action("install")
                path.write_bytes(before)
        self.assertEqual(self.active.read_bytes(), b"original\n")

    def test_leaf_parent_symlinks_and_hardlinks_refused(self):
        original = self.root / "saved.css"
        self.active.rename(original)
        self.active.symlink_to(original)
        with self.assertRaises(OSError):
            self.run_action("install")
        self.active.unlink()
        os.link(original, self.active)
        with self.assertRaises(RuntimeError):
            self.run_action("install")
        self.active.unlink()
        original.rename(self.active)
        moved = self.root / "moved-waybar"
        self.active.parent.rename(moved)
        self.active.parent.symlink_to(moved, target_is_directory=True)
        with self.assertRaises(OSError):
            self.run_action("install")

    def test_modes_ownership_traversal_and_concurrency_refused(self):
        self.active.chmod(0o644)
        with self.assertRaises(RuntimeError):
            self.run_action("install")
        self.active.chmod(0o640)
        with patch.object(candidate.os, "getgid", return_value=os.getgid() + 1):
            with self.assertRaises(RuntimeError):
                self.run_action("install")
        with self.assertRaises(RuntimeError):
            candidate.absolute(str(self.root) + "/../style.css")
        with candidate.directory(self.review) as fd:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            with self.assertRaises(BlockingIOError):
                self.run_action("install")

    def test_staging_failure_preserves_active_and_cleans_temporary(self):
        with patch.object(candidate.os, "fsync", side_effect=OSError("simulated full disk")):
            with self.assertRaises(OSError):
                self.run_action("install")
        self.assertEqual(self.active.read_bytes(), b"original\n")
        self.assertEqual(list(self.active.parent.glob(".chroma-waybar-*")), [])

    def test_staging_collision_does_not_delete_existing_file(self):
        path = self.active.parent / ".chroma-waybar-fixed.tmp"
        path.write_bytes(b"unrelated")
        with patch.object(candidate.secrets, "token_hex", return_value="fixed"):
            with self.assertRaises(FileExistsError):
                self.run_action("install")
        self.assertEqual(path.read_bytes(), b"unrelated")

    def test_edit_during_staging_is_preserved(self):
        def edit(_fd):
            self.active.write_bytes(b"concurrent user edit\n")
        with patch.object(candidate.os, "fsync", side_effect=edit):
            with self.assertRaises(RuntimeError):
                self.run_action("install")
        self.assertEqual(self.active.read_bytes(), b"concurrent user edit\n")
        self.assertEqual(list(self.active.parent.glob(".chroma-waybar-*")), [])

    def test_unpreserved_extended_attributes_are_refused(self):
        os.setxattr(self.active, "user.chroma-test", b"preserve me")
        with self.assertRaises(RuntimeError):
            self.run_action("install")
        self.assertEqual(self.active.read_bytes(), b"original\n")
        self.assertEqual(os.getxattr(self.active, "user.chroma-test"), b"preserve me")


if __name__ == "__main__":
    unittest.main()
