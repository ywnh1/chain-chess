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
  </p>
</div>

---

**连锁棋** 是一款基于「爆裂棋 / Chain Reaction」玩法改进的多人策略棋盘游戏。支持本地多人和 AI 对战，棋子数量超过棋盘格容量时会发生连锁爆裂，扩散到相邻格子。

当前版本为纯客户端应用（Tauri + 纯前端），无需服务器。

## 🎮 游戏规则

1. **落子** — 点击空位落子（每位玩家首步需避开所有已有棋子周围 12 格），点击自己的棋子加子
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
| 🧭 **Router 页面管理** | 7 页统一路由、生命周期钩子、智能回退导航 |
| 🖥️ **CheckOutPage** | 专属结算/历史详情页面，替代动态 overlay |
| 🎛️ **AI 算法快捷切换** | 策略/A-B 一键切换，深度 +/- 微调（斗蛐蛐模式） |
| ⚡ **Rust + Rayon 引擎** | Alpha-Beta 使用 Rust 实现，Rayon 多核并行搜索 |
| 🎯 **搜索深度可调** | Alpha-Beta 深度 1~10 可配置 |
| 🎲 **随机化走法** | 早期对局加入随机探索，同等走法随机选择 |
| 🎯 **第一轮限制区域** | 每位玩家首步需避开所有已有棋子周围 12 格 |
| ⏸ **暂停功能** | 游戏暂停时查看实时统计和折线图 |
| 🔊 **音效系统** | 落子/爆炸/淘汰/获胜 音频反馈（Web Audio API 合成） |
| 📊 **结算统计** | 游戏结束后展示双折线图（棋子数/点数变化） |
| 📱 **响应式 UI** | 平板横屏 / 手机竖屏自动适配 |
| 🎨 **深色主题 + 液态玻璃 UI** | 柔和光泽动画，流畅按键过渡，护眼舒适 |
| 🖥️ **桌面应用** | 基于 Tauri v2 的桌面客户端，Rust 后台 |
| 📱 **Android APK** | 支持 Android 设备原生运行 |
| 📋 **历史记录** | 自动保存对局记录，可回看统计图表 |
| 🧠 **内存+磁盘两层存储** | 历史超过 500 步自动溢出到磁盘，防内存暴涨 |
| 🔍 **图表全屏缩放** | 点击图表全屏查看，支持双指缩放 / 鼠标滚轮 |
| 🎯 **核心逻辑 Rust 加速** | 落子处理走 Rust 通道（Tauri invoke），回退 JS 引擎 |
| ⏭️ **连锁动画跳过** | 长连爆可一键跳过动画，提升体验 |
| 🖱️ **UI 动效打磨** | 按钮按压反馈、select 自定义箭头、挖孔屏安全区适配 |

## 🚀 快速开始

### 桌面应用（Tauri）

```bash
cd tauri
npm install
npx tauri dev          # 开发模式
npx tauri build        # 构建可执行文件
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

### Alpha-Beta 剪枝（Rust + `alpha_beta_pruning` crate 🦀）

桌面 & Android 端均可使用的强力搜索算法：

- **Rust 实现** — 全部棋盘逻辑和搜索代码用 Rust 编写，极致性能
- **Rayon 多线程** — 根节点走法并行搜索，充分利用多核 CPU
- **走法排序** — 三级棋子优先、周围对手多优先，提高剪枝效率
- **分支限制** — 每层最多搜索 top 10 个走法
- **深度可调** — 支持 1~10 层搜索（默认 2）
- **随机化探索** — 早期对局加入随机扰动，增加走法多样性
- **`alpha_beta_pruning` crate** — 使用标准 AlphaBeta trait 实现，克隆式搜索适配连锁反应

### AI 设置界面

- **PVE 模式** — 算法选择：策略算法 / Alpha-Beta 剪枝；搜索深度滑块（1~10）；用户可选择自己的颜色
- **AI 斗蛐蛐模式** — 支持 2~10 个 AI 对战，每个 AI 独立配置算法和深度；策略算法不显示深度选项

## 🎨 UI 设计

- **深色主题** — 低对比度深色背景，护眼舒适
- **液态玻璃光泽** — 卡片、按钮的柔和扫光动画，12s 周期避免闪烁
- **流畅过渡** — 按钮 `:active` 状态使用 `transition`，消除生硬跳跃
- **响应式布局** — 平板横屏 / 手机竖屏自动适配
- **自定义 select 箭头** — 统一下拉选择器视觉风格，支持深色主题选项
- **挖孔屏安全区适配** — `env(safe-area-inset-top)` 适配刘海屏/挖孔屏

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

- 🏆 **获胜者** — 用颜色名称标识
- 📊 **统计卡片** — 每位玩家展示棋子数 / 点数 / 连爆统计
- 📈 **折线图** — 所有玩家棋子数 / 点数变化趋势（支持全屏缩放）
- 🔥 **最高连爆** — 本局最长连锁爆裂记录
- 📋 **历史记录** — 自动保存对局记录，可回看统计图表

## 🧠 内存管理

- **内存+磁盘两层存储** — 游戏过程中历史数据保存在内存；超过 500 步自动溢出到 `round_data.json`
- **暂停/结算/查看历史时** — 全部数据刷入磁盘，再从磁盘读取生成完整图表
- **统一渲染** — 暂停、结算、历史记录 checkout 复用同一个 `renderGameCharts()` 函数
- **历史记录持久化** — 对局完成后自动保存到 Tauri 后端，支持查看回放统计

## 📁 项目结构

```
chain-chess/
├── build_apk.sh               # Android APK 一键构建 + 签名脚本
├── tauri/
│   ├── public/
│   │   └── index.html         # 前端（完整游戏 UI + AI + 音效，2500+ 行单页）
│   └── src-tauri/
│       ├── Cargo.toml         # Rust 依赖（tauri + rayon + alpha_beta_pruning + serde）
│       ├── gen/               # Android 平台生成代码
│       └── src/
│           └── lib.rs         # 🦀 游戏引擎（棋盘逻辑 + GameState + AlphaBeta trait）
├── LICENSE                    # MIT 开源协议
└── README.md                  # 本文件
```

## 🔧 技术栈

| 层级 | 技术 |
|------|------|
| 🌐 前端 | 纯 HTML / CSS / JavaScript 单页应用（Canvas 绘制棋盘） |
| 🦀 AI 引擎 | Rust + `alpha_beta_pruning` crate（AlphaBeta trait + Rayon 并行） |
| 🖥️ 桌面框架 | Tauri v2 + 系统 WebView |
| 📱 移动端 | Android APK（Tauri 构建） |

## 📄 开源协议

本项目基于 **MIT License** 开源 — 详见 [LICENSE](LICENSE) 文件。

Copyright (c) 2026 ywnh1

---

<div align="center">
  <sub>Built with ♟ by <a href="https://github.com/nihao15900375400">ywnh1</a> · v2.0.0-beta</sub>
</div>
