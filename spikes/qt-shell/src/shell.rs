//! Shell skeleton and spike logic. Safe Rust only.
#![forbid(unsafe_code)]

use crate::qt::*;
use crate::qt::ffi::*;
use std::cell::{Cell, RefCell};
use std::io::Write;
use std::rc::Rc;

#[derive(Clone)]
struct Opts {
    measure: bool,
    exit_after_present: bool,
    stay_ms: i32,
    self_test: bool,
    direct: bool,
    naive: bool,
    win: (i32, i32),
    dock_width: i32,
    left_margin: i32,
    top_margin: i32,
}

fn parse(args: &[String]) -> Opts {
    let mut o = Opts {
        measure: false,
        exit_after_present: false,
        stay_ms: 0,
        self_test: false,
        direct: false,
        naive: false,
        win: (1220, 820),
        dock_width: 200,
        left_margin: 0,
        top_margin: 0,
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut val = || it.next().expect("option value").clone();
        match a.as_str() {
            "--measure" => o.measure = true,
            "--exit-after-present" => o.exit_after_present = true,
            "--self-test" => o.self_test = true,
            "--stay-ms" => o.stay_ms = val().parse().unwrap(),
            "--paint-mode" => {
                let m = val();
                o.direct = m == "direct";
                o.naive = m == "naive";
            }
            "--dock-width" => o.dock_width = val().parse().unwrap(),
            "--left-margin" => o.left_margin = val().parse().unwrap(),
            "--top-margin" => o.top_margin = val().parse().unwrap(),
            "--win" => {
                let v = val();
                let (w, h) = v.split_once('x').unwrap();
                o.win = (w.parse().unwrap(), h.parse().unwrap());
            }
            _ => {}
        }
    }
    o
}

/// Process start: harness-supplied monotonic timestamp if present, else /proc ticks.
fn process_start_ns() -> (u64, &'static str) {
    if let Some(v) = std::env::var("MDV_T0_NS").ok().and_then(|v| v.parse().ok()) {
        return (v, "harness-spawn");
    }
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let after = stat.rsplit(')').next().unwrap_or("");
    let ticks: u64 = after.split_whitespace().nth(19).and_then(|s| s.parse().ok()).unwrap_or(0);
    (ticks * 10_000_000, "proc-starttime-10ms")
}

fn ms_since(t0: u64) -> f64 {
    (monotonic_ns().saturating_sub(t0)) as f64 / 1e6
}

fn out(line: &str) {
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{line}");
    let _ = o.flush();
}

#[derive(Default)]
struct Hits {
    paint: Cell<u32>,
    press: Cell<Option<(f64, f64)>>,
    moved: Cell<Option<(f64, f64)>>,
    wheel: Cell<Option<(i32, i32)>>,
    enter: RefCell<Option<String>>,
    dropped: RefCell<Option<String>>,
    action: Cell<bool>,
    text: RefCell<Option<String>>,
}

struct Shell {
    opts: Opts,
    t0: u64,
    t0_src: &'static str,
    hits: Hits,
    buf: RefCell<(Vec<u8>, i32, i32)>,
    first_paint_ms: Cell<f64>,
    canvas: Cell<Option<H<Canvas>>>,
    win: Cell<Option<H<QMainWindow>>>,
    action: Cell<Option<H<QAction>>>,
    find: Cell<Option<H<QLineEdit>>>,
    post_paint_timer: Cell<i32>,
    stay_timer: Cell<i32>,
    presented: Cell<bool>,
}

fn checkerboard(w: i32, h: i32) -> Vec<u8> {
    let mut v = vec![0u8; (w * h * 4) as usize];
    for (i, px) in v.chunks_exact_mut(4).enumerate() {
        let (x, y) = (i as i32 % w, i as i32 / w);
        let c = if (x + y) % 2 == 0 { 0xff } else { 0x00 };
        px.copy_from_slice(&[c, c, c, 0xff]);
    }
    v
}

