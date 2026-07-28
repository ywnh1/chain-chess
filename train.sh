#!/bin/sh
# train.sh — 连锁棋 ML 模型训练 & 迭代自我对弈
# 用法: ./train.sh [v2|v3|v4]   # 默认 v1（从头训练）

set -e

ROOT="$(cd "$(dirname "$0")" && pwd)"
MODEL_DIR="${ROOT}/tauri/src-tauri"
DATA_DIR="${ROOT}/data"
SCRIPT="${ROOT}/train_xgb_model.py"
SELFPLAY_BIN="${ROOT}/tauri/src-tauri"

# 检测型号版本
VERSION="${1:-v1}"
OUTPUT="${MODEL_DIR}/xgb_model_board.json"

echo "=============================="
echo "  ♟ 连锁棋 ML 模型训练"
echo "  版本: ${VERSION}"
echo "=============================="
echo ""

case "${VERSION}" in
  v1)
    # 第 1 步：用手写评估数据重训模型（调参后）
    echo "📦 训练基线模型 (v1)..."
    python3 "${SCRIPT}" "${DATA_DIR}"/selfplay_*.jsonl --output "${OUTPUT}"
    ;;

  v2)
    # 第 2 步：用 v1 ML 模型生成新数据，合并训练
    echo "🤖 用 ML 模型自对弈生成 v2 数据..."
    cd "${SELFPLAY_BIN}" && cargo run --release --bin selfplay -- 7 2 3000 /tmp/selfplay_v2.jsonl
    cd "${ROOT}"
    echo "📦 合并训练 v2..."
    python3 "${SCRIPT}" "${DATA_DIR}"/selfplay_7_2_3000.jsonl /tmp/selfplay_v2.jsonl --output "${OUTPUT}"
    ;;

  v3)
    echo "🤖 用 ML 模型自对弈生成 v3 数据..."
    cd "${SELFPLAY_BIN}" && cargo run --release --bin selfplay -- 7 2 3000 /tmp/selfplay_v3.jsonl
    cd "${ROOT}"
    echo "📦 合并训练 v3..."
    python3 "${SCRIPT}" "${DATA_DIR}"/selfplay_7_2_3000.jsonl /tmp/selfplay_v2.jsonl /tmp/selfplay_v3.jsonl --output "${OUTPUT}"
    ;;

  v4)
    echo "🤖 用 ML 模型自对弈生成 v4 数据..."
    cd "${SELFPLAY_BIN}" && cargo run --release --bin selfplay -- 7 2 3000 /tmp/selfplay_v4.jsonl
    cd "${ROOT}"
    echo "📦 合并训练 v4..."
    python3 "${SCRIPT}" "${DATA_DIR}"/selfplay_7_2_3000.jsonl \
      /tmp/selfplay*.jsonl
      --output "${OUTPUT}"
    ;;

  all)
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "  🚀 开始三轮迭代自我对弈..."
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo ""
    # 先跑 v1 基线（不走二进制验证）
    echo "📦 [第 0/3 轮] 训练基线模型 (v1)..."
    python3 "${SCRIPT}" "${DATA_DIR}"/selfplay_*.jsonl --output "${OUTPUT}"
    echo ""
    # 执行 v2→v3→v4 轮次
    for iter in v2 v3 v4; do
      data_file="/tmp/selfplay_${iter}.jsonl"
      echo "━━━━━━━━━━━━━━━━━━━"
      echo "  🔄 第 ${iter#v}/3 轮"
      echo "━━━━━━━━━━━━━━━━━━━"
      echo "🤖 自对弈生成数据..."
      cd "${SELFPLAY_BIN}" && cargo run --release --bin selfplay -- 7 2 3000 "${data_file}"
      cd "${ROOT}"
      # 收集所有已生成的数据文件
      all_files="${DATA_DIR}/selfplay_7_2_3000.jsonl"
      for f in "/tmp/selfplay_v2.jsonl" "/tmp/selfplay_v3.jsonl" "/tmp/selfplay_v4.jsonl"; do
        [ -f "$f" ] && all_files="${all_files} ${f}"
      done
      echo "📦 合并训练..."
      python3 "${SCRIPT}" ${all_files} --output "${OUTPUT}"
      echo ""
    done
    ;;

  *)
    echo "❌ 未知版本: ${VERSION}，可选: v1 v2 v3 v4 all"
    exit 1
    ;;
esac

echo ""
echo "=============================="
echo "  ✅ 模型已保存: ${OUTPUT}"
echo "=============================="

# 验证模型嵌入
echo ""
echo "🔍 验证模型是否打包进二进制..."
cd "${SELFPLAY_BIN}" && cargo build --release 2>/dev/null
if strings target/release/chain-chess | grep -q "num_trees"; then
  echo "  ✅ 模型已成功嵌入二进制"
else
  echo "  ⚠️  模型未嵌入，检查 include_str! 路径"
fi
