# hreinsa

`hreinsa` (Icelandic for *to clean/sanitize/purify*) is a Rust port of [OpenType Sanitizer (OTS)](https://github.com/khaledhosny/ots), built on top of [`read-fonts`](https://crates.io/crates/read-fonts) and [`write-fonts`](https://crates.io/crates/write-fonts).

It parses, validates, sanitizes, and serializes untrusted OpenType font files to ensure they are safe for system font rasterizers.
