# Minimal Wayland client (no libraries): sends real seat input through KWin's fake-input global.
# Usage: fakeinput.py CMD...   where CMD is move:X:Y  btn:left|right:down|up  click:left|right
#        key:CODE:down|up  tap:CODE  axis:VALUE  sleep:MS
import os, socket, struct, sys, time

sock = socket.socket(socket.AF_UNIX)
sock.connect(os.path.join(os.environ["XDG_RUNTIME_DIR"], os.environ["WAYLAND_DISPLAY"]))

def send(obj, op, payload=b""):
    sock.sendall(struct.pack("<IHH", obj, op, 8 + len(payload)) + payload)

def wstr(s):
    b = s.encode() + b"\0"
    return struct.pack("<I", len(b)) + b + b"\0" * (-len(b) % 4)

def fixed(v):
    return struct.pack("<i", int(round(v * 256)))

REG, CB = 2, 3
send(1, 1, struct.pack("<I", REG))      # wl_display.get_registry
send(1, 0, struct.pack("<I", CB))       # wl_display.sync
buf, name, version, done = b"", None, 0, False
while not done:
    buf += sock.recv(65536)
    while len(buf) >= 8:
        obj, oc = struct.unpack("<II", buf[:8])
        op, size = oc & 0xffff, oc >> 16
        if len(buf) < size:
            break
        body, buf = buf[8:size], buf[size:]
        if obj == REG and op == 0:
            n, ln = struct.unpack("<II", body[:8])
            iface = body[8:8 + ln - 1].decode()
            if iface == "org_kde_kwin_fake_input":
                name, version = n, struct.unpack("<I", body[8 + ((ln + 3) & ~3):][:4])[0]
        elif obj == CB:
            done = True
if name is None:
    sys.exit("org_kde_kwin_fake_input not offered to this client")
FI = 4
send(REG, 0, struct.pack("<I", name) + wstr("org_kde_kwin_fake_input") + struct.pack("<II", min(version, 6), FI))
send(FI, 0, wstr("mdview spike") + wstr("test input"))   # authenticate
OP_BUTTON, OP_AXIS, OP_ABS, OP_KEY = 2, 3, 9, 10
BTN = {"left": 0x110, "right": 0x111}
ST = {"down": 1, "up": 0}
def flush():
    time.sleep(0.04)
for c in sys.argv[1:]:
    p = c.split(":")
    if p[0] == "move":
        send(FI, OP_ABS, fixed(float(p[1])) + fixed(float(p[2])))
    elif p[0] == "btn":
        send(FI, OP_BUTTON, struct.pack("<II", BTN[p[1]], ST[p[2]]))
    elif p[0] == "click":
        send(FI, OP_BUTTON, struct.pack("<II", BTN[p[1]], 1)); flush()
        send(FI, OP_BUTTON, struct.pack("<II", BTN[p[1]], 0))
    elif p[0] == "key":
        send(FI, OP_KEY, struct.pack("<II", int(p[1]), ST[p[2]]))
    elif p[0] == "tap":
        send(FI, OP_KEY, struct.pack("<II", int(p[1]), 1)); flush()
        send(FI, OP_KEY, struct.pack("<II", int(p[1]), 0))
    elif p[0] == "axis":
        send(FI, OP_AXIS, struct.pack("<I", 0) + fixed(float(p[1])))
    elif p[0] == "sleep":
        time.sleep(int(p[1]) / 1000)
    flush()
# Round trip so everything is handled before exit.
send(1, 0, struct.pack("<I", 5))
sock.settimeout(2)
try:
    sock.recv(4096)
except OSError:
    pass
