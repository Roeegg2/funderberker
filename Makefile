EXECUTABLE = target/x86_64-unknown-uefi/debug/funderberker.efi
EFI_DIR = esp/efi/boot
QEMU = qemu-system-x86_64 -d in_asm,int -D qemu.log -serial stdio -nographic -nodefaults -no-reboot \
  -drive if=pflash,format=raw,readonly=on,file=OVMF_CODE.fd \
  -drive if=pflash,format=raw,readonly=on,file=OVMF_VARS.fd \
  -global isa-debugcon.iobase=0x402 \
  -drive format=raw,file=fat:rw:esp


# change these depending on what hardware you have/what features you want to enable! Check README for more explanation.
FEATURES = amd 

.DEFAULT_GOAL := build

build:
	cargo +nightly build --features $(FEATURES) 
	mkdir -p $(EFI_DIR)
	cp $(EXECUTABLE) $(EFI_DIR)/bootx64.efi

run: build
	$(QEMU)

debug: build
	$(QEMU) -s -S &

media: build
	./media.sh

clean:
	cargo clean
	rm -rf $(EFI_DIR)

