#!/usr/bin/env bash
set -euo pipefail

# lecoq -- the Rust toolchain pin preflight (#14, spec §9).
#
# Usage: ./tools/local-ci/check-toolchain-pin.sh [repo-root] [--rustc "<rustc --version output>"]
#
# The Rust version is pinned in rust-toolchain.toml (`channel`), the single source of truth: the
# gate image is built FROM rust:<channel> and installs exactly that toolchain. Two other places
# must agree with it, and this script fails (exit 1, naming the mismatch) when they do not:
#
#   - Cargo.toml's [workspace.package] rust-version, i.e. the same version without a patch level
#     (channel "1.95" or "1.95.3" both agree with rust-version "1.95");
#   - with --rustc, the compiler actually in use: "rustc 1.95.0 (...)" agrees with channel "1.95"
#     or "1.95.0", never "1.96" or "1.95.1".
#
# The channel must be an exact version (X.Y or X.Y.Z); "stable", "nightly" etc. float and are
# refused. On success it prints the channel, alone, on stdout. run.sh calls it first in every mode.

root="."
rustc_line=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --rustc)
      if [[ $# -lt 2 || -z "$2" ]]; then
        echo "ERROR: --rustc needs a value (the output of rustc --version)." >&2
        sed -n '/^# Usage:/p' "$0" | sed 's/^# \{0,1\}//' >&2
        exit 2
      fi
      rustc_line="$2"; shift ;;
    -h|--help) sed -n '/^# Usage:/p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) root="$1" ;;
  esac
  shift
done

toolchain_file="${root}/rust-toolchain.toml"
cargo_file="${root}/Cargo.toml"

# The first `key = "value"` line for key in a TOML file (enough for these two flat files).
toml_string() { # <file> <key>
  sed -n "s/^[[:space:]]*$2[[:space:]]*=[[:space:]]*\"\\([^\"]*\\)\".*/\\1/p" "$1" | head -n1
}

channel="$(toml_string "${toolchain_file}" channel)"
if [[ -z "${channel}" ]]; then
  echo "ERROR: ${toolchain_file} has no 'channel = \"...\"' line; the Rust pin lives there." >&2
  exit 1
fi
if [[ ! "${channel}" =~ ^[0-9]+\.[0-9]+(\.[0-9]+)?$ ]]; then
  echo "ERROR: ${toolchain_file}'s channel '${channel}' is not an exact version (X.Y or X.Y.Z)." >&2
  echo "       The gate image pins it; a floating channel would make the gate irreproducible." >&2
  exit 1
fi
minor="$(cut -d. -f1-2 <<<"${channel}")"

rust_version="$(toml_string "${cargo_file}" rust-version)"
if [[ "${rust_version}" != "${minor}" ]]; then
  echo "ERROR: ${cargo_file}'s rust-version is '${rust_version:-<not found>}', but ${toolchain_file}" >&2
  echo "       pins channel ${channel}. Bump both together (tools/local-ci/README.md)." >&2
  exit 1
fi

if [[ -n "${rustc_line}" ]]; then
  active="$(sed -n 's/^rustc \([0-9][0-9.]*\).*/\1/p' <<<"${rustc_line}")"
  if [[ "${active}" != "${channel}" && "${active}" != "${channel}".* ]]; then
    echo "ERROR: the compiler in use is '${rustc_line}', but ${toolchain_file} pins ${channel}." >&2
    echo "       In the gate this means the image is stale or wrong; in place, check rustup." >&2
    exit 1
  fi
fi

echo "${channel}"
