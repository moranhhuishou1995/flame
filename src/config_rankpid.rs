use std::fs::File;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::process::{Command, Output};
use std::net::TcpListener;
use std::path::Path;
use thiserror::Error;
use chrono::Local;
use get_if_addrs::get_if_addrs;
use serde_json;

/// 进程 RANK 信息
#[derive(Debug, Clone)]
pub struct ProcessRank {
    pub pid: u32,
    pub local_rank: u32,
}

/// 进程信息获取错误
#[derive(Debug, thiserror::Error)]
pub enum ProcessRankError {
    #[error("IO error: {0}")]
    IoError(io::Error),
    #[error("Pgrep failed: {0}")]
    PgrepFailed(String),
    #[error("Invalid UTF-8 in environment: {0}")]
    Utf8Error(std::string::FromUtf8Error),
    #[error("Invalid LOCAL_RANK value: {0}")]
    InvalidLocalRank(String),
    #[error("Failed to execute command: {0}")]
    CommandFailed(String),
    #[error("No processes with LOCAL_RANK found")]
    NoProcessesFound,
    #[error("No valid network interfaces found")]
    NoValidInterfaces,
}

/// 进程 RANK 信息 API
pub struct ProcessRankApi;

impl ProcessRankApi {
    /// 获取所有 Python 进程的 LOCAL_RANK 信息
    pub fn get_all_python_local_ranks() -> Result<Vec<ProcessRank>, ProcessRankError> {
        let pids = Self::get_python_processes()?;
        let mut ranks = Vec::new();

        for pid in pids {
            if let Ok(Some(local_rank)) = Self::get_process_local_rank(pid) {
                ranks.push(ProcessRank { pid, local_rank });
            }
        }

        if ranks.is_empty() {
            return Err(ProcessRankError::NoProcessesFound);
        }

        // 按 LOCAL_RANK 排序
        ranks.sort_by_key(|r| r.local_rank);
        
        Ok(ranks)
    }

    /// 获取指定进程的 LOCAL_RANK 信息
    fn get_process_local_rank(pid: u32) -> Result<Option<u32>, ProcessRankError> {
        let environ_path = format!("/proc/{}/environ", pid);
        let mut file = File::open(&environ_path)
            .map_err(ProcessRankError::IoError)?;

        let mut contents = Vec::new();
        file.read_to_end(&mut contents)
            .map_err(ProcessRankError::IoError)?;

        let env_str = String::from_utf8(contents)
            .map_err(ProcessRankError::Utf8Error)?;
        let env_vars = env_str
            .split('\0')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        let local_rank_str = Self::find_env_var(&env_vars, "LOCAL_RANK=");
        
        match local_rank_str {
            Some(value) => value.parse::<u32>()
                .map(Some)
                .map_err(|_| ProcessRankError::InvalidLocalRank(value)),
            None => Ok(None),
        }
    }

