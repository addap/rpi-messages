.DEFAULT_GOAL := flash_program

BINARY = ./target/thumbv6m-none-eabi/release/rpi-messages-pico
UF2 = $(BINARY).uf2
ELF2UF2 = /home/adrian/programming/rpi_pico_test/pico-sdk/build/elf2uf2/elf2uf2
MOUNT = /run/media/adrian/RPI-RP2

flash_program:
	cargo build --release
	$(ELF2UF2) $(BINARY) $(UF2)
	@cp $(UF2) $(MOUNT)