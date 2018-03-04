all: core platforms test

core: platforms
	cargo build --release

platforms:
	./platforms/build.sh

test: core platforms
	cargo test --release

clean:
	./platforms/clean.sh
	cargo clean

.PHONY: core platforms test clean
