#!/bin/bash
export PATH="/c/Users/abhi0/.cargo/bin:/c/Users/abhi0/.local/tools/perl/perl/bin:/c/Program Files (x86)/Microsoft Visual Studio/18/BuildTools/Common7/IDE/CommonExtensions/Microsoft/CMake/CMake/bin:$PATH"
export RUSTC="/c/Users/abhi0/.rustup/toolchains/1.98.0-x86_64-pc-windows-msvc/bin/rustc.exe"
export OPENSSL_SRC_PERL="/c/Users/abhi0/.local/tools/perl/perl/bin/perl.exe"
cd "/c/Users/abhi0/Desktop/New folder (2)/zapfast"
exec "/c/Users/abhi0/.rustup/toolchains/1.98.0-x86_64-pc-windows-msvc/bin/cargo.exe" check --all-targets --all-features 2>&1
