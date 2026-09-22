# 连锁棋 · PWA 版

这是把 Tauri 桌面/移动应用搬到浏览器 PWA 的版本。
**不改 `tauri/` 目录里的任何东西**，本目录是独立副本，外加一个编译成 WASM 的引擎。

## 架构

```
docs/                                 # 本目录（PWA 独立构建）
├── index.html / style.css / app.js   # 前端 UI（与 tauri/public 同步维护，最小改造）
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

核心决策：**游戏引擎（规则 + AI）编译成 WASM**，不用 JS 重写一遍。
`tauri/src-tauri/src/lib.rs` 里的纯逻辑部分（游戏规则、Alpha-Beta / PVS / MCTS
/ 策略搜索、XGBoost 机器学习评估）被提取成本目录的 `wasm/` crate，用
`wasm-bindgen` 导出：

| WASM 导出 | 对应 Tauri 命令 |
|---|---|
| `process_move_cmd` | `process_move` |
| `ai_move_cmd`（按 `algorithm` 分派） | `ai_move` / `ai_move_v2` / `ai_move_mcts` / `ai_move_strategy` |
| `simulate_to_end_cmd` | `simulate_to_end` |
| `bench_ai_game_cmd` | `bench_ai_game`（设备性能检测 · AI 计算基准） |
| `engine_version` | — |

Tauri 特有的功能，由 `engine.js` 在浏览器侧等价实现：

| Tauri 能力 | PWA 实现 |
|---|---|
| 文件系统存储（历史/存档/设置） | `localStorage`（`chainchess:` 前缀） |
| 导出对话框（`export_game_history_dialog`） | Blob 下载（返回 `fallback:字节数`，兼容前端） |
| `import_game_history` | 按 `id` 去重合并 |
| 触觉反馈（haptics 插件） | 前端已有 `navigator.vibrate` 回退，无需改动 |
| 更新机制（check/download/install） | 降级：更新提示仍显示，下载按钮跳转下载中心 |
| `exit_app` | 无操作 |

前端接入方式：`app.js` 的 `tauriInvoke()` 加了一个降级分支，非 Tauri 环境转调
`window.ChainEngine.webInvoke(cmd, args)`。原来的 Tauri 调用路径没动，页面跑在
Tauri WebView 里时行为和以前完全一样。

## 构建 WASM 引擎

```bash
# 在 docs/ 目录内执行
cd wasm
wasm-pack build --target web --out-dir ../pkg --release
# 本地测试用（可选）：
wasm-pack build --target nodejs --out-dir ../pkg-node --release
```

编译时踩到几个坑，都已在 `Cargo.toml` 里处理：

- `alpha_beta_pruning` 0.1.0 硬依赖 rayon（编不到 wasm32），已 vendor 到
  `wasm/vendor/`，rayon 改成可选 feature（`default-features = false`）。
- 原代码 3 处 `par_iter()` 改成顺序 `iter()`（wasm 单线程）。
- `rand` 0.8 在 wasm 需要 `getrandom` 的 `js` feature。
- rustc 1.97 生成的 bulk-memory 指令和 wasm-pack 自带旧版 wasm-opt 不兼容，
  于是设了 `wasm-opt = false`（Rust 侧已经是 `-O3` + LTO）。

模型 `xgb_model_board.json`（3.3MB）内嵌进 wasm 二进制，加载即用，离线可用。

## 运行

PWA 必须通过 HTTP(S) 访问，`file://` 下动态 import 和 wasm fetch 都用不了：

```bash
# 在 docs/ 目录内执行
python3 -m http.server 8899
# 浏览器打开 http://localhost:8899
```

- 首次访问后可以"安装"（Chrome 地址栏的安装图标），之后离线可用，Service Worker 已经预缓存了全部资源。
- 移动端 PWA 同样支持，触感反馈走 `navigator.vibrate`。

## 已知差异 / 限制

- **存储容量**：历史记录用 `localStorage`，约 5MB 上限。Tauri 版写文件系统没这个限制。
  以后记录多了可以换成 IndexedDB，`engine.js` 的存储命令是集中封装的，改动点很小。
- **AI 速度**：WASM 版没有根级并行（rayon 用不了），深度大的 AI 会比 Tauri 桌面版慢，
  浏览器单线程，移动端更明显。深度 ≤ 3 时体感没差别。
- **更新提示**：PWA 同样参与更新检查，发现新版本后点"下载更新"会跳到下载中心
  （`https://ywnh1.free.leoi.org`）让用户选平台。
- **导出路径**：Tauri 版弹系统对话框选路径；PWA 版直接下载到下载目录。

## 与 Tauri 版同步维护

引擎源码 `wasm/src/lib.rs` 是从 `tauri/src-tauri/src/lib.rs` 提取的快照。
Tauri 版改了游戏规则或 AI 逻辑，就得重新提取：

```bash
python3 - <<'EOF'   # 见下方说明：行区间随上游变动需调整
EOF
```

提取按**函数/类型名**定位，行号随上游变动，别依赖具体行号：

- 核心类型：`BorderMode` / `CapMode` / `Cell` / `GameBoard` / `ProcessMoveResult` 等
- 结构体：`SimulateResult` 及其历史类型
- 纯函数：`simulate_to_end`（去掉 `spawn_blocking` 包装）
- 纯逻辑主体：XGBoost 引擎 → `find_best_move_by_alg`（含各算法搜索函数）

提取完还要人工处理两处：rayon `par_iter()` 改成 `iter()`，`std::fs` 函数加
`#[cfg(not(target_arch = "wasm32"))]`。建议改 Tauri 引擎时顺手跑一遍
`wasm-pack build` 验证。
