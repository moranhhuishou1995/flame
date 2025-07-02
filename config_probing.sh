#!/bin/bash

# 获取IP地址，默认为10.107.204.71
IP_ADDRESS=${1:-10.107.204.71}

# 获取起始端口号，默认为11490
START_PORT=${2:-11490}

# 执行ps命令获取/opt/conda/bin/python进程信息
echo "执行 ps 命令获取/opt/conda/bin/python进程信息..."
PROCESSES=$(ps -ef | grep "/opt/conda/bin/python" | grep -v grep)

# 检查是否有进程需要配置
if [ -z "$PROCESSES" ]; then
    echo "没有找到/opt/conda/bin/python相关进程！"
    exit 1
fi

# 使用关联数组存储rank到PID的映射
declare -A RANK_PID_MAP
echo "解析进程ID和rank信息..."
while IFS= read -r line; do
    # 提取PID（第2列）
    PID=$(echo "$line" | awk '{print $2}')

    # 尝试从环境变量中提取LOCAL_RANK（针对torchrun）
    ENV_RANK=$(tr '\0' '\n' < /proc/$PID/environ 2>/dev/null | grep "^LOCAL_RANK=" | cut -d= -f2)
    
    # 尝试从命令行参数中提取local-rank（针对python -m torch.distributed.launch）
    CMD_RANK=$(echo "$line" | sed -n 's/.*--local-rank=\([0-9]*\).*/\1/p')
    
    # 优先使用环境变量中的LOCAL_RANK
    if [[ ! -z "$ENV_RANK" && "$ENV_RANK" =~ ^[0-9]+$ ]]; then
        RANK=$ENV_RANK
    elif [[ ! -z "$CMD_RANK" && "$CMD_RANK" =~ ^[0-9]+$ ]]; then
        RANK=$CMD_RANK
    else
        RANK=""
    fi

    # 只处理成功提取rank的行
    if [[ ! -z "$RANK" && "$RANK" =~ ^[0-9]+$ && ! -z "$PID" && "$PID" =~ ^[0-9]+$ ]]; then
        RANK_PID_MAP[$RANK]=$PID
        echo "已解析: rank=$RANK, PID=$PID"
    fi
done <<< "$PROCESSES"

# 检查是否成功提取到rank和PID
if [ ${#RANK_PID_MAP[@]} -eq 0 ]; then
    echo "无法从输出中解析出有效的rank和进程ID！"
    echo "原始输出:"
    echo "$PROCESSES"
    exit 1
fi

# 获取排序后的rank列表
SORTED_RANKS=($(for rank in "${!RANK_PID_MAP[@]}"; do echo $rank; done | sort -n))

# 为每个rank配置对应的端口
echo "开始配置每个rank的probing.server.address..."
PORT=$START_PORT
SUCCESS=0
FAILED=0

for rank in "${SORTED_RANKS[@]}"; do
    PID=${RANK_PID_MAP[$rank]}
    ADDRESS="${IP_ADDRESS}:${PORT}"

    echo -n "配置 rank $rank (PID: $PID) 到地址: $ADDRESS ... "

    # 检查进程是否存在
    if ! ps -p "$PID" > /dev/null; then
        echo "失败 (进程 $PID 不存在)"
        ((FAILED++))
        continue
    fi

    # 执行配置命令并捕获输出
    OUTPUT=$(probing -t $PID config "probing.server.address='$ADDRESS'" 2>&1)

    # 检查命令执行状态
    if [ $? -eq 0 ]; then
        echo "成功"
        ((SUCCESS++))
    else
        echo "失败"
        echo "  错误信息: $OUTPUT"
        ((FAILED++))
    fi

    # 增加端口号
    ((PORT++))
done

echo "配置完成！成功: $SUCCESS, 失败: $FAILED"

if [ $FAILED -gt 0 ]; then
    echo "警告: 部分配置失败，请检查错误信息"
    exit 1
else
    echo "所有配置已成功应用"
    exit 0
fi