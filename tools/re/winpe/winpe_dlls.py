#!/usr/bin/env python3
"""Helpers for reproduce.sh. Run as: python3 -I winpe_dlls.py <command> ...

Commands:
  version <burn-manifest>
  payloads <burn-manifest> <msi-package-id>
  locate <msi-table-dir>
  schema-path <wim-listing>
  build <wim-listing> <apisetschema.dll> <out-dir> [key=value ...]
"""
import re
import struct
import sys
import urllib.parse
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import NamedTuple

BURN = "{http://schemas.microsoft.com/wix/2008/Burn}"
AMD64_DIR = "Windows Preinstallation Environment\\amd64\\en-us"
SCHEMA_VERSION = 6


class ApiSet(NamedTuple):
    name: str
    hashed_name: str
    default_host: str | None
    aliases: dict[str, str]

    def resolvable(self, dlls: set[str]) -> bool:
        return bool(self.default_host) and self.default_host.lower() in dlls


def parse_apiset_section(data: bytes) -> tuple[int, int, list[ApiSet]]:
    """Return (schema version, header count, entries) from a .apiset section."""
    version, _size, _flags, count, entry_off, _hash_off, _factor = struct.unpack_from("<7I", data, 0)
    if version != SCHEMA_VERSION:
        raise ValueError(f"API set schema version {version}, expected {SCHEMA_VERSION}")

    def text(off: int, length: int) -> str:
        if off + length > len(data):
            raise ValueError("string outside the .apiset section")
        return data[off : off + length].decode("utf-16-le")

    entries = []
    for i in range(count):
        _f, name_off, name_len, hashed_len, val_off, val_count = struct.unpack_from("<6I", data, entry_off + 24 * i)
        default_host: str | None = None
        aliases: dict[str, str] = {}
        for j in range(val_count):
            _vf, imp_off, imp_len, host_off, host_len = struct.unpack_from("<5I", data, val_off + 20 * j)
            importer, host = text(imp_off, imp_len), text(host_off, host_len)
            if importer == "":
                if default_host is None:
                    default_host = host
            else:
                aliases[importer] = host
        entries.append(ApiSet(text(name_off, name_len), text(name_off, hashed_len), default_host, aliases))
    return version, count, entries


def apiset_section(pe: bytes) -> bytes:
    pe_off = struct.unpack_from("<I", pe, 0x3C)[0]
    if pe[pe_off : pe_off + 4] != b"PE\0\0":
        raise ValueError("not a PE file")
    nsec, opt_size = struct.unpack_from("<H12xH", pe, pe_off + 6)
    table = pe_off + 24 + opt_size
    for i in range(nsec):
        name, _vsize, _va, raw_size, raw_ptr = struct.unpack_from("<8s4I", pe, table + 40 * i)
        if name.rstrip(b"\0") == b".apiset":
            return pe[raw_ptr : raw_ptr + raw_size]
    raise ValueError("no .apiset section")


def read_idt(path: Path) -> list[dict[str, str]]:
    lines = path.read_text(encoding="utf-8", errors="replace").replace("\r", "").split("\n")
    cols = lines[0].split("\t")
    return [dict(zip(cols, line.split("\t"))) for line in lines[3:] if line]


def long_name(default_dir: str) -> str:
    return default_dir.split("|")[-1]


def locate_amd64_wim(table_dir: Path) -> tuple[str, str, int]:
    """Return (file key, cabinet name, file size) of the amd64 winpe.wim."""
    dirs = {r["Directory"]: r for r in read_idt(table_dir / "Directory.idt")}
    comp_dir = {r["Component"]: r["Directory_"] for r in read_idt(table_dir / "Component.idt")}

    def dir_path(key: str) -> str:
        parts = []
        while key:
            row = dirs[key]
            parts.append(long_name(row["DefaultDir"]))
            key = row["Directory_Parent"]
        return "\\".join(reversed(parts))

    found = [
        r
        for r in read_idt(table_dir / "File.idt")
        if long_name(r["FileName"]).lower() == "winpe.wim"
        and dir_path(comp_dir[r["Component_"]]).endswith(AMD64_DIR)
    ]
    if len(found) != 1:
        raise ValueError(f"expected one amd64 winpe.wim in the File table, found {len(found)}")
    row = found[0]
    seq = int(row["Sequence"])
    media = [m for m in read_idt(table_dir / "Media.idt") if int(m["LastSequence"]) >= seq]
    cab = min(media, key=lambda m: int(m["LastSequence"]))["Cabinet"]
    return row["File"], cab, int(row["FileSize"])


def parse_listing(text: str) -> tuple[ET.Element, list[tuple[str, bool]]]:
    head, _, body = text.partition("\n----------\n")
    xml = re.search(r"<WIM>.*?</WIM>", head, re.S)
    if not xml:
        raise ValueError("no WIM XML in the listing")
    entries: list[tuple[str, bool]] = []
    for line in body.split("\n"):
        if line.startswith("Path = "):
            entries.append((line[7:], False))
        elif line.startswith("Folder = ") and entries:
            entries[-1] = (entries[-1][0], line[9:] == "+")
    return ET.fromstring(xml.group(0)), entries


