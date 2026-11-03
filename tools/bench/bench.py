#!/usr/bin/env python3
"""tools/bench: lecoq's performance budgets (spec §1.2), descended from the cold-start spike's
bench.py (branch prototype/startup-spike, acastaner/lecoq#9).

Not part of the gate: it depends on the hardware and the power profile. Run it by hand on the
reference machine (i5-12600KF, balanced power profile, Wayland) and put the numbers in the release
notes.

  tools/bench/bench.py fixture DIR [--count N]
      Write N (default 1) generated 12 MP (4000x3000) JPEGs, IMG_0001.jpg..., into DIR. Needs Pillow.
      Keep them out of the repository.

  tools/bench/bench.py cold IMAGE [--runs 15] [--idle 1.5] [--x11] [--binary PATH] [--out FILE]
      Cold start (§1.2): launch lecoq on IMAGE `runs` times, each after `idle` seconds of idle,
      and time launch -> first image painted. Wayland unless --x11 (XWayland).

  tools/bench/bench.py switch DIR [--count 25] [--interval 200] [--x11] [--binary PATH] [--out FILE]
      Next Sibling (§1.2): open the first image of DIR (at least count + 1 images, e.g. from
      `fixture DIR --count 26`), then move to the next Sibling `count` times, `interval` ms after
      each switch was painted, timing each from the command to the frame that draws the new image.

  tools/bench/bench.py folder IMAGE [--files 10000] [--runs 15] [--idle 1.5] [--x11] [--binary PATH]
      Large directories (§1.2): cold-start IMAGE alone in a folder, then in a folder of `files`
      files (IMAGE plus hard links to a small JPEG), and compare. Also reports when the scan of the
      big folder finished (`siblings_scanned`), which happens after the first paint.

The clock starts just before exec (LECOQ_BENCH_T0_NS) and stops when lecoq builds the frame that
draws the first image (`first_image_painted`), which eframe paints and hands to the compositor
straight after; it appears on screen at the next vblank. LECOQ_BENCH_EXIT=1 makes lecoq close
right after. Other marks: main, renderer_ready (window + GL context created), decoded,
siblings_scanned. LECOQ_BENCH_NAV=<interval ms>,<count> drives the Sibling switches (`switch_ms`
lines); with a count of 0 lecoq closes as soon as the scan is done.
"""
import argparse, json, os, platform, statistics, subprocess, sys, time

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
DEFAULT_BINARY = os.path.join(ROOT, "target", "release", "lecoq")
BUDGET_COLD_P90_MS = 150.0
BUDGET_SWITCH_P90_MS = 50.0
# "No later than in a folder of one": p90s within this much of each other count as equal.
FOLDER_NOISE_MS = 5.0


def pct(values, q):
    v = sorted(values)
    return v[min(len(v) - 1, int(round(q * (len(v) - 1))))]


def power_profile():
    try:
        return subprocess.run(["powerprofilesctl", "get"], capture_output=True, text=True, timeout=5).stdout.strip() or "unknown"
    except (OSError, subprocess.SubprocessError):
        return "unknown"


def cpu_model():
    try:
        with open("/proc/cpuinfo") as f:
            for line in f:
                if line.startswith("model name"):
                    return line.split(":", 1)[1].strip()
    except OSError:
        pass
    return platform.processor() or "unknown"


