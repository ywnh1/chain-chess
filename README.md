<div align="center">
  <h1>♟ 连锁棋 · Chain Chess</h1>
  <p>
    <strong>多人实时在线棋盘游戏 · 支持 Web / 桌面 / Android</strong>
  </p>
  <p>
    <img src="https://img.shields.io/badge/Python-3.10+-3776AB?logo=python&logoColor=fff">
    <img src="https://img.shields.io/badge/FastAPI-WebSocket-009688?logo=fastapi&logoColor=fff">
    <img src="https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=fff">
    <img src="https://img.shields.io/badge/Android-APK-3DDC84?logo=android&logoColor=fff">
    <img src="https://img.shields.io/badge/Rust-Rayon-F74C00?logo=rust&logoColor=fff">
  </p>
</div>

---

**连锁棋** 是一款基于「爆裂棋 / Chain Reaction」玩法改进的多人策略棋盘游戏。支持 2~7 人在同一房间中轮流落子，棋子数量超过棋盘格容量时会发生连锁爆裂，扩散到相邻格子。

## 🎮 游戏规则

1. **落子**：点击空位落子（首次需远离所有已有棋子），点击自己的棋子加子
2. **爆裂**：格子中的棋子数达到容量（4个）时，向上下左右各扩散一个棋子
3. **连锁**：爆裂扩散会触发相邻格子的连锁爆裂
4. **淘汰**：当某玩家的所有棋子被吞噬时该玩家被淘汰
5. **胜利**：最后存活的一位玩家获胜

## ✨ 功能特性

| 功能 | 说明 |
|------|------|
| 🕹️ 多人在线 | 2~7 人实时对战，WebSocket 通信 |
| 🔐 密码房间 | 创建房间时可设置密码，私密对战 |
| 💾 存档系统 | 游戏过程中随时存档，设置密码保护 |
| 🤖 **AI 对战** | **两种算法可选：策略算法 / Alpha-Beta 剪枝** |
| ⚡ **Rust + Rayon 引擎** | **桌面端 Alpha-Beta 使用 Rust 实现，Rayon 多核并行搜索** |
| 🎛️ **搜索深度可调** | Alpha-Beta 深度 1~4 可配置 |
| 🔊 **音效系统** | 落子/爆炸/淘汰/获胜 音频反馈 |
| 📊 **结算统计** | 游戏结束后显示双折线图（棋子数/点数变化） |
| 📱 **响应式 UI** | 平板横屏 / 手机竖屏自动适配 |
| 🎨 **深色主题** | 高对比度配色，护眼舒适 |
| 🖥️ **桌面应用** | 基于 Tauri v2 的桌面客户端，Rust 后台 |
| 📱 **Android APK** | 支持 Android 设备原生运行 |

## 🚀 快速开始

### 方式一：Web 服务器

```bash
# 1. 创建虚拟环境
python3 -m venv venv
source venv/bin/activate

# 2. 安装依赖
pip install fastapi uvicorn websockets

# 3. 启动服务器
./start.sh
# 或者直接
python3 main.py

# 4. 打开浏览器访问
# http://localhost:8000
```

### 方式二：桌面应用（Tauri）

```bash
cd tauri
npm install
npx tauri dev          # 开发模式
cargo build --release  # 构建可执行文件
# 二进制文件：tauri/src-tauri/target/release/chain-chess
```

### 方式三：Android APK

```bash
cd tauri
npx tauri android init      # 初始化 Android 项目（仅首次）
npx tauri android build     # 构建 APK
```

预编译 APK 文件可在 [Releases](https://github.com/ywnh1/chain-chess/releases) 中下载。

## 🤖 AI 引擎

### 策略算法（Strategy）
基于规则的启发式搜索，速度快、消耗低：
1. **三级优先** — 优先引爆即将爆炸的棋子（count=3）
2. **安全升级** — 避开对手三级棋子的二级棋子
3. **一进二** / **下三级** — 逐步升级棋子等级
4. **首子中心** — 首回合只落在非边角位置，选最靠近棋盘中心

### Alpha-Beta 剪枝（Rust + Rayon 🦀）
桌面端专属的强力搜索算法：
- **Rust 实现** — 全部棋盘逻辑和搜索代码用 Rust 编写
- **Rayon 多线程** — 根节点走法并行搜索，充分利用多核 CPU
- **走法排序** — 三级棋子优先、周围对手多优先，提高剪枝效率
- **自适应限时** — 每步自动分配时间（3s + depth × 0.5s）
- **分支限制** — 每层最多搜索 top 10 个走法
- **深度可调** — 支持 1~4 层搜索（默认 2）

### AI 设置界面
- 算法选择：策略算法 / Alpha-Beta 剪枝
- 搜索深度滑块（Alpha-Beta 模式下可用）
- 思考中提示

## 🔊 音效

使用 Web Audio API 生成：
- **落子** — 短促高音
- **爆炸** — 低沉锯齿波
- **淘汰** — 下行音阶
- **获胜** — 上行琶音

## 📊 结算页面

游戏结束后自动显示：
- 🏆 获胜者（用颜色名称标识）
- 每位玩家的 **棋子数** / **点数** 统计卡片
- **折线图 1**：所有玩家棋子数变化趋势
- **折线图 2**：所有玩家点数（按等级加权）变化趋势

## 🐳 使用 start.sh

```bash
./start.sh          # 启动服务器
./start.sh stop     # 停止服务器
```

启动脚本会自动检查依赖、清理旧进程、轮询等待服务就绪，并可选启动 ngrok 外网穿透。

## 📁 项目结构

```
chain-chess/
├── main.py                   # FastAPI WebSocket 游戏服务器
├── run.py                    # 守护进程（崩溃自动重启）
├── start.sh                  # 启动/停止脚本
├── static/
│   └── index.html            # 网页版前端（含完整 AI + 音效 + 结算）
├── tauri/
│   ├── package.json
│   ├── public/
│   │   └── index.html        # Tauri 桌面/移动端前端
│   └── src-tauri/
│       ├── Cargo.toml        # Rust 项目配置（tauri + rayon）
│       ├── tauri.conf.json   # Tauri 配置
│       ├── capabilities/
│       │   └── default.json
│       └── src/
│           ├── main.rs       # 入口
│           └── lib.rs        # 🦀 Rust 游戏引擎（棋盘逻辑 + Alpha-Beta + Rayon）
├── release/                  # GitHub Release 构建产物
├── logs/                     # 运行时日志
└── venv/                     # Python 虚拟环境
```

## 🔧 技术栈

- **后端**：Python 3 + FastAPI + WebSocket + Uvicorn
- **前端**：纯 HTML/CSS/JavaScript 单页应用（Canvas 图表）
- **桌面端 AI 引擎**：Rust + Rayon 并行计算
- **桌面端**：Tauri v2
- **移动端**：Android APK（Tauri 构建）
- **通信协议**：JSON over WebSocket

## 📄 协议

本项目基于 [MIT License](LICENSE) 开源。

---

<div align="center">
  <sub>Built with ♟ by ywnh1 · v1.1.0</sub>
</div>
