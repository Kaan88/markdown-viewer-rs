//! A stand-in for the editor's live preview, for checking the viewer without it.
//!
//! `live_send FILE TEXT_FILE [LINE]` sends the contents of TEXT_FILE as the live text of FILE to the
//! running mdview and prints how long the send took; `live_send FILE off` ends the live state.
//! `MD_SISTER_MDVIEW_SETTINGS` points at the settings file of a viewer started with `MDVIEW_SETTINGS`.

use mdshared_app::sister::{Request, send};

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(file), Some(what)) = (args.next(), args.next()) else {
        eprintln!("usage: live_send FILE (TEXT_FILE [LINE] | off)");
        std::process::exit(2);
    };
    let mut request = Request::new(file);
    request.from = Some("mdedit".into());
    if what == "off" {
        request.live_off = true;
    } else {
        request.text = Some(std::fs::read_to_string(&what).expect("text file"));
        request.line = args.next().and_then(|l| l.parse().ok());
    }
    let t = std::time::Instant::now();
    let ok = send("mdview", &request);
    println!("sent: {ok} in {:?}", t.elapsed());
    std::process::exit(if ok { 0 } else { 1 });
}
