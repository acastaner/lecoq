#!/usr/bin/env bash
set -euo pipefail

# lecoq -- self-test of the gate's own guard scripts (#14).
#
# Usage: ./tools/local-ci/test_local_ci.sh
#
# Exercises tools/local-ci/check-toolchain-pin.sh and tools/local-ci/check-gate-record.sh through
# their command-line interface only (exit code and message), against throwaway repositories in a
# temporary directory. The `test` suite of run.sh runs it, so a regression in the guards fails the
# gate like any other code. Needs bash and git, nothing else.

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PIN="${HERE}/check-toolchain-pin.sh"
RECORD="${HERE}/check-gate-record.sh"

WORK="$(mktemp -d)"
trap 'rm -rf "${WORK}"' EXIT

PASSED=0
FAILED=0

pass() { PASSED=$((PASSED + 1)); echo "  ok    $1"; }
fail() { FAILED=$((FAILED + 1)); echo "  FAIL  $1"; [[ -n "${2:-}" ]] && printf '%s\n' "$2" | sed 's/^/        /'; return 0; }

# expect <name> <want-exit> <want-substring-of-output> -- <command...>
expect() {
  local name="$1" want_rc="$2" want_text="$3"
  shift 4
  local out rc=0
  out="$("$@" 2>&1)" || rc=$?
  if [[ "${rc}" != "${want_rc}" ]]; then
    fail "${name}" "exit ${rc}, wanted ${want_rc}; output:"$'\n'"${out}"
  elif [[ -n "${want_text}" && "${out}" != *"${want_text}"* ]]; then
    fail "${name}" "output lacks '${want_text}':"$'\n'"${out}"
  else
    pass "${name}"
  fi
}

# --- check-toolchain-pin.sh --------------------------------------------------------------------

# A repository root with the two files the pin lives in.
make_pin_repo() { # <dir> <toolchain channel line> <rust-version line>
  mkdir -p "$1"
  printf '[toolchain]\n%s\ncomponents = ["rustfmt", "clippy"]\nprofile = "minimal"\n' "$2" \
    > "$1/rust-toolchain.toml"
  printf '[workspace]\nmembers = ["crates/*"]\n\n[workspace.package]\nedition = "2024"\n%s\n' "$3" \
    > "$1/Cargo.toml"
}

echo "check-toolchain-pin.sh"

make_pin_repo "${WORK}/pin-ok" 'channel = "1.95"' 'rust-version = "1.95"'
expect "agreeing pins print the channel" 0 "1.95" -- "${PIN}" "${WORK}/pin-ok"
out="$("${PIN}" "${WORK}/pin-ok")"
if [[ "${out}" == "1.95" ]]; then pass "stdout is exactly the channel"; else fail "stdout is exactly the channel" "${out}"; fi

make_pin_repo "${WORK}/pin-patch" 'channel = "1.95.0"' 'rust-version = "1.95"'
expect "a patch-level channel agrees with a minor-level rust-version" 0 "1.95.0" -- "${PIN}" "${WORK}/pin-patch"

make_pin_repo "${WORK}/pin-mismatch" 'channel = "1.96"' 'rust-version = "1.95"'
expect "channel and rust-version disagree" 1 "rust-version" -- "${PIN}" "${WORK}/pin-mismatch"

make_pin_repo "${WORK}/pin-stable" 'channel = "stable"' 'rust-version = "1.95"'
expect "a floating channel is refused" 1 "not an exact version" -- "${PIN}" "${WORK}/pin-stable"

make_pin_repo "${WORK}/pin-nochannel" '' 'rust-version = "1.95"'
expect "a missing channel is refused" 1 "channel" -- "${PIN}" "${WORK}/pin-nochannel"

make_pin_repo "${WORK}/pin-norv" 'channel = "1.95"' ''
expect "a missing rust-version is refused" 1 "rust-version" -- "${PIN}" "${WORK}/pin-norv"

expect "the active rustc matches" 0 "1.95" -- \
  "${PIN}" "${WORK}/pin-ok" --rustc "rustc 1.95.0 (59807616e 2026-04-14)"
expect "the active rustc is another version" 1 "rustc 1.94.1" -- \
  "${PIN}" "${WORK}/pin-ok" --rustc "rustc 1.94.1 (e408947bf 2026-03-25)"
expect "channel 1.95 does not match rustc 1.951" 1 "" -- \
  "${PIN}" "${WORK}/pin-ok" --rustc "rustc 1.951.0 (0000000 2099-01-01)"
expect "a patch-level channel needs that exact patch" 1 "" -- \
  "${PIN}" "${WORK}/pin-patch" --rustc "rustc 1.95.1 (0000000 2026-05-01)"

expect "--rustc without a value is a usage error" 2 "Usage" -- "${PIN}" "${WORK}/pin-ok" --rustc

# --- check-gate-record.sh ----------------------------------------------------------------------

echo "check-gate-record.sh"

repo="${WORK}/record"
g() { git -C "${repo}" -c user.name=t -c user.email=t@t "$@"; }
git init --quiet "${repo}"
g commit --quiet --allow-empty -m one
g commit --quiet --allow-empty -m two
echo a > "${repo}/a"
g add a
g commit --quiet -m three
in_repo() { (cd "${repo}" && "$@"); }

expect "no argument prints usage" 2 "Usage" -- in_repo "${RECORD}"
expect "an ungated tree fails" 1 "no gate record" -- in_repo "${RECORD}" HEAD
expect "a bad commit-ish fails" 1 "does not resolve" -- in_repo "${RECORD}" no-such-ref

g notes --ref=gate add -m "PASS lint test · rust 1.95 · now" "HEAD^{tree}" 2>/dev/null
expect "a full-pass note on the tree passes" 0 "PASS lint test" -- in_repo "${RECORD}" HEAD
expect "the previous commit's tree is still ungated" 1 "no gate record" -- in_repo "${RECORD}" HEAD~1

# A merge-like commit with the same tree carries the record.
same_tree="$(g commit-tree -p HEAD -m merge "HEAD^{tree}")"
expect "another commit with the same tree passes" 0 "PASS lint test" -- in_repo "${RECORD}" "${same_tree}"

g notes --ref=gate add -f -m "PASS lint · rust 1.95 · now" "HEAD^{tree}" 2>/dev/null
expect "a subset note is not a full pass" 1 "does not record a full pass" -- in_repo "${RECORD}" HEAD

g notes --ref=gate remove "HEAD^{tree}" 2>/dev/null
g notes --ref=gate add -f -m "PASS lint test · rust 1.95 · now" "HEAD" 2>/dev/null
expect "a full-pass note on the commit instead of the tree does not count" 1 "no gate record" -- \
  in_repo "${RECORD}" HEAD

echo
echo "${PASSED} passed, ${FAILED} failed"
(( FAILED == 0 ))
