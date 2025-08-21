use clap::Parser;
use std::error::Error;

mod flame;
use crate::flame::flame_jsonstacks;

/// Call stack flame graph generation tool
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Path to input directory containing mergedstack_rankN.json files (required)
    #[clap(short, long, value_parser)]
    input_dir: String,

    /// Path to output directory (optional)
    #[clap(short, long, value_parser)]
    output_dir: Option<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    // Parse command line arguments
    let args = Args::parse();

    println!("Input directory: {}", args.input_dir);
    if let Some(output) = &args.output_dir {
        println!("Output directory: {}", output);
    } else {
        println!("No output directory specified, will use default path");
    }

    // Call flame graph generation interface
    flame_jsonstacks(&args.input_dir, args.output_dir.as_deref())?;

    Ok(())
}
    