use mdf4_rs::{MDF, Result};

fn main() -> Result<()> {
    // 1) Parse the file (no data is decoded yet)
    // This assumes `write_file` has been run to create the file
    let path = "example.mf4";
    let mdf = MDF::from_file(path)?;
    println!();

    // 2) Walk all ChannelGroups
    for group in mdf.channel_groups() {
        // a) Group metadata
        if let Some(name) = group.name()? {
            println!("Channel Group Name : {}", name);
        } else {
            println!("Channel Group Name : <unnamed>");
        }

        if let Some(comment) = group.comment()? {
            println!("Channel Group Comment : {}", comment);
        }

        if let Some(src) = group.source()? {
            // src.name, src.path, src.comment are Option<String>
            println!("Acquisition Source:");
            println!("  Name   : {}", src.name.as_deref().unwrap_or("<none>"));
            println!("  Path   : {}", src.path.as_deref().unwrap_or("<none>"));
            println!("  Comment: {}", src.comment.as_deref().unwrap_or("<none>"));
        } else {
            println!("Acquisition Source: <none>");
        }
        println!();

        println!("Channels:");
        // b) Iterate channels (still no sample decoding)
        for channel in group.channels() {
            // Channel metadata
            println!();
            if let Some(name) = channel.name()? {
                print!("    Channel Name {}", name);
            }
            println!();

            if let Some(unit) = channel.unit()? {
                print!("    Channel Unit [{}]", unit);
            }
            println!();

            if let Some(cmt) = channel.comment()? {
                println!("    Comment: {}", cmt);
            }
            if let Some(src) = channel.source()? {
                println!("    Signal Source:");
                println!(
                    "      Source Name   : {}",
                    src.name.as_deref().unwrap_or("<none>")
                );
                println!(
                    "      Source Path   : {}",
                    src.path.as_deref().unwrap_or("<none>")
                );
                println!(
                    "      Source Comment: {}",
                    src.comment.as_deref().unwrap_or("<none>")
                );
            } else {
                println!("    Signal Source: <none>");
            }
            // 3) Decode samples *on demand*
            let samples = channel.values()?;
            let total_samples = samples.len();
            println!("    Samples: {} records", total_samples);
            println!(
                "    Values: first 5 = {:?}",
                &samples[..5.min(total_samples)]
            );
            println!(
                "    Values: last 5 = {:?}",
                &samples[total_samples.saturating_sub(5)..]
            );
        }

        println!(); // blank line between groups
    }

    Ok(())
}