impl Shell {
    fn self_test(&self) {
        let win = self.win.get().unwrap();
        let canvas = self.canvas.get().unwrap();
        let qw = win.widget().window_handle().unwrap();
        let (x, y) = canvas.widget().center_in_window();
        qw.send_mouse(EV_MOUSE_PRESS, x, y);
        qw.send_mouse(EV_MOUSE_MOVE, x + 3.0, y + 4.0);
        qw.send_mouse(EV_MOUSE_RELEASE, x + 3.0, y + 4.0);
        qw.send_wheel(x, y, -240);
        let (a, b) = qw.send_drag_and_drop(x, y, "file:///tmp/a.md");
        self.action.get().unwrap().trigger();
        self.find.get().unwrap().set_text("needle");
        let h = &self.hits;
        let checks: Vec<(&str, bool, String)> = vec![
            ("paintEvent", h.paint.get() > 0, format!("{}", h.paint.get())),
            ("mousePressEvent", h.press.get().is_some(), format!("{:?}", h.press.get())),
            ("mouseMoveEvent", h.moved.get().is_some(), format!("{:?}", h.moved.get())),
            ("wheelEvent", h.wheel.get() == Some((0, -240)), format!("{:?}", h.wheel.get())),
            ("dragEnterEvent", h.enter.borrow().as_deref() == Some("file:///tmp/a.md"), format!("{:?} sent={a}", h.enter.borrow())),
            ("dropEvent", h.dropped.borrow().as_deref() == Some("file:///tmp/a.md"), format!("{:?} sent={b}", h.dropped.borrow())),
            ("QAction::triggered", h.action.get(), String::new()),
            ("QLineEdit::textChanged", h.text.borrow().as_deref() == Some("needle"), format!("{:?}", h.text.borrow())),
        ];
        let mut ok = true;
        for (name, pass, detail) in checks {
            ok &= pass;
            out(&format!("SELFTEST {} {name} {detail}", if pass { "PASS" } else { "FAIL" }));
        }
        out(&format!("SELFTEST {}", if ok { "ALL-PASS" } else { "SOME-FAILED" }));
    }

    fn on_presented(&self) {
        self.presented.set(true);
        let canvas = self.canvas.get().unwrap();
        out(&format!(
            "PRESENTED present_ms={:.2} first_paint_ms={:.2} t0={} {}",
            ms_since(self.t0),
            self.first_paint_ms.get(),
            self.t0_src,
            canvas.describe(),
        ));
        if self.opts.self_test {
            self.self_test();
        }
        if self.opts.stay_ms > 0 {
            self.stay_timer.set(canvas.start_timer(self.opts.stay_ms));
        } else if self.opts.exit_after_present {
            fast_exit(0);
        }
    }
}

impl CanvasHandler for Shell {
    fn paint(&self, t: &mut PaintTarget<'_>) {
        if self.hits.paint.get() == 0 {
            self.first_paint_ms.set(ms_since(self.t0));
        }
        self.hits.paint.set(self.hits.paint.get() + 1);
        let (lw, lh) = t.logical_size();
        let dpr = t.dpr();
        let (dw, dh) = ((lw as f64 * dpr).ceil() as i32 + 2, (lh as f64 * dpr).ceil() as i32 + 2);
        let mut b = self.buf.borrow_mut();
        if b.1 != dw || b.2 != dh {
            *b = (checkerboard(dw, dh), dw, dh);
        }
        if self.opts.direct {
            if !t.blit_direct(&b.0, b.1, b.2) {
                out("DIRECT-BLIT-UNAVAILABLE");
            }
        } else {
            t.blit_painter(&b.0, b.1, b.2, !self.opts.naive);
        }
        // Ask for the frame callback once this paint has been committed: a zero
        // timer runs after the backing store flush in the same event loop turn.
        if self.hits.paint.get() == 1 {
            let c = self.canvas.get().unwrap();
            self.post_paint_timer.set(c.start_timer(0));
        }
    }
    fn mouse_press(&self, x: f64, y: f64) {
        self.hits.press.set(Some((x, y)));
    }
    fn mouse_move(&self, x: f64, y: f64) {
        self.hits.moved.set(Some((x, y)));
    }
    fn wheel(&self, dx: i32, dy: i32) {
        self.hits.wheel.set(Some((dx, dy)));
    }
    fn drag_enter(&self, text: String) -> bool {
        *self.hits.enter.borrow_mut() = Some(text);
        true
    }
    fn dropped(&self, text: String) {
        *self.hits.dropped.borrow_mut() = Some(text);
    }
    fn timer(&self, id: i32) {
        let c = self.canvas.get().unwrap();
        c.kill_timer(id);
        if id == self.post_paint_timer.get() {
            let qw = self.win.get().unwrap().widget().window_handle().unwrap();
            qw.request_update();
        } else if id == self.stay_timer.get() {
            fast_exit(0);
        }
    }
    fn update_request(&self) {
        if self.hits.paint.get() > 0 && !self.presented.get() {
            self.on_presented();
        }
    }
}

