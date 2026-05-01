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
    QEMU="qemu-system-x86_64"
    QEMU_ARGS="-cpu qemu64,+apic,+pae -machine q35 -m 256M -serial stdio -vga std -drive format=raw,file=build/vibe-os-x86_64.img"
elif [ "$TARGET_ARCH" = "aarch64" ]; then
    RUST_TARGET="aarch64-unknown-none"
    QEMU="qemu-system-aarch64"
    QEMU_ARGS="-machine virt -cpu cortex-a72 -m 256M -serial stdio -device virtio-gpu-pci -kernel build/vibe-os-aarch64.img"
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
cargo build --target "${RUST_TARGET}" --release --package bootloader

# Link into a flat binary or disk image
echo "[3/4] Creating disk image..."
# Placeholder: just concatenate bootloader + kernel for now
# A proper linker script / objcopy step would go here
BOOTLOADER_ELF="${ROOT_DIR}/target/${RUST_TARGET}/release/bootloader"
KERNEL_ELF="${ROOT_DIR}/target/${RUST_TARGET}/release/kernel"

# For now: use objcopy to create flat binaries
rustc --print sysroot 2>/dev/null || true
LLVM_PREFIX=$(rustc --print sysroot)/lib/rustlib/x86_64-unknown-linux-gnu/bin/
if [ -f "${LLVM_PREFIX}llvm-objcopy" ]; then
    OBJCOPY="${LLVM_PREFIX}llvm-objcopy"
elif command -v llvm-objcopy >/dev/null 2>&1; then
    OBJCOPY="llvm-objcopy"
elif command -v objcopy >/dev/null 2>&1; then
    OBJCOPY="objcopy"
else
    echo "ERROR: no objcopy found"; exit 1
fi

"${OBJCOPY}" -O binary "${BOOTLOADER_ELF}" "${ROOT_DIR}/build/bootloader.bin" || true
"${OBJCOPY}" -O binary "${KERNEL_ELF}" "${ROOT_DIR}/build/kernel.bin" || true

# Create a simple disk image: 512-byte bootloader sector + kernel after 1MB mark
# This is simplified — a real image would use a proper partition table.
if [ -f "${ROOT_DIR}/build/bootloader.bin" ] && [ -f "${ROOT_DIR}/build/kernel.bin" ]; then
    IMG="${ROOT_DIR}/build/vibe-os-${TARGET_ARCH}.img"
    dd if=/dev/zero of="${IMG}" bs=1M count=8 2>/dev/null
    dd if="${ROOT_DIR}/build/bootloader.bin" of="${IMG}" conv=notrunc bs=512 seek=0 2>/dev/null
    dd if="${ROOT_DIR}/build/kernel.bin" of="${IMG}" conv=notrunc bs=512 seek=2048 2>/dev/null
    echo "Disk image created: ${IMG}"
else
    echo "WARNING: Missing bootloader.bin or kernel.bin, skipping image creation."
fi

# Run in QEMU if requested
if [ "$QEMU_RUN" = "1" ]; then
    echo "[4/4] Launching QEMU..."
    ${QEMU} ${QEMU_ARGS}
fi

echo "== Build complete =="
