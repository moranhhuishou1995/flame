use reqwest;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use futures::future::join_all; 
use chrono::Local;
use std::path::PathBuf;
use std::env;

/// Fetches JSON data from a list of URLs and saves the combined data to a file.
pub async fn fetch_stack_from_urls(urls: Vec<String>) -> Result<String, Box<dyn std::error::Error>> {
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

    let results: Vec<Result<Value, Box<dyn std::error::Error>>> = futures::future::join_all(tasks).await;

    let mut data_list = Vec::new();
    for result in results {
        match result {
            Ok(json) => data_list.push(json),
            Err(e) => eprintln!("Error: {}", e),
        }
    }

    let output = serde_json::to_string_pretty(&data_list)?;

    println!("Data has been processed successfully");

    Ok(output)
}