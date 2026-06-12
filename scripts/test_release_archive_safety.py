#!/usr/bin/env python3
from __future__ import annotations

import io
import subprocess
import tarfile
import tempfile
import unittest
from pathlib import Path

import release_archive


ROOT = Path(__file__).resolve().parents[1]


def write_tar(path: Path, members: list[tuple[tarfile.TarInfo, bytes]]) -> None:
    with tarfile.open(path, "w:gz") as archive:
        for info, payload in members:
            if info.isfile():
                info.size = len(payload)
                archive.addfile(info, io.BytesIO(payload))
            else:
                archive.addfile(info)


def file_member(name: str, mode: int = 0o644, payload: bytes = b"ok") -> tarfile.TarInfo:
    info = tarfile.TarInfo(name)
    info.type = tarfile.REGTYPE
    info.mode = mode
    return info


def dir_member(name: str, mode: int = 0o755) -> tarfile.TarInfo:
    info = tarfile.TarInfo(name)
    info.type = tarfile.DIRTYPE
    info.mode = mode
    return info


class ReleaseArchiveSafetyTest(unittest.TestCase):
    def test_safe_archive_extracts_under_expected_root(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            archive = tmp / "archive.tar.gz"
            write_tar(
                archive,
                [
                    (dir_member("rzn-phone"), b""),
                    (file_member("rzn-phone/VERSION"), b"1.2.3\n"),
                ],
            )

            dest = tmp / "dest"
            dest.mkdir()
            release_archive.safe_extract(archive, dest, "rzn-phone")

            self.assertEqual((dest / "rzn-phone" / "VERSION").read_text(), "1.2.3\n")

    def test_sha256_mismatch_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            archive = tmp / "rzn-phone-1.2.3-macos_universal.tar.gz"
            archive.write_bytes(b"not the expected bytes")
            sums = tmp / f"{archive.name}.sha256"
            sums.write_text(f"{'0' * 64}  {archive.name}\n", encoding="utf-8")

            with self.assertRaises(SystemExit):
                release_archive.verify_sha256(archive, sums)

    def test_malicious_members_are_rejected(self) -> None:
        cases: list[tuple[str, tarfile.TarInfo]] = [
            ("absolute", file_member("/tmp/pwned")),
            ("traversal", file_member("rzn-phone/../pwned")),
            ("wrong_root", file_member("other/VERSION")),
            ("world_writable", file_member("rzn-phone/VERSION", mode=0o666)),
        ]

        symlink = tarfile.TarInfo("rzn-phone/link")
        symlink.type = tarfile.SYMTYPE
        symlink.linkname = "VERSION"
        symlink.mode = 0o777
        cases.append(("symlink", symlink))

        hardlink = tarfile.TarInfo("rzn-phone/hardlink")
        hardlink.type = tarfile.LNKTYPE
        hardlink.linkname = "rzn-phone/VERSION"
        hardlink.mode = 0o644
        cases.append(("hardlink", hardlink))

        for label, member in cases:
            with self.subTest(label=label), tempfile.TemporaryDirectory() as raw:
                tmp = Path(raw)
                archive = tmp / "archive.tar.gz"
                write_tar(archive, [(member, b"bad")])

                with self.assertRaises(SystemExit):
                    release_archive.safe_extract(archive, tmp / "dest", "rzn-phone")

    def test_installer_rejects_unsafe_version_before_path_use(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            stage = tmp / "stage"
            bin_dir = tmp / "bin"
            stage.mkdir()
            bin_dir.mkdir()

            result = subprocess.run(
                [
                    "bash",
                    str(ROOT / "scripts" / "install_rzn_phone.sh"),
                    "--stage",
                    str(stage),
                    "--version",
                    "../1.2.3",
                    "--install-root",
                    str(tmp / "install"),
                    "--bin-dir",
                    str(bin_dir),
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            self.assertNotEqual(result.returncode, 0)
            self.assertIn("invalid release version", result.stderr)
            self.assertFalse((tmp / "install").exists())


if __name__ == "__main__":
    unittest.main()
