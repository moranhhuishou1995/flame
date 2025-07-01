use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, BTreeSet}; // 新增 BTreeSet 导入
use std::error::Error;
use std::fs::File;
use std::io::{Read, Write};
use serde_json;
use std::path::PathBuf;
use std::env;

use reqwest;
use serde_json::Value;
use futures::future::join_all; 

/// Represents a frame in the call stack, which can be either a C frame or a Python frame.
#[derive(Debug, Deserialize, Serialize, Clone)]
enum Frame {
    CFrame(CFrame),
    PyFrame(PyFrame),
}

/// Represents a C frame in the call stack.
#[derive(Debug, Deserialize, Serialize, Clone)]
struct CFrame {
    file: String,
    func: String,
    ip: String,
    lineno: u32,
}

/// Represents a Python frame in the call stack.
#[derive(Debug, Deserialize, Serialize, Clone)]
struct PyFrame {
    file: String,
    func: String,
    lineno: u32,
    locals: serde_json::Value,
}

/// Represents a node in the Trie structure for stack traces.
#[derive(Debug, Clone)]
pub struct TrieNode {
    children: HashMap<String, TrieNode>,
    is_end_of_stack: bool,
    ranks: BTreeSet<u32>, // 使用BTreeSet确保唯一性和有序性
}

impl TrieNode {
    fn new() -> Self {
        TrieNode {
            children: HashMap::new(),
            is_end_of_stack: false,
            ranks: BTreeSet::new(),
        }
    }

    fn add_rank(&mut self, rank: u32) {
        self.ranks.insert(rank); // 自动去重
    }
}

/// Represents a Trie structure for merging stack traces.
pub struct StackTrie {
    pub root: TrieNode,
    all_ranks: BTreeSet<u32>, // 使用BTreeSet确保唯一性和有序性
}

impl StackTrie {
    fn new(all_ranks: Vec<u32>) -> Self {
        // 将all_ranks转换为BTreeSet确保唯一性和有序性
        let all_ranks_set: BTreeSet<_> = all_ranks.into_iter().collect();
        
        StackTrie {
            root: TrieNode::new(),
            all_ranks: all_ranks_set,
        }
    }

    fn insert(&mut self, stack: Vec<&str>, rank: u32) {
        let mut node = &mut self.root;
        for frame in stack {
            // 跳过包含"lto_priv"的帧，与Python实现保持一致
            if frame.contains("lto_priv") {
                break;
            }
            
            node = node.children.entry(frame.to_string()).or_insert_with(TrieNode::new);
            node.add_rank(rank);
        }
        node.is_end_of_stack = true;
        node.add_rank(rank); // 保留这行，与Python实现一致
    }

    fn format_rank_str(&self, ranks: &BTreeSet<u32>) -> String {
        // 转换为有序向量
        let ranks_vec: Vec<_> = ranks.iter().cloned().collect();
        
        // 计算leak_ranks，使用集合操作确保正确性
        let leak_ranks: Vec<_> = self.all_ranks
            .difference(ranks)
            .cloned()
            .collect();

        fn inner_format(ranks: &[u32]) -> String {
            if ranks.is_empty() {
                return String::new();
            }

            let mut ranges = Vec::new();
            let mut i = 0;
            let n = ranks.len();
            
            while i < n {
                let start = ranks[i];
                let mut end = start;
                
                // 与Python实现保持一致的区间合并逻辑
                while i + 1 < n && ranks[i + 1] == end + 1 {
                    end = ranks[i + 1];
                    i += 1;
                }
                
                let range_str = if start == end {
                    start.to_string()
                } else {
                    format!("{}-{}", start, end)
                };
                
                ranges.push(range_str);
                i += 1;
            }
            
            ranges.join("/")
        }

        let has_stack_ranks = inner_format(&ranks_vec);
        let leak_stack_ranks = inner_format(&leak_ranks);
        format!("@{}|{}", has_stack_ranks, leak_stack_ranks)
    }

    pub fn traverse_with_all_stack<'a>(&'a self, node: &'a TrieNode, path: Vec<&str>) -> Vec<(Vec<String>, String)> {
        let mut result = Vec::new();
        for (frame, child) in &node.children {
            let rank_str = self.format_rank_str(&child.ranks);
            if child.is_end_of_stack {
                let path_str = path.join(";");
                result.push((vec![path_str, frame.to_string()], rank_str.clone()));
            }
            let mut child_path = path.clone();
            let frame_rank = format!("{}{}", frame, rank_str);
            child_path.push(&frame_rank[..]);
            result.extend(self.traverse_with_all_stack(child, child_path));
        }
        result
    }
}

