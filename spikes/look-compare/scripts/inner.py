#!/usr/bin/python3
"""Runs inside a nested KWin. Args: MODE SCALE THEME OUTDIR.
T0 is CLOCK_MONOTONIC just before spawn, passed in MDV_T0_NS, the same as qt-shell's harness."""
import json, os, re, signal, socket, struct, subprocess, sys, tempfile, time
MODE, S_, T, OUT = sys.argv[1:5]
S = float(S_)
HERE = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
QT = f"{HERE}/target/qt/release/look-qt"
SL = f"{HERE}/target/slint/release/look-slint"
TOOLS = f"{HERE}/../qt-shell/tools/target/release"
APPS = {
  "qt":    dict(bin=QT, base=[f"--theme={T}"], stay=lambda ms: [f"--stay-ms={ms}"], find=["--find-open"], scroll=lambda n: [f"--scroll={n}"]),
  "slint": dict(bin=SL, base=["--no-kde", f"--theme={T}"], stay=lambda ms: [f"--stay-ms={ms}"], find=["--find"], scroll=lambda n: [f"--scroll-px={n}"]),
}
ENV = dict(os.environ, QT_QPA_PLATFORM="wayland")
mono = lambda: time.clock_gettime_ns(time.CLOCK_MONOTONIC)
boot = lambda: time.clock_gettime_ns(time.CLOCK_BOOTTIME)
TICK = os.sysconf("SC_CLK_TCK")

def field(line, key):
    m = re.search(rf"\b{key}=([^ ]+)", line)
    return m.group(1) if m else None

def rollup(pid):
    d = {}
    for l in open(f"/proc/{pid}/smaps_rollup"):
        m = re.match(r"(\w+):\s+(\d+) kB", l)
        if m: d[m.group(1)] = int(m.group(2))
    return d

def category(path, binp):
    if not path: return "anon"
    if path == "[heap]": return "anon"
    if path.startswith("["): return "stack_vdso"
    if path == binp: return "own_binary"
    if "memfd:" in path or path.startswith(("/dev/shm", "/SYSV", "/dev/zero")): return "shm_buffers"
    if path.endswith(".bgra"): return "page_file"
    if re.search(r"/(libQt6|libKF6|libKirigami|libkdecor|libbreeze)|/qt6/plugins/|/qt6/", path): return "qt_kde_libs"
    if ".so" in path: return "other_libs"
    if re.search(r"\.(ttf|ttc|otf)$", path): return "fonts"
    if path.endswith(("(deleted)",)) and ("wayland" in path or "/tmp" in path or "shm" in path): return "shm_buffers"
    return "other_files"

def smaps_cats(pid, binp):
    cats, cur, top = {}, None, []
    for l in open(f"/proc/{pid}/smaps"):
        m = re.match(r"[0-9a-f]+-[0-9a-f]+ \S+ \S+ \S+ \S+\s*(.*)$", l)
        if m:
            cur = cats.setdefault(category(m.group(1).strip(), binp), {})
            name = m.group(1).strip()
            continue
        m = re.match(r"(Rss|Pss|Private_Clean|Private_Dirty|Shared_Clean|Shared_Dirty):\s+(\d+) kB", l)
        if m and cur is not None:
            cur[m.group(1)] = cur.get(m.group(1), 0) + int(m.group(2))
    return cats

def libs(pid):
    s = set()
    for l in open(f"/proc/{pid}/maps"):
        p = l.split(None, 5)[5].strip() if len(l.split(None, 5)) > 5 else ""
        if p.startswith("/") and (".so" in p or p.endswith((".ttf", ".ttc", ".otf"))): s.add(p)
    return sorted(s)

