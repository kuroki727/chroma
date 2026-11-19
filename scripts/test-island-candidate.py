"""Isolated writer safety tests; never address the live desktop."""

import fcntl
import hashlib
import importlib.util
import json
import os
import subprocess
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("candidate", Path(__file__).with_name("island-candidate.py"))
candidate = importlib.util.module_from_spec(spec)
spec.loader.exec_module(candidate)


class CandidateSafety(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.review = self.root / "review"
        self.review.mkdir(mode=0o700)
        self.active = self.root / "adapters/desktop-island.toml"
        self.active.parent.mkdir()
        self.active.write_bytes(b"desktop = 'island'\n")
        self.active.chmod(0o640)
        self.dependency = self.root / "colors.css"
        self.dependency.write_bytes(b"palette\n")
        for name, value in (("original", b"desktop = 'island'\n"), ("candidate", b"desktop = 'island'\nisland_readable = true\n")):
            path = self.review / name
            path.write_bytes(value)
            path.chmod(0o600)
        self.plan = {"version": 1, "active": str(self.active),
                     "original": candidate.metadata(self.active),
                     "candidate_sha256": candidate.digest(b"desktop = 'island'\nisland_readable = true\n"),
                     "guards": {str(self.dependency): candidate.metadata(self.dependency)}}
        self.plan.update(kind="adapter", checker=str(self.root / "checker"), css_candidate=str(self.root / "css-candidate"), css_sha256=candidate.digest(b"reviewed CSS"))
        Path(self.plan["checker"]).write_bytes(b"pinned checker")
        Path(self.plan["css_candidate"]).write_bytes(b"reviewed CSS")
        Path(self.plan["css_candidate"]).chmod(0o600)
        self.plan["checker_sha256"] = candidate.digest(b"pinned checker")
        self.save_plan()
        self.native = patch.object(candidate.subprocess, "run", return_value=__import__("subprocess").CompletedProcess([],0,stderr=b""))
        self.native.start()
        self.addCleanup(self.native.stop)

    def save_plan(self):
        manifest = self.review / "manifest.json"
        manifest.write_text(json.dumps(self.plan))
        manifest.chmod(0o600)
        self.pin = hashlib.sha256(manifest.read_bytes()).hexdigest()

    def run_action(self, action):
        candidate.run(action, self.review, self.pin)

    def test_round_trip_preserves_mode_backups_and_dependencies(self):
        self.run_action("check")
        self.assertEqual(self.active.read_bytes(), b"desktop = 'island'\n")
        self.run_action("install")
        self.assertEqual(self.active.read_bytes(), b"desktop = 'island'\nisland_readable = true\n")
        with self.assertRaises(RuntimeError): self.run_action("install")
        self.assertEqual(self.active.stat().st_mode & 0o777, 0o640)
        self.dependency.write_bytes(b"later palette\n")
        self.run_action("reverse")
        self.assertEqual(self.active.read_bytes(), b"desktop = 'island'\n")
        self.assertEqual((self.review / "original").read_bytes(), b"desktop = 'island'\n")
        self.assertEqual(self.dependency.read_bytes(), b"later palette\n")
        with self.assertRaises(RuntimeError):
            self.run_action("reverse")

    def test_later_edits_block_install_and_reversal(self):
        self.active.write_bytes(b"user edit\n")
        with self.assertRaises(RuntimeError):
            self.run_action("install")
        self.active.write_bytes(b"desktop = 'island'\n")
        self.run_action("install")
        self.active.write_bytes(b"later user edit\n")
        with self.assertRaises(RuntimeError):
            self.run_action("reverse")
        self.assertEqual(self.active.read_bytes(), b"later user edit\n")

    def test_changed_dependency_candidate_backup_and_manifest_refused(self):
        for path in (self.dependency, self.review / "candidate", self.review / "original", self.review / "manifest.json"):
            with self.subTest(path=path):
                before = path.read_bytes()
                path.write_bytes(b"changed")
                with self.assertRaises(RuntimeError):
                    self.run_action("install")
                path.write_bytes(before)
        self.assertEqual(self.active.read_bytes(), b"desktop = 'island'\n")

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
        moved = self.root / "moved-adapters"
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
        self.assertEqual(self.active.read_bytes(), b"desktop = 'island'\n")
        self.assertEqual(list(self.active.parent.glob(".chroma-island-*")), [])

    def test_staging_collision_does_not_delete_existing_file(self):
        path = self.active.parent / ".chroma-island-fixed.tmp"
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
        self.assertEqual(list(self.active.parent.glob(".chroma-island-*")), [])

    def test_unpreserved_extended_attributes_are_refused(self):
        os.setxattr(self.active, "user.chroma-test", b"preserve me")
        with self.assertRaises(RuntimeError):
            self.run_action("install")
        self.assertEqual(self.active.read_bytes(), b"desktop = 'island'\n")
        self.assertEqual(os.getxattr(self.active, "user.chroma-test"), b"preserve me")


    def test_parser_failure_missing_css_and_checker_changes_refuse_install(self):
        with patch.object(candidate.subprocess, "run", return_value=subprocess.CompletedProcess([],1,stderr=b"invalid CSS")):
            with self.assertRaises(RuntimeError): self.run_action("install")
        Path(self.plan["css_candidate"]).unlink()
        with self.assertRaises(FileNotFoundError): self.run_action("install")
        Path(self.plan["css_candidate"]).write_bytes(b"reviewed CSS")
        Path(self.plan["css_candidate"]).chmod(0o600)
        Path(self.plan["checker"]).write_bytes(b"modified")
        with self.assertRaises(RuntimeError): self.run_action("install")
        self.assertEqual(self.active.read_bytes(), b"desktop = 'island'\n")

    def startup_fixture(self):
        import copy
        self.plan.update(startup_dir=str(self.root / "startup"), startup_active=str(self.root / "niri/config.kdl"),
                         generated=str(self.dependency), state=str(self.root / "state/state.toml"), old_head="tx-1-1")
        directory=Path(self.plan["startup_dir"]);directory.mkdir(mode=0o700)
        original=b'spawn-at-startup "/home/kuroki/.local/bin/island"\n'
        updated=original.rstrip()+b' "-s" "/home/kuroki/.config/chroma/generated/desktop-island.css"\n'
        active=Path(self.plan["startup_active"]);active.parent.mkdir();active.write_bytes(original)
        for name,value in (("original",original),("candidate",updated)):
            (directory/name).write_bytes(value);(directory/name).chmod(0o600)
        self.plan["startup_candidate_sha256"]=candidate.digest(updated)
        self.plan["guards"][str(active)]=candidate.metadata(active)
        self.plan["css_sha256"]=candidate.digest(b"reviewed CSS")
        state=Path(self.plan["state"]);state.parent.mkdir();state.write_bytes(b"original state");state.chmod(0o600)
        self.plan["guards"][str(state)]=candidate.metadata(state)
        self.save_plan()
        self.run_action("install")
        old_bytes=list(b"palette\n");new_bytes=list(b"reviewed CSS");path=json.dumps(str(self.dependency))
        def current(data):return '{id="boats-preview", palette={name="fixture"},files={'+path+'={mode=420,bytes='+str(data)+'}}}'
        before=current(old_bytes);after=current(new_bytes)
        self.dependency.write_bytes(b"reviewed CSS")
        state.write_text('head="tx-2-2"\ncurrent='+after+'\n');state.chmod(0o600)
        transactions=state.parent/'transactions';transactions.mkdir(mode=0o700)
        tx=transactions/'tx-2-2.toml'
        tx.write_text('parent="tx-1-1"\nbefore='+before+'\nafter='+after+'\nentries=[{path='+path+',before={mode=420,bytes='+str(old_bytes)+'},after={mode=420,bytes='+str(new_bytes)+'}}]\n');tx.chmod(0o600)
        old_tx=transactions/'tx-1-1.toml';old_tx.write_text('after='+before+'\n');old_tx.chmod(0o600)
        self.plan['guards'][str(old_tx)]=candidate.metadata(old_tx);self.save_plan()
        return directory,active

    def test_startup_preparation_requires_transaction_and_preserves_later_edits(self):
        directory,active=self.startup_fixture()
        candidate.prepare_startup(self.review,self.pin)
        raw=(directory/'manifest.json').read_bytes();pin=candidate.digest(raw)
        with patch.object(candidate.subprocess,'run',return_value=subprocess.CompletedProcess([],0,stderr=b'')):
            candidate.run('install',directory,pin)
            active.write_bytes(b'later opacity edit')
            with self.assertRaises(RuntimeError):candidate.run('reverse',directory,pin)
        self.assertEqual(active.read_bytes(),b'later opacity edit')

    def test_startup_preparation_refuses_missing_profile(self):
        directory,active=self.startup_fixture()
        self.dependency.write_bytes(b'legacy CSS')
        with self.assertRaises(RuntimeError):candidate.prepare_startup(self.review,self.pin)
        self.assertFalse((directory/'manifest.json').exists())

if __name__ == "__main__":
    unittest.main()
