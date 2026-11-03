#!/usr/bin/env bash
set -euo pipefail

# lecoq -- the gate, run locally on this machine. There is no other: GitHub runs no tests.
#
# Usage: ./tools/local-ci/run.sh [suite ...] [options]
#
#   Suites (default, or `all`: both, in this order)
#     lint   cargo fmt --check; cargo clippy --workspace --all-targets --all-features -- -D warnings
#     test   the guard scripts' self-test; cargo test --workspace --all-features (the headless App
#            tests and the Sibling ordering comparator, spec §9)
#     --all-features builds AVIF (libavif + dav1d), which needs cmake, meson, ninja and nasm: the
#     gate image has them; --in-place needs them on the host.
#
#   Options
#     --in-place        run in the working tree with the host toolchain (fast iteration).
#                       NOT a gate result: no clean room, no gate record.
#     --cold            clean room only: delete the cargo cache volume before the run
#     --keep            clean room only: keep the run directory after a pass (it is always
#                       kept after a failure)
#     --no-clean        skip this run's leftover-container sweep on the way out
#     -h|--help         this text
#
# TWO MODES, ONE SCRIPT (a port of the clean-room local gate; see tools/local-ci/README.md).
#
#   Clean room (the default, and the only gate result). The launcher refuses a dirty working tree,
#   clones the committed HEAD into ~/.cache/lecoq-gate/<run-id>/src, builds the gate image
#   (tools/local-ci/Dockerfile: Rust pinned by rust-toolchain.toml) and re-executes this script
#   inside it with LECOQ_GATE_INSIDE=1. Nothing from the host leaks in -- no target/, no
#   ~/.cargo, no config -- and the working tree is never touched. After a pass of all suites the
#   launcher records it as a git note (refs/notes/gate) on the tested commit's TREE and pushes it;
#   the release workflow's gate-record job (#41) refuses a tree without one (check-gate-record.sh).
#
#   --in-place. The suites run here, in the working tree, with whatever toolchain rustup resolves
#   for rust-toolchain.toml. Useful while iterating; never a gate.
#
# ON DOCKER. Nothing here runs a `docker` command against anything it did not create. Every run has
# a run id; the gate container carries it as `lecoq.run=<run-id>` (and `lecoq.gate=<run-id>`, for
# the one-gate-at-a-time check), runs as the calling user (never root), and sees no host resource
# but its run directory and one unlabelled cache volume (lecoq-gate-cargo: the crates.io registry,
# never build output). The tests start no containers, so the gate container gets no Docker socket.
# The exit sweep removes ONLY containers labelled with this run's id.

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "${REPO_ROOT}"

GATE_LABEL="lecoq.gate"
# On everything a run starts; the exit sweep selects by it. Its value is the run id.
RUN_LABEL="lecoq.run"
GATE_IMAGE="lecoq-gate:local"
GATE_CACHE_ROOT="${HOME}/.cache/lecoq-gate"
GATE_CARGO_VOLUME="lecoq-gate-cargo"
ALL_SUITES=(lint test)

usage() { sed -n '/^# Usage:/,/-h|--help/p' "$0" | sed 's/^# \{0,1\}//'; }

# --- Arguments -----------------------------------------------------------------------------

SUITES=()
NO_CLEAN=0
IN_PLACE=0
COLD=0
KEEP=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    lint|test)   SUITES+=("$1") ;;
    all)         SUITES+=("${ALL_SUITES[@]}") ;;
    --in-place)  IN_PLACE=1 ;;
    --cold)      COLD=1 ;;
    --keep)      KEEP=1 ;;
    --no-clean)  NO_CLEAN=1 ;;
    -h|--help)   usage; exit 0 ;;
    *)
      echo "ERROR: unknown argument '$1'." >&2
      echo >&2
      usage >&2
      exit 1
      ;;
  esac
  shift
done

