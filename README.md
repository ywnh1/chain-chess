<div align="center">
  <img src="icon.png" width="140" alt="连锁棋">
  <h1>♟ 连锁棋 · Chain Chess</h1>
  <p>
    <strong>棋盘策略游戏 · 桌面 / Android / 浏览器 PWA</strong>
  </p>
  <p>
    <img src="https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=fff" alt="Tauri">
    <img src="https://img.shields.io/badge/Android-APK-3DDC84?logo=android&logoColor=fff" alt="Android">
    <img src="https://img.shields.io/badge/Rust-Rayon-F74C00?logo=rust&logoColor=fff" alt="Rust">
    <img src="https://img.shields.io/badge/PWA-WASM-5A67D8?logo=pwa&logoColor=fff" alt="PWA">
    <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT">
    <img src="https://img.shields.io/badge/version-3.3.7-orange" alt="v3.3.7">
  </p>
</div>

---

你点下的每一颗棋子，都可能引爆整张棋盘。

**连锁棋** 是基于 Chain Reaction / 爆裂棋玩法改进的多人策略游戏。2~10 人对战，棋子攒满格子就炸，炸出来的棋子接着引爆下一轮，直到只剩一人。

前端和 AI 引擎是同一份代码，出三种形态：**Tauri 桌面 / Android 应用**（Rust 引擎 + Rayon 多核并行），以及**浏览器 PWA**（引擎编译成 WASM，可安装离线游玩）。都不用服务器，本地就能开局。

---

## 🎮 游戏规则

- **落子**：点空位落子，点自己的棋子加子。首步要避开已有棋子周围 12 格
- **爆裂**：格子里的棋子攒到 4 颗就爆，向上下左右各扩散一颗
- **连锁**：扩散出去的棋子会引爆相邻格子，接着往下传
- **淘汰**：某位玩家的棋子被全部吞噬，出局
- **胜利**：最后存活的一位玩家获胜

---

## ✨ 功能一览

### 游戏模式

- **本地对战**：2~10 人在同一台设备上轮流落子，每人可自定义名称
- **AI 对战（PVE）**：1 名人类 + 1~9 个 AI，每个 AI 的算法、搜索深度、随机刻度都能单独配
- **AI 斗蛐蛐**：2~10 个 AI 互相对战，纯观赏

### 游戏设置

- **棋盘大小**：5×5 到 19×19 自由选择
- **玩家配置**：弹窗里设置每位玩家的类型（人类 / 各类 AI）、名称、算法、搜索深度、随机刻度和评估函数（ML / 手写）
- **随机刻度**：0%~30% 可调，开局固定 40%，逐渐降到设定值

### AI 引擎

- **四种算法**：策略（启发式规则）、Alpha-Beta 剪枝、PVS NegaMax、MCTS 蒙特卡洛树搜索
- **Rust + Rayon**：都用 Rust 实现，Rayon 多核并行，深度 1~10 层可配

### 用户界面

- **液态玻璃主题**：深色虹彩渐变玻璃 UI，10 个彩色光球缓慢浮动
- **响应式布局**：平板横屏、手机竖屏各自适配
- **挖孔屏适配**：用 `safe-area-inset-top` 处理刘海和挖孔
- **路由管理**：统一 Router 管理页面，带生命周期钩子和智能回退导航

### 数据与统计

- **结算图表**：对局结束后展示双折线图（棋子数 / 点数变化），支持预览、全屏和缩放
- **历史记录**：自动保存对局，可回看统计图表
- **中断恢复**：异常退出或手动暂停时自动存下未完成的对局，历史页面一键继续
- **内存 + 磁盘两层存储**：超过 500 步自动溢出到磁盘，内存不会无限涨

### 设备性能检测

- **WebView 性能**：WebGL 体积着色器压力测试，实时显示 FPS 与帧时间，报告平均 / 最低 / 1% Low FPS、丢帧统计以及 WebView、GPU 信息
- **AI 计算性能**：50 局 AI 对局基准（棋盘、人数、算法、深度都可选），逐局刷新进度，最后报告各算法胜率和每局耗时

### 音效与反馈

