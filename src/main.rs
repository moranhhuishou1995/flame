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
    // Get the project root directory
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Build the output directory path
    let output_dir = project_root.join(format!("output_{}", date));

    // Create the output_YYYYMMDD directory
    if let Err(e) = fs::create_dir_all(&output_dir) {
        panic!("Failed to create output directory: {}", e);
    }

    // Create subdirectories
    let sub_dirs = ["url_stack", "merged_stack", "flame_svg"];
    for sub_dir in sub_dirs {
        let sub_dir_path = output_dir.join(sub_dir);
        if let Err(e) = fs::create_dir_all(&sub_dir_path) {
            panic!("Failed to create {} directory: {}", sub_dir, e);
        }
    }

    command::run_cli().await?;
    Ok(())
}