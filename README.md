[![Build
Status](https://travis-ci.org/andrewjstone/ferris.svg?branch=master)](https://travis-ci.org/andrewjstone/ferris)

[API Documentation](https://docs.rs/ferris)

### Usage

Add the following to your `Cargo.toml`

```toml
[dependencies]
ferris = "0.1"
```

Add this to your crate root

```rust
extern crate ferris;
```

### Description
Ferris is a hierarchical timer wheel. There are multiple inner wheels with different resolutions to provide
a large time range with minimal memory use.

The wheel is optimized for large numbers of timers that are frequently cancelled instead of
expiring. This is useful for handling things like timeouts in distributed protocols. Note that
expiring timers are still very lightweight.

Start and Stop are O(1) operations. Expiry is O(n) on the number of elements in the slot.

