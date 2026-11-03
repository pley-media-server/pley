//! The simulator with this crate's screens alone; other pages show the shell's placeholder:
//! `cargo run -p pley-home --example home_sim -- --fixtures --route home` (`--help` lists the options).

fn main() {
    pley_host::main(pley_host::App {
        version: env!("CARGO_PKG_VERSION"),
        screens: || {
            let mut screens = pley_shell::Registry::new();
            screens.add(pley_home::screen);
            screens
        },
    });
}
