#!/bin/sh
# AI 评测一键跑：编译 ai_bench（若需要）→ 跑对局 → 出报告
#
# 用法（仓库根目录）:
#   ./todo.sh                  # 默认：2 形状 × 2 边界 × 3 配对 × 30 局/格 = 360 局
#   ./todo.sh --games 10       # 参数原样透传给 ai_strength.py，常用覆盖见下
#   ./todo.sh --shapes all     # 加测长方形 / 挖洞 / 十字
#   ./todo.sh --no-ml          # 手写评估对照
#
# 报告输出到 tauri/tests/ai_report.md 与 ai_report.json。
# 这两个文件是评测页的数据源，跑完执行一次
#   python3 tauri/tests/gen_report_html.py
# 就能把应用内「AI 性能评测」页刷新成新数据。

set -e
cd "$(dirname "$0")"

echo "==> 编译 ai_bench（release）"
(cd tauri/src-tauri && cargo build --release --bin ai_bench)

echo ""
echo "==> 跑对局"
python3 tauri/tests/ai_strength.py \
  --mode duel \
  --shapes square,ring \
  --caps 4 \
  --borders default,wrap \
  --algs strategy,alphabeta,pvs \
  --games 30 \
  --seed 7 \
  "$@"

echo ""
echo "==> 完成。刷新应用内评测页:"
echo "    python3 tauri/tests/gen_report_html.py"
