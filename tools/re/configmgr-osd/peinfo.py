"""Print a TSV inventory of PE files directly under <root>.

Columns: file, sha256, machine, native/managed, FileVersion, ProductVersion,
FileDescription, link date. Subdirectories (language folders) are skipped.

Usage: python -I peinfo.py <root> [suffix ...]   (default suffixes: .exe .dll)
"""
import datetime
import hashlib
import os
import sys

import pefile

root = sys.argv[1]
exts = tuple(a.lower() for a in sys.argv[2:]) or (".exe", ".dll")
MACH = {0x14C: "x86", 0x8664: "x64", 0xAA64: "arm64", 0x1C4: "arm"}

print("file\tsha256\tmachine\tkind\tfile_version\tproduct_version\tdescription\tlink_date")
for dirpath, _, files in os.walk(root):
    if dirpath != root:
        continue
    for name in sorted(files, key=str.lower):
        if not name.lower().endswith(exts):
            continue
        path = os.path.join(dirpath, name)
        try:
            data = open(path, "rb").read()
            pe = pefile.PE(data=data, fast_load=True)
            pe.parse_data_directories(directories=[
                pefile.DIRECTORY_ENTRY["IMAGE_DIRECTORY_ENTRY_RESOURCE"],
            ])
        except Exception as exc:  # not a PE
            print(f"{os.path.relpath(path, root)}\tERR {exc}")
            continue
        info = {}
        for fi in getattr(pe, "FileInfo", []) or []:
            for entry in fi:
                for st in getattr(entry, "StringTable", []) or []:
                    for k, v in st.entries.items():
                        info[k.decode(errors="replace")] = v.decode(errors="replace")
        clr = pe.OPTIONAL_HEADER.DATA_DIRECTORY[14].VirtualAddress != 0
        ts = datetime.datetime.fromtimestamp(pe.FILE_HEADER.TimeDateStamp, datetime.UTC).date()
        print("\t".join([
            os.path.relpath(path, root),
            hashlib.sha256(data).hexdigest(),
            MACH.get(pe.FILE_HEADER.Machine, hex(pe.FILE_HEADER.Machine)),
            "managed" if clr else "native",
            info.get("FileVersion", ""),
            info.get("ProductVersion", ""),
            info.get("FileDescription", ""),
            str(ts),
        ]))
