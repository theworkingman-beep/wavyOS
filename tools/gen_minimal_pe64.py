#!/usr/bin/env python3
"""Generate a synthetic x86_64 PE fixture for Aperture OS bring-up.

The fixture runs inside the kernel and exercises the NT syscall dispatch by
performing a small sequence of file and memory operations:

  1. NtAllocateVirtualMemory
  2. NtCreateFile("/bin/hello.txt", create, write)
  3. NtWriteFile(handle, "Hello from Aperture OS PE fixture\n")
  4. NtClose(handle)

It uses only instructions supported by the baseline interpreter
(MOV imm, XOR reg/mem, LEA RIP-relative, SYSCALL, RET) so the same binary
exercises both the native x86_64 SYSCALL path and the AArch64 interpreter
path.
"""

import struct
from pathlib import Path

IMAGE_BASE = 0x0000_0001_4000_0000
TEXT_RVA = 0x1000
SECTION_ALIGNMENT = 0x1000
FILE_ALIGNMENT = 0x200
TEXT_FILE_OFFSET = 0x200
TEXT_SIZE_RAW = 0x200  # 512 byte .text section


def build_code() -> bytes:
    """Emit the .text bytes: code followed by data slots and strings."""
    code = bytearray()

    # NtAllocateVirtualMemory
    code += b"\xB8\x18\x00\x00\x00"          # mov eax, 0x18
    code += b"\x48\x31\xFF"                  # xor rdi, rdi
    code += b"\x48\x8D\x35" + struct.pack("<i", 0)  # lea rsi, [rip+disp] placeholder
    alloc_lea_offset = len(code) - 4
    code += b"\x48\x31\xD2"                  # xor rdx, rdx
    code += b"\x49\xC7\xC2\x00\x10\x00\x00"  # mov r10, 0x1000
    code += b"\x4D\x31\xC0"                  # xor r8, r8
    code += b"\x4D\x31\xC9"                  # xor r9, r9
    code += b"\x0F\x05"                      # syscall

    # NtCreateFile
    code += b"\xB8\x55\x00\x00\x00"          # mov eax, 0x55
    code += b"\x48\x8D\x3D" + struct.pack("<i", 0)  # lea rdi, [rip+disp] placeholder
    create_lea_rdi_offset = len(code) - 4
    code += b"\x48\x8D\x35" + struct.pack("<i", 0)  # lea rsi, [rip+disp] placeholder
    create_lea_rsi_offset = len(code) - 4
    code += b"\xBA\x01\x00\x00\x00"          # mov edx, 1 (create)
    code += b"\x49\xB8" + struct.pack("<Q", 0x4000_0000)  # mov r10, GENERIC_WRITE
    code += b"\x0F\x05"                      # syscall

    # NtWriteFile
    code += b"\xB8\x08\x00\x00\x00"          # mov eax, 0x08
    code += b"\x48\x31\xFF"                  # xor rdi, rdi
    code += b"\x48\x33\x3D" + struct.pack("<i", 0)  # xor rdi, [rip+disp] placeholder
    write_xor_rdi_offset = len(code) - 4
    code += b"\x48\x8D\x35" + struct.pack("<i", 0)  # lea rsi, [rip+disp] placeholder
    write_lea_rsi_offset = len(code) - 4
    code += b"\xBA" + struct.pack("<I", 0)  # mov edx, msg_len placeholder
    write_len_offset = len(code) - 4
    code += b"\x0F\x05"                      # syscall

    # NtClose
    code += b"\xB8\x0F\x00\x00\x00"          # mov eax, 0x0F
    code += b"\x48\x31\xFF"                  # xor rdi, rdi
    code += b"\x48\x33\x3D" + struct.pack("<i", 0)  # xor rdi, [rip+disp] placeholder
    close_xor_rdi_offset = len(code) - 4
    code += b"\x0F\x05"                      # syscall

    # NtQuerySystemInformation(SystemBasicInformation, buf, 64, 0)
    code += b"\xB8\x36\x00\x00\x00"          # mov eax, 0x36
    code += b"\x48\x31\xFF"                  # xor rdi, rdi (class 0)
    code += b"\x48\x8D\x35" + struct.pack("<i", 0)  # lea rsi, [rip+disp] placeholder
    sysinfo_lea_rsi_offset = len(code) - 4
    code += b"\xBA\x40\x00\x00\x00"          # mov edx, 64
    code += b"\x4D\x31\xD2"                  # xor r10, r10 (return-length ptr = null)
    code += b"\x0F\x05"                      # syscall

    # NtQueryInformationProcess(-1, ProcessBasicInformation, buf, 48, 0)
    code += b"\xB8\x19\x00\x00\x00"          # mov eax, 0x19
    code += b"\x48\xC7\xC7\xFF\xFF\xFF\xFF"  # mov rdi, -1 (NtCurrentProcess)
    code += b"\x48\x31\xF6"                  # xor rsi, rsi (class 0)
    code += b"\x48\x8D\x15" + struct.pack("<i", 0)  # lea rdx, [rip+disp] placeholder
    procinfo_lea_rdx_offset = len(code) - 4
    code += b"\x49\xC7\xC2\x30\x00\x00\x00"  # mov r10, 48
    code += b"\x4D\x31\xC0"                  # xor r8, r8 (return-length ptr = null)
    code += b"\x0F\x05"                      # syscall

    # NtDelayExecution(FALSE, interval)
    code += b"\xB8\x34\x00\x00\x00"          # mov eax, 0x34
    code += b"\x48\x31\xFF"                  # xor rdi, rdi (alertable = FALSE)
    code += b"\x48\x8D\x35" + struct.pack("<i", 0)  # lea rsi, [rip+disp] placeholder
    delay_lea_rsi_offset = len(code) - 4
    code += b"\x0F\x05"                      # syscall

    code += b"\xC3"                          # ret

    # Pad to 8-byte alignment for the data slots.
    while len(code) % 8 != 0:
        code += b"\x00"

    base_slot = len(code)
    code += struct.pack("<Q", 0)             # base_slot: dq 0

    handle_slot = len(code)
    code += struct.pack("<Q", 0)             # handle_slot: dq 0

    sysinfo_slot = len(code)
    code += b"\x00" * 64                     # sysinfo_slot: 64-byte buffer

    procinfo_slot = len(code)
    code += b"\x00" * 48                     # procinfo_slot: 48-byte buffer

    delay_slot = len(code)
    code += struct.pack("<q", -10000)        # delay_slot: 1 ms relative interval

    path = b"/bin/hello.txt\x00"
    path_offset = len(code)
    code += path

    msg = b"Hello from Aperture OS PE fixture\n"
    msg_offset = len(code)
    code += msg

    # Patch RIP-relative displacements. Each displacement is the target
    # offset minus the offset of the instruction following the LEA/XOR.
    def patch(offset: int, target: int, instr_len_after: int):
        disp = target - (offset + instr_len_after)
        code[offset:offset + 4] = struct.pack("<i", disp)

    # LEA is 7 bytes total; the displacement field ends 4 bytes after offset.
    patch(alloc_lea_offset, base_slot, 7)
    patch(create_lea_rdi_offset, handle_slot, 7)
    patch(create_lea_rsi_offset, path_offset, 7)
    patch(write_lea_rsi_offset, msg_offset, 7)
    patch(sysinfo_lea_rsi_offset, sysinfo_slot, 7)
    patch(procinfo_lea_rdx_offset, procinfo_slot, 7)
    patch(delay_lea_rsi_offset, delay_slot, 7)

    # XOR r/m64, r64 with RIP-relative addressing is 7 bytes too.
    patch(write_xor_rdi_offset, handle_slot, 7)
    patch(close_xor_rdi_offset, handle_slot, 7)

    # Patch write length.
    code[write_len_offset:write_len_offset + 4] = struct.pack("<I", len(msg))

    if len(code) > TEXT_SIZE_RAW:
        raise RuntimeError(f".text section too large: {len(code)} > {TEXT_SIZE_RAW}")
    code += b"\x00" * (TEXT_SIZE_RAW - len(code))
    return bytes(code)


