# Fuzz

Fuzzing is generally run by https://github.com/google/oss-fuzz.

To run fuzzing locally:

```shell
# Make sure you have cargo-fuzz and the nightly toolchain
$ cargo install cargo-fuzz
$ rustup install nightly

# Build the fuzzer
$ cargo +nightly  fuzz build -O --debug-assertions
$ target/x86_64-unknown-linux-gnu/release/fuzz_skrifa
```
