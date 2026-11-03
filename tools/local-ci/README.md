# `tools/local-ci/` — the gate

lecoq's test gate runs on this machine, not on GitHub. GitHub runs no tests (spec §9). The gate
is a port of the clean-room local gate pattern used in ForgeLog (the reference implementation is
`~/Git/dotnet-forgeLog/tools/local-ci/`).

```bash
./tools/local-ci/run.sh              # the gate: both suites, clean room, records a pass
./tools/local-ci/run.sh lint         # one suite (no record)
./tools/local-ci/run.sh --in-place   # fast iteration in the working tree (never a gate)
./tools/local-ci/check-gate-record.sh HEAD   # was this commit's tree gated?
```

| File | What it is |
|---|---|
| `run.sh` | One script, three modes: launcher, inside the container, `--in-place` |
| `Dockerfile` | The gate image: `rust:<channel>-slim-bookworm`, with the channel taken from `rust-toolchain.toml` |
| `check-toolchain-pin.sh` | The Rust pin preflight, called first by `run.sh` in every mode |
| `check-gate-record.sh` | Does this commit's tree have a pass note? Exits 0 or 1 |
| `test_local_ci.sh` | Self-test of the two guard scripts above. The `test` suite runs it |

## The workflow: commit, gate, then fix in a new commit

1. Commit. The gate refuses a dirty tree (modified, staged or untracked files that aren't
   ignored) and lists the paths.
2. Run `./tools/local-ci/run.sh`.
3. If it's red, fix it in a **new** commit (not an amend) and gate again.

Any later commit, even a docs-only one, changes the tree and so loses the record. Commit any
bookkeeping **before** the final gate run.

## The suites

| Suite | Commands |
|---|---|
| `lint` | `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| `test` | `tools/local-ci/test_local_ci.sh`, `cargo test --workspace --all-features` |

Both run after `cargo fetch --locked`, which fails if `Cargo.toml` and `Cargo.lock` disagree.
`cargo test --workspace --all-features` covers both seams of spec §9: the headless App under
`egui_kittest`, which needs no display or GPU, and the Sibling ordering comparator. The tests need
no network, Docker or secrets. `--all-features` turns on AVIF (libavif + dav1d, built from
source), which needs cmake, meson, ninja and nasm: the gate image installs them, and `--in-place`
needs them on the host.

## Two modes

**Clean room** (the default, and the only result that counts as a gate):

- The launcher (`run.sh` on the host) refuses a dirty tree. It also refuses while another lecoq
  gate is running (label `lecoq.gate`).
- It clones the committed HEAD into `~/.cache/lecoq-gate/<UTC>-<sha7>/src`. That is a real
  `git clone --no-hardlinks` with a detached checkout, not `git archive`. It isn't `/tmp`, which
  is RAM-backed here, and `target/` grows to several GB.
- It builds the gate image from an empty context (`docker build - < Dockerfile`), which the layer
  cache makes near-instant. Then it re-executes the same `run.sh` inside the container with
  `LECOQ_GATE_INSIDE=1`.
- The run directory is deleted after a pass (`--keep` keeps it). After a failure it's kept and
  its path is printed.

**`--in-place`**: the suites run in your working tree with the toolchain rustup resolves. It
prints `IN-PLACE PASS … (not a gate result)` and never writes a record.

### The gate container

```bash
docker run --rm --init --name lecoq-gate-<run-id> \
  --label lecoq.gate=<run-id> --label lecoq.run=<run-id> \
  -u "$(id -u):$(id -g)" -e LECOQ_GATE_INSIDE=1 -e TINI_KILL_PROCESS_GROUP=1 \
  -e HOME=<run-dir>/home -v lecoq-gate-cargo:/cache/cargo -v <run-dir>:<run-dir> \
  -w <run-dir>/src lecoq-gate:local <run-dir>/src/tools/local-ci/run.sh <suites>
