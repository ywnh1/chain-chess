#!/bin/sh
# build.sh - 编译并签名连锁棋（APK / Windows exe / Linux deb+AppImage / PWA zip）
# 用法: ./build.sh [选项] <keystore_password>    （./build.sh -h 查看完整帮助）
# 仅支持 release 构建（debug 模式已移除）
#       ./build.sh -a chainchess      # 编译安卓 APK 到 release/，更新 update.json 的 android size
#       ./build.sh -e                 # 编译 Windows exe 到 release/，更新 update.json 的 windows size
#       ./build.sh -l                 # 编译 Linux 版（deb + AppImage）到 release/，更新 linux size
#       ./build.sh -z                 # 重新编译 WASM + 打包 PWA zip 到 release/（update.json 无 pwa 条目，不更新 size）
#       ./build.sh -A chainchess      # 编译 apk + exe + linux + zip 全部到 release/，只更新本次编译的 size
#       ./build.sh -n chainchess      # 编译安卓 Native APK 到 release/，不碰 update.json
#       ./build.sh -r chainchess      # 编译除 Native 外所有（apk+exe+linux+zip），全部更新 size，
#                                     #   并发布：release/ → /storage/emulated/0/用户/，
#                                     #   update.json + PWA 必要内容 → ../chain-chess-release
#       ./build.sh -r -c chainchess   # 同上，并自动 commit 两个仓库（push 仍手动）
#       ./build.sh -V                 # 打印当前项目版本号（来源 tauri.conf.json）
#
# 版本号：tauri/src-tauri/tauri.conf.json 是唯一来源。每次构建自动同步到
#         Cargo.toml / package.json / 两个 index.html / sw.js / download.html /
#         README badge / update.json，发版只需改那一个文件。

set -e

mkdir -p release 

# ── 配置 ──────────────────────────────────────────────────
KEYSTORE="release.keystore"
PRODUCT="chainchess"
VERSION="3.3.7"          # 兜底值；实际取 tauri.conf.json，见下方「版本号单一来源」

CARGO_CONFIG="tauri/src-tauri/.cargo/config.toml"
CARGO_TOML="tauri/src-tauri/Cargo.toml"
TAURI_CONF="tauri/src-tauri/tauri.conf.json"

# ── 版本号单一来源 ────────────────────────────────────────
# 发版只需要改 tauri.conf.json 的 version 一处：
#   1. 这里把它读出来作为 $VERSION
#   2. sync_version() 把其余落点对齐过去
#   3. verify_version_sync() 逐一复核，没对齐就报错退出，不静默通过
if [ -f "$TAURI_CONF" ] && command -v jq >/dev/null 2>&1; then
  _CONF_VER=$(jq -r '.version // empty' "$TAURI_CONF" 2>/dev/null || true)
  if [ -n "$_CONF_VER" ]; then
    VERSION="$_CONF_VER"
  fi
fi

GRADLE_PROPS="tauri/src-tauri/gen/android/gradle.properties"
RUST_DIR="tauri/src-tauri"

# Windows exe 交叉编译目标（cargo-xwin）
EXE_TARGET="x86_64-pc-windows-msvc"
EXE_SRC="${RUST_DIR}/target/${EXE_TARGET}/release/chain-chess.exe"
EXE_OUTPUT="release/${PRODUCT}-${VERSION}.exe"

# ── 版本同步工具 ──────────────────────────────────────────
SYNCED_FILES=""

# sync_one <文件> <sed 表达式...>
# 只在内容确有变化时把文件计入 SYNCED_FILES，--commit 据此精确 git add
sync_one() {
  _f="$1"; shift
  [ -f "$_f" ] || return 0
  _before=$(cat "$_f")
  sed -i "$@" "$_f" 2>/dev/null || { echo "  ⚠️  版本同步写入失败: $_f"; return 0; }
  _after=$(cat "$_f")
  if [ "$_before" != "$_after" ]; then
    SYNCED_FILES="$SYNCED_FILES $_f"
  fi
  return 0
}

