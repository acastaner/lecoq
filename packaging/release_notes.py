#!/usr/bin/env python3
"""lecoq release notes (#41, spec §9): the maintainer's notes plus the tools/bench numbers.

  packaging/release_notes.py <version>      e.g. 0.1.0; prints Markdown on stdout

Reads, for version X.Y.Z, from packaging/release-notes/:
  vX.Y.Z.md                 the notes, written by hand (required)
  vX.Y.Z.bench-*.json       tools/bench output (`bench.py cold IMAGE --out ...`) taken on the
                            reference machine before the release (at least the cold start)

Exits 1 naming what's missing, so the release workflow refuses a tag without bench numbers.
Commit these files before the final gate run: they are part of the gated tree.
"""
import glob, json, os, sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))
NOTES_DIR = os.path.join(ROOT, "packaging", "release-notes")


def fail(msg):
    print(f"ERROR: {msg}", file=sys.stderr)
    sys.exit(1)


def ms(v):
    return f"{v:.1f} ms"


def cold_section(data):
    s = data["summary"]
    verdict = "met" if s["within_budget"] else "**missed**"
    phases = " · ".join(f"{k} {v:.1f}" for k, v in s.get("phases_median", {}).items())
    rows = [
        "| Cold start (launch to first image painted) | p50 | p90 | max | budget (p90) |",
        "|---|---|---|---|---|",
        f"| {data['runs']} runs, {data['idle_s']} s idle | {ms(s['p50'])} | {ms(s['p90'])} | "
        f"{ms(s['max'])} | ≤ {s['budget_p90_ms']:.0f} ms: {verdict} |",
    ]
    if phases:
        rows.append("")
        rows.append(f"Phases (median ms from launch): {phases}.")
    if s.get("failed_runs"):
        rows.append(f"{s['failed_runs']} run(s) painted no image.")
    return rows


def main():
    if len(sys.argv) != 2 or sys.argv[1].startswith("-"):
        print(__doc__, file=sys.stderr)
        sys.exit(2)
    version = sys.argv[1].removeprefix("v")
    notes_path = os.path.join(NOTES_DIR, f"v{version}.md")
    if not os.path.isfile(notes_path):
        fail(f"no release notes at {os.path.relpath(notes_path, ROOT)}.")
    bench_paths = sorted(glob.glob(os.path.join(NOTES_DIR, f"v{version}.bench-*.json")))
    benches = []
    for p in bench_paths:
        with open(p) as f:
            benches.append(json.load(f))
    if not any(b.get("bench") == "cold" for b in benches):
        fail(f"no cold-start numbers: run tools/bench/bench.py cold IMAGE --out "
             f"packaging/release-notes/v{version}.bench-cold.json on the reference machine.")

    with open(notes_path) as f:
        out = [f.read().rstrip(), "", "## Performance (tools/bench)", ""]
    first = benches[0]
    out.append(f"Measured on {first['cpu']}, power profile {first['power_profile']}, "
               f"{first['session']} (spec §1.2).")
    for b in benches:
        out.append("")
        if b.get("bench") == "cold":
            out.extend(cold_section(b))
        else:
            # Later benches (Sibling switching, large folders) render their summary as-is until
            # they get a table of their own.
            out.append(f"{b.get('bench', '?')}: `{json.dumps(b.get('summary', {}))}`")
    print("\n".join(out))


if __name__ == "__main__":
    main()