def launch(binary, image, x11, timeout=30, extra_env=None, switches=None):
    """Run lecoq once; returns its marks. `switch_ms` lines are appended to `switches`."""
    env = dict(os.environ, LC_ALL="C.UTF-8")
    env.update(extra_env if extra_env is not None else {"LECOQ_BENCH_EXIT": "1"})
    if x11:
        env.pop("WAYLAND_DISPLAY", None)
        env["XDG_SESSION_TYPE"] = "x11"
    env["LECOQ_BENCH_T0_NS"] = str(time.time_ns())
    try:
        out = subprocess.run([binary, image], env=env, capture_output=True, text=True, timeout=timeout).stdout
    except subprocess.TimeoutExpired as e:  # e.g. the compositor stopped sending frame callbacks
        out = e.stdout.decode() if isinstance(e.stdout, bytes) else (e.stdout or "")
    marks = {}
    for line in out.splitlines():
        p = line.split()
        if len(p) == 2:
            try:
                value = float(p[1])
            except ValueError:
                continue
            if p[0] == "switch_ms":
                if switches is not None:
                    switches.append(value)
            else:
                marks[p[0]] = value
    return marks


def check_binary(args):
    if not os.access(args.binary, os.X_OK):
        sys.exit(f"no lecoq binary at {args.binary}: run `cargo build --release` first")


def session_name(args):
    return "x11 (XWayland)" if args.x11 else ("wayland" if os.environ.get("WAYLAND_DISPLAY") else os.environ.get("XDG_SESSION_TYPE", "?"))


def cold_runs(args, image, label="run"):
    """`args.runs` cold starts of `image`; returns the marks of the runs that painted, and the failures."""
    runs, failed = [], 0
    for i in range(args.runs):
        time.sleep(args.idle)
        m = launch(args.binary, image, args.x11)
        if "first_image_painted" in m:
            runs.append(m)
            print(f"  {label} {i + 1:2d}: {m['first_image_painted']:6.1f} ms", flush=True)
        else:
            failed += 1
            print(f"  {label} {i + 1:2d}: no first_image_painted mark ({m})", flush=True)
    return runs, failed


def cmd_cold(args):
    check_binary(args)
    session = session_name(args)
    profile = power_profile()
    print(f"cold start: {args.runs} runs, {args.idle}s idle, {session}, power profile {profile}, {cpu_model()}", flush=True)
    runs, failed = cold_runs(args, args.image)
    if not runs:
        sys.exit("no run painted an image")
    fp = [m["first_image_painted"] for m in runs]
    phases = {k: statistics.median(m[k] for m in runs if k in m) for k in ("main", "renderer_ready", "decoded", "first_image_painted") if any(k in m for m in runs)}
    p90 = pct(fp, 0.9)
    summary = {
        "p50": statistics.median(fp), "p90": p90, "max": max(fp), "min": min(fp),
        "phases_median": phases, "failed_runs": failed,
        "budget_p90_ms": BUDGET_COLD_P90_MS, "within_budget": p90 <= BUDGET_COLD_P90_MS,
    }
    print(f"first image painted: p50 {summary['p50']:.1f}  p90 {p90:.1f}  max {summary['max']:.1f} ms "
          f"(budget p90 <= {BUDGET_COLD_P90_MS:.0f}: {'met' if summary['within_budget'] else 'MISSED'})")
    print("phases (median ms from launch): " + " · ".join(f"{k} {v:.1f}" for k, v in phases.items()))
    if args.out:
        with open(args.out, "w") as f:
            json.dump({
                "bench": "cold", "image": os.path.abspath(args.image), "binary": os.path.abspath(args.binary),
                "session": session, "power_profile": profile, "cpu": cpu_model(),
                "runs": args.runs, "idle_s": args.idle, "summary": summary, "marks": runs,
            }, f, indent=1)
        print(f"results written to {args.out}")


