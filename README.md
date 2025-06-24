# FLAME介绍

>堆栈合并（Stack Merging）是一种技术，用于将多个调用堆栈中的信息汇总到一起，以便于分析和优化。
>
>火焰图则是一种图形化工具，通过可视化的方式展示函数调用的层次和时间分布，帮助开发者快速定位性能瓶颈。

## 1. 收集数据

**前置条件**: 确保已经启用了性能分析工具Probing网页服务，并且已经生成了性能分析数据.

## 2. 堆栈合并

收集多个rank堆栈信息并进行合并.

## 3. 火焰图绘制

定制化火焰图生成.

## 4. 编译安装

安装好rust工具链后, 执行以下命令:
  
```bash
git https://github.com/moranhhuishou1995/flame
cd flame
cargo build
```

## 5. 使用方式

### 5.1 配置url

在url_config文件夹下的urls.json文件中，配置各个节点需要拉取的堆栈信息的url地址

### 5.2 获取个节点堆栈的json数据

执行以下命令:

```bash
./mapp fetch -f /home/zj/wangqi/flame/url_config/urls.json
```

### 5.3 合并及处理json数据

执行以下命令合并处理json数据，-i参数文件需要为json格式数据, 是必须传入的参数:

```bash
./myapp process -i /home/zj/wangqi/flame/output_20250623/url_stack/merged_output.json
```

也可以通过-o参数指定处理后的文件的输出路径:

```bash
./myapp process -i /home/zj/wangqi/flame/output_20250623/url_stack/merged_output.json -o /home/zj/wangqi/flame/output_20250623/merged_stack/
```

### 5.4 生成火焰图

执行以下命令生成堆栈火焰图, -i参数为合并后的堆栈信息文件，是必须传入的参数:

```bash
./myapp draw -i /home/zj/wangqi/flame/output_20250623/merged_stack/merged_output.txt
```

也可以通过-o参数指定输出的火焰图文件路径:

```bash
./myapp draw -i /home/zj/wangqi/flame/output_20250623/merged_stack/merged_output.txt -o /home/zj/wangqi/flame/output_20250623/flame_svg
```

## 0x04 Output文件说明

- `urls.json` 为各个节点的url配置文件;
- `merged_output.json` 为从各节点拉取的;
- `merged_output.txt` 为合并后的堆栈信息;
- `merged_output.svg` 为生成的火焰图;
