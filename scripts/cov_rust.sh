#!/bin/sh

# cargo llvm-cov
# cargo llvm-cov --html
cargo llvm-cov --lcov --output-path lcov.info

# CARGO_INCREMENTAL=0 RUSTFLAGS='-Cinstrument-coverage' LLVM_PROFILE_FILE='cargo-test-%p-%m.profraw' cargo test
# llvm-profdata merge -sparse cargo-test-*.profraw -o tinyproto.profdata
# grcov . --binary-path ./target/debug/deps/ -s . -t html --branch --ignore-not-existing --ignore '../*' --ignore "/*" -o target/coverage/html
# grcov . --binary-path ./target/debug/deps/ -s . -t lcov --branch --ignore-not-existing --ignore '../*' --ignore "/*" -o target/coverage/tests.lcov
