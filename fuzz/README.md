# `fuzz`

This crates provides fuzz testes for yuv crates.

## Usage

Install [`cargo-afl`] and run this command from root of the project:

``` sh
cargo afl fuzz -i fuzz/in -o fuzz/out target/debug/fuzz
```

[`cargo-afl`]: https://rust-fuzz.github.io/book/cargo-fuzz.html
