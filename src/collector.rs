use reqwest;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use futures::future::join_all; 
use chrono::Local;
use std::path::PathBuf;
use std::env;

/// Fetches JSON data from a list of URLs and saves the combined data to a file.
pub async fn fetch_and_save_urls(urls: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();

    let mut tasks = Vec::new();
    for url in urls {
        let client = client.clone();
        tasks.push(async move {
            let res = client.get(&url).send().await?;
            let body = res.text().await?;
            // 显式将 serde_json::Error 转换为 Box<dyn std::error::Error>
            let json: Value = serde_json::from_str(&body).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            Ok(json)
        });
    }

    let results: Vec<Result<Value, Box<dyn std::error::Error>>> = join_all(tasks).await;

    let mut data_list = Vec::new();
    for result in results {
        match result {
            Ok(json) => data_list.push(json),
            Err(e) => eprintln!("Error: {}", e),
        }
    }

    let output = serde_json::to_string_pretty(&data_list)?;

    // Get the current date
    let date = Local::now().format("%Y%m%d").to_string();
    // Get the project root directory
    let project_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Build the output directory path
    let output_dir = project_root.join(format!("output_{}", date)).join("url_stack");

    // Ensure the output directory exists
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        panic!("Failed to create output directory: {}", e);
    }

    // Get the current timestamp
    let timestamp = Local::now().format("%Y%m%d%H%M%S").to_string();
    // Build the output file path
    let output_path = output_dir.join(format!("stacktrace_{}.json", timestamp));

    let mut file = File::create(output_path.clone())?;
    file.write_all(output.as_bytes())?;

    println!("Data has been saved to {}", output_path.display());

    Ok(())
}