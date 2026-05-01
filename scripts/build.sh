#!/usr/bin/env bash
set -e
export HERMITOS="vibe-coded-os"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

TARGET_ARCH="${TARGET_ARCH:-x86_64}"
MODE="${MODE:-release}"
QEMU_RUN="${QEMU_RUN:-0}"

if [ "$TARGET_ARCH" = "x86_64" ]; then
    RUST_TARGET="x86_64-unknown-none"
    UEFI_TARGET="x86_64-unknown-uefi"
    EFI_NAME="BOOTX64.EFI"
    QEMU="qemu-system-x86_64"
    QEMU_ARGS="-cpu qemu64,+apic,+pae -machine q35 -m 256M -serial stdio -drive format=raw,file=build/vibe-os-x86_64.img"
elif [ "$TARGET_ARCH" = "aarch64" ]; then
    RUST_TARGET="aarch64-unknown-none"
    UEFI_TARGET="aarch64-unknown-uefi"
    EFI_NAME="BOOTAA64.EFI"
    QEMU="qemu-system-aarch64"
    QEMU_ARGS="-machine virt -cpu cortex-a72 -m 256M -serial stdio -device virtio-gpu-pci -drive format=raw,file=build/vibe-os-aarch64.img"
else
    echo "Unsupported arch: $TARGET_ARCH"; exit 1
fi

echo "== Building $HERMITOS for $TARGET_ARCH ($MODE) =="
mkdir -p "${ROOT_DIR}/build"

cd "${ROOT_DIR}"

# Build kernel
echo "[1/4] Building kernel..."
cargo build --target "${RUST_TARGET}" --release --package kernel

# Build bootloader
echo "[2/4] Building bootloader..."
cargo build --target "${UEFI_TARGET}" --release --package bootloader

# Create disk image
echo "[3/4] Creating disk image..."
BOOTLOADER_EFI="${ROOT_DIR}/target/${UEFI_TARGET}/release/bootloader.efi"
KERNEL_ELF="${ROOT_DIR}/target/${RUST_TARGET}/release/kernel"

IMG="${ROOT_DIR}/build/vibe-os-${TARGET_ARCH}.img"

# Create EFI FAT32 image when tooling is available
if command -v mkfs.fat >/dev/null 2>&1 && command -v mcopy >/dev/null 2>&1 && command -v mmd >/dev/null 2>&1; then
    dd if=/dev/zero of="${IMG}" bs=1M count=64 2>/dev/null || true
    mkfs.fat -F 32 -n VIBEOS "${IMG}" >/dev/null 2>&1 || true
    mmd -i "${IMG}" ::/EFI >/dev/null 2>&1 || true
    mmd -i "${IMG}" ::/EFI/BOOT >/dev/null 2>&1 || true
    if [ -f "${BOOTLOADER_EFI}" ]; then
        mcopy -i "${IMG}" "${BOOTLOADER_EFI}" "::/EFI/BOOT/${EFI_NAME}" >/dev/null 2>&1 || true
    fi
    if [ -f "${KERNEL_ELF}" ]; then
        mcopy -i "${IMG}" "${KERNEL_ELF}" ::/kernel >/dev/null 2>&1 || true
    fi
    echo "EFI disk image created: ${IMG}"
else
    # Fallback raw image for environments without mtools
    if command -v llvm-objcopy >/dev/null 2>&1; then
        OBJCOPY="llvm-objcopy"
    elif command -v objcopy >/dev/null 2>&1; then
        OBJCOPY="objcopy"
    else
        echo "WARNING: no objcopy for raw fallback"
        OBJCOPY=""
    fi
    if [ -n "$OBJCOPY" ]; then
        "$OBJCOPY" -O binary "${BOOTLOADER_EFI}" "${ROOT_DIR}/build/bootloader.bin" || true
        "$OBJCOPY" -O binary "${KERNEL_ELF}" "${ROOT_DIR}/build/kernel.bin" || true
    fi
    dd if=/dev/zero of="${IMG}" bs=1M count=8 2>/dev/null || true
    if [ -f "${ROOT_DIR}/build/bootloader.bin" ]; then
        dd if="${ROOT_DIR}/build/bootloader.bin" of="${IMG}" conv=notrunc bs=512 seek=0 2>/dev/null || true
    fi
    if [ -f "${ROOT_DIR}/build/kernel.bin" ]; then
        dd if="${ROOT_DIR}/build/kernel.bin" of="${IMG}" conv=notrunc bs=512 seek=2048 2>/dev/null || true
    fi
    echo "Raw disk image created: ${IMG} (EFI fallback)"
fi

# Run in QEMU if requested
if [ "$QEMU_RUN" = "1" ]; then
    echo "[4/4] Launching QEMU..."
    ${QEMU} ${QEMU_ARGS}
fi

echo "== Build complete =="
