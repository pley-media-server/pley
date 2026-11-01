//! `getauxval`, which Rust's standard library calls to learn what the CPU can do, and which the
//! TV toolchain's glibc 2.12 doesn't have (it came in 2.16). It reads the kernel's auxiliary
//! vector from `/proc/self/auxv` without allocating, as it may be called before anything else.
//! Being in the program, it is the one every library of the process uses, the TV's included.

use std::ffi::c_ulong;

/// Entries read at most: a process's vector has a few dozen.
const MAX_WORDS: usize = 256;

/// The value of `kind` in the auxiliary vector; 0 with `errno` set to `ENOENT` when it isn't
/// there, as glibc's.
///
/// # Safety
///
/// Callable from any thread at any time, like glibc's; `unsafe` only because it is exported.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getauxval(kind: c_ulong) -> c_ulong {
    let mut words = [0 as c_ulong; MAX_WORDS];
    let mut filled = 0;
    // SAFETY: plain system calls on a path and a buffer of our own; `read` writes within the
    // buffer's remaining bytes.
    unsafe {
        let fd = libc::open(c"/proc/self/auxv".as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC);
        if fd >= 0 {
            let size = size_of_val(&words);
            while filled < size {
                let n = libc::read(fd, words.as_mut_ptr().cast::<u8>().add(filled).cast(), size - filled);
                if n <= 0 {
                    break;
                }
                filled += n as usize;
            }
            libc::close(fd);
        }
    }
    let whole = filled / size_of::<c_ulong>() / 2 * 2;
    for pair in words[..whole].chunks_exact(2) {
        match pair[0] {
            // AT_NULL ends the vector.
            0 => break,
            k if k == kind => return pair[1],
            _ => {}
        }
    }
    // SAFETY: errno is this thread's.
    unsafe { *libc::__errno_location() = libc::ENOENT };
    0
}
