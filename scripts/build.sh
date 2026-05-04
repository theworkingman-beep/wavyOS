#!/usr/bin/env bash
set -e
export HERMITOS="vibe-coded-os"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Ensure Rust toolchain is on PATH
export PATH="$HOME/.cargo/bin:$HOME/.rustup/toolchains/nightly-aarch64-unknown-linux-gnu/bin:$PATH"

TARGET_ARCH="${TARGET_ARCH:-x86_64}"
MODE="${MODE:-release}"
QEMU_RUN="${QEMU_RUN:-0}"
BUILD_ISO="${BUILD_ISO:-1}"

if [ "$TARGET_ARCH" = "x86_64" ]; then
    RUST_TARGET="x86_64-unknown-none"
    UEFI_TARGET="x86_64-unknown-uefi"
    EFI_NAME="BOOTX64.EFI"
    QEMU="qemu-system-x86_64"
    QEMU_ARGS="-cpu qemu64,+apic,+pae -machine q35 -m 256M -serial stdio -drive format=raw,file=build/vibe-os-x86_64.img"
    GRUB_ARCH="x86_64-efi"
elif [ "$TARGET_ARCH" = "aarch64" ]; then
    RUST_TARGET="aarch64-unknown-none"
    UEFI_TARGET="aarch64-unknown-uefi"
    EFI_NAME="BOOTAA64.EFI"
    QEMU="qemu-system-aarch64"
    QEMU_ARGS="-machine virt -cpu cortex-a72 -m 256M -serial stdio -device virtio-gpu-pci -drive format=raw,file=build/vibe-os-aarch64.img"
    GRUB_ARCH="arm64-efi"
else
    echo "Unsupported arch: $TARGET_ARCH"; exit 1
fi

echo "== Building $HERMITOS for $TARGET_ARCH ($MODE) =="
mkdir -p "${ROOT_DIR}/build"

cd "${ROOT_DIR}"

echo "[1/5] Building kernel..."
cargo build --target "${RUST_TARGET}" --release --package kernel

echo "[2/5] Building bootloader..."
cargo build --target "${UEFI_TARGET}" --release --package bootloader

echo "[2b/5] Building user-space apps..."
APPS_TARGET="${ROOT_DIR}/targets/vibeos-${TARGET_ARCH}.json"
for app_dir in libvibe windowserver desktop_shell sample_app; do
    if [ -d "${ROOT_DIR}/apps/${app_dir}" ]; then
        cargo +nightly build -Zbuild-std=core,alloc -Zjson-target-spec --target "${APPS_TARGET}" --manifest-path "${ROOT_DIR}/apps/${app_dir}/Cargo.toml" --release 2>&1 | grep -v "^warning:" || true
    fi
done

BOOTLOADER_EFI="${ROOT_DIR}/target/${UEFI_TARGET}/release/bootloader.efi"
KERNEL_ELF="${ROOT_DIR}/target/${RUST_TARGET}/release/kernel"
IMG="${ROOT_DIR}/build/vibe-os-${TARGET_ARCH}.img"
ISO="${ROOT_DIR}/build/vibe-os-${TARGET_ARCH}.iso"

echo "[3/5] Creating disk image..."
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
    for app in windowserver desktop_shell sample_app; do
        APP_ELF="${ROOT_DIR}/target/vibeos-${TARGET_ARCH}/release/${app}"
        if [ -f "${APP_ELF}" ]; then
            mcopy -i "${IMG}" "${APP_ELF}" "::/${app}" >/dev/null 2>&1 || true
        fi
    done
    echo "EFI disk image: ${IMG}"
else
    dd if=/dev/zero of="${IMG}" bs=1M count=8 2>/dev/null || true
    echo "Raw disk image: ${IMG} (no mtools)"
fi

echo "[4/5] Creating bootable ISO..."
if [ "$BUILD_ISO" = "1" ] && command -v grub-mkrescue >/dev/null 2>&1; then
    ISO_TMP="${ROOT_DIR}/build/iso_tmp"
    rm -rf "${ISO_TMP}"
    mkdir -p "${ISO_TMP}/EFI/BOOT" "${ISO_TMP}/boot/grub" "${ISO_TMP}/vibeos"
    if [ -f "${BOOTLOADER_EFI}" ]; then
        cp "${BOOTLOADER_EFI}" "${ISO_TMP}/EFI/BOOT/${EFI_NAME}"
    fi
    if [ -f "${KERNEL_ELF}" ]; then
        cp "${KERNEL_ELF}" "${ISO_TMP}/vibeos/kernel"
    fi
    cat > "${ISO_TMP}/boot/grub/grub.cfg" << GRUBEOF
set timeout=0
set default=0

menuentry "Vibe Coded OS" {
    chainloader /EFI/BOOT/${EFI_NAME}
}
GRUBEOF
    grub-mkrescue -o "${ISO}" "${ISO_TMP}" >/dev/null 2>&1 || true
    rm -rf "${ISO_TMP}"
    if [ -f "${ISO}" ]; then
        echo "Bootable ISO: ${ISO}"
    else
        echo "ISO creation failed"
    fi
else
    echo "Skipping ISO (grub-mkrescue unavailable)"
fi

if [ "$QEMU_RUN" = "1" ]; then
    echo "[5/5] Launching QEMU..."
    ${QEMU} ${QEMU_ARGS}
else
    echo "[5/5] QEMU skipped (set QEMU_RUN=1 to run)"
fi

echo "== Build complete =="
