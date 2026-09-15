//! Run one filter over a sample photograph and write the result out, so that
//! what a filter *looks* like can be checked against the CS6 original rather
//! than only asserted about.
//!
//! The engine has no image decoder — it takes pixels from the shell, which has
//! Qt's — so this speaks raw RGBA and leaves the PNG/JPEG ends to whatever is
//! on hand. See CLAUDE.md §7 for the three commands that wrap it.
//!
//!     cargo run --release --example filter_sample -- \
//!         /tmp/in.raw 597 900 /tmp/out.raw "Dry Brush" 2 8 2
//!
//! The filter is named as the Filter menu names it, and its numbers follow in
//! the order its dialog lists them — the same pair the shell hands across the
//! bridge, so what this runs is what the menu item runs.

use photorust_core::buffer::Pixmap;
use photorust_core::filters::Filter;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 5 {
        eprintln!("usage: filter_sample <in.raw> <width> <height> <out.raw> <filter> [numbers...]");
        std::process::exit(2);
    }
    let (w, h) = (parse(&args[1]) as u32, parse(&args[2]) as u32);
    let name = &args[4];
    let numbers: Vec<f32> = args[5..].iter().map(|a| parse(a)).collect();

    let bytes = std::fs::read(&args[0]).unwrap_or_else(|e| panic!("{}: {e}", args[0]));
    let pixmap = Pixmap::from_raw(w, h, bytes)
        .unwrap_or_else(|| panic!("{w}×{h} does not match the size of {}", args[0]));
    let filter = Filter::from_menu_name(name, &numbers)
        .unwrap_or_else(|| panic!("no filter is called {name:?}"));

    let mut out = pixmap;
    let started = Instant::now();
    filter.apply(&mut out);
    eprintln!("{name} over {w}×{h}: {:?}", started.elapsed());

    std::fs::write(&args[3], out.as_bytes()).unwrap_or_else(|e| panic!("{}: {e}", args[3]));
}

fn parse(s: &str) -> f32 {
    s.parse().unwrap_or_else(|_| panic!("{s:?} is not a number"))
}
