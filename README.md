# mdf4-rs

A safe, efficient Rust library for reading and writing ASAM MDF 4 (Measurement Data Format) files.

[![Crates.io](https://img.shields.io/crates/v/mdf4-rs.svg)](https://crates.io/crates/mdf4-rs)
[![Documentation](https://docs.rs/mdf4-rs/badge.svg)](https://docs.rs/mdf4-rs)
[![License](https://img.shields.io/crates/l/mdf4-rs.svg)](LICENSE)

## Features

- **100% safe Rust** - `#![forbid(unsafe_code)]`
- **Minimal dependencies** - Only `serde`/`serde_json` for serialization
- **Memory efficient** - Streaming index for large files (335x faster, 50x less memory)
- **Full read/write** - Create, read, and modify MDF4 files
- **CAN logging** - Integrated CAN bus data logging with DBC support

## Quick Start

```toml
[dependencies]
mdf4-rs = "0.1.0-alpha1"
```

### Reading

```rust
use mdf4_rs::MDF;

let mdf = MDF::from_file("recording.mf4")?;
for group in mdf.channel_groups() {
    for channel in group.channels() {
        let values = channel.values()?;
        println!("{}: {} samples", channel.name()?.unwrap_or_default(), values.len());
    }
}
```

### Writing

```rust
use mdf4_rs::{MdfWriter, DataType, DecodedValue};

let mut writer = MdfWriter::new("output.mf4")?;
writer.init_mdf_file()?;
let cg = writer.add_channel_group(None, |_| {})?;
writer.add_channel(&cg, None, |ch| {
    ch.data_type = DataType::FloatLE;
    ch.name = Some("Temperature".into());
    ch.bit_count = 64;
})?;
// ... write data
```

### CAN Logging

```rust
use mdf4_rs::can::DbcMdfLogger;

let dbc = dbc_rs::Dbc::parse(dbc_content)?;
let mut logger = DbcMdfLogger::builder(&dbc).build()?;
logger.log(0x100, timestamp_us, &frame_data);
let mdf_bytes = logger.finalize()?;
```

## Performance

| File Size | Streaming Index | Memory Savings |
|-----------|-----------------|----------------|
| 1 MB | 6x faster | 50x less |
| 40 MB | 335x faster | 50x less |

## Documentation

- [API Reference](https://docs.rs/mdf4-rs)
- [ARCHITECTURE.md](ARCHITECTURE.md) - Internal design

## License

MIT OR Apache-2.0. See [LICENSING.md](LICENSING.md).
