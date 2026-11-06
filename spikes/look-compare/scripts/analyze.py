#!/usr/bin/python3
"""Summarises results/launch, idle, t0, trace into results/summary.txt. First 2 rounds per app are dropped."""
import glob, json, os, re, statistics as st, sys
R = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "results")
M = lambda v: st.median(v) if v else float("nan")
K = 1024.0
lines = []
def p(*a):
    s = " ".join(str(x) for x in a); lines.append(s); print(s)
def owned(r):
    c = r["cats"]
    anon = c.get("anon", {}).get("Rss", 0)
    shm = c.get("shm_buffers", {}).get("Rss", 0)
    filedirty = sum(v.get("Private_Dirty", 0) for k, v in c.items() if k not in ("anon", "shm_buffers", "stack_vdso"))
    return anon + shm + filedirty
def alone(r):
    return r["roll"]["Rss"]
for D in ("launch", "launch-b", "launch-c"):
  for T in ("light", "dark"):
    for S in ("1.0", "1.5", "2.2"):
        f = f"{R}/{D}/launch-{T}@{S}.jsonl"
        if not os.path.exists(f): continue
        rows = [r for r in map(json.loads, open(f)) if "app" in r]
        p(f"\n## {D} {T}@{S} (12 rounds interleaved, first 2 dropped, n=10)")
        for app in ("qt", "slint"):
            all_ = [r for r in rows if r["app"] == app]
            bad = [r for r in all_ if not r["ok"]]
            k = [r for r in all_ if r["ok"] and r["round"] >= 2]
            v = sorted(r["present_ms"] for r in k)
            p(f"{app}: failed={len(bad)} present median={M(v):.2f} min={v[0]:.2f} max={v[-1]:.2f} first_paint={M([r['first_paint_ms'] for r in k]):.2f} ext_read={M([r['ext_ms'] for r in k]):.2f} raw_all={[round(r['present_ms'],1) for r in all_]}")
            g = lambda key: M([r["roll"][key] / K for r in k])
            rng = lambda key: (min(r["roll"][key] / K for r in k), max(r["roll"][key] / K for r in k))
            uss = [(r["roll"]["Private_Clean"] + r["roll"]["Private_Dirty"]) / K for r in k]
            p(f"   Private_Dirty={g('Private_Dirty'):.2f} {rng('Private_Dirty')[0]:.2f}-{rng('Private_Dirty')[1]:.2f}  Pss={g('Pss'):.2f} {rng('Pss')[0]:.2f}-{rng('Pss')[1]:.2f}  USS(PrivClean+PrivDirty)={M(uss):.2f} {min(uss):.2f}-{max(uss):.2f}  Shared_Dirty={g('Shared_Dirty'):.2f} Rss={g('Rss'):.2f}")
            own = [owned(r) / K for r in k]
            p(f"   owned(anon+shm+file-dirty)={M(own):.2f} {min(own):.2f}-{max(own):.2f}")
            cats = sorted({c for r in k for c in r["cats"]})
            for c in cats:
                vals = lambda key: M([r["cats"].get(c, {}).get(key, 0) / K for r in k])
                p(f"     {c:12s} Rss={vals('Rss'):7.2f} Pss={vals('Pss'):7.2f} PrivClean={vals('Private_Clean'):7.2f} PrivDirty={vals('Private_Dirty'):7.2f} ShClean={vals('Shared_Clean'):7.2f} ShDirty={vals('Shared_Dirty'):7.2f}")
        for app in ("qt", "slint"):
            hf = f"{R}/{D}/harness-{app}-{T}@{S}.txt"
            if os.path.exists(hf):
                t = open(hf).read()
                g = lambda n: re.search(rf"^{n}: median_after_drop=([\d.]+)", t, re.M)
                p(f"harness-check {app}: present={g('present_ms').group(1)} Private_Dirty={g('Private_Dirty_MiB').group(1)} Pss={g('Pss_MiB').group(1)} PrivateTotal={g('PrivateTotal_MiB').group(1)}")
# t0
f = f"{R}/t0/t0-light@1.0.jsonl"
if os.path.exists(f):
    rows = [json.loads(l) for l in open(f)]
    p("\n## T0 checks at light@1.0 (7 runs per row, first dropped, median)")
    for app in ("qt", "slint"):
        for kind in ("normal", "sleep100_before_exec", "no_env"):
            k = [r for r in rows if r["app"] == app and r["kind"] == kind and r["i"] >= 1 and r["ok"]]
            p(f"{app} {kind}: present={M([r['present_ms'] for r in k]):.2f} ext_read_from_pipe={M([r['ext_ms'] for r in k]):.2f} kernel_start_minus_t0={M([r['kernel_start_minus_t0_ms'] for r in k]):.2f} t0_src={k[0]['t0_src']} raw={[round(r['present_ms'],1) for r in k]}")
# idle
for S in ("1.0", "1.5", "2.2"):
    f = f"{R}/idle/idle-light@{S}.jsonl"
    if not os.path.exists(f): continue
    rows = [json.loads(l) for l in open(f)]
    p(f"\n## idle light@{S}, 30 s window starting 5 s after present")
    for app in ("qt", "slint"):
        for extra in (False, True):
            k = [r for r in rows if r["app"] == app and bool(r["extra"]) == extra]
            if not k: continue
            p(f"{app}{' find-open' if extra else ''}: n={len(k)} cpu_ms={[round(r['cpu_ms'],2) for r in k]} pct={[round(r['cpu_ms']/r['wall_s']/10,4) for r in k]} jiffies={[r['jiffies'] for r in k]} wakeups={[r['vol'] for r in k]} invol={[r['invol'] for r in k]} threads={k[0]['threads0']}->{k[0]['threads1']} exited={[r['exited'] for r in k]}")
            r = k[0]
            for tag, m in (("0.8s", r["r_08"]), ("5s", r["r0"]), ("35s", r["r1"])):
                p(f"     t={tag}: Private_Dirty={m['Private_Dirty']/K:.2f} Private_Clean={m['Private_Clean']/K:.2f} Shared_Dirty={m['Shared_Dirty']/K:.2f} Pss={m['Pss']/K:.2f} Rss={m['Rss']/K:.2f}")
            c = r["cats1"]
            p("     t=35s shm_Rss=%.2f anon_Rss=%.2f" % (c.get("shm_buffers", {}).get("Rss", 0)/K, c.get("anon", {}).get("Rss", 0)/K))
# traces
p("\n## Wayland traces (WAYLAND_DEBUG inflates times; order and counts only)")
for f in sorted(glob.glob(f"{R}/trace/trace-*.log")):
    n = os.path.basename(f)
    commits = buf = 0; pres = None; done_before = None; first_full = None; sizes = set()
    last_done = None
    for l in open(f, errors="replace"):
        if l.startswith("PRESENTED"):
            pres = float(re.search(r"present_ms=([\d.]+)", l).group(1)); done_before = last_done; pres_commits = commits; pres_bufs = buf; break
        if ".commit()" in l and "wl_surface" in l: commits += 1
        m = re.search(r"create_buffer\(new id wl_buffer#\d+, \d+, (\d+), (\d+),", l)
        if m:
            buf += 1; sizes.add(m.group(1) + "x" + m.group(2))
        if re.search(r"wl_callback#\d+\.done\(\d{5,}\)", l): last_done = l.split("]")[0][1:]
    p(f"{n}: present={pres} commits_before={commits} buffers_before={buf} sizes={sorted(sizes)} frame_done_just_before={done_before}")
open(f"{R}/summary.txt", "w").write("\n".join(lines) + "\n")
