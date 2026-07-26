#!/bin/sh
# build_apk.sh - 编译并签名连锁棋 APK
# 用法: ./build_apk.sh <keystore_password>
# 例如: ./build_apk.sh chainchess

set -e

# ── 配置 ──────────────────────────────────────────────────
KEYSTORE="release.keystore"
PRODUCT="连锁棋"
VERSION="2.3.8"
OUTPUT="release/${PRODUCT}-${VERSION}.apk"
UNSIGNED_APK="tauri/src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk"

# ── 参数检查 ──────────────────────────────────────────────
if [ $# -lt 1 ]; then
  echo "用法: $0 <keystore_password>"
  echo "例如: $0 chainchess"
  exit 1
fi
PASSWORD="$1"

if [ ! -f "$KEYSTORE" ]; then
  echo "❌ 错误: 找不到 keystore 文件 $KEYSTORE"
  exit 1
fi

START_EPOCH=$(date +%s)

# ── 步骤 1: 编译 APK ─────────────────────────────────────
# Tauri v2 的 android build 会一次性完成 Rust 交叉编译 + APK 打包。
# TAURI_ANDROID_SKIP_RUST_BUILD 防止 Gradle 重复编译 Rust lib。
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  🔨 编译 arm64 APK"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  产物: ${OUTPUT}"
echo ""

export TAURI_ANDROID_SKIP_RUST_BUILD=1
cd tauri
npx tauri android build --target aarch64 --apk
cd ..

# ── 步骤 2: 签名 ──────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  📝 签名 APK"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  keystore: ${KEYSTORE}"
echo ""

if [ ! -f "$UNSIGNED_APK" ]; then
  echo "❌ 错误: 编译产物不存在: $UNSIGNED_APK"
  echo "  编译阶段可能失败，请检查上方日志。"
  exit 1
fi

mkdir -p release
cp "$UNSIGNED_APK" "$OUTPUT"
apksigner sign --ks "$KEYSTORE" --ks-pass "pass:${PASSWORD}" --out "$OUTPUT" "$OUTPUT"

# ── 步骤 3: 验证签名 ──────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  ✅ 验证签名"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

apksigner verify --verbose "$OUTPUT" | sed 's/^/  /'

# ── 完成 ──────────────────────────────────────────────────
ELAPSED=$(( $(date +%s) - START_EPOCH ))
FILESIZE=$(ls -lh "$OUTPUT" | awk '{print $5}')

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  🎉 构建完成！"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  文件: ${OUTPUT}"
echo "  大小: ${FILESIZE}"
echo "  耗时: ${ELAPSED}s"
echo ""

cp release /storage/emulated/0/用户/ -r

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  🎉 复制完成！"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
