# AI 实力测试（tauri 端引擎）

一键测试**不同棋盘（5 种形状 × 4 种边界 × 3 种爆炸阈值）下各 AI 算法（策略 / A-B / PVS / MCTS）的胜率**，
带实时进度条，输出 Markdown 报告 + 对局数据 JSON。

## 架构

```
tauri/tests/
├── ai_strength.py     # 调度入口：构造任务 → 调 ai_bench → 进度条 → 生成报告
└── ai_report.md       # 报告输出（默认）
engine/src/lib.rs      # 共享引擎：simulate_to_end 等（tauri 与 wasm 共用）
tauri/src-tauri/
└── src/bin/ai_bench.rs# Rust 基准引擎：读 JSON 配置 → 逐局模拟 → stdout 逐行 JSON 结果
```

- **引擎**：`engine/` 共享 crate（`chain_chess_core`），与正式对战完全同源
- **对局**：`simulate_to_end`（一键终局同款），4 算法各执一色混战（ffa）或两两对阵（duel）
- **棋盘形状**：`square` 正方形 / `rect` 长方形 / `holes` 挖洞 / `ring` 口字 / `cross` 十字，
  全部确定性生成（同一 seed 可跨版本比较）。口字与十字的带宽取边长的 1/3，与前端形状
  编辑器的预设一致；异形棋盘的空洞由引擎按形状边界处理
- **初始局面**：从可用格里贪心散布首子，**距离用边界语义的 BFS**（回环会把隔着洞的
  两格连成邻居，直线距离会骗人），首子等级取「本格阈值 − 1」（降级边界下边缘格阈值
  更低，硬编码会让首子超临界、AI 无棋可走）
- **评估函数**：`--ml`（默认）走 XGBoost 模型，`--no-ml` 走手写规则；策略 / MCTS 不用评估函数
- **口径**：应用内「AI 性能评测」页对应 `--quick`（5 形状 × 4 边界 × 3 阈值 × 6 配对 × 1 局 = 360 局）。
  页面内容由 `gen_report_html.py` 从 `ai_report.json` 生成，改了报告格式或重跑数据后执行一次即可

## 使用

```bash
# 1) 编译基准引擎（一次即可）
cd tauri/src-tauri
cargo build --release --bin ai_bench

# 2) 运行测试（在 tauri/ 目录下）
cd ..
python3 tests/ai_strength.py                          # 默认：7×7，每组合 6 局，4 算法混战
python3 tests/ai_strength.py --games 10 --size 9      # 更多局数 / 更大棋盘
python3 tests/ai_strength.py --mode duel              # 两两对阵（正方形单形状，432 局）
python3 tests/ai_strength.py --quick                  # 快速全覆盖：duel + 5 形状 + 每组合 1 局（360 局）
python3 tests/ai_strength.py --mode duel --shapes all # 两两对阵 + 全部形状（2160 局）
python3 tests/ai_strength.py --mode duel --no-ml      # 手写评估对照
python3 tests/ai_strength.py --mode duel --tag "ML 模型"   # 给报告加标题后缀
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
| `--shapes` | square | `square,rect,holes,ring,cross`，或 `all` |
| `--quick` | 关 | 快速全覆盖：duel + 全部形状 + 每组合 1 局 |
| `--borders` | default,wrap,bounce,degrade | 边界模式过滤 |
| `--caps` | 3,4,5 | 爆炸阈值过滤（引擎不再支持 mixed） |
| `--algs` | strategy,alphabeta,pvs,mcts | 参与算法（ffa 每玩家一算法；duel 用前两个或全配对） |
| `--depth` | 2 | A-B/PVS 搜索深度 |
| `--mcts-depth` | 1 | MCTS 深度（迭代 = depth×800） |
| `--random` | 5 | 搜索随机刻度 % |
| `--ml` / `--no-ml` | `--ml` | A-B / PVS 用 ML 模型还是手写评估 |
| `--tag` | 空 | 报告标题后缀（区分 ML / 手写对照） |
| `--seed` | 0 | 对局号偏移（同 seed 可复现同一批对局） |
| `--out` | tests/ai_report.md | Markdown 报告路径 |
| `--out-json` | tests/ai_report.json | 对局数据 JSON（供图表/分析） |

## 刷新应用内评测页

页面里的数字全部由数据生成，重跑对局后执行：

```bash
python3 tests/gen_report_html.py                    # 用默认 ai_report.json，写两份 index.html
python3 tests/ai_strength.py --from-json tests/ai_report.json   # 只重建 Markdown 报告（不重跑）
```

## 输出

- **Markdown 报告**：算法总体排名（胜场/胜率/平均淘汰名次）、分棋盘组合胜率矩阵、
  平均步数矩阵、两两对阵表（duel 模式）
- **JSON 数据**：全部对局原始结果（`{bm, cm, mode, players, winner, steps, order}`），
  可用于绘制图表或二次分析
- **依赖**：`python3`、`tqdm`（无 tqdm 自动降级为内置进度条）、`cargo`/`rustc`（编译引擎一次）
