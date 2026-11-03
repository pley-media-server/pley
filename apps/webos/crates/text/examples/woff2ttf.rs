//! Converts WOFF 1 fonts back to the TrueType/OpenType files they were made
//! from: how `assets/fonts` gets its Roboto.
//!
//! ```text
//! cargo run -p pley-text --example woff2ttf                      # Roboto into assets/fonts
//! cargo run -p pley-text --example woff2ttf -- --from <files dir> # the same, from another copy
//! cargo run -p pley-text --example woff2ttf -- in.woff out.ttf    # one file
//! ```
//!
//! Without a file, it looks for `@fontsource/roboto` in the `node_modules` of
//! this checkout or a folder above it, converts the Latin and Latin Extended
//! subsets of the three weights the design uses (400, 500, 700) and writes the
//! package's licence as `OFL.txt` beside them.
//!
//! A WOFF 1 file is an sfnt whose tables are each zlib-compressed (or stored
//! when that saves nothing), behind a header of its own. Converting means
//! inflating every table, checking it against the checksum the WOFF kept, and
//! writing the tables out again behind an sfnt table directory.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use rustybuzz::ttf_parser;

/// The design's weights (`pley_base::text::Weight`).
const WEIGHTS: [u16; 3] = [400, 500, 700];
/// Google Fonts' Latin subsets: Latin and Latin Extended.
const SUBSETS: [&str; 2] = ["latin", "latin-ext"];

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [input, output] if !input.starts_with("--") => convert_file(Path::new(input), Path::new(output)),
        [flag, dir] if flag == "--from" => convert_roboto(Path::new(dir)),
        [] => match find_fontsource() {
            Some(dir) => convert_roboto(&dir),
            None => Err("no node_modules/@fontsource/roboto/files found here or above; pass --from <dir>".into()),
        },
        _ => Err("usage: woff2ttf [in.woff out.ttf | --from <@fontsource/roboto/files>]".into()),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("woff2ttf: {e}");
            ExitCode::FAILURE
        }
    }
}

/// `@fontsource/roboto/files` in the nearest `node_modules` at or above this crate.
fn find_fontsource() -> Option<PathBuf> {
    let start = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    start
        .ancestors()
        .map(|dir| dir.join("node_modules").join("@fontsource").join("roboto").join("files"))
        .find(|dir| dir.is_dir())
}

fn assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/fonts")
}

fn convert_roboto(files: &Path) -> Result<(), String> {
    let out = assets_dir();
    std::fs::create_dir_all(&out).map_err(|e| format!("{}: {e}", out.display()))?;
    println!("from {}", files.display());
    for weight in WEIGHTS {
        for subset in SUBSETS {
            let input = files.join(format!("roboto-{subset}-{weight}-normal.woff"));
            let output = out.join(format!("roboto-{subset}-{weight}.ttf"));
            convert_file(&input, &output)?;
        }
    }
    // The package's LICENSE is the OFL with Roboto's copyright line on top.
    let licence = files.parent().map(|p| p.join("LICENSE")).ok_or("no package folder")?;
    let text = std::fs::read_to_string(&licence).map_err(|e| format!("{}: {e}", licence.display()))?;
    if !text.contains("SIL OPEN FONT LICENSE Version 1.1") {
        return Err(format!("{} is not the OFL", licence.display()));
    }
    let ofl = out.join("OFL.txt");
    std::fs::write(&ofl, text.replace("\r\n", "\n")).map_err(|e| format!("{}: {e}", ofl.display()))?;
    println!("wrote {}", ofl.display());
    Ok(())
}

fn convert_file(input: &Path, output: &Path) -> Result<(), String> {
    let woff = std::fs::read(input).map_err(|e| format!("{}: {e}", input.display()))?;
    let sfnt = woff_to_sfnt(&woff).map_err(|e| format!("{}: {e}", input.display()))?;
    let face = ttf_parser::Face::parse(&sfnt, 0)
        .map_err(|e| format!("{}: not a font after converting: {e}", input.display()))?;
    let family = face
        .names()
        .into_iter()
        .find(|n| n.name_id == ttf_parser::name_id::FAMILY && n.is_unicode())
        .and_then(|n| n.to_string())
        .unwrap_or_default();
    std::fs::write(output, &sfnt).map_err(|e| format!("{}: {e}", output.display()))?;
    println!(
        "wrote {} ({family}, weight {}, {} glyphs, {} bytes)",
        output.display(),
        face.weight().to_number(),
        face.number_of_glyphs(),
        sfnt.len()
    );
    Ok(())
}

struct TableEntry {
    tag: [u8; 4],
    offset: usize,
    comp_length: usize,
    orig_length: usize,
    checksum: u32,
}

fn be16(b: &[u8], at: usize) -> Result<u16, String> {
    b.get(at..at + 2).map(|s| u16::from_be_bytes([s[0], s[1]])).ok_or_else(|| "truncated".to_string())
}

fn be32(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]])).ok_or_else(|| "truncated".to_string())
}

/// The sfnt checksum: the sum of the data as big-endian 32-bit words, the last
/// one padded with zeros.
fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

