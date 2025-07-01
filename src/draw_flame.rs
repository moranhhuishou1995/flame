use std::fs::File;
use std::io::BufReader;
use inferno::flamegraph::{self, Options, Palette};
use std::path::PathBuf;
use chrono::Local;
use std::env;

/// Generates a flamegraph from a stack trace file and saves it as an SVG file.
/// If `output_path` is `None`, the SVG file will be saved in the 'flame_svg' directory 
/// at the same level as the parent directory of the input file.
/// If `output_path` is `Some`, the SVG file will be saved in the specified directory.
pub fn draw_frame_graph(file_path: &str, output_path: Option<&str>) {
    // Open the input file containing stack trace data
    let file = File::open(file_path).expect("Failed to open file");
    // Wrap the file in a BufReader for efficient reading
    let reader = BufReader::new(file);

    // Initialize flamegraph generation options with default values
    let mut options = Options::default();
    // Set the color palette for the flamegraph to Java multi-color scheme
    options.colors = Palette::Multi(flamegraph::color::MultiPalette::Java);

    // Convert the input file path string to a PathBuf
    let input_file_path = PathBuf::from(file_path);
    // Extract the file name without the extension from the input file path
    let file_stem = input_file_path.file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .expect("Failed to get file stem");

    // Determine the output directory
    let output_dir = match output_path {
        // Use the specified output path if provided
        Some(path) => PathBuf::from(path),
        // Use the default output path in /tmp/output_xxxx/flame_svg
        None => {
            use chrono::Local;
            let date = Local::now().format("%Y%m%d").to_string();
            let default_output_dir = PathBuf::from("/tmp").join(format!("output_{}", date)).join("flame_svg");
            default_output_dir
        }
    };

    // Create the output directory if it doesn't exist
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        panic!("Failed to create output directory: {}", e);
    }

    // Construct the output file path
    let mut output_path = output_dir.clone();
    output_path.push(format!("{}.svg", file_stem));

    // Create the output file for the generated flamegraph
    let mut output_file = File::create(output_path.clone()).expect("Failed to create SVG file");
    // Generate the flamegraph from the input data and write it to the output file
    flamegraph::from_reader(&mut options, reader, &mut output_file).expect("Failed to generate flamegraph");

    // Print a message indicating that the flamegraph has been generated and saved
    println!("Flamegraph generated and saved as {}", output_path.display());
}

#[cfg(test)]
use std::fs;
mod tests {
    use super::*;
    use std::env;

    /// Tests the `draw_frame_graph` function.
    /// Checks if an SVG file with the same name as the input file exists in the test directory.
    #[test]
    fn test_draw_frame_graph() {
        // Get the project root directory
        let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // Build the input file path
        let input_file_path = project_root.join("test").join("merged_output.txt");
        let input_file_path_str = input_file_path.to_str().expect("Failed to convert input path to string");
        // Build the output directory path
        let output_dir = project_root.join("test");
        let output_dir_str = output_dir.to_str().expect("Failed to convert output path to string");

        // Call the draw_frame_graph function
        draw_frame_graph(input_file_path_str, Some(output_dir_str));

        // Get the expected SVG file name
        let expected_file_name = input_file_path.file_stem()
            .and_then(std::ffi::OsStr::to_str)
            .expect("Failed to get file stem");
        let expected_svg_name = format!("{}.svg", expected_file_name);

        // Check if the expected SVG file exists in the output directory
        if let Ok(mut entries) = std::fs::read_dir(&output_dir) {
            let found = entries.any(|entry| {
                if let Ok(entry) = entry {
                    let os_str = entry.file_name();
                    let file_name = os_str.to_string_lossy();
                    file_name == expected_svg_name
                } else {
                    false
                }
            });
            assert!(found, "SVG file '{}' should exist in {}", expected_svg_name, output_dir_str);
        } else {
            assert!(false, "Failed to read output directory: {}", output_dir_str);
        }
    }
}