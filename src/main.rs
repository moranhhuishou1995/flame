use std::fs;
use std::path::PathBuf;
use chrono::Local;
mod collector;
mod process;
mod draw_flame;
mod command;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get the current date
    let date = Local::now().format("%Y%m%d").to_string();
    // Build the output directory path in /tmp
    let output_dir = PathBuf::from("/tmp").join(format!("output_{}", date));

    // Create the output_YYYYMMDD directory
    if let Err(e) = fs::create_dir_all(&output_dir) {
        panic!("Failed to create output directory: {}", e);
    }

    // Create subdirectories
    let sub_dirs = ["merged_stack", "flame_svg", "url_config"];
    for sub_dir in sub_dirs {
        let sub_dir_path = output_dir.join(sub_dir);
        if let Err(e) = fs::create_dir_all(&sub_dir_path) {
            panic!("Failed to create {} directory: {}", sub_dir, e);
        }
    }

    command::run_cli().await?;
    Ok(())
}