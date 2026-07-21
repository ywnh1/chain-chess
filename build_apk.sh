#!/bin/sh
# build_apk.sh - 编译并签名连锁棋 APK
# 用法: ./build_apk.sh <keystore_password>
# 例如: ./build_apk.sh chainchess

set -e

if [ $# -lt 1 ]; then
  echo "用法: $0 <keystore_password>"
  echo "例如: $0 chainchess"
  exit 1
fi

PASSWORD="$1"
KEYSTORE="release.keystore"
PRODUCT="连锁棋"
VERSION="2.0.0-beta"
OUTPUT="release/${PRODUCT}-${VERSION}.apk"
UNSIGNED_APK="tauri/src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk"

# 检查 keystore
if [ ! -f "$KEYSTORE" ]; then
  echo "错误: 找不到 keystore 文件 $KEYSTORE"
  exit 1
fi

echo "=== 编译 arm64 APK ==="
npx tauri android build --target aarch64

echo ""
echo "=== 验证编译产物 ==="
if [ ! -f "$UNSIGNED_APK" ]; then
  echo "错误: 编译产物不存在: $UNSIGNED_APK"
  exit 1
fi

echo "=== 签名 APK ==="
mkdir -p release
cp "$UNSIGNED_APK" "$OUTPUT"
apksigner sign --ks "$KEYSTORE" --ks-pass "pass:${PASSWORD}" --out "$OUTPUT" "$OUTPUT"

echo ""
echo "=== 验证签名 ==="
apksigner verify --verbose "$OUTPUT"

echo ""
echo "✅ 完成: $OUTPUT"
ls -lh "$OUTPUT"