# 把所有版本号落点对齐到 $1（幂等：已经是目标值就不动文件）
sync_version() {
  _v="$1"
  [ -n "$_v" ] || return 0

  sync_one "tauri/src-tauri/Cargo.toml" \
    "0,/^version = \"[0-9][^\"]*\"/s//version = \"$_v\"/"
  sync_one "tauri/package.json" \
    "0,/\"version\": \"[0-9][^\"]*\"/s//\"version\": \"$_v\"/"
  sync_one "README.md" \
    "s|badge/version-[0-9][0-9.]*-orange|badge/version-$_v-orange|;s|alt=\"v[0-9][0-9.]*\"|alt=\"v$_v\"|"

  # 前端资源版本戳（style.css / app.js / engine.js）+ 关于页显示的版本
  sync_one "tauri/public/index.html" \
    "s|\(href=\"style\.css\)[^\"]*|\1?v=$_v|;s|\(src=\"app\.js\)[^\"]*|\1?v=$_v|;s|\(版本</span><span class=\"info-val\">\)v[0-9][^<]*|\1v$_v|"
  sync_one "docs/index.html" \
    "s|\(href=\"style\.css\)[^\"]*|\1?v=$_v|;s|\(src=\"app\.js\)[^\"]*|\1?v=$_v|;s|\(src=\"engine\.js\)[^\"]*|\1?v=$_v|;s|\(版本</span><span class=\"info-val\">\)v[0-9][^<]*|\1v$_v|"

  # Service Worker 缓存名（变号即触发 SW 更新）
  sync_one "docs/sw.js" \
    "s|^const CACHE_NAME = 'chain-chess-v[0-9][^']*'|const CACHE_NAME = 'chain-chess-v$_v'|"

  # 下载中心：顶部版本、三条下载直链、卡片副标题、VERSION 常量
  # （更新日志数组是历史条目，刻意不动；新版本条目需手动新增）
  sync_one "download.html" \
    "s|\(<span class=\"version\">\)v[0-9][0-9.]*|\1v$_v|;s|\(var VERSION = '\)[0-9][0-9.]*'|\1$_v'|;s|releases/download/v[0-9][0-9.]*/|releases/download/v$_v/|g;s|chainchess-[0-9][0-9.]*\.apk|chainchess-$_v.apk|g;s|chainchess-[0-9][0-9.]*\.exe|chainchess-$_v.exe|g;s|chainchess-[0-9][0-9.]*\.deb|chainchess-$_v.deb|g;s|chainchess-[0-9][0-9.]*\.AppImage|chainchess-$_v.AppImage|g;s|chain-chess-pwa-v[0-9][0-9.]*\.zip|chain-chess-pwa-v$_v.zip|g;s|\(<div class=\"meta\">\)v[0-9][0-9.]*|\1v$_v|g;s|\(<span>\)v[0-9][0-9.]*|\1v$_v|"

  # 发布版本 + 更新说明（中英）
  # notes / notes_zh 都从 download.html 的 CHANGELOG 首条提取：
  # en 与 notes 本应逐字相同，zh 供客户端更新弹窗显示中文。发版时只写 download.html 一处。
  if [ -f "update.json" ] && command -v jq >/dev/null 2>&1; then
    if [ "$(jq -r '.version // empty' update.json 2>/dev/null || true)" != "$_v" ]; then
      jq --arg v "$_v" '.version = $v' update.json > tmp.json && mv tmp.json update.json
      SYNCED_FILES="$SYNCED_FILES update.json"
    fi
    if [ -f "download.html" ] && command -v python3 >/dev/null 2>&1; then
      _UPDSYNC=$(python3 - <<'PYEOF'
import re, json, sys
h = open('download.html', encoding='utf-8').read()
m = re.search(r"\{v:'v[^']*',\s*\n\s*zh:'([^']*)',\s*\n\s*en:'([^']*)'\}", h)
if not m:
    print('WARN'); sys.exit(0)
zh, en = m.group(1).strip(), m.group(2).strip()
d = json.load(open('update.json', encoding='utf-8'))
if d.get('notes_zh') == zh and d.get('notes') == en:
    print('SAME'); sys.exit(0)
d['notes_zh'] = zh
d['notes'] = en
with open('update.json', 'w', encoding='utf-8') as f:
    json.dump(d, f, ensure_ascii=False, indent=2)
    f.write('\n')
print('CHANGED')
PYEOF
)
      case "$_UPDSYNC" in
        CHANGED)
          echo "  📝 update.json: notes / notes_zh 已从 download.html 同步"
          SYNCED_FILES="$SYNCED_FILES update.json"
          ;;
        WARN)
          echo "  ⚠️  没能从 download.html 提取中英说明，notes 保持原样"
          ;;
      esac
    fi
  fi
  return 0
}