- **合成音效**：落子、爆炸、淘汰、获胜各有独立的音频反馈（Web Audio API）
- **震动反馈**：Android 原生支持，桌面静默降级
- **连锁动画跳过**：长连爆可以一键跳过

### 平台形态

- **桌面 / Android**：Tauri 应用，自动更新、触感反馈、文件系统存储
- **浏览器 PWA**：`docs/` 目录独立构建（WASM 引擎），浏览器打开就能玩，可安装离线使用

---

## 🚀 快速开始

### 桌面应用

```bash
cd tauri
npm install
npx tauri dev          # 开发模式
npx tauri build        # 构建可执行文件
```

### Android APK / Windows exe

```bash
./build.sh --apk chainchess        # 构建签名 APK
./build.sh --exe                   # 构建 Windows exe（cargo-xwin 交叉编译）
./build.sh --zip                   # 打包 PWA zip（用 docs/ 预编译产物，无需编译）
./build.sh --all chainchess        # 一次构建 apk + exe + zip
./build.sh --native chainchess     # APK 加 target-cpu=native 极致优化
./build.sh --release chainchess    # 构建全部并发布到设备与发布仓库
```

- **APK**：脚本自动编译 arm64 APK，用 `release.keystore` 签名后输出到 `release/`（需要 keystore 密码）。
- **exe**：`cargo-xwin` 交叉编译 `x86_64-pc-windows-msvc`，不需要密码，输出 `release/chainchess-<version>.exe`。
- **zip**：取 `docs/` 的静态资源和预编译 `pkg/*.wasm` 打包成 `release/chain-chess-pwa-v<version>.zip`（排除 wasm 源码和 pkg-node）。
- **all / release**：`--all` 一次编译 apk + exe + zip；`--release` 额外发布到 Android 设备和 `../chain-chess-release` 仓库。
- 每次构建都会更新 `update.json` 里对应平台的 URL 与 size。

### 浏览器 PWA（本地预览）

```bash
cd docs
python3 -m http.server 8899
# 浏览器打开 http://localhost:8899
```

PWA 必须通过 HTTP(S) 访问，`file://` 下 WASM 动态加载用不了。首次打开后可以安装，之后离线也能玩。

首次构建 APK 需要先初始化 Android 项目：

```bash
cd tauri
npx tauri android init
cd ..
./build.sh chainchess
```

