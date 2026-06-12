#!/usr/bin/env bash
set -euo pipefail

failures=0

section() {
  printf '\n[%s]\n' "$1"
}

pass() {
  printf '  [ok] %s\n' "$1"
}

warn() {
  printf '  [warn] %s\n' "$1"
}

fail() {
  printf '  [fail] %s\n' "$1"
  failures=$((failures + 1))
}

require_cmd() {
  local cmd="$1"
  local hint="$2"
  if command -v "$cmd" >/dev/null 2>&1; then
    pass "$cmd: $(command -v "$cmd")"
  else
    fail "$cmd missing. $hint"
  fi
}

section "Platform"
if [[ "$(uname -s)" == "Darwin" ]]; then
  pass "macOS detected"
else
  fail "This tester kit currently supports macOS only."
fi

section "Toolchain"
require_cmd xcodebuild "Install Xcode from the App Store and open it once."
require_cmd xcrun "Install Xcode command line tools with: xcode-select --install"
require_cmd python3 "Install Python 3."
require_cmd node "Install Node.js 20+."
require_cmd npm "Install npm with Node.js."

if command -v rustc >/dev/null 2>&1; then
  pass "rustc: $(rustc --version)"
else
  warn "rustc not found. Fine for tester-kit use; only needed if rebuilding from source."
fi

section "Appium"
if command -v appium >/dev/null 2>&1; then
  pass "appium: $(command -v appium)"
else
  fail "appium missing. Install with: npm i -g appium"
fi

if command -v appium >/dev/null 2>&1; then
  if appium driver list --installed 2>/dev/null | grep -qi xcuitest; then
    pass "xcuitest driver installed"
  else
    fail "Appium xcuitest driver missing. Install with: appium driver install xcuitest"
  fi
fi

section "iPhone detection"
if command -v xcrun >/dev/null 2>&1; then
  device_output="$(xcrun xctrace list devices 2>/dev/null || true)"
  if [[ -n "$device_output" ]]; then
    physical_devices="$(printf '%s\n' "$device_output" | grep -E "iPhone|iPad" | grep -v Simulator || true)"
    if [[ -n "$physical_devices" ]]; then
      pass "physical Apple device(s) detected"
      printf '%s\n' "$physical_devices" | sed 's/^/    /'
    else
      warn "No trusted physical iPhone/iPad detected. Connect the device, unlock it, and tap Trust."
    fi
  else
    warn "Unable to query xctrace devices. Open Xcode once and accept any license prompts."
  fi
fi

section "Signing hints"
if [[ -n "${IOS_XCODE_ORG_ID:-}" ]]; then
  pass "IOS_XCODE_ORG_ID is set"
else
  warn "IOS_XCODE_ORG_ID is not set. You may need it if WDA provisioning fails."
fi

if [[ -n "${IOS_XCODE_SIGNING_ID:-}" ]]; then
  pass "IOS_XCODE_SIGNING_ID is set"
else
  warn "IOS_XCODE_SIGNING_ID is not set. Usually 'Apple Development' when manual signing is needed."
fi

if [[ -n "${IOS_UPDATED_WDA_BUNDLE_ID:-}" ]]; then
  pass "IOS_UPDATED_WDA_BUNDLE_ID is set"
else
  warn "IOS_UPDATED_WDA_BUNDLE_ID is not set. Set it if your team requires a unique WebDriverAgent bundle id."
fi

section "Summary"
if [[ "$failures" -eq 0 ]]; then
  printf 'Environment looks ready for tester-kit setup.\n'
else
  printf 'Found %d blocking issue(s). Fix those first, then rerun this script.\n' "$failures"
  exit 1
fi
