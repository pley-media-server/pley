//! Checks on the TV binary: what the TV's loader and CPU need, read from the ELF file.

use object::Endianness;
use object::elf::{self, FileHeader32};
use object::read::elf::{Dyn, FileHeader, ProgramHeader, SectionHeader, Sym};

use crate::Result;

/// The only libraries the binary may ask the loader for: glibc's and GCC's unwinder (which
/// Rust's panics use), present under these names on every webOS release. SDL2, EGL and GLES,
/// luna-service2, GLib and LG's media libraries are opened at run time with `dlopen`, so a
/// library missing on one firmware can't stop the app before `main`.
pub const ALLOWED_NEEDED: &[&str] = &[
    "ld-linux.so.3",
    "libc.so.6",
    "libdl.so.2",
    "libgcc_s.so.1",
    "libm.so.6",
    "libpthread.so.0",
    "librt.so.1",
    "libutil.so.1",
];

/// The newest glibc symbol version the binary may need: webOS 4.0, the oldest release Pley
/// supports (LG's 2018 TVs), ships glibc 2.24. The toolchain's
/// sysroot is glibc 2.12, so a newer version can only come from a changed toolchain.
pub const GLIBC_FLOOR: (u32, u32) = (2, 24);

/// libm's functions that compiler-builtins also defines, weakly and without the FPU. A local
/// definition means the link took those instead of glibc's VFP ones, because libm came after
/// the Rust libraries on the link line (`.cargo/config.toml`).
pub const LIBM_NAMES: &[&str] = &[
    "cbrt", "cbrtf", "ceil", "ceilf", "floor", "floorf", "fma", "fmaf", "fmod", "fmodf", "round", "roundf", "sqrt",
    "sqrtf", "trunc", "truncf",
];

/// Prefixes of the symbols of the toolchain's `libglibc_polyfills.a`, which its GCC links into
/// every program unless told not to: its licence isn't known, so it must not ship. The app
/// defines `getauxval` itself (`-tno-glibc-polyfill` in `.cargo/config.toml`).
pub const POLYFILL_PREFIXES: &[&str] = &["_getauxval_", "_glibc_polyfills", "_aligned_alloc_polyfill"];

/// The most the stripped binary may weigh. Raised only in the commit that needs more, saying why.
pub const SIZE_BUDGET: u64 = 16 * 1024 * 1024;

/// The dynamic loader of webOS userland: the soft-float EABI one (a hard-float system's is
/// `ld-linux-armhf.so.3`).
pub const LOADER: &str = "/lib/ld-linux.so.3";

/// `e_flags`'s EABI version field, and version 5 (ARM IHI 0044, "ELF for the Arm Architecture").
const EF_ARM_EABIMASK: u32 = 0xff00_0000;
const EF_ARM_EABI_VER5: u32 = 0x0500_0000;
/// `Tag_CPU_arch` for ARMv7 (ARM IHI 0045, "Addenda to, and errata in, the ABI for the Arm Architecture").
const CPU_ARCH_V7: u64 = 10;
const TAG_FILE: u8 = 1;
const TAG_CPU_ARCH: u64 = 6;
const TAG_ABI_VFP_ARGS: u64 = 28;

/// What the binary is, as the check found it.
#[derive(Debug, Default)]
pub struct Facts {
    /// The libraries in `DT_NEEDED`, in file order.
    pub needed: Vec<String>,
    /// The newest `GLIBC_x.y` version it needs.
    pub newest_glibc: Option<(u32, u32)>,
    /// Its size once stripped: the file without its symbol tables and debug sections.
    pub stripped_size: u64,
}

