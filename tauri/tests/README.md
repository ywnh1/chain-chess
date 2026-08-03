# AI 实力测试（tauri 端引擎）

一键测试**不同棋盘（4 种边界 × 4 种爆炸阈值）下各 AI 算法（策略 / A-B / PVS / MCTS）的胜率**，
带进度条，输出 Markdown 报告 + 对局数据 JSON。

## 架构

```
tauri/tests/
├── ai_strength.py     # 调度入口：构造任务 → 调 ai_bench → 进度条 → 生成报告
└── ai_report.md       # 报告输出（默认）
tauri/src-tauri/
├── src/lib.rs         # 引擎：simulate_to_end（已提取为 pub fn 供基准复用）
└── src/bin/ai_bench.rs# Rust 基准引擎：读 JSON 配置 → 逐局模拟 → stdout 逐行 JSON 结果
```

- **引擎**：tauri 端 Rust（`chain_chess_lib`），与正式对战完全同源
- **对局**：`simulate_to_end`（一键终局同款），4 算法各执一色混战（ffa）或两两对阵（duel）
- **初始局面**：按「首子 12 格距离限制」内圈均匀散布每玩家一枚首子（count=3），模拟真实中盘；
  混合棋盘(mixed)按对局号确定性分配每格阈值 3/4/5

## 使用

```bash
# 1) 编译基准引擎（一次即可）
cd tauri/src-tauri
cargo build --release --bin ai_bench

# 2) 运行测试（在 tauri/ 目录下）
cd ..
python3 tests/ai_strength.py                          # 默认：7×7，每组合 6 局，4 算法混战
python3 tests/ai_strength.py --games 10 --size 9      # 更多局数 / 更大棋盘
python3 tests/ai_strength.py --mode duel              # 两两对阵（全配对）
python3 tests/ai_strength.py --algs strategy,alphabeta --mode duel   # 指定算法对阵
python3 tests/ai_strength.py --caps 3,4 --borders default,wrap       # 只测部分组合
python3 tests/ai_strength.py --out report.md --out-json data.json    # 自定义输出
```

### 参数

| 参数 | 默认 | 说明 |
|---|---|---|
| `--size` | 7 | 棋盘大小 |
| `--games` | 6 | 每组合对局数（总对局 = 组合数 × games） |
| `--mode` | ffa | `ffa` 多算法混战 / `duel` 两两对阵 |
| `--borders` | default,wrap,bounce,degrade | 边界模式过滤 |
| `--caps` | 3,4,5,mixed | 爆炸阈值过滤 |
| `--algs` | strategy,alphabeta,pvs,mcts | 参与算法（ffa 每玩家一算法；duel 用前两个或全配对） |
| `--depth` | 2 | A-B/PVS 搜索深度 |
| `--mcts-depth` | 1 | MCTS 深度（迭代 = depth×800） |
| `--random` | 5 | 搜索随机刻度 % |
| `--seed` | 0 | 对局号偏移（可复现混合棋盘分布） |
| `--out` | tests/ai_report.md | Markdown 报告路径 |
| `--out-json` | tests/ai_report.json | 对局数据 JSON（供图表/分析） |

## 输出

- **Markdown 报告**：算法总体排名（胜场/胜率/平均淘汰名次）、分棋盘组合胜率矩阵、
  平均步数矩阵、两两对阵表（duel 模式）
- **JSON 数据**：全部对局原始结果（`{bm, cm, mode, players, winner, steps, order}`），
  可用于绘制图表或二次分析
- **依赖**：`python3`、`tqdm`（无 tqdm 自动降级为内置进度条）、`cargo`/`rustc`（编译引擎一次）