预编译好的应用可以在[下载中心](https://ywnh1.free.leoi.org)下载，或者直接去 [Gitee Releases](https://gitee.com/ywnh1/chain-chess-release/releases)。

---

## 🤖 AI 引擎详解

内置四款 AI，分属三种思路：启发式规则、树搜索剪枝、随机模拟。

### ⚡ 策略算法（启发式规则）

Rust 实现，按人工定的优先级出招，不做树搜索，响应最快，适合新手入门和低算力设备。

- **三级优先**：棋子攒到 3 颗就先动它，好触发连锁
- **安全升级**：躲开对手三级棋子旁边的二级棋子，免得被吞
- **渐进控制**：一进二、下三级，逐步扩大地盘
- **首子中心**：第一手落在棋盘中央附近

### 🧠 Alpha-Beta 剪枝

标准 Alpha-Beta 剪枝，Rust + Rayon 多核并行。每次走法克隆棋盘而不是撤销，多玩家连锁反应天然适配。

- **Rayon 多线程**：根节点走法并行搜索，吃满多核 CPU
- **走法排序**：三级棋子优先、周围对手多的优先，剪枝更有效
- **根级分支**：根级只搜 top 10 走法，内部节点不设分支限制，全量搜索
- **深度可调**：1~10 层（默认 2）
- **随机化探索**：开局加入随机扰动，走法不至于千篇一律

### 🔬 PVS NegaMax（精炼剪枝）

PVS（Principal Variation Search，主变例搜索）是 Alpha-Beta 的精炼版。思路是：先假设第一个搜的走法最好，后面的走法用零窗口（Null Window）试探，快速证伪，省掉大量搜索。

- **Killer 启发**：每层记两条杀招走法，后续优先尝试
- **History 启发**：40×40 全局历史表，记录各位置的成功率
- **QSearch 静止搜索**：搜到底时只搜爆炸性走法（限 3 层），防止水平线效应
- **动态分支控制**：浅层多搜（depth=1 搜 14 条），深层少搜（depth=3 搜 8 条）
- **根级 Rayon 并行**：多核加速根层搜索

432 局两两对战评测（7×7、ML 评估、深度 2）里，树搜索赢得很彻底：Alpha-Beta 胜率 73.6%，PVS 71.3%，策略算法 46.8%，MCTS 只有 8.3%（深度 1 的迭代量不够）。完整报告在应用内「关于 → AI 性能评测」。

### 🎲 MCTS 蒙特卡洛树搜索

另一条路子：不用完整的评估函数，靠大量随机模拟（单次最多 40 步，提前终局就停）判断走法好坏。

```
SELECT → EXPAND → PLAYOUT → BACKPROPAGATE
```

- **UCB1**：平衡探索与利用
- **随机 Playout**：均匀随机选走法模拟
- **渐进展开**：子节点访问满 3 次才展开新分支
- **迭代数**：depth=1~10 对应 400~4000 次迭代，树节点上限 2000

擅长找剪枝搜索不容易发现的非直觉走法。

### 🤖 评估函数

Alpha-Beta 和 PVS 可以换用两种评估函数：

- **增强版评估**（默认）：在手写规则上加了位置权重、邻居威胁评分和爆发势能，棋力更强
- **手写规则**：经典启发式评估，稳定

在玩家配置弹窗里随时切换，不用重启游戏。

---

## 🎨 UI 设计

- **深色液态玻璃主题**：虹彩渐变玻璃卡片，`backdrop-filter: 32px` 毛玻璃，内斜面采光高光
- **背景光球动效**：10 个彩色光球，`filter: blur(70px)` 虚化浮动，与玻璃元素自然折射反射
- **棱镜色散**：玻璃元素内嵌 `::before` 高光
- **流体动画**：所有过渡使用 `cubic-bezier(0.34, 1.56, 0.64, 1)` 弹性曲线
- **自定义控件**：统一下拉选择器、按钮按压反馈、挖孔屏安全区适配

---

## 🔧 技术架构

```
┌──────────────────┐     ┌──────────────────┐
│  Web 前端（JS）   │     │  Rust 引擎        │
│                  │     │                  │
│  ┌─ UI 渲染 ──┐  │     │  ┌─ AI 搜索 ──┐  │
│  │ 棋盘/弹窗/   │  │     │  │A-B/PVS/MCTS│  │
│  │ 图表/路由    │  │     │  └───────────┘  │
│  └────────────┘  │     │  ┌─ 评估函数 ─┐   │
│  ┌─ 音效 ────┐  │◄────►│  │增强/手写   │   │
│  │Web Audio   │  │invoke│  └───────────┘  │
│  └────────────┘  │     │  ┌─ 游戏逻辑 ─┐  │
│  ┌─ 存储 ────┐  │     │  │落子/连锁/   │  │
│  │Tauri FS    │  │     │  │淘汰判定     │  │
│  └────────────┘  │     │  └───────────┘  │
└──────────────────┘     └──────────────────┘
```

- **前端**：原生 JavaScript + CSS，零框架依赖（`tauri/public/`，桌面和移动端共用）
- **桌面 / Android**：Rust 引擎（`tauri/src-tauri/`）+ Tauri v2 桥接，Rayon 多核并行
- **浏览器 PWA**：`docs/` 目录独立构建，同一引擎编译成 WASM（`docs/wasm/` crate + wasm-bindgen）；`engine.js` 在浏览器侧模拟 Tauri invoke 语义，`tauriInvoke` 自动降级到 `ChainEngine.webInvoke`
- **AI 引擎**：Alpha-Beta 用 `alpha_beta_pruning` crate，PVS 和 MCTS 自己实现
- **并行计算**：Rayon `par_iter()` 多核并行搜索（桌面端；WASM 版单线程顺序执行）
- **评估函数**：增强版手写评估（位置权重 + 威胁评分）或 XGBoost 模型（内嵌，可扩展）
- **存储**：桌面和移动端用 JSON 文件（`history.json`、`round_data.json`、`saved_game.json`），PWA 用 `localStorage`（`chainchess:` 前缀）

---

## 📝 许可证

MIT License，详见 [LICENSE](LICENSE)。
