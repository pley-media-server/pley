//! Firmware libraries opened at run time.
//!
//! A `DT_NEEDED` entry the firmware can't satisfy kills the process before
//! `main`, and a strong reference to a symbol a firmware lacks kills it at the
//! first call (RES/plx-build.md §5). So nothing of the TV's is linked: each
//! library is opened by the first of its SONAME candidates that loads, its
//! functions are looked up by name, and a table is all or nothing. Handles are
//! never closed: unmapping a media library with live threads unmaps running
//! code.

use std::mem::ManuallyDrop;

/// A library opened by SONAME, kept open for the life of the process.
pub struct Lib {
    /// The candidate that opened.
    pub name: &'static str,
    lib: ManuallyDrop<libloading::Library>,
}

impl Lib {
    /// Opens the first candidate that loads, resolving every symbol now and
    /// making them visible to the libraries opened after it (`RTLD_NOW |
    /// RTLD_GLOBAL`: the TV's libraries satisfy each other's imports).
    pub fn open(candidates: &[&'static str]) -> Option<Lib> {
        candidates.iter().find_map(|&name| {
            // SAFETY: opening runs the library's initialisers; the candidates are
            // the firmware's own system libraries.
            let lib = unsafe { open_global(name) }.ok()?;
            Some(Lib { name, lib: ManuallyDrop::new(lib) })
        })
    }

    /// The copy of a library another library already loaded, without loading
    /// a second one (`RTLD_NOLOAD`); then [`Lib::open`] when none is loaded.
    pub fn open_loaded(candidates: &[&'static str]) -> Option<Lib> {
        let loaded = candidates.iter().find_map(|&name| {
            // SAFETY: as in `open`; a library already loaded runs no initialiser.
            let lib = unsafe { open_no_load(name) }.ok()?;
            Some(Lib { name, lib: ManuallyDrop::new(lib) })
        });
        loaded.or_else(|| Lib::open(candidates))
    }

    /// The address of `symbol` as a value of `T`, a function pointer type.
    ///
    /// # Safety
    ///
    /// `T` must be the symbol's real type (signature and calling convention).
    pub unsafe fn sym<T: Copy>(&self, symbol: &str) -> Option<T> {
        // SAFETY: the caller vouches for `T`; the library stays open for ever,
        // so the pointer never dangles.
        unsafe { self.lib.get::<T>(symbol.as_bytes()).ok().map(|s| *s) }
    }
}

#[cfg(unix)]
unsafe fn open_global(name: &str) -> Result<libloading::Library, libloading::Error> {
    use libloading::os::unix::{Library, RTLD_GLOBAL, RTLD_NOW};
    // SAFETY: forwarded from the caller.
    unsafe { Library::open(Some(name), RTLD_NOW | RTLD_GLOBAL) }.map(Into::into)
}

#[cfg(unix)]
unsafe fn open_no_load(name: &str) -> Result<libloading::Library, libloading::Error> {
    use libloading::os::unix::{Library, RTLD_GLOBAL, RTLD_NOW};
    // SAFETY: forwarded from the caller.
    unsafe { Library::open(Some(name), RTLD_NOW | RTLD_GLOBAL | libc::RTLD_NOLOAD) }.map(Into::into)
}

// Off the TV (the simulator and tests on Windows) there is nothing of LG's to
// open; the plain loader keeps the code paths the same.
#[cfg(not(unix))]
unsafe fn open_global(name: &str) -> Result<libloading::Library, libloading::Error> {
    // SAFETY: forwarded from the caller.
    unsafe { libloading::Library::new(name) }
}

#[cfg(not(unix))]
unsafe fn open_no_load(name: &str) -> Result<libloading::Library, libloading::Error> {
    // SAFETY: forwarded from the caller.
    unsafe { libloading::Library::new(name) }
}

/// Declares a table of C functions resolved from one [`Lib`], all or nothing:
/// `load` gives the table, or the names that were missing.
macro_rules! fn_table {
    ($(#[$meta:meta])* $vis:vis struct $name:ident { $($field:ident: $ty:ty,)* }) => {
        $(#[$meta])*
        #[allow(non_snake_case)]
        #[derive(Clone, Copy)]
        $vis struct $name {
            $(pub $field: $ty,)*
        }

        impl $name {
            /// Resolves every function, or names the ones the library lacks.
            #[allow(non_snake_case)]
            pub fn load(lib: &$crate::dl::Lib) -> Result<$name, Vec<&'static str>> {
                let mut missing = Vec::new();
                $(
                    // SAFETY: the field's type is the function's C signature.
                    let $field = unsafe { lib.sym::<$ty>(stringify!($field)) };
                    if $field.is_none() {
                        missing.push(stringify!($field));
                    }
                )*
                match ($($field,)*) {
                    ($(Some($field),)*) => Ok($name { $($field,)* }),
                    _ => Err(missing),
                }
            }
        }
    };
}

pub(crate) use fn_table;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_library_opens_nothing() {
        assert!(Lib::open(&["libpley-no-such-library.so.9"]).is_none());
        assert!(Lib::open_loaded(&["libpley-no-such-library.so.9"]).is_none());
    }
}
