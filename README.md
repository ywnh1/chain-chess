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
  </p>
</div>

---

**连锁棋** 是一款基于「爆裂棋 / Chain Reaction」玩法改进的多人策略棋盘游戏。支持 2~7 人在同一房间中轮流落子，棋子数量超过棋盘格容量时会发生连锁爆裂，扩散到相邻格子。融合淘汰机制、游戏存档与恢复、密码保护等完整功能。

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
| 📦 存档恢复 | 从存档恢复游戏，继续对战 |
| 🎨 响应式 UI | 移动端友好，深色主题设计 |
| 🖥️ 桌面应用 | 基于 Tauri v2 的桌面客户端 |
| 📱 Android APK | 支持 Android 设备原生运行 |

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
npx tauri dev     # 开发模式
npx tauri build   # 构建可执行文件
```

### 方式三：Android APK

```bash
# 使用 Tauri 构建（需要 Android SDK + NDK）
bash build_apk.sh

# 或使用简化版手动构建
bash build_apk_simple.sh
```

预编译 APK 文件可在 [Releases](https://github.com/ywnh1/chain-chess/releases) 中下载。

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
├── build_apk.sh              # Tauri APK 构建脚本
├── build_apk_simple.sh       # 手动 APK 构建脚本（简化版）
├── aapt2_arm64               # ARM64 AAPT2 工具（APK 构建用）
├── chainchess.apk            # 编译好的 APK
├── static/
│   └── index.html            # 前端页面（单页应用）
├── tauri/
│   ├── package.json
│   ├── public/
│   │   └── index.html        # Tauri 前端入口
│   └── src-tauri/
│       ├── Cargo.toml        # Rust 项目配置
│       ├── tauri.conf.json   # Tauri 配置
│       └── src/
│           ├── main.rs
│           └── lib.rs
├── release/                  # GitHub Release 构建产物
├── logs/                     # 运行时日志
└── venv/                     # Python 虚拟环境
```

## 🔧 技术栈

- **后端**：Python 3 + FastAPI + WebSocket + Uvicorn
- **前端**：纯 HTML/CSS/JavaScript 单页应用
- **桌面端**：Rust + Tauri v2
- **移动端**：Android APK（Tauri 构建）
- **通信协议**：JSON over WebSocket

## 📄 协议

本项目基于 [MIT License](LICENSE) 开源。

---

<div align="center">
  <sub>Built with ♟ by ywnh1</sub>
</div>
