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
Ferris consists of two concrete hierarchical timer wheels. Each has multiple inner wheels with
different resolutions to provide a large time range with minimal memory use.

There is an [allocating
wheel](https://github.com/andrewjstone/ferris/blob/master/src/alloc_wheel.rs) that allocates each
timer on the heap and a [copying
wheel](https://github.com/andrewjstone/ferris/blob/master/src/copy_wheel.rs) that doesn't. Which one
you use is simply a matter of preference and benchmarking in your specific application.

Start and Stop are O(1) operations. Expiry is O(n) on the number of elements in the slot.