    /// 获取所有 Python 进程的 PID 列表
    fn get_python_processes() -> Result<Vec<u32>, ProcessRankError> {
        let output = Command::new("pgrep")
            .arg("python")
            .output()
            .map_err(ProcessRankError::IoError)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProcessRankError::PgrepFailed(stderr.to_string()));
        }

        let stdout = String::from_utf8(output.stdout)
            .map_err(ProcessRankError::Utf8Error)?;
            
        let pids = stdout
            .trim()
            .split('\n')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect();

        Ok(pids)
    }

    /// 从环境变量列表中查找指定前缀的变量值
    fn find_env_var(env_vars: &[&str], prefix: &str) -> Option<String> {
        env_vars.iter()
            .find(|&&var| var.starts_with(prefix))
            .map(|var| var[prefix.len()..].to_string())
    }

    /// 为每个进程分配 IP 和端口并执行配置命令
    pub fn configure_processes_with_ports(
        processes: &[ProcessRank],
        base_port: Option<u16>,
    ) -> Result<Vec<(u32, String, u16)>, ProcessRankError> {
        let base_port = base_port.unwrap_or(12345);
        let mut configured = Vec::with_capacity(processes.len());

        for process in processes {
            let (available_ip, available_port) = find_available_port(base_port)?;
            let next_port = available_port + 1;
            
            let address = format!("{}:{}", available_ip, available_port);
            let config = format!("probing.server.address='{}'", address);
            
            // 执行配置命令
            Self::execute_probing_command(process.pid, &config)?;
            
            configured.push((process.pid, address, available_port));
        }

        Ok(configured)
    }

    /// 执行 probing 配置命令
    fn execute_probing_command(pid: u32, config: &str) -> Result<(), ProcessRankError> {
        let command = format!("probing -t {} config \"{}\"", pid, config);
        let output = Command::new("sh")
            .arg("-c")
            .arg(&command)
            .output()
            .map_err(|e| ProcessRankError::CommandFailed(e.to_string()))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ProcessRankError::CommandFailed(
                format!("Command '{}' failed: {}", command, stderr)
            ));
        }

        Ok(())
    }

    pub fn write_rank_ports_to_json(
        configured: &[(u32, String, u16)],
        json_path: Option<&Path>, // 修改为可选参数
    ) -> Result<(), ProcessRankError> {
        // 创建 JSON 对象
        let mut rank_ports = serde_json::Map::new();
        
        // 将每个 rank 的 IP:端口信息添加到 JSON 对象
        for (index, (_, address, _)) in configured.iter().enumerate() {
            let key = format!("rank{}", index);
            rank_ports.insert(key, serde_json::Value::String(address.clone()));
        }
        
        // 将 JSON 对象序列化为字符串
        let json_str = serde_json::to_string_pretty(&rank_ports)
            .map_err(|e| ProcessRankError::CommandFailed(e.to_string()))?;
        
        // 确定写入文件的路径
        let final_path = match json_path {
            Some(path) => path.join("urls.json"),
            None => {
                let date = Local::now().format("%Y%m%d").to_string();
                Path::new("/tmp").join(format!("output_{}", date)).join("url_config").join("urls.json")
            }
        };

        // 创建目录（如果不存在）
        if let Some(parent_dir) = final_path.parent() {
            std::fs::create_dir_all(parent_dir).map_err(|e| ProcessRankError::CommandFailed(e.to_string()))?;
        }

        // 写入文件
        let mut file = File::create(&final_path)
            .map_err(|e| ProcessRankError::CommandFailed(e.to_string()))?;
        
        file.write_all(json_str.as_bytes())
            .map_err(|e| ProcessRankError::CommandFailed(e.to_string()))?;

        // 打印文件路径
        println!("JSON file has been written to: {}", final_path.display());
        
        Ok(())
    }

    pub fn get_configure_and_write(
        base_port: Option<u16>,
        json_path: Option<&Path>, // 修改为可选参数
    ) -> Result<(), ProcessRankError> {
        // 获取所有 Python 进程的 LOCAL_RANK 信息
        let ranks = Self::get_all_python_local_ranks()?;

        // 为每个进程分配 IP 和端口并执行配置命令
        let configured = Self::configure_processes_with_ports(&ranks, base_port)?;

        // 将 rank 的 IP:端口信息写入 JSON 文件
        Self::write_rank_ports_to_json(&configured, json_path)?;

        Ok(())
    }
}

/// 查找可用端口
fn find_available_port(mut port: u16) -> Result<(IpAddr, u16), ProcessRankError> {
    const MAX_PORT: u16 = 65535;

    // Get the IP addresses of local network interfaces
    let interfaces = get_if_addrs().map_err(|e| {
        ProcessRankError::CommandFailed(format!("Failed to get network interfaces: {}", e))
    })?;

    let mut valid_ips: Vec<IpAddr> = interfaces
        .into_iter()
        // Filter out loopback addresses
        .filter(|iface| !iface.addr.ip().is_loopback())
        .filter_map(|iface| {
            let ip = iface.addr.ip();
            match ip {
                IpAddr::V4(_) | IpAddr::V6(_) => Some(ip),
                _ => None,
            }
        })
        .collect();

    if valid_ips.is_empty() {
        return Err(ProcessRankError::NoValidInterfaces);
    }

    while port <= MAX_PORT {
        for ip in &valid_ips {
            // Dereference the IP address reference
            match TcpListener::bind((*ip, port)) {
                Ok(_) => return Ok((*ip, port)),
                Err(_) => continue,
            }
        }
        port += 1;
    }

    Err(ProcessRankError::CommandFailed(
        "No available ports found in range".to_string(),
    ))
}