#!/usr/bin/python3
"""Per-run round robin, for small differences. T0 is CLOCK_MONOTONIC just before spawn,
passed in MDV_T0_NS. Memory comes from the app's own --measure line (800 ms after present).
Usage: scripts/measure_rr.py ROUNDS"""
import collections, os, re, statistics as st, subprocess, sys, time
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = f"{ROOT}/target/release/look-qt"
VARS = {"qss=none": ["--qss=none"], "qss=widgets": [], "qss=app": ["--qss=app"],
        "find-open": ["--find-open"], "find-open-noshadow": ["--find-open", "--no-shadow"]}
rounds = int(sys.argv[1])
res = collections.defaultdict(list)
env = dict(os.environ, QT_QPA_PLATFORM="wayland")
for r in range(rounds):
    for name, args in VARS.items():
        env["MDV_T0_NS"] = str(time.clock_gettime_ns(time.CLOCK_MONOTONIC))
        o = subprocess.run(["timeout", "15", BIN, "--measure", *args], env=env, capture_output=True, text=True).stdout
        p = float(re.search(r"present_ms=([\d.]+)", o).group(1))
        d = re.search(r"dpr=([\d.]+)", o).group(1)
        w = re.search(r"window_logical=(\S+)", o).group(1)
        m = float(re.search(r"private_total_mib=([\d.]+)", o).group(1))
        res[name].append((p, m, d, w))
print(f"rounds={rounds} load_end={open('/proc/loadavg').read().split()[:3]}")
for name, v in res.items():
    ps = [x[0] for x in v][2:]  # drop first 2
    ms = sorted(x[1] for x in v)
    print(f"{name}: present_ms median={st.median(ps):.2f} p10={sorted(ps)[len(ps)//10]:.2f} p90={sorted(ps)[len(ps)*9//10]:.2f} n={len(ps)} "
          f"private_mib median={st.median(ms):.2f} min={ms[0]:.2f} max={ms[-1]:.2f} dpr={dict(collections.Counter(x[2] for x in v))} win={dict(collections.Counter(x[3] for x in v))}")
    print("  raw_present=" + " ".join(f"{x[0]:.1f}" for x in v))
    print("  raw_private=" + " ".join(f"{x[1]:.1f}" for x in v))
