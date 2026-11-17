# CPace-Ristretto255, a balanced PAKE

A CPace implementation for Rust, with `no_std` compatibility.

## About

[CPace](https://tools.ietf.org/id/draft-haase-cpace-06.html) is a protocol for two parties that share a low-entropy secret (password) to derive a strong shared key without disclosing the secret to offline dictionary attacks. This abstraction is commonly known as a PAKE (password-authenticated key exchange). This library currently tracks [revision 6 (2022)](https://datatracker.ietf.org/doc/draft-irtf-cfrg-cpace/06/) of the IETF CPace draft specification.

CPace is a balanced (or symmetrical) PAKE, meaning that both parties must know the low-entropy secret. It was [selected by the CFRG](https://github.com/cfrg/pake-selection) for balanced algorithms.

### Details

This library hardcodes Ristretto255 as the group with SHA-512. This allows the code to benefit from the many security benefits of higher level abstractions, such as safety from insecure input from network-level attackers.

The CPace protocol only requires a single round trip, but this library enforces one additional set of messages in order to perform [key confirmation](https://www.ietf.org/archive/id/draft-irtf-cfrg-cpace-06.html#name-key-confirmation), which mutually proves that each side produced the same output key and wasn't externally influenced or manipulated. This provides a stronger level of baseline security using the protocol in higher level constructions.

### Security

This crate has received a [security audit by Cure53](./assets/2022-Cure53-Audit.pdf), with no significant findings. This work was also funded internally, in order to have an external party verify the correctness of the implementation we (1Password) created.

## Credits

This crate was written by the 1Password Data Security team. However, it originated as a fork of Frank Denis' [rust-cpace](https://github.com/jedisct1/rust-cpace) crate and as such currently shares the same name despite having been entirely rewritten from scratch.

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE.txt">Apache License, Version
2.0</a> or <a href="LICENSE-MIT.txt">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>
