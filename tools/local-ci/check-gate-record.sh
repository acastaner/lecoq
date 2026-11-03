#!/usr/bin/env bash
set -euo pipefail

# lecoq -- is there a gate record for this commit's tree? (#14, spec §9)
#
# Usage: ./tools/local-ci/check-gate-record.sh <commit-ish>
#
# Exits 0 when refs/notes/gate holds a note on <commit-ish>'s TREE whose first line starts with
# "PASS lint test" -- written by ./tools/local-ci/run.sh after a clean-room pass of all suites.
# Otherwise names the tree that lacks one and exits 1 (2 on a usage error).
#
# The note is on the tree, not the commit, so a --no-ff merge of a gated, up-to-date branch carries
# the branch's record. The release workflow's gate-record job (#41) runs this before building
# anything; locally, fetch the notes first if another machine ran the gate:
#   git fetch origin refs/notes/gate:refs/notes/gate

if [[ $# -ne 1 ]]; then
  sed -n '/^# Usage:/p' "$0" | sed 's/^# \{0,1\}//' >&2
  exit 2
fi

commitish="$1"
if ! tree="$(git rev-parse --verify --quiet "${commitish}^{tree}")"; then
  echo "ERROR: '${commitish}' does not resolve to a tree." >&2
  exit 1
fi

first_line="$(git notes --ref=gate show "${tree}" 2>/dev/null | head -n1 || true)"
if [[ "${first_line}" == "PASS lint test"* ]]; then
  echo "Gate record for ${commitish} (tree ${tree}): ${first_line}"
  exit 0
fi

echo "ERROR: no gate record for ${commitish} -- tree ${tree} was never gated." >&2
if [[ -n "${first_line}" ]]; then
  echo "       Its note does not record a full pass: ${first_line}" >&2
fi
echo "       Run ./tools/local-ci/run.sh (all suites, clean room) on a commit with this tree," >&2
echo "       then make sure refs/notes/gate is pushed: git push origin refs/notes/gate" >&2
exit 1
