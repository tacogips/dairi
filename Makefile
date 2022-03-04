build:
	cargo build --release
	cp ./target/release/libdairi.so lua/dairi.so

build-dev:
	cargo build
	cp ./target/debug/libdairi.so lua/dairi.so

install: build
	cargo install --force --path .