/// Checks the TV binary: an EABI version 5, soft-float, ARMv7 executable that needs only
/// glibc's libraries at versions webOS 4.0 has, takes libm's functions from glibc, carries
/// none of the toolchain's polyfill, names none of `private_paths` (the build machine's
/// folders, which `--remap-path-prefix` replaces) and fits the size budget. The binary must
/// still have its symbol table: `cargo xtask build` keeps it, and `ipk` strips the copy it packs.
pub fn check(data: &[u8], private_paths: &[String], budget: u64) -> Result<(Facts, Vec<String>)> {
    let header = FileHeader32::<Endianness>::parse(data).map_err(|_| "not a 32-bit ELF file")?;
    let endian = header.endian()?;
    let mut wrong = Vec::new();
    let mut facts = Facts::default();

    if header.e_ident().data != elf::ELFDATA2LSB {
        wrong.push("not little-endian".to_owned());
    }
    if header.e_machine(endian) != elf::EM_ARM {
        wrong.push(format!("machine {} is not ARM ({})", header.e_machine(endian).0, elf::EM_ARM.0));
    }
    let kind = header.e_type(endian);
    if kind != elf::ET_EXEC && kind != elf::ET_DYN {
        wrong.push(format!("type {} is not an executable", kind.0));
    }
    let flags = header.e_flags(endian).0;
    if flags & EF_ARM_EABIMASK != EF_ARM_EABI_VER5 {
        wrong.push(format!("flags {flags:#x}: not EABI version 5"));
    }
    if flags & elf::EF_ARM_ABI_FLOAT_HARD != 0 || flags & elf::EF_ARM_ABI_FLOAT_SOFT == 0 {
        wrong.push(format!("flags {flags:#x}: not the soft-float ABI of webOS userland"));
    }
    let mut interpreter = None;
    for segment in header.program_headers(endian, data)? {
        interpreter = interpreter.or(segment.interpreter(endian, data)?);
    }
    match interpreter {
        None => wrong.push("no program interpreter: not a dynamically linked program".to_owned()),
        Some(path) if path != LOADER.as_bytes() => wrong.push(format!(
            "its loader is {}, not {LOADER} (the soft-float one every webOS TV has)",
            String::from_utf8_lossy(path)
        )),
        Some(_) => {}
    }

    let sections = header.sections(endian, data)?;
    match sections.section_by_name(endian, b".ARM.attributes") {
        None => wrong.push("no .ARM.attributes section".to_owned()),
        Some((_, section)) => {
            let attributes = arm_attributes(section.data(endian, data)?)?;
            if attributes.cpu_arch != Some(CPU_ARCH_V7) {
                wrong.push(format!("Tag_CPU_arch is {:?}, not ARMv7 ({CPU_ARCH_V7})", attributes.cpu_arch));
            }
            if attributes.vfp_args == Some(1) {
                wrong.push("Tag_ABI_VFP_args says floats are passed in VFP registers (hard-float)".to_owned());
            }
        }
    }

    match sections.dynamic(endian, data)? {
        None => wrong.push("no dynamic section: not linked against the TV's glibc".to_owned()),
        Some((entries, link)) => {
            let strings = sections.strings(endian, data, link)?;
            for entry in entries.iter().filter(|e| e.d_tag(endian) == elf::DT_NEEDED) {
                facts.needed.push(String::from_utf8_lossy(entry.string(endian, strings)?).into_owned());
            }
        }
    }
    for name in &facts.needed {
        if !ALLOWED_NEEDED.contains(&name.as_str()) {
            wrong.push(format!("needs {name}: load it with dlopen (only glibc's libraries are linked)"));
        }
    }

    if let Some((mut needs, link)) = sections.gnu_verneed(endian, data)? {
        let strings = sections.strings(endian, data, link)?;
        while let Some((need, mut aux)) = needs.next()? {
            let file = String::from_utf8_lossy(need.file(endian, strings)?).into_owned();
            while let Some(version) = aux.next()? {
                let name = String::from_utf8_lossy(version.name(endian, strings)?).into_owned();
                let Some(number) = name.strip_prefix("GLIBC_").and_then(glibc_version) else { continue };
                facts.newest_glibc = facts.newest_glibc.max(Some(number));
                if number > GLIBC_FLOOR {
                    wrong.push(format!(
                        "needs {name} from {file}: webOS 4.0 has glibc {}.{}",
                        GLIBC_FLOOR.0, GLIBC_FLOOR.1
                    ));
                }
            }
        }
    }

    let symbols = sections.symbols(endian, data, elf::SHT_SYMTAB)?;
    if symbols.is_empty() {
        wrong.push("stripped: the check reads the symbol table (check `cargo xtask build`'s binary)".to_owned());
    } else {
        for symbol in symbols.iter() {
            let name = symbols.symbol_name(endian, symbol).unwrap_or_default();
            let Ok(name) = std::str::from_utf8(name) else { continue };
            let defined = symbol.st_shndx(endian) != elf::SHN_UNDEF;
            if defined && symbol.st_type() == elf::STT_FUNC && LIBM_NAMES.contains(&name) {
                wrong.push(format!(
                    "defines {name} itself: compiler-builtins' soft-float copy won over glibc's libm (put libm first)"
                ));
            }
            if POLYFILL_PREFIXES.iter().any(|p| name.starts_with(p)) {
                wrong.push(format!("carries the toolchain's glibc polyfill ({name}): link with -tno-glibc-polyfill"));
            }
        }
    }

    for path in private_paths.iter().filter(|p| p.len() > 3) {
        if contains(data, path.as_bytes()) {
            wrong.push(format!("names the build machine's {path} (pass --remap-path-prefix)"));
        }
    }

    let mut removable = 0;
    for section in sections.iter() {
        let name = sections.section_name(endian, section).unwrap_or_default();
        let kind = section.sh_type(endian);
        if kind == elf::SHT_SYMTAB || name == b".strtab" || name.starts_with(b".debug_") {
            removable += u64::from(section.sh_size(endian));
        }
    }
    facts.stripped_size = (data.len() as u64).saturating_sub(removable);
    if facts.stripped_size > budget {
        wrong.push(format!("{} bytes once stripped, over the budget of {budget}", facts.stripped_size));
    }
    wrong.dedup();
    Ok((facts, wrong))
}

