use clap::{Command, Parser, Arg};
use crate::collector::fetch_stack_from_urls;
use crate::process::process_and_merge_callstacks;
use crate::draw_flame::draw_frame_graph;
use serde_json::from_str;
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use crate::config_rankpid::{ProcessRankApi, ProcessRankError};

/// 主命令结构体
#[derive(Parser, Debug)]
#[command(
    name = "flame",
    about = "Perform call stack collection, processing, and frame graph drawing tasks.",
    long_about = "This tool can collect call stack information from URLs, process call stack data, and draw frame graphs based on the processed results."
)]
struct Cli {
    /// 基于合并后的调用栈文件绘制火焰图（互斥选项）
    #[arg(
        short = 'i',
        long = "input",
        group = "action",
        help = "Path to the merged call stack file, used as the basis for drawing the frame graph."
    )]
    draw_input: Option<String>,

    /// 统一用于指定输出目录路径
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
        help = "Path to the output directory for storing the generated frame graph, the processed call stack information, or the rank configuration JSON file."
    )]
    output: Option<String>,

    /// 从 URL 列表获取调用栈信息并处理（互斥选项）
    #[arg(
        short = 'f',
        long = "file",
        group = "action",
        help = "Path to the file containing a JSON array of URLs."
    )]
    fetch_file: Option<String>,

    /// 调用 config_rankpid.rs 中的 get_configure_and_write 函数
    #[arg(
        short = 'c',
        long = "configure",
        num_args = 0..1, // 修改为 0 到 1 个参数
        value_name = "BASE_PORT",
        group = "action",
        help = "Call get_configure_and_write function in config_rankpid.rs. Optional base port. If not provided, default to 12345. The JSON file will be saved to the output directory specified by -o."
    )]
    configure: Option<Option<u16>>, // 修改为 Option<Option<u16>>

    #[arg(
        short = 'r',
        long = "rank",
        num_args = 1,
        value_name = "RANK:<IP:PORT>",
        action = clap::ArgAction::Append,
        help = "Specify the rank, IP address, and port number to get the call stack from the corresponding address. \
                The format should be RANK:<IP:PORT>, and this option can be used multiple times."
    )]
    ranks: Vec<String>,
}

/// 构建命令行解析器
pub fn build_cli() -> Command {
    <Cli as clap::CommandFactory>::command()
}

/// 合并 fetch_and_save_urls 和 process_and_merge_callstacks 为一个函数
pub async fn fetch_process_and_merge(url_file: &str, output: Option<&str>) -> Result<(), Box<dyn Error>> {
    let mut file = File::open(url_file)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    let json: serde_json::Value = serde_json::from_str(&contents)?;

    let mut urls = Vec::new();
    let mut rank_list = Vec::new();
    if let serde_json::Value::Object(map) = json {
        for (rank_str, value) in map {
            // 提取rank后的数字部分
            let rank_num_str = rank_str.trim_start_matches("rank");
            
            // 尝试解析数字部分
            if let Ok(rank) = rank_num_str.parse::<u32>() {
                rank_list.push(rank);
            }
            
            if let serde_json::Value::String(address) = value {
                let new_url = format!("http://{}/apis/pythonext/callstack", address);
                urls.push(new_url);
            }
        }
    }
    
    if urls.is_empty() {
        return Err("No valid URLs found in the file".into());
    }
    
    println!("Loaded {} URLs from file", urls.len());
    println!("Ranks parsed: {:?}", rank_list); // 打印解析的rank列表

    let json_data = fetch_stack_from_urls(urls).await?;
    process_and_merge_callstacks(&json_data, rank_list, output)?;

    Ok(())
}

async fn fetch_selected_rankstacks(ranks: Vec<String>, output: Option<&str>) -> Result<(), Box<dyn Error>> {
    let mut rank_list = Vec::new();
    let mut urls = Vec::new();
    
    for rank_str in ranks {
        let parts: Vec<&str> = rank_str.splitn(2, ':').collect();
        
        if parts.len() == 2 {
            // 去除排名部分的括号并解析
            let rank_part = parts[0].trim_matches(|c| c == '<' || c == '>');
            if let Ok(rank) = rank_part.parse::<u32>() {
                rank_list.push(rank);
            } else {
                eprintln!("Warning: Failed to parse rank from '{}'", parts[0]);
            }
            
            // 去除IP:PORT部分的括号
            let ip_port = parts[1].trim_matches(|c| c == '<' || c == '>');
            let url = format!("http://{}/apis/pythonext/callstack", ip_port);
            println!("Generated URL: {}", url);
            urls.push(url);
        } else {
            eprintln!("Warning: Invalid format '{}', expected '<rank>:<ip:port>'", rank_str);
        }
    }

    if urls.is_empty() {
        return Err("No valid URLs generated from -r arguments".into());
    }

    println!("Parsed ranks: {:?}", rank_list); // 调试输出
    
    let json_data = fetch_stack_from_urls(urls).await?;
    process_and_merge_callstacks(&json_data, rank_list, output)?;

    Ok(())
}

/// 解析命令行并调用相应函数
pub async fn run_cli() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();

    match (cli.draw_input, cli.fetch_file, cli.configure, !cli.ranks.is_empty()) {
        (Some(input), _, _, _) => {
            draw_frame_graph(&input, cli.output.as_deref());
            println!("Frame graph has been drawn successfully");
        }
        (_, Some(file), _, false) => {
            // 仅使用 -f 参数，原有从文件读取 URL 的逻辑
            fetch_process_and_merge(&file, cli.output.as_deref()).await?;
            println!("Call stacks have been collected, processed, and merged successfully");
        }
        (_, _, _, true) => {
            // 仅使用 -r 参数
            fetch_selected_rankstacks(cli.ranks, cli.output.as_deref()).await?;
            println!("Call stacks have been collected, processed, and merged successfully");
        }
        (_, _, Some(base_port), _) => {
            let json_path = cli.output.as_deref().map(|output| PathBuf::from(output).join("rank_ports.json"));
            match ProcessRankApi::get_configure_and_write(base_port, json_path.as_deref()) {
                Ok(()) => println!("Successfully configured ranks and wrote to JSON file."),
                Err(e) => eprintln!("Error configuring ranks: {}", e),
            }
        }
        _ => {
            // 如果没有提供任何选项，显示帮助信息
            eprintln!("Error: You must specify either -i/--input, -f/--file, -c/--configure or -r/--rank option.");
            eprintln!("Run `flame --help` for usage information.");
            std::process::exit(1);
        }
    }

    Ok(())
}