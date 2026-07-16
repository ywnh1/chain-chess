<div align="center">
  <h1>♟ 连锁棋 · Chain Chess</h1>
  <p>
    <strong>棋盘策略游戏 · 支持桌面 / Android</strong>
  </p>
  <p>
    <img src="https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=fff" alt="Tauri">
    <img src="https://img.shields.io/badge/Android-APK-3DDC84?logo=android&logoColor=fff" alt="Android">
    <img src="https://img.shields.io/badge/Rust-Rayon-F74C00?logo=rust&logoColor=fff" alt="Rust">
    <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT">
    <img src="https://img.shields.io/github/v/release/nihao15900375400/chain-chess?include_prereleases&label=release" alt="Release">
  </p>
</div>

---

**连锁棋** 是一款基于「爆裂棋 / Chain Reaction」玩法改进的多人策略棋盘游戏。支持本地多人和 AI 对战，棋子数量超过棋盘格容量时会发生连锁爆裂，扩散到相邻格子。

当前版本为纯客户端应用（Tauri + 纯前端），无需服务器。

## 🎮 游戏规则

1. **落子** — 点击空位落子（首位玩家任意落子，后续玩家避开首子 12 格限制区），点击自己的棋子加子
2. **爆裂** — 格子中的棋子数达到容量（4 个）时，向上下左右各扩散一个棋子
3. **连锁** — 爆裂扩散会触发相邻格子的连锁爆裂
4. **淘汰** — 当某玩家的所有棋子被吞噬时该玩家被淘汰
5. **胜利** — 最后存活的一位玩家获胜

## ✨ 功能特性

| 功能 | 说明 |
|------|------|
| 👥 **本地对战** | 2~7 人在同一设备轮流落子 |
| 🤖 **AI 对战（PVE）** | 1 名人类 + 1~6 个 AI，可自定义颜色 |
| ⚔️ **AI 斗蛐蛐** | 2~10 个 AI 对战，每个 AI 独立配置算法和深度 |
| ⚡ **Rust + Rayon 引擎** | Alpha-Beta 使用 Rust 实现，Rayon 多核并行搜索 |
| 🎛️ **搜索深度可调** | Alpha-Beta 深度 1~10 可配置 |
| 🎲 **随机化走法** | 早期对局加入随机探索，同等走法随机选择 |
| 🎯 **首子限制区域** | 首位玩家任意落子，后续玩家避开 12 格限制区 |
| ⏸ **暂停功能** | 游戏暂停时查看实时统计和折线图 |
| 🔊 **音效系统** | 落子/爆炸/淘汰/获胜 音频反馈（Web Audio API 合成） |
| 📊 **结算统计** | 游戏结束后展示双折线图（棋子数/点数变化） |
| 📱 **响应式 UI** | 平板横屏 / 手机竖屏自动适配 |
| 🎨 **深色主题** | 高对比度配色，护眼舒适 |
| 🖥️ **桌面应用** | 基于 Tauri v2 的桌面客户端，Rust 后台 |
| 📱 **Android APK** | 支持 Android 设备原生运行 |
| 📋 **历史记录** | 自动保存对局记录，可回看统计图表 |

## 🚀 快速开始

### 桌面应用（Tauri）

```bash
cd tauri
npm install
npx tauri dev          # 开发模式
cargo build --release  # 构建可执行文件
# 二进制文件：tauri/src-tauri/target/release/chain-chess
```

### Android APK

```bash
# 一键编译 + 签名
./build_apk.sh <keystore_password>

# 例如：
./build_apk.sh chainchess
```

> 脚本自动编译 arm64 APK，用 `release.keystore` 签名并输出到 `release/` 目录。