/// `2.24` as (2, 24); `None` for anything else.
fn glibc_version(text: &str) -> Option<(u32, u32)> {
    let mut parts = text.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().map_or(Some(0), |m| m.parse().ok())?;
    Some((major, minor))
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|w| w == needle)
}

/// The two build attributes the check reads.
#[derive(Debug, Default, PartialEq)]
struct ArmAttributes {
    cpu_arch: Option<u64>,
    vfp_args: Option<u64>,
}

/// Reads the file-wide `aeabi` attributes of a `.ARM.attributes` section: the format byte `A`,
/// then subsections (length, vendor name, then tagged groups of attributes). Tags 4, 5 and 67
/// and the odd tags above 32 hold strings, tag 32 a number and a string, the rest numbers
/// (ULEB128).
fn arm_attributes(data: &[u8]) -> Result<ArmAttributes> {
    let mut found = ArmAttributes::default();
    let Some((&b'A', mut rest)) = data.split_first() else { return Err(".ARM.attributes: not format A".into()) };
    while rest.len() >= 4 {
        let length = u32::from_le_bytes(rest[..4].try_into()?) as usize;
        if length < 4 || length > rest.len() {
            return Err(".ARM.attributes: a subsection overruns the section".into());
        }
        let subsection = &rest[4..length];
        rest = &rest[length..];
        let Some(nul) = subsection.iter().position(|&b| b == 0) else { continue };
        if &subsection[..nul] != b"aeabi" {
            continue;
        }
        let mut groups = &subsection[nul + 1..];
        while groups.len() >= 5 {
            let tag = groups[0];
            let size = u32::from_le_bytes(groups[1..5].try_into()?) as usize;
            if size < 5 || size > groups.len() {
                return Err(".ARM.attributes: a group overruns its subsection".into());
            }
            let mut attributes = &groups[5..size];
            groups = &groups[size..];
            if tag != TAG_FILE {
                continue;
            }
            while !attributes.is_empty() {
                let tag = uleb128(&mut attributes)?;
                let is_string = matches!(tag, 4 | 5 | 67) || (tag > 32 && tag % 2 == 1);
                if tag == 32 {
                    uleb128(&mut attributes)?;
                    skip_string(&mut attributes)?;
                } else if is_string {
                    skip_string(&mut attributes)?;
                } else {
                    let value = uleb128(&mut attributes)?;
                    match tag {
                        TAG_CPU_ARCH => found.cpu_arch = Some(value),
                        TAG_ABI_VFP_ARGS => found.vfp_args = Some(value),
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(found)
}

fn uleb128(data: &mut &[u8]) -> Result<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let (&byte, rest) = data.split_first().ok_or(".ARM.attributes: a number runs off the end")?;
        *data = rest;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Ok(value);
        }
    }
    Err(".ARM.attributes: a number too long".into())
}

fn skip_string(data: &mut &[u8]) -> Result<()> {
    let nul = data.iter().position(|&b| b == 0).ok_or(".ARM.attributes: a string runs off the end")?;
    *data = &data[nul + 1..];
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small ARM ELF executable put together by hand, enough for the checks to read.
    pub struct TestElf {
        pub flags: u32,
        pub machine: u16,
        pub needed: Vec<&'static str>,
        /// (library, versions it needs from it).
        pub verneed: Vec<(&'static str, Vec<&'static str>)>,
        /// (name, defined here, ELF symbol type).
        pub symbols: Vec<(&'static str, bool, elf::SymbolType)>,
        pub attributes: Option<Vec<u8>>,
        pub interpreter: Option<&'static str>,
        pub rodata: Vec<u8>,
        pub debug: usize,
    }

    impl Default for TestElf {
        fn default() -> Self {
            TestElf {
                flags: EF_ARM_EABI_VER5 | elf::EF_ARM_ABI_FLOAT_SOFT,
                machine: elf::EM_ARM.0,
                needed: vec!["libc.so.6", "libm.so.6", "libgcc_s.so.1", "ld-linux.so.3"],
                verneed: vec![
                    ("libc.so.6", vec!["GLIBC_2.4", "GLIBC_2.7"]),
                    ("libm.so.6", vec!["GLIBC_2.4"]),
                    ("libgcc_s.so.1", vec!["GCC_3.5"]),
                ],
                symbols: vec![
                    ("main", true, elf::STT_FUNC),
                    ("roundf", false, elf::STT_FUNC),
                    ("getauxval", true, elf::STT_FUNC),
                ],
                attributes: Some(attributes(&[
                    (5, Value::Text("7-A")),
                    (6, Value::Number(10)),
                    (7, Value::Number(0x41)),
                    (28, Value::Number(0)),
                ])),
                interpreter: Some(LOADER),
                rodata: b"src/main.rs\0/pley/crates/app/src/main.rs\0".to_vec(),
                debug: 0,
            }
        }
    }

    pub enum Value {
        Number(u64),
        Text(&'static str),
    }

    /// An `aeabi` subsection holding one file-wide group.
    pub fn attributes(values: &[(u64, Value)]) -> Vec<u8> {
        let mut body = Vec::new();
        for (tag, value) in values {
            push_uleb(&mut body, *tag);
            match value {
                Value::Number(n) => push_uleb(&mut body, *n),
                Value::Text(t) => {
                    body.extend_from_slice(t.as_bytes());
                    body.push(0);
                }
            }
        }
        let mut group = vec![TAG_FILE];
        group.extend_from_slice(&(5 + body.len() as u32).to_le_bytes());
        group.extend_from_slice(&body);
        let mut subsection = Vec::new();
        subsection.extend_from_slice(&(4 + 6 + group.len() as u32).to_le_bytes());
        subsection.extend_from_slice(b"aeabi\0");
        subsection.extend_from_slice(&group);
        let mut data = vec![b'A'];
        // A vendor's subsection that the reader skips.
        data.extend_from_slice(&(4 + 7 + 3u32).to_le_bytes());
        data.extend_from_slice(b"vendor\0\x01\x02\x03");
        data.extend_from_slice(&subsection);
        data
    }

    fn push_uleb(out: &mut Vec<u8>, mut value: u64) {
        loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            if value == 0 {
                out.push(byte);
                return;
            }
            out.push(byte | 0x80);
        }
    }

    /// One section of a test file: its name, type, bytes, `sh_link`, `sh_info` and entry size.
    struct Section {
        name: &'static str,
        kind: elf::SectionType,
        data: Vec<u8>,
        link: u32,
        info: u32,
        entsize: u32,
    }

    /// A string table being written: its bytes, and each string's offset as it is added.
    struct Strings(Vec<u8>);

    impl Strings {
        fn new() -> Self {
            Strings(vec![0])
        }
        fn add(&mut self, s: &str) -> u32 {
            let at = self.0.len() as u32;
            self.0.extend_from_slice(s.as_bytes());
            self.0.push(0);
            at
        }
    }

    impl TestElf {
        pub fn build(&self) -> Vec<u8> {
            let mut dynstr = Strings::new();
            let mut dynamic = Vec::new();
            for lib in &self.needed {
                let at = dynstr.add(lib);
                dynamic.extend_from_slice(&(elf::DT_NEEDED.0 as u32).to_le_bytes());
                dynamic.extend_from_slice(&at.to_le_bytes());
            }
            dynamic.extend_from_slice(&[0; 8]);
            let mut verneed = Vec::new();
            for (i, (file, versions)) in self.verneed.iter().enumerate() {
                let file_at = dynstr.add(file);
                let last = i + 1 == self.verneed.len();
                let next = if last { 0 } else { 16 + 16 * versions.len() as u32 };
                verneed.extend_from_slice(&1u16.to_le_bytes());
                verneed.extend_from_slice(&(versions.len() as u16).to_le_bytes());
                verneed.extend_from_slice(&file_at.to_le_bytes());
                verneed.extend_from_slice(&16u32.to_le_bytes());
                verneed.extend_from_slice(&next.to_le_bytes());
                for (j, version) in versions.iter().enumerate() {
                    let name_at = dynstr.add(version);
                    let next = if j + 1 == versions.len() { 0u32 } else { 16 };
                    verneed.extend_from_slice(&0u32.to_le_bytes());
                    verneed.extend_from_slice(&0u16.to_le_bytes());
                    verneed.extend_from_slice(&(2 + j as u16).to_le_bytes());
                    verneed.extend_from_slice(&name_at.to_le_bytes());
                    verneed.extend_from_slice(&next.to_le_bytes());
                }
            }
            let mut strtab = Strings::new();
            let mut symtab = vec![0; 16];
            for (name, defined, kind) in &self.symbols {
                let at = strtab.add(name);
                symtab.extend_from_slice(&at.to_le_bytes());
                symtab.extend_from_slice(&0x1000u32.to_le_bytes());
                symtab.extend_from_slice(&4u32.to_le_bytes());
                symtab.push((elf::STB_GLOBAL.0 << 4) | kind.0);
                symtab.push(0);
                symtab.extend_from_slice(&(if *defined { 7u16 } else { 0 }).to_le_bytes());
            }

            let section = |name, kind, data, link, info, entsize| Section { name, kind, data, link, info, entsize };
            let mut sections = vec![
                section(".dynstr", elf::SHT_STRTAB, dynstr.0.clone(), 0, 0, 0),
                section(".dynamic", elf::SHT_DYNAMIC, dynamic, 1, 0, 8),
                section(".gnu.version_r", elf::SHT_GNU_VERNEED, verneed, 1, self.verneed.len() as u32, 0),
                section(".strtab", elf::SHT_STRTAB, strtab.0.clone(), 0, 0, 0),
                section(".symtab", elf::SHT_SYMTAB, symtab, 4, 1, 16),
                section(".rodata", elf::SHT_PROGBITS, self.rodata.clone(), 0, 0, 0),
                section(".text", elf::SHT_PROGBITS, vec![0; 16], 0, 0, 0),
            ];
            if let Some(attributes) = &self.attributes {
                sections.push(section(".ARM.attributes", elf::SHT_ARM_ATTRIBUTES, attributes.clone(), 0, 0, 0));
            }
            if self.debug > 0 {
                sections.push(section(".debug_info", elf::SHT_PROGBITS, vec![0; self.debug], 0, 0, 0));
            }
            let mut shstrtab = Strings::new();
            let mut names: Vec<u32> = sections.iter().map(|s| shstrtab.add(s.name)).collect();
            names.push(shstrtab.add(".shstrtab"));
            sections.push(section(".shstrtab", elf::SHT_STRTAB, shstrtab.0.clone(), 0, 0, 0));

            let mut out = vec![0u8; 52];
            let mut headers = vec![0u8; 40];
            for (s, name) in sections.iter().zip(names) {
                out.resize(out.len().next_multiple_of(4), 0);
                let offset = out.len() as u32;
                out.extend_from_slice(&s.data);
                for value in [name, s.kind.0, 0, 0, offset, s.data.len() as u32, s.link, s.info, 1, s.entsize] {
                    headers.extend_from_slice(&value.to_le_bytes());
                }
            }
            out.resize(out.len().next_multiple_of(4), 0);
            let shoff = out.len() as u32;
            out.extend_from_slice(&headers);
            let phoff = out.len() as u32;
            if let Some(loader) = self.interpreter {
                let size = loader.len() as u32 + 1;
                // PT_INTERP: type, offset, address, physical address, file size, memory size, flags, alignment.
                for value in [elf::PT_INTERP.0, phoff + 32, 0, 0, size, size, 4, 1] {
                    out.extend_from_slice(&value.to_le_bytes());
                }
                out.extend_from_slice(loader.as_bytes());
                out.push(0);
            }

            let header = &mut out[..52];
            if self.interpreter.is_some() {
                header[28..32].copy_from_slice(&phoff.to_le_bytes());
                header[42..44].copy_from_slice(&32u16.to_le_bytes());
                header[44..46].copy_from_slice(&1u16.to_le_bytes());
            }
            header[..4].copy_from_slice(b"\x7fELF");
            header[4] = elf::ELFCLASS32.0;
            header[5] = elf::ELFDATA2LSB.0;
            header[6] = 1;
            header[16..18].copy_from_slice(&elf::ET_DYN.0.to_le_bytes());
            header[18..20].copy_from_slice(&self.machine.to_le_bytes());
            header[20..24].copy_from_slice(&1u32.to_le_bytes());
            header[32..36].copy_from_slice(&shoff.to_le_bytes());
            header[36..40].copy_from_slice(&self.flags.to_le_bytes());
            header[40..42].copy_from_slice(&52u16.to_le_bytes());
            header[46..48].copy_from_slice(&40u16.to_le_bytes());
            header[48..50].copy_from_slice(&(sections.len() as u16 + 1).to_le_bytes());
            header[50..52].copy_from_slice(&(sections.len() as u16).to_le_bytes());
            out
        }
    }

    fn problems(elf: &TestElf) -> Vec<String> {
        check(&elf.build(), &["/home/runner".to_owned()], SIZE_BUDGET).unwrap().1
    }

    #[test]
    fn a_binary_the_tv_can_run_passes() {
        let elf = TestElf { debug: 300, ..TestElf::default() };
        let data = elf.build();
        let (facts, wrong) = check(&data, &["/home/runner".to_owned()], SIZE_BUDGET).unwrap();
        assert_eq!(wrong, Vec::<String>::new());
        assert_eq!(facts.needed, ["libc.so.6", "libm.so.6", "libgcc_s.so.1", "ld-linux.so.3"]);
        assert_eq!(facts.newest_glibc, Some((2, 7)));
        assert!(facts.stripped_size < data.len() as u64 - 300, "symbols and debug info don't count");
    }

    #[test]
    fn wrong_architecture_or_float_abi_fails() {
        let wrong = problems(&TestElf { machine: elf::EM_AARCH64.0, ..TestElf::default() });
        assert!(wrong.iter().any(|w| w.contains("not ARM")), "{wrong:?}");
        let hard = TestElf { flags: EF_ARM_EABI_VER5 | elf::EF_ARM_ABI_FLOAT_HARD, ..TestElf::default() };
        assert!(problems(&hard).iter().any(|w| w.contains("soft-float")));
        let old_abi = TestElf { flags: 0x0400_0000 | elf::EF_ARM_ABI_FLOAT_SOFT, ..TestElf::default() };
        assert!(problems(&old_abi).iter().any(|w| w.contains("EABI version 5")));
        let v8 = TestElf { attributes: Some(attributes(&[(6, Value::Number(14))])), ..TestElf::default() };
        assert!(problems(&v8).iter().any(|w| w.contains("Tag_CPU_arch is Some(14)")));
        let vfp_args = TestElf {
            attributes: Some(attributes(&[(6, Value::Number(10)), (28, Value::Number(1))])),
            ..TestElf::default()
        };
        assert!(problems(&vfp_args).iter().any(|w| w.contains("hard-float")));
        let none = TestElf { attributes: None, ..TestElf::default() };
        assert!(problems(&none).iter().any(|w| w.contains("no .ARM.attributes")));
        let armhf = TestElf { interpreter: Some("/lib/ld-linux-armhf.so.3"), ..TestElf::default() };
        assert_eq!(
            problems(&armhf),
            ["its loader is /lib/ld-linux-armhf.so.3, not /lib/ld-linux.so.3 (the soft-float one every webOS TV has)"]
        );
        let static_program = TestElf { interpreter: None, ..TestElf::default() };
        assert!(problems(&static_program).iter().any(|w| w.starts_with("no program interpreter")));
    }

    #[test]
    fn a_firmware_library_in_dt_needed_fails() {
        let mut elf = TestElf::default();
        elf.needed.push("libSDL2-2.0.so.0");
        let wrong = problems(&elf);
        assert_eq!(wrong.len(), 1, "{wrong:?}");
        assert!(wrong[0].starts_with("needs libSDL2-2.0.so.0"));
    }

    #[test]
    fn glibc_newer_than_webos_4_fails() {
        let mut elf = TestElf::default();
        elf.verneed[0].1.push("GLIBC_2.25");
        elf.verneed[0].1.push("GLIBC_2.24");
        let wrong = problems(&elf);
        assert_eq!(wrong, ["needs GLIBC_2.25 from libc.so.6: webOS 4.0 has glibc 2.24"]);
        assert_eq!(glibc_version("2.24"), Some((2, 24)));
        assert_eq!(glibc_version("2"), Some((2, 0)));
        assert_eq!(glibc_version("PRIVATE"), None);
    }

    #[test]
    fn a_local_libm_function_or_the_polyfill_fails() {
        let mut elf = TestElf::default();
        elf.symbols.push(("floorf", true, elf::STT_FUNC));
        elf.symbols.push(("_getauxval_polyfill", true, elf::STT_FUNC));
        let wrong = problems(&elf);
        assert_eq!(wrong.len(), 2, "{wrong:?}");
        assert!(wrong[0].starts_with("defines floorf itself"));
        assert!(wrong[1].contains("glibc polyfill"));
    }

    #[test]
    fn a_stripped_binary_cannot_be_checked() {
        let elf = TestElf { symbols: Vec::new(), ..TestElf::default() };
        // An empty symbol table still holds the null symbol; a stripped file has none at all.
        let wrong = problems(&elf);
        assert!(wrong.is_empty(), "{wrong:?}");
        let mut data = elf.build();
        // Make .symtab look empty: its size 0.
        let shoff = u32::from_le_bytes(data[32..36].try_into().unwrap()) as usize;
        let symtab = shoff + 40 * 5;
        data[symtab + 20..symtab + 24].copy_from_slice(&0u32.to_le_bytes());
        let (_, wrong) = check(&data, &[], SIZE_BUDGET).unwrap();
        assert!(wrong.iter().any(|w| w.starts_with("stripped")), "{wrong:?}");
    }

    #[test]
    fn build_machine_paths_and_size_fail() {
        let elf = TestElf { rodata: b"/home/runner/.cargo/registry/src/x.rs\0".to_vec(), ..TestElf::default() };
        assert!(problems(&elf).iter().any(|w| w.contains("/home/runner")));
        let data = TestElf::default().build();
        let (_, wrong) = check(&data, &[], 100).unwrap();
        assert!(wrong.iter().any(|w| w.contains("over the budget of 100")), "{wrong:?}");
    }

    #[test]
    fn not_an_elf_file_is_an_error() {
        assert!(check(b"#!/bin/sh\necho stand-in\n", &[], SIZE_BUDGET).is_err());
    }

    #[test]
    fn reads_attributes_with_strings_and_compatibility() {
        let data = attributes(&[
            (4, Value::Text("cortex-a9")),
            (32, Value::Number(1)),
            (6, Value::Number(10)),
            (65, Value::Text("x")),
            (28, Value::Number(0)),
        ]);
        // Tag 32 holds a number then a string: put the string after its number by hand.
        let mut fixed = Vec::new();
        let at = data.iter().position(|&b| b == 32).unwrap();
        fixed.extend_from_slice(&data[..at + 2]);
        fixed.extend_from_slice(b"gnu\0");
        fixed.extend_from_slice(&data[at + 2..]);
        // Grow the subsection and group lengths by the four bytes added.
        let sub = 1 + 4 + 7 + 3;
        let length = u32::from_le_bytes(fixed[sub..sub + 4].try_into().unwrap()) + 4;
        fixed[sub..sub + 4].copy_from_slice(&length.to_le_bytes());
        let group = sub + 4 + 6 + 1;
        let size = u32::from_le_bytes(fixed[group..group + 4].try_into().unwrap()) + 4;
        fixed[group..group + 4].copy_from_slice(&size.to_le_bytes());
        assert_eq!(arm_attributes(&fixed).unwrap(), ArmAttributes { cpu_arch: Some(10), vfp_args: Some(0) });
        assert!(arm_attributes(b"B").is_err());
        assert!(arm_attributes(b"A\xff\x00\x00\x00").is_err());
    }
}
