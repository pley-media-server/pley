//! Pley for LG TV: the `pley` binary, which runs the app (pley-host) with every screen.
//!
//! On a computer (the default `desktop` feature) it is the simulator: a window playing the TV's
//! part with a fake device, and `--fixtures` for the fake Pley server. On the TV (the `tv`
//! feature) the window, the device and the media pipeline are `pley-webos`'s.

#[cfg(all(feature = "tv", target_os = "linux"))]
mod auxv;
mod screens;

fn main() {
    pley_host::main(pley_host::App { version: env!("CARGO_PKG_VERSION"), screens: screens::registry });
}
