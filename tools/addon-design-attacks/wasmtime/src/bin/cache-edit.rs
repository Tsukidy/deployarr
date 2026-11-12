//! Finding 5: a compiled component written to disk is code that wasmtime loads back without
//! an integrity check. The probe serializes a component whose function returns 0x1234567,
//! changes the 4 bytes of that constant in the file's `.text` section, loads the file with
//! `Component::deserialize_file`, and calls the function again.
//!
//!   cache-edit <folder>

use wasmtime::component::{Component, Linker};
use wasmtime::{Engine, Result, Store};

const WAT: &str = r#"(component
    (core module $m (func (export "f") (result i32) (i32.const 0x1234567)))
    (core instance $i (instantiate $m))
    (func (export "f") (result u32) (canon lift (core func $i "f"))))"#;

fn call(engine: &Engine, component: &Component) -> Result<u32> {
    let linker: Linker<()> = Linker::new(engine);
    let mut store = Store::new(engine, ());
    let instance = linker.instantiate(&mut store, component)?;
    let f = instance.get_typed_func::<(), (u32,)>(&mut store, "f")?;
    Ok(f.call(&mut store, ())?.0)
}

fn u16_at(b: &[u8], at: usize) -> usize {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap()) as usize
}
fn u32_at(b: &[u8], at: usize) -> usize {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
}
fn u64_at(b: &[u8], at: usize) -> usize {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap()) as usize
}

/// The file offset and size of the ELF64 section named `want`.
fn section(elf: &[u8], want: &str) -> Option<(usize, usize)> {
    let (shoff, shentsize, shnum, shstrndx) = (u64_at(elf, 0x28), u16_at(elf, 0x3a), u16_at(elf, 0x3c), u16_at(elf, 0x3e));
    let header = |i: usize| shoff + i * shentsize;
    let strtab = u64_at(elf, header(shstrndx) + 24);
    (0..shnum).find_map(|i| {
        let name_at = strtab + u32_at(elf, header(i));
        let end = elf[name_at..].iter().position(|&c| c == 0)? + name_at;
        (&elf[name_at..end] == want.as_bytes()).then(|| (u64_at(elf, header(i) + 24), u64_at(elf, header(i) + 32)))
    })
}

fn main() -> Result<()> {
    let folder = std::env::args().nth(1).expect("usage: cache-edit <folder>");
    let path = std::path::Path::new(&folder).join("component.cwasm");
    let engine = Engine::default();

    let original = Component::new(&engine, WAT)?;
    let mut bytes = original.serialize()?;
    std::fs::write(&path, &bytes)?;
    let loaded = unsafe { Component::deserialize_file(&engine, &path)? };
    let before = call(&engine, &loaded)?;

    let (text_at, text_len) = section(&bytes, ".text").expect("the serialized component is ELF with a .text section");
    let needle = 0x1234567u32.to_le_bytes();
    let hits: Vec<usize> = bytes[text_at..text_at + text_len]
        .windows(4)
        .enumerate()
        .filter(|(_, w)| *w == needle)
        .map(|(i, _)| text_at + i)
        .collect();
    let in_file = bytes.windows(4).filter(|w| *w == needle).count();
    let at = hits[0];
    bytes[at..at + 4].copy_from_slice(&0xbadc0deu32.to_le_bytes());
    std::fs::write(&path, &bytes)?;
    let edited = unsafe { Component::deserialize_file(&engine, &path)? };
    let after = call(&engine, &edited)?;

    println!(
        "cache file: {} bytes; the constant occurs {in_file} time(s) in the file, {} time(s) in .text; edited 4 bytes at offset {at}",
        bytes.len(),
        hits.len()
    );
    println!("cache file: component returns {before:#x}; after editing 4 bytes of the cache file it returns {after:#x}");
    Ok(())
}
