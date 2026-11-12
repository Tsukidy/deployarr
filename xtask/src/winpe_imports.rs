use std::collections::HashSet;
use std::fmt;
use std::path::Path;

use object::LittleEndian as LE;
use object::read::pe::PeFile64;

const LIST: &str = include_str!("../../docs/research/winpe-10.1.26100.9457-x64-dlls.txt");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Table {
    Import,
    DelayLoad,
}

impl fmt::Display for Table {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Table::Import => "import table",
            Table::DelayLoad => "delay-load import table",
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportedModule {
    pub name: String,
    pub table: Table,
}

pub struct AllowList {
    exact: HashSet<String>,
    api_set_prefixes: HashSet<String>,
}

fn is_api_set(lowered: &str) -> bool {
    lowered.starts_with("api-") || lowered.starts_with("ext-")
}

fn api_set_prefix(lowered: &str) -> &str {
    lowered
        .rsplit_once('-')
        .map_or(lowered, |(prefix, _)| prefix)
}

impl AllowList {
    pub fn parse(text: &str) -> Self {
        let mut exact = HashSet::new();
        let mut api_set_prefixes = HashSet::new();
        for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
            let lowered = line.to_ascii_lowercase();
            if is_api_set(&lowered) {
                api_set_prefixes.insert(api_set_prefix(&lowered).to_owned());
            }
            exact.insert(lowered);
        }
        Self {
            exact,
            api_set_prefixes,
        }
    }

    pub fn bundled() -> Self {
        Self::parse(LIST)
    }

    pub fn accepts(&self, import: &str) -> bool {
        let lowered = import.to_ascii_lowercase();
        if is_api_set(&lowered) {
            self.api_set_prefixes.contains(api_set_prefix(&lowered))
        } else {
            self.exact.contains(&lowered)
        }
    }
}

pub fn rejected<'a>(list: &AllowList, imports: &'a [ImportedModule]) -> Vec<&'a ImportedModule> {
    imports.iter().filter(|i| !list.accepts(&i.name)).collect()
}

pub fn read_imports(data: &[u8]) -> Result<Vec<ImportedModule>, String> {
    let pe = PeFile64::parse(data).map_err(|e| format!("not a 64-bit PE file: {e}"))?;
    let mut out = Vec::new();
    let read_err = |what: &str, e: object::Error| format!("cannot read the {what}: {e}");
    if let Some(table) = pe.import_table().map_err(|e| read_err("import table", e))? {
        let mut descriptors = table
            .descriptors()
            .map_err(|e| read_err("import table", e))?;
        while let Some(d) = descriptors
            .next()
            .map_err(|e| read_err("import table", e))?
        {
            let name = table
                .name(d.name.get(LE))
                .map_err(|e| read_err("import table", e))?;
            out.push(ImportedModule {
                name: String::from_utf8_lossy(name).into_owned(),
                table: Table::Import,
            });
        }
    }
    if let Some(table) = pe
        .delay_load_import_table()
        .map_err(|e| read_err("delay-load import table", e))?
    {
        let mut descriptors = table
            .descriptors()
            .map_err(|e| read_err("delay-load import table", e))?;
        while let Some(d) = descriptors
            .next()
            .map_err(|e| read_err("delay-load import table", e))?
        {
            let name = table
                .name(d.dll_name_rva.get(LE))
                .map_err(|e| read_err("delay-load import table", e))?;
            out.push(ImportedModule {
                name: String::from_utf8_lossy(name).into_owned(),
                table: Table::DelayLoad,
            });
        }
    }
    Ok(out)
}

