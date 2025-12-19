//! Benchmarks comparing old (full file load) vs new (streaming index) approaches.
//!
//! Run with: cargo bench --bench index_benchmark

use mdf4_rs::{
    BufferedRangeReader, DataType, DecodedValue, FileRangeReader, MdfIndex, MdfWriter, MDF,
};
use std::time::{Duration, Instant};

/// Benchmark result for a single operation
struct BenchResult {
    name: String,
    duration: Duration,
    iterations: u32,
}

impl BenchResult {
    fn avg_ms(&self) -> f64 {
        self.duration.as_secs_f64() * 1000.0 / self.iterations as f64
    }
}

/// Run a benchmark function multiple times and measure average time
fn bench<F: FnMut()>(name: &str, iterations: u32, mut f: F) -> BenchResult {
    // Warmup
    f();

    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let duration = start.elapsed();

    BenchResult {
        name: name.to_string(),
        duration,
        iterations,
    }
}

/// Create a test MDF file with specified number of channels and records
fn create_test_file(path: &str, num_channels: usize, num_records: usize) -> mdf4_rs::Result<()> {
    let mut writer = MdfWriter::new(path)?;
    writer.init_mdf_file()?;

    let cg_id = writer.add_channel_group(None, |_| {})?;

    // Create channels
    let mut prev_ch_id: Option<String> = None;
    for i in 0..num_channels {
        let ch_id = writer.add_channel(&cg_id, prev_ch_id.as_deref(), |ch| {
            ch.data_type = DataType::FloatLE;
            ch.name = Some(format!("Channel_{}", i));
            ch.bit_count = 64;
        })?;
        if i == 0 {
            writer.set_time_channel(&ch_id)?;
        }
        prev_ch_id = Some(ch_id);
    }

    // Write records
    writer.start_data_block_for_cg(&cg_id, 0)?;

    let record: Vec<DecodedValue> = (0..num_channels)
        .map(|i| DecodedValue::Float(i as f64 * 0.1))
        .collect();

    for _ in 0..num_records {
        writer.write_record(&cg_id, &record)?;
    }

    writer.finish_data_block(&cg_id)?;
    writer.finalize()?;

    Ok(())
}

