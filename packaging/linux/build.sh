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
#   appimage           <out>/lecoq-<ver>-<arch>.AppImage (#52): the tarball's tree under usr/,
#                      packed by the pinned appimagetool; bundles no libraries, like the tarball.
#                      Its root desktop file is io.github.acastaner.lecoq.desktop (see below)
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

# appimagetool and the AppImage runtime it embeds, pinned by version and SHA-256 (x86_64 only).
appimagetool_version="1.9.1"
appimagetool_sha256="ed4ce84f0d9caff66f50bcca6ff6f35aae54ce8135408b3fa33abfc3cb384eb0"
runtime_version="20251108"
runtime_sha256="2fca8b443c92510f1483a883f60061ad09b46b978b2631c807cd873a47ec260d"

# Staging directories, removed on exit, also when a step fails.
stages=()
trap 'rm -rf "${stages[@]}"' EXIT
# Sets the caller's `stage` (not in a $(...) subshell, which would lose the stages+=).
new_stage() { stage="$(mktemp -d)"; stages+=("${stage}"); }

need_binary() {
  [[ -x "${binary}" ]] || {
    echo "ERROR: ${binary} is missing: run cargo build --release --locked -p lecoq --features avif first." >&2
    exit 1
  }
}

# The /usr-style tree shared by the tarball (at its root) and the AppImage (under usr/).
install_tree() {
  local t="$1"
  install -Dm755 "${binary}" "${t}/bin/lecoq"
  install -Dm644 "${here}/lecoq.desktop" "${t}/share/applications/lecoq.desktop"
  install -Dm644 "${here}/lecoq.svg" "${t}/share/icons/hicolor/scalable/apps/lecoq.svg"
  install -Dm644 "${here}/io.github.acastaner.lecoq.metainfo.xml" \
    "${t}/share/metainfo/io.github.acastaner.lecoq.metainfo.xml"
}

build_tarball() {
  need_binary
  local name="lecoq-${version}-${arch}-linux" stage
  new_stage
  local t="${stage}/${name}"
  install_tree "${t}"
  install -Dm644 LICENSE "${t}/LICENSE"
  # Reproducible-ish: fixed owner, sorted names, mtimes clamped to the commit time when known.
  local mtime
  mtime="$(git log -1 --format=%ct 2>/dev/null || date +%s)"
  tar --sort=name --owner=0 --group=0 --numeric-owner --mtime="@${mtime}" \
    -C "${stage}" -czf "${out}/${name}.tar.gz" "${name}"
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

# Download $1 to $3 unless it is already there, and check it against the SHA-256 $2.
fetch_pinned() {
  local url="$1" sha="$2" dest="$3"
  if [[ ! -f "${dest}" ]] || ! echo "${sha}  ${dest}" | sha256sum -c --status; then
    mkdir -p "$(dirname "${dest}")"
    curl -fsSL -o "${dest}.part" "${url}"
    echo "${sha}  ${dest}.part" | sha256sum -c --status || {
      echo "ERROR: ${url} does not match its pinned SHA-256." >&2
      rm -f "${dest}.part"
      exit 1
    }
    mv "${dest}.part" "${dest}"
  fi
}

build_appimage() {
  need_binary
  [[ "${arch}" == "x86_64" ]] || { echo "ERROR: the AppImage is built for x86_64 only." >&2; exit 1; }
  local cache="${XDG_CACHE_HOME:-${HOME}/.cache}/lecoq-appimage" stage
  local tool="${cache}/appimagetool-${appimagetool_version}"
  local runtime="${cache}/runtime-${runtime_version}"
  fetch_pinned "https://github.com/AppImage/appimagetool/releases/download/${appimagetool_version}/appimagetool-x86_64.AppImage" \
    "${appimagetool_sha256}" "${tool}"
  fetch_pinned "https://github.com/AppImage/type2-runtime/releases/download/${runtime_version}/runtime-x86_64" \
    "${runtime_sha256}" "${runtime}"
  chmod +x "${tool}"
  new_stage
  local appdir="${stage}/lecoq.AppDir"
  install_tree "${appdir}/usr"
  install -Dm644 LICENSE "${appdir}/usr/share/licenses/lecoq/LICENSE"
  # appimagetool validates the metainfo (with appstreamcli, when installed) only if it is named
  # after the root desktop file, as .appdata.xml; appstreamcli wants it named after the app ID.
  # Naming the root desktop file after the app ID satisfies both.
  cp "${here}/lecoq.desktop" "${appdir}/io.github.acastaner.lecoq.desktop"
  ln -s io.github.acastaner.lecoq.metainfo.xml \
    "${appdir}/usr/share/metainfo/io.github.acastaner.lecoq.appdata.xml"
  cp "${here}/lecoq.svg" "${appdir}/"
  ln -s lecoq.svg "${appdir}/.DirIcon"
  ln -s usr/bin/lecoq "${appdir}/AppRun"
  local image="${out}/lecoq-${version}-${arch}.AppImage"
  # Extract-and-run: the build container has no FUSE.
  APPIMAGE_EXTRACT_AND_RUN=1 ARCH="${arch}" "${tool}" --runtime-file "${runtime}" "${appdir}" "${image}" >&2
  echo "${image}"
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
    appimage) build_appimage ;;
    *) echo "ERROR: unknown artifact '$1'." >&2; usage ;;
  esac
  shift
done
