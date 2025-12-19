# Architecture

This document describes the internal architecture of `mdf4-rs`, a Rust library for reading and writing ASAM MDF 4 (Measurement Data Format) files.

## Overview

MDF4 is a binary file format used primarily in automotive and industrial applications for storing measurement data. The format consists of linked binary blocks that describe metadata and contain raw measurement samples.

```
┌─────────────────────────────────────────────────────────────────────┐
│                           MDF4 File                                 │
├─────────────────────────────────────────────────────────────────────┤
│  ID Block (64 bytes) - File identifier and version                  │
├─────────────────────────────────────────────────────────────────────┤
│  HD Block - Header with file metadata and links                     │
├─────────────────────────────────────────────────────────────────────┤
│  DG Block(s) - Data Groups containing channel groups                │
│    └── CG Block(s) - Channel Groups with record layout              │
│          └── CN Block(s) - Channels with data type info             │
│                └── CC Block - Conversion rules (optional)           │
├─────────────────────────────────────────────────────────────────────┤
│  DT/DL Blocks - Raw data records                                    │
├─────────────────────────────────────────────────────────────────────┤
│  TX/MD Blocks - Text and metadata strings                           │
└─────────────────────────────────────────────────────────────────────┘
```

## Module Structure

```
src/
├── lib.rs              # Public API re-exports and crate documentation
├── error.rs            # Error types and Result alias
├── mdf.rs              # High-level MDF reader (entry point)
├── channel.rs          # Channel wrapper for value access
├── channel_group.rs    # Channel group wrapper
│
├── blocks/             # Low-level MDF block definitions
│   ├── mod.rs          # Block type re-exports
│   ├── common.rs       # BlockHeader, DataType, parsing utilities
│   ├── identification_block.rs
│   ├── header_block.rs
│   ├── data_group_block.rs
│   ├── channel_group_block.rs
│   ├── channel_block.rs
│   ├── conversion/     # Value conversion implementations
│   │   ├── base.rs     # ConversionBlock definition
│   │   ├── linear.rs   # Linear/rational/algebraic conversions
│   │   ├── text.rs     # Value-to-text mappings
│   │   └── ...
│   └── ...
│
├── parsing/            # File parsing and raw data access
│   ├── mod.rs          # Parser re-exports
│   ├── mdf_file.rs     # Full file parser
│   ├── raw_data_group.rs
│   ├── raw_channel_group.rs
│   ├── raw_channel.rs  # Record iteration
│   ├── decoder.rs      # DecodedValue and decoding logic
│   └── ...
│
├── writer/             # MDF file creation
│   ├── mod.rs          # MdfWriter struct and docs
│   ├── io.rs           # File I/O and block writing
│   ├── init.rs         # Block initialization and linking
│   └── data.rs         # Record encoding
│
├── index.rs            # JSON-serializable file index
├── cut.rs              # Time-based segment extraction
└── merge.rs            # File merging
```

## Core Components

### Reading Pipeline

```
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│  MDF::from   │───▶│   MdfFile    │───▶│ RawDataGroup │
│    _file()   │    │   (parser)   │    │  (parsed)    │
└──────────────┘    └──────────────┘    └──────────────┘
                                               │
                                               ▼
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│   Channel    │◀───│ ChannelGroup │◀───│RawChannelGrp │
│  .values()   │    │  (wrapper)   │    │  (parsed)    │
└──────────────┘    └──────────────┘    └──────────────┘
       │
       ▼
┌──────────────┐    ┌──────────────┐
│  Decoder     │───▶│ DecodedValue │
│  + CC Block  │    │   (output)   │
└──────────────┘    └──────────────┘
```

1. **MDF** (`src/mdf.rs`): Entry point that memory-maps the file and delegates to `MdfFile`
2. **MdfFile** (`src/parsing/mdf_file.rs`): Parses all blocks into raw structures
3. **RawDataGroup/RawChannelGroup/RawChannel**: Hold parsed block data and provide iteration
4. **ChannelGroup/Channel**: High-level wrappers providing ergonomic access
5. **Decoder** (`src/parsing/decoder.rs`): Converts raw bytes to `DecodedValue` enum
6. **ConversionBlock**: Applies unit conversions (linear, polynomial, text mappings)

