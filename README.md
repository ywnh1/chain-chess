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
| 🎛️ **AI 算法快捷切换** | 策略/A-B/PVS/MCTS 一键切换，深度 +/- 微调（斗蛐蛐模式） |
| ⚡ **Rust + Rayon 引擎** | Alpha-Beta / PVS / MCTS 使用 Rust 实现，Rayon 多核并行搜索 |
| 🎯 **搜索深度可调** | Alpha-Beta / PVS / MCTS 深度 1~10 可配置 |
| 🎲 **MCTS 完整树搜索** | Selection → Expansion → Playout → Backpropagation 四步循环，UCB1 探索 |
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

连锁棋内置四款 AI 算法，覆盖三大搜索范式：**启发式规则**、**树搜索剪枝** 和 **随机模拟**。

| 算法 | 范式 | 引擎 | 适用场景 |
|------|------|------|----------|
| 策略算法 | 启发式规则 | JavaScript | 新手入门、低算力设备 |
| Alpha-Beta 剪枝 | 搜索剪枝 | Rust + Rayon | 日常对局、标准棋力 |
| PVS NegaMax | 精炼剪枝 | Rust + Rayon | 竞技对局、更强棋力 |
| MCTS 蒙特卡洛 | 随机模拟 | Rust + Rayon | 探索非直觉走法、特殊局面 |

### ⚡ 策略算法（Strategy）

纯 JavaScript 启发式规则引擎，响应最快、零依赖：

1. **三级优先** — 优先引爆即将爆炸的棋子（count = 3），触发连锁反应的起点
2. **安全升级** — 避开对手三级棋子附近的二级棋子，防止被吞噬
3. **一进二 / 下三级** — 逐步升级棋子等级，稳步扩大控制范围
4. **首子中心** — 首回合落在非边角位置，选最靠近棋盘中心的格子

> 适合新手入门。无需 Rust 引擎，所有平台即时响应。

### 🧠 Alpha-Beta 剪枝（`alpha_beta_pruning` crate 🦀）

标准 Alpha-Beta 剪枝搜索，Rust + Rayon 多核并行。采用**克隆式搜索**（每次走法复制棋盘而非 set/unset 撤销），天然适配连锁反应的多玩家轮换逻辑。

- **Rayon 多线程** — 根节点走法并行搜索，充分利用多核 CPU
- **走法排序** — 三级棋子优先、周围对手多优先，提高剪枝效率
- **分支限制** — 每层最多搜索 top 15 个走法
- **深度可调** — 支持 1~10 层搜索（默认 2）
- **随机化探索** — 早期对局加入随机扰动，增加走法多样性

### 🔬 PVS NegaMax + Killer/History + QSearch

Principal Variation Search（主变例搜索）是 Alpha-Beta 的精炼版本。核心优化：假设首走法即最优，后续走法用**零窗口（Null Window）**试探快速证明或证伪。

**增强优化：**
- **Killer 启发** — 每层记录 2 条杀招走法，后续搜索优先尝试
- **History 启发** — 全局历史表记录走法效果，同等局面排序更准
- **QSearch 静止搜索** — 搜索到底时只搜索爆炸性走法，避免水平线效应
- **Rayon 多线程** — 根节点走法并行

> 同等深度下比标准 Alpha-Beta 探索更高效，适合需要更强棋力的竞技场景。

### 🎲 MCTS 蒙特卡洛树搜索

与剪枝搜索截然不同的范式——**不依赖任何评估函数**，而是通过大量随机模拟来评估走法优劣。完整四步循环：

```
SELECT → EXPAND → PLAYOUT → BACKPROPAGATE
  │         │         │            │
  │    UCB1 平衡    │    随机走子    │
  │    探索与利用    │    直到终局    │
  │         │         │            │
  └─────────┴─────────┴────────────┘
          反向传播更新统计
```

**技术细节：**
- **UCB1 公式** — `w/n + C·√(ln(N)/n)`，C=1.414 平衡探索与利用
- **完整树搜索** — 每棵子树独立存储棋盘状态和 eliminated 列表
- **优先展开** — 使用 `order_moves` 排序未试走法，最有希望的先展开
- **树节点上限** — 3000 节点防内存暴涨
- **Rayon 根级并行** — 每个候选走法独立建树，均分迭代预算
- **深度 1~10** — 对应 1200~12000 次完整迭代

> MCTS 能发现 Alpha-Beta 不易察觉的非直觉走法，对连锁爆裂等混沌局面有独特应对能力。探索性强，走法多样，不会陷入固定套路。

### AI 设置界面

- **PVE 模式** — 算法选择：策略算法 / Alpha-Beta 剪枝 / PVS NegaMax / MCTS
- **搜索深度** — Alpha-Beta / PVS / MCTS 支持 1~10 深度滑块，策略算法无深度概念
- **颜色选择** — 玩家可在 PVE 模式中选择自己的棋盘颜色
- **AI 斗蛐蛐（AI Battle Royale）** — 2~10 个 AI 对战，每个 AI 独立配置算法和深度

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
│           └── lib.rs         # 🦀 游戏引擎（棋盘逻辑 + AlphaBeta + PVS + MCTS）
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
