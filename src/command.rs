use clap::{Command, Parser, Subcommand};
use crate::collector::fetch_and_save_urls;
use crate::process::process_and_merge_callstacks;
use crate::draw_flame::draw_frame_graph;
use serde_json::from_str;
use std::error::Error;
use std::fs::File;
use std::io::{Read, BufReader, BufRead, Write};
use std::path::PathBuf;

/// Subcommand enum, defining fetch, process, and draw subcommands
#[derive(Subcommand, Debug)]
enum SubCommands {
    /// Fetch call stack information from URLs listed in a specified file and save it to a JSON file.
    Fetch {
        /// Path to the file containing a JSON array of URLs.
        #[arg(
            short = 'f',
            long = "file",
            default_value = concat!(env!("CARGO_MANIFEST_DIR"), "/url_config/urls.json")
        )]
        url_file: String,
    },
    /// Process the call stack information stored in a JSON file and save the processed results to a text file.
    Process {
        /// Path to the input JSON file containing call stack information to be processed.
        #[arg(
            short = 'i',
            long = "input",
            required = true,
            help = "Path to the input JSON file containing call stack information to be processed."
        )]
        input: String,
        /// Path to the output directory for storing the processed call stack information.
        #[arg(
            short = 'o',
            long = "output",
            value_parser = |input_path_str: &str| -> Result<String, String> {
                let input_path = PathBuf::from(input_path_str);
                if input_path.is_dir() {
                    return Ok(input_path.to_string_lossy().to_string());
                }
                Err("Output path must be a directory".to_string())
            },
            help = "Path to the output directory for storing the processed call stack information."
        )]
        output: Option<String>,
    },
    /// Draw a frame graph based on the merged call stack file.
    Draw {
        /// Path to the merged call stack file, used as the basis for drawing the frame graph.
        #[arg(
            short = 'i',
            long = "input",
            required = true,
            help = "Path to the merged call stack file, used as the basis for drawing the frame graph."
        )]
        input: String,
        /// Path to the output directory for storing the generated frame graph.
        #[arg(
            short = 'o',
            long = "output",
            value_parser = |input_path_str: &str| -> Result<String, String> {
                let input_path = PathBuf::from(input_path_str);
                if input_path.is_dir() {
                    return Ok(input_path.to_string_lossy().to_string());
                }
                Err("Output path must be a valid directory".to_string())
            },
            help = "Path to the output directory for storing the generated frame graph."
        )]
        output: Option<String>,
    },
}

/// Main command structure
#[derive(Parser, Debug)]
#[command(
    name = "myapp",
    about = "Perform call stack collection, processing, and frame graph drawing tasks.",
    long_about = "This tool provides three subcommands for collecting call stack information from URLs, processing call stack data, and drawing frame graphs based on the processed results."
)]
struct Cli {
    #[command(subcommand)]
    command: SubCommands,
}

/// Build the command-line parser
pub fn build_cli() -> Command {
    // Use the `clap::CommandFactory::command` method
    <Cli as clap::CommandFactory>::command()
}

/// Parse the command line and call the corresponding functions
pub async fn run_cli() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match cli.command {
        SubCommands::Fetch { url_file } => {
            let mut file = File::open(url_file)?;
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;
            let urls: Vec<String> = from_str(&contents)?;

            fetch_and_save_urls(urls).await?;
            println!("Call stacks have been collected and saved to output.json");
        }
        SubCommands::Process { input, output } => {
            process_and_merge_callstacks(&input,  output.as_deref())?;
            println!("Merged call stacks have been written successfully");
        }
        SubCommands::Draw { input, output } => {
            draw_frame_graph(&input,  output.as_deref());
            println!("Frame graph has been drawn successfully");
        }
    }

    Ok(())
}