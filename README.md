# Stowage

Declarative dependency rules for Cargo workspaces.

Teams that split a Rust service into a workspace of crates usually have rules about which crates may depend on which: the domain stays free of infrastructure, nothing depends on a binary, test harnesses never reach production code. Cargo doesn't enforce any of this. Stowage checks these rules from a `stowage.toml` config, inside `cargo test`.

Stowage is in early development and not yet published.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