def one_launch(app, wrapper=None, send_t0=True):
    a = APPS[app]
    cmd = [a["bin"]] + a["base"] + ["--measure"] + (a["stay"](1200) if app == "qt" else [])
    if wrapper: cmd = wrapper + cmd
    env = dict(ENV); env.pop("MDV_T0_NS", None)
    t0, b0 = mono(), boot()
    if send_t0: env["MDV_T0_NS"] = str(t0)
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=env, text=True, bufsize=1)
    line = p.stdout.readline()
    t1 = mono()
    r = dict(app=app, ok=line.startswith("PRESENTED"), line=line.strip())
    if r["ok"]:
        r["present_ms"] = float(field(line, "present_ms")); r["first_paint_ms"] = float(field(line, "first_paint_ms"))
        r["t0_src"] = field(line, "t0"); r["ext_ms"] = (t1 - t0) / 1e6
        st = open(f"/proc/{p.pid}/stat").read().rsplit(")", 1)[1].split()
        r["kernel_start_minus_t0_ms"] = int(st[19]) * 1000 / TICK - b0 / 1e6
        time.sleep(0.8)
        r["roll"] = rollup(p.pid); r["cats"] = smaps_cats(p.pid, a["bin"]); r["libs"] = libs(p.pid)
    for _ in range(3000):
        if p.poll() is not None: break
        time.sleep(0.005)
    else:
        p.kill(); r["killed"] = True
    p.stdout.close(); p.wait()
    return r

def task_stats(pid):
    """Per thread, so a thread that exits between snapshots can't make a delta negative."""
    t = {}
    for tid in os.listdir(f"/proc/{pid}/task"):
        try:
            run = int(open(f"/proc/{pid}/task/{tid}/schedstat").read().split()[0])
            s = open(f"/proc/{pid}/task/{tid}/status").read()
            t[tid] = (run, int(re.search(r"^voluntary_ctxt_switches:\s+(\d+)", s, re.M).group(1)),
                      int(re.search(r"^nonvoluntary_ctxt_switches:\s+(\d+)", s, re.M).group(1)))
        except (FileNotFoundError, ProcessLookupError):
            pass
    st = open(f"/proc/{pid}/stat").read().rsplit(")", 1)[1].split()
    return t, int(st[11]) + int(st[12])

