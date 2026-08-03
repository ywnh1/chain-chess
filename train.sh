#!/bin/sh
# train.sh — 连锁棋 ML 模型训练 & 数据管理
#
# 用法:
#   ./train.sh collect [局数] [seed]   # 生成混合随机自对弈数据（默认 5000 局 / seed 42）
#   ./train.sh train [文件...]          # 用 data/ 下全部（或指定）数据训练，输出并同步两份模型
#   ./train.sh stats                    # 查看 data/ 下数据文件概览
#   ./train.sh smoke                    # 冒烟测试训练脚本（合成数据）
#   ./train.sh clean [确认=y]           # 删除 data/ 下全部数据文件（需 ./train.sh clean y 确认）
#   ./train.sh                          # 等同 train
#
# 说明:
#   - 所有数据统一放在 data/ 下，文件名 selfplay_*.jsonl 才会被 train 自动收集
#   - collect 生成的文件带时间戳，不会覆盖旧数据
#   - 训练输出模型同时写入 tauri 与 pwa 两份 xgb_model_board.json，桌面端与 PWA 保持一致

set -e

ROOT="$(cd "$(dirname "$0")" && pwd)"
DATA_DIR="${ROOT}/data"
SCRIPT="${ROOT}/train_xgb_model.py"
SELFPLAY_BIN="${ROOT}/tauri/src-tauri"
MODEL_TAURI="${ROOT}/tauri/src-tauri/xgb_model_board.json"
MODEL_PWA="${ROOT}/pwa/wasm/xgb_model_board.json"

mkdir -p "${DATA_DIR}"

# 数据文件列表：data/ 下所有 selfplay_*.jsonl，按修改时间旧→新排序
data_files() {
  ls -t "${DATA_DIR}"/selfplay_*.jsonl 2>/dev/null | tac || true
}

banner() {
  echo "=============================="
  echo "  ♟ 连锁棋 ML 模型训练"
  echo "=============================="
}

case "${1:-train}" in

  collect)
    games="${2:-5000}"
    seed="${3:-42}"
    out="${DATA_DIR}/selfplay_mixed_${games}_$(date +%H%M%S).jsonl"
    echo "📦 生成混合随机自对弈数据：${games} 局 / seed ${seed}"
    echo "   输出: ${out}"
    echo ""
    # 先编译一次 release 二进制（源码更新后自动重编），再跑对局
    (cd "${SELFPLAY_BIN}" && cargo build --release --bin selfplay)
    (cd "${SELFPLAY_BIN}" && cargo run --release --bin selfplay -- --mixed "${games}" "${out}" "${seed}")
    echo ""
    echo "✅ 数据已生成: ${out}"
    echo "   训练: ./train.sh train"
    ;;

  train)
    if [ "$#" -ge 2 ]; then
      shift
      FILES="$*"
      echo "📦 用指定文件训练: ${FILES}"
    else
      FILES=$(data_files)
      if [ -z "${FILES}" ]; then
        echo "❌ ${DATA_DIR} 下没有 selfplay_*.jsonl 数据，先运行 ./train.sh collect"
        exit 1
      fi
      echo "📦 用 data/ 下全部数据训练（共 $(echo "${FILES}" | wc -l) 个文件）"
    fi
    echo ""
    python3 "${SCRIPT}" ${FILES} --output "${MODEL_TAURI}"
    echo ""
    # 同步模型到 PWA（两份引擎共用同一模型，保持行为一致）
    cp -f "${MODEL_TAURI}" "${MODEL_PWA}"
    echo "✅ 模型已保存并同步:"
    echo "   ${MODEL_TAURI}"
    echo "   ${MODEL_PWA}"
    ;;

  stats)
    FILES=$(data_files)
    if [ -z "${FILES}" ]; then
      echo "📂 data/ 下暂无数据文件"
      exit 0
    fi
    echo "📂 data/ 数据文件概览:"
    echo ""
    total=0
    for f in ${FILES}; do
      n=$(wc -l < "${f}")
      sz=$(du -h "${f}" | cut -f1)
      total=$((total + n))
      printf "  %-45s %10s 行  %8s\n" "$(basename "${f}")" "$n" "$sz"
    done
    echo ""
    games_est=$(awk "BEGIN{printf \"%.0f\", ${total} / 60.0}")
    echo "  合计: ${total} 行（约 ${games_est} 局，按 60 步/局估）"
    echo ""
    echo "  训练: ./train.sh train"
    ;;

  smoke)
    echo "🧪 冒烟测试训练脚本（合成数据）..."
    python3 "${SCRIPT}" --smoke
    ;;

  clean)
    if [ "${2}" != "y" ]; then
      echo "⚠️  将删除 data/ 下所有 selfplay_*.jsonl 数据文件！"
      echo "   确认请运行: ./train.sh clean y"
      exit 1
    fi
    FILES=$(data_files)
    if [ -z "${FILES}" ]; then
      echo "📂 data/ 下已无数据文件"
      exit 0
    fi
    for f in ${FILES}; do
      rm -f "${f}"
      echo "  🗑  已删除: $(basename "${f}")"
    done
    echo "✅ 清理完成"
    ;;

  *)
    echo "❌ 未知命令: ${1}"
    echo "   用法: ./train.sh [collect|train|stats|smoke|clean]"
    exit 1
    ;;
esac
