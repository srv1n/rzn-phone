#!/usr/bin/env python3
from __future__ import annotations

import io
import base64
import functools
import http.server
import os
import subprocess
import tarfile
import tempfile
import threading
import unittest
from pathlib import Path

import release_archive
import release_package


ROOT = Path(__file__).resolve().parents[1]
TEST_SIGNING_SEED = bytes(range(32))


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


def write_sha256(path: Path) -> Path:
    sha_path = path.with_name(f"{path.name}.sha256")
    sha_path.write_text(
        f"{release_archive.sha256_file(path)}  {path.name}\n",
        encoding="utf-8",
    )
    return sha_path


def write_test_keypair(tmp: Path) -> tuple[Path, Path]:
    private_key = tmp / "ed25519.private"
    public_key = tmp / "ed25519.public"
    private_key.write_text(
        base64.b64encode(TEST_SIGNING_SEED).decode("ascii") + "\n",
        encoding="utf-8",
    )
    public_key.write_text(
        base64.b64encode(
            release_archive.ed25519_public_from_seed(TEST_SIGNING_SEED)
        ).decode("ascii")
        + "\n",
        encoding="utf-8",
    )
    return private_key, public_key


def write_test_signature(archive: Path, private_key: Path) -> Path:
    sig_path = archive.with_name(f"{archive.name}.sig")
    release_archive.sign_file(archive, private_key, sig_path)
    return sig_path


class QuietHTTPRequestHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, format: str, *args: object) -> None:
        return None


class ReleaseArchiveSafetyTest(unittest.TestCase):
    def test_release_tar_is_path_independent(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            source = tmp / "source"
            source.mkdir()
            (source / "payload.txt").write_text("same payload\n", encoding="utf-8")
            first = tmp / "first.tar.gz"
            second = tmp / "nested" / "second.tar.gz"
            release_package.build_tar_gz(source, first, "package")
            release_package.build_tar_gz(source, second, "package")
            self.assertEqual(first.read_bytes(), second.read_bytes())

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

    def test_signature_rejects_replaced_archive_with_matching_sha256(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            private_key, public_key = write_test_keypair(tmp)
            archive = tmp / "rzn-phone-workflows-1.2.3.tar.gz"
            archive.write_bytes(b"trusted archive")
            write_sha256(archive)
            sig_path = write_test_signature(archive, private_key)

            release_archive.verify_sha256(archive, archive.with_name(f"{archive.name}.sha256"))
            release_archive.verify_signature(archive, sig_path, public_key)

            archive.write_bytes(b"attacker archive")
            write_sha256(archive)

            release_archive.verify_sha256(archive, archive.with_name(f"{archive.name}.sha256"))
            with self.assertRaises(SystemExit):
                release_archive.verify_signature(archive, sig_path, public_key)

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

    def test_installer_allows_local_archive_without_signature(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            archive = tmp / "rzn-phone-1.2.3-macos_universal.tar.gz"
            write_tar(
                archive,
                [
                    (dir_member("rzn-phone"), b""),
                    (file_member("rzn-phone/VERSION"), b"1.2.3\n"),
                    (dir_member("rzn-phone/bin"), b""),
                    (
                        file_member("rzn-phone/bin/rzn-phone", mode=0o755),
                        b"#!/usr/bin/env bash\nexit 0\n",
                    ),
                ],
            )
            write_sha256(archive)
            bin_dir = tmp / "bin"
            bin_dir.mkdir()

            result = subprocess.run(
                [
                    "bash",
                    str(ROOT / "scripts" / "install_rzn_phone.sh"),
                    "--archive",
                    str(archive),
                    "--install-root",
                    str(tmp / "install"),
                    "--bin-dir",
                    str(bin_dir),
                ],
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue((tmp / "install" / "current" / "VERSION").is_file())

    def test_installer_rejects_remote_archive_without_signature(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            archive = tmp / "rzn-phone-1.2.3-macos_universal.tar.gz"
            archive.write_bytes(b"not extracted because signature is missing")
            write_sha256(archive)
            handler = functools.partial(
                QuietHTTPRequestHandler,
                directory=str(tmp),
            )
            server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            self.addCleanup(server.server_close)
            self.addCleanup(server.shutdown)
            bin_dir = tmp / "bin"
            bin_dir.mkdir()

            result = subprocess.run(
                [
                    "bash",
                    str(ROOT / "scripts" / "install_rzn_phone.sh"),
                    "--archive",
                    f"http://127.0.0.1:{server.server_port}/{archive.name}",
                    "--version",
                    "1.2.3",
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
            self.assertFalse((tmp / "install").exists())

    def test_signed_workflow_pack_builds_expected_sidecars(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            tmp = Path(raw)
            private_key, public_key = write_test_keypair(tmp)

            result = subprocess.run(
                [
                    "python3",
                    str(ROOT / "scripts" / "build_workflow_pack.py"),
                    "--out",
                    str(tmp / "packs"),
                    "--signing-key",
                    str(private_key),
                ],
                cwd=ROOT,
                env={
                    **os.environ,
                    "RZN_PHONE_RELEASE_ALLOW_TEST_SIGNING_KEY": "1",
                },
                text=True,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
            )

            self.assertEqual(result.returncode, 0, result.stderr)
            out_dir = Path(result.stdout.strip())
            archives = list(out_dir.glob("rzn-phone-workflows-*.tar.gz"))
            self.assertEqual(len(archives), 1)
            archive = archives[0]
            self.assertTrue(archive.with_name(f"{archive.name}.sha256").is_file())
            sig_path = archive.with_name(f"{archive.name}.sig")
            self.assertTrue(sig_path.is_file())
            release_archive.verify_signature(archive, sig_path, public_key)


if __name__ == "__main__":
    unittest.main()
