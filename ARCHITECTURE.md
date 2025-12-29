# Architecture

This document describes the internal architecture of the `mdf4-rs` library.

## Design Principles

1. **Zero Unsafe Code** - The crate uses `#![forbid(unsafe_code)]` to guarantee memory safety at compile time

2. **Minimal Dependencies** - Only `serde`/`serde_json` for serialization; zero deps with `alloc` only

3. **Lazy Evaluation** - Block data is parsed on-demand; channel values decoded only when accessed

4. **Streaming Support** - Large files handled via indexing without loading entire file into memory

5. **Validation at Construction** - All inputs validated when structures are created, not when accessed

## Feature Flags

| Feature | Default | Requires | Provides |
|---------|---------|----------|----------|
| `std` | ✅ | — | `alloc` + serde/serde_json, file I/O |
| `alloc` | ❌ | Global allocator | Heap-allocated collections |
| `can` | ✅ | — | CAN frame logging via `embedded-can` |
| `dbc` | ✅ | `alloc` | DBC decoding via `dbc-rs` |
| `serde` | ❌ | — | Serialization support |
| `compression` | ❌ | `alloc` | DZ block decompression via `miniz_oxide` |

**Dependency graph:**
```
std ───────► alloc ───────► Core MDF reading/writing
      │
      └────► serde ────────► JSON index serialization

can ─────────────────────► CAN frame logging (embedded-can)

dbc ───────► alloc ───────► DBC-decoded CAN logging (dbc-rs)

compression ► alloc ───────► DZ block decompression (miniz_oxide)
```

**Rules:**
- You MUST enable `std` OR `alloc` (one allocation strategy required)
- `std` implicitly enables `alloc`
- `dbc` requires `alloc` (dbc-rs dependency)
- `compression` requires `alloc` (miniz_oxide dependency)
- `can` and `dbc` are independent and can be enabled separately

## Module Structure

```
src/
├── lib.rs              # Public API re-exports and crate documentation
├── error.rs            # Error types and Result alias
├── mdf.rs              # High-level MDF reader (entry point)
├── channel.rs          # Channel wrapper for value access
├── channel_group.rs    # Channel group wrapper
├── types.rs            # Common types (DataType, etc.)
│
├── blocks/             # Low-level MDF block definitions
│   ├── mod.rs          # Block type re-exports
│   ├── common.rs       # BlockHeader, parsing utilities
│   ├── identification_block.rs
│   ├── header_block.rs
│   ├── data_group_block.rs
│   ├── channel_group_block.rs
│   ├── channel_block.rs
│   └── conversion/     # Value conversion implementations
│       ├── base.rs     # ConversionBlock definition
│       ├── linear.rs   # Linear/rational/algebraic
│       └── text.rs     # Value-to-text mappings
│
├── parsing/            # File parsing and raw data access
│   ├── mod.rs          # Parser re-exports
│   ├── mdf_file.rs     # Full file parser
│   ├── raw_data_group.rs
│   ├── raw_channel_group.rs
│   ├── raw_channel.rs  # Record iteration
│   └── decoder.rs      # DecodedValue and decoding logic
│
├── writer/             # MDF file creation
│   ├── mod.rs          # MdfWriter struct
│   ├── io.rs           # File I/O and block writing
│   ├── init.rs         # Block initialization and linking
│   └── data.rs         # Record encoding
│
├── can/                # CAN bus logging [std + can/dbc features]
│   ├── mod.rs          # CAN logging re-exports
│   └── dbc_logger.rs   # CanDbcLogger implementation
│
├── index.rs            # JSON-serializable file index
├── cut.rs              # Time-based segment extraction
└── merge.rs            # File merging
```

## MDF4 File Format Overview

MDF4 is a binary file format for storing measurement data. Files consist of linked blocks:

```
┌─────────────────────────────────────────────────────────────────────┐
│                           MDF4 File                                 │
├─────────────────────────────────────────────────────────────────────┤
│  ID Block (64 bytes) - File identifier and version                 │
├─────────────────────────────────────────────────────────────────────┤
│  HD Block - Header with file metadata and links                    │
├─────────────────────────────────────────────────────────────────────┤
│  DG Block(s) - Data Groups containing channel groups               │
│    └── CG Block(s) - Channel Groups with record layout             │
│          └── CN Block(s) - Channels with data type info            │
│                └── CC Block - Conversion rules (optional)          │
├─────────────────────────────────────────────────────────────────────┤
│  DT/DL Blocks - Raw data records                                   │
├─────────────────────────────────────────────────────────────────────┤
│  TX/MD Blocks - Text and metadata strings                          │
└─────────────────────────────────────────────────────────────────────┘
```

