#!/usr/bin/env bash
set -euo pipefail

INSTALL_ROOT="${RZN_PHONE_INSTALL_ROOT:-$HOME/.local/share/rzn-phone}"
STATE_DIR="${RZN_PHONE_STATE_DIR:-$HOME/.rzn-phone}"
SCRIPT_PATH="${BASH_SOURCE[0]:-$0}"
SCRIPT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" 2>/dev/null && pwd || pwd)"
RELEASE_ARCHIVE_HELPER="$SCRIPT_DIR/release_archive.py"
RELEASE_PUBLIC_KEY="$SCRIPT_DIR/rzn_phone_release_ed25519.pub"
RELEASE_REPO="${RZN_PHONE_RELEASE_REPO:-srv1n/rzn-phone}"
BIN_DIR="${RZN_PHONE_BIN_DIR:-}"
PLATFORM="macos_universal"
VERSION=""
SOURCE=""
ARCHIVE=""
STAGE=""
UPDATE_SOURCE=""
UNINSTALL="0"
PURGE_STATE="0"

usage() {
  cat <<'EOF'
Usage: scripts/install_rzn_phone.sh [options]

Install rzn-phone into a versioned local runtime and expose a global `rzn-phone` shim.
With no source options, installs the latest GitHub release from srv1n/rzn-phone.

Options:
  --stage <dir>            Install from an unpacked release directory.
  --archive <path|url>     Install from a release tarball.
  --source <path|url>      Release directory base used to resolve VERSION + tarball names.
  --version <version>      Release version to install. Optional when VERSION can be discovered.
  --update-source <value>  Persist workflow update source for `rzn-phone workflows update`.
  --install-root <dir>     Override install root (default: ~/.local/share/rzn-phone).
  --bin-dir <dir>          Override shim directory.
  --uninstall              Remove the installed runtime and installer-managed shim.
  --purge-state            With --uninstall, also remove local history/favorites (~/.rzn-phone).
  --state-dir <dir>        Override state dir used by --purge-state.
  -h, --help               Show this help.

Examples:
  curl -fsSL https://raw.githubusercontent.com/srv1n/rzn-phone/main/scripts/install_rzn_phone.sh | bash
  curl -fsSL https://raw.githubusercontent.com/srv1n/rzn-phone/main/scripts/install_rzn_phone.sh | bash -s -- --uninstall
EOF
}

fail() {
  echo "rzn-phone install: $*" >&2
  exit 1
}

expand_path() {
  local raw="$1"
  case "$raw" in
    "~")
      printf '%s\n' "$HOME"
      ;;
    "~/"*)
      printf '%s/%s\n' "$HOME" "${raw#~/}"
      ;;
    *)
      printf '%s\n' "$raw"
      ;;
  esac
}

read_source_text() {
  local source="$1"
  case "$source" in
    http://*|https://*)
      curl -fsSL "$source"
      ;;
    file://*)
      local path="${source#file://}"
      cat "$path"
      ;;
    *)
      cat "$(expand_path "$source")"
      ;;
  esac
}

read_source_to_file() {
  local source="$1"
  local target="$2"
  case "$source" in
    http://*|https://*)
      curl -fsSL "$source" -o "$target"
      ;;
    file://*)
      cp "${source#file://}" "$target"
      ;;
    *)
      cp "$(expand_path "$source")" "$target"
      ;;
  esac
}

github_latest_tag() {
  local api payload tag
  api="${RZN_PHONE_RELEASE_API:-https://api.github.com/repos/${RELEASE_REPO}/releases/latest}"
  payload="$(curl -fsSL "$api")" || fail "unable to read latest release from $api"
  tag="$(
    printf '%s\n' "$payload" \
      | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' \
      | head -n 1
  )"
  [[ -n "$tag" ]] || fail "unable to determine latest release tag from $api"
  printf '%s\n' "$tag"
}

configure_latest_release_source() {
  local tag
  tag="$(github_latest_tag)"
  VERSION="${tag#v}"
  SOURCE="${RZN_PHONE_RELEASE_BASE:-https://github.com/${RELEASE_REPO}/releases/download/${tag}}"
  UPDATE_SOURCE="${UPDATE_SOURCE:-$SOURCE}"
}