### Writing Pipeline

```
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│  MdfWriter   │───▶│ init_mdf_    │───▶│  ID + HD     │
│    ::new()   │    │   file()     │    │   blocks     │
└──────────────┘    └──────────────┘    └──────────────┘
       │
       ▼
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│ add_channel  │───▶│ add_channel  │───▶│  DG + CG +   │
│   _group()   │    │     ()       │    │  CN blocks   │
└──────────────┘    └──────────────┘    └──────────────┘
       │
       ▼
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│ start_data   │───▶│ write_record │───▶│  DT block    │
│   _block()   │    │     ()       │    │   (data)     │
└──────────────┘    └──────────────┘    └──────────────┘
       │
       ▼
┌──────────────┐
│  finalize()  │───▶ Flush + update links
└──────────────┘
```

1. **MdfWriter** (`src/writer/mod.rs`): Main writer state machine
2. **IO layer** (`src/writer/io.rs`): Block writing with 8-byte alignment
3. **Init layer** (`src/writer/init.rs`): Block creation and link management
4. **Data layer** (`src/writer/data.rs`): Record encoding to bytes

## Key Design Decisions

### Memory Mapping

The library uses `memmap2` for reading files. This allows:
- Zero-copy access to file data
- Efficient random access for block traversal
- OS-level caching and prefetching

### Block Parsing

Blocks are parsed lazily where possible:
- Block headers are parsed to locate data
- Channel values are only decoded when `values()` is called
- String blocks are read on demand via `read_string_block()`

### Value Conversions

MDF supports complex conversion chains:

```
Raw Value → CC Block 1 → CC Block 2 → ... → Physical Value
```

Conversions are implemented in `src/blocks/conversion/`:
- **Identity** (type 0): No conversion
- **Linear** (type 1): `y = a + b*x`
- **Rational** (type 2): `y = (a + bx + cx²) / (d + ex + fx²)`
- **Algebraic** (type 3): Formula evaluation with custom parser
- **Value-to-Text** (types 7-8): Lookup tables
- **Text-to-Value** (type 9): Reverse lookup

### Error Handling

All fallible operations return `Result<T, Error>`:
- I/O errors are wrapped in `Error::IOError`
- Parse errors provide context (expected vs actual)
- Conversion errors are propagated through the chain

### Indexing

The `MdfIndex` system (`src/index.rs`) enables:
- Creating lightweight JSON metadata files
- Reading specific channels without full file parsing
- HTTP range request support via `ByteRangeReader` trait

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

## Thread Safety

- **Reading**: `MDF` is not `Send`/`Sync` due to internal `&[u8]` references
- **Writing**: `MdfWriter` is single-threaded (uses internal buffers)
- **Indexing**: `MdfIndex` is `Send`/`Sync` (owns all data)

## Performance Considerations

1. **Large Files**: Use indexing to avoid parsing entire file
2. **Many Records**: Records are decoded on-demand via iterators
3. **Writing**: Default 1 MB buffer; use `new_with_capacity()` to tune
4. **Memory**: Memory mapping means OS manages page cache

## Extending the Library

### Adding a New Conversion Type

1. Add variant to `ConversionType` in `src/blocks/conversion/base.rs`
2. Implement conversion logic in appropriate file under `src/blocks/conversion/`
3. Update `ConversionBlock::apply_decoded()` dispatch

### Adding a New Block Type

1. Create `src/blocks/new_block.rs` with struct and `BlockParse` impl
2. Add to `src/blocks/mod.rs` re-exports
3. Update parser in `src/parsing/` to read the block
4. Update writer in `src/writer/` if block is writable

## Testing

```
tests/
├── api.rs                      # High-level API tests
├── blocks.rs                   # Block roundtrip tests
├── data_files.rs               # Integration tests with real files
├── index.rs                    # Indexing tests
├── merge.rs                    # File merging tests
├── test_invalidation_bits.rs   # Invalidation bit handling
└── enhanced_index_conversions.rs
```

Run tests with:
```bash
cargo test
```
