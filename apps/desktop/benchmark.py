#!/usr/bin/env python3
"""Measure local JPEG workflows. Originals are read-only; reports omit source paths."""
import argparse
import json
import os
from pathlib import Path
import statistics
import subprocess
import time


def require_display():
    # A locked/asleep display can leave the window key but stop all frame delivery.
    state = subprocess.check_output([
        "swift", "-e",
        'import CoreGraphics; '
        'let s = CGSessionCopyCurrentDictionary() as? [String: Any]; '
        'let locked = (s?["CGSSessionScreenIsLocked"] as? Bool) ?? false; '
        'let ready = s != nil && !locked && CGDisplayIsActive(CGMainDisplayID()) != 0 '
        '&& CGDisplayIsAsleep(CGMainDisplayID()) == 0; '
        'print(ready ? "ready" : "blocked")',
    ], text=True, timeout=30).strip()
    if state != "ready":
        raise RuntimeError("GUI benchmark blocked: unlock the Mac and wake its display first")


parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("folder", type=Path)
parser.add_argument("--count", type=int, default=20)
parser.add_argument("--seconds", type=int, default=30)
parser.add_argument("--output", type=Path, required=True, help="New directory outside Git, or benchmarks/local/")
parser.add_argument("--mode", choices=["gui", "pipeline"], default="gui")
parser.add_argument("--keep-active", action="store_true", help="Keep the GUI window active during each timed run")
args = parser.parse_args()
if args.count < 1 or args.seconds < 10:
    parser.error("count must be positive and seconds at least 10")
root = Path(__file__).resolve().parent
if args.mode == "gui":
    require_display()
args.output.mkdir(parents=True, exist_ok=False)
files = []
for directory, children, names in os.walk(args.folder):
    children[:] = sorted(d for d in children if not d.endswith((".photoslibrary", ".photolibrary")) and d != "Photo Booth Library")
    files.extend(Path(directory) / name for name in sorted(names) if Path(name).suffix.lower() in (".jpg", ".jpeg") and not (Path(directory) / name).is_symlink())
files.sort()
if not files:
    parser.error("No JPEGs found")
count = min(args.count, len(files))
# Sample across the folder tree instead of benchmarking one consecutive burst.
selected = [files[i * len(files) // count] for i in range(count)]
sizes = [path.stat().st_size for path in selected]
metadata = {
    "hardware": subprocess.check_output(["sysctl", "-n", "machdep.cpu.brand_string"], text=True).strip(),
    "ram_bytes": int(subprocess.check_output(["sysctl", "-n", "hw.memsize"], text=True)),
    "os": subprocess.check_output(["sw_vers", "-productVersion"], text=True).strip(),
    "keep_active": args.keep_active,
    "vips": subprocess.check_output(["vips", "--version"], text=True).strip(),
    "photos": count, "input_bytes": sum(sizes), "median_jpeg_bytes": statistics.median(sizes),
    "cache_definition": "cold = empty derivative cache; warm = all thumbnails + exact requested large previews cached, fresh process; OS file cache not flushed",
    "scope": "fixed-duration browsing window; cold import may still be running" if args.mode == "gui" else "complete thumbnail import",
}

def run(name, command, env):
    if args.mode == "gui":
        require_display()
    stdout_path = args.output / (name + ".stdout")
    stderr_path = args.output / (name + ".stderr")
    with stdout_path.open("w") as stdout, stderr_path.open("w") as stderr:
        process = subprocess.Popen(command, stdout=stdout, stderr=stderr, env=env)
        started = time.monotonic()
        peak_rss = 0
        try:
            while process.poll() is None:
                rows = subprocess.check_output(["ps", "-axo", "pid=,ppid=,rss="], text=True)
                processes = [tuple(map(int, row.split())) for row in rows.splitlines() if row.strip()]
                family = {process.pid}
                while True:
                    expanded = family | {pid for pid, parent, _ in processes if parent in family}
                    if expanded == family:
                        break
                    family = expanded
                peak_rss = max(peak_rss, sum(rss * 1024 for pid, _, rss in processes if pid in family))
                deadline = args.seconds + 30 if args.mode == "gui" else max(60, count * 10)
                if time.monotonic() - started > deadline:
                    raise TimeoutError(name)
                time.sleep(0.1)  # Sample app + native encoder child RSS, not just the parent.
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=10)
        if process.returncode:
            raise RuntimeError(f"{name} exited {process.returncode}; inspect {stderr_path}")
    result = json.loads(stdout_path.read_text().splitlines()[-1])
    result["sampled_family_peak_rss_bytes"] = peak_rss or None
    if args.mode == "gui":
        preview_budget = 1500 if name == "cold" else 200
        result["budgets"] = {
            "draw_p95_ms": 8.33, "present_interval_p95_ms": 20,
            "present_interval_p99_ms": 33.34, "preview_ready_p95_ms": preview_budget,
            "sampled_family_peak_rss_bytes": 512 * 1024 * 1024,
            "last_render_age_ms": 250, "max_render_gap_ms": 250,
            "thumbnail_cache_peak_entries": 64, "preview_cache_peak_entries": 2,
            "thumbnail_peak_in_flight": 4, "preview_peak_in_flight": 1,
        }
        result["passed"] = (
            result["photos"] == count and result["failed"] == 0
            and result["draw_samples"] > 100 and result["preview_ready_samples"] >= 3
            and result["preview_attempts"] == result["preview_ready_samples"] == result["preview_decoded_samples"]
            and result["preview_failures"] == 0 and result["decode_failures"] == 0
            and (name == "cold" or result["preview_cache_hits"] == result["preview_attempts"])
            and all(result.get(key) is not None and result[key] <= limit for key, limit in result["budgets"].items())
        )
    print(name, json.dumps(result), flush=True)
    return result

results = {"environment": metadata, "runs": {}}
for pipeline in (["gui"] if args.mode == "gui" else ["rust", "vips"]):
    cache = args.output / (pipeline + "-cache")
    for state in ["cold", "warm"]:
        if pipeline == "gui" and state == "warm":
            # Match Gallery::benchmark's deterministic request times and rows.
            rows = (count + 3) // 4
            indices = sorted({((second * 4) % rows) * 4 for second in range(1, args.seconds - 1, 3)})
            warming_env = dict(os.environ, PHOTO_PREWARM_INDICES=",".join(map(str, indices)))
            warming = subprocess.run(
                [str(root / "target/release/benchmark-pipeline"), "vips", str(cache)] + [str(p.resolve()) for p in selected],
                env=warming_env, check=True, capture_output=True, text=True, timeout=max(120, count * 10),
            )
            results["warm_preparation"] = json.loads(warming.stdout.splitlines()[-1])
            (args.output / "report.json").write_text(json.dumps(results, indent=2) + "\n")
        env = dict(os.environ, PHOTO_CACHE_DIR=str(cache.resolve()), PHOTO_BENCH_SECONDS=str(args.seconds))
        env.pop("PHOTO_BENCH_KEEP_ACTIVE", None)
        if args.keep_active:
            env["PHOTO_BENCH_KEEP_ACTIVE"] = "1"
        command = [str(root / "target/release/photo-desktop")] if pipeline == "gui" else [str(root / "target/release/benchmark-pipeline"), pipeline, str(cache)]
        name = state if pipeline == "gui" else pipeline + "-" + state
        results["runs"][name] = run(name, command + [str(p.resolve()) for p in selected], env)
        (args.output / "report.json").write_text(json.dumps(results, indent=2) + "\n")
if args.mode == "gui" and not all(r["passed"] for r in results["runs"].values()):
    raise SystemExit("Performance gate failed; inspect report.json")