# 复核每个落点确实等于 $1；有偏差就列出来并返回非零
verify_version_sync() {
  _v="$1"
  _bad=""
  vcheck() {
    [ -f "$1" ] || return 0
    grep -qE "$2" "$1" 2>/dev/null || _bad="$_bad
     - $3  ($1)"
    return 0
  }
  vcheck "tauri/src-tauri/Cargo.toml" "^version = \"$_v\"$"              "Cargo.toml version"
  vcheck "tauri/package.json"          "\"version\": \"$_v\""            "package.json version"
  vcheck "README.md"                   "badge/version-$_v-orange"        "README badge"
  vcheck "tauri/public/index.html"     "style\.css\?v=$_v"               "public/index.html 资源戳"
  vcheck "docs/index.html"             "style\.css\?v=$_v"               "docs/index.html 资源戳"
  vcheck "docs/sw.js"                  "CACHE_NAME = 'chain-chess-v$_v'" "sw.js 缓存名"
  vcheck "download.html"               "<span class=\"version\">v$_v"    "download.html 顶部版本"
  vcheck "update.json"                 "\"version\": \"$_v\""            "update.json version"
  if [ -n "$_bad" ]; then
    echo ""
    echo "  ❌ 版本号未对齐（目标 v$_v）：$_bad"
    echo "     检查上面这些文件，或确认 tauri.conf.json 是唯一来源。"
    return 1
  fi
  return 0
}

# 自动提交（只 commit 不 push）：主仓库只提版本号文件，发布仓库整仓提交
do_commit() {
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "  📝 自动提交（只 commit，不 push）"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

  # 1) 发布仓库：内容全是构建产物，整仓提交
  if [ -d "../chain-chess-release/.git" ]; then
    ( cd ../chain-chess-release && git add -A )
    if ( cd ../chain-chess-release && ! git diff --cached --quiet ); then
      if ( cd ../chain-chess-release && git commit -m "release: v${VERSION}" >/dev/null 2>&1 ); then
        echo "  ✅ ../chain-chess-release 已提交"
      else
        echo "  ⚠️  ../chain-chess-release 提交失败"
      fi
    else
      echo "  ℹ️  ../chain-chess-release 无改动"
    fi
  else
    echo "  ⚠️  未找到 ../chain-chess-release/.git，跳过"
  fi

  # 2) 主仓库：只提交本次实际同步过的版本号文件，不裹挟其他改动
  if [ -n "$SYNCED_FILES" ] && [ -d ".git" ]; then
    # shellcheck disable=SC2086  # 这里就是要按空格分词传多个路径
    git add -- $SYNCED_FILES
    if ! git diff --cached --quiet; then
      if git commit -m "chore: 版本号对齐 v${VERSION}" >/dev/null 2>&1; then
        echo "  ✅ 主仓库已提交:$SYNCED_FILES"
      else
        echo "  ⚠️  主仓库提交失败"
      fi
    else
      echo "  ℹ️  主仓库无版本号改动"
    fi
  fi

  echo ""
  echo "  📌 push 仍需手动（脚本不碰远端凭证）:"
  echo "     git push"
  echo "     (cd ../chain-chess-release && git push)"
  echo ""
  return 0
}

# ── 环境检测 ──────────────────────────────────────────
if [ -z "${ANDROID_HOME}" ] && [ -d "$HOME/Android/Sdk" ]; then
  export ANDROID_HOME="$HOME/Android/Sdk"
  echo "  📋 ANDROID_HOME 自动设为: $ANDROID_HOME"
fi

# ── 参数解析 ──────────────────────────────────────────────
# --apk    : 编译 Android APK（需 keystore 密码）→ 更新 android size
# --exe    : 编译 Windows exe（cargo-xwin）→ 更新 windows size
# --zip    : 重新编译 WASM + 打包 PWA zip → 无 size 条目
# --all    : apk + exe + zip 全部编译到 release/，只更新本次新编译内容的 size
# --native : 编译 Android Native APK，不碰 update.json
# --release: 编译除 Native 外所有（apk+exe+zip），全部更新 size，并发布
#            （release/ → /storage/emulated/0/用户/，update.json + PWA → ../chain-chess-release）
APK=false
EXE=false
LINUX=false
ZIP=false
ZIP_ONLY=false
ALL=false
NATIVE=false
PUBLISH=false
HELP=false
SHOW_VERSION=false
COMMIT=false
SYNC_ONLY=false
PASSWORD=""

