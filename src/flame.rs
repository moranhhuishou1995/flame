use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::PathBuf;

use inferno::flamegraph::{self, Options, Palette};
use regex::Regex;
use serde_json;

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
    ranks: BTreeSet<u32>, // Using BTreeSet to ensure uniqueness and ordering
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
        self.ranks.insert(rank); // Automatically handles duplicates
    }
}

/// Represents a Trie structure for merging stack traces.
pub struct StackTrie {
    pub root: TrieNode,
    all_ranks: BTreeSet<u32>, // Using BTreeSet to ensure uniqueness and ordering
}

impl StackTrie {
    fn new(all_ranks: Vec<u32>) -> Self {
        // Convert all_ranks to BTreeSet to ensure uniqueness and ordering
        let all_ranks_set: BTreeSet<_> = all_ranks.into_iter().collect();
        
        StackTrie {
            root: TrieNode::new(),
            all_ranks: all_ranks_set,
        }
    }

    fn insert(&mut self, stack: Vec<&str>, rank: u32) {
        let mut node = &mut self.root;
        for frame in stack {
            // Skip frames containing "lto_priv", consistent with Python implementation
            if frame.contains("lto_priv") {
                break;
            }
            
            node = node.children.entry(frame.to_string()).or_insert_with(TrieNode::new);
            node.add_rank(rank);
        }
        node.is_end_of_stack = true;
        node.add_rank(rank); // Keep this line consistent with Python implementation
    }

    fn format_rank_str(&self, ranks: &BTreeSet<u32>) -> String {
        // Convert to ordered vector
        let ranks_vec: Vec<_> = ranks.iter().cloned().collect();
        
        // Calculate leak_ranks using set operations to ensure correctness
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
                
                // Range merging logic consistent with Python implementation
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

/// Loads all mergedstack_rankN.json files from the specified directory, merges call stacks, and generates a flame graph
pub fn flame_jsonstacks(input_dir: &str, output_path: Option<&str>) -> Result<(), Box<dyn Error>> {
    // 1. Scan directory and collect all JSON files matching the format
    let re = Regex::new(r"^mergedstack_rank(\d+)\.json$")?;
    let entries = fs::read_dir(input_dir)?;
    let mut rank_files = Vec::new();

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() {
            if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                if let Some(captures) = re.captures(file_name) {
                    if let Some(rank_str) = captures.get(1) {
                        let rank: u32 = rank_str.as_str().parse()?;
                        rank_files.push((rank, path));
                    }
                }
            }
        }
    }

    if rank_files.is_empty() {
        return Err("No files matching mergedstack_rankN.json format found".into());
    }

    // Sort by rank number
    rank_files.sort_by_key(|&(rank, _)| rank);
    let all_ranks: Vec<u32> = rank_files.iter().map(|&(rank, _)| rank).collect();

    // 2. Read and parse all JSON files, extract call stacks
    let mut all_stacks = Vec::new(); // Stores (call stack frame strings, corresponding rank)
    for (rank, path) in &rank_files {
        println!("Loading rank {} data: {:?}", rank, path);
        
        // Read file content
        let mut file = File::open(path)?;
        let mut json_data = String::new();
        file.read_to_string(&mut json_data)?;

        // Parse into stack list
        let single_stack: Vec<Frame> = serde_json::from_str(&json_data)?;  // First parse as single stack
        let frames = vec![single_stack]; 

        // Process each call stack
        for trace in frames {
            let mut stack_frames = Vec::new();
            for frame in trace {
                // Format frame information (consistent with original logic)
                let frame_str = match frame {
                    Frame::CFrame(c) => format!("{} ({}:{})", c.func, c.file, c.lineno),
                    Frame::PyFrame(p) => format!("{} ({}:{})", p.func, p.file, p.lineno),
                };
                stack_frames.push(frame_str);
            }
            // Reverse stack order (top -> bottom)
            stack_frames.reverse();
            all_stacks.push((stack_frames, *rank));
        }
    }

    // 3. Merge call stacks using StackTrie
    let mut trie = StackTrie::new(all_ranks);
    for (stack, rank) in all_stacks {
        // Convert stack frames to &str slices for inserting into Trie
        let stack_refs: Vec<&str> = stack.iter().map(|s| s.as_str()).collect();
        trie.insert(stack_refs, rank);
    }

    // 4. Traverse Trie to generate flame graph input format
    let trie_result = trie.traverse_with_all_stack(&trie.root, Vec::new());
    let mut flamegraph_input = String::new();

    for (path, rank_str) in trie_result {
        // Flame graph format: path joined with semicolons, followed by weight (fixed at 1 here, could be adjusted based on rank)
        let stack_line = format!("{} {} 1\n", path.join(";"), rank_str);
        flamegraph_input.push_str(&stack_line);
    }

    // 5. Generate flame graph
    let (output_dir, file_name) = {
        let timestamp = Local::now().format("%Y%m%d%H%M%S").to_string();
        let name = format!("flamegraph_{}.svg", timestamp);

        match output_path {
            // Output to the same directory as input when no output path specified
            Some(path) => (PathBuf::from(path), name),
            None => (PathBuf::from(input_dir), name),
        }
    };

    // Create output directory if it doesn't exist
    fs::create_dir_all(&output_dir)?;
    let output_path = output_dir.join(file_name);

    // Configure flame graph generation options
    let mut options = Options::default();
    options.colors = Palette::Multi(flamegraph::color::MultiPalette::Java);

    // Generate flame graph from in-memory string
    let reader = BufReader::new(flamegraph_input.as_bytes());
    let mut output_file = File::create(&output_path)?;
    flamegraph::from_reader(&mut options, reader, &mut output_file)?;

    println!("Flame graph generated: {}", output_path.display());
    Ok(())
}
