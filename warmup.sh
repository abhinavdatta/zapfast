#!/bin/bash
export RUSTUP_TOOLCHAIN=1.98.0-x86_64-pc-windows-msvc
CMAKE_DIR="/c/Program Files (x86)/Microsoft Visual Studio/18/BuildTools/Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin"
export PATH="/c/Users/abhi0/.cargo/bin:/c/Users/abhi0/.rustup/toolchains/1.98.0-x86_64-pc-windows-msvc/bin:/c/Users/abhi0/.local/tools/perl/bin:$CMAKE_DIR:$PATH"
cd "$(dirname "$0")"
cargo check --locked --all-targets --all-features > target/check-warmup.log 2>&1
echo "EXIT:$?" >> target/check-warmup.log
