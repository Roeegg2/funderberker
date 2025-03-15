TEST_CRATES := utils
RUST_PROFILE := debug
QEMU := qemu-system-x86_64 \
		-nographic \
		-nodefaults \
		-serial stdio \
		-no-reboot \
		-drive if=pflash,unit=0,format=raw,file=ovmf/ovmf-code-x86_64.fd,readonly=on \
		-drive if=pflash,unit=1,format=raw,file=ovmf/ovmf-vars-x86_64.fd \
		-cdrom funderberker.iso

# Actually build Funderberker
.PHONY: funderberker
funderberker:
ifeq ($(RUST_PROFILE), debug)
	cd kernel && RUSTFLAGS="-C relocation-model=static" cargo +nightly build --target x86_64-unknown-none
else ifeq ($(RUST_PROFILE), release)
	cd kernel && RUSTFLAGS="-C relocation-model=static" cargo +nightly build --release --target x86_64-unknown-none
else
	exit
endif
	cp kernel/target/x86_64-unknown-none/$(RUST_PROFILE)/kernel funderberker

.PHONY: build
build: funderberker.iso ovmf/ovmf-code-x86_64.fd ovmf/ovmf-vars-x86_64.fd

# Build & run with QEMU
.PHONY: run
run: funderberker.iso ovmf/ovmf-code-x86_64.fd ovmf/ovmf-vars-x86_64.fd
	$(QEMU)

# Build & run with QEMU logging stuff
.PHONY: debug
debug: funderberker.iso ovmf/ovmf-code-x86_64.fd ovmf/ovmf-vars-x86_64.fd
	$(QEMU) -d in_asm,int -D qemu.log

# unit test
.PHONY: test
test: 
	for crate in $(TEST_CRATES); do \
		(cd $$crate && cargo test) \
	done

# Clean everything
.PHONY: clean
clean:
	rm funderberker
	rm funderberker.iso
	rm -rf iso_root
	cd kernel && cargo clean

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
funderberker.iso: funderberker limine/limine
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
		iso_root -o funderberker.iso
	./limine/limine bios-install funderberker.iso