fn build(shell: &Rc<Shell>) {
    let o = &shell.opts;
    let win = H::<QMainWindow>::new();
    win.widget().set_title("mdview spike");
    win.widget().resize(o.win.0, o.win.1);
    shell.win.set(Some(win));

    let menus = win.menu_bar();
    let file = menus.add_menu("&File");
    let open = file.add_action("&Open...");
    file.add_action("&Quit");
    menus.add_menu("&View").add_action("&Zoom in");
    menus.add_menu("&Help").add_action("&About");
    let s = shell.clone();
    open.on_triggered(move |_| s.hits.action.set(true));
    shell.action.set(Some(open));

    let bar = win.add_tool_bar("Find");
    let find = H::<QLineEdit>::new();
    bar.add_widget(find.widget());
    let s = shell.clone();
    find.on_text_changed(move |t| *s.hits.text.borrow_mut() = Some(t));
    shell.find.set(Some(find));

    let canvas = H::<Canvas>::new(shell.clone() as Rc<dyn CanvasHandler>);
    canvas.widget().accept_drops();
    canvas.widget().track_mouse();
    shell.canvas.set(Some(canvas));
    let page = new_plain_widget();
    layout_with_margins(page, canvas.widget(), o.left_margin, o.top_margin);
    let tabs = H::<QTabWidget>::new();
    tabs.add_tab(page, "showcase.md");
    tabs.add_tab(new_plain_widget(), "linked.md");
    win.set_central(tabs.widget());

    let left = H::<QDockWidget>::new("Folder", win);
    let tree = H::<QTreeWidget>::new();
    let root = tree.add_top("fixtures");
    root.add_child("showcase.md");
    root.add_child("linked.md");
    tree.widget().fix_width(o.dock_width);
    left.set_widget(tree.widget());
    win.add_dock(true, left);

    let right = H::<QDockWidget>::new("Outline", win);
    let outline = H::<QTreeWidget>::new();
    outline.add_top("Heading");
    outline.widget().fix_width(160);
    right.set_widget(outline.widget());
    win.add_dock(false, right);

    win.status_bar().message("Ready");
    win.widget().show();
    let qw = win.widget().window_handle().expect("window handle after show");
    canvas.watch(qw);
}

pub fn main_with_args(args: Vec<String>) -> i32 {
    // Second-launch probe in the directly linked binary: everything Qt has already
    // been loaded by ld.so when this line runs.
    if let Some(i) = args.iter().position(|a| a == "--second-launch-probe") {
        return second_launch(&args[i + 1]);
    }
    let opts = parse(&args);
    let (t0, t0_src) = process_start_ns();
    let shell = Rc::new(Shell {
        opts,
        t0,
        t0_src,
        hits: Hits::default(),
        buf: RefCell::new((Vec::new(), 0, 0)),
        first_paint_ms: Cell::new(0.0),
        canvas: Cell::new(None),
        win: Cell::new(None),
        action: Cell::new(None),
        find: Cell::new(None),
        post_paint_timer: Cell::new(-1),
        stay_timer: Cell::new(-1),
        presented: Cell::new(false),
    });
    run_app(|| build(&shell))
}

fn second_launch(name: &str) -> i32 {
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixStream};
    let Ok(addr) = SocketAddr::from_abstract_name(name) else { return 2 };
    match UnixStream::connect_addr(&addr) {
        Ok(mut s) => {
            let _ = s.write_all(b"open\n");
            0
        }
        Err(_) => 3,
    }
}
