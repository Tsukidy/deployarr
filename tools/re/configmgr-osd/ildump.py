"""Dump CIL of methods in a .NET assembly whose body loads a matching ldstr literal.

Usage: python -I ildump.py <assembly> <regex-on-ldstr> [--names regex-on-method-name]
Prints Type::Method then each instruction as: offset opcode operand (strings and
constants resolved; call targets shown by name).
"""
import argparse
import re

import dnfile
from dncil.cil.body import CilMethodBody
from dncil.cil.body.reader import CilMethodBodyReaderBase
from dncil.cil.error import MethodBodyFormatError
from dncil.clr.token import StringToken, Token, InvalidToken

ap = argparse.ArgumentParser()
ap.add_argument("asm")
ap.add_argument("pattern")
ap.add_argument("--names")
args = ap.parse_args()
rx = re.compile(args.pattern)
nrx = re.compile(args.names) if args.names else None

pe = dnfile.dnPE(args.asm)


class Reader(CilMethodBodyReaderBase):
    def __init__(self, pe, rva):
        self.pe = pe
        self.offset = pe.get_offset_from_rva(rva)

    def read(self, n):
        data = self.pe.get_data(self.pe.get_rva_from_offset(self.offset), n)
        self.offset += n
        return data

    def tell(self):
        return self.offset

    def seek(self, off):
        self.offset = off
        return self.offset


def us(token):
    try:
        return pe.net.user_strings.get(token.rid).value
    except Exception:
        return None


def name_of(token):
    if not isinstance(token, Token) or isinstance(token, InvalidToken):
        return str(token)
    table = {0x06: "MethodDef", 0x0A: "MemberRef", 0x2B: "MethodSpec", 0x04: "Field",
             0x01: "TypeRef", 0x02: "TypeDef"}.get(token.table)
    if not table:
        return str(token)
    try:
        row = getattr(pe.net.mdtables, table)[token.rid - 1]
        nm = getattr(row, "Name", None) or getattr(row, "TypeName", None)
        parent = ""
        cls = getattr(row, "Class", None)
        if cls is not None and getattr(cls, "row", None) is not None:
            parent = (getattr(cls.row, "TypeName", None) or getattr(cls.row, "Name", "") or "") + "::"
        return f"{parent}{nm}"
    except Exception:
        return str(token)


typedefs = pe.net.mdtables.TypeDef
owner = {}
for td in typedefs:
    for m in td.MethodList:
        owner[m.row_index] = f"{td.TypeNamespace}.{td.TypeName}"

for idx, row in enumerate(pe.net.mdtables.MethodDef, start=1):
    if not row.Rva:
        continue
    try:
        body = CilMethodBody(Reader(pe, row.Rva))
    except MethodBodyFormatError:
        continue
    lits = [us(i.operand) for i in body.instructions if isinstance(i.operand, StringToken)]
    full = f"{owner.get(idx, '?')}::{row.Name}"
    if nrx and not nrx.search(full):
        continue
    if not any(s and rx.search(s) for s in lits):
        continue
    print(f"=== {full}")
    for ins in body.instructions:
        op = ins.operand
        if isinstance(op, StringToken):
            val = repr(us(op))
        elif isinstance(op, Token):
            val = name_of(op)
        else:
            val = "" if op is None else repr(op)
        print(f"  {ins.offset:04x} {ins.opcode.name:<12} {val}")