def cmd_switch(args):
    check_binary(args)
    images = sorted(f for f in os.listdir(args.dir) if f.lower().endswith((".jpg", ".jpeg")))
    if len(images) < args.count + 1:
        sys.exit(f"{args.dir} holds {len(images)} JPEGs; {args.count} switches need {args.count + 1} "
                 f"(tools/bench/bench.py fixture {args.dir} --count {args.count + 1})")
    session = session_name(args)
    profile = power_profile()
    print(f"sibling switch: {args.count} switches every {args.interval} ms, {session}, power profile {profile}, {cpu_model()}", flush=True)
    time.sleep(args.idle)
    switches = []
    timeout = 30 + args.count * (args.interval / 1000 + 1)
    m = launch(args.binary, os.path.join(args.dir, images[0]), args.x11, timeout=timeout,
               extra_env={"LECOQ_BENCH_NAV": f"{args.interval},{args.count}"}, switches=switches)
    if not switches:
        sys.exit(f"no switch was timed ({m})")
    for i, ms in enumerate(switches):
        print(f"  switch {i + 1:2d}: {ms:6.1f} ms", flush=True)
    p90 = pct(switches, 0.9)
    summary = {
        "switches": len(switches), "p50": statistics.median(switches), "p90": p90, "max": max(switches),
        "budget_p90_ms": BUDGET_SWITCH_P90_MS, "within_budget": p90 <= BUDGET_SWITCH_P90_MS,
        "siblings_scanned_ms": m.get("siblings_scanned"),
    }
    print(f"key press to paint: p50 {summary['p50']:.1f}  p90 {p90:.1f}  max {summary['max']:.1f} ms "
          f"(budget p90 <= {BUDGET_SWITCH_P90_MS:.0f}: {'met' if summary['within_budget'] else 'MISSED'})")
    if args.out:
        with open(args.out, "w") as f:
            json.dump({
                "bench": "switch", "dir": os.path.abspath(args.dir), "binary": os.path.abspath(args.binary),
                "session": session, "power_profile": profile, "cpu": cpu_model(), "interval_ms": args.interval,
                "summary": summary, "switch_ms": switches, "marks": m,
            }, f, indent=1)
        print(f"results written to {args.out}")


def cmd_folder(args):
    import shutil, tempfile
    check_binary(args)
    session = session_name(args)
    profile = power_profile()
    filler = os.path.join(ROOT, "tests", "fixtures", "small-320x240.jpg")
    name = os.path.basename(args.image)
    with tempfile.TemporaryDirectory(prefix="lecoq-bench-") as tmp:
        alone, big = os.path.join(tmp, "alone"), os.path.join(tmp, "big")
        os.makedirs(alone)
        os.makedirs(big)
        shutil.copy(args.image, os.path.join(alone, name))
        shutil.copy(args.image, os.path.join(big, name))
        shutil.copy(filler, os.path.join(big, "filler.jpg"))
        for n in range(args.files - 2):
            os.link(os.path.join(big, "filler.jpg"), os.path.join(big, f"photo_{n:05d}.jpg"))
        print(f"large directory: {args.runs} runs each, {args.files} files, {session}, power profile {profile}, {cpu_model()}", flush=True)
        alone_runs, _ = cold_runs(args, os.path.join(alone, name), "alone")
        big_runs, _ = cold_runs(args, os.path.join(big, name), "big  ")
        if not alone_runs or not big_runs:
            sys.exit("a run painted no image")
        # When does the scan of the big folder finish? (A count of 0 closes lecoq once it has.)
        time.sleep(args.idle)
        scan = launch(args.binary, os.path.join(big, name), args.x11, extra_env={"LECOQ_BENCH_NAV": "0,0"})
    a = pct([m["first_image_painted"] for m in alone_runs], 0.9)
    b = pct([m["first_image_painted"] for m in big_runs], 0.9)
    within = b <= a + FOLDER_NOISE_MS
    summary = {"alone_p90": a, "big_p90": b, "noise_ms": FOLDER_NOISE_MS, "within_budget": within,
               "siblings_scanned_ms": scan.get("siblings_scanned"), "scan_first_image_painted_ms": scan.get("first_image_painted")}
    print(f"first image painted p90: alone {a:.1f} ms, in {args.files} files {b:.1f} ms "
          f"(budget: no later, within {FOLDER_NOISE_MS:.0f} ms noise: {'met' if within else 'MISSED'})")
    if scan.get("siblings_scanned") is not None:
        print(f"scan of {args.files} files done {scan['siblings_scanned']:.1f} ms after launch "
              f"(first image painted at {scan.get('first_image_painted', float('nan')):.1f} ms)")
    if args.out:
        with open(args.out, "w") as f:
            json.dump({"bench": "folder", "image": os.path.abspath(args.image), "files": args.files,
                       "session": session, "power_profile": profile, "cpu": cpu_model(), "summary": summary,
                       "alone": alone_runs, "big": big_runs, "scan_run": scan}, f, indent=1)
        print(f"results written to {args.out}")