for arg in "$@"; do
  case "$arg" in
    --apk|-a)     APK=true ;;
    --exe|-e)     EXE=true ;;
    --linux|-l)   LINUX=true ;;
    --zip|-z)     ZIP=true; ZIP_ONLY=true ;;
    --all|-A)     ALL=true ;;
    --native|-n)  NATIVE=true ;;
    --release|-r) PUBLISH=true ;;
    --commit|-c)  COMMIT=true ;;
    --sync-only|-s) SYNC_ONLY=true ;;
    --help|-h)    HELP=true ;;
    --version|-V) SHOW_VERSION=true ;;
    -*)
      echo "❌ 未知参数: $arg"
      echo "用法: $0 [选项] <keystore_password>   （-h 查看帮助）"
      exit 1
      ;;
    *) PASSWORD="$arg"
  esac
done

# ── 帮助 / 版本：打印后立即退出（优先级最高，不触发默认构建）──
if [ "$HELP" = true ]; then
  cat <<HELP_EOF
用法: $0 [选项] <keystore_password>

编译并签名连锁棋（APK / Windows exe / Linux deb+AppImage / PWA zip）到 release/，
并按模式更新 update.json 的安装包大小条目。

版本号以 tauri/src-tauri/tauri.conf.json 为唯一来源，构建时自动同步到
Cargo.toml / package.json / 两个 index.html / sw.js / download.html /
README badge / update.json —— 发版只需改 tauri.conf.json 一处。

选项:
  -a, --apk <密码>     编译安卓 APK，更新 update.json 的 android size
  -e, --exe            编译 Windows exe（cargo-xwin），更新 windows size
  -l, --linux          编译 Linux 版（deb + AppImage，本机原生编译），更新 linux size
  -z, --zip            重新编译 WASM 并打包 PWA zip（无平台条目，不更新 size）
  -A, --all <密码>     编译 apk + exe + linux + zip 全部，只更新本次编译的 size
  -n, --native <密码>  编译安卓 Native APK，不碰 update.json
  -r, --release <密码> 编译除 Native 外所有，全部更新 size，并发布：
                       release/ → /storage/emulated/0/用户/
                       update.json + PWA 必要内容 → ../chain-chess-release
  -c, --commit         构建后自动 git commit（主仓库的版本号文件 + ../chain-chess-release
                       的发布产物）。只 commit 不 push，push 仍需手动执行
  -s, --sync-only      只把版本号同步到各落点并复核，不构建（改完 tauri.conf.json 后可用）
  -h, --help           显示本帮助
  -V, --version        打印当前项目版本号（唯一来源 tauri.conf.json）

示例:
  $0 -a chainchess          # 仅编译 APK
  $0 -e                     # 仅编译 Windows exe
  $0 -l                     # 仅编译 Linux 版（deb + AppImage）
  $0 -z                     # 仅打包 PWA zip
  $0 -A chainchess          # 编译全部（apk+exe+zip），不发布
  $0 -n chainchess          # 编译 Native APK，不碰 update.json
  $0 -r chainchess          # 编译全部 + 更新全部 size + 发布
  $0 -r -c chainchess       # 同上，并自动 commit 两个仓库（需手动 push）
  $0 -s                     # 只同步版本号（改完 tauri.conf.json 后执行）
  $0 -V                     # 打印项目版本号
HELP_EOF
  exit 0
fi

if [ "$SHOW_VERSION" = true ]; then
  # 版本号唯一来源：tauri.conf.json（脚本顶部已读出），读不到时用脚本兜底值
  echo "$VERSION"
  exit 0
fi

# ── 版本号对齐（发版只改 tauri.conf.json 一处）──────────
# 在任何构建动作之前执行；--sync-only 到此为止，不进构建流程
sync_version "$VERSION"
if ! verify_version_sync "$VERSION"; then
  exit 1
fi
if [ -n "$SYNCED_FILES" ]; then
  echo ""
  echo "  🔖 版本号 v${VERSION} 已同步到:$SYNCED_FILES"
fi
if [ "$SYNC_ONLY" = true ]; then
  echo ""
  echo "  ✅ 版本号同步完成（--sync-only，未构建）"
  if [ "$COMMIT" = true ]; then
    do_commit
  fi
  exit 0
