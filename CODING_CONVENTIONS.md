# Coding Conventions

## Naming Conventions

* Methods which take a `Read` instance to construct an asset type should be named `Type::load[_optional_part]`.
* Methods which take an `AsyncRead` instance to construct an asset shoudlbe named `Type::load[_optional_part]_async`.
* Methods which directly construct an assert from a `&[u8]` should be named `Type::from[_optional_part]_bytes`.
* Features are named `riddle_x`, so that they match the riddle crate names.

## Misc

* `load` methods which can accept multiple file types should take an explicit argument with the expected file
    type. This is to not depend on a single crate for all formats, or to allow Riddle to move from a crate which can autodetect types to one that can't, or to several specialized crates.
