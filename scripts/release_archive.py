#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import hashlib
import posixpath
import re
import sys
import tarfile
import warnings
from hashlib import sha512
from pathlib import Path


SHA256_RE = re.compile(r"^[0-9a-fA-F]{64}$")
SAFE_MODE_MASK = 0o7777
SPECIAL_MODE_BITS = 0o7000
DEFAULT_PUBLIC_KEY_PATH = Path(__file__).with_name("rzn_phone_release_ed25519.pub")

ED25519_P = 2**255 - 19
ED25519_Q = 2**252 + 27742317777372353535851937790883648493
ED25519_D = -121665 * pow(121666, ED25519_P - 2, ED25519_P) % ED25519_P
ED25519_I = pow(2, (ED25519_P - 1) // 4, ED25519_P)
ED25519_BY = 4 * pow(5, ED25519_P - 2, ED25519_P) % ED25519_P


def ed25519_xrecover(y: int) -> int:
    xx = (y * y - 1) * pow(ED25519_D * y * y + 1, ED25519_P - 2, ED25519_P)
    xx %= ED25519_P
    x = pow(xx, (ED25519_P + 3) // 8, ED25519_P)
    if (x * x - xx) % ED25519_P != 0:
        x = (x * ED25519_I) % ED25519_P
    if x & 1:
        x = ED25519_P - x
    return x


ED25519_B = (ed25519_xrecover(ED25519_BY), ED25519_BY)
ED25519_IDENTITY = (0, 1)


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def expected_sha256(sum_path: Path, archive_name: str) -> str:
    fallback: str | None = None
    for line in sum_path.read_text(encoding="utf-8").splitlines():
        fields = line.strip().split()
        if not fields:
            continue
        digest = fields[0]
        if not SHA256_RE.fullmatch(digest):
            continue
        if len(fields) == 1:
            fallback = digest.lower()
            continue
        names = [field.lstrip("*") for field in fields[1:]]
        if archive_name in names:
            return digest.lower()
    if fallback is not None:
        return fallback
    raise SystemExit(f"no sha256 entry found for {archive_name} in {sum_path}")


def verify_sha256(archive_path: Path, sum_path: Path) -> None:
    expected = expected_sha256(sum_path, archive_path.name)
    actual = sha256_file(archive_path)
    if actual.lower() != expected:
        raise SystemExit(
            f"sha256 mismatch for {archive_path.name}: expected {expected}, got {actual}"
        )


def read_base64_bytes(path: Path, *, expected_len: int, label: str) -> bytes:
    try:
        encoded = "".join(path.read_text(encoding="utf-8").split())
        raw = base64.b64decode(encoded, validate=True)
    except Exception as exc:
        raise SystemExit(f"invalid {label} at {path}: {exc}") from None
    if len(raw) != expected_len:
        raise SystemExit(
            f"invalid {label} at {path}: expected {expected_len} bytes, got {len(raw)}"
        )
    return raw


def ed25519_point_add(p: tuple[int, int], q: tuple[int, int]) -> tuple[int, int]:
    x1, y1 = p
    x2, y2 = q
    xyxy = ED25519_D * x1 * x2 * y1 * y2
    x3 = (x1 * y2 + x2 * y1) * pow(1 + xyxy, ED25519_P - 2, ED25519_P)
    y3 = (y1 * y2 + x1 * x2) * pow(1 - xyxy, ED25519_P - 2, ED25519_P)
    return (x3 % ED25519_P, y3 % ED25519_P)


def ed25519_scalar_mult(point: tuple[int, int], scalar: int) -> tuple[int, int]:
    result = ED25519_IDENTITY
    addend = point
    while scalar:
        if scalar & 1:
            result = ed25519_point_add(result, addend)
        addend = ed25519_point_add(addend, addend)
        scalar >>= 1
    return result


def ed25519_is_on_curve(point: tuple[int, int]) -> bool:
    x, y = point
    return (-x * x + y * y - 1 - ED25519_D * x * x * y * y) % ED25519_P == 0


def ed25519_encode_point(point: tuple[int, int]) -> bytes:
    x, y = point
    encoded = bytearray(y.to_bytes(32, "little"))
    encoded[31] |= (x & 1) << 7
    return bytes(encoded)


def ed25519_decode_point(encoded: bytes) -> tuple[int, int]:
    if len(encoded) != 32:
        raise ValueError("encoded point must be 32 bytes")
    y = int.from_bytes(encoded, "little") & ((1 << 255) - 1)
    sign = encoded[31] >> 7
    if y >= ED25519_P:
        raise ValueError("point encoding is non-canonical")
    x = ed25519_xrecover(y)
    if (x & 1) != sign:
        x = ED25519_P - x
    point = (x, y)
    if not ed25519_is_on_curve(point):
        raise ValueError("point is not on Ed25519 curve")
    return point


def ed25519_secret_scalar(seed: bytes) -> tuple[int, bytes]:
    h = sha512(seed).digest()
    private = bytearray(h[:32])
    private[0] &= 248
    private[31] &= 63
    private[31] |= 64
    return int.from_bytes(private, "little"), h[32:]


def ed25519_public_from_seed(seed: bytes) -> bytes:
    private, _ = ed25519_secret_scalar(seed)
    return ed25519_encode_point(ed25519_scalar_mult(ED25519_B, private))


def ed25519_sign(message: bytes, seed: bytes) -> bytes:
    if len(seed) != 32:
        raise ValueError("Ed25519 seed must be 32 bytes")
    private, prefix = ed25519_secret_scalar(seed)
    public_key = ed25519_encode_point(ed25519_scalar_mult(ED25519_B, private))
    r = int.from_bytes(sha512(prefix + message).digest(), "little") % ED25519_Q
    encoded_r = ed25519_encode_point(ed25519_scalar_mult(ED25519_B, r))
    k = int.from_bytes(sha512(encoded_r + public_key + message).digest(), "little") % ED25519_Q
    s = (r + k * private) % ED25519_Q
    return encoded_r + s.to_bytes(32, "little")


def ed25519_verify(message: bytes, signature: bytes, public_key: bytes) -> bool:
    if len(signature) != 64 or len(public_key) != 32:
        return False
    encoded_r = signature[:32]
    s = int.from_bytes(signature[32:], "little")
    if s >= ED25519_Q:
        return False
    try:
        public_point = ed25519_decode_point(public_key)
        r_point = ed25519_decode_point(encoded_r)
    except ValueError:
        return False
    k = int.from_bytes(sha512(encoded_r + public_key + message).digest(), "little") % ED25519_Q
    left = ed25519_scalar_mult(ED25519_B, s)
    right = ed25519_point_add(r_point, ed25519_scalar_mult(public_point, k))
    return ed25519_encode_point(left) == ed25519_encode_point(right)


def sign_file(archive_path: Path, key_path: Path, signature_path: Path) -> None:
    seed = read_base64_bytes(key_path, expected_len=32, label="Ed25519 private seed")
    signature = ed25519_sign(archive_path.read_bytes(), seed)
    signature_path.parent.mkdir(parents=True, exist_ok=True)
    signature_path.write_text(
        base64.b64encode(signature).decode("ascii") + "\n",
        encoding="utf-8",
    )


def verify_signature(
    archive_path: Path,
    signature_path: Path,
    public_key_path: Path = DEFAULT_PUBLIC_KEY_PATH,
) -> None:
    public_key = read_base64_bytes(
        public_key_path,
        expected_len=32,
        label="Ed25519 public key",
    )
    signature = read_base64_bytes(
        signature_path,
        expected_len=64,
        label="Ed25519 signature",
    )
    if not ed25519_verify(archive_path.read_bytes(), signature, public_key):
        raise SystemExit(f"signature verification failed for {archive_path.name}")


def validate_member(info: tarfile.TarInfo, root_name: str) -> None:
    name = info.name
    normalized = posixpath.normpath(name)
    if (
        not name
        or name.startswith("/")
        or normalized == "."
        or normalized.startswith("../")
        or "/../" in normalized
        or normalized != name.rstrip("/")
    ):
        raise SystemExit(f"unsafe archive path: {name!r}")

    parts = normalized.split("/")
    if parts[0] != root_name:
        raise SystemExit(f"archive member is outside {root_name}: {name!r}")

    if info.issym() or info.islnk():
        raise SystemExit(f"archive links are not allowed: {name!r}")
    if not (info.isdir() or info.isfile()):
        raise SystemExit(f"archive member type is not allowed: {name!r}")

    mode = info.mode & SAFE_MODE_MASK
    if mode & SPECIAL_MODE_BITS:
        raise SystemExit(f"archive member has special mode bits: {name!r}")
    if mode & 0o002:
        raise SystemExit(f"archive member is world-writable: {name!r}")


def safe_extract(archive_path: Path, dest_dir: Path, root_name: str) -> None:
    with tarfile.open(archive_path, "r:gz") as archive:
        members = archive.getmembers()
        if not members:
            raise SystemExit("archive is empty")
        for member in members:
            validate_member(member, root_name)
        with warnings.catch_warnings():
            warnings.filterwarnings("ignore", category=DeprecationWarning)
            archive.extractall(dest_dir, members=members)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Verify release archive checksums/signatures and safely extract tarballs."
    )
    subparsers = parser.add_subparsers(dest="command", required=True)

    verify_parser = subparsers.add_parser("verify-sha256")
    verify_parser.add_argument("--archive", required=True, type=Path)
    verify_parser.add_argument("--sha256", required=True, type=Path)

    signature_parser = subparsers.add_parser("verify-signature")
    signature_parser.add_argument("--archive", required=True, type=Path)
    signature_parser.add_argument("--signature", required=True, type=Path)
    signature_parser.add_argument(
        "--public-key",
        default=DEFAULT_PUBLIC_KEY_PATH,
        type=Path,
        help="Base64 Ed25519 public key path.",
    )

    sign_parser = subparsers.add_parser("sign")
    sign_parser.add_argument("--archive", required=True, type=Path)
    sign_parser.add_argument("--key", required=True, type=Path)
    sign_parser.add_argument("--signature", required=True, type=Path)

    extract_parser = subparsers.add_parser("safe-extract")
    extract_parser.add_argument("--archive", required=True, type=Path)
    extract_parser.add_argument("--dest", required=True, type=Path)
    extract_parser.add_argument("--root-name", required=True)

    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if args.command == "verify-sha256":
        verify_sha256(args.archive, args.sha256)
        return 0
    if args.command == "verify-signature":
        verify_signature(args.archive, args.signature, args.public_key)
        return 0
    if args.command == "sign":
        sign_file(args.archive, args.key, args.signature)
        return 0
    if args.command == "safe-extract":
        safe_extract(args.archive, args.dest, args.root_name)
        return 0
    raise AssertionError(args.command)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except tarfile.TarError as exc:
        print(f"invalid tar archive: {exc}", file=sys.stderr)
        raise SystemExit(1)