def idle(app, extra=()):
    a = APPS[app]
    p = subprocess.Popen([a["bin"]] + a["base"] + list(extra) + a["stay"](45000),
                         stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, env=ENV, text=True, bufsize=1)
    p.stdout.readline(); pid = p.pid
    time.sleep(0.8); r_08 = rollup(pid)
    time.sleep(4.2)
    r0 = rollup(pid); c0 = smaps_cats(pid, a["bin"]); (t0s, j0) = task_stats(pid); t0 = time.monotonic()
    time.sleep(30)
    (t1s, j1) = task_stats(pid); t1 = time.monotonic(); r1 = rollup(pid); c1 = smaps_cats(pid, a["bin"])
    p.terminate(); p.wait(); p.stdout.close()
    d = [tuple(x - y for x, y in zip(t1s[k], t0s.get(k, (0, 0, 0)))) for k in t1s]
    per = {k: t1s[k][0] - t0s.get(k, (0, 0, 0))[0] for k in t1s}
    return dict(app=app, extra=list(extra), wall_s=t1 - t0, cpu_ms=sum(x[0] for x in d) / 1e6, vol=sum(x[1] for x in d), invol=sum(x[2] for x in d),
                jiffies=j1 - j0, threads0=len(t0s), threads1=len(t1s), exited=len(set(t0s) - set(t1s)), per_thread_us={k: v // 1000 for k, v in per.items()},
                r_08=r_08, r0=r0, r1=r1, cats0=c0, cats1=c1)

def kwin_move(x, y):
    js = f"""var all = workspace.windowList();
for (var i = 0; i < all.length; i++) {{ var w = all[i];
  if (w.caption.indexOf("mdview") >= 0) {{ var g = w.frameGeometry;
    w.frameGeometry = {{x: {x}, y: {y}, width: g.width, height: g.height}}; workspace.activeWindow = w; }} }}"""
    with tempfile.NamedTemporaryFile("w", suffix=".js", delete=False) as f: f.write(js)
    q = lambda *a: subprocess.run(["qdbus6", "org.kde.KWin", *a], capture_output=True, text=True).stdout.strip()
    sid = q("/Scripting", "org.kde.kwin.Scripting.loadScript", f.name, f"m{time.monotonic_ns()}")
    q(f"/Scripting/Script{sid}", "org.kde.kwin.Script.run")
    time.sleep(0.5); os.unlink(f.name)

class Fake:
    """Real compositor input through org_kde_kwin_fake_input. From look-qt/scripts/input_session.py."""
    def __init__(self):
        self.s = socket.socket(socket.AF_UNIX)
        self.s.connect(os.path.join(os.environ["XDG_RUNTIME_DIR"], os.environ["WAYLAND_DISPLAY"]))
        self.send(1, 1, struct.pack("<I", 2)); self.send(1, 0, struct.pack("<I", 3))
        name, version, buf, done = None, 0, b"", False
        while not done:
            buf += self.s.recv(65536)
            while len(buf) >= 8:
                oid, so = struct.unpack("<II", buf[:8]); size, op = so >> 16, so & 0xffff
                if len(buf) < size: break
                body, buf = buf[8:size], buf[size:]
                if oid == 2 and op == 0:
                    n, slen = struct.unpack("<II", body[:8]); iface = body[8:8 + slen - 1].decode()
                    if iface == "org_kde_kwin_fake_input":
                        name = n; version = struct.unpack("<I", body[8 + (slen + 3) // 4 * 4:][:4])[0]
                if oid == 3: done = True
        self.send(2, 0, struct.pack("<I", name) + self.string("org_kde_kwin_fake_input") + struct.pack("<II", version, 4))
        self.send(4, 0, self.string("look-compare") + self.string("input"))
    @staticmethod
    def string(t):
        b = t.encode() + b"\0"
        return struct.pack("<I", len(b)) + b + b"\0" * ((4 - len(b) % 4) % 4)
    def send(self, oid, op, body=b""): self.s.sendall(struct.pack("<II", oid, ((8 + len(body)) << 16) | op) + body)
    def move(self, x, y): self.send(4, 9, struct.pack("<ii", int(x * 256), int(y * 256)))
    def click(self):
        self.send(4, 2, struct.pack("<II", 272, 1)); time.sleep(0.05); self.send(4, 2, struct.pack("<II", 272, 0))

def capture(path): subprocess.run([f"{TOOLS}/kwin_capture", path], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

def crop(full, out):
    x, y, w, h = int(24 * S), int(24 * S), int(1252 * S), int(888 * S)
    subprocess.run(["magick", full, "-crop", f"{w}x{h}+{x}+{y}", "+repage", out], check=True)

def start(app, args, log, trace=False):
    a = APPS[app]
    env = dict(ENV)
    if trace: env["WAYLAND_DEBUG"] = "client"
    lf = open(log, "w")
    p = subprocess.Popen(["timeout", "25", a["bin"]] + a["base"] + list(args), stdout=lf, stderr=lf, env=env)
    lf.close()
    for _ in range(200):
        if "PRESENTED" in open(log, errors="replace").read(): break
        time.sleep(0.05)
    time.sleep(0.3)
    return p

def stop(p):
    p.terminate(); p.wait(); time.sleep(0.25)

def mode_measure():
    out = open(f"{OUT}/launch-{T}@{S_}.jsonl", "w")
    out.write(json.dumps(dict(load=open("/proc/loadavg").read(), top=subprocess.run("ps -eo pcpu,comm --sort=-pcpu | head -6", shell=True, capture_output=True, text=True).stdout)) + "\n")
    for i in range(12):
        order = ["qt", "slint"] if i % 2 == 0 else ["slint", "qt"]
        for app in order:
            r = one_launch(app); r["round"] = i; out.write(json.dumps(r) + "\n"); out.flush()
    # Cross-check with the existing harness, not interleaved.
    for app in ("qt", "slint"):
        a = APPS[app]
        cmd = [f"{TOOLS}/harness", "--runs", "12", "--drop", "2", "--smaps-after-ms", "800", "--", a["bin"]] + a["base"] + ["--measure"] + (a["stay"](1200) if app == "qt" else [])
        h = subprocess.run(cmd, capture_output=True, text=True, env=ENV, timeout=60)
        open(f"{OUT}/harness-{app}-{T}@{S_}.txt", "w").write(h.stdout)

def mode_t0():
    out = open(f"{OUT}/t0-{T}@{S_}.jsonl", "w")
    for app in ("qt", "slint"):
        for kind, kw in (("normal", {}), ("sleep100_before_exec", dict(wrapper=["sh", "-c", 'sleep 0.1; exec "$@"', "sh"])), ("no_env", dict(send_t0=False))):
            for i in range(7):
                r = one_launch(app, **kw); r.pop("cats", None); r.pop("libs", None); r.pop("roll", None)
                r["kind"] = kind; r["i"] = i; out.write(json.dumps(r) + "\n"); out.flush()

def mode_idle():
    out = open(f"{OUT}/idle-{T}@{S_}.jsonl", "w")
    for i in range(3):
        for app in (("qt", "slint") if i % 2 == 0 else ("slint", "qt")):
            r = idle(app); r["i"] = i; out.write(json.dumps(r) + "\n"); out.flush()
    if S_ == "1.0":
        for app in ("qt", "slint"):
            r = idle(app, APPS[app]["find"]); r["i"] = 0; out.write(json.dumps(r) + "\n"); out.flush()

def mode_trace():
    for app in ("qt", "slint"):
        for i in range(3):
            a = APPS[app]
            env = dict(ENV, WAYLAND_DEBUG="client"); log = f"{OUT}/trace-{app}-{T}@{S_}-{i}.log"
            t0 = mono(); env["MDV_T0_NS"] = str(t0)
            with open(log, "w") as lf:
                subprocess.run([a["bin"]] + a["base"] + ["--measure"] + (a["stay"](1200) if app == "qt" else []), stdout=lf, stderr=lf, env=env, timeout=20)

def mode_shots():
    for app in ("qt", "slint"):
        for name, args in (("plain", []), ("find", APPS[app]["find"]), ("scrolled", APPS[app]["scroll"](900))):
            log = f"{OUT}/.{app}-{T}@{S_}-{name}.log"
            p = start(app, list(args) + APPS[app]["stay"](9000), log)
            kwin_move(40, 40); time.sleep(1.0)
            full = f"{OUT}/.{app}-{T}@{S_}-{name}.full.png"
            capture(full); crop(full, f"{OUT}/{app}-{T}@{S_}-{name}.png")
            if name in ("plain", "scrolled"):
                # An odd logical position lands on a fractional device origin at 1.5 and 2.2.
                kwin_move(41, 43); time.sleep(1.0)
                capture(f"{OUT}/.{app}-{T}@{S_}-{name}-odd.full.png")
            stop(p)

def mode_menu():
    """Real pointer: open the app menu, with a Wayland trace to see what kind of surface it is."""
    for app in ("qt", "slint"):
        log = f"{OUT}/.{app}-{T}@{S_}-menu.log"
        p = start(app, APPS[app]["stay"](14000), log, trace=True)
        kwin_move(40, 40); time.sleep(0.4)
        fi = Fake(); time.sleep(0.3)
        fi.move(60, 90); time.sleep(0.4); fi.click(); time.sleep(0.8)
        full = f"{OUT}/.{app}-{T}@{S_}-menu.full.png"; capture(full); crop(full, f"{OUT}/{app}-{T}@{S_}-menu.png")
        for i in range(8):
            fi.move(90 + i, 126); time.sleep(0.03)
        time.sleep(1.0)
        full = f"{OUT}/.{app}-{T}@{S_}-menu2.full.png"; capture(full); crop(full, f"{OUT}/{app}-{T}@{S_}-menu2.png")
        stop(p)

def mode_ab():
    tag = os.environ.get("AB_TAG", "x")
    for app in ("qt", "slint"):
        a = APPS[app]
        cmd = [f"{TOOLS}/harness", "--runs", "12", "--drop", "2", "--smaps-after-ms", "800", "--", a["bin"]] + a["base"] + ["--measure"] + (a["stay"](1200) if app == "qt" else [])
        h = subprocess.run(cmd, capture_output=True, text=True, env=ENV, timeout=60)
        open(f"{OUT}/ab-{tag}-{app}.txt", "w").write(h.stdout)

{"ab": mode_ab, "measure": mode_measure, "t0": mode_t0, "idle": mode_idle, "trace": mode_trace, "shots": mode_shots, "menu": mode_menu}[MODE]()
