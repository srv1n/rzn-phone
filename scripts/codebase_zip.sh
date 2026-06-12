#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(git -C "$SCRIPT_DIR/.." rev-parse --show-toplevel)"
REPO_NAME="$(basename "$REPO_ROOT")"
ARTIFACTS_DIR="${CODEBASE_ZIP_DIR:-$REPO_ROOT/artifacts}"
TIMESTAMP="$(date +%Y%m%d-%H%M%S)"
ZIP_NAME="${CODEBASE_ZIP_NAME:-$REPO_NAME-codebase-$TIMESTAMP.zip}"

if [[ "$ZIP_NAME" != *.zip ]]; then
  ZIP_NAME="$ZIP_NAME.zip"
fi

ZIP_PATH="$ARTIFACTS_DIR/$ZIP_NAME"

require_command() {
  local command_name="$1"

  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "error: required command not found: $command_name" >&2
    exit 1
  fi
}

should_skip() {
  local path="$1"

  case "$path" in
    .beads/*|.git/*|.playwright-cli/*|.secrets/*|.tmp/*|.cache/*|*/.cache/*)
      return 0
      ;;
    artifacts/*|dist/*|target/*|output/*|libexec/*|node_modules/*|vendor/*)
      return 0
      ;;
    */artifacts/*|*/dist/*|*/target/*|*/output/*|*/node_modules/*|*/vendor/*)
      return 0
      ;;
    __pycache__/*|*/__pycache__/*|.DS_Store|*/.DS_Store)
      return 0
      ;;
    *.pyc|*.pyo|*.log|*.tmp|*.zip|*.tar|*.tar.gz|*.tgz|*.dmg|*.pkg)
      return 0
      ;;
    *.app|*.ipa|*.xcarchive|*.dSYM|*.dSYM/*|*.a|*.rlib)
      return 0
      ;;
    .env|.env.*|*/.env|*/.env.*|*.pem|*.key|*.p12|*.pfx|*.mobileprovision)
      case "$path" in
        .env.example|*/.env.example|.env.sample|*/.env.sample)
          return 1
          ;;
      esac
      return 0
      ;;
  esac

  return 1
}

require_command git
require_command rsync
require_command zip

mkdir -p "$ARTIFACTS_DIR"

MANIFEST="$(mktemp)"
STAGING_DIR="$(mktemp -d)"
trap 'rm -f "$MANIFEST"; rm -rf "$STAGING_DIR"' EXIT

cd "$REPO_ROOT"

git ls-files -z --cached --others --exclude-standard --deduplicate |
  while IFS= read -r -d '' path; do
    [[ -e "$path" || -L "$path" ]] || continue
    should_skip "$path" && continue
    printf '%s\n' "$path"
  done >"$MANIFEST"

FILE_COUNT="$(wc -l <"$MANIFEST" | tr -d '[:space:]')"

if [[ "$FILE_COUNT" == "0" ]]; then
  echo "error: no files matched the codebase archive filters" >&2
  exit 1
fi

rm -f "$ZIP_PATH"
mkdir -p "$STAGING_DIR/$REPO_NAME"
rsync -a --files-from="$MANIFEST" "$REPO_ROOT/" "$STAGING_DIR/$REPO_NAME/"

(
  cd "$STAGING_DIR"
  zip -qry "$ZIP_PATH" "$REPO_NAME"
)

echo "Created $ZIP_PATH"
echo "Included $FILE_COUNT files"

if [[ "${CODEBASE_ZIP_OPEN:-1}" != "0" ]]; then
  if command -v open >/dev/null 2>&1; then
    open "$ARTIFACTS_DIR"
  else
    echo "Artifacts folder: $ARTIFACTS_DIR"
  fi
fi
