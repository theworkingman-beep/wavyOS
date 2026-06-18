#!/usr/bin/env bash
set -euo pipefail

# Build a bootable AArch64 UEFI ISO using the Limine bootloader.
#
# Usage: aarch64-bootimage.sh <kernel-elf> <output-iso>

KERNEL_ELF="$1"
OUTPUT_ISO="$2"
LIMINE_VERSION="12.3.3"
LIMINE_DIR="tools/limine-cache"
LIMINE_URL="https://github.com/limine-bootloader/limine/releases/download/v${LIMINE_VERSION}/limine-binary.tar.xz"

for tool in curl tar xorriso; do
    if ! command -v "$tool" > /dev/null 2>&1; then
        echo "Error: required tool '$tool' is not installed." >&2
        exit 1
    fi
done

CURL="curl -fsSL --max-time 120 --retry 3 --retry-delay 2"

if [[ ! -f "$KERNEL_ELF" ]]; then
    echo "Error: kernel ELF not found: $KERNEL_ELF" >&2
    exit 1
fi

mkdir -p "$LIMINE_DIR"

# Download the Limine binary release if we don't already have it.
if [[ ! -f "$LIMINE_DIR/BOOTAA64.EFI" ]]; then
    echo "Downloading Limine v${LIMINE_VERSION} binaries..."
    $CURL "$LIMINE_URL" | tar -xJf - -C "$LIMINE_DIR" --strip-components=1
fi

STAGE_DIR="target/aarch64-iso-staging"
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR/EFI/BOOT"

cp "$LIMINE_DIR/BOOTAA64.EFI" "$STAGE_DIR/EFI/BOOT/"
cp "$LIMINE_DIR/limine-uefi-cd.bin" "$STAGE_DIR/"
cp tools/limine.conf "$STAGE_DIR/limine.conf"
cp "$KERNEL_ELF" "$STAGE_DIR/kernel.elf"

echo "Building AArch64 UEFI ISO: $OUTPUT_ISO"
xorriso -as mkisofs \
    -r -V 'Aperture OS AArch64' \
    -o "$OUTPUT_ISO" \
    -b limine-uefi-cd.bin \
    -no-emul-boot -boot-load-size 4 \
    -eltorito-platform efi \
    "$STAGE_DIR"

rm -rf "$STAGE_DIR"
echo "Done: $OUTPUT_ISO"
