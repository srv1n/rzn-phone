#!/usr/bin/env bash
set -euo pipefail

INSTALL_ROOT="${RZN_PHONE_INSTALL_ROOT:-$HOME/.local/share/rzn-phone}"
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

    if read_source_text "$version_ref" >/tmp/rzn-phone-version.$$ 2>/dev/null; then
      tr -d '\n' </tmp/rzn-phone-version.$$
      rm -f /tmp/rzn-phone-version.$$
      return 0
    fi
    rm -f /tmp/rzn-phone-version.$$
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

stage_from_archive() {
  local archive_ref="$1"
  local tmpdir="$2"
  local archive_path="$tmpdir/archive.tar.gz"

  case "$archive_ref" in
    http://*|https://*)
      curl -fsSL "$archive_ref" -o "$archive_path"
      ;;
    file://*)
      cp "${archive_ref#file://}" "$archive_path"
      ;;
    *)
      cp "$(expand_path "$archive_ref")" "$archive_path"
      ;;
  esac

  tar -xzf "$archive_path" -C "$tmpdir"
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
