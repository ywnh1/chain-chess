# 连锁棋 · PWA 版

把 Tauri 桌面/移动应用迁移到浏览器 PWA（Progressive Web App）的产物。
**不修改 `tauri/` 目录任何内容**，本目录是独立副本 + WASM 引擎。

## 架构

```
pwa/
├── index.html / style.css / app.js   # 前端 UI（复制自 tauri/public，做了最小改造）
├── engine.js                         # 桥接层：模拟 Tauri invoke()
├── manifest.webmanifest / sw.js      # PWA 清单 + Service Worker（离线缓存）
├── icons/                            # PWA 图标（192/512/maskable）
├── audio/                            # 音效资源
├── pkg/                              # WASM 引擎产物（wasm-pack 构建）
│   ├── chain_chess_engine.js         #   web target glue
│   └── chain_chess_engine_bg.wasm    #   3.5MB（含内嵌 XGBoost 模型）
├── wasm/                             # WASM 引擎源码（Rust crate）
│   ├── src/lib.rs                    #   从 tauri/src-tauri/src/lib.rs 提取的纯逻辑
│   ├── xgb_model_board.json          #   XGBoost 模型（内嵌编译）
│   └── vendor/alpha_beta_pruning/    #   vendor 化（rayon 改为可选 feature）
└── pkg-node/                         # nodejs target 产物（仅本地测试用，可删除）
```

## 迁移方案

核心决策：**游戏引擎（规则 + AI）用 Rust 编译成 WASM**，而不是用 JS 重写。
`tauri/src-tauri/src/lib.rs` 的 2755 行中，纯逻辑部分（游戏规则、
Alpha-Beta / PVS / MCTS / 策略搜索、XGBoost 机器学习评估）被提取为
`pwa/wasm` crate，通过 `wasm-bindgen` 导出 4 个函数：

| WASM 导出 | 对应 Tauri 命令 |
|---|---|
| `process_move_cmd` | `process_move` |
| `ai_move_cmd`（按 `algorithm` 分派） | `ai_move` / `ai_move_v2` / `ai_move_mcts` / `ai_move_strategy` |
| `simulate_to_end_cmd` | `simulate_to_end` |
| `engine_version` | — |

Tauri 特定功能由 `engine.js` 在浏览器侧等价实现：

| Tauri 能力 | PWA 实现 |
|---|---|
| 文件系统存储（历史/存档/设置） | `localStorage`（`chainchess:` 前缀） |
| 导出对话框（`export_game_history_dialog`） | Blob 下载（返回 `fallback:字节数`，兼容前端） |
| `import_game_history` | 按 `id` 去重合并 |
| 触觉反馈（haptics 插件） | 前端已有 `navigator.vibrate` 回退，无需改动 |
| 更新机制（check/download/install） | 降级：更新提示仍显示，下载按钮提示"测试模式" |
| `exit_app` | 无操作 |

前端接入方式：`pwa/app.js` 的 `tauriInvoke()` 增加降级分支——非 Tauri
环境转调 `window.ChainEngine.webInvoke(cmd, args)`。原 Tauri 调用路径
不受影响（若本页面运行在 Tauri WebView 里，行为与原来完全一致）。

## 构建 WASM 引擎

```bash
cd pwa/wasm
wasm-pack build --target web --out-dir ../pkg --release
# 本地测试用（可选）：
wasm-pack build --target nodejs --out-dir ../pkg-node --release
```

说明：
- `alpha_beta_pruning` 0.1.0 硬依赖 rayon（无法编译到 wasm32），已 vendor 到
  `wasm/vendor/` 并把 rayon 改为可选 feature（`default-features = false`）。
- 原代码 3 处 `par_iter()` 在 WASM 版改为顺序 `iter()`（wasm 单线程）。
- `rand` 0.8 在 wasm 需要 `getrandom` 的 `js` feature（已在 Cargo.toml 处理）。
- rustc 1.97 生成的 bulk-memory 指令与 wasm-pack 自带旧版 wasm-opt 不兼容，
  Cargo.toml 已设置 `wasm-opt = false`（Rust 侧已 `-O3` + LTO）。
- 模型 `xgb_model_board.json`（3.3MB）内嵌进 wasm 二进制，加载即用，离线可用。

## 运行

PWA 必须通过 HTTP(S) 访问（动态 import + wasm fetch 在 `file://` 下不可用）：

```bash
cd pwa
python3 -m http.server 8899
# 浏览器打开 http://localhost:8899
```

- 首次访问后可"安装"（Chrome 地址栏安装图标），随后可离线使用
  （Service Worker 已预缓存全部资源）。
- 移动端 PWA 同样支持（触感反馈走 `navigator.vibrate`）。

## 已知差异 / 限制

- **存储容量**：历史记录用 `localStorage`（约 5MB）。Tauri 版写入文件系统无
  此限制。若未来记录很多，可升级为 IndexedDB（`engine.js` 的存储命令已
  集中封装，改动点很小）。
- **AI 速度**：WASM 版去掉了根级并行搜索（rayon），深度大的 AI 思考会比
  Tauri 桌面版慢（浏览器单线程），移动端尤其明显。深度 ≤ 3 时体感无差。
- **更新提示**：PWA 下的"检查更新"仍指向 Android/Linux 安装包，对网页无
  实际意义，仅提示"测试模式"，不影响使用。
- **导出路径**：Tauri 版弹出系统对话框选路径；PWA 版直接下载到下载目录。

## 与 Tauri 版同步维护

引擎源码 `pwa/wasm/src/lib.rs` 是从 `tauri/src-tauri/src/lib.rs` 提取的快照。
若 Tauri 版修改了游戏规则或 AI 逻辑，需要重新提取：

```bash
python3 - <<'EOF'   # 见下方说明：行区间随上游变动需调整
EOF
```

提取规则（`tauri/src-tauri/src/lib.rs` 1-indexed 行号）：
- 核心类型：19–111（BorderMode / Cell / GameBoard / ProcessMoveResult 等）
- SimulateResult：252–263
- `simulate_to_end` 纯函数体：279–401（去掉 `spawn_blocking` 包装）
- 纯逻辑主体：938–2636（XGBoost 引擎 → `find_best_move_by_alg`）

提取后需人工处理：rayon `par_iter()` → `iter()`、`std::fs` 函数加
`#[cfg(not(target_arch = "wasm32"))]`。建议在修改 Tauri 引擎时同步跑一遍
`wasm-pack build` 验证。
