#!/bin/sh
# build.sh - 编译并签名连锁棋（APK / Windows exe）
# 用法: ./build.sh [--apk] [--exe] [--native] [--release] <keystore_password>
# 仅支持 release 构建（debug 模式已移除）
#       ./build.sh --apk chainchess           # 仅构建 APK（签名，需 keystore 密码）
#       ./build.sh --exe                      # 仅构建 Windows exe（cargo-xwin 交叉编译，无需密码）
#       ./build.sh --apk --exe chainchess     # 同时构建 APK + exe（默认）
#       ./build.sh --native chainchess        # release + target-cpu=native 极致优化（仅 APK）
#       ./build.sh --native --release chainchess

set -e

# ── 配置 ──────────────────────────────────────────────────
KEYSTORE="release.keystore"
PRODUCT="chainchess"
VERSION="3.3.0"

CARGO_CONFIG="tauri/src-tauri/.cargo/config.toml"
CARGO_TOML="tauri/src-tauri/Cargo.toml"
RUST_DIR="tauri/src-tauri"

# Windows exe 交叉编译目标（cargo-xwin）
EXE_TARGET="x86_64-pc-windows-msvc"
EXE_SRC="${RUST_DIR}/target/${EXE_TARGET}/release/chain-chess.exe"
EXE_OUTPUT="release/${PRODUCT}-${VERSION}.exe"

# ── 环境检测 ──────────────────────────────────────────
if [ -z "${ANDROID_HOME}" ] && [ -d "$HOME/Android/Sdk" ]; then
  export ANDROID_HOME="$HOME/Android/Sdk"
  echo "  📋 ANDROID_HOME 自动设为: $ANDROID_HOME"
fi

# ── 参数解析 ──────────────────────────────────────────────
# --apk  : 构建 Android APK（需 keystore 密码）
# --exe  : 构建 Windows exe（cargo-xwin）
# --pwa  : 仅打包 PWA zip（不编译）
# --all  : 打包全部（不编译）：pwa zip + 登记 release/ 已编译的 apk/exe size 到 update.json
# --release: 启用 release 激进优化（fat LTO 等，编译慢但性能/体积最优）
# --native : target-cpu=native 极致优化，自动视为 --release（仅 APK 生效）
APK=false
EXE=false
PWA=false
ALL=false
RELEASE=true
NATIVE=false
PASSWORD=""

for arg in "$@"; do
  case "$arg" in
    --apk) APK=true ;;
    --exe) EXE=true ;;
    --pwa) PWA=true ;;
    --all) ALL=true ;;
    --release) RELEASE=true ;;
    --native) NATIVE=true ;;
    -*)
      echo "❌ 未知参数: $arg"
      echo "用法: $0 [--apk] [--exe] [--native] [--release] <keystore_password>"
      exit 1
      ;;
    *) PASSWORD="$arg"
  esac
done

# 未指定平台时默认 APK + exe 都构建（--all / --pwa 除外）
if [ "$ALL" = false ] && [ "$PWA" = false ] && [ "$APK" = false ] && [ "$EXE" = false ]; then
  APK=true
  EXE=true
fi

# --native 自动 release
if [ "$NATIVE" = true ]; then
  RELEASE=true
fi

# APK 构建需要 keystore 密码
if [ "$APK" = true ]; then
  if [ -z "$PASSWORD" ]; then
    echo "用法: $0 [--apk] [--exe] [--native] [--release] <keystore_password>"
    echo "       $0 --apk chainchess"
    echo "       $0 --exe"
    echo "       $0 --native chainchess"
    exit 1
  fi

  if [ ! -f "$KEYSTORE" ]; then
    echo "❌ 错误: 找不到 keystore 文件 $KEYSTORE"
    exit 1
  fi
fi

START_EPOCH=$(date +%s)

# ── 构建模式提示 ──────────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
if [ "$APK" = true ] && [ "$EXE" = true ]; then
  echo "  📦 构建目标: APK + Windows exe"
elif [ "$APK" = true ]; then
  echo "  📦 构建目标: APK"
else
  echo "  📦 构建目标: Windows exe"
fi
if [ "$APK" = true ] && [ "$NATIVE" = true ]; then
  echo "  🚀 APK 模式: release + native（target-cpu=native 极致优化）"
fi
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