fi

# 模式语义展开
# --all: apk + exe + zip；--release: apk + exe + zip + 发布动作
if [ "$ALL" = true ]; then
  APK=true; EXE=true; ZIP=true; LINUX=true
fi
if [ "$PUBLISH" = true ]; then
  APK=true; EXE=true; ZIP=true; LINUX=true
fi
# --native: 编译 native APK
if [ "$NATIVE" = true ]; then
  APK=true
fi
# 未指定平台时默认 APK + exe 都构建
if [ "$APK" = false ] && [ "$EXE" = false ] && [ "$ZIP" = false ] && [ "$LINUX" = false ]; then
  APK=true
  EXE=true
fi
# --native 与 --all / --release 互斥（--release 定义即为"除 Native 外所有"）
if [ "$NATIVE" = true ] && { [ "$ALL" = true ] || [ "$PUBLISH" = true ]; }; then
  echo "❌ 参数冲突: --native 与 --all / --release 互斥"
  echo "   --native 单独编译 Native APK；--all / --release 编译普通 apk + exe + zip"
  exit 1
fi

# APK 构建需要 keystore 密码（普通 APK / native APK 都需要）
if [ "$APK" = true ]; then
  if [ -z "$PASSWORD" ]; then
    echo "用法: $0 --apk <keystore_password>"
    echo "       $0 --all <keystore_password>"
    echo "       $0 --native <keystore_password>"
    echo "       $0 --release <keystore_password>"
    echo "       $0 --exe"
    echo "       $0 --zip"
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
if [ "$PUBLISH" = true ]; then
  echo "  📦 构建目标: APK + Windows exe + Linux deb/AppImage + PWA zip（发布模式）"
elif [ "$ALL" = true ]; then
  echo "  📦 构建目标: APK + Windows exe + Linux deb/AppImage + PWA zip（全部，不发布）"
elif [ "$APK" = true ] && [ "$EXE" = true ]; then
  echo "  📦 构建目标: APK + Windows exe"
elif [ "$LINUX" = true ] && [ "$APK" = false ] && [ "$EXE" = false ] && [ "$ZIP" = false ]; then
  echo "  📦 构建目标: Linux deb/AppImage"
elif [ "$APK" = true ] && [ "$NATIVE" = true ]; then
  echo "  📦 构建目标: Native APK（不更新 update.json）"
elif [ "$APK" = true ]; then
  echo "  📦 构建目标: APK"
elif [ "$EXE" = true ]; then
  echo "  📦 构建目标: Windows exe"
else
  echo "  📦 构建目标: PWA zip"
fi
if [ "$NATIVE" = true ]; then
  echo "  🚀 APK 模式: release + native（target-cpu=native 极致优化，不碰 update.json）"
fi
if [ "$PUBLISH" = true ]; then
  echo "  🚀 发布动作: release/ → /storage/emulated/0/用户/，update.json + PWA → ../chain-chess-release"
fi
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo ""
# ════════════════════════════════════════════════════════
# 步骤 Z: 打包 PWA zip（--zip / --all / --release）
# -r / -z 时先重新编译 wasm（docs/wasm → docs/pkg），再打包；-A 用现有 pkg/ 产物；update.json 无 pwa 平台条目，不更新 size
# ════════════════════════════════════════════════════════
if [ "$ZIP" = true ]; then
  PWA_ZIP="release/chain-chess-pwa-v${VERSION}.zip"
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "  📦 打包 PWA: ${PWA_ZIP}"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""
  if [ -d "docs" ]; then
    # 重新编译 WASM 引擎（--release / --zip 时）：docs/wasm crate → docs/pkg
    if [ "$PUBLISH" = true ] || [ "$ZIP_ONLY" = true ]; then
      echo "  🦀 重新编译 WASM 引擎（docs/wasm → docs/pkg）..."
      if command -v wasm-pack >/dev/null 2>&1; then
        ( cd docs/wasm && wasm-pack build --target web --out-dir ../pkg --release )
        echo "  ✅ WASM 引擎编译完成"
      else
        echo "  ⚠️  未找到 wasm-pack，跳过 WASM 重编译（使用现有 pkg/ 产物）"
      fi
      echo ""
    fi
    TMPPKG=$(mktemp -d)
    cp -r docs "$TMPPKG/chain-chess-pwa-v${VERSION}"
    # 排除 wasm 源码目录（含 target/vendor）与 pkg-node（仅发布编译好的 pkg/*.wasm）
    rm -rf "$TMPPKG/chain-chess-pwa-v${VERSION}/wasm" "$TMPPKG/chain-chess-pwa-v${VERSION}/pkg-node"
    (cd "$TMPPKG" && zip -qr "$OLDPWD/$PWA_ZIP" "chain-chess-pwa-v${VERSION}")
    rm -rf "$TMPPKG"
    PWA_BYTES=$(stat -c %s "$PWA_ZIP" 2>/dev/null || echo 0)
    echo "  ✅ PWA zip 打包完成 (${PWA_BYTES} bytes)"
    echo ""
  else
    echo "  ⚠️  未找到 docs/ 目录，跳过 PWA 打包"
    echo ""
  fi
