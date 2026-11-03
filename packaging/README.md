# `packaging/`: release artifacts and file associations

Spec §8.5 and §9. A pushed tag `vX.Y.Z` runs `.github/workflows/release.yml`, which builds these
and publishes a GitHub release. GitHub runs no tests: its first job, `gate-record`, refuses a tree
that the local gate (`tools/local-ci/`) never passed.

| Path | What it is |
|---|---|
| `linux/lecoq.desktop` | The desktop entry. `MimeType=` lists every format lecoq reads (§5.1) |
| `linux/lecoq.svg` | The icon (hicolor, scalable) |
| `linux/io.github.acastaner.lecoq.metainfo.xml` | AppStream metadata. Needs a `<release>` for each version |
| `linux/build.sh` | Builds the tarball, `.deb`, `.rpm`, AUR files, Flatpak staging and AppImage from a built binary |
| `aur/PKGBUILD.in`, `aur/.SRCINFO.in` | AUR templates (a source build of the tag); `build.sh pkgbuild` fills in version and hash |
| `flatpak/io.github.acastaner.lecoq.yml` | Flatpak manifest; repackages the tarball's binary |
| `windows/lecoq.wxs` | WiX v3 source for the MSI (cargo-wix): ProgIDs and *Open with* entries per extension |
| `windows/winget/*.yaml` | winget manifest templates; the release fills in the version and the MSI's hash |
| `windows/build.sh` | Builds the MSI, the winget manifest and the portable zip from a built binary (Git Bash on Windows) |
| `windows/portable.txt` | The portable zip's marker: next to `lecoq.exe`, it keeps the settings in `config\` beside it |
| `windows/lecoq.ico`, `macos/lecoq.icns` | Icons rendered from `linux/lecoq.svg` by `make_icons.py` (committed) |
| `macos/Info.plist` | The app's Info.plist: `CFBundleDocumentTypes` for every readable format, rank Alternate |
| `macos/build.sh` | Builds the ad-hoc-signed `lecoq.app` and its DMG from a built binary (macOS) |
| `check_associations.py` | Checks that all of the above list exactly `READ_EXTENSIONS` and never claim the default |
| `release_notes.py` | Release notes: `release-notes/vX.Y.Z.md` plus the `tools/bench` numbers |
| `release-notes/` | One `vX.Y.Z.md` and its `vX.Y.Z.bench-*.json` per release |

The `.deb` and `.rpm` metadata (dependencies, assets) is in `crates/lecoq/Cargo.toml`.

## Default handler: never

lecoq registers as **a** handler for its formats and never makes itself the default (§8.5). No
package ships a `mimeapps.list`, runs `xdg-mime default`, or has a maintainer script that does.
Debian and RPM triggers update the desktop database, which only lists lecoq under *Open with*.
The MSI writes ProgIDs, `OpenWithProgids`, `Applications\lecoq.exe\SupportedTypes` and Default
Programs capabilities, never an extension key's default value (so no WiX `<ProgId>`/`<Extension>`).
The macOS document types are `LSHandlerRank` `Alternate`. `check_associations.py`, run by the
release's `gate-record` job, enforces both, and that every package lists the same formats.

## Portable builds

Two artifacts run from any folder and register nothing (§8.3, §8.5):

- **Windows:** `lecoq-X.Y.Z-x86_64-windows-portable.zip`. It holds `lecoq.exe`, `LICENSE.txt` and
  `portable.txt`. While `portable.txt` sits next to the exe, `config.toml` lives in a `config\`
  folder beside it. `.cargo/config.toml` links the C runtime statically, so the exe needs no
  Visual C++ Redistributable. The release's `windows` job checks this with `dumpbin`.
- **Linux:** `lecoq-X.Y.Z-x86_64.AppImage`, packed by `build.sh appimage` with `appimagetool` and
  its runtime (both pinned by SHA-256 in `build.sh`). Like the tarball it bundles no libraries and
  needs glibc 2.36 or later. For portable settings, run it once with `--appimage-portable-config`,
  or create `lecoq-X.Y.Z-x86_64.AppImage.config/` next to it: the runtime then sets
  `XDG_CONFIG_HOME` to that folder.

A new readable format goes in `READ_EXTENSIONS`, the desktop file, the metainfo, `lecoq.wxs`'s
`Extensions`, `Info.plist` and the winget installer manifest.

## Releasing

1. Bump `version` in the root `Cargo.toml` (`[workspace.package]`) and run `cargo check` so
   `Cargo.lock` follows.
2. Add `<release version="X.Y.Z" date="YYYY-MM-DD"/>` at the top of `<releases>` in the metainfo.
3. On the reference machine (i5-12600KF, balanced power profile, Wayland), build the release binary
   and run the benchmarks:

   ```bash
   cargo build --release --locked -p lecoq --features avif
   tools/bench/bench.py fixture tools/bench/images
   tools/bench/bench.py cold tools/bench/images/IMG_0001.jpg --out packaging/release-notes/vX.Y.Z.bench-cold.json
   ```

4. Write `packaging/release-notes/vX.Y.Z.md` and check the result with
   `packaging/release_notes.py X.Y.Z`.
5. Commit all of it, then run the gate: `./tools/local-ci/run.sh`. It pushes `refs/notes/gate`.
6. Merge to `master`, tag the gated commit `vX.Y.Z` and push the tag.
7. After the release: publish the AUR files (`lecoq-X.Y.Z-aur.tar.gz`) to the AUR by hand, and
   submit the winget manifest (`lecoq-X.Y.Z-winget.zip`) to `microsoft/winget-pkgs` under
   `manifests/a/acastaner/lecoq/X.Y.Z/` (for instance with `wingetcreate submit`).

Windows and macOS builds are best-effort (§8.5): they build on GitHub's runners, which run no
tests, so the Explorer and Finder Sibling ordering tables (#17), which compile only there, go
untested. The macOS app is ad-hoc signed, not notarised, so Gatekeeper asks
before its first launch (right-click > Open); the MSI is unsigned, so SmartScreen may warn.

## Building locally

```bash
cargo build --release --locked -p lecoq --features avif
cargo install --locked cargo-deb@3.8.0 cargo-generate-rpm@0.21.1   # once
packaging/linux/build.sh dist tarball deb rpm
git archive --prefix=lecoq-X.Y.Z/ -o /some/dir/source.tar.gz HEAD
packaging/linux/build.sh dist pkgbuild /some/dir/source.tar.gz flatpak
flatpak-builder --user --force-clean build-dir dist/flatpak/io.github.acastaner.lecoq.yml
```

The release builds in `rust:<channel>-slim-bookworm`, the gate's base image, so its binary needs
glibc 2.36 or later. `--features avif` builds libavif and dav1d from source, which needs cmake,
make, a C++ compiler, meson, ninja and nasm; they link statically, as does lcms2, so the packages
gain no runtime dependency from them.
