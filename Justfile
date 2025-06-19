# Configuration variables
test-crates := "utils"
rust-profile := "debug"

# Directory and file paths
kernel-dir := "kernel"
iso-root := "iso_root"
ovmf-dir := "ovmf"
limine-dir := "limine"
iso-file := "funderberker.iso"
kernel-bin := "funderberker"

# File paths
ovmf-code := ovmf-dir + "/ovmf-code-x86_64.fd"
ovmf-vars := ovmf-dir + "/ovmf-vars-x86_64.fd"
limine-binary := limine-dir + "/limine"

# Rust flags
rustflags := "-C relocation-model=static"
features := "default"

# Default recipe (runs when just is called without arguments)
default: build

# Help message
help:
    @echo "Funderberker OS Build System"
    @echo ""
    @echo "Available recipes:"
    @echo "  default   - Build the kernel and create bootable ISO"
    @echo "  build     - Same as 'default'"
    @echo "  run       - Build and run in QEMU"
    @echo "  debug     - Build and run in QEMU with debug output"
    @echo "  test      - Build and run kernel tests in QEMU"
    @echo "  crates-test      - Run other crates' unit tests"
    @echo "  clean     - Remove all build artifacts"
    @echo "  media     - Write the compiled ISO to a USB device"
    @echo "  media-test - Write the test compiled ISO to a USB device"
    @echo ""
    @echo "Configuration:"
    @echo "  rust-profile=debug|release (current: {{rust-profile}})"
    @echo "  test-crates='{{test-crates}}'"

# Build the kernel based on profile
build-kernel:
    #!/usr/bin/env bash
    if [ "{{rust-profile}}" = "debug" ]; then
        RUSTFLAGS="{{rustflags}} -g" cargo +nightly build --manifest-path="kernel/Cargo.toml" --features {{features}} --target x86_64-unknown-none
    elif [ "{{rust-profile}}" = "release" ]; then
        RUSTFLAGS="{{rustflags}}" cargo +nightly build --manifest-path="kernel/Cargo.toml" --release --features {{features}} --target x86_64-unknown-none
    else
        echo "Error: Invalid rust-profile '{{rust-profile}}'. Must be 'debug' or 'release'."
        exit 1
    fi
    BIN=`find kernel/target/x86_64-unknown-none -type f -executable -name "kernel" | head -n 1`
    cp $BIN {{kernel-bin}}

# Build the kernel tests
build-kernel-test: clean
    #!/usr/bin/env bash
    pwd
    if [ "{{rust-profile}}" = "debug" ]; then
        RUSTFLAGS="{{rustflags}}" cargo +nightly test --manifest-path="kernel/Cargo.toml" --features {{features}} --no-run --target x86_64-unknown-none
    elif [ "{{rust-profile}}" = "release" ]; then
        RUSTFLAGS="{{rustflags}}" cargo +nightly test --manifest-path="kernel/Cargo.toml" --features {{features}} --no-run --release --target x86_64-unknown-none
    else
        echo "Error: Invalid rust-profile '{{rust-profile}}'. Must be 'debug' or 'release'."
        exit 1
    fi
    BIN=`find kernel/target/x86_64-unknown-none -type f -executable -name "kernel-*" | head -n 1`
    cp $BIN {{kernel-bin}}

# Run crate tests
crates-test:
    #!/usr/bin/env bash
    for crate in {{test-crates}}; do
        cargo test --manifest-path="$crate/Cargo.toml"
    done

build-test: build-kernel-test  _create-iso-common

# Build everything
build: build-kernel _create-iso-common

# Run kernel tests and launch in QEMU
test: build-test
    @just _run-qemu

# Run in QEMU
run: build
    @just _run-qemu

# Run with debugging enabled
debug: build-test
    @just _run-qemu-debug

# Write a test compiled ISO to a USB device
media-test: build-test _media

# Write the compiled ISO to a USB device
media: build _media