fn main() -> mdf4_rs::Result<()> {
    println!("=== MDF4-RS Index Benchmark ===\n");

    // Test configurations: (channels, records, description)
    let configs = [
        (5, 1_000, "Small (5 ch, 1K records)"),
        (10, 10_000, "Medium (10 ch, 10K records)"),
        (20, 50_000, "Large (20 ch, 50K records)"),
        (50, 100_000, "XLarge (50 ch, 100K records)"),
    ];

    for (num_channels, num_records, desc) in configs {
        println!("--- {} ---", desc);

        let path = std::env::temp_dir().join(format!(
            "bench_{}ch_{}rec.mf4",
            num_channels, num_records
        ));
        let path_str = path.to_str().unwrap();

        // Create test file
        print!("Creating test file... ");
        let start = Instant::now();
        create_test_file(path_str, num_channels, num_records)?;
        let file_size = std::fs::metadata(&path)?.len();
        println!(
            "done ({:.2} MB, {:.0}ms)",
            file_size as f64 / 1_048_576.0,
            start.elapsed().as_millis()
        );

        let iterations = if num_records <= 10_000 { 5 } else { 3 };

        // Benchmark: Index creation - old method (loads entire file)
        let old_index = bench("Index (from_file - loads entire file)", iterations, || {
            let _ = MdfIndex::from_file(path_str).unwrap();
        });

        // Benchmark: Index creation - new streaming method
        let new_index = bench("Index (from_file_streaming - minimal memory)", iterations, || {
            let _ = MdfIndex::from_file_streaming(path_str).unwrap();
        });

        // Benchmark: Read single channel - old method
        let old_read = bench("Read channel (MDF::from_file + values())", iterations, || {
            let mdf = MDF::from_file(path_str).unwrap();
            let _values = mdf.channel_groups()[0].channels()[1].values().unwrap();
        });

        // Benchmark: Read single channel - new method with index
        let index = MdfIndex::from_file_streaming(path_str)?;
        let new_read = bench(
            "Read channel (streaming index + BufferedRangeReader)",
            iterations,
            || {
                let mut reader = BufferedRangeReader::new(path_str).unwrap();
                let _values = index.read_channel_values(0, 1, &mut reader).unwrap();
            },
        );

        // Benchmark: Read single channel - new method with unbuffered reader
        let unbuf_read = bench(
            "Read channel (streaming index + FileRangeReader)",
            iterations,
            || {
                let mut reader = FileRangeReader::new(path_str).unwrap();
                let _values = index.read_channel_values(0, 1, &mut reader).unwrap();
            },
        );

        // Benchmark: Read with pre-loaded index (simulates cached index)
        let index_path = std::env::temp_dir().join("bench_index.json");
        index.save_to_file(index_path.to_str().unwrap())?;
        let cached_read = bench(
            "Read channel (cached index + BufferedRangeReader)",
            iterations,
            || {
                let idx = MdfIndex::load_from_file(index_path.to_str().unwrap()).unwrap();
                let mut reader = BufferedRangeReader::new(path_str).unwrap();
                let _values = idx.read_channel_values(0, 1, &mut reader).unwrap();
            },
        );

        // Print results
        println!("\nResults ({} iterations each):", iterations);
        println!("  {:50} {:>10.2} ms", old_index.name, old_index.avg_ms());
        println!("  {:50} {:>10.2} ms", new_index.name, new_index.avg_ms());
        let speedup_index = old_index.avg_ms() / new_index.avg_ms();
        println!(
            "  -> Streaming index is {:.1}x {} than loading entire file",
            if speedup_index >= 1.0 {
                speedup_index
            } else {
                1.0 / speedup_index
            },
            if speedup_index >= 1.0 {
                "faster"
            } else {
                "slower"
            }
        );

        println!();
        println!("  {:50} {:>10.2} ms", old_read.name, old_read.avg_ms());
        println!("  {:50} {:>10.2} ms", new_read.name, new_read.avg_ms());
        println!("  {:50} {:>10.2} ms", unbuf_read.name, unbuf_read.avg_ms());
        println!("  {:50} {:>10.2} ms", cached_read.name, cached_read.avg_ms());

        let speedup_read = old_read.avg_ms() / new_read.avg_ms();
        println!(
            "  -> Streaming read is {:.1}x {} than loading entire file",
            if speedup_read >= 1.0 {
                speedup_read
            } else {
                1.0 / speedup_read
            },
            if speedup_read >= 1.0 {
                "faster"
            } else {
                "slower"
            }
        );

        // Memory estimation
        let record_size = num_channels * 8; // 64-bit floats
        let data_size = record_size * num_records;
        println!("\nEstimated memory usage:");
        println!(
            "  Old method (MDF::from_file): ~{:.2} MB (entire file in memory)",
            file_size as f64 / 1_048_576.0
        );
        println!(
            "  New method (streaming):      ~{:.2} KB (index only)",
            (num_channels * 200) as f64 / 1024.0 // ~200 bytes per channel in index
        );
        println!(
            "  Data read:                   ~{:.2} MB (channel data)",
            (num_records * 8) as f64 / 1_048_576.0 // 8 bytes per value for one channel
        );

        // Cleanup
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&index_path);

        println!();
    }

    println!("=== Summary ===");
    println!("The streaming index approach excels when:");
    println!("  - You only need to read a subset of channels");
    println!("  - Working with large files that don't fit in memory");
    println!("  - Index can be cached and reused across sessions");
    println!("  - Reading from remote sources (HTTP range requests)");
    println!();
    println!("The old approach (MDF::from_file) is better when:");
    println!("  - Reading all channels from a file");
    println!("  - File fits comfortably in memory");
    println!("  - Multiple passes over the same data are needed");

    Ok(())
}
