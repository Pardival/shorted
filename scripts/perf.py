#!/usr/bin/env python3
"""Runs the benchmarks and prints a table against the NFR-PERF budgets.

  scripts/perf.py                      engine benches, decode, app measurement on this machine
  scripts/perf.py --engine-only        without the app (for CI)
  scripts/perf.py --baseline old.json  also fails on a regression above 10 %

Writes the results to --out (default perf-report.json).
"""
import argparse
import json
import platform
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ENGINE = ROOT / "engine"
APP = ROOT / "app"
CLIPS = ENGINE / "fixtures" / "media"
CONTAINER = Path.home() / (
    "Library/Containers/app.shorted.shorted/Data/Library/Application Support/app.shorted.shorted"
)
REGRESSION_LIMIT = 0.10


def run(command, cwd):
    return subprocess.run(command, cwd=cwd, check=True, capture_output=True, text=True).stdout


def engine_metrics():
    run(["cargo", "bench", "-p", "shorted_benches"], ENGINE)
    estimates = ENGINE / "target/criterion/render_1080p_scene_pixel_copy/new/estimates.json"
    if not estimates.exists():
        return {}
    return {"render_ms": json.loads(estimates.read_text())["mean"]["point_estimate"] / 1e6}


def decode_metrics():
    clip = CLIPS / "h264_1080p60.mov"
    if platform.system() != "Darwin":
        return {}
    if not clip.exists():
        run(["swift", "scripts/make_fixtures.swift", str(CLIPS)], ROOT)
    run(["cargo", "build", "--release", "-p", "shorted_media", "--example", "decode_spike"], ENGINE)
    output = run([str(ENGINE / "target/release/examples/decode_spike"), str(clip)], ROOT)
    report = json.loads(output)
    return {"decode_fps": report["decodeFps"], "seek_ms_p95": report["seekMsP95"]}


def app_metrics():
    if platform.system() != "Darwin":
        return {}
    run(["flutter", "build", "macos", "--profile", "--dart-define=SHORTED_SPIKE=true"], APP)
    binary = APP / "build/macos/Build/Products/Profile/shorted.app/Contents/MacOS/shorted"
    CONTAINER.mkdir(parents=True, exist_ok=True)
    for name in ("h264_1080p60.mov", "h264_1080p60_rotated.mov"):
        if (CLIPS / name).exists():
            shutil.copy(CLIPS / name, CONTAINER / name)
    subprocess.run([str(binary)], check=True, capture_output=True, timeout=300)
    report = json.loads((CONTAINER / "spike-report.json").read_text())
    results = {result["mode"]: result for result in report["results"]}
    video = results.get("zeroCopyVideo") or results["zeroCopy"]
    frames = video["uiBuildMs"]["count"] or 1
    return {
        "preview_fps": video["texturePresentedFps"],
        "preview_has_video": "zeroCopyVideo" in results,
        "ui_frames_over_budget_pct": 100 * video["uiFramesOverBudget"] / frames,
    }


def budget_rows(metrics):
    """(ID, metric, budget on the main phone, value, verdict). The verdict needs a reference device."""
    fps = metrics.get("preview_fps")
    seek = metrics.get("seek_ms_p95")
    jank = metrics.get("ui_frames_over_budget_pct")
    measured = [
        ("NFR-PERF-03", "Preview fps", "≥ 30, target 60", fps, fps is not None and fps >= 30),
        ("NFR-PERF-04", "Decoder seek p95 ms (not full scrub)", "≤ 80", seek, seek is not None and seek <= 80),
        ("NFR-PERF-05", "UI frames over 16 ms, %", "0", jank, jank == 0),
    ]
    missing = [
        ("NFR-PERF-01", "Cold start", "≤ 2.0 s"),
        ("NFR-PERF-02", "Open project", "≤ 1.0 s"),
        ("NFR-PERF-07", "Export 60 s", "≤ 60 s"),
    ]
    rows = []
    for identifier, name, budget, value, passed in measured:
        if value is None:
            rows.append((identifier, name, budget, "not measured", "—"))
        else:
            rows.append((identifier, name, budget, f"{value:.2f}", "ok" if passed else "OVER"))
    rows += [(identifier, name, budget, "not available yet", "—") for identifier, name, budget in missing]
    return rows


def regressions(metrics, baseline):
    found = []
    for key, lower_is_better in (("render_ms", True), ("seek_ms_p95", True), ("decode_fps", False)):
        if key not in metrics or key not in baseline:
            continue
        change = (metrics[key] - baseline[key]) / baseline[key]
        if (change if lower_is_better else -change) > REGRESSION_LIMIT:
            found.append(f"{key}: {baseline[key]:.2f} -> {metrics[key]:.2f} ({change:+.0%})")
    return found


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawTextHelpFormatter)
    parser.add_argument("--engine-only", action="store_true")
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--out", type=Path, default=Path("perf-report.json"))
    args = parser.parse_args()

    metrics = {**engine_metrics(), **decode_metrics()}
    if not args.engine_only:
        metrics.update(app_metrics())
    args.out.write_text(json.dumps(metrics, indent=2))

    print(f"Machine: {platform.platform()}. Not a reference device unless stated by the operator.")
    print(f"{'ID':<12} {'Metric':<38} {'Budget':<16} {'Value':<18} Verdict")
    for row in budget_rows(metrics):
        print(f"{row[0]:<12} {row[1]:<38} {row[2]:<16} {row[3]:<18} {row[4]}")
    print()
    print("Engine:", json.dumps({k: round(v, 2) for k, v in metrics.items() if isinstance(v, float)}))

    if args.baseline and args.baseline.exists():
        found = regressions(metrics, json.loads(args.baseline.read_text()))
        for line in found:
            print("REGRESSION", line)
        if found:
            sys.exit(1)


if __name__ == "__main__":
    main()
