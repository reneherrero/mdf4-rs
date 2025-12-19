# mdf4-rs

A safe, efficient Rust library for reading and writing ASAM MDF 4 (Measurement Data Format) files.

[![Crates.io](https://img.shields.io/crates/v/mdf4-rs.svg)](https://crates.io/crates/mdf4-rs)
[![Documentation](https://docs.rs/mdf4-rs/badge.svg)](https://docs.rs/mdf4-rs)
[![License](https://img.shields.io/crates/l/mdf4-rs.svg)](https://github.com/reneherrero/mdf4-rs/blob/main/LICENSE)
[![CI](https://github.com/reneherrero/mdf4-rs/workflows/CI/badge.svg)](https://github.com/reneherrero/mdf4-rs/actions)

## Features

- **100% safe Rust** - `#![forbid(unsafe_code)]`, no unsafe blocks
- **Minimal dependencies** - Only `serde` and `serde_json` for serialization
- **Memory efficient** - Streaming index for large files (335x faster, 50x less memory)
- **Full read/write support** - Create, read, and modify MDF4 files
- **Well tested** - 70+ tests covering all functionality

For design principles and module structure, see [ARCHITECTURE.md](ARCHITECTURE.md).

## Quick Start

### Reading an MDF file

```rust
use mdf4_rs::{MDF, Result};

fn main() -> Result<()> {
    let mdf = MDF::from_file("recording.mf4")?;

    for group in mdf.channel_groups() {
        println!("Group: {:?}", group.name()?);

        for channel in group.channels() {
            let name = channel.name()?.unwrap_or_default();
            let values = channel.values()?;
            println!("  {}: {} samples", name, values.len());
        }
    }
    Ok(())
}
```

### Writing an MDF file

```rust
use mdf4_rs::{MdfWriter, DataType, DecodedValue, Result};

fn main() -> Result<()> {
    let mut writer = MdfWriter::new("output.mf4")?;
    writer.init_mdf_file()?;

    let cg = writer.add_channel_group(None, |_| {})?;

    writer.add_channel(&cg, None, |ch| {
        ch.data_type = DataType::FloatLE;
        ch.name = Some("Temperature".into());
        ch.bit_count = 64;
    })?;

    writer.start_data_block_for_cg(&cg, 0)?;
    for temp in [20.5, 21.0, 21.5, 22.0] {
        writer.write_record(&cg, &[DecodedValue::Float(temp)])?;
    }
    writer.finish_data_block(&cg)?;
    writer.finalize()?;

    Ok(())
}
```

### Efficient reading with Index (for large files)

```rust
use mdf4_rs::{MdfIndex, BufferedRangeReader, Result};

fn main() -> Result<()> {
    // Create index using streaming (minimal memory)
    let index = MdfIndex::from_file_streaming("large_recording.mf4")?;

    // Save index for instant reuse
    index.save_to_file("recording.index")?;

    // Read only the channel you need
    let mut reader = BufferedRangeReader::new("large_recording.mf4")?;
    let values = index.read_channel_values_by_name("Temperature", &mut reader)?;

    println!("Read {} values", values.len());
    Ok(())
}
```

## Performance

The streaming index approach provides significant benefits for large files:

| File Size | Index Creation | Memory Usage |
|-----------|----------------|--------------|
| 1 MB | 6x faster | 50x less |
| 10 MB | 80x faster | 50x less |
| 40 MB | 335x faster | 50x less |

When reading a single channel from a 40 MB file with 50 channels:
- **Old method**: 20ms, loads entire 40MB into memory
- **Streaming**: 11ms, uses only ~10KB (index) + channel data

## MDF4 Format Support

### Supported Features
- **Blocks**: ID, HD, DG, CG, CN, CC, TX, MD, DT, DL, SI
- **Data types**: All integer types (8-64 bit, LE/BE), floats (32/64 bit), strings (UTF-8, Latin-1)
- **Conversions**: Identity, linear, rational, algebraic, value-to-text, range-to-text, text-to-value
- **Invalidation bits**: Per-sample validity tracking
- **Multiple channel groups**: Separate record layouts in one file
- **Fragmented data**: DL blocks with multiple DT fragments

### Limitations
- **Compression**: DZ blocks (zlib) not yet supported
- **Attachments**: AT blocks not implemented
- **Events**: EV blocks not implemented
- **Bus logging**: CAN/LIN/FlexRay specific blocks not implemented

## API Overview

| Module | Description |
|--------|-------------|
| `MDF` | High-level file reader |
| `MdfWriter` | File creation with builder API |
| `MdfIndex` | Lightweight index for efficient partial reads |
| `FileRangeReader` / `BufferedRangeReader` | Byte-range readers for index-based access |
| `cut_mdf_by_time()` | Extract time-based segments |
| `merge_files()` | Combine multiple MDF files |

## Error Handling

All operations return `Result<T, Error>`:

```rust
use mdf4_rs::{MDF, Error};

match MDF::from_file("recording.mf4") {
    Ok(mdf) => println!("Loaded {} groups", mdf.channel_groups().len()),
    Err(Error::IOError(e)) => eprintln!("IO error: {}", e),
    Err(Error::FileIdentifierError(id)) => eprintln!("Not an MDF file: {}", id),
    Err(Error::FileVersioningError(v)) => eprintln!("Unsupported version: {}", v),
    Err(e) => eprintln!("Error: {:?}", e),
}
```

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
mdf4-rs = "0.1.0-alpha1"
```

## Benchmarks

Run the included benchmarks:

```bash
cargo bench --bench index_benchmark
```

## Contributing

Contributions welcome! Areas needing work:
- DZ block decompression (zlib)
- Attachment blocks
- Event blocks
- Bus logging blocks (CAN, LIN, FlexRay)
- More conversion types

## License

Available under **MIT OR Apache-2.0** (open source) or commercial licensing. See [LICENSING.md](LICENSING.md) for details.

## References

- [Architecture](ARCHITECTURE.md) - Internal design and module structure
- [ASAM MDF Standard](https://www.asam.net/standards/detail/mdf/) - Official specification
- [asammdf Lib](https://github.com/danielhrisca/asammdf) - Python MDF4 reference implementation
- [mf4-rs](https://github.com/dmagyar-0/mf4-rs) - Original Starting Point