def cmd_fixture(args):
    try:
        from PIL import Image, ImageDraw, ImageFilter
    except ImportError:
        sys.exit("generating fixtures needs Pillow (python3 -m pip install pillow)")
    import random
    os.makedirs(args.dir, exist_ok=True)
    w, h = 4000, 3000
    for n in range(1, args.count + 1):
        rnd = random.Random(n)
        # Photo-like content: smooth colour fields, shapes and fine grain, so the JPEG has about the
        # entropy (3-5 MB at quality 92) of a 12 MP camera image rather than compressing to nothing.
        base = Image.new("RGB", (w // 8, h // 8))
        draw = ImageDraw.Draw(base)
        for _ in range(60):
            x, y = rnd.randrange(w // 8), rnd.randrange(h // 8)
            r = rnd.randrange(20, 200)
            draw.ellipse([x - r, y - r, x + r, y + r], fill=tuple(rnd.randrange(256) for _ in range(3)))
        img = base.filter(ImageFilter.GaussianBlur(12)).resize((w, h), Image.BICUBIC)
        grain = Image.merge("RGB", [Image.effect_noise((w, h), 40 + 10 * c) for c in range(3)])
        img = Image.blend(img, grain, 0.18)
        path = os.path.join(args.dir, f"IMG_{n:04d}.jpg")
        img.save(path, quality=92)
        print(f"{path}: {w}x{h}, {os.path.getsize(path) / 1e6:.1f} MB")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("cold", help="cold start: launch to first image painted")
    c.add_argument("image")
    c.add_argument("--runs", type=int, default=15)
    c.add_argument("--idle", type=float, default=1.5)
    c.add_argument("--x11", action="store_true", help="run under XWayland instead of Wayland")
    c.add_argument("--binary", default=DEFAULT_BINARY)
    c.add_argument("--out", help="write the marks and summary as JSON")
    c.set_defaults(func=cmd_cold)
    s = sub.add_parser("switch", help="next Sibling: key press to paint, flipping every 200 ms")
    s.add_argument("dir")
    s.add_argument("--count", type=int, default=25)
    s.add_argument("--interval", type=int, default=200, help="ms between a switch's paint and the next key press")
    s.add_argument("--idle", type=float, default=1.5)
    s.add_argument("--x11", action="store_true", help="run under XWayland instead of Wayland")
    s.add_argument("--binary", default=DEFAULT_BINARY)
    s.add_argument("--out", help="write the timings and summary as JSON")
    s.set_defaults(func=cmd_switch)
    d = sub.add_parser("folder", help="large directories: first paint alone vs among 10,000 files")
    d.add_argument("image")
    d.add_argument("--files", type=int, default=10000)
    d.add_argument("--runs", type=int, default=15)
    d.add_argument("--idle", type=float, default=1.5)
    d.add_argument("--x11", action="store_true", help="run under XWayland instead of Wayland")
    d.add_argument("--binary", default=DEFAULT_BINARY)
    d.add_argument("--out", help="write the marks and summary as JSON")
    d.set_defaults(func=cmd_folder)
    f = sub.add_parser("fixture", help="generate 12 MP JPEGs")
    f.add_argument("dir")
    f.add_argument("--count", type=int, default=1)
    f.set_defaults(func=cmd_fixture)
    args = ap.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