validate_release_version() {
  local version="$1"
  if [[ ! "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?(\+[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?$ ]]; then
    fail "invalid release version: $version"
  fi
}

assert_supported_platform() {
  local os arch
  os="$(uname -s 2>/dev/null || true)"
  arch="$(uname -m 2>/dev/null || true)"

  if [[ "$os" != "Darwin" ]]; then
    fail "unsupported platform: rzn-phone ${PLATFORM} install requires macOS, got ${os:-unknown}"
  fi
  case "$arch" in
    arm64|x86_64)
      ;;
    *)
      fail "unsupported macOS architecture for ${PLATFORM}: ${arch:-unknown}"
      ;;
  esac
}

release_archive_tool() {
  if [[ -f "$RELEASE_ARCHIVE_HELPER" ]]; then
    python3 "$RELEASE_ARCHIVE_HELPER" "$@"
    return $?
  fi

  python3 - "$@" <<'PY'
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
DEFAULT_PUBLIC_KEY_B64 = "1p/wWuPPELkYlRb6lojwXDCmFp3ziDxq1haj4RiV3FY="
ED25519_P = 2**255 - 19
ED25519_Q = 2**252 + 27742317777372353535851937790883648493
ED25519_D = -121665 * pow(121666, ED25519_P - 2, ED25519_P) % ED25519_P
ED25519_I = pow(2, (ED25519_P - 1) // 4, ED25519_P)
ED25519_BY = 4 * pow(5, ED25519_P - 2, ED25519_P) % ED25519_P

def ed25519_xrecover(y):
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

def sha256_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()

def expected_sha256(sum_path, archive_name):
    fallback = None
    for line in sum_path.read_text(encoding="utf-8").splitlines():
        fields = line.strip().split()
        if not fields or not SHA256_RE.fullmatch(fields[0]):
            continue
        if len(fields) == 1:
            fallback = fields[0].lower()
            continue
        if archive_name in [field.lstrip("*") for field in fields[1:]]:
            return fields[0].lower()
    if fallback:
        return fallback
    raise SystemExit(f"no sha256 entry found for {archive_name} in {sum_path}")

def verify_sha256(archive_path, sum_path):
    expected = expected_sha256(sum_path, archive_path.name)
    actual = sha256_file(archive_path)
    if actual.lower() != expected:
        raise SystemExit(f"sha256 mismatch for {archive_path.name}: expected {expected}, got {actual}")

def read_base64_bytes(path, expected_len, label):
    try:
        encoded = "".join(Path(path).read_text(encoding="utf-8").split())
        raw = base64.b64decode(encoded, validate=True)
    except Exception as exc:
        raise SystemExit(f"invalid {label} at {path}: {exc}") from None
    if len(raw) != expected_len:
        raise SystemExit(f"invalid {label} at {path}: expected {expected_len} bytes, got {len(raw)}")
    return raw

def default_public_key():
    return base64.b64decode(DEFAULT_PUBLIC_KEY_B64, validate=True)

def ed25519_point_add(p, q):
    x1, y1 = p
    x2, y2 = q
    xyxy = ED25519_D * x1 * x2 * y1 * y2
    x3 = (x1 * y2 + x2 * y1) * pow(1 + xyxy, ED25519_P - 2, ED25519_P)
    y3 = (y1 * y2 + x1 * x2) * pow(1 - xyxy, ED25519_P - 2, ED25519_P)
    return (x3 % ED25519_P, y3 % ED25519_P)

def ed25519_scalar_mult(point, scalar):
    result = ED25519_IDENTITY
    addend = point
    while scalar:
        if scalar & 1:
            result = ed25519_point_add(result, addend)
        addend = ed25519_point_add(addend, addend)
        scalar >>= 1
    return result

def ed25519_is_on_curve(point):
    x, y = point
    return (-x * x + y * y - 1 - ED25519_D * x * x * y * y) % ED25519_P == 0

def ed25519_encode_point(point):
    x, y = point
    encoded = bytearray(y.to_bytes(32, "little"))
    encoded[31] |= (x & 1) << 7
    return bytes(encoded)

def ed25519_decode_point(encoded):
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

def ed25519_verify(message, signature, public_key):
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

def verify_signature(archive_path, signature_path, public_key_path=None):
    if public_key_path and Path(public_key_path).is_file():
        public_key = read_base64_bytes(public_key_path, 32, "Ed25519 public key")
    else:
        public_key = default_public_key()
    signature = read_base64_bytes(signature_path, 64, "Ed25519 signature")
    if not ed25519_verify(archive_path.read_bytes(), signature, public_key):
        raise SystemExit(f"signature verification failed for {archive_path.name}")

def validate_member(info, root_name):
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
    if normalized.split("/")[0] != root_name:
        raise SystemExit(f"archive member is outside {root_name}: {name!r}")
    if info.issym() or info.islnk():
        raise SystemExit(f"archive links are not allowed: {name!r}")
    if not (info.isdir() or info.isfile()):
        raise SystemExit(f"archive member type is not allowed: {name!r}")
    mode = info.mode & 0o7777
    if mode & 0o7000:
        raise SystemExit(f"archive member has special mode bits: {name!r}")
    if mode & 0o002:
        raise SystemExit(f"archive member is world-writable: {name!r}")

def safe_extract(archive_path, dest_dir, root_name):
    with tarfile.open(archive_path, "r:gz") as archive:
        members = archive.getmembers()
        if not members:
            raise SystemExit("archive is empty")
        for member in members:
            validate_member(member, root_name)
        with warnings.catch_warnings():
            warnings.filterwarnings("ignore", category=DeprecationWarning)
            archive.extractall(dest_dir, members=members)

parser = argparse.ArgumentParser()
subparsers = parser.add_subparsers(dest="command", required=True)
verify = subparsers.add_parser("verify-sha256")
verify.add_argument("--archive", required=True, type=Path)
verify.add_argument("--sha256", required=True, type=Path)
signature = subparsers.add_parser("verify-signature")
signature.add_argument("--archive", required=True, type=Path)
signature.add_argument("--signature", required=True, type=Path)
signature.add_argument("--public-key", type=Path)
extract = subparsers.add_parser("safe-extract")
extract.add_argument("--archive", required=True, type=Path)
extract.add_argument("--dest", required=True, type=Path)
extract.add_argument("--root-name", required=True)
args = parser.parse_args()
try:
    if args.command == "verify-sha256":
        verify_sha256(args.archive, args.sha256)
    elif args.command == "verify-signature":
        verify_signature(args.archive, args.signature, args.public_key)
    else:
        safe_extract(args.archive, args.dest, args.root_name)
except tarfile.TarError as exc:
    print(f"invalid tar archive: {exc}", file=sys.stderr)
    raise SystemExit(1)
PY
}

discover_version() {
  if [[ -n "$VERSION" ]]; then
    printf '%s\n' "$VERSION"
    return 0
  fi

  if [[ -n "$STAGE" && -f "$STAGE/VERSION" ]]; then
    tr -d '\n' <"$STAGE/VERSION"
    return 0
  fi

  if [[ -n "$SOURCE" ]]; then
    local version_ref="$SOURCE"
    case "$SOURCE" in
      http://*|https://*|file://*)
        version_ref="${SOURCE%/}/VERSION"
        ;;
      *)
        local expanded
        expanded="$(expand_path "$SOURCE")"
        if [[ -d "$expanded" ]]; then
          version_ref="$expanded/VERSION"
        fi
        ;;
    esac

    local version_tmp
    version_tmp="$(mktemp /tmp/rzn-phone-version.XXXXXX)"
    if read_source_text "$version_ref" >"$version_tmp" 2>/dev/null; then
      tr -d '\n' <"$version_tmp"
      rm -f "$version_tmp"
      return 0
    fi
    rm -f "$version_tmp"
  fi

  if [[ -n "$ARCHIVE" ]]; then
    local name
    name="$(basename "$ARCHIVE")"
    case "$name" in
      rzn-phone-*-macos_universal.tar.gz)
        name="${name#rzn-phone-}"
        name="${name%-macos_universal.tar.gz}"
        printf '%s\n' "$name"
        return 0
        ;;
    esac
  fi

  fail "unable to determine version; pass --version or provide a source with VERSION"
}

