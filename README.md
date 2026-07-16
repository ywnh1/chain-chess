<div align="center">
  <h1>♟ 连锁棋 · Chain Chess</h1>
  <p>
    <strong>多人实时在线棋盘游戏 · 支持 Web / 桌面 / Android</strong>
  </p>
  <p>
    <img src="https://img.shields.io/badge/Python-3.10+-3776AB?logo=python&logoColor=fff" alt="Python">
    <img src="https://img.shields.io/badge/FastAPI-WebSocket-009688?logo=fastapi&logoColor=fff" alt="FastAPI">
    <img src="https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=fff" alt="Tauri">
    <img src="https://img.shields.io/badge/Android-APK-3DDC84?logo=android&logoColor=fff" alt="Android">
    <img src="https://img.shields.io/badge/Rust-Rayon-F74C00?logo=rust&logoColor=fff" alt="Rust">
    <img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT">
    <img src="https://img.shields.io/github/v/release/ywnh1/chain-chess?include_prereleases&label=release" alt="Release">
  </p>
</div>

---

**连锁棋** 是一款基于「爆裂棋 / Chain Reaction」玩法改进的多人策略棋盘游戏。支持 2~7 人在同一房间中轮流落子，棋子数量超过棋盘格容量时会发生连锁爆裂，扩散到相邻格子。

## 🎮 游戏规则

1. **落子** — 点击空位落子（首位玩家任意落子，后续玩家避开首子 12 格限制区），点击自己的棋子加子
2. **爆裂** — 格子中的棋子数达到容量（4个）时，向上下左右各扩散一个棋子
3. **连锁** — 爆裂扩散会触发相邻格子的连锁爆裂
4. **淘汰** — 当某玩家的所有棋子被吞噬时该玩家被淘汰
5. **胜利** — 最后存活的一位玩家获胜

## ✨ 功能特性

| 功能 | 说明 |
|------|------|
| 🕹️ 多人在线 | 2~7 人实时对战，WebSocket 通信 |
| 🔐 密码房间 | 创建房间时可设置密码，私密对战 |
| 💾 存档系统 | 游戏过程中随时存档，设置密码保护 |
| 🤖 **AI 对战** | **PVE：1 名人类 + 1~6 个 AI，可自定义颜色** |
| ⚔️ **AI 斗蛐蛐** | **2~10 个 AI 对战，每个 AI 可独立配置算法和深度** |
| ⚡ **Rust + Rayon 引擎** | **桌面端 Alpha-Beta 使用 Rust 实现，Rayon 多核并行搜索** |
| 🎛️ **搜索深度可调** | Alpha-Beta 深度 1~7 可配置 |
| 🎲 **随机化走法** | 早期对局加入随机探索，同等走法随机选择 |
| 🎯 **首子限制区域** | 首位玩家任意落子，后续玩家避开 12 格限制区 |
| ⏸ **暂停功能** | 游戏暂停时查看实时统计和折线图 |
| 🔊 **音效系统** | 落子/爆炸/淘汰/获胜 音频反馈 |
| 📊 **结算统计** | 游戏结束后显示双折线图（棋子数/点数变化） |
| 📱 **响应式 UI** | 平板横屏 / 手机竖屏自动适配 |
| 🎨 **深色主题** | 高对比度配色，护眼舒适 |
| 🖥️ **桌面应用** | 基于 Tauri v2 的桌面客户端，Rust 后台 |
| 📱 **Android APK** | 支持 Android 设备原生运行 |

## 🚀 快速开始

### 方式一：Web 服务器（推荐）

```bash
# 1. 创建虚拟环境
python3 -m venv venv
source venv/bin/activate

# 2. 安装依赖
pip install fastapi uvicorn websockets

# 3. 启动服务器（自动处理依赖检查、进程管理）
./start.sh

# 4. 打开浏览器访问
# http://localhost:8000
```

> 💡 直接 `python3 main.py` 也可启动，但 `start.sh` 提供更完善的守护进程管理。

### 方式二：桌面应用（Tauri）

```bash
cd tauri
npm install
npx tauri dev          # 开发模式
cargo build --release  # 构建可执行文件
# 二进制文件：tauri/src-tauri/target/release/chain-chess
```

