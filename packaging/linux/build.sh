#!/usr/bin/env bash
set -euo pipefail

# lecoq -- build the Linux release artifacts (#41, spec §8.5) from an already-built binary.
#
# Usage: packaging/linux/build.sh <out-dir> <artifact>...
#
#   tarball            <out>/lecoq-<ver>-<arch>-linux.tar.gz (bin/ and share/, a /usr-style tree)
#   deb                <out>/lecoq_<ver>-1_<debarch>.deb      (cargo-deb --no-build)
#   rpm                <out>/lecoq-<ver>-1.<arch>.rpm          (cargo generate-rpm)
#   pkgbuild <archive> <out>/aur/PKGBUILD and .SRCINFO, for the source <archive> (GitHub's tag
#                      tarball, lecoq-<ver>/...); it is only hashed here, never unpacked
#   flatpak            <out>/flatpak/: the manifest and the tarball it packages (needs `tarball`
#                      first), for flatpak-builder
#
# Run from anywhere; it works in the repository root. It never builds lecoq: run
# `cargo build --release --locked -p lecoq --features avif` first. The version is the workspace
# version in Cargo.toml. Nothing here registers lecoq as a *default* handler (spec §8.5).

root="$(cd "$(dirname "$0")/../.." && pwd)"
here="${root}/packaging/linux"
cd "${root}"

usage() { sed -n '/^# Usage:/,/^# Run from/p' "$0" | sed 's/^# \{0,1\}//' | sed '$d' >&2; exit 2; }
[[ $# -ge 2 ]] || usage

out="$1"; shift
mkdir -p "${out}"
out="$(cd "${out}" && pwd)"

version="$(sed -n 's/^version[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' Cargo.toml | head -n1)"
[[ -n "${version}" ]] || { echo "ERROR: no version in Cargo.toml's [workspace.package]." >&2; exit 1; }
arch="$(uname -m)"
binary="${root}/target/release/lecoq"

need_binary() {
  [[ -x "${binary}" ]] || {
    echo "ERROR: ${binary} is missing: run cargo build --release --locked -p lecoq --features avif first." >&2
    exit 1
  }
}

build_tarball() {
  need_binary
  local name="lecoq-${version}-${arch}-linux" stage
  stage="$(mktemp -d)"
  local t="${stage}/${name}"
  install -Dm755 "${binary}" "${t}/bin/lecoq"
  install -Dm644 "${here}/lecoq.desktop" "${t}/share/applications/lecoq.desktop"
  install -Dm644 "${here}/lecoq.svg" "${t}/share/icons/hicolor/scalable/apps/lecoq.svg"
  install -Dm644 "${here}/io.github.acastaner.lecoq.metainfo.xml" \
    "${t}/share/metainfo/io.github.acastaner.lecoq.metainfo.xml"
  install -Dm644 LICENSE "${t}/LICENSE"
  # Reproducible-ish: fixed owner, sorted names, mtimes clamped to the commit time when known.
  local mtime
  mtime="$(git log -1 --format=%ct 2>/dev/null || date +%s)"
  tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@${mtime}" \
    -C "${stage}" -czf "${out}/${name}.tar.gz" "${name}"
  rm -rf "${stage}"
  echo "${out}/${name}.tar.gz"
}

build_deb() {
  need_binary
  cargo deb -p lecoq --no-build --locked --output "${out}" >/dev/null
  ls "${out}"/lecoq_"${version}"*.deb
}

build_rpm() {
  need_binary
  cargo generate-rpm -p crates/lecoq --output "${out}" >/dev/null
  ls "${out}"/lecoq-"${version}"*.rpm
}

build_pkgbuild() {
  local archive="$1" sha
  [[ -f "${archive}" ]] || { echo "ERROR: no source archive at ${archive}." >&2; exit 1; }
  sha="$(sha256sum "${archive}" | cut -d' ' -f1)"
  mkdir -p "${out}/aur"
  local f
  for f in PKGBUILD .SRCINFO; do
    sed -e "s/@VERSION@/${version}/g" -e "s/@SHA256@/${sha}/g" "${root}/packaging/aur/${f}.in" \
      > "${out}/aur/${f}"
  done
  echo "${out}/aur/PKGBUILD"
  echo "${out}/aur/.SRCINFO"
}

build_flatpak() {
  local tarball="${out}/lecoq-${version}-${arch}-linux.tar.gz"
  [[ -f "${tarball}" ]] || { echo "ERROR: build the tarball first (${tarball})." >&2; exit 1; }
  mkdir -p "${out}/flatpak"
  cp "${root}/packaging/flatpak/io.github.acastaner.lecoq.yml" "${out}/flatpak/"
  cp "${tarball}" "${out}/flatpak/lecoq-linux.tar.gz"
  echo "${out}/flatpak/io.github.acastaner.lecoq.yml"
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    tarball) build_tarball ;;
    deb) build_deb ;;
    rpm) build_rpm ;;
    pkgbuild)
      [[ $# -ge 2 ]] || usage
      build_pkgbuild "$2"; shift ;;
    flatpak) build_flatpak ;;
    *) echo "ERROR: unknown artifact '$1'." >&2; usage ;;
  esac
  shift
done
