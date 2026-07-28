#!/bin/sh
# run_ranking.sh — AI 战力排位赛
# 编译 battle → 运行 15 组循环赛 → 生成 AI_RANKING.md
# 用法: ./run_ranking.sh
# 预计耗时: 30~120 分钟（取决于深度和局数）

set -e

ROOT="$(cd "$(dirname "$0")" && pwd)"
BATTLE_BIN="${ROOT}/tauri/src-tauri/target/release/battle"

echo "=============================="
echo "  ♟ 连锁棋 AI 战力排位赛"
echo "=============================="
echo ""

# 第 1 步：编译 battle（只编译，不跑测试）
echo "🔨 编译 battle binary..."
cd "${ROOT}/tauri/src-tauri"
cargo build --release --bin battle 2>&1 | tail -3
echo "✅ 编译完成"
echo ""

# 第 2 步：运行排位赛
echo "🏟  开始排位赛..."
echo "    6 个选手 × 15 组对战 × 10 局 = 150 局"
echo ""
cd "${ROOT}"
python3 run_ranking.py 2>&1

echo ""
echo "✅ 排位赛结束！报告文件: AI_RANKING.md"
