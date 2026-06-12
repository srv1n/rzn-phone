#!/usr/bin/env bash
set -euo pipefail

INSTALL_ROOT="${RZN_PHONE_INSTALL_ROOT:-$HOME/.local/share/rzn-phone}"
SCRIPT_PATH="${BASH_SOURCE[0]:-$0}"
SCRIPT_DIR="$(cd "$(dirname "$SCRIPT_PATH")" 2>/dev/null && pwd || pwd)"
RELEASE_ARCHIVE_HELPER="$SCRIPT_DIR/release_archive.py"
BIN_DIR="${RZN_PHONE_BIN_DIR:-}"
PLATFORM="macos_universal"
VERSION=""
SOURCE=""
ARCHIVE=""
STAGE=""
UPDATE_SOURCE=""

usage() {
  cat <<'EOF'
Usage: scripts/install_rzn_phone.sh [options]

Install rzn-phone into a versioned local runtime and expose a global `rzn-phone` shim.

Options:
  --stage <dir>            Install from an unpacked release directory.
  --archive <path|url>     Install from a release tarball.
  --source <path|url>      Release directory base used to resolve VERSION + tarball names.
  --version <version>      Release version to install. Optional when VERSION can be discovered.
  --update-source <value>  Persist workflow update source for `rzn-phone workflows update`.
  --install-root <dir>     Override install root (default: ~/.local/share/rzn-phone).
  --bin-dir <dir>          Override shim directory.
  -h, --help               Show this help.
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

validate_release_version() {
  local version="$1"
  if [[ ! "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?(\+[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?$ ]]; then
    fail "invalid release version: $version"
  fi
}

release_archive_tool() {
  if [[ -f "$RELEASE_ARCHIVE_HELPER" ]]; then
    python3 "$RELEASE_ARCHIVE_HELPER" "$@"
    return $?
  fi

  python3 - "$@" <<'PY'
import argparse
import hashlib
import posixpath
import re
import sys
import tarfile
import warnings
from pathlib import Path

SHA256_RE = re.compile(r"^[0-9a-fA-F]{64}$")

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
extract = subparsers.add_parser("safe-extract")
extract.add_argument("--archive", required=True, type=Path)
extract.add_argument("--dest", required=True, type=Path)
extract.add_argument("--root-name", required=True)
args = parser.parse_args()
try:
    if args.command == "verify-sha256":
        verify_sha256(args.archive, args.sha256)
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

stage_from_archive() {
  local archive_ref="$1"
  local tmpdir="$2"
  local archive_name
  archive_name="$(basename "${archive_ref%%\?*}")"
  [[ -n "$archive_name" && "$archive_name" == *.tar.gz ]] || fail "release archive must be a .tar.gz file: $archive_ref"
  local archive_path="$tmpdir/$archive_name"
  local sha_path="$tmpdir/$archive_name.sha256"
  local sha_ref
  sha_ref="$(resolve_sha_ref "$archive_ref")"

  read_source_to_file "$archive_ref" "$archive_path"
  read_source_to_file "$sha_ref" "$sha_path"

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
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

if [[ -z "$STAGE" && -z "$ARCHIVE" && -z "$SOURCE" ]]; then
  usage >&2
  exit 1
fi

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

if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
  cat <<EOF
Installed rzn-phone ${VERSION} to $INSTALL_ROOT/current
Shim: $BIN_DIR/rzn-phone
Note: $BIN_DIR is not on PATH in this shell. Add it, then restart your shell.
EOF
else
  cat <<EOF
Installed rzn-phone ${VERSION} to $INSTALL_ROOT/current
Shim: $BIN_DIR/rzn-phone
Run: rzn-phone version
EOF
fi