def system32_dlls(entries: list[tuple[str, bool]]) -> list[str]:
    pat = re.compile(r"^Windows[\\/]System32[\\/]([^\\/]+\.(?:dll|drv))$", re.I)
    return [m.group(1) for p, folder in entries if not folder and (m := pat.match(p))]


def wim_facts(root: ET.Element) -> dict[str, str]:
    images = root.findall("IMAGE")
    if len(images) != 1:
        raise ValueError(f"expected one image in the WIM, found {len(images)}")
    img = images[0]
    name = img.findtext("NAME") or ""
    if "(amd64)" not in name:
        raise ValueError(f"WIM is not the amd64 image: {name!r}")
    ver = ".".join(img.findtext(f"WINDOWS/VERSION/{t}") or "" for t in ("MAJOR", "MINOR", "BUILD", "SPBUILD"))
    return {
        "wim_name": name,
        "wim_version": ver,
        "wim_branch": img.findtext("WINDOWS/VERSION/BRANCH") or "",
    }


def cmd_build(listing: Path, schema_dll: Path, out: Path, facts: list[str]) -> None:
    root, entries = parse_listing(listing.read_text(encoding="utf-8", errors="replace"))
    wim = wim_facts(root)
    dlls = sorted(system32_dlls(entries), key=lambda n: (n.lower(), n))
    lower = {d.lower() for d in dlls}

    version, count, sets = parse_apiset_section(apiset_section(schema_dll.read_bytes()))
    if len(sets) != count:
        raise ValueError(f"parsed {len(sets)} entries, header says {count}")
    bad = [s.name for s in sets if not s.name.startswith(("api-", "ext-"))]
    if bad:
        raise ValueError(f"unexpected API set names: {bad[:5]}")

    ok = {s.name for s in sets if s.resolvable(lower)}
    names = sorted(lower | {f"{n.lower()}.dll" for n in ok})

    out.mkdir(parents=True, exist_ok=True)
    (out / "winpe-x64-dlls.txt").write_text("".join(f"{n}\n" for n in names), encoding="utf-8", newline="\n")
    (out / "system32-dlls.txt").write_text("".join(f"{n}\n" for n in dlls), encoding="utf-8", newline="\n")
    rows = ["name\thashed_name\tdefault_host\taliases\tresolvable"]
    for s in sets:
        aliases = ";".join(f"{k}={v}" for k, v in s.aliases.items())
        rows.append("\t".join((s.name, s.hashed_name, s.default_host or "", aliases, "yes" if s.name in ok else "no")))
    (out / "apisets.tsv").write_text("\n".join(rows) + "\n", encoding="utf-8", newline="\n")

    summary = [tuple(f.split("=", 1)) for f in facts]
    summary += list(wim.items())
    summary += [
        ("system32_dll_count", str(len(dlls))),
        ("apiset_schema_version", str(version)),
        ("apiset_entry_count", str(count)),
        ("apisets_resolvable", str(len(ok))),
        ("apisets_unresolvable", str(count - len(ok))),
        ("winpe_x64_dll_list_count", str(len(names))),
        ("tbs.dll_present", "yes" if "tbs.dll" in lower else "no"),
    ]
    (out / "summary.txt").write_text("".join(f"{k}: {v}\n" for k, v in summary), encoding="utf-8", newline="\n")


def package_payloads(manifest: Path, package_id: str) -> list[ET.Element]:
    root = ET.parse(manifest).getroot()
    pkg = next((p for p in root.iter(f"{BURN}MsiPackage") if p.get("Id") == package_id), None)
    if pkg is None:
        raise ValueError(f"no MsiPackage {package_id} in the manifest")
    refs = {r.get("Id") for r in pkg.iter(f"{BURN}PayloadRef")}
    return [p for p in root.iter(f"{BURN}Payload") if p.get("Id") in refs]


def main(argv: list[str]) -> None:
    cmd, args = argv[1], argv[2:]
    if cmd == "version":
        reg = next(ET.parse(args[0]).getroot().iter(f"{BURN}Registration"))
        print(reg.get("Version"))
    elif cmd == "payloads":
        for p in package_payloads(Path(args[0]), args[1]):
            fp = p.get("FilePath", "")
            url_path = urllib.parse.quote(fp.replace("\\", "/"))
            print("\t".join((fp, p.get("FileSize", ""), p.get("Hash", "").lower(), url_path)))
    elif cmd == "locate":
        print("\t".join(map(str, locate_amd64_wim(Path(args[0])))))
    elif cmd == "schema-path":
        _, entries = parse_listing(Path(args[0]).read_text(encoding="utf-8", errors="replace"))
        pat = re.compile(r"^Windows[\\/]System32[\\/]apisetschema\.dll$", re.I)
        hits = [p for p, folder in entries if not folder and pat.match(p)]
        if len(hits) != 1:
            raise ValueError(f"expected one apisetschema.dll, found {len(hits)}")
        print(hits[0])
    elif cmd == "build":
        cmd_build(Path(args[0]), Path(args[1]), Path(args[2]), args[3:])
    else:
        raise SystemExit(__doc__)


if __name__ == "__main__":
    try:
        main(sys.argv)
    except (ValueError, KeyError, StopIteration, struct.error) as e:
        sys.exit(f"winpe_dlls.py: {type(e).__name__}: {e}")
