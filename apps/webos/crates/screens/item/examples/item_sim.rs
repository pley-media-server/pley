//! The simulator with this crate's screens alone; other pages show the shell's placeholder:
//! `cargo run -p pley-item --example item_sim -- --fixtures --route item:<id>` (`--help` lists the options).

fn main() {
    pley_host::main(pley_host::App {
        version: env!("CARGO_PKG_VERSION"),
        screens: || {
            let mut screens = pley_shell::Registry::new();
            screens.add(pley_item::screen);
            screens
        },
    });
}
