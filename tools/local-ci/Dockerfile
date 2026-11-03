# syntax=docker/dockerfile:1
# check=skip=InvalidDefaultArgInFrom
# (Parser directives must lead the file. The check is skipped because RUST_VERSION deliberately
# has no default -- rust-toolchain.toml is its only source -- and a missing build arg still fails
# the build loudly.)
#
# The clean-room gate image (#14, spec §9). tools/local-ci/run.sh builds it on every run (the layer
# cache makes that near-instant) with an EMPTY build context -- the Dockerfile on stdin, as below --
# because nothing in it comes from the repository: the repo is a fresh clone mounted at run time.
#
#   docker build --build-arg RUST_VERSION=<rust-toolchain.toml channel> \
#                --build-arg GATE_UID="$(id -u)" --build-arg GATE_GID="$(id -g)" \
#                -t lecoq-gate:local - < tools/local-ci/Dockerfile
#
# The Rust version is pinned by rust-toolchain.toml: the launcher reads its channel (through
# check-toolchain-pin.sh) and passes it in. The suites run cargo through rustup's proxies, which
# honour rust-toolchain.toml, so the toolchain must be installed under exactly the channel's NAME:
# the official image installs "1.95.0", and a file saying "1.95" would otherwise make rustup try
# (and, as a non-root user, fail) to download a second toolchain mid-run. Hence the explicit
# `rustup toolchain install`. The inner run then re-checks `rustc --version` against the file.

ARG RUST_VERSION
FROM rust:${RUST_VERSION}-slim-bookworm

ARG RUST_VERSION
# git: the self-test of the guard scripts builds throwaway repositories. cmake, make, g++, meson,
# ninja and nasm: libavif-sys builds libavif and dav1d from source (the `avif` feature, §5.1).
# curl, ca-certificates, fontconfig and freetype: skia-bindings (lecoq-render, #25) downloads its
# prebuilt Skia with curl at build time and links fontconfig, freetype and libstdc++. fonts-dejavu-core:
# the text tool tests (#28) need at least one installed font.
# locales + en_US.UTF-8: the Sibling ordering tables (#15) are measured in that locale (GLib's
# collation runs through libc's wcsxfrm); slim images ship only C.UTF-8.
RUN apt-get update \
    && apt-get install -y --no-install-recommends git cmake make g++ meson ninja-build nasm locales \
         curl ca-certificates libfontconfig1-dev libfreetype-dev fonts-dejavu-core \
    && sed -i 's/^# *en_US.UTF-8 UTF-8/en_US.UTF-8 UTF-8/' /etc/locale.gen \
    && locale-gen \
    && rm -rf /var/lib/apt/lists/* \
    && rustup toolchain install "${RUST_VERSION}" --profile minimal \
         --component rustfmt --component clippy \
    && rustup default "${RUST_VERSION}" \
    && for t in $(rustup toolchain list | cut -d' ' -f1); do \
         case "$t" in "${RUST_VERSION}"-*) ;; *) rustup toolchain uninstall "$t" ;; esac; \
       done \
    && rustup toolchain list && rustc --version

# Cache mount point, owned by the (non-root) user the gate runs as. Docker copies an image
# directory's ownership into an EMPTY named volume on first mount, so a brand-new lecoq-gate-cargo
# volume is writable without ever running as root. It holds only the crates.io registry and the
# downloaded sources -- never build output, which stays in the clone's target/ and goes with it.
ARG GATE_UID=1000
ARG GATE_GID=1000
RUN mkdir -p /cache/cargo \
    && chown -R "${GATE_UID}:${GATE_GID}" /cache

# CARGO_HOME moves to the cache volume; the rustup proxies stay on PATH in /usr/local/cargo/bin,
# and RUSTUP_HOME (/usr/local/rustup, root-owned, read-only to the gate user) is untouched.
# CARGO_TERM_COLOR keeps the log readable when redirected to a file.
ENV CARGO_HOME=/cache/cargo \
    CARGO_TERM_COLOR=never \
    CARGO_INCREMENTAL=0