预编译 APK 可在 [Releases](https://github.com/nihao15900375400/chain-chess/releases) 下载。

#### 首次构建（已初始化则跳过）

```bash
cd tauri
npx tauri android init      # 初始化 Android 项目（仅首次）
cd ..
./build_apk.sh chainchess
```

## 🤖 AI 引擎

### 策略算法（Strategy）

基于规则的启发式搜索，速度快、消耗低，所有平台均可使用：

1. **三级优先** — 优先引爆即将爆炸的棋子（count = 3）
2. **安全升级** — 避开对手三级棋子附近的二级棋子
3. **一进二 / 下三级** — 逐步升级棋子等级
4. **首子中心** — 首回合落在非边角位置，选最靠近棋盘中心的格子

### Alpha-Beta 剪枝（Rust + Rayon 🦀）

桌面 & Android 端均可使用的强力搜索算法：

- **Rust 实现** — 全部棋盘逻辑和搜索代码用 Rust 编写，极致性能
- **Rayon 多线程** — 根节点走法并行搜索，充分利用多核 CPU
- **走法排序** — 三级棋子优先、周围对手多优先，提高剪枝效率
- **自适应限时** — 每步自动分配时间
- **分支限制** — 每层最多搜索 top 10 个走法
- **深度可调** — 支持 1~10 层搜索（默认 2）
- **随机化探索** — 早期对局加入随机扰动，增加走法多样性

### AI 设置界面

- **PVE 模式** — 算法选择：策略算法 / Alpha-Beta 剪枝；搜索深度滑块（1~10）；用户可选择自己的颜色
- **AI 斗蛐蛐模式** — 支持 2~10 个 AI 对战，每个 AI 独立配置算法和深度；策略算法不显示深度选项
- 思考中提示

## 🔊 音效系统

使用 **Web Audio API** 合成，无需额外音频文件：

| 事件 | 音效 | 波形 |
|------|------|------|
| 🎯 落子 | 短促高音 | 方波 |
| 💥 爆炸 | 低沉噪声 | 锯齿波 |
| ❌ 淘汰 | 下行音阶 | 合成音序 |
| 🏆 获胜 | 上行琶音 | 合成音序 |

## 📊 结算统计

游戏结束后自动展示：

- 🏆 **获胜者** — 用颜色名称标识（如"红色玩家获胜"）
- 📊 **统计卡片** — 每位玩家展示棋子数 / 点数 / 连爆统计
- 📈 **折线图 1** — 所有玩家棋子数变化趋势
- 📈 **折线图 2** — 所有玩家点数（按等级加权）变化趋势
- 🔥 **最高连爆** — 本局最长连锁爆裂记录

## 📁 项目结构

```
chain-chess/
├── build_apk.sh               # Android APK 一键构建 + 签名脚本
├── release.keystore           # Android 签名密钥（请勿公开）
├── tauri/
│   ├── package.json           # Node 项目配置（Tauri CLI）
│   ├── public/
│   │   └── index.html         # 前端（完整游戏 UI + AI + 音效，单页应用）
│   └── src-tauri/
│       ├── Cargo.toml         # Rust 依赖（tauri + rayon + serde）
│       ├── tauri.conf.json    # Tauri 窗口与应用配置
│       ├── build.rs           # Tauri 构建脚本
│       ├── capabilities/
│       │   └── default.json   # 权限声明
│       ├── gen/               # Android 平台生成代码
│       └── src/
│           ├── main.rs        # Rust 入口
│           └── lib.rs         # 🦀 游戏引擎（棋盘逻辑 + Alpha-Beta + Rayon）
├── LICENSE                    # MIT 开源协议
└── README.md                  # 本文件
```

## 🔧 技术栈

| 层级 | 技术 |
|------|------|
| 🌐 前端 | 纯 HTML / CSS / JavaScript 单页应用（Canvas 绘制棋盘） |
| 🦀 AI 引擎 | Rust + Rayon 并行计算（Alpha-Beta 剪枝） |
| 🖥️ 桌面框架 | Tauri v2 + 系统 WebView |
| 📱 移动端 | Android APK（Tauri 构建） |
| 🎨 音效 | Web Audio API 合成（无额外音频文件） |

## 📄 开源协议

本项目基于 **MIT License** 开源 — 详见 [LICENSE](LICENSE) 文件。

Copyright (c) 2026 ywnh1

---

<div align="center">
  <sub>Built with ♟ by <a href="https://github.com/nihao15900375400">ywnh1</a> · v1.3.1-beta</sub>
</div>