resolve_archive_ref() {
  local version="$1"
  local archive_name="rzn-phone-${version}-${PLATFORM}.tar.gz"

  if [[ -n "$ARCHIVE" ]]; then
    printf '%s\n' "$ARCHIVE"
    return 0
  fi

  [[ -n "$SOURCE" ]] || fail "missing install source; pass --stage, --archive, or --source"

  case "$SOURCE" in
    http://*|https://*|file://*)
      if [[ "$SOURCE" == *.tar.gz ]]; then
        printf '%s\n' "$SOURCE"
      else
        printf '%s/%s\n' "${SOURCE%/}" "$archive_name"
      fi
      ;;
    *)
      local expanded
      expanded="$(expand_path "$SOURCE")"
      if [[ -d "$expanded" ]]; then
        printf '%s/%s\n' "$expanded" "$archive_name"
      else
        printf '%s\n' "$expanded"
      fi
      ;;
  esac
}

resolve_sha_ref() {
  local archive_ref="$1"
  printf '%s.sha256\n' "$archive_ref"
}

resolve_sig_ref() {
  local archive_ref="$1"
  printf '%s.sig\n' "$archive_ref"
}

is_remote_ref() {
  case "$1" in
    http://*|https://*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

stage_from_archive() {
  local archive_ref="$1"
  local tmpdir="$2"
  local archive_name
  archive_name="$(basename "${archive_ref%%\?*}")"
  [[ -n "$archive_name" && "$archive_name" == *.tar.gz ]] || fail "release archive must be a .tar.gz file: $archive_ref"
  local archive_path="$tmpdir/$archive_name"
  local sha_path="$tmpdir/$archive_name.sha256"
  local sig_path="$tmpdir/$archive_name.sig"
  local sha_ref
  sha_ref="$(resolve_sha_ref "$archive_ref")"

  read_source_to_file "$archive_ref" "$archive_path"
  read_source_to_file "$sha_ref" "$sha_path"

  if is_remote_ref "$archive_ref"; then
    if read_source_to_file "$(resolve_sig_ref "$archive_ref")" "$sig_path"; then
      if [[ -f "$RELEASE_PUBLIC_KEY" ]]; then
        release_archive_tool verify-signature --archive "$archive_path" --signature "$sig_path" --public-key "$RELEASE_PUBLIC_KEY"
      else
        release_archive_tool verify-signature --archive "$archive_path" --signature "$sig_path"
      fi
    else
      printf 'rzn-phone install: release signature sidecar not found; falling back to sha256 verification only\n' >&2
    fi
  fi
  release_archive_tool verify-sha256 --archive "$archive_path" --sha256 "$sha_path"
  release_archive_tool safe-extract --archive "$archive_path" --dest "$tmpdir" --root-name rzn-phone
  if [[ -d "$tmpdir/rzn-phone" ]]; then
    printf '%s\n' "$tmpdir/rzn-phone"
    return 0
  fi
  fail "archive did not contain the expected rzn-phone root"
}

select_bin_dir() {
  if [[ -n "$BIN_DIR" ]]; then
    mkdir -p "$BIN_DIR"
    printf '%s\n' "$BIN_DIR"
    return 0
  fi

  local candidate
  for candidate in "$HOME/.local/bin" "$HOME/bin" "/opt/homebrew/bin" "/usr/local/bin"; do
    mkdir -p "$candidate" 2>/dev/null || true
    if [[ -d "$candidate" && -w "$candidate" ]]; then
      printf '%s\n' "$candidate"
      return 0
    fi
  done

  fail "could not find a writable bin directory; pass --bin-dir"
}

write_shim() {
  local shim_path="$1"
  local current_target="$2"
  cat >"$shim_path" <<EOF
#!/usr/bin/env bash
set -euo pipefail
exec "$current_target/bin/rzn-phone" "\$@"
EOF
  chmod +x "$shim_path"
}

install_stage() {
  local stage="$1"
  local version="$2"
  local bin_dir="$3"
  local resolved_update_source="$4"

  local releases_dir="$INSTALL_ROOT/releases"
  local dest="$releases_dir/$version"
  local temp_dest="$releases_dir/.${version}.tmp"
  mkdir -p "$releases_dir"
  rm -rf "$temp_dest"
  mkdir -p "$temp_dest"
  cp -R "$stage/." "$temp_dest/"

  if [[ -n "$resolved_update_source" ]]; then
    printf '%s\n' "$resolved_update_source" >"$temp_dest/UPDATE_SOURCE"
  fi

  rm -rf "$dest"
  mv "$temp_dest" "$dest"
  ln -sfn "$dest" "$INSTALL_ROOT/current"
  write_shim "$bin_dir/rzn-phone" "$INSTALL_ROOT/current"
}

shim_points_to_install_root() {
  local shim_path="$1"
  [[ -f "$shim_path" ]] || return 1
  grep -F "exec \"$INSTALL_ROOT/current/bin/rzn-phone\"" "$shim_path" >/dev/null 2>&1
}

uninstall_runtime() {
  local candidates=()
  local command_path bin_dir shim_path removed_shims skipped_shims
  removed_shims=""
  skipped_shims=""

  if [[ -n "$BIN_DIR" ]]; then
    candidates+=("$BIN_DIR")
  else
    candidates+=("$HOME/.local/bin" "$HOME/bin" "/opt/homebrew/bin" "/usr/local/bin")
    command_path="$(command -v rzn-phone 2>/dev/null || true)"
    if [[ -n "$command_path" ]]; then
      candidates+=("$(dirname "$command_path")")
    fi
  fi

  for bin_dir in "${candidates[@]}"; do
    [[ -n "$bin_dir" && -d "$bin_dir" ]] || continue
    shim_path="$bin_dir/rzn-phone"
    [[ -e "$shim_path" ]] || continue
    if shim_points_to_install_root "$shim_path"; then
      rm -f "$shim_path"
      removed_shims="${removed_shims}${removed_shims:+, }$shim_path"
    else
      skipped_shims="${skipped_shims}${skipped_shims:+, }$shim_path"
    fi
  done

  if [[ -d "$INSTALL_ROOT" ]]; then
    rm -rf "$INSTALL_ROOT"
  fi

  if [[ "$PURGE_STATE" == "1" && -d "$STATE_DIR" ]]; then
    rm -rf "$STATE_DIR"
  fi

  cat <<EOF
Uninstalled rzn-phone runtime from $INSTALL_ROOT
Removed shim(s): ${removed_shims:-none found}
EOF
  if [[ -n "$skipped_shims" ]]; then
    printf 'Skipped non-installer-managed rzn-phone shim(s): %s\n' "$skipped_shims"
  fi
  if [[ "$PURGE_STATE" == "1" ]]; then
    printf 'Removed local state: %s\n' "$STATE_DIR"
  else
    printf 'Kept local state: %s\n' "$STATE_DIR"
  fi
}

print_post_install() {
  local version="$1"
  local bin_dir="$2"
  local shim_path="$bin_dir/rzn-phone"
  local install_url="https://raw.githubusercontent.com/${RELEASE_REPO}/main/scripts/install_rzn_phone.sh"

  cat <<EOF
Installed rzn-phone ${version}
Runtime: $INSTALL_ROOT/current
Shim: $shim_path

Next steps:
1. Make sure the shim is on PATH.
EOF

  if [[ ":$PATH:" != *":$bin_dir:"* ]]; then
    cat <<EOF
   export PATH="$bin_dir:\$PATH"
   Then restart your shell or add that line to your shell profile.
EOF
  else
    cat <<'EOF'
   Already on PATH for this shell.
EOF
  fi

  cat <<EOF

2. Install the phone automation prerequisites.
   xcode-select --install
   node --version || brew install node
   npm i -g appium
   appium driver install xcuitest

3. Connect a physical iPhone.
   Unlock it, tap Trust on the device, keep it awake, and sign into the apps you plan to automate.

4. Verify the machine and device.
   rzn-phone doctor
   rzn-phone devices

5. Run a read-only smoke test.
   rzn-phone list --compact
   rzn-phone run safari/google_search --args-json '{"query":"rzn-phone","limit":3}'

Optional Appium setup:
   rzn-phone can start Appium when it is on PATH. For desktop/agent hosts, an explicit Appium
   endpoint is usually more predictable:

   appium
   export RZN_IOS_APPIUM_URL="http://127.0.0.1:4723"

MCP setup:
   Use this command in your MCP client config:
   command: "$shim_path"
   args: ["worker"]

Uninstall:
   curl -fsSL $install_url | bash -s -- --uninstall
   curl -fsSL $install_url | bash -s -- --uninstall --purge-state
EOF
}

while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --stage)
      STAGE="$(expand_path "${2:-}")"
      shift 2
      ;;
    --archive)
      ARCHIVE="${2:-}"
      shift 2
      ;;
    --source)
      SOURCE="${2:-}"
      shift 2
      ;;
    --version)
      VERSION="${2:-}"
      shift 2
      ;;
    --update-source)
      UPDATE_SOURCE="${2:-}"
      shift 2
      ;;
    --install-root)
      INSTALL_ROOT="$(expand_path "${2:-}")"
      shift 2
      ;;
    --bin-dir)
      BIN_DIR="$(expand_path "${2:-}")"
      shift 2
      ;;
    --state-dir)
      STATE_DIR="$(expand_path "${2:-}")"
      shift 2
      ;;
    --uninstall)
      UNINSTALL="1"
      shift
      ;;
    --purge-state)
      PURGE_STATE="1"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