# ════════════════════════════════════════════════════════
# 步骤 0: --pwa / --all 版本管理 + 打包（不编译）
# --pwa: 仅打包 pwa zip；--all: pwa zip + 登记 release/ 已编译 apk/exe size
# ════════════════════════════════════════════════════════
if [ "$PWA" = true ] || [ "$ALL" = true ]; then
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  if [ "$ALL" = true ]; then
    echo "  📦 --all 模式：打包全部（pwa zip + 登记 apk/exe，不编译）"
  else
    echo "  📦 --pwa 模式：仅打包 PWA zip（不编译）"
  fi
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""

  # 确保 update.json 存在且版本正确
  if [ ! -f "update.json" ]; then
    echo "❌ 错误: 找不到 update.json"
    exit 1
  fi
  VERSION=$(jq -r '.version' update.json)
  echo "  📋 update.json 版本: ${VERSION}"

  # ── 打包 PWA zip（与现有结构一致：顶层目录带版本号）──
  PWA_ZIP="release/chain-chess-pwa-v${VERSION}.zip"
  echo ""
  echo "  📦 打包 PWA: ${PWA_ZIP}"
  if [ -d "pwa" ]; then
    TMPPKG=$(mktemp -d)
    cp -r pwa "$TMPPKG/chain-chess-pwa-v${VERSION}"
    # 排除 wasm 源码目录（含 target/vendor）与 pkg-node（仅发布编译好的 pkg/*.wasm）
    rm -rf "$TMPPKG/chain-chess-pwa-v${VERSION}/wasm" "$TMPPKG/chain-chess-pwa-v${VERSION}/pkg-node"
    (cd "$TMPPKG" && zip -qr "$OLDPWD/$PWA_ZIP" "chain-chess-pwa-v${VERSION}")
    rm -rf "$TMPPKG"
    PWA_BYTES=$(stat -c %s "$PWA_ZIP" 2>/dev/null || echo 0)
    echo "  ✅ PWA zip 打包完成 (${PWA_BYTES} bytes)"
  else
    echo "  ⚠️  未找到 pwa/ 目录，跳过"
  fi

  # ── 登记已编译产物 size 到 update.json（仅 --all）──
  if [ "$ALL" = true ]; then
    echo ""
    for PLAT in "android:apk" "windows:exe"; do
      KEY=${PLAT%%:*}; EXT=${PLAT##*:}
      FILE="release/${PRODUCT}-${VERSION}.${EXT}"
      if [ -f "$FILE" ]; then
        SZ=$(stat -c %s "$FILE")
        jq --argjson sz "$SZ" --arg url "https://gitee.com/ywnh1/chain-chess-release/releases/download/v${VERSION}/${PRODUCT}-${VERSION}.${EXT}" \
           --arg plat "$KEY" '.platforms[$plat] = {url: $url, size: $sz}' \
           update.json > tmp.json && mv tmp.json update.json
        echo "  📄 update.json: ${KEY}.size = ${SZ} (${FILE})"
      else
        echo "  ⚠️  未找到 ${FILE}，跳过（请先编译或运行 ./build.sh --apk / --exe）"
      fi
    done
  fi

  # ── 展示最终 update.json ──
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "  📄 最终 update.json"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  batcat update.json 2>/dev/null || cat update.json
  echo ""
  echo "  📌 上传到 Gitee Release 前请确认："
  echo "     1. update.json → 提交到 chain-chess-release 仓库 main 分支"
  echo "     2. release/ 下产物 → 上传到 Gitee Release 附件 (tag: v${VERSION})"
  echo ""
  exit 0
fi

# ════════════════════════════════════════════════════════
# 步骤 1: 构建 Windows exe（cargo-xwin 交叉编译）
# 说明：x86_64-pc-windows-msvc target 已通过 rustup 安装，
#       cargo-xwin 负责提供 MSVC CRT/SDK 并调用 clang-cl。
# ════════════════════════════════════════════════════════
if [ "$EXE" = true ]; then
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "  🪟 编译 Windows exe (${EXE_TARGET})"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""

  (
    cd "$RUST_DIR"
    cargo xwin build --release --target "$EXE_TARGET"
  )

  if [ ! -f "$EXE_SRC" ]; then
    echo "❌ 错误: exe 编译产物不存在: $EXE_SRC"
    exit 1
  fi

  mkdir -p release
  cp "$EXE_SRC" "$EXE_OUTPUT"
  EXE_BYTES=$(stat -c %s "$EXE_OUTPUT")
  EXE_SIZE=$(ls -lh "$EXE_OUTPUT" | awk '{print $5}')

  echo ""
  echo "  ✅ Windows exe 构建完成: ${EXE_OUTPUT} (${EXE_SIZE})"
  echo ""

  # 更新 update.json 的 windows 平台条目
  if [ -f "update.json" ]; then
    VERSION=$(jq -r '.version' update.json)
    EXE_URL="https://gitee.com/ywnh1/chain-chess-release/releases/download/v${VERSION}/${PRODUCT}-${VERSION}.exe"
    jq --arg url "$EXE_URL" --argjson sz "$EXE_BYTES" \
       '.platforms.windows = {url: $url, size: $sz}' \
       update.json > tmp.json && mv tmp.json update.json
    echo "  📄 update.json: windows 平台已更新（url + size=${EXE_BYTES}）"
    echo ""
  fi
fi

# ════════════════════════════════════════════════════════
# 步骤 2: 构建 Android APK
# ════════════════════════════════════════════════════════
if [ "$APK" = true ]; then

# ── 构建模式与产物路径（仅 release；debug 已移除）───────
UNSIGNED_APK="tauri/src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk"
if [ "$NATIVE" = true ]; then
  OUTPUT="release/${PRODUCT}-native-${VERSION}.apk"
else
  OUTPUT="release/${PRODUCT}-${VERSION}.apk"
fi

# ── Release 激进优化（修改 Cargo.toml，构建后自动恢复）─────
# 仅 release 构建（debug 已移除），固定启用 fat LTO：
# 基础 [profile.release] 已含 opt-level=3 / strip / codegen-units=1 / panic=abort，
# fat LTO 全程序链接优化，跨 crate 内联，性能与体积更优，但链接耗时显著增加。
if grep -q '^\[profile\.release\]' "$CARGO_TOML"; then
  cp "$CARGO_TOML" "${CARGO_TOML}.bak"
  if grep -q '^lto' "$CARGO_TOML"; then
    sed -i 's/^lto = .*/lto = "fat"/' "$CARGO_TOML"
  else
    sed -i '/^\[profile\.release\]/a lto = "fat"' "$CARGO_TOML"
  fi
  echo "  ⚙️  Cargo.toml: lto → fat（全程序链接优化）"
else
  echo "  ⚠️  Cargo.toml 缺少 [profile.release]，跳过激进优化"
fi
echo ""

# ── 原生优化模式（--native，仅 release 生效）─────────────
if [ "$NATIVE" = true ]; then
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "  🚀 启用 native 极致优化 (target-cpu=native)"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""

  # 创建 .cargo 目录（如果不存在）
  mkdir -p "$(dirname "$CARGO_CONFIG")"

  # 写入优化配置
  # 注：Tauri 的 android build 会自动设置 NDK 链接器路径，
  # 我们只需注入 rustflags，让 Tauri 管理整个编译流程
  cat > "$CARGO_CONFIG" << 'CONFIG_EOF'
# Auto-generated by build.sh --native
# target-cpu=native: 使用本机 CPU 全部指令集优化
[target.aarch64-linux-android]
rustflags = ["-C", "target-cpu=native"]
CONFIG_EOF

  echo "  已创建: $CARGO_CONFIG"
  echo ""

  echo "  Tauri 将使用 NDK 链接器自动编译 Rust 库（带 native 优化）"
  echo ""
fi

# ── 自动清理 ──────────────────────────────────────────────
# 统一退出清理：恢复被修改的配置文件
APP_BUILD_GRADLE="tauri/src-tauri/gen/android/app/build.gradle.kts"
cleanup_all() {
  # 恢复 Gradle 编译配置（JDK 21 降级）
  if [ -f "${APP_BUILD_GRADLE}.bak" ]; then
    cp "${APP_BUILD_GRADLE}.bak" "$APP_BUILD_GRADLE"
    rm -f "${APP_BUILD_GRADLE}.bak"
    echo ""
    echo "  🧹 已恢复: app/build.gradle.kts"
  fi
  # 恢复 Cargo.toml（--release 模式修改）
  if [ -f "${CARGO_TOML}.bak" ]; then
    cp "${CARGO_TOML}.bak" "$CARGO_TOML"
    rm -f "${CARGO_TOML}.bak"
    echo ""
    echo "  🧹 已恢复: Cargo.toml"
  fi
  # 清理 update.json 修改产生的临时文件
  rm -f tmp.json
  # 清理 native 优化配置（--native 模式生成）
  if [ -f "$CARGO_CONFIG" ]; then
    rm -f "$CARGO_CONFIG"
    echo ""
    echo "  🧹 已清理: .cargo/config.toml"
  fi
}
trap cleanup_all EXIT

# ── Android SDK 版本检测与自动降级 ──────────────────────
# build-tools 34 无法加载 android-35+ 的平台资源
# 检测 compileSdk 是否超出 build-tools 兼容范围，是则自动降级到 34
COMPILE_SDK=$(grep 'compileSdk =' "$APP_BUILD_GRADLE" | sed 's/.*compileSdk = \([0-9]*\).*/\1/')
TARGET_SDK=$(grep 'targetSdk =' "$APP_BUILD_GRADLE" | sed 's/.*targetSdk = \([0-9]*\).*/\1/')
SDK_DOWNGRADED=false

if [ -n "$COMPILE_SDK" ]; then
  # 用 aapt2 验证平台 jar 是否可加载
  PLATFORM_JAR="${ANDROID_HOME}/platforms/android-${COMPILE_SDK}/android.jar"
  AAPT2=$(ls ${ANDROID_HOME}/build-tools/*/aapt2 2>/dev/null | head -1)
  PLATFORM_BAD=false

  if [ ! -f "$PLATFORM_JAR" ] || [ ! -s "$PLATFORM_JAR" ]; then
    PLATFORM_BAD=true
  elif [ -n "$AAPT2" ]; then
    # aapt2 dump resources 验证 jar 可被正确解析
    if ! "$AAPT2" dump resources "$PLATFORM_JAR" > /dev/null 2>&1; then
      PLATFORM_BAD=true
    fi
  fi

  if [ "$PLATFORM_BAD" = true ]; then
    echo ""
    echo "  ⚠️  Android SDK 平台 android-${COMPILE_SDK} 与当前 build-tools 不兼容"
    echo "  🛠️  自动降级 compileSdk/targetSdk: ${COMPILE_SDK} → 34"
    echo ""

    # 备份原始文件
    cp "$APP_BUILD_GRADLE" "${APP_BUILD_GRADLE}.bak"

    # 降级 compileSdk/targetSdk + 依赖版本（高版本依赖要求 compileSdk >= 35）
    awk '{
  gsub(/compileSdk = [0-9]+/, "compileSdk = 34")
  gsub(/targetSdk = [0-9]+/, "targetSdk = 34")
  gsub(/androidx\.activity:activity-ktx:[0-9]+\.[0-9]+\.[0-9]+/, "androidx.activity:activity-ktx:1.9.3")
  gsub(/androidx\.webkit:webkit:[0-9]+\.[0-9]+\.[0-9]+/, "androidx.webkit:webkit:1.12.1")
  gsub(/androidx\.lifecycle:lifecycle-process:[0-9]+\.[0-9]+\.[0-9]+/, "androidx.lifecycle:lifecycle-process:2.8.7")
  print
}' "${APP_BUILD_GRADLE}.bak" > "$APP_BUILD_GRADLE"

    SDK_DOWNGRADED=true

    echo "  ✅ 已降级到 34，构建完成后自动恢复"
    echo ""
  fi
fi

# ── 检查 Android 项目结构 ──────────────────────────────
ANDROID_MAIN_ACTIVITY="tauri/src-tauri/gen/android/app/src/main/java/com/ywnh1/chainchess/MainActivity.kt"
if [ ! -f "$ANDROID_MAIN_ACTIVITY" ]; then
  echo "  ⚠️  Android 项目结构不匹配，正在重新初始化..."
  echo "  (若因 tauri.conf.json identifier 变更导致)"
  echo ""
  rm -rf tauri/src-tauri/gen/android
  (cd tauri && npx tauri android init)
  echo "  ✅ Android 项目已重新初始化"
  echo ""
fi

# 确保 tauri.properties 存在
TAURI_PROPERTIES="tauri/src-tauri/gen/android/app/tauri.properties"
if [ -f "$TAURI_PROPERTIES" ]; then
  # 更新版本号
  sed -i "s/^tauri.android.versionName=.*/tauri.android.versionName=${VERSION%-beta}/" "$TAURI_PROPERTIES"
  # versionCode: 3.1.0 → 3001000 (major*1e6 + minor*1e3 + patch)
  VC_MAJOR=$(echo "$VERSION" | cut -d. -f1)
  VC_MINOR=$(echo "$VERSION" | cut -d. -f2)
  VC_PATCH=$(echo "$VERSION" | cut -d. -f3 | cut -d- -f1)
  VC=$(( VC_MAJOR * 1000000 + VC_MINOR * 1000 + VC_PATCH ))
  sed -i "s/^tauri.android.versionCode=.*/tauri.android.versionCode=${VC}/" "$TAURI_PROPERTIES"
fi

# ── 步骤 2: 复制前端资源到 assets ──────────────────────
# Tauri CLI 在 SKIP_RUST_BUILD 下可能跳过前端资源复制
ASSETS_DIR="tauri/src-tauri/gen/android/app/src/main/assets"
FRONTEND_DIR="tauri/public"
mkdir -p "$ASSETS_DIR"
cp -r "$FRONTEND_DIR"/. "$ASSETS_DIR"/ 2>/dev/null
cp "tauri/src-tauri/tauri.conf.json" "$ASSETS_DIR"/ 2>/dev/null
# 给 assets 里的资源引用打上版本戳，避免 WebView 缓存旧版 CSS/JS（升级后仍加载旧样式）
sed -i "s/\(href=\"style\.css\)[^\"]*/\1?v=${VERSION}/; s/\(src=\"app\.js\)[^\"]*/\1?v=${VERSION}/" "$ASSETS_DIR/index.html" 2>/dev/null || true
echo "  📦 前端资源已复制到 assets ($(ls -1 "$ASSETS_DIR" | wc -l) 个文件)"

# ── 步骤 3: 编译 APK ─────────────────────────────────────
# TAURI_ANDROID_SKIP_RUST_BUILD 让 Gradle 跳过 Rust 重编译
# Tauri CLI 已负责 Rust 编译，Gradle 无需重复
export TAURI_ANDROID_SKIP_RUST_BUILD=1

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  🔨 编译 arm64 release APK"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  产物: ${OUTPUT}"
echo ""
(
  cd tauri
  npx tauri android build --target aarch64 --apk
)

# ── 步骤 4: 签名 ──────────────────────────────────────────
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

# ── 步骤 5: 验证签名 ──────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  ✅ 验证签名"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""

apksigner verify --verbose "$OUTPUT" | sed 's/^/  /'

# ── 完成 ──────────────────────────────────────────────────
ELAPSED=$(( $(date +%s) - START_EPOCH ))
FILESIZE=$(ls -lh "$OUTPUT" | awk '{print $5}')
FILESIZE_BYTES=$(stat -c %s "$OUTPUT")

echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  🎉 APK 构建完成！"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  文件: ${OUTPUT}"
echo "  大小: ${FILESIZE}"
echo "  耗时: ${ELAPSED}s"

# ── 步骤 6: 更新 update.json（APK 大小）──────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  📄 验证 update.json"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ -f "update.json" ]; then
  # 用 APK 真实字节数更新 .android.size（--argjson 要求合法 JSON 数值）
  jq --argjson sz "$FILESIZE_BYTES" '.platforms.android.size = $sz' update.json > tmp.json && mv tmp.json update.json

  VERSION=$(jq -r '.version' update.json)
  
  echo "  已验证: update.json"
  echo "  版本: ${VERSION}"
  echo "  尺寸: ${FILESIZE}"
  echo ""
  batcat update.json 2>/dev/null || cat update.json
  echo ""
  echo "  📌 上传到 Gitee Release 前请确认："
  echo "     1. update.json → 提交到 chain-chess-release 仓库 main 分支"
  echo "     2. ${OUTPUT##release/} → 上传到 Gitee Release 附件"
  if [ "$EXE" = true ]; then
    echo "     3. ${EXE_OUTPUT##release/} → 上传到 Gitee Release 附件（Windows）"
  fi
  echo ""
else
  echo "update.json不存在"
fi

fi # end APK

# ── 收尾：展示全部产物 ────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  📦 release/ 目录产物"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
ls -lh release/ 2>/dev/null | grep -E "chainchess|\.exe" || true
echo ""

# 尝试复制 APK 到 Android 设备（仅当路径存在时）
if [ "$APK" = true ] && [ -d "/storage/emulated/0/用户" ]; then
  cp "$OUTPUT" /storage/emulated/0/用户/
  echo "  📱 已复制到 /storage/emulated/0/用户/"
  better-rm release 2>/dev/null || true
fi
if [ -f "update.json" ] && [ -d "../chain-chess-release" ]; then
  cp update.json ../chain-chess-release
  echo "  🌐 已复制到 ../chain-chess-release"
  better-rm -s update.json 2>/dev/null || true
fi
echo ""