桌面版额外提供 **Alpha-Beta 剪枝 AI**（Rust + Rayon 多线程），搜索深度可达 7 层。

### 方式三：Android APK

```bash
# 一键编译 + 签名（初次使用需先初始化 Android 项目）
./build_apk.sh <keystore_password>

# 例如：
./build_apk.sh chainchess
```

> 脚本自动编译 arm64 APK，用 `release.keystore` 签名并输出到 `release/` 目录。

预编译 APK 可在 [Releases](https://github.com/ywnh1/chain-chess/releases) 下载。

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

桌面端专属的强力搜索算法：

- **Rust 实现** — 全部棋盘逻辑和搜索代码用 Rust 编写，极致性能
- **Rayon 多线程** — 根节点走法并行搜索，充分利用多核 CPU
- **走法排序** — 三级棋子优先、周围对手多优先，提高剪枝效率
- **自适应限时** — 每步自动分配时间（3 s + depth × 0.5 s）
- **分支限制** — 每层最多搜索 top 10 个走法
- **深度可调** — 支持 1~7 层搜索（默认 2）
- **随机化探索** — 早期对局（前 5~7 局）加入随机扰动，增加走法多样性

### AI 设置界面

- **PVE 模式** — 算法选择：策略算法 / Alpha-Beta 剪枝；搜索深度滑块（1~7）；用户可选择自己的颜色
- **AI 斗蛐蛐模式** — 支持 2~10 个 AI 对战，每个 AI 独立配置算法和深度
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
- 📊 **统计卡片** — 每位玩家展示棋子数 / 点数
- 📈 **折线图 1** — 所有玩家棋子数变化趋势
- 📈 **折线图 2** — 所有玩家点数（按等级加权）变化趋势

## 📁 项目结构

```
chain-chess/
├── main.py                   # FastAPI WebSocket 游戏服务器
├── run.py                    # 守护进程（崩溃自动重启）
├── start.sh                  # 启动/停止/依赖检查脚本
├── static/
│   └── index.html            # 网页版前端（完整游戏 UI + AI + 音效）
├── tauri/
│   ├── package.json           # Node 项目配置（Tauri CLI）
│   ├── public/
│   │   └── index.html         # Tauri 桌面/移动端前端
│   └── src-tauri/
│       ├── Cargo.toml         # Rust 依赖（tauri + rayon + serde）
│       ├── tauri.conf.json    # Tauri 窗口与应用配置
│       ├── build.rs           # Tauri 构建脚本
│       ├── capabilities/
│       │   └── default.json   # 权限声明
│       └── src/
│           ├── main.rs        # Rust 入口
│           └── lib.rs         # 🦀 游戏引擎（棋盘逻辑 + Alpha-Beta + Rayon）
├── build_apk.sh               # Android APK 一键构建 + 签名脚本
├── release/                   # 发布构建产物（APK）
├── release.keystore           # Android 签名密钥（请勿公开）
├── logs/                      # 运行时日志
├── LICENSE                    # MIT 开源协议
└── README.md                  # 本文件
```

## 🔧 技术栈

| 层级 | 技术 |
|------|------|
| 🖥️ 后端 | Python 3 + FastAPI + WebSocket + Uvicorn |
| 🌐 前端 | 纯 HTML / CSS / JavaScript 单页应用（Canvas 绘制） |
| 🦀 桌面 AI 引擎 | Rust + Rayon 并行计算 |
| 🖥️ 桌面框架 | Tauri v2 |
| 📱 移动端 | Android APK（Tauri 构建） |
| 🔗 通信协议 | JSON over WebSocket |

## 🐳 start.sh 参考

```bash
./start.sh          # 启动服务器（含依赖检查、进程守护）
./start.sh stop     # 停止服务器
```

- 自动创建虚拟环境检查
- 自动校验 Python 依赖完整性
- 可选启动 ngrok 外网穿透
- 守护进程自动重启（`run.py`）

## 📄 开源协议

本项目基于 **MIT License** 开源 — 详见 [LICENSE](LICENSE) 文件。

Copyright (c) 2026 ywnh1

---

<div align="center">
  <sub>Built with ♟ by <a href="https://github.com/ywnh1">ywnh1</a> · v1.3.1-beta</sub>
</div>