```

- **Never root**, so nothing root-owned lands on the host.
- **`HOME` is an empty directory**, so no host `~/.cargo/config.toml` or `~/.gitconfig` leaks in.
- **No Docker socket, no host network, no secrets.** The tests start no containers. Cargo reaches
  crates.io over the default bridge network. That network stays up for the whole run, so a test
  could reach the internet. None does, and none may: tests use fixtures copied into a temp dir.
- **Ctrl-C** stops it at once. That needs both `--init` and `TINI_KILL_PROCESS_GROUP=1`: without
  group mode, bash defers the signal until `cargo` finishes.
- **The sweep is run-scoped.** After the container exits, the launcher removes containers
  labelled `lecoq.run=<this run's id>` and nothing else. The container is `--rm`, so this only
  matters when the client was killed hard. A hung gate is removed with
  `docker rm -f $(docker ps -aq --filter label=lecoq.gate)`.

### Caches

The `lecoq-gate-cargo` named volume is `CARGO_HOME`: the crates.io registry index and the
downloaded crate sources. It holds no build output. `target/` lives in the clone and goes with it,
so every gate run is a full build. `--cold` deletes the volume first. The volume is unlabelled,
so no sweep touches it.

### The gate record

After a clean-room pass of **all** suites, the launcher writes a note on the commit's **tree**
and pushes it. A failed push is only a warning, for when you're offline.

```bash
git notes --ref=gate add -f -m "PASS lint test · rust <channel> · <UTC>" "<sha>^{tree}"
git push origin refs/notes/gate
```

- **It's on the tree**, so a `--no-ff` merge of an up-to-date branch is a new commit with the
  same tree and carries the record. If the base moved in the meantime, the merge tree is new and
  ungated, and the release check refuses it, correctly.
- **Subset and `--in-place` runs never write a note.**
- **The push is a plain `git push`.** If notes were written on another machine too, it's rejected
  as a non-fast-forward, and the launcher only warns. Fetch and merge first, then push again:
  `git fetch origin refs/notes/gate:refs/notes/gate-remote && git notes --ref=gate merge -s union refs/notes/gate-remote`.
- **It guards against forgetting, not against tampering.**
- **`check-gate-record.sh <commit-ish>`** exits 0 only for a note on that commit's tree whose
  first line starts with `PASS lint test`. The release workflow's `gate-record` job (#41) runs it
  after `git fetch origin refs/notes/gate:refs/notes/gate`.

### Bumping Rust

The pin lives in `rust-toolchain.toml` (`channel`), which must be an exact version (`X.Y` or
`X.Y.Z`). `Cargo.toml`'s `[workspace.package] rust-version` must equal its `X.Y`. With no
production Dockerfile to compare against, the pin's second place is the MSRV: this deliberately
ties the MSRV to the toolchain the gate proves.
`check-toolchain-pin.sh` enforces both in every mode. Inside the container and in place, it also
checks that `rustc --version` is that channel, so a stale image or a rustup override fails at the
start instead of halfway through. Bump the two files together, then check that the image tag
exists: `docker manifest inspect rust:<channel>-slim-bookworm`.

The official image installs its toolchain as `1.95.0`. rustup treats a `rust-toolchain.toml` that
says `1.95` as a different toolchain and would try to download it at run time, which fails as a
non-root user. The Dockerfile therefore installs the toolchain under exactly the channel's name
and uninstalls the other one.

A channel of `X.Y` floats within its patch releases. The image tag and `rustup toolchain install`
both take the newest `X.Y.z` when the layer is first built, and the layer cache then freezes it on
that machine. The preflight accepts any `X.Y.z` compiler for an `X.Y` channel. To pin the patch
too, write `X.Y.Z` in `rust-toolchain.toml`.

## Gotchas

- **Never edit `run.sh` while a run of it is executing**, even from another session sharing the
  checkout. Bash reads scripts incrementally. Do gate-script work in a separate worktree.
- **Don't pipe `run.sh` into `tail` or `tee` and trust `$?`.** Redirect it to a file and check
  the exit code yourself.
- **Host load causes fake reds.** Wait for the 1-minute load average to drop below the core
  count, then re-run the **same commit**. Don't "fix" the code for it.
- **New scripts need `git add --chmod=+x`.** This checkout sits on a fuseblk mount with
  `core.filemode=false`, so a plain `git add` records 100644.