INSTALL_ROOT="$(expand_path "$INSTALL_ROOT")"
STATE_DIR="$(expand_path "$STATE_DIR")"
if [[ -n "$BIN_DIR" ]]; then
  BIN_DIR="$(expand_path "$BIN_DIR")"
fi

if [[ "$PURGE_STATE" == "1" && "$UNINSTALL" != "1" ]]; then
  fail "--purge-state requires --uninstall"
fi

if [[ "$UNINSTALL" == "1" ]]; then
  uninstall_runtime
  exit 0
fi

if [[ -z "$STAGE" && -z "$ARCHIVE" && -z "$SOURCE" ]]; then
  configure_latest_release_source
fi

assert_supported_platform
VERSION="$(discover_version)"
validate_release_version "$VERSION"
BIN_DIR="$(select_bin_dir)"
UPDATE_SOURCE="${UPDATE_SOURCE:-$SOURCE}"

if [[ -n "$STAGE" ]]; then
  [[ -d "$STAGE" ]] || fail "stage directory not found: $STAGE"
  install_stage "$STAGE" "$VERSION" "$BIN_DIR" "$UPDATE_SOURCE"
else
  TMPDIR="$(mktemp -d /tmp/rzn-phone-install.XXXXXX)"
  trap 'rm -rf "$TMPDIR"' EXIT
  ARCHIVE_REF="$(resolve_archive_ref "$VERSION")"
  STAGE_DIR="$(stage_from_archive "$ARCHIVE_REF" "$TMPDIR")"
  if [[ -z "$UPDATE_SOURCE" ]]; then
    case "$ARCHIVE_REF" in
      http://*|https://*|file://*)
        UPDATE_SOURCE="${ARCHIVE_REF%/*}"
        ;;
      *)
        UPDATE_SOURCE="$(dirname "$(expand_path "$ARCHIVE_REF")")"
        ;;
    esac
  fi
  install_stage "$STAGE_DIR" "$VERSION" "$BIN_DIR" "$UPDATE_SOURCE"
fi

print_post_install "$VERSION" "$BIN_DIR"
