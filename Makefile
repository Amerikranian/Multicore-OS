override STORAGE_NAME := storage_test

.PHONY: check
check:
	echo "Checking clippy" && \
	cargo clippy -- -D warnings --all && \
	echo "Checking formatting" && \
	cargo fmt --check --all

.PHONY: build
build:
	@cargo build --features "strict"

.PHONY: run
run: blank_drive
	@cargo run

.PHONY: run-term
run-term: blank_drive
	@cargo run mode terminal

.PHONY: gdb-term
gdb-term: blank_drive
	@cargo run mode gdb-terminal

.PHONY: gdb-gui
gdb-gui: blank_drive
	@cargo run mode gdb-gui

.PHONY: test
test: blank_drive
		@cd $(shell pwd) && CARGO_TARGET_DIR=$(shell pwd)/target cargo test -p taos

.PHONY: fmt
fmt:
	@cargo fmt --all

.PHONY: objdump
objdump:
	@cargo objdump --lib --release -- -d -M intel

.PHONY: blank_drive
blank_drive:
	@if [ ! -f "$(STORAGE_NAME).img" ]; then \
		dd if=/dev/zero of=$(STORAGE_NAME).img bs=1M count=4k; \
	fi
	@ln -sf $(shell pwd)/$(STORAGE_NAME).img kernel/$(STORAGE_NAME).img

.PHONY: clean
clean:
	@rm -f $(STORAGE_NAME).img
	@cargo clean
