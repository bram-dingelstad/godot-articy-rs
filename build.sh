#!/bin/bash
export C_INCLUDE_PATH=$C_INCLUDE_PATH:/Library/Developer/CommandLineTools/SDKs/MacOSX14.0.sdk/usr/include

# cargo clean
cargo build --target aarch64-apple-darwin --release
cargo build --target x86_64-apple-darwin --release
cargo build --target x86_64-pc-windows-gnu --release
mkdir -p target/universal-apple-darwin/release

lipo \
  -create \
  -output target/universal-apple-darwin/release/libgodot_articy.dylib \
  target/x86_64-apple-darwin/release/libgodot_articy.dylib \
  target/aarch64-apple-darwin/release/libgodot_articy.dylib
echo built universal!