pub fn run(path: &Path) -> Result<(), String> {
    let data = std::fs::read(path).map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let imports = read_imports(&data)?;
    let list = AllowList::bundled();
    let bad = rejected(&list, &imports);
    if bad.is_empty() {
        println!(
            "{}: all {} imported modules are in base WinPE",
            path.display(),
            imports.len()
        );
        return Ok(());
    }
    let mut msg = format!(
        "{} imports {} module(s) that base WinPE lacks:",
        path.display(),
        bad.len()
    );
    for i in bad {
        msg.push_str(&format!("\n  {:?} in the {}", i.name, i.table));
    }
    Err(msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> AllowList {
        AllowList::parse(
            "kernel32.dll\napi-ms-win-core-synch-l1-2-1.dll\next-ms-win-ntuser-window-l1-1-4.dll\n",
        )
    }

    #[test]
    fn matching_rule() {
        let cases = [
            ("kernel32.dll", true, "plain module hit"),
            ("KERNEL32.DLL", true, "case folding of a plain module"),
            ("comctl32.dll", false, "plain module miss"),
            ("kernel32", false, "plain module needs the exact name"),
            (
                "api-ms-win-core-synch-l1-2-1.dll",
                true,
                "api set, same number",
            ),
            (
                "api-ms-win-core-synch-l1-2-0.dll",
                true,
                "api set, lower last number",
            ),
            (
                "API-MS-Win-Core-Synch-L1-2-0.DLL",
                true,
                "api set, case folding",
            ),
            (
                "api-ms-win-core-synch-l1-1-0.dll",
                false,
                "api set, different version stem",
            ),
            ("api-ms-win-core-nothere-l1-1-0.dll", false, "api set miss"),
            (
                "ext-ms-win-ntuser-window-l1-1-0.dll",
                true,
                "ext set resolves through its prefix",
            ),
            (
                "ext-ms-win-ntuser-window-l1-1-4.dll",
                true,
                "ext set, same number",
            ),
            ("ext-ms-win-other-l1-1-0.dll", false, "ext set miss"),
        ];
        let list = list();
        for (name, want, why) in cases {
            assert_eq!(list.accepts(name), want, "{name}: {why}");
        }
    }

    #[test]
    fn bundled_list_has_the_documented_entries() {
        let list = AllowList::bundled();
        assert!(list.accepts("kernel32.dll"));
        assert!(list.accepts("api-ms-win-core-synch-l1-2-0.dll"));
        assert!(!list.accepts("comctl32.dll"));
    }

    #[test]
    fn rejected_reports_name_and_table() {
        let imports = vec![
            ImportedModule {
                name: "kernel32.dll".into(),
                table: Table::Import,
            },
            ImportedModule {
                name: "comctl32.dll".into(),
                table: Table::DelayLoad,
            },
        ];
        let bad = rejected(&list(), &imports);
        assert_eq!(bad, vec![&imports[1]]);
    }

    fn minimal_pe(imports: &[&str], delay_loads: &[&str]) -> Vec<u8> {
        const SECTION_RVA: u32 = 0x1000;
        const SECTION_OFFSET: usize = 0x200;
        let import_len = (imports.len() + 1) * 20;
        let delay_len = (delay_loads.len() + 1) * 32;
        let mut section = vec![0u8; import_len + delay_len];
        let mut names = Vec::new();
        let mut name_rva = |name: &str| {
            let rva = SECTION_RVA + (import_len + delay_len + names.len()) as u32;
            names.extend_from_slice(name.as_bytes());
            names.push(0);
            rva
        };
        for (i, name) in imports.iter().enumerate() {
            let rva = name_rva(name);
            section[i * 20 + 12..i * 20 + 16].copy_from_slice(&rva.to_le_bytes());
        }
        for (i, name) in delay_loads.iter().enumerate() {
            let rva = name_rva(name);
            let at = import_len + i * 32;
            section[at..at + 4].copy_from_slice(&1u32.to_le_bytes());
            section[at + 4..at + 8].copy_from_slice(&rva.to_le_bytes());
        }
        section.extend_from_slice(&names);

        let mut pe = vec![0u8; SECTION_OFFSET];
        pe[..2].copy_from_slice(b"MZ");
        pe[0x3c..0x40].copy_from_slice(&0x40u32.to_le_bytes());
        pe[0x40..0x44].copy_from_slice(b"PE\0\0");
        let coff = 0x44;
        pe[coff..coff + 2].copy_from_slice(&0x8664u16.to_le_bytes());
        pe[coff + 2..coff + 4].copy_from_slice(&1u16.to_le_bytes());
        pe[coff + 16..coff + 18].copy_from_slice(&240u16.to_le_bytes());
        let opt = coff + 20;
        pe[opt..opt + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        pe[opt + 32..opt + 36].copy_from_slice(&0x1000u32.to_le_bytes());
        pe[opt + 36..opt + 40].copy_from_slice(&0x200u32.to_le_bytes());
        pe[opt + 56..opt + 60].copy_from_slice(&0x3000u32.to_le_bytes());
        pe[opt + 60..opt + 64].copy_from_slice(&0x200u32.to_le_bytes());
        pe[opt + 108..opt + 112].copy_from_slice(&16u32.to_le_bytes());
        let dirs = opt + 112;
        let mut dir = |index: usize, rva: u32, size: usize| {
            let at = dirs + index * 8;
            pe[at..at + 4].copy_from_slice(&rva.to_le_bytes());
            pe[at + 4..at + 8].copy_from_slice(&(size as u32).to_le_bytes());
        };
        dir(1, SECTION_RVA, import_len);
        dir(13, SECTION_RVA + import_len as u32, delay_len);
        let header = opt + 240;
        pe[header..header + 6].copy_from_slice(b".idata");
        pe[header + 8..header + 12].copy_from_slice(&(section.len() as u32).to_le_bytes());
        pe[header + 12..header + 16].copy_from_slice(&SECTION_RVA.to_le_bytes());
        pe[header + 16..header + 20].copy_from_slice(&(section.len() as u32).to_le_bytes());
        pe[header + 20..header + 24].copy_from_slice(&(SECTION_OFFSET as u32).to_le_bytes());
        pe.extend_from_slice(&section);
        pe
    }

    #[test]
    fn reads_both_tables_in_a_pe_file() {
        let pe = minimal_pe(
            &["KERNEL32.dll", "comctl32.dll"],
            &["bcrypt.dll", "tbs.dll"],
        );
        let imports = read_imports(&pe).unwrap();
        let got: Vec<_> = imports.iter().map(|i| (i.name.as_str(), i.table)).collect();
        assert_eq!(
            got,
            [
                ("KERNEL32.dll", Table::Import),
                ("comctl32.dll", Table::Import),
                ("bcrypt.dll", Table::DelayLoad),
                ("tbs.dll", Table::DelayLoad),
            ]
        );
        let bad: Vec<_> = rejected(&AllowList::bundled(), &imports)
            .iter()
            .map(|i| (i.name.as_str(), i.table))
            .collect();
        assert_eq!(
            bad,
            [
                ("comctl32.dll", Table::Import),
                ("tbs.dll", Table::DelayLoad)
            ]
        );
    }

    #[test]
    fn non_pe_input_is_an_error() {
        assert!(read_imports(b"not a pe").is_err());
    }
}
