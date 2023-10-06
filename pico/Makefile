.DEFAULT_GOAL := release

BINARY = ./target/thumbv6m-none-eabi/release/rpi-messages-pico
UF2 = $(BINARY).uf2
ELF2UF2 = elf2uf2-rs
MOUNT = /run/media/adrian/RPI-RP2

.PHONY: release deploy

release:
	cargo build --release
	$(ELF2UF2) $(BINARY) $(UF2)
	@cp $(UF2) $(MOUNT)

deploy:
	cargo build --release --features deploy
	$(ELF2UF2) $(BINARY) $(UF2)
	@cp $(UF2) $(MOUNT)