fi

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

  # 更新 update.json 的 windows 平台条目（跳转目标统一为下载中心）
  if [ -f "update.json" ]; then
    EXE_URL="https://ywnh1.free.leoi.org"
    jq --arg url "$EXE_URL" --argjson sz "$EXE_BYTES" \
       '.platforms.windows = {url: $url, size: $sz}' \
       update.json > tmp.json && mv tmp.json update.json
    echo "  📄 update.json: windows 平台已更新（url + size=${EXE_BYTES}）"
    echo ""
  fi
fi
# ════════════════════════════════════════════════════════
# 步骤 1.5: 构建 Linux 版（deb / AppImage，本机原生编译）
# deb 用 Tauri 自带 bundler（依赖 dpkg-deb + fakeroot）；
# AppImage 需要 appimagetool，失败不阻断（deb 已够用）。
# ════════════════════════════════════════════════════════
if [ "$LINUX" = true ]; then
  echo ""
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "  🐧 编译 Linux 版（deb + AppImage）"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""

  (cd tauri && npx tauri build --bundles deb)

  # 只认当前版本的产物：同一目录会积累历史 deb（连锁棋_3.3.8_*.deb 等），
  # 按名字排序取第一个会捞到旧版本。
  DEB_SRC=$(ls -1t "$RUST_DIR"/target/release/bundle/deb/*_"${VERSION}"_*.deb 2>/dev/null | head -1)
  if [ -z "$DEB_SRC" ]; then
    echo "❌ 错误: 未找到 v${VERSION} 的 deb 产物（$RUST_DIR/target/release/bundle/deb/）"
    echo "   目录现有："
    ls -1 "$RUST_DIR"/target/release/bundle/deb/ 2>/dev/null | sed 's/^/     /' || true
    exit 1
  fi
  mkdir -p release
  DEB_OUTPUT="release/${PRODUCT}-${VERSION}.deb"

  # Tauri 挑主二进制时会挑到 src/bin/ 下的 CLI 工具（battle 等），deb 里装的就是
  # 那个 CLI 而不是 GUI；它还会顺手把产物改成 mainBinaryName，所以光看文件名看不
  # 出来。按 GUI 独有的 webkit 特征串找出 cargo 编出的真主程序，换进 deb。
  _gui_bin=$(ls -1t "$RUST_DIR"/target/release/deps/chain_chess-* 2>/dev/null | grep -v '\.d$' | head -1)
  if [ -z "$_gui_bin" ] || ! strings -a "$_gui_bin" 2>/dev/null | grep -qi 'webkit'; then
    echo "❌ 错误: 找不到 Tauri GUI 主程序产物（$RUST_DIR/target/release/deps/chain_chess-*）"
    exit 1
  fi

  # Tauri 拿 productName（这里含中文）当 Package 字段，dpkg 要求包名以字母或数字
  # 开头，原样打包会直接安装失败。解包重写 control 再压回去。
  _deb_tmp=$(mktemp -d)
  dpkg-deb -R "$DEB_SRC" "$_deb_tmp"
  sed -i 's/^Package: .*/Package: chain-chess/' "$_deb_tmp/DEBIAN/control"
  if ! strings -a "$_deb_tmp/usr/bin/chain-chess" 2>/dev/null | grep -qi 'webkit'; then
    echo "  ⚠️  deb 里的主程序不是 GUI（Tauri 挑错了 bin），已替换为 GUI 产物"
    cp "$_gui_bin" "$_deb_tmp/usr/bin/chain-chess"
  fi
  dpkg-deb -b "$_deb_tmp" "$DEB_OUTPUT" >/dev/null
  rm -rf "$_deb_tmp"

  DEB_BYTES=$(stat -c %s "$DEB_OUTPUT")
  echo "  ✅ Linux deb 构建完成: ${DEB_OUTPUT} ($(ls -lh "$DEB_OUTPUT" | awk '{print $5}'))"
  echo ""

  # AppImage：Tauri 自带 bundler 需联网下载 linuxdeploy，这里改用手工打包。
  # 从刚生成的 deb 解出文件树，补上 AppImage 要求的顶层 AppRun/.desktop/图标。
  if command -v appimagetool >/dev/null 2>&1; then
    APPDIR="$RUST_DIR/target/release/bundle/appimage/AppDir"
    rm -rf "$APPDIR"; mkdir -p "$APPDIR"
    dpkg-deb -x "$DEB_OUTPUT" "$APPDIR" 2>/dev/null

    MAIN_BIN="$APPDIR/usr/bin/chain-chess"
    if [ ! -f "$MAIN_BIN" ]; then
      echo "  ⚠️  deb 里未找到 usr/bin/chain-chess，跳过 AppImage"
      echo ""
    else
      # AppImage 顶层要求：.desktop、图标、AppRun
      DESKTOP_SRC=$(ls -1 "$APPDIR"/usr/share/applications/*.desktop 2>/dev/null | head -1)
      [ -n "$DESKTOP_SRC" ] && cp "$DESKTOP_SRC" "$APPDIR/"
      ICON_SRC=$(find "$APPDIR/usr/share/icons" -name '*.png' 2>/dev/null | sort -r | head -1)
      [ -n "$ICON_SRC" ] && cp "$ICON_SRC" "$APPDIR/"
      ln -sf usr/bin/chain-chess "$APPDIR/AppRun"

      APP_OUTPUT="release/${PRODUCT}-${VERSION}.AppImage"
      # appimagetool 自身也是 AppImage，无 FUSE 环境需要 extract-and-run
      if APPIMAGE_EXTRACT_AND_RUN=1 ARCH=$(uname -m) appimagetool "$APPDIR" "$APP_OUTPUT" >/dev/null 2>&1; then
        echo "  ✅ Linux AppImage 构建完成: ${APP_OUTPUT} ($(ls -lh "$APP_OUTPUT" | awk '{print $5}'))"
        echo ""
      else
        echo "  ⚠️  AppImage 打包失败（已跳过，deb 不受影响）"
        echo ""
      fi
    fi
  else
    echo "  ⚠️  未找到 appimagetool，跳过 AppImage"
    echo ""
  fi

  # 更新 update.json 的 linux 平台条目（跳转目标与 android/windows 一致）
  if [ -f "update.json" ]; then
    LINUX_URL="https://ywnh1.free.leoi.org"
    jq --arg url "$LINUX_URL" --argjson sz "$DEB_BYTES" \
       '.platforms.linux = {url: $url, size: $sz}' \
       update.json > tmp.json && mv tmp.json update.json
    echo "  📄 update.json: linux 平台已更新（url + size=${DEB_BYTES}）"
    echo ""
  fi
fi
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
  # 恢复 gradle.properties（aarch64 主机的 aapt2 原生覆盖）
  if [ -f "${GRADLE_PROPS}.bak" ]; then
    cp "${GRADLE_PROPS}.bak" "$GRADLE_PROPS"
    rm -f "${GRADLE_PROPS}.bak"
    echo ""
    echo "  🧹 已恢复: gradle.properties"
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

# ── aapt2 原生覆盖（aarch64 主机）────────────────────────
# AGP 从 maven.google.com 取的 aapt2 只有 linux-x86_64 版：在 aarch64 主机
# （PRoot-Distro / Termux 等）上执行会 SIGILL（Illegal instruction），
# 表现为 "AAPT2 ... Daemon startup failed"（AarResourcesCompilerTransform 失败）。
# 用 android.aapt2FromMavenOverride 指向 build-tools 里的本机原生 aapt2 即可绕过；
# 构建结束由 cleanup_all 恢复 gradle.properties。
HOST_ARCH=$(uname -m)
if [ -f "$GRADLE_PROPS" ] && [ -n "$ANDROID_HOME" ] \
   && { [ "$HOST_ARCH" = "aarch64" ] || [ "$HOST_ARCH" = "arm64" ]; }; then
  AAPT2_NATIVE=""
  for cand in $(ls -r "${ANDROID_HOME}"/build-tools/*/aapt2 2>/dev/null); do
    if "$cand" version >/dev/null 2>&1; then
      AAPT2_NATIVE="$cand"
      break
    fi
  done
  if [ -n "$AAPT2_NATIVE" ]; then
    cp "$GRADLE_PROPS" "${GRADLE_PROPS}.bak"
    sed -i '/^android\.aapt2FromMavenOverride=/d' "$GRADLE_PROPS"
    # 用 sed $a 追加：gradle.properties 末尾没有换行符，>> 会拼坏最后一行
    sed -i "\$a android.aapt2FromMavenOverride=${AAPT2_NATIVE}" "$GRADLE_PROPS"
    echo "  🧩 ${HOST_ARCH} 主机：aapt2 改用本机原生二进制（构建后自动恢复 gradle.properties）"
    echo "     ${AAPT2_NATIVE}"
    echo ""
  else
    echo "  ⚠️  ${HOST_ARCH} 主机但未找到可执行的原生 aapt2"
    echo "     AGP 自带的是 x86_64 aapt2，本机跑不起来，构建会失败。"
    echo "     请在 ${ANDROID_HOME}/build-tools/<版本>/ 放置 aarch64 版 aapt2 后重试。"
    echo ""
  fi
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
# native 模式不碰 update.json（native 产物无对应发布条目）
if [ "$NATIVE" = true ]; then
  echo ""
  echo "  🚫 --native 模式：跳过 update.json 更新"
  echo ""
else
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  📄 验证 update.json"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

if [ -f "update.json" ]; then
  # 用 APK 真实字节数更新 .android.size（--argjson 要求合法 JSON 数值）
  jq --argjson sz "$FILESIZE_BYTES" '.platforms.android.size = $sz' update.json > tmp.json && mv tmp.json update.json

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
fi # end APK 更新（native 跳过）

fi # end APK
# ── 收尾：展示全部产物 ────────────────────────────────────
echo ""
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
echo "  📦 release/ 目录产物"
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
ls -lh release/ 2>/dev/null | grep -E "chainchess|chain-chess-pwa" || true
echo ""

# ── 发布动作（仅 --release）────────────────────────────────
if [ "$PUBLISH" = true ]; then
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo "  🚀 发布：release/ → /storage/emulated/0/用户/"
  echo "        update.json + PWA 必要内容 → ../chain-chess-release"
  echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
  echo ""

  # 1) 复制 release/ 全部产物到 Android 设备目录（仅当路径存在时）
  if [ -d "/storage/emulated/0/用户" ]; then
    cp -f release/chainchess-*.apk release/chainchess-*.exe release/chain-chess-pwa-*.zip release/chainchess-*.deb release/chainchess-*.AppImage /storage/emulated/0/用户/ 2>/dev/null || true
    echo "  📱 release/ 产物已复制到 /storage/emulated/0/用户/"
  else
    echo "  ⚠️  未找到 /storage/emulated/0/用户/，跳过设备复制"
  fi

  # 2) 复制 update.json + PWA 必要内容到发布仓库（仅当仓库存在时）
  if [ -d "../chain-chess-release" ]; then
    if [ -f "update.json" ]; then
      cp -f update.json ../chain-chess-release/
      echo "  📄 update.json 已复制到 ../chain-chess-release"
    else
      echo "  ⚠️  未找到 update.json，跳过（发布仓库将缺少更新数据）"
    fi
    # PWA 必要内容：运行所需文件（排除 wasm 源码 / pkg-node / 仓库自有文档 .git 等）
    for f in index.html app.js style.css engine.js sw.js manifest.webmanifest; do
      [ -f "docs/$f" ] && cp -f "docs/$f" ../chain-chess-release/
    done
    cp -rf docs/icons ../chain-chess-release/ 2>/dev/null || true
    cp -rf docs/audio ../chain-chess-release/ 2>/dev/null || true
    cp -rf docs/pkg ../chain-chess-release/ 2>/dev/null || true
    echo "  🌐 PWA 必要内容已复制到 ../chain-chess-release"
  else
    echo "  ⚠️  未找到 ../chain-chess-release，跳过仓库复制"
  fi
  echo ""
fi

if [ "$COMMIT" = true ]; then
  do_commit
fi

echo ""
