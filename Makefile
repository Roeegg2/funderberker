RUST_PROFILE := debug
IMAGE_NAME := funderberker
QEMU := qemu-system-x86_64 -m 4G \
		-nographic \
		-nodefaults \
		-serial stdio \
		-no-reboot \
		-drive if=pflash,unit=0,format=raw,file=ovmf/ovmf-code-x86_64.fd,readonly=on \
		-drive if=pflash,unit=1,format=raw,file=ovmf/ovmf-vars-x86_64.fd \
		-cdrom $(IMAGE_NAME).iso

# Actually build Funderberker
.PHONY: funderberker
funderberker:
ifeq ($(RUST_PROFILE), debug)
	RUSTFLAGS="-C relocation-model=static" cargo +nightly build --target x86_64-unknown-none
	cp target/x86_64-unknown-none/debug/funderberker funderberker
else
	RUSTFLAGS="-C relocation-model=static" cargo +nightly build --release --target x86_64-unknown-none
	cp target/x86_64-unknown-none/release/funderberker funderberker
endif

.PHONY: build
build: $(IMAGE_NAME).iso ovmf/ovmf-code-x86_64.fd ovmf/ovmf-vars-x86_64.fd

# Build & run with QEMU
.PHONY: run
run: $(IMAGE_NAME).iso ovmf/ovmf-code-x86_64.fd ovmf/ovmf-vars-x86_64.fd
	$(QEMU)

# Build & run with QEMU logging stuff
.PHONY: debug
debug: $(IMAGE_NAME).iso ovmf/ovmf-code-x86_64.fd ovmf/ovmf-vars-x86_64.fd
	$(QEMU) -d in_asm,int -D qemu.log

# Clean everything
.PHONY: clean
clean:
	cargo clean
	rm funderberker

# Getting UEFI firmware code
ovmf/ovmf-code-x86_64.fd:
	mkdir -p ovmf
	curl -Lo $@ https://github.com/osdev0/edk2-ovmf-nightly/releases/latest/download/ovmf-code-x86_64.fd

# Getting UEFI firmware 
ovmf/ovmf-vars-x86_64.fd:
	mkdir -p ovmf
	curl -Lo $@ https://github.com/osdev0/edk2-ovmf-nightly/releases/latest/download/ovmf-vars-x86_64.fd

limine/limine:
	rm -rf limine
	git clone https://github.com/limine-bootloader/limine.git --branch=v8.x-binary --depth=1
	$(MAKE) -C limine

# Create the ISO
$(IMAGE_NAME).iso: funderberker limine/limine
	rm -rf iso_root
	mkdir -p iso_root/boot
	cp -v funderberker iso_root/boot/
	mkdir -p iso_root/boot/limine
	cp -v limine.conf iso_root/boot/limine/
	mkdir -p iso_root/EFI/BOOT
	cp -v limine/limine-bios.sys limine/limine-bios-cd.bin limine/limine-uefi-cd.bin iso_root/boot/limine/
	cp -v limine/BOOTX64.EFI iso_root/EFI/BOOT/
	cp -v limine/BOOTIA32.EFI iso_root/EFI/BOOT/
	xorriso -as mkisofs -b boot/limine/limine-bios-cd.bin \
		-no-emul-boot -boot-load-size 4 -boot-info-table \
		--efi-boot boot/limine/limine-uefi-cd.bin \
		-efi-boot-part --efi-boot-image --protective-msdos-label \
		iso_root -o $(IMAGE_NAME).iso
	./limine/limine bios-install $(IMAGE_NAME).iso