fn woff_to_sfnt(woff: &[u8]) -> Result<Vec<u8>, String> {
    const HEADER: usize = 44;
    const ENTRY: usize = 20;
    if be32(woff, 0)? != u32::from_be_bytes(*b"wOFF") {
        return Err("not a WOFF 1 file".into());
    }
    let flavor = be32(woff, 4)?;
    if be32(woff, 8)? as usize != woff.len() {
        return Err("the header's length is not the file's".into());
    }
    let num_tables = be16(woff, 12)? as usize;
    let total_sfnt_size = be32(woff, 16)? as usize;
    if num_tables == 0 || HEADER + num_tables * ENTRY > woff.len() {
        return Err("bad table count".into());
    }

    let mut entries = Vec::with_capacity(num_tables);
    for i in 0..num_tables {
        let at = HEADER + i * ENTRY;
        let mut tag = [0u8; 4];
        tag.copy_from_slice(&woff[at..at + 4]);
        let entry = TableEntry {
            tag,
            offset: be32(woff, at + 4)? as usize,
            comp_length: be32(woff, at + 8)? as usize,
            orig_length: be32(woff, at + 12)? as usize,
            checksum: be32(woff, at + 16)?,
        };
        if entry.comp_length > entry.orig_length || entry.offset + entry.comp_length > woff.len() {
            return Err(format!("table {} is out of bounds", String::from_utf8_lossy(&tag)));
        }
        entries.push(entry);
    }

    // Inflate every table and check it against the checksum the original font had.
    let mut tables = Vec::with_capacity(num_tables);
    for e in &entries {
        let name = String::from_utf8_lossy(&e.tag).into_owned();
        let stored = &woff[e.offset..e.offset + e.comp_length];
        let data = if e.comp_length < e.orig_length {
            miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(stored, e.orig_length)
                .map_err(|err| format!("table {name} does not inflate: {err:?}"))?
        } else {
            stored.to_vec()
        };
        if data.len() != e.orig_length {
            return Err(format!("table {name} inflates to {} bytes, not {}", data.len(), e.orig_length));
        }
        // The head table's checksum is taken with its checkSumAdjustment at zero.
        let sum = if &e.tag == b"head" && data.len() >= 12 {
            let mut head = data.clone();
            head[8..12].fill(0);
            checksum(&head)
        } else {
            checksum(&data)
        };
        if sum != e.checksum {
            return Err(format!("table {name}'s checksum is {sum:#010x}, the WOFF says {:#010x}", e.checksum));
        }
        tables.push(data);
    }

    // The table directory is sorted by tag; the tables follow in the order the
    // WOFF stored them (the original font's), each on a 4-byte boundary.
    let mut by_tag: Vec<usize> = (0..num_tables).collect();
    by_tag.sort_by_key(|&i| entries[i].tag);
    let mut by_offset: Vec<usize> = (0..num_tables).collect();
    by_offset.sort_by_key(|&i| entries[i].offset);

    let entry_selector = (usize::BITS - 1 - num_tables.leading_zeros()) as u16;
    let search_range = (1u16 << entry_selector) * 16;
    let range_shift = num_tables as u16 * 16 - search_range;

    let directory_len = 12 + 16 * num_tables;
    let mut offsets = vec![0usize; num_tables];
    let mut at = directory_len;
    for &i in &by_offset {
        offsets[i] = at;
        at += tables[i].len().next_multiple_of(4);
    }

    let mut sfnt = Vec::with_capacity(at);
    sfnt.extend_from_slice(&flavor.to_be_bytes());
    sfnt.extend_from_slice(&(num_tables as u16).to_be_bytes());
    sfnt.extend_from_slice(&search_range.to_be_bytes());
    sfnt.extend_from_slice(&entry_selector.to_be_bytes());
    sfnt.extend_from_slice(&range_shift.to_be_bytes());
    for &i in &by_tag {
        sfnt.extend_from_slice(&entries[i].tag);
        sfnt.extend_from_slice(&entries[i].checksum.to_be_bytes());
        sfnt.extend_from_slice(&(offsets[i] as u32).to_be_bytes());
        sfnt.extend_from_slice(&(tables[i].len() as u32).to_be_bytes());
    }
    for &i in &by_offset {
        debug_assert_eq!(sfnt.len(), offsets[i]);
        sfnt.extend_from_slice(&tables[i]);
        sfnt.resize(sfnt.len().next_multiple_of(4), 0);
    }
    if sfnt.len() != total_sfnt_size {
        return Err(format!("rebuilt {} bytes, the WOFF says the font had {total_sfnt_size}", sfnt.len()));
    }

    // checkSumAdjustment makes the whole font sum to 0xB1B0AFBA; recompute it
    // for the layout written here.
    let head = by_tag.iter().copied().find(|&i| &entries[i].tag == b"head").ok_or("no head table")?;
    let adjust_at = offsets[head] + 8;
    sfnt[adjust_at..adjust_at + 4].fill(0);
    let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&sfnt));
    sfnt[adjust_at..adjust_at + 4].copy_from_slice(&adjustment.to_be_bytes());
    Ok(sfnt)
}