## Reading Pipeline

```
MDF::from_file()
    │
    ▼
MdfFile (parser) ──► RawDataGroup ──► RawChannelGroup
                                            │
                                            ▼
Channel.values() ◄── ChannelGroup ◄── RawChannel
    │
    ▼
Decoder + CC Block ──► DecodedValue
```

1. **MDF** (`src/mdf.rs`): Entry point that memory-maps the file
2. **MdfFile** (`src/parsing/mdf_file.rs`): Parses all blocks into raw structures
3. **RawDataGroup/RawChannelGroup/RawChannel**: Hold parsed block data
4. **ChannelGroup/Channel**: High-level wrappers providing ergonomic access
5. **Decoder** (`src/parsing/decoder.rs`): Converts raw bytes to `DecodedValue`
6. **ConversionBlock**: Applies unit conversions (linear, polynomial, text mappings)

## Writing Pipeline

```
MdfWriter::new()
    │
    ▼
init_mdf_file() ──► ID + HD blocks
    │
    ▼
add_channel_group() ──► DG + CG blocks
    │
    ▼
add_channel() ──► CN blocks
    │
    ▼
start_data_block() ──► DT block header
    │
    ▼
write_record() ──► Raw data
    │
    ▼
finalize() ──► Flush + update links
```

1. **MdfWriter** (`src/writer/mod.rs`): Main writer state machine
2. **IO layer** (`src/writer/io.rs`): Block writing with 8-byte alignment
3. **Init layer** (`src/writer/init.rs`): Block creation and link management
4. **Data layer** (`src/writer/data.rs`): Record encoding to bytes

## Value Conversions

MDF supports complex conversion chains:

```
Raw Value → CC Block 1 → CC Block 2 → ... → Physical Value
```

Conversions are implemented in `src/blocks/conversion/`:
- **Identity** (type 0): No conversion
- **Linear** (type 1): `y = a + b*x`
- **Rational** (type 2): `y = (a + bx + cx²) / (d + ex + fx²)`
- **Algebraic** (type 3): Formula evaluation
- **Value-to-Text** (types 7-8): Lookup tables
- **Text-to-Value** (type 9): Reverse lookup

## Block Types Reference

| Block ID | Name | Purpose |
|----------|------|---------|
| `##ID` | Identification | File format identifier (always first 64 bytes) |
| `##HD` | Header | File metadata, links to first DG |
| `##DG` | Data Group | Groups related channel groups |
| `##CG` | Channel Group | Defines record layout |
| `##CN` | Channel | Individual signal definition |
| `##CC` | Conversion | Value transformation rules |
| `##TX` | Text | String storage |
| `##MD` | Metadata | XML metadata |
| `##DT` | Data | Raw sample records |
| `##DL` | Data List | Links multiple DT blocks |
| `##SI` | Source Info | Acquisition source metadata |

## Error Handling

All fallible operations return `Result<T, Error>`:

```rust
pub enum Error {
    IOError(std::io::Error),
    InvalidBlock { expected: &'static str, found: String },
    InvalidData { msg: &'static str },
    UnsupportedFeature { msg: &'static str },
    ConversionError { msg: String },
    // ...
}
```

- I/O errors wrapped in `Error::IOError`
- Parse errors provide context (expected vs actual)
- Conversion errors propagated through the chain

## Indexing

The `MdfIndex` system (`src/index.rs`) enables:
- Creating lightweight JSON metadata files
- Reading specific channels without full file parsing
- HTTP range request support via `ByteRangeReader` trait

## Testing Strategy

```
tests/
├── api.rs              # High-level API tests
├── blocks.rs           # Block roundtrip tests
├── data_files.rs       # Integration tests with real files
├── index.rs            # Indexing tests
├── merge.rs            # File merging tests
└── ...
```

Unit tests are co-located with implementation (`#[cfg(test)] mod tests`).

## Performance Considerations

1. **Large Files** - Use indexing to avoid parsing entire file
2. **Many Records** - Records are decoded on-demand via iterators
3. **Writing** - Default 1 MB buffer; use `new_with_capacity()` to tune
4. **Memory** - Memory mapping means OS manages page cache
5. **Streaming Index** - 335x faster, 50x less memory for large files
