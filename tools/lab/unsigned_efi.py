"""Write a minimal unsigned x64 EFI application that returns EFI_SUCCESS.

check.sh serves it as a PXE boot program. Firmware with Secure Boot on must
refuse it with "Access Denied". Firmware with Secure Boot off runs it.
Usage: python3 unsigned_efi.py <output file>
"""

import struct
import sys

SECTION_ALIGNMENT = 0x1000
FILE_ALIGNMENT = 0x200


def build() -> bytes:
    dos = bytearray(64)
    dos[0:2] = b"MZ"
    struct.pack_into("<I", dos, 0x3C, len(dos))
    # COFF header: x64, one section, a PE32+ optional header, executable.
    coff = struct.pack("<HHIIIHH", 0x8664, 1, 0, 0, 0, 240, 0x0022)
    optional = struct.pack(
        "<HBBIIIII", 0x20B, 0, 0, FILE_ALIGNMENT, 0, 0, SECTION_ALIGNMENT, SECTION_ALIGNMENT
    )
    optional += struct.pack(
        "<QIIHHHHHHIIIIHH",
        0x10000000,  # image base
        SECTION_ALIGNMENT,
        FILE_ALIGNMENT,
        0, 0, 0, 0, 0, 0, 0,
        2 * SECTION_ALIGNMENT,  # size of image
        FILE_ALIGNMENT,  # size of headers
        0,
        10,  # subsystem: EFI application
        0,
    )
    optional += struct.pack("<QQQQII", 0x10000, 0x1000, 0x10000, 0x1000, 0, 16)
    optional += bytes(16 * 8)
    assert len(optional) == 240
    section = struct.pack(
        "<8sIIIIIIHHI",
        b".text",
        0x10,
        SECTION_ALIGNMENT,
        FILE_ALIGNMENT,
        FILE_ALIGNMENT,
        0, 0, 0, 0,
        0x60000020,  # code, execute, read
    )
    headers = bytes(dos) + b"PE\0\0" + coff + optional + section
    code = b"\x31\xc0\xc3"  # xor eax, eax; ret
    return headers.ljust(FILE_ALIGNMENT, b"\0") + code.ljust(FILE_ALIGNMENT, b"\0")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.exit(__doc__)
    with open(sys.argv[1], "wb") as out:
        out.write(build())