/// Process call stacks from a JSON string, merge them, and write the result to an output file.
pub fn process_and_merge_callstacks(json_data: &str, rank_list: Vec<u32>, output_path: Option<&str>) -> Result<(), Box<dyn Error>> {
    // Parse the JSON data
    let frames:  Vec<Vec<Frame>> = serde_json::from_str(json_data)?;

    // Process the call stacks
    let mut out_stacks = Vec::new();
    for (_, trace) in frames.iter().enumerate() {
        let mut local_stack = Vec::new();
        for frame in trace {
            local_stack.push(frame.clone());
        }
        local_stack.reverse();
        out_stacks.push(local_stack);
    }

    // Prepare stack strings
    let mut prepare_stacks = Vec::new();
    for rank in out_stacks {
        if!rank.is_empty() {
            let data = rank
                .iter()
                .map(|entry| match entry {
                    Frame::CFrame(frame) => format!("{} ({}:{})", frame.func, frame.file, frame.lineno),
                    Frame::PyFrame(frame) => format!("{} ({}:{})", frame.func, frame.file, frame.lineno),
                })
                .collect::<Vec<String>>()
                .join(";");
            prepare_stacks.push(data);
        }
    }

    // Initialize StackTrie directly using the provided rank list
    let mut trie = StackTrie::new(rank_list.clone());

    // Ensure the number of stacks does not exceed the number of ranks
    println!("prepare stacks length {}", prepare_stacks.len());
    println!("rank list length {}", rank_list.len());
    if prepare_stacks.len() > rank_list.len() {
        return Err("Number of stacks exceeds number of ranks".into());
    }

    for (index, stack) in prepare_stacks.iter().enumerate() {
        let stack_frames: Vec<&str> = stack.split(';').collect();
        // Use the rank value at the corresponding index in the rank list
        let rank = rank_list[index];
        trie.insert(stack_frames, rank);
    }

    // Determine the output file path
    let output_path = match output_path {
        // Use the specified output path if provided
        Some(path) => {
            let output_dir = PathBuf::from(path);
            // Create the output directory if it doesn't exist
            std::fs::create_dir_all(&output_dir)?;
            let timestamp = Local::now().format("%Y%m%d%H%M%S").to_string();
            output_dir.join(format!("stacktrace_{}.txt", timestamp))
        }
        // Use the default output path in /tmp/output_xxxx/merged_stack
        None => {
            let date = Local::now().format("%Y%m%d").to_string();
            let output_dir = PathBuf::from("/tmp").join(format!("output_{}", date)).join("merged_stack");
            // Create the output directory if it doesn't exist
            std::fs::create_dir_all(&output_dir)?;
            let timestamp = Local::now().format("%Y%m%d%H%M%S").to_string();
            output_dir.join(format!("stacktrace_{}.txt", timestamp))
        }
    };

    // Create the output file
    let mut output_file = File::create(&output_path)?;

    for (path, rank_str) in trie.traverse_with_all_stack(&trie.root, Vec::new()) {
        writeln!(output_file, "{} {} 1", path.join(";"), rank_str)?;
    }

    // Print the output file path
    println!("Output file path: {}", output_path.display());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// Test the `process_and_merge_callstacks` function.
    #[test]
    fn test_process_and_merge_callstacks() {
        // Define the path to the input file
        let input_file_path = "test/merged_output.json";
        // Check if the input file exists
        assert!(Path::new(input_file_path).exists(), "Input file does not exist");

        // Define the output directory
        let output_dir = "test";
        // Define the rank list
        let rank_list = vec![0, 1, 2]; 
        // Call the function to process and merge call stacks
        let json_data = fs::read_to_string(input_file_path).expect("Failed to read input file");
        process_and_merge_callstacks(&json_data, rank_list, Some(output_dir)).expect("Processing failed");

        // Verify if the output file exists
        let input_path = Path::new(input_file_path);
        let file_stem = input_path.file_stem().and_then(std::ffi::OsStr::to_str).unwrap();
        let expected_output_path = Path::new(output_dir).join(format!("{}.txt", file_stem));
        assert!(expected_output_path.exists(), "Output file should be created");

        // Verify that the output file content is not empty
        let output_content = fs::read_to_string(&expected_output_path).expect("Failed to read output file");
        assert!(!output_content.is_empty(), "Output file should not be empty");
    }
}