if [[ ${#SUITES[@]} -eq 0 ]]; then
  SUITES=("${ALL_SUITES[@]}")
fi

# inside     -- this script, re-executed by the launcher inside the gate container
# in-place   -- the working tree and the host toolchain; not a gate result
# clean-room -- the launcher (the default)
if [[ "${LECOQ_GATE_INSIDE:-0}" == "1" ]]; then
  # Only the launcher sets the marker, inside the gate container. Leaked into a host shell it would
  # silently skip the clean room, so refuse it there.
  if [[ ! -f /.dockerenv ]]; then
    echo "ERROR: LECOQ_GATE_INSIDE=1 is set, but this is not the gate container. Unset it." >&2
    exit 1
  fi
  MODE="inside"
elif (( IN_PLACE == 1 )); then
  MODE="in-place"
else
  MODE="clean-room"
fi

if [[ "${MODE}" == "in-place" ]] && (( COLD == 1 || KEEP == 1 )); then
  echo "ERROR: --cold and --keep apply to the clean room only, not to --in-place." >&2
  exit 1
fi

# --- Preconditions -------------------------------------------------------------------------

if [[ ! -f Cargo.toml || ! -f rust-toolchain.toml ]]; then
  echo "ERROR: run this from the repository root (Cargo.toml and rust-toolchain.toml must be here)." >&2
  exit 1
fi

# The Rust pin: rust-toolchain.toml's channel, which Cargo.toml's rust-version must match. Checked
# first, in every mode. Where the suites run (inside, in-place) the compiler actually in use must
# match it too -- a stale gate image or a rustup override fails here, not halfway through.
if [[ "${MODE}" == "clean-room" ]]; then
  RUST_VERSION="$(./tools/local-ci/check-toolchain-pin.sh .)"
else
  if ! command -v rustc >/dev/null 2>&1; then
    echo "ERROR: rustc is not on PATH; install rustup (it reads rust-toolchain.toml)." >&2
    exit 1
  fi
  RUST_VERSION="$(./tools/local-ci/check-toolchain-pin.sh . --rustc "$(rustc --version)")"
fi

if [[ "${MODE}" == "clean-room" ]]; then
  if ! command -v docker >/dev/null 2>&1; then
    echo "ERROR: docker is required -- the gate runs every suite inside a container." >&2
    echo "       './tools/local-ci/run.sh --in-place' needs no Docker (and is not a gate)." >&2
    exit 1
  fi
  if ! docker info >/dev/null 2>&1; then
    echo "ERROR: the Docker daemon is not reachable." >&2
    echo "       You must be in the 'docker' group: id -nG | tr ' ' '\\n' | grep -x docker" >&2
    exit 1
  fi
fi

# One clean-room gate at a time, per repository: two would compete for the machine's CPU and memory
# (a loaded host makes timing-sensitive tests flaky). A resource rule, not a safety one -- the exit
# sweep is run-scoped. Selects on this repository's own gate label only. Checked by the launcher,
# before the exit trap is armed, so the refusal itself cannot sweep.
refuse_if_gate_running() {
  local running
  running="$(docker ps -q --filter "label=${GATE_LABEL}")"
  [[ -z "${running}" ]] && return 0
  echo "ERROR: a lecoq clean-room gate is already running:" >&2
  docker ps --filter "label=${GATE_LABEL}" --format '    {{.Names}}  ({{.Status}})' >&2
  echo "       Wait for it to finish. A hung one is removed with:" >&2
  echo "       docker rm -f \$(docker ps -aq --filter label=${GATE_LABEL})" >&2
  exit 1
}

# --- Cleanup -------------------------------------------------------------------------------
#
# The gate container is --rm, so normally nothing is left. But a launcher killed hard (or a docker
# client that lost its daemon connection) can leave it running. The sweep removes everything
# labelled `lecoq.run=<this run's id>` and nothing else -- never a machine-wide selector. Only the
# host launcher sweeps, once, after the gate container exits (or on the way out of an abort).
SWEPT_RUN_ID=""
SWEEP_ON_EXIT=0

sweep_run_containers() {
  (( NO_CLEAN == 1 )) && { echo "--- Skipping the container sweep (--no-clean) ---"; return 0; }
  [[ -n "${SWEPT_RUN_ID}" ]] || return 0
  command -v docker >/dev/null 2>&1 || return 0
  local own
  own="$(docker ps -aq --filter "label=${RUN_LABEL}=${SWEPT_RUN_ID}" 2>/dev/null || true)"
  if [[ -z "${own}" ]]; then
    echo "Docker is clean -- this run (${SWEPT_RUN_ID}) left no containers."
  else
    echo "--- Sweeping this run's leftover containers (${RUN_LABEL}=${SWEPT_RUN_ID}): $(wc -w <<<"${own}") ---"
    # shellcheck disable=SC2086
    docker rm -f -v ${own} >/dev/null 2>&1 || true
    echo "Swept."
  fi
}

# Set by the launcher once the run directory exists, so an abort before the gate container even
# starts (a failed clone or image build) still says where the directory was left.
GATE_RUN_DIR=""
# Set once a failure has been explained, so on_exit does not add a generic line after it.
FAILURE_REPORTED=0

# `set -e` aborts silently on its own; name the exit code, and sweep when armed.
on_exit() {
  local rc=$?
  if (( SWEEP_ON_EXIT == 1 )); then
    echo
    sweep_run_containers || true
  fi
  if (( rc != 0 && FAILURE_REPORTED == 0 )); then
    echo >&2
    echo "ERROR: gate run aborted (exit code ${rc}). See the message above for the cause." >&2
    if [[ -n "${GATE_RUN_DIR}" && -d "${GATE_RUN_DIR}" ]]; then
      echo "       Run directory kept for inspection: ${GATE_RUN_DIR}" >&2
    fi
  fi
}

# --- Suites --------------------------------------------------------------------------------

banner() {
  echo
  echo "=================================================================="
  echo "Suite: $1"
  echo "=================================================================="
}

run_lint() {
  banner "lint -- rustfmt, clippy"
  echo "--- cargo fmt --check ---"
  cargo fmt --check
  echo "--- cargo clippy --workspace --all-targets --all-features -- -D warnings ---"
  cargo clippy --workspace --all-targets --all-features -- -D warnings
}

run_test() {
  banner "test -- guard self-test, cargo test"
  # The gate's own guards (toolchain pin, gate record). A regression in them is silent until the
  # day they wave something through, so they are gated like any other code.
  echo "--- Guard scripts self-test ---"
  ./tools/local-ci/test_local_ci.sh
  # Both seams of spec §9: the headless App (egui_kittest, crates/lecoq/tests) and the Sibling
  # ordering comparator. No display, GPU or network is needed.
  echo "--- cargo test --workspace --all-features ---"
  cargo test --workspace --all-features
}

# --- Run: the suites (inside the gate container, or --in-place) ------------------------------

run_suites() {
  echo "=================================================================="
  if [[ "${MODE}" == "inside" ]]; then
    echo "lecoq gate -- clean room (inside the gate container)"
  else
    echo "lecoq gate -- IN-PLACE (working tree, host toolchain; not a gate result)"
  fi
  echo "  Suites  : ${SUITES[*]}"
  echo "  Rust    : $(rustc --version) (rust-toolchain.toml pins ${RUST_VERSION})"
  echo "=================================================================="

  # The lockfile is the dependency pin: fail here, loudly, when Cargo.toml and Cargo.lock disagree,
  # rather than let the suites below quietly resolve something else.
  echo "--- cargo fetch --locked ---"
  cargo fetch --locked

  RAN=()
  for suite in "${SUITES[@]}"; do
    case "${suite}" in
      lint) run_lint ; RAN+=(lint) ;;
      test) run_test ; RAN+=(test) ;;
    esac
  done

  # Name every suite that actually ran: a run that quietly covered less than it was asked to is the
  # failure mode that makes a green gate worthless.
  echo
  echo "=================================================================="
  if [[ "${MODE}" == "inside" ]]; then
    echo "PASS -- suites run: ${RAN[*]}"
  else
    echo "IN-PLACE PASS -- suites run: ${RAN[*]} (not a gate result)"
  fi
  echo "  Requested: ${SUITES[*]}"
  echo "=================================================================="
}

# --- Run: the clean-room launcher (the default) --------------------------------------------

# True when this run covers the whole gate. Only such a run may write a gate record.
covers_all_suites() {
  local want
  for want in "${ALL_SUITES[@]}"; do
    [[ " ${SUITES[*]} " == *" ${want} "* ]] || return 1
  done
}

launch_clean_room() {
  refuse_if_gate_running
  trap on_exit EXIT

  # HEAD only: the gate proves a commit, so anything the commit does not contain is refused rather
  # than silently left out of the run.
  local dirty
  dirty="$(git status --porcelain)"
  if [[ -n "${dirty}" ]]; then
    echo "ERROR: the working tree is not clean. The gate tests the committed HEAD, so it refuses" >&2
    echo "       to start while these paths differ from it (modified, staged or untracked):" >&2
    printf '%s\n' "${dirty}" | sed 's/^/         /' >&2
    echo "       Commit them, or iterate with --in-place (which is not a gate)." >&2
    exit 1
  fi

  local head_sha run_id run_dir
  head_sha="$(git rev-parse HEAD)"
  run_id="$(date -u +%Y%m%dT%H%M%SZ)-${head_sha:0:7}"
  # Not /tmp: on this machine that is a RAM-backed tmpfs, and target/ grows to several GB.
  run_dir="${GATE_CACHE_ROOT}/${run_id}"

  echo "=================================================================="
  echo "lecoq gate -- clean room"
  echo "  Commit  : ${head_sha} ($(git rev-parse --abbrev-ref HEAD))"
  echo "  Suites  : ${SUITES[*]}"
  echo "  Rust    : ${RUST_VERSION} (rust-toolchain.toml)"
  echo "  Run dir : ${run_dir}"
  echo "=================================================================="

  # A real clone with a real .git, not an archive, so git-based checks keep working inside.
  echo "--- Cloning HEAD ---"
  mkdir -p "${GATE_CACHE_ROOT}"
  GATE_RUN_DIR="${run_dir}"
  git clone --no-hardlinks --quiet "${REPO_ROOT}" "${run_dir}/src"
  git -C "${run_dir}/src" checkout --quiet --detach "${head_sha}"
  if [[ "$(git -C "${run_dir}/src" rev-parse HEAD)" != "${head_sha}" ]]; then
    echo "ERROR: the clone in ${run_dir}/src is not at ${head_sha}." >&2
    exit 1
  fi
  # An empty HOME: no host ~/.cargo/config, ~/.gitconfig or anything else.
  mkdir -p "${run_dir}/home"

  if (( COLD == 1 )); then
    echo "--- Deleting the cargo cache volume (--cold); it is recreated on mount ---"
    docker volume rm -f "${GATE_CARGO_VOLUME}" >/dev/null
  fi

  # Built every run from an empty context -- the image needs no repo files -- and the layer cache
  # makes a rebuild near-instant. The Dockerfile comes from the clone, i.e. from HEAD.
  echo "--- Gate image (${GATE_IMAGE}, rust ${RUST_VERSION}) ---"
  docker build --quiet \
    --build-arg "RUST_VERSION=${RUST_VERSION}" \
    --build-arg "GATE_UID=$(id -u)" \
    --build-arg "GATE_GID=$(id -g)" \
    -t "${GATE_IMAGE}" - < "${run_dir}/src/tools/local-ci/Dockerfile"

  # From here on the host sweep runs whatever happens, including Ctrl-C.
  SWEPT_RUN_ID="${run_id}"
  SWEEP_ON_EXIT=1
  local rc=0
  # --init + TINI_KILL_PROCESS_GROUP=1: without --init the inner bash is PID 1, which ignores the
  #   SIGINT/SIGTERM docker proxies on Ctrl-C; with --init alone, tini signals bash only, and bash
  #   defers it until its foreground child (cargo) finishes. Signalling the whole group stops it.
  # -u: never root, so nothing root-owned lands in the run directory.
  # -v run_dir at the identical path: the paths printed inside are valid on the host too.
  # Default (bridge) network: cargo fetches from crates.io; the tests need no network.
  docker run --rm --init \
    --name "lecoq-gate-${run_id}" \
    --label "${GATE_LABEL}=${run_id}" \
    --label "${RUN_LABEL}=${run_id}" \
    -u "$(id -u):$(id -g)" \
    -e LECOQ_GATE_INSIDE=1 \
    -e TINI_KILL_PROCESS_GROUP=1 \
    -e HOME="${run_dir}/home" \
    -v "${GATE_CARGO_VOLUME}:/cache/cargo" \
    -v "${run_dir}:${run_dir}" \
    -w "${run_dir}/src" \
    "${GATE_IMAGE}" \
    "${run_dir}/src/tools/local-ci/run.sh" "${SUITES[@]}" || rc=$?

  echo
  SWEEP_ON_EXIT=0
  sweep_run_containers || true

  if (( rc != 0 )); then
    echo >&2
    echo "FAIL -- the clean-room gate failed (exit ${rc}) at ${head_sha}." >&2
    echo "       Run directory kept for inspection: ${run_dir}" >&2
    if (( rc == 137 )); then
      echo "       Exit 137 is SIGKILL, not a failing test: the container was killed from outside" >&2
      echo "       (the OOM killer -- grep oom_kill /proc/vmstat -- or a manual kill)." >&2
      echo "       Re-run the gate on the same commit before suspecting the change." >&2
    elif (( rc == 130 || rc == 143 )); then
      echo "       Interrupted (SIGINT/SIGTERM), not a failing test. Re-run when ready." >&2
    else
      echo "       Fix it in a NEW commit (not an amend) and run the gate again." >&2
    fi
    FAILURE_REPORTED=1   # explained just above; on_exit need not repeat it
    exit "${rc}"
  fi

  # The gate record: a note on the commit's TREE, not the commit. A --no-ff merge of an up-to-date
  # branch is a new commit with the same tree, and carries the record; if the base moved meanwhile,
  # the merge tree differs and has none -- correctly, because that tree was never gated. This
  # guards against forgetting, not against tampering.
  local record="none (subset run -- only a run of all suites writes one)"
  if covers_all_suites; then
    local note
    note="PASS ${ALL_SUITES[*]} · rust ${RUST_VERSION} · $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    git notes --ref=gate add -f -m "${note}" "${head_sha}^{tree}"
    record="refs/notes/gate on tree $(git rev-parse --short "${head_sha}^{tree}")"
    if git push --quiet origin refs/notes/gate; then
      record+=" (pushed)"
    else
      echo "WARNING: could not push refs/notes/gate -- the record exists only locally." >&2
      echo "         Push it before tagging: git push origin refs/notes/gate" >&2
      record+=" (NOT pushed)"
    fi
  fi

  if (( KEEP == 1 )); then
    echo "Run directory kept (--keep): ${run_dir}"
  else
    rm -rf "${run_dir}"
  fi
  GATE_RUN_DIR=""

  echo
  echo "=================================================================="
  echo "GATE PASS -- clean room, suites: ${SUITES[*]}"
  echo "  Commit  : ${head_sha}"
  echo "  Record  : ${record}"
  echo "=================================================================="
}

if [[ "${MODE}" == "clean-room" ]]; then
  launch_clean_room
else
  trap on_exit EXIT
  run_suites
fi
