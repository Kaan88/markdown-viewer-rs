#!/usr/bin/python3
"""Runs inside a nested KWin: drives the real app with real compositor input, through
KWin's org_kde_kwin_fake_input (a raw Wayland client, no dependencies), and captures.
Args: SCALE THEME OUTDIR."""
import os, socket, struct, subprocess, sys, tempfile, time

S, T, OUT = float(sys.argv[1]), sys.argv[2], sys.argv[3]
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = f"{ROOT}/target/release/look-qt"
CAP = f"{ROOT}/../qt-shell/tools/target/release/kwin_capture"

class Fake:
    def __init__(self):
        self.s = socket.socket(socket.AF_UNIX)
        self.s.connect(os.path.join(os.environ["XDG_RUNTIME_DIR"], os.environ["WAYLAND_DISPLAY"]))
        self.send(1, 1, struct.pack("<I", 2))  # wl_display.get_registry -> id 2
        self.send(1, 0, struct.pack("<I", 3))  # wl_display.sync -> id 3
        name, version, buf, done = None, 0, b"", False
        while not done:
            buf += self.s.recv(65536)
            while len(buf) >= 8:
                oid, so = struct.unpack("<II", buf[:8])
                size, op = so >> 16, so & 0xffff
                if len(buf) < size:
                    break
                body, buf = buf[8:size], buf[size:]
                if oid == 2 and op == 0:
                    n, slen = struct.unpack("<II", body[:8])
                    iface = body[8:8 + slen - 1].decode()
                    if iface == "org_kde_kwin_fake_input":
                        name = n
                        version = struct.unpack("<I", body[8 + (slen + 3) // 4 * 4:][:4])[0]
                if oid == 3:
                    done = True
        assert name is not None, "no org_kde_kwin_fake_input global"
        self.version = version
        self.send(2, 0, struct.pack("<I", name) + self.string("org_kde_kwin_fake_input") + struct.pack("<II", version, 4))
        self.send(4, 0, self.string("look-qt") + self.string("spike input test"))
    @staticmethod
    def string(t):
        b = t.encode() + b"\0"
        return struct.pack("<I", len(b)) + b + b"\0" * ((4 - len(b) % 4) % 4)
    def send(self, oid, op, body=b""):
        self.s.sendall(struct.pack("<II", oid, ((8 + len(body)) << 16) | op) + body)
    def move(self, x, y):
        self.send(4, 9, struct.pack("<ii", int(x * 256), int(y * 256)))
    def button(self, b, state):
        self.send(4, 2, struct.pack("<II", b, state))
    def click(self):
        self.button(272, 1); time.sleep(0.05); self.button(272, 0)
    def axis(self, value):
        self.send(4, 3, struct.pack("<Ii", 0, int(value * 256)))
    def key(self, code, state):
        self.send(4, 10, struct.pack("<II", code, state))
    def tap(self, *codes):
        for c in codes:
            self.key(c, 1)
        time.sleep(0.05)
        for c in reversed(codes):
            self.key(c, 0)

def qdbus(*a):
    return subprocess.run(["qdbus6", "org.kde.KWin", *a], capture_output=True, text=True).stdout.strip()

def move_window():
    js = """var all = workspace.windowList();
for (var i = 0; i < all.length; i++) { var w = all[i];
  if (w.caption.indexOf("mdview") >= 0) { var g = w.frameGeometry;
    w.frameGeometry = {x: 40, y: 40, width: g.width, height: g.height}; workspace.activeWindow = w; } }"""
    with tempfile.NamedTemporaryFile("w", suffix=".js", delete=False) as f:
        f.write(js)
    sid = qdbus("/Scripting", "org.kde.kwin.Scripting.loadScript", f.name, "lookmove")
    qdbus(f"/Scripting/Script{sid}", "org.kde.kwin.Script.run")
    time.sleep(0.5)

def shot(name):
    full = f"{OUT}/.{T}@{S}-{name}.full.png"
    subprocess.run([CAP, full], check=True, stdout=subprocess.DEVNULL)
    x, y, w, h = int(24 * S), int(24 * S), int(1252 * S), int(888 * S)
    subprocess.run(["magick", full, "-crop", f"{w}x{h}+{x}+{y}", "+repage", f"{OUT}/{T}@{S}-{name}.png"], check=True)

targ = [] if T == "system" else [f"--theme={T}"]
log = f"{OUT}/.{T}@{S}-input.log"
env = dict(os.environ, QT_QPA_PLATFORM="wayland")
with open(log, "w") as lf:
    app = subprocess.Popen(["timeout", "30", BIN, *targ, "--trace"], stdout=lf, stderr=lf, env=env)
for _ in range(100):
    if "PRESENTED" in open(log).read():
        break
    time.sleep(0.05)
time.sleep(0.3)
move_window()
fi = Fake()
time.sleep(0.3)
KEY_ESC, KEY_LEFTCTRL, KEY_F = 1, 29, 33
# Positions are logical screen coordinates; the window's frame is at (40, 40).
fi.move(700, 500); time.sleep(0.3)
fi.tap(KEY_LEFTCTRL, KEY_F); time.sleep(0.5); shot("ctrl-f-open")
fi.tap(KEY_ESC); time.sleep(0.4); shot("esc-closed")
fi.tap(KEY_LEFTCTRL, KEY_F); time.sleep(0.3); fi.tap(KEY_LEFTCTRL, KEY_F); time.sleep(0.4)
fi.move(134, 298); time.sleep(0.5); shot("hover-tree")
fi.move(244, 88); time.sleep(0.5); shot("hover-tab")
fi.move(60, 88); time.sleep(0.4); fi.click(); time.sleep(0.6); shot("menu")
# QMenu ignores a pointer until it has moved a few times.
for i in range(8):
    fi.move(90 + i, 124); time.sleep(0.03)
time.sleep(1.0); shot("menu-file")
fi.tap(KEY_ESC); time.sleep(0.2); fi.tap(KEY_ESC); time.sleep(0.4)
fi.move(700, 500); time.sleep(0.3); fi.axis(45); time.sleep(0.5); shot("wheel")
fi.move(244, 88); time.sleep(0.3); fi.click(); time.sleep(0.5); shot("tab-2")
app.terminate(); app.wait()
