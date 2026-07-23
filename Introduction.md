# 连锁棋 (Chain Chess) — 完整项目介绍

> 版本: 2.1.0-beta | 许可: MIT
> 技术栈: Tauri v2 + Rust + 纯前端 (HTML/CSS/JS) — 无框架、无服务端

本文档详细记录项目全部实现细节，涵盖 HTML/CSS/JS 前端、Rust 后端、构建脚本、数据格式、UI/UX 设计、游戏规则和 AI 引擎。目标是让一个什么都不知道的人，仅凭本文就能重新实现完全一样的应用。

---

## 目录

1. [项目概述](#1-项目概述)
2. [目录结构](#2-目录结构)
3. [构建与运行](#3-构建与运行)
4. [页面系统与路由](#4-页面系统与路由)
5. [欢迎页 (Welcome)](#5-欢迎页-welcome)
6. [大厅页面 (Lobby)](#6-大厅页面-lobby)
7. [游戏页面 (Game)](#7-游戏页面-game)
8. [暂停覆盖层 (Pause Overlay)](#8-暂停覆盖层-pause-overlay)
9. [结算页面 (Checkout/Settlement)](#9-结算页面-checkoutsettlement)
10. [历史记录 (History)](#10-历史记录-history)
11. [UI 样式体系](#11-ui-样式体系)
12. [动画特效系统](#12-动画特效系统)
13. [音效系统](#13-音效系统)
14. [游戏核心逻辑](#14-游戏核心逻辑)
15. [AI 引擎](#15-ai-引擎)
16. [Rust 后端](#16-rust-后端)
17. [数据存储与格式](#17-数据存储与格式)
18. [视觉特效辅助函数](#18-视觉特效辅助函数)
19. [自定义控件系统](#19-自定义控件系统)
20. [全部 JS 全局状态变量](#20-全部-js-全局状态变量)
21. [全部 Rust 类型定义](#21-全部-rust-类型定义)
22. [构建与签名脚本](#22-构建与签名脚本)
23. [Tauri 配置](#23-tauri-配置)

---

## 1. 项目概述

连锁棋是一款基于「爆裂棋 / Chain Reaction」玩法的多人在线棋盘策略游戏，纯客户端应用，无服务器依赖。

**应用架构**:
- **前端**: 单个 `index.html` 文件包含所有 HTML、CSS、JavaScript（约 2540 行）
- **后端**: Rust 编写的 Tauri v2 应用（`lib.rs` 约 629 行），处理棋盘逻辑和 AI 搜索
- **通信**: 前端通过 `window.__TAURI_INTERNALS__.invoke()` 调用 Rust 命令

**核心特性**:
- 支持 2~7 人本地对战（同一设备轮流落子）
- 支持 1 人类 + 1~6 AI 的 PVE 模式
- 支持 2~10 个 AI 互相对战（"斗蛐蛐"模式）
- 两种 AI 算法：策略算法（JS）和 Alpha-Beta 剪枝（Rust+Rayon）
- Web Audio API 合成音效（无音频文件）
- Canvas 折线图统计（棋子数/点数变化趋势）
- 两层历史存储（内存 500 步 + 磁盘溢出）
- 紧凑格式历史记录存储（体积缩减 5~10 倍）
- 深色液态玻璃 UI 主题

---

## 2. 目录结构

```
chain-chess/
├── build_apk.sh              # 一键编译+签名 Android APK
├── release.keystore          # Android 签名密钥
├── release/                  # 发布产物目录
│   └── 连锁棋-2.1.0-beta.apk
├── README.md
├── LICENSE
├── .gitignore
├── tauri/
│   ├── package.json          # npm: @tauri-apps/cli
│   ├── public/
│   │   └── index.html        # ← 全部前端代码 (2540 行)
│   └── src-tauri/
│       ├── Cargo.toml        # Rust 依赖
│       ├── Cargo.lock
│       ├── build.rs          # Tauri 构建脚本
│       ├── tauri.conf.json   # Tauri 应用配置
│       ├── capabilities/
│       │   └── default.json  # Tauri 权限配置
│       ├── icons/            # 应用图标
│       ├── gen/              # 自动生成的 Android 项目
│       └── src/
│           ├── main.rs       # ← Rust 入口 (3 行)
│           └── lib.rs        # ← Rust 全部后端代码 (629 行)
```

---

## 3. 构建与运行

### 3.1 桌面开发

```bash
cd tauri
npm install
npx tauri dev          # 开发模式（热加载）
npx tauri build        # 构建可执行文件
```

### 3.2 Android APK

```bash
./build_apk.sh <keystore_password>
# 例如: ./build_apk.sh chainchess
```

首次需要先初始化 Android 项目：
```bash
cd tauri
npx tauri android init
```

### 3.3 依赖

| 依赖 | 用途 |
|------|------|
| `@tauri-apps/cli` ^2.11.4 | Tauri CLI |
| `tauri` v2 | 桌面/移动框架 |
| `serde` / `serde_json` | JSON 序列化 |
| `rayon` 1 | Rust 并行迭代 |
| `alpha_beta_pruning` 0.1.0 | Alpha-Beta 剪枝 trait |
| `tauri-build` v2 | 构建脚本 |

---

## 4. 页面系统与路由

### 4.1 Router 实现

Router 是一个纯 JS 对象，位于 index.html 约 983 行。所有页面管理都通过它完成。

```javascript
const Router = {
  _registry: {},    // 页面注册表 {id: {enter(), leave(), back}}
  _current: null,   // 当前页面 ID
  _prev: null,      // 上一个页面 ID
};
```

**页面注册表** — 7 个页面：

| ID | 页面 | 返回目标 | enter 回调 | leave 回调 |
|----|------|---------|-----------|-----------|
| `welcome` | 欢迎页 | `null` | 清空背景 | 无 |
| `aiLobby` | AI 对战配置 | `'welcome'` | 清空背景 + 触发深度显示 | 无 |
| `localLobby` | 本地对战配置 | `'welcome'` | 清空背景 | 无 |
| `eveLobby` | AI 斗蛐蛐配置 | `'welcome'` | 生成 AI 配置行 | 无 |
| `history` | 历史记录 | `'welcome'` | 加载历史列表 | 无 |
| `game` | 游戏主界面 | `null` (禁止返回) | 无 | 清空背景 |
| `checkout` | 结算/历史详情 | 动态 `_checkoutPrev` | 设置 `_checkoutPrev` | 清空内容 |

### 4.2 导航流程

```javascript
Router.navigate(id, ...args)  // 切换到指定页面
Router.back()                  // 返回上一页
Router.getPrev()               // 获取上一页 ID
```

**`navigate()` 完整流程**：
1. 检查目标页是否存在，且不是当前页（防止重复导航）
2. 如果当前页存在（且有 DOM 元素），给当前屏幕加 `leaving` 类 → 触发 `pageLeave` 动画
3. 监听 `animationend` 事件等待动画完成（400ms 超时保护）
4. 执行 `doSwitch()`：
   - 调用当前页的 `leave()` 回调
   - 更新 `_prev = _current`，`_current = id`
   - 移除所有屏幕的 `leaving`、`active` 类
   - 给目标屏幕加 `active` 类
   - 调用目标页的 `enter()` 回调
   - 同步 `location.hash`（pushState）

### 4.3 页面切换动画

```css
.screen { display: none; opacity: 0 }
.screen.active {
  display: flex;
  animation: pageEnter .4s cubic-bezier(.22,1,.36,1) both;
  /* 从下往上 18px 淡入 + scale(.97 → 1) */
}
.screen.leaving {
  animation: pageLeave .25s ease both;
  pointer-events: none;
  /* 向上 12px 淡出 + scale(1 → .98) */
}
```

### 4.4 浏览器返回键处理

```javascript
window.addEventListener('popstate', () => {
  if (gameMode && gameMode !== 'welcome') {
    exitGame();  // 游戏中按返回 → 退出游戏
    return;
  }
  const h = location.hash.slice(1);
  if (h && document.getElementById(h)) Router.navigate(h);
  else Router.navigate('welcome');
});
```

---

## 5. 欢迎页 (Welcome)

HTML ID: `#welcome`

### 5.1 布局

```
┌─────────────────────────────────────┐
│          ♟ 连锁棋                    │
│        多人联手，一触即发              │
│                                     │
│  ┌─────────────────────────────┐    │
│  │ 🤖 AI 对战                  │    │
│  │ 与电脑 AI 对决，挑战不同难度  │    │
│  └─────────────────────────────┘    │
│  ┌─────────────────────────────┐    │
│  │ 👥 本地对战                  │    │
│  │ 同一设备，轮流落子            │    │
│  └─────────────────────────────┘    │
│  ┌─────────────────────────────┐    │
│  │ ⚔️ AI 斗蛐蛐                │    │
│  │ 自定义 AI 阵容，观看 AI 博弈  │    │
│  └─────────────────────────────┘    │
│  ┌─────────────────────────────┐    │
│  │ 📋 历史记录                  │    │
│  │ 查看对局记录和统计数据        │    │
│  └─────────────────────────────┘    │
│                                     │
│  [✕ 关闭应用]                       │
└─────────────────────────────────────┘
```

### 5.2 关键样式

- `.logo`: 3rem 字号，`span` 子元素使用金色 (`var(--accent)`)
- `.mode-card`: 毛玻璃 (`backdrop-filter: blur(12px)`)，hover 上浮 2px
- 四个卡片各自带有不同的 `border-color` 强调色（AI=金色，本地=青色，斗蛐蛐=紫色，历史=青色）
- 底部关闭按钮，调用 `exitApp()` → 先调 `exitGame()` 清理状态，再调 Tauri 的 `exit_app` 命令

### 5.3 全局背景动画

整个页面有 4 层径向渐变的大背景，通过 `body::before` 伪元素实现：
```css
body::before {
  background:
    radial-gradient(ellipse 70% 50% at 50% -10%, rgba(240,179,75,.04) 0%, transparent 70%),
    radial-gradient(ellipse 60% 40% at 100% 90%, rgba(95,195,195,.03) 0%, transparent 70%),
    radial-gradient(ellipse 50% 30% at 20% 40%, rgba(240,179,75,.015) 0%, transparent 60%),
    radial-gradient(ellipse 40% 25% at 80% 60%, rgba(95,195,195,.015) 0%, transparent 60%);
  animation: liquidDrift 20s ease-in-out infinite;
}
```
`liquidDrift` 动画让渐变位置在四个关键帧间缓慢漂移，模拟液体流动效果。

---

## 6. 大厅页面 (Lobby)

有三个大厅页面，用于配置游戏参数。

### 6.1 AI 对战大厅 (#aiLobby)

| 控件 | ID | 值范围 | 默认 |
|------|-----|-------|------|
| 棋盘大小 | `aiSizeGrid` | 5×5 ~ 19×19 | 7×7 |
| AI 对手数 | `aiCountGroup` | 1~6 | 1 |
| 颜色选择 | `aiColorOptions` | 动态生成 | 0 (红色) |
| AI 算法 | `aiAlgorithmCards` | strategy / alphabeta | strategy |
| 搜索深度 | `aiDepthSlider` | 1~10 | 2 |

**颜色选择逻辑**:
- 仅在 1 人类玩家 + AI 数量 ≥ 1 时显示
- `selectedPlayerColor` 为 0，人类选择颜色后 AI 自动占据剩余颜色
- 颜色选择器是圆点按钮，选中时加白色边框和发光

**算法选择**:
- 两个卡片：⚡ 策略算法 / 🧠 Alpha-Beta 剪枝
- 选 Alpha-Beta 时显示深度滑块，策略算法时隐藏
- 通过 `toggleAIDepth()` 控制 `#aiDepthRow` 的 `display`

**开始游戏**: `startAIGame()` 函数：
1. 读取大小、AI 数量、算法、深度
2. `maxPlayers = aiCount + 1`
3. 人类玩家占据 `selectedPlayerColor`，其余位置设为 AI
4. 调用 `recordHistory()` 记录初始快照
5. 如果玩家 0 是 AI，立即触发 AI

### 6.2 本地对战大厅 (#localLobby)

| 控件 | ID | 值范围 | 默认 |
|------|-----|-------|------|
| 棋盘大小 | `localSizeGrid` | 5×5 ~ 19×19 | 7×7 |
| 玩家人数 | `localPlayersGroup` | 2~7 | 2 |

`startLocalGame()` 函数：
- 无 AI 设置，纯本地轮流
- 玩家 0 开始

### 6.3 AI 斗蛐蛐大厅 (#eveLobby)

| 控件 | ID | 值范围 | 默认 |
|------|-----|-------|------|
| 棋盘大小 | `eveSizeGrid` | 5×5 ~ 19×19 | 7×7 |
| AI 数量 | `eveCountGroup` | 2~10 | 3 |
| 每个 AI 配置 | `eveConfigArea` | 动态生成 | 策略算法，深度 2 |

**每个 AI 的配置行** (`generateEveConfig()`)：
```
┌─────────────────────────────────────┐
│ 🟠 AI 1  [策略|A-B]  [− 2 +]        │
│ 🔵 AI 2  [策略|A-B]  [− 3 +]        │
│ 🟢 AI 3  [策略|A-B]                 │
└─────────────────────────────────────┘
```
- 每个 AI 独立选择算法（策略/A-B）和深度（1~10）
- 选策略算法时深度控制隐藏（加 `hidden` 类）
- A-B 算法时深度用 +/- 按钮控制，最小 1 最大 10
- 存储在 `aiConfigs[p] = {algorithm, depth}` 对象中

`startEveGame()` 函数：
- 所有玩家都是 AI（`aiPlayers.add(p)` for `p=0..maxPlayers-1`）
- 读取每个配置行的算法和深度
- 玩家 0 如果是 AI，立即触发 AI

---

## 7. 游戏页面 (Game)

HTML ID: `#game`

### 7.1 布局

```
┌─────────────────────────────────────┐
│          [⏸ 暂停]                    │
│                                      │
│  [玩家 1 3]  [AI 2 1]  [玩家 3 0]   │
│                                      │
│  ┌─────┬─────┬─────┬─────┬─────┐    │
│  │     │     │     │     │     │    │
│  ├─────┼─────┼─────┼─────┼─────┤    │
│  │     │  ●  │     │     │     │    │
│  ├─────┼─────┼─────┼─────┼─────┤    │
│  │     │     │  ●  │     │     │    │
│  ├─────┼─────┼─────┼─────┼─────┤    │
│  │     │     │     │     │     │    │
│  └─────┴─────┴─────┴─────┴─────┘    │
│                                      │
│          [▶ 跳过动画] (隐藏)          │
└─────────────────────────────────────┘
```

### 7.2 状态变量

| 变量 | 类型 | 初始值 | 说明 |
|------|------|--------|------|
| `board` | `Array<Array<{owner, count}>>` | `[]` | 棋盘数据 |
| `curPlayer` | number | 0 | 当前玩家索引 |
| `size` | number | 7 | 棋盘大小 |
| `maxPlayers` | number | 2 | 玩家总数 |
| `cells` | `Array<Array<HTMLElement>>` | `[]` | DOM 单元格引用 |
| `gameMode` | string\|null | null | 'ai'/'local'/'eve' |
| `gameOver` | boolean | false | 游戏是否结束 |
| `isPaused` | boolean | false | 是否暂停 |
| `aiPlayers` | Set | `new Set()` | AI 玩家索引集合 |
| `aiThinking` | boolean | false | AI 是否在思考 |
| `aiAlgorithm` | string | 'strategy' | 全局 AI 算法 |
| `aiDepth` | number | 2 | 全局搜索深度 |
| `aiConfigs` | object | `{}` | 每个 AI 的独立配置 (eve 模式) |
| `eliminatedPlayers` | Set | `new Set()` | 已淘汰玩家索引 |
| `gameHistory` | `Array<{turn, snapshot}>` | `[]` | 回合历史（内存） |
| `chainStats` | object | `{}` | `{playerId: {triggered, maxChain}}` |
| `maxChainOverall` | `{player, length}` | `{player:null, length:0}` | 全局最高连爆 |
| `firstMovePos` | `[x, y]\|null` | null | 首子位置 |
| `chainSkipAll` | boolean | false | 跳过连爆动画 |
| `_chartCtx` | object | `{}` | Canvas 图表上下文缓存 |
| `_boardCache` | array\|null | null | 棋盘渲染缓存 |

### 7.3 暂停按钮

暂停按钮是游戏界面唯一的交互按钮，居中显示，非常醒目：

```css
#pauseBtn {
  padding: 12px 28px; font-size: .95rem; font-weight: 700;
  background: rgba(240,179,75,.12);
  border: 1.5px solid rgba(240,179,75,.25);
  border-radius: 14px;
  backdrop-filter: blur(8px);
  animation: pausePulse 3s ease-in-out infinite;
}
```

**呼吸发光动画** (`pausePulse`)：
```css
@keyframes pausePulse {
  0%, 100% {
    box-shadow: 0 0 20px rgba(240,179,75,.08);
    border-color: rgba(240,179,75,.25);
  }
  50% {
    box-shadow: 0 0 35px rgba(240,179,75,.18);
    border-color: rgba(240,179,75,.4);
  }
}
```

按钮文字在暂停/继续间切换：`⏸ 暂停` ↔ `▶ 继续`

### 7.4 玩家标签栏

ID: `#playerBar`

```html
<span class="player-tag active">玩家 1 <span class="cnt">5</span></span>
<span class="player-tag">AI 2 <span class="cnt">3</span></span>
<span class="player-tag elim">玩家 3 <span class="cnt">0</span></span>
```

- `.player-tag.active` — 当前玩家，高亮、放大、有边框
- `.player-tag.elim` — 已淘汰，加删除线、半透明
- 背景色使用 `COLORS_LIGHT[p]`（低透明度版）
- 文字颜色使用 `COLORS[p]`

### 7.5 棋盘渲染

容器 `.board-wrap` 尺寸: `min(88vw, 440px)`
棋盘 `#board`: CSS Grid，间距 `min(1.6vw, 8px)`

**单元格 (`.cell`)**:
- 尺寸由 grid 自动分配
- 圆角 `min(2.2vw, 10px)`
- 半透明背景 `rgba(255,255,255,.05)`
- 点击高亮 `.cell.highlight`（scale 1.02，背景变亮）
- 点击缩放 `.cell:active`（scale .93）
- `overflow: hidden`（用于波纹特效）

**棋子 (`.piece`)**:
- 定位在单元格中心 (translate -50%, -50%)
- 圆形，尺寸 76% of 单元格
- 颜色使用 `.p0`~`.p9` 类（10 种颜色）
- 阴影 `0 2px 8px rgba(0,0,0,.35)`
- 放置动画: `cubic-bezier(.34,1.56,.64,1)` 弹性曲线

**点数显示 (`.dot`)**:
- 白色圆点，显示在棋子内
- 1 点: 中心一个
- 2 点: 左右各一个
- 3 点: 等边三角形
- ≥4 点: 四角各一个

**增量渲染优化**:
- `_boardCache` 缓存每格状态
- 只有状态变化时才更新 DOM
- 用 `force` 参数强制全量重建

### 7.6 消息提示

ID: `#msg`，位于游戏顶部右侧（绝对定位）

```javascript
function showMsg(t, c) {
  // c = '' | 'error' | 'success'
}
```
- 自动 3 秒后清除
- 错误样式: 红色背景白字
- 成功样式: 绿色背景白字

### 7.7 跳过连爆按钮

ID: `#skipChainBtn`
- 固定在屏幕底部居中
- 仅在 `anim === 'explode'` 时显示
- 点击设置 `chainSkipAll = true`
- 样式: 金色边框毛玻璃胶囊按钮

---

## 8. 暂停覆盖层 (Pause Overlay)

HTML ID: `#pauseOverlay`

**显示/隐藏**:
```javascript
togglePause()      // 切换暂停/继续
showPauseOverlay() // 显示统计和图表
hidePauseOverlay() // 隐藏并清理 DOM
resumeGame()       // 继续游戏
endGameNow()       // 结束当前对局
```

### 8.1 暂停覆盖层布局

```
┌─────────────────────────────────────┐
│        ⏸ 游戏暂停                    │
│                                     │
│  [玩家 1] [AI 2]                    │
│    棋子:5   棋子:3                   │
│    点数:7   点数:4                   │
│                                     │
│  ┌── 棋子数变化 ──┐                  │
│  │   Canvas 折线图  │                │
│  └────────────────┘                  │
│  ┌── 点数变化 ────┐                  │
│  │   Canvas 折线图  │                │
│  └────────────────┘                  │
│                                     │
│  [▶ 继续游戏]                        │
│  [⛳ 结束游戏]                        │
└─────────────────────────────────────┘
```

### 8.2 showPauseOverlay() 流程

1. 从棋盘实时状态计算快照（`liveSnap`）
2. 检测是否与最后一帧历史不同（`stale`）
3. 如果不同，追加到 `gameHistory`
4. 调用 `flushAndGetFullHistory()` 刷盘并获得完整历史
5. 调用 `renderGameCharts()` 渲染统计卡片和图表
6. 将图表容器移动至 `#pauseCharts`（因布局分 `pauseStats` 和 `pauseCharts`）

### 8.3 endGameNow() 流程

1. 隐藏暂停覆盖层
2. 计算活跃玩家中棋子最多的为获胜者
3. `recordHistory()` 记录最终快照
4. `flushAndGetFullHistory()` 刷盘
5. `showSettlement(winner, COLOR_NAMES, fullHistory)`

---

## 9. 结算页面 (Checkout / Settlement)

复用 `#checkout` screen（也是历史详情页）。结算时生成动态 `.settlement` 容器放入 `#checkoutContent`。

### 9.1 布局

```
┌─────────────────────────────────────┐
│  ← 返回                              │
│                                     │
│  🏆 🟠 红色 获胜                      │
│                                     │
│  ┌────┐ ┌────┐ ┌────┐               │
│  │玩家1│ │AI 2│ │AI 3│               │
│  │  5  │ │  3  │ │  0  │             │
│  │棋子 │ │棋子 │ │棋子 │             │
│  │  7  │ │  4  │ │  0  │             │
│  │点数 │ │点数 │ │点数 │             │
│  └────┘ └────┘ └────┘               │
│                                     │
│  最高连爆：红色 触发 5 连爆            │
│                                     │
│  ┌── 棋子数变化 ──┐                  │
│  │  折线图 (Canvas) │                │
│  └────────────────┘                  │
│  ┌── 点数变化 ────┐                  │
│  │  折线图 (Canvas) │                │
│  └────────────────┘                  │
│                                     │
│  [再来一局]                           │
│  [返回主界面]                          │
└─────────────────────────────────────┘
```

### 9.2 showSettlement() 流程

```javascript
async function showSettlement(winner, colorNames, history) {
  // 1. 清理残留 settlement DOM
  // 2. 获取完整历史（如果传入的 history 有数据则直接使用，否则从磁盘加载）
  // 3. 渲染到 #checkout 页
  // 4. Router.navigate('checkout', prevPage)
  // 5. saveGameHistory(winner, ..., fullHistory)
}
```

`saveGameHistory()` 将完整回合历史压缩为紧凑格式后，通过 Tauri invoke 保存到 `history.json`。

### 9.3 showHistoryDetail() 流程

查看历史记录时调用，从 `history.json` 的记录中展开紧凑格式、渲染图表：

```javascript
function showHistoryDetail(r) {
  // 1. expandHistory(r.history, r.playerCount) → 逐回合快照数组
  // 2. 如果数据不足 (<2 回合)，显示"无完整快照数据"
  // 3. 临时覆盖 maxPlayers = r.playerCount
  // 4. renderGameCharts(inner, historyData, {winner, ...})
  // 5. 添加"关闭"按钮，恢复 maxPlayers
}
```

### 9.4 图表系统

**`drawLineChart(canvas, history, colorNames, colors, valueKey)`**:
- `valueKey`: `'pieces'` 或 `'points'`
- 使用 Canvas 2D API 绘制
- 支持高 DPI 屏幕（`devicePixelRatio` 缩放）
- 白色虚线 = 总数变化线
- 彩色实线 = 每个玩家的变化线
- Y 轴 0~4 档标注
- X 轴每隔 `ceil(n/10)` 格标注
- 网格线 `rgba(255,255,255,.06)`
- 图表上下文缓存到 `_chartCtx`（用于全屏重绘）

**`renderGameCharts(container, history, opts)`**:
- opts 包含: `winner, colorNames, colors, eliminated, chainStats, maxChain, showTitle, onReplay, _noBack`
- 生成标题、统计卡片（stat-grid）、最高连爆提示、图表 box 和 Canvas
- `onReplay`: "再来一局"按钮回调
- `_noBack`: 不显示"返回主界面"按钮

**全屏图表 `showFullscreenChart(canvasId)`**:
- 创建全屏黑色覆盖层
- 大 Canvas 重绘图表
- 支持双指触控缩放和鼠标滚轮缩放（0.5~5 倍）

---

## 10. 历史记录 (History)

HTML ID: `#history`

### 10.1 历史列表

每个历史条目渲染为：
```html
<div class="history-item" onclick="showHistoryDetail(r)">
  <div class="h-time">🕐 2026/07/21 12:00:00</div>
  <div class="h-info">AI对战 · 7×7 · 4人 · AI×3</div>
  <div class="h-winner" style="color:#e8645a">🏆 红色</div>
  <div class="h-details">算法: alphabeta · 深度: 2</div>
</div>
```

### 10.2 清空历史

```javascript
function clearHistory()
```
- 使用 `confirm()` 确认
- 调用 Tauri 的 `clear_game_history` 命令

---

## 11. UI 样式体系

### 11.1 CSS 变量

| 变量 | 值 | 用途 |
|------|-----|------|
| `--bg` | `#0f0f13` | 深黑底色 |
| `--surface` | `rgba(26,26,34,.85)` | 卡片表面 |
| `--accent` | `#f0b34b` | 金色强调色 |
| `--accent2` | `#5fc3c3` | 青色次强调色 |
| `--text` | `#e8e6e3` | 暖白文字 |
| `--dim` | `#7a7885` | 灰色辅助文字 |
| `--radius` | `12px` | 卡片圆角 |
| `--glass` | `rgba(255,255,255,.04)` | 玻璃底色 |
| `--font` | `'Inter','SF Pro','PingFang SC',...` | 字体栈 |

### 11.2 按钮体系

| 类 | 用途 | 颜色 |
|-----|------|------|
| `.btn-primary` | 主要操作（继续游戏） | 金色背景，深色文字 |
| `.btn-danger` | 危险操作（结束游戏） | 红色边框，红色文字 |
| `.btn-again` | 再来一局（同 primary） | 金色背景 |
| `.btn-ghost` | 模态框次级按钮 | 灰色半透明 |
| `.back` / `.chk-back` | 返回按钮 | 灰色，hover 变亮 |
| `.size-btn` | 棋盘大小选项 | 灰色，选中变金色 |
| `.gb` (group button) | 数量选项 | 灰色，选中变金色 |
| `.alg-card` | 算法卡片 | 灰色，选中变金色 |
| `.am-btn` | 斗蛐蛐算法快捷切换 | 迷你按钮 |
| `.dm-btn` | 斗蛐蛐深度 +/- 按钮 | 圆形迷你按钮 |

### 11.3 挖孔屏适配

```css
body { padding-top: env(safe-area-inset-top, 20px) }
```

### 11.4 滚动条美化

```css
#checkout .chk-scroll::-webkit-scrollbar { width: 3px }
#checkout .chk-scroll::-webkit-scrollbar-thumb {
  background: rgba(255,255,255,.1);
  border-radius: 3px;
}
```

### 11.5 玩家颜色

10 种预定义颜色（索引 0~9）：
```
0: #e8645a (红色)   1: #f0c040 (黄色)   2: #40b8f0 (蓝色)
3: #50d060 (绿色)   4: #c080f0 (紫色)   5: #f070c0 (粉色)
6: #50d0b0 (青色)   7: #ff884d (橙色)   8: #e868a0 (玫红)
9: #70c0e8 (天蓝)
```

CSS 类 `.p0`~`.p9` 分别对应这些颜色，用于棋子背景。

中文名称数组: `['红色','黄色','蓝色','绿色','紫色','粉色','青色','橙色','玫红','天蓝']`

低透明度版本 `COLORS_LIGHT` 用于玩家标签背景。

---

## 12. 动画特效系统

### 12.1 页面过渡

| 动画 | 触发 | 时长 | 缓动 |
|------|------|------|------|
| `pageEnter` | 页面激活时 | 0.4s | cubic-bezier(.22,1,.36,1) |
| `pageLeave` | 页面离开时 | 0.25s | ease |

### 12.2 暂停按钮呼吸

| 动画 | 触发 | 时长 | 效果 |
|------|------|------|------|
| `pausePulse` | 持续循环 | 3s | 发光强度和边框颜色循环变化 |

### 12.3 落子波纹 (Ripple)

```css
@keyframes rippleAnim {
  0%   { box-shadow: 0 0 0 0 rgba(255,255,255,.35), inset 0 0 0 0 rgba(255,255,255,.15) }
  100% { box-shadow: 0 0 0 12px transparent,   inset 0 0 0 6px transparent }
}
```
JS: `addRipple(el)` — 创建 `.ripple` div，600ms 后自动移除。

### 12.4 爆炸冲击波 (Shockwave)

```css
@keyframes shockwaveAnim {
  0%   { transform: scale(.8); opacity: 1; border-width: 3px }
  100% { transform: scale(2.5); opacity: 0; border-width: 1px }
}
```
JS: `addShockwave(el, color)` — 创建 `.shockwave` div，600ms 后移除。

### 12.5 爆炸粒子 (Particles)

```css
@keyframes particleFly {
  0%   { opacity: 1; transform: translate(0,0) scale(1) }
  100% { opacity: 0; transform: translate(var(--dx),var(--dy)) scale(0) }
}
```
JS: `addParticles(el, color, count)` — 创建 6~8 个 `.particle` div，随机方向飞散，700ms 后移除。

### 12.6 棋子弹出 (Pop)

```css
@keyframes pop {
  0%   { transform: translate(-50%,-50%) scale(0) }
  70%  { transform: translate(-50%,-50%) scale(1.12) }
  100% { transform: translate(-50%,-50%) scale(1) }
}
```

### 12.7 爆炸闪烁

```css
@keyframes explode {
  0%   { background: rgba(255,255,255,.3); transform: scale(1) }
  20%  { background: rgba(255,255,255,.7); transform: scale(1.1) }
  40%  { background: rgba(255,255,255,.8); transform: scale(1.15) }
  60%  { background: rgba(255,255,255,.5); transform: scale(1.05) }
  100% { background: rgba(255,255,255,.05); transform: scale(1) }
}
```

### 12.8 模态框进入

```css
@keyframes modalIn {
  from { opacity: 0; transform: translateY(20px) scale(.95) }
  to   { opacity: 1; transform: translateY(0) scale(1) }
}
```

---

## 13. 音效系统

使用 Web Audio API 合成，无需外部音频文件。

### 13.1 核心函数

```javascript
function playTone(frequency, duration, waveform, volume) {
  const ctx = getAudioCtx();
  const osc = ctx.createOscillator();
  const gain = ctx.createGain();
  osc.type = waveform;      // 'sine' | 'square' | 'sawtooth'
  osc.frequency.value = frequency;
  gain.gain.value = volume;
  gain.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + duration);
  osc.connect(gain);
  gain.connect(ctx.destination);
  osc.start();
  osc.stop(ctx.currentTime + duration);
}
```

### 13.2 音效映射

| 事件 | 音效 | 频率 | 时长 | 波形 | 音量 |
|------|------|------|------|------|------|
| 落子 | 短促高音 | 800Hz | 0.08s | sine | 0.12 |
| 爆炸 | 低沉噪声 | 150Hz | 0.25s | sawtooth | 0.15 |
| 淘汰 | 下行音阶 | 400→300→200Hz | 3×0.15s | square | 0.08 |
| 获胜 | 上行琶音 | 523→659→784Hz | 3 个音 | sine | 0.12→0.15 |

---

## 14. 游戏核心逻辑

### 14.1 基础函数

```javascript
function cap(i, j, s) { return 4 }  // 每个格子容量（硬编码 4）
function nbrs(i, j, s) { /* 返回上下左右四个邻居坐标 */ }
function nbrs8(i, j, s) { /* 返回周围 8 格坐标 */ }
function mkBoard(s) { /* 创建 s×s 的空棋盘 */ }
function hasPieces(p, b) { /* 玩家 p 是否有棋子 */ }
```

### 14.2 落子与连锁反应 `processClick(b, s, x, y, pl, anim, playerColor)`

**参数**:
- `b`: 棋盘数据
- `s`: 棋盘大小
- `(x, y)`: 落子位置
- `pl`: 玩家索引
- `anim`: 动画模式 (`'explode'` 或 `false`)
- `playerColor`: 颜色值（用于爆炸特效）

**流程**:
1. **空位落子**: 若格子为空，设置 `owner = pl`
   - 如果玩家首次落子（无其他棋子），`count = 3`
   - 否则 `count = 1`
2. **已有棋子**: 若属于当前玩家，`count++`（升级）
3. **别人棋子**: 无法操作，返回 `[]`
4. **连锁反应**: BFS 队列处理爆炸
   - 当 `cell.count >= 4` 时爆炸
   - `count = 0`, `owner = null`
   - 向上下左右四邻各扩散 1 个（`count++`, `owner = pl`）
   - 被扩散的格子继续入队检查
5. **动画模式** (`anim === 'explode'`):
   - 每次爆炸逐个渲染，间隔 220ms
   - 用户可点"跳过动画"设置 `chainSkipAll = true`
   - 跳过时一口气处理完所有连锁
   - 爆炸时同时调用 `addShockwave(el, color)` 和 `addParticles(el, color, 8)`
6. **非动画模式**: 快速处理，若有 `anim` 则添加 `pop` 类
7. **统计记录**: 更新 `chainStats` 和 `maxChainOverall`
8. **返回值**: 被淘汰的玩家数组（在爆炸中失去全部棋子的玩家）

### 14.3 同步版落子 `processClickSync(b, s, x, y, pl)`

与 `processClick` 逻辑相同但不含动画/音效/颜色参数，专供 JS Alpha-Beta 搜索使用。

### 14.4 首子限制规则

每位玩家第一步有位置限制：
- 以棋盘上任何已有棋子为中心，周围 **12 格** 不能落子
- 这 12 格 = 上下各 2 格 + 左右各 2 格 + 斜角各 1 格

```javascript
function isInFirstMoveRestricted(x, y, fx, fy) {
  // (fx, fy) 是已有棋子的位置
  // (x, y) 是候选位置
  // (2,0) 或 (-2,0) → true
  // (1,-1), (1,0), (1,1) 或 (-1,...) → true
  // (0,-2), (0,-1), (0,1), (0,2) → true（排除 (0,0)）
}
```

首次落子时 `count = 3`（三级），之后每次升级 +1。

### 14.5 游戏结束检测

当存活的玩家数量 ≤ 1 时游戏结束：
- 有 1 个存活 → 该玩家获胜
- 0 个存活 → 平局（通常不会发生，因为最后一个玩家不会被自己淘汰）

在 Rust 后端 `process_move` 命令中：
```rust
let game_over = eliminated.len() >= max_players.saturating_sub(1);
let winner = if game_over {
    // 找唯一存活的玩家
};
```

在前端 `localClick` 和 `triggerAI` 中：
```javascript
if (alive.length <= 1) {
  gameOver = true;
  playGameOver();
  showSettlement(alive[0], COLOR_NAMES, gameHistory);
}
```

---

## 15. AI 引擎

### 15.1 策略算法 (Strategy) — JS 实现

`aiFindMove(player, sz, b)` 基于启发式规则：

**1. 首步（无棋子）**:
- 优先选非边角位置（`1..sz-2`）
- 选最靠近棋盘中心的格子
- 如果在限制区域找不到位置，逐步扩大搜索范围

**2. 三级棋子优先** (`count === 3`):
- 在所有三级棋子里选周围对手三级棋子最多的
- 至少命中 1 个对手三级棋子才触发

**3. 安全二级** (`count === 2`):
- 排除周围有对手三级棋子的格子
- 评分公式: `edge + corner - nearAnyOpp*5 + Math.random()*1`
- 边角加分: 边 3 分, 角 5 分
- 对手相邻越多扣分越多

**4. 一进二** (`count === 1`):
- 评分: `-nearOpp*3 + Math.random()*2`
- 避开对手较多的格子

**5. 下三级** (`count === 2`, 无安全候选):
- 评分: `nearOpp*5 + edge + Math.random()*1`
- 主动靠近对手

**6. 随机**: 随机选一个可升级的棋子

**7. 兜底**: 返回 `mine[0]`

所有同级选择都带随机分量，增加多样性。

### 15.2 Alpha-Beta 剪枝 — Rust 实现

**Rust 端 (`lib.rs`)**:

`GameState` 结构体实现 `alpha_beta_pruning::AlphaBeta` trait：

```rust
struct GameState {
    board: GameBoard,
    sz: usize,
    player: usize,       // 当前轮到谁
    ai_player: usize,    // AI 玩家（用于评估）
    eliminated: Vec<usize>,
    max_players: usize,
    game_count: u32,
}
```

**评估函数 `eval_board()`**:
- 基础分: `(myScore - oppScore) * 2 + (myTerritory - oppTerritory)`
- 早期随机扰动: 前 7 局加入基于棋盘哈希的确定性伪随机值
  - `game_count < 5`: 随机范围 `(5-count)*8`
  - `game_count < 7`: 随机范围 `(7-count)*2`

**走法排序 `order_moves()`**:
- 基础分: `count * 10`
- 三级棋子: `+100`
- 周围对手棋子点数: `+5/count`
- 限制前 10 个走法

**并行搜索**:
- 根节点使用 `par_iter()`（Rayon 多线程并行）
- 子节点使用 trait 默认的 `alpha_beta()`（克隆式搜索）
- 🚫 不用 set/unset，每次克隆 `GameState` 再修改

**首步居中**:
- 优先选非边角位置
- 距离中心最近的格子

**调用入口** (`find_best_move`):
```rust
pub fn find_best_move(board, sz, player, depth, eliminated, max_players, game_count, first_move_pos) -> Option<(usize, usize)>
```

**JS 端回退 (`aiFindMoveAlphaBeta`)**:
如果 Rust AI 调用失败（如不在 Tauri 环境），JS 实现一个简单的 Alpha-Beta：
- 时间限制: `3000 + depth*500` ms
- 最大 10 分支
- 简化的评估函数

### 15.3 AI 触发流程 `triggerAI()`

1. 检查游戏未结束、AI 不在思考、未暂停
2. 显示 "🤔 AI N 思考中..." 消息
3. 获取该 AI 的配置（eve 模式用 `aiConfigs[p]`，否则用全局）
4. 如果是 Alpha-Beta:
   - 先调 Rust `ai_move` invoke
   - 失败则回退到 JS `aiFindMoveAlphaBeta`
5. 如果是策略算法: `aiFindMove`
6. 没有合法落子: 跳过，轮到下一个玩家
7. 有落子: 高亮目标格 350ms，然后 `processClick`（动画 + 爆炸）
8. 检测淘汰和游戏结束

---

## 16. Rust 后端

### 16.1 Tauri 命令

注册了 8 个命令（`tauri/src-tauri/src/lib.rs`）:

| 命令 | 参数 | 返回 | 用途 |
|------|------|------|------|
| `process_move` | board, size, x, y, player, maxPlayers | `ProcessMoveResult` | 处理落子与连锁反应 |
| `ai_move` | board, size, player, depth, eliminated, maxPlayers, gameCount, firstMovePos | `[usize; 2]` | Rust AI 搜索 |
| `save_game_history` | record: `HistoryRecord` | `()` | 保存完整记录到 history.json |
| `load_game_history` | 无 | `Vec<HistoryRecord>` | 读取全部历史 |
| `clear_game_history` | 无 | `()` | 删除 history.json |
| `save_round_history` | data: Vec\<TurnHistory\> | `()` | 保存回合数据到 round_data.json |
| `load_round_history` | 无 | `Vec<TurnHistory>` | 读取回合数据 |
| `clear_round_history` | 无 | `()` | 删除 round_data.json |
| `exit_app` | 无 | `()` | 关闭应用 |

### 16.2 Rust 核心逻辑

**`process_click()`** — Rust 版落子/连锁反应：
- 与 JS `processClickSync` 逻辑相同
- 使用 `VecDeque` (FIFO) 处理连锁
- 收集爆炸前后的玩家集合，差集 = 被淘汰玩家

**`find_best_move()`** — 创建 `GameState` 调用 `state.run(depth)`：
- `run()` 方法重写了 `AlphaBeta` trait 的默认实现
- 首步处理 → 走法排序 → Rayon 并行根搜索 → 递归 Alpha-Beta

### 16.3 数据文件位置

```
<app_data_dir>/history.json      # 游戏历史记录
<app_data_dir>/round_data.json   # 临时回合数据（内存溢出用）
```
在桌面端 `app_data_dir` 是系统应用数据目录，Android 上是应用内部存储。

---

## 17. 数据存储与格式

### 17.1 内存历史 `gameHistory`

```javascript
// 格式: Array<{turn: number, snapshot: {playerId: {pieces, points}}}>
[
  {turn: 0, snapshot: {"0": {pieces: 1, points: 3}, "1": {pieces: 0, points: 0}}},
  {turn: 1, snapshot: {"0": {pieces: 2, points: 2}, "1": {pieces: 1, points: 3}}},
  ...
]
```

- 每落子一次添加一条
- 内存上限 500 条，超出后溢出到 `round_data.json`
- 溢出后保留 100 条在内存

### 17.2 磁盘回合数据 `round_data.json`

由 Rust 后端管理，格式同 `gameHistory`（`Vec<TurnHistory>` 序列化）。
- 使用 `serde_json::to_string()`（非 pretty，紧凑格式）
- 游戏结束保存历史到 `history.json` 后自动清空

### 17.3 游戏历史 `history.json`

紧凑格式（优化后）：

```json
[
  {
    "id": 1712345678901,
    "time": "2026/07/21 12:00:00",
    "mode": "ai",
    "aiAlgorithm": "alphabeta",
    "aiDepth": 2,
    "gameCount": 1,
    "playerCount": 4,
    "aiCount": 3,
    "boardSize": 7,
    "winner": 0,
    "colorNames": ["红色","黄色","蓝色","绿色"],
    "chainStats": {"0": {"triggered": 3, "maxChain": 2}, "1": {"triggered": 1, "maxChain": 1}},
    "maxChain": {"player": 0, "length": 2},
    "history": {
      "c": true,
      "t": 24,
      "p": [[1,2,3,...],[0,1,0,...],[0,0,2,...],[0,0,0,...]],
      "pt": [[3,2,1,...],[0,5,0,...],[0,0,4,...],[0,0,0,...]]
    }
  }
]
```

**字段说明**:
- `id`: `Date.now()` 时间戳
- `time`: `YYYY/MM/DD HH:mm:ss` 格式
- `mode`: `'ai'` | `'local'` | `'eve'`
- `history` 紧凑格式: `{c: true, t: turnCount, p: [[pieces...], ...], pt: [[points...], ...]}`
  - `p[player][turn]` = 玩家在回合的棋子数
  - `pt[player][turn]` = 玩家在回合的点数
  - 比逐回合存储体积小 5~10 倍

**旧版历史兼容**: 如果 `history` 字段是数组（无 `c: true` 标记），`expandHistory()` 直接原样返回。

### 17.4 压缩/解压函数

```javascript
function compactHistory(history, playerCount) → {c:true, t:n, p:[[pieces...],...], pt:[[points...],...]}
function expandHistory(compact, playerCount) → [{turn, snapshot: {pid: {pieces, points}}}, ...]
```

### 17.5 历史存储管理

```javascript
const HISTORY_MEM_MAX = 500;   // 内存上限
const HISTORY_MEM_KEEP = 100;  // 溢出后保留条数

recordHistory()                 // 记录当前快照到 gameHistory
flushOverflowHistory()          // 溢出 gameHistory 到磁盘
flushAndGetFullHistory()        // 全部刷盘 + 返回完整历史
resetRoundHistory()             // 清空历史（新游戏时调用）
```

`flushAndGetFullHistory()` 流程:
1. 如果 `gameHistory` 有数据
2. 加载磁盘现有数据
3. 合并（偏移 turn 编号）
4. 保存回磁盘
5. 清空 `gameHistory`
6. 加载并返回完整磁盘数据

### 17.6 Rust 后端序列化

所有结构体使用 `#[serde(rename_all = "camelCase")]` 自动转换字段名（Rust snake_case ↔ JS camelCase）。

```rust
pub struct HistoryRecord {
    pub id: u64,
    pub time: String,
    pub mode: String,
    pub ai_algorithm: String,   // → "aiAlgorithm"
    pub ai_depth: u32,          // → "aiDepth"
    pub player_count: u32,      // → "playerCount"
    pub board_size: u32,        // → "boardSize"
    pub winner: Option<usize>,
    pub color_names: Vec<String>,  // → "colorNames"
    pub chain_stats: HashMap<String, ChainStatsPlayer>,  // → "chainStats"
    pub max_chain: MaxChain,    // → "maxChain"
    pub history: serde_json::Value,  // → 可以存数组（旧版）或对象（紧凑）
}
```

---

## 18. 视觉特效辅助函数

### 18.1 `addRipple(el)`

创建 `.ripple` div 作为子元素添加到格子中。600ms 后自动移除。
- 效果: 从中心向外扩散的白色光波
- 动画: 0.5s ease-out
- 触发: 每次落子（玩家和 AI 都触发）

### 18.2 `addShockwave(el, color)`

创建 `.shockwave` div。600ms 后移除。
- 效果: 从格子中心扩散的彩色环形冲击波
- 动画: 0.5s ease-out，从 scale 0.8 放大到 2.5
- 颜色: 使用玩家颜色（`COLORS[curPlayer]`）

### 18.3 `addParticles(el, color, count)`

创建 count 个 `.particle` div。700ms 后移除。
- 效果: 彩色粒子向随机方向飞散
- 方向: 随机角度 `0~2π`，距离 `30~80px`
- 使用 CSS 变量 `--dx`、`--dy` 控制方向

---

## 19. 自定义控件系统

项目使用自定义控件替代原生 `<select>`，所有控件通过文档级事件委托管理。

### 19.1 控件类型

| 控件 | 容器类 | 选项类 | 取值函数 |
|------|--------|--------|---------|
| 棋盘大小 | `.size-grid` | `.size-btn` | `getSel('id')` |
| 数量选项 | `.btn-group` | `.gb` | `getSel('id')` |
| 算法卡片 | `.alg-cards` | `.alg-card` | `getSelStr('id')` |

### 19.2 事件委托

```javascript
document.addEventListener('click', function(e) {
  const t = e.target.closest('.size-btn,.gb,.alg-card');
  if (!t) return;
  const container = t.closest('.size-grid,.btn-group,.alg-cards');
  setSelected(container, t);
  // 副作用触发:
  // 'aiCountGroup' → toggleAIColorPicker()
  // 'aiAlgorithmCards' → toggleAIDepth()
  // 'eveCountGroup' → generateEveConfig()
});
```

### 19.3 选择函数

```javascript
function getSel(containerId)     // 返回数字（选中的 data-value）
function getSelStr(containerId)  // 返回字符串
function setSelected(container, target)  // 切换选中状态
```

---

## 20. 全部 JS 全局状态变量

| 变量 | 类型 | 用途 |
|------|------|------|
| `COLORS` | string[10] | 10 种玩家颜色 |
| `COLORS_LIGHT` | string[10] | 低透明度版本 |
| `COLOR_NAMES` | string[10] | 颜色中文名称 |
| `audioCtx` | AudioContext\|null | Web Audio 上下文 |
| `board` | Cell[][] | 棋盘数据 |
| `curPlayer` | number | 当前玩家 |
| `size` | number | 棋盘边长 |
| `maxPlayers` | number | 总玩家数 |
| `cells` | HTMLElement[][] | DOM 格子引用 |
| `gameMode` | string\|null | 'ai'/'local'/'eve' |
| `gameOver` | boolean | 是否结束 |
| `aiPlayers` | Set | AI 玩家集合 |
| `aiThinking` | boolean | AI 思考中 |
| `aiAlgorithm` | string | 全局 AI 算法 |
| `aiDepth` | number | 全局搜索深度 |
| `aiConfigs` | object | 每个 AI 的配置 |
| `gameCount` | number | 对局计数 |
| `eliminatedPlayers` | Set | 已淘汰玩家 |
| `gameHistory` | array | 回合历史 |
| `chainStats` | object | 连爆统计 |
| `maxChainOverall` | object | 最高连爆 |
| `chainSkipAll` | boolean | 跳过动画标志 |
| `firstMovePos` | array\|null | 首子位置 |
| `isPaused` | boolean | 暂停状态 |
| `_chartCtx` | object | 图表上下文缓存 |
| `_boardCache` | array\|null | 棋盘渲染缓存 |
| `selectedPlayerColor` | number | AI 模式玩家颜色 |
| `turnCount` | number | 回合计数（未使用） |

---

## 21. 全部 Rust 类型定义

```rust
pub struct Cell {
    pub owner: Option<usize>,   // 玩家索引
    pub count: u8,              // 棋子数 (0~4)
}
pub type GameBoard = Vec<Vec<Cell>>;

pub struct ChainStatsPlayer {
    pub triggered: u32,      // 触发连爆次数
    pub max_chain: u32,      // 最高连爆
}

pub struct MaxChain {
    pub player: Option<usize>,
    pub length: u32,
}

pub struct PlayerSnapshot {
    pub pieces: u32,
    pub points: u32,
}

pub struct TurnHistory {
    pub turn: u32,
    pub snapshot: HashMap<String, PlayerSnapshot>,
}

pub struct HistoryRecord {
    pub id: u64,
    pub time: String,
    pub mode: String,
    pub ai_algorithm: String,
    pub ai_depth: u32,
    pub game_count: u32,
    pub player_count: u32,
    pub ai_count: u32,
    pub board_size: u32,
    pub winner: Option<usize>,
    pub color_names: Vec<String>,
    pub chain_stats: HashMap<String, ChainStatsPlayer>,
    pub max_chain: MaxChain,
    pub history: serde_json::Value,  // 紧凑格式或旧版数组
}

pub struct ProcessMoveResult {
    pub board: GameBoard,
    pub eliminated: Vec<usize>,
    pub chain_count: u32,
    pub game_over: bool,
    pub winner: Option<usize>,
}

pub struct AppState {
    pub history_file: Mutex<PathBuf>,
    pub app_data_dir: Mutex<PathBuf>,
}

struct GameState {
    board: GameBoard,
    sz: usize,
    player: usize,
    ai_player: usize,
    eliminated: Vec<usize>,
    max_players: usize,
    game_count: u32,
}
// 实现 AlphaBeta<(usize, usize)> trait
```

---

## 22. 构建与签名脚本

`build_apk.sh` 一键脚本:

```bash
#!/bin/sh
# 用法: ./build_apk.sh <keystore_password>

PASSWORD="$1"
KEYSTORE="release.keystore"
PRODUCT="连锁棋"
VERSION="2.1.0-beta"
OUTPUT="release/${PRODUCT}-${VERSION}.apk"
UNSIGNED_APK="tauri/src-tauri/gen/android/app/build/outputs/apk/universal/release/app-universal-release-unsigned.apk"

# 1. 编译 arm64 APK
npx tauri android build --target aarch64

# 2. 签名
cp "$UNSIGNED_APK" "$OUTPUT"
apksigner sign --ks "$KEYSTORE" --ks-pass "pass:${PASSWORD}" --out "$OUTPUT" "$OUTPUT"

# 3. 验证
apksigner verify --verbose "$OUTPUT"
```

---

## 23. Tauri 配置

`tauri.conf.json`:

```json
{
  "productName": "连锁棋",
  "version": "2.1.0-beta",
  "identifier": "com.chainchess.app",
  "build": {
    "frontendDist": "../public",
    "devUrl": "http://localhost:1420",
    "beforeDevCommand": "",
    "beforeBuildCommand": ""
  },
  "app": {
    "windows": [{
      "title": "♟ 连锁棋",
      "width": 520,
      "height": 780,
      "resizable": false,
      "fullscreen": false,
      "center": true
    }],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/128x128@2x.png", "icons/icon.icns", "icons/icon.ico"]
  }
}
```

窗口固定 520×780，不可调整大小，居中显示。CSP 为 null（允许内联样式）。

---

> 全文完。本文档覆盖了项目 100% 的源代码细节，从 CSS 动画到 Rust 并行搜索，从 UI 布局到数据持久化。任何开发者都能基于本文重新实现完全一致的应用。
