#!/usr/bin/env bash
set -euo pipefail

# lecoq -- build the Windows release artifacts (#42, spec §8.5) from an already-built binary.
#
# Usage: packaging/windows/build.sh <out-dir> <artifact>...
#
#   msi       <out>/lecoq-<ver>-x86_64.msi (cargo wix --no-build, packaging/windows/lecoq.wxs)
#   winget    <out>/winget/acastaner.lecoq*.yaml: the winget manifest for that MSI (needs `msi`
#             first); submit it to microsoft/winget-pkgs by hand
#   portable  <out>/lecoq-<ver>-x86_64-windows-portable.zip: lecoq.exe, LICENSE.txt and
#             portable.txt, which keeps the settings next to the exe (#52); registers nothing
#
# Runs in Git Bash on Windows (the release workflow's `windows` job), with the WiX Toolset v3 and
# cargo-wix installed. It never builds lecoq: run
# `cargo build --release --locked -p lecoq --features avif` first. The version is the workspace
# version in Cargo.toml. The MSI adds lecoq to *Open with* and never makes it the default handler
# (spec §8.5; see the comment in lecoq.wxs).

root="$(cd "$(dirname "$0")/../.." && pwd)"
here="${root}/packaging/windows"
cd "${root}"

usage() { sed -n '/^# Usage:/,/^# Runs in/p' "$0" | sed 's/^# \{0,1\}//' | sed '$d' >&2; exit 2; }
[[ $# -ge 2 ]] || usage

out="$1"; shift
mkdir -p "${out}"
out="$(cd "${out}" && pwd)"

version="$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' Cargo.toml | head -n1)"
[[ -n "${version}" ]] || { echo "ERROR: no version in Cargo.toml's [workspace.package]." >&2; exit 1; }
binary="${root}/target/release/lecoq.exe"
msi="${out}/lecoq-${version}-x86_64.msi"
portable="${out}/lecoq-${version}-x86_64-windows-portable.zip"

# WiX and cargo-wix want Windows paths; elsewhere (a dry run of `winget`) keep them as they are.
winpath() { if command -v cygpath > /dev/null; then cygpath -w "$1"; else printf '%s' "$1"; fi; }

need_binary() {
  [[ -f "${binary}" ]] || {
    echo "ERROR: target/release/lecoq.exe is missing: run cargo build --release --locked -p lecoq --features avif first." >&2
    exit 1
  }
}

build_msi() {
  need_binary
  cargo wix --package lecoq --no-build --nocapture \
    --include "$(winpath "${here}/lecoq.wxs")" \
    --compiler-arg "-dRepoDir=$(winpath "${root}")" \
    --output "$(winpath "${msi}")"
  echo "${msi}"
}

build_winget() {
  [[ -f "${msi}" ]] || { echo "ERROR: ${msi} is missing: build \`msi\` first." >&2; exit 1; }
  local sha f
  sha="$(sha256sum "${msi}" | cut -d' ' -f1 | tr '[:lower:]' '[:upper:]')"
  mkdir -p "${out}/winget"
  for f in "${here}"/winget/*.yaml; do
    # Drop the template's own comments; keep the schema line.
    sed -e '/^# yaml-language-server:/!{/^#/d;}' \
      -e "s/@VERSION@/${version}/g" -e "s/@SHA256@/${sha}/g" \
      "${f}" > "${out}/winget/$(basename "${f}")"
  done
  if grep -l '@[A-Z0-9]*@' "${out}"/winget/*.yaml; then
    echo "ERROR: unfilled placeholders in the winget manifest (above)." >&2
    exit 1
  fi
  echo "${out}/winget"
}

build_portable() {
  need_binary
  # Python's zipfile, since Git Bash has no zip. Fixed timestamps, so the same inputs give the
  # same zip.
  python - "${portable}" "${binary}" "${root}/LICENSE" "${here}/portable.txt" <<'PY'
import sys, zipfile
out, exe, licence, marker = sys.argv[1:]
entries = [("lecoq.exe", exe), ("LICENSE.txt", licence), ("portable.txt", marker)]
with zipfile.ZipFile(out, "w") as z:
    for name, src in entries:
        info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
        info.compress_type = zipfile.ZIP_DEFLATED
        with open(src, "rb") as f:
            z.writestr(info, f.read())
PY
  echo "${portable}"
}

for artifact in "$@"; do
  case "${artifact}" in
    msi) build_msi ;;
    winget) build_winget ;;
    portable) build_portable ;;
    *) echo "ERROR: unknown artifact '${artifact}'." >&2; usage ;;
  esac
done
