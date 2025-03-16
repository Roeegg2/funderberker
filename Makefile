# Configuration variables
TEST_CRATES := utils
RUST_PROFILE := debug
TEST_FEATURE := true
# Directory and file paths
KERNEL_DIR := kernel
ISO_ROOT := iso_root
OVMF_DIR := ovmf
LIMINE_DIR := limine
ISO_FILE := funderberker.iso
KERNEL_BIN := funderberker
# File paths
OVMF_CODE := $(OVMF_DIR)/ovmf-code-x86_64.fd
OVMF_VARS := $(OVMF_DIR)/ovmf-vars-x86_64.fd
LIMINE_BINARY := $(LIMINE_DIR)/limine
# Build output paths
KERNEL_TARGET_DIR := $(KERNEL_DIR)/target/x86_64-unknown-none/$(RUST_PROFILE)
KERNEL_OUTPUT := $(KERNEL_TARGET_DIR)/kernel
# QEMU settings
QEMU := qemu-system-x86_64 \
		-nographic \
		-nodefaults \
		-serial stdio \
		-no-reboot \
		-drive if=pflash,unit=0,format=raw,file=$(OVMF_CODE),readonly=on \
		-drive if=pflash,unit=1,format=raw,file=$(OVMF_VARS) \
		-cdrom $(ISO_FILE)
# Rust flags
RUSTFLAGS := -C relocation-model=static

# Define all phony targets
.PHONY: all help build run debug test clean funderberker $(KERNEL_OUTPUT)

# Default target
all: build

# Help message
help:
	@echo "Funderberker OS Build System"
	@echo ""
	@echo "Available targets:"
	@echo "  all       - Build the kernel and create bootable ISO (default)"
	@echo "  build     - Same as 'all'"
	@echo "  run       - Build and run in QEMU"
	@echo "  debug     - Build and run in QEMU with debug output"
	@echo "  test      - Run unit tests"
	@echo "  clean     - Remove all build artifacts"
	@echo ""
	@echo "Configuration:"
	@echo "  RUST_PROFILE=debug|release (current: $(RUST_PROFILE))"
	@echo "  TEST_CRATES='$(TEST_CRATES)'"

# Build the kernel
funderberker: $(KERNEL_OUTPUT)
	cp $< $(KERNEL_BIN)
$(KERNEL_OUTPUT):
ifeq ($(RUST_PROFILE), debug)
	cd $(KERNEL_DIR) && RUSTFLAGS="$(RUSTFLAGS)" cargo +nightly build $(if $(filter true,$(TEST_FEATURE)),--features test,) --target x86_64-unknown-none
else ifeq ($(RUST_PROFILE), release)
	cd $(KERNEL_DIR) && RUSTFLAGS="$(RUSTFLAGS)" cargo +nightly build --release $(if $(filter true,$(TEST_FEATURE)),--features test,) --target x86_64-unknown-none
else
	@echo "Error: Invalid RUST_PROFILE '$(RUST_PROFILE)'. Must be 'debug' or 'release'."
	@exit 1
endif

# Build everything
build: $(ISO_FILE)

# Run in QEMU
run: $(ISO_FILE) $(OVMF_CODE) $(OVMF_VARS)
	$(QEMU)

# Run with debugging enabled
debug: $(ISO_FILE) $(OVMF_CODE) $(OVMF_VARS)
	$(QEMU) -d in_asm,int -D qemu.log

# Run unit tests
test:
	@for crate in $(TEST_CRATES); do \
		echo "Testing $$crate..."; \
		(cd $$crate && cargo test) || exit 1; \
	done

# Clean build artifacts
clean:
	-rm -f $(KERNEL_BIN)
	-rm -f $(ISO_FILE)
	-rm -rf $(ISO_ROOT)
	-cd $(KERNEL_DIR) && cargo clean
# Download UEFI firmware files
$(OVMF_CODE):
	mkdir -p $(OVMF_DIR)
	curl -Lo $@ https://github.com/osdev0/edk2-ovmf-nightly/releases/latest/download/ovmf-code-x86_64.fd
$(OVMF_VARS):
	mkdir -p $(OVMF_DIR)
	curl -Lo $@ https://github.com/osdev0/edk2-ovmf-nightly/releases/latest/download/ovmf-vars-x86_64.fd
# Clone and build Limine bootloader
$(LIMINE_BINARY):
	rm -rf $(LIMINE_DIR)
	git clone https://github.com/limine-bootloader/limine.git --branch=v8.x-binary --depth=1 $(LIMINE_DIR)
	$(MAKE) -C $(LIMINE_DIR)
# Create bootable ISO
$(ISO_FILE): $(KERNEL_BIN) $(LIMINE_BINARY)
	rm -rf $(ISO_ROOT)
	mkdir -p $(ISO_ROOT)/boot
	cp -v $(KERNEL_BIN) $(ISO_ROOT)/boot/
	mkdir -p $(ISO_ROOT)/boot/limine
	cp -v limine.conf $(ISO_ROOT)/boot/limine/
	mkdir -p $(ISO_ROOT)/EFI/BOOT
	cp -v $(LIMINE_DIR)/limine-bios.sys $(LIMINE_DIR)/limine-bios-cd.bin $(LIMINE_DIR)/limine-uefi-cd.bin $(ISO_ROOT)/boot/limine/
	cp -v $(LIMINE_DIR)/BOOTX64.EFI $(ISO_ROOT)/EFI/BOOT/
	cp -v $(LIMINE_DIR)/BOOTIA32.EFI $(ISO_ROOT)/EFI/BOOT/
	xorriso -as mkisofs -b boot/limine/limine-bios-cd.bin \
		-no-emul-boot -boot-load-size 4 -boot-info-table \
		--efi-boot boot/limine/limine-uefi-cd.bin \
		-efi-boot-part --efi-boot-image --protective-msdos-label \
		$(ISO_ROOT) -o $@
	./$(LIMINE_DIR)/limine bios-install $@