# Helper recipe for writing ISO to USB device
_media:
  #!/usr/bin/env bash
  lsblk
  read -p "Enter device (in the format /dev/<...>):" DEVICE
  while true; do
    read -p "WARNING - everything on '$DEVICE' WILL be deleted. Are you OK with that? (y/n)" yn
    case $yn in
      [Yy]* ) break;;
      [Nn]* ) echo "Exiting..."; exit;;
      * ) echo "Please answer yes or no.";;
    esac
  done
  sudo dd if={{iso-file}} of=$DEVICE bs=4M status=progress oflag=sync

# Helper recipe for running QEMU
_run-qemu: _download-firmware
    qemu-system-x86_64 \
        -machine q35 \
        -vga virtio \
        -nodefaults \
        -serial stdio \
        -no-reboot \
        -drive if=pflash,unit=0,format=raw,file={{ovmf-code}},readonly=on \
        -drive if=pflash,unit=1,format=raw,file={{ovmf-vars}} \
        -cdrom {{iso-file}}

# Helper recipe for running QEMU with debug
#
# Add `-s -S` for debugging with GDB
_run-qemu-debug: _download-firmware
    qemu-system-x86_64 \
        -machine q35 \
        -vga virtio \
        -nodefaults \
        -serial stdio \
        -no-reboot \
        -drive if=pflash,unit=0,format=raw,file={{ovmf-code}},readonly=on \
        -drive if=pflash,unit=1,format=raw,file={{ovmf-vars}} \
        -cdrom {{iso-file}} \
        -d in_asm,int -D qemu.log \
        -D qemu.log \

# Common ISO creation steps
_create-iso-common: _setup-limine
    rm -rf {{iso-root}}
    mkdir -p {{iso-root}}/boot
    cp -v {{kernel-bin}} {{iso-root}}/boot/
    # rm {{kernel-bin}}
    mkdir -p {{iso-root}}/boot/limine
    cp -v limine.conf {{iso-root}}/boot/limine/
    mkdir -p {{iso-root}}/EFI/BOOT
    cp -v {{limine-dir}}/limine-bios.sys {{limine-dir}}/limine-bios-cd.bin {{limine-dir}}/limine-uefi-cd.bin {{iso-root}}/boot/limine/
    cp -v {{limine-dir}}/BOOTX64.EFI {{iso-root}}/EFI/BOOT/
    cp -v {{limine-dir}}/BOOTIA32.EFI {{iso-root}}/EFI/BOOT/
    xorriso -as mkisofs -b boot/limine/limine-bios-cd.bin \
        -no-emul-boot -boot-load-size 4 -boot-info-table \
        --efi-boot boot/limine/limine-uefi-cd.bin \
        -efi-boot-part --efi-boot-image --protective-msdos-label \
        {{iso-root}} -o {{iso-file}}
    ./{{limine-dir}}/limine bios-install {{iso-file}}

# Clean build artifacts
clean:
    -rm -f {{kernel-bin}}
    -rm -f {{iso-file}}
    -rm -rf {{iso-root}}
    cargo clean --manifest-path=kernel/Cargo.toml

# Download UEFI firmware files
_download-firmware:
    #!/usr/bin/env bash
    if [ ! -f "{{ovmf-code}}" ]; then
        mkdir -p {{ovmf-dir}}
        curl -Lo {{ovmf-code}} https://github.com/osdev0/edk2-ovmf-nightly/releases/latest/download/ovmf-code-x86_64.fd
    fi
    if [ ! -f "{{ovmf-vars}}" ]; then
        mkdir -p {{ovmf-dir}}
        curl -Lo {{ovmf-vars}} https://github.com/osdev0/edk2-ovmf-nightly/releases/latest/download/ovmf-vars-x86_64.fd
    fi

# Clone and build Limine bootloader
_setup-limine:
    #!/usr/bin/env bash
    if [ ! -f "{{limine-binary}}" ]; then
        rm -rf {{limine-dir}}
        git clone https://github.com/limine-bootloader/limine.git --branch=v8.x-binary --depth=1 {{limine-dir}}
        make -C {{limine-dir}}
    fi
