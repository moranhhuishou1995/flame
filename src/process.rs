use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Write};
use serde_json;
use std::path::PathBuf;
use chrono::Local;
use std::env;

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
    ranks: Vec<u32>,
}

impl TrieNode {
    fn new() -> Self {
        TrieNode {
            children: HashMap::new(),
            is_end_of_stack: false,
            ranks: Vec::new(),
        }
    }

    fn add_rank(&mut self, rank: u32) {
        self.ranks.push(rank);
    }
}

/// Represents a Trie structure for merging stack traces.
pub struct StackTrie {
    pub root: TrieNode,
    all_ranks: Vec<u32>,
}

impl StackTrie {
    fn new(all_ranks: Vec<u32>) -> Self {
        StackTrie {
            root: TrieNode::new(),
            all_ranks,
        }
    }

    fn insert(&mut self, stack: Vec<&str>, rank: u32) {
        let mut node = &mut self.root;
        for frame in stack {
            node = node.children.entry(frame.to_string()).or_insert_with(TrieNode::new);
            node.add_rank(rank);
        }
        node.is_end_of_stack = true;
        node.add_rank(rank);
    }

    fn format_rank_str(&self, ranks: &[u32]) -> String {
        let mut ranks = ranks.to_vec();
        ranks.sort_unstable();
        let mut leak_ranks: Vec<u32> = self.all_ranks.iter().copied().filter(|r| !ranks.contains(r)).collect();
        leak_ranks.sort_unstable();

        fn inner_format(ranks: &[u32]) -> String {
            let mut str_buf = String::new();
            let mut low = 0;
            let mut high = 0;
            if ranks.len() == 0 {
                return str_buf;
            }
            while high < ranks.len() - 1 {
                let low_value = ranks[low];
                let mut high_value = ranks[high];
                while high < ranks.len() - 1 && high_value + 1 == ranks[high + 1] {
                    high += 1;
                    high_value = ranks[high];
                }
                low = high + 1;
                high += 1;
                if low_value != high_value {
                    str_buf.push_str(&format!("{}-{}", low_value, high_value));
                } else {
                    str_buf.push_str(&low_value.to_string());
                }
                if high < ranks.len() {
                    str_buf.push('/');
                }
            }
            if high == ranks.len() - 1 {
                str_buf.push_str(&ranks[high].to_string());
            }
            str_buf
        }

        let has_stack_ranks = inner_format(&ranks);
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


/// Process call stacks from a JSON file, merge them, and write the result to an output file.
pub fn process_and_merge_callstacks(input_file: &str, output_path: Option<&str>) -> io::Result<()> {
    // Read and parse the JSON file
    let mut file = File::open(input_file)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    // Parse the JSON data
    let frames:  Vec<Vec<Frame>> = serde_json::from_str(&contents)?;

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

    let all_ranks: Vec<u32> = (0..prepare_stacks.len() as u32).collect();
    let mut trie = StackTrie::new(all_ranks);
    for (rank, stack) in prepare_stacks.iter().enumerate() {
        let stack_frames: Vec<&str> = stack.split(';').collect();
        trie.insert(stack_frames, rank as u32);
    }

    // Determine the output file path
    let output_path = match output_path {
        // Use the specified output path if provided
        Some(path) => {
            let input_file_path = PathBuf::from(input_file);
            let file_stem = input_file_path.file_stem().and_then(std::ffi::OsStr::to_str).unwrap_or("output");
            let output_dir = PathBuf::from(path);
            // Create the output directory if it doesn't exist
            std::fs::create_dir_all(&output_dir)?;
            output_dir.join(format!("{}.txt", file_stem))
        }
        // Use the default output path if not provided
        None => {
            let input_file_path = PathBuf::from(input_file);
            // Get the parent directory of the input file
            let input_parent_dir = input_file_path.parent().unwrap_or_else(|| std::path::Path::new("."));
            // Get the grandparent directory of the input file
            let grandparent_dir = input_parent_dir.parent().unwrap_or_else(|| std::path::Path::new("."));
            let file_stem = input_file_path.file_stem().and_then(std::ffi::OsStr::to_str).unwrap_or("output");
            // Set the output directory to the 'merged_stack' folder under the grandparent directory
            let output_dir = grandparent_dir.join("merged_stack");
            // Create the output directory if it doesn't exist
            std::fs::create_dir_all(&output_dir)?;
            output_dir.join(format!("{}.txt", file_stem))
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
        // Call the function to process and merge call stacks
        process_and_merge_callstacks(input_file_path, Some(output_dir)).expect("Processing failed");

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