def build_pe(text_bytes: bytes) -> bytes:
    """Build a minimal 64-bit PE around the provided .text bytes."""
    # DOS header (64 bytes)
    dos = bytearray(64)
    dos[0:2] = b"MZ"
    dos[0x3C:0x40] = struct.pack("<I", 0x40)  # e_lfanew

    # PE signature (4 bytes)
    pe_sig = b"PE\x00\x00"

    # Optional header fields packed explicitly to avoid bytearray slice errors.
    optional = struct.pack(
        "<HBBIIIIIQIIHHHHHHIIIIHHQQQQII",
        0x20B,              # Magic (PE32+)
        0,                  # MajorLinkerVersion
        0,                  # MinorLinkerVersion
        TEXT_SIZE_RAW,      # SizeOfCode
        0,                  # SizeOfInitializedData
        0,                  # SizeOfUninitializedData
        TEXT_RVA,           # AddressOfEntryPoint
        TEXT_RVA,           # BaseOfCode
        IMAGE_BASE,         # ImageBase
        SECTION_ALIGNMENT,  # SectionAlignment
        FILE_ALIGNMENT,     # FileAlignment
        6,                  # MajorOperatingSystemVersion
        0,                  # MinorOperatingSystemVersion
        0,                  # MajorImageVersion
        0,                  # MinorImageVersion
        6,                  # MajorSubsystemVersion
        0,                  # MinorSubsystemVersion
        0,                  # Win32VersionValue
        0x2000,             # SizeOfImage
        TEXT_FILE_OFFSET,   # SizeOfHeaders
        0,                  # CheckSum
        1,                  # Subsystem (native)
        0,                  # DllCharacteristics
        0x10_0000,          # SizeOfStackReserve
        0x1000,             # SizeOfStackCommit
        0x10_0000,          # SizeOfHeapReserve
        0x1000,             # SizeOfHeapCommit
        0,                  # LoaderFlags
        16,                 # NumberOfRvaAndSizes
    )
    # Data directories (16 entries * 8 bytes)
    optional += b"\x00" * (16 * 8)
    assert len(optional) == 240, f"optional header size {len(optional)}"

    # COFF header (20 bytes)
    coff = struct.pack(
        "<HHIIIHH",
        0x8664,            # Machine: AMD64
        1,                 # NumberOfSections
        0,                 # TimeDateStamp
        0,                 # PointerToSymbolTable
        0,                 # NumberOfSymbols
        len(optional),     # SizeOfOptionalHeader
        0x22,              # Characteristics
    )

    # Section table entry for .text (40 bytes)
    section = struct.pack(
        "<8sIIIIIIHHI",
        b".text\x00\x00\x00",  # Name
        SECTION_ALIGNMENT,      # VirtualSize
        TEXT_RVA,               # VirtualAddress
        TEXT_SIZE_RAW,          # SizeOfRawData
        TEXT_FILE_OFFSET,       # PointerToRawData
        0,                      # PointerToRelocations
        0,                      # PointerToLinenumbers
        0,                      # NumberOfRelocations
        0,                      # NumberOfLinenumbers
        0x6000_0020,            # Characteristics
    )
    assert len(section) == 40, f"section entry size {len(section)}"

    # Header bytes up through the section table.
    header = bytes(dos) + pe_sig + coff + optional + section
    assert len(header) == 0x148 + 40, f"header size {len(header)}"

    # Pad header to the file alignment used for the .text raw offset.
    padding_needed = TEXT_FILE_OFFSET - len(header)
    if padding_needed < 0:
        raise RuntimeError(f"header larger than TEXT_FILE_OFFSET: {len(header)}")
    pe = header + b"\x00" * padding_needed + text_bytes
    return pe


def main():
    text = build_code()
    pe = build_pe(text)

    out = Path(__file__).parent.parent / "kernel" / "src" / "win32" / "minimal_pe64.bin"
    out.write_bytes(pe)
    print(f"Wrote {len(pe)} bytes to {out}")


if __name__ == "__main__":
    main()
