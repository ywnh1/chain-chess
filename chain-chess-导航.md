# 连锁棋 · Chain Chess — 原生安卓重构规格文档

> **用途**：作为从 Tauri + Rust + JS 重构到原生 Android（Kotlin）的规格参考。
> **`tauri/` 目录保留不动**，所有新代码写在 `native/` 下。
> 本文档只陈述基于现有代码已检查过的事实，不推测未实现的行为。

---

## 目录

1. [项目一句话总结](#项目一句话总结)
2. [游戏规则规格（必须精确复现）](#游戏规则规格必须精确复现)
3. [目标安卓架构](#目标安卓架构)
4. [页面/屏幕映射（13 屏）](#页面屏幕映射13-屏)
5. [状态管理规格](#状态管理规格)
6. [AI 引擎移植规格](#ai-引擎移植规格)
7. [数据持久化规格](#数据持久化规格)
8. [音效规格](#音效规格)
9. [视觉主题规格](#视觉主题规格)
10. [Tauri 后端命令 → Kotlin 实现对照](#tauri-后端命令--kotlin-实现对照)
11. [附录：现有实现文件索引（参考用）](#附录现有实现文件索引参考用)

---

## 项目一句话总结

**连锁棋** 是一款基于"爆裂棋（Chain Reaction）"玩法改进的多策略棋盘对战游戏。当前实现在 `tauri/` 下（Tauri 2 + Rust 后端 + 纯 JS 前端），即将在 `native/` 下用 Kotlin 原生 Android 重构。

---

## 游戏规则规格（必须精确复现）

### 棋盘与格容量

- 棋盘为 N×N 网格，N 取值范围 **5~19**
- 每个格子最多容纳 **4 个棋子**
- 格子容量为定值 4，与位置无关

### 落子规则

| 场景 | 行为 |
|------|------|
| 点击**空位** | `owner = 点击者`，`count = 3`（若点击者首子）/ 否则 `count = 1` |
| 点击**自己的棋子** | `count += 1` |
| 点击**别人的棋子** | 无效，无反应 |

### 第一轮限制区域

- 每位玩家的**第一步**落子，必须避开所有已有棋子周围的 **12 格区域**
- 12 格定义（相对于已有棋子所在位置 fx,fy）：

```
                  ↑ 上2格
   ←左2格   ←左1格上1格  上1格  上1格右1格→   →右2格
             ←左1格下1格  下1格  下1格右1格→
                  ↓ 下2格
```

即：
- `dx=2, dy=0`（上2格）
- `dx=-2, dy=0`（下2格）
- `dx=1, dy∈[-1,1]`（上1格及斜角）
- `dx=-1, dy∈[-1,1]`（下1格及斜角）
- `dx=0, dy∈[-2,2]`, 排除 `(0,0)`（左右各2格）

当棋盘上已有多个棋子时，**所有**棋子的 12 格取并集。

### 连锁爆裂

触发条件：格子 `count >= 4`

```
爆裂过程：
1. 格子清零：owner=null, count=0
2. 向上下左右各扩散 1 个棋子
3. 扩散的棋子归爆裂者所有
4. 相邻格子 count+1
5. 若扩散后相邻格子 count >= 4 → 入队继续爆裂（BFS）
```

**连锁动画**：
- 逐格爆炸（间隔约 220ms），每格带冲击波 + 粒子特效
- 用户可点击"跳过动画"一次跳过所有剩余连锁
- 设自动跳过后，后续所有连锁动画跳过

### 淘汰与获胜

- 玩家所有棋子被吞噬（棋盘上无该玩家任何棋子）时 **淘汰**
- 最后存活的一位玩家 **获胜**
- 获胜条件：淘汰人数 `>= maxPlayers - 1`

### 游戏模式

| 模式 | 说明 | 玩家构成 |
|------|------|---------|
| **本地对战** | 2~7 人在同一设备轮流落子 | 全是人类 |
| **AI 对战 (PVE)** | 1 名人类 + 1~6 个 AI | 混合 |
| **AI 斗蛐蛐** | 2~10 个 AI 互相对战 | 全是 AI |

---

## 目标安卓架构

```
native/
├── app/
│   ├── src/main/java/com/chainchess/
│   │   ├── MainActivity.kt              # 单一 Activity（单 Activity 架构）
│   │   ├── navigation/
│   │   │   └── NavGraph.kt              # Navigation Compose 路由定义
│   │   ├── screen/
│   │   │   ├── WelcomeScreen.kt         # 欢迎页
│   │   │   ├── AiLobbyScreen.kt         # AI 对战大厅
│   │   │   ├── LocalLobbyScreen.kt      # 本地对战大厅
│   │   │   ├── EveLobbyScreen.kt        # AI 斗蛐蛐大厅
│   │   │   ├── GameScreen.kt            # 游戏主界面（核心）
│   │   │   ├── HistoryScreen.kt         # 历史记录
│   │   │   ├── CheckoutScreen.kt        # 结算/历史详情
│   │   │   ├── AboutScreen.kt           # 关于
│   │   │   ├── AboutAiScreen.kt         # AI 算法说明
│   │   │   ├── AboutChangelogScreen.kt  # 更新日志
│   │   │   └── AboutLicenseScreen.kt    # 许可证
│   │   ├── viewmodel/
│   │   │   ├── GameViewModel.kt         # 游戏核心状态（棋盘、玩家、回合）
│   │   │   ├── LobbyViewModel.kt        # 大厅配置状态
│   │   │   └── HistoryViewModel.kt      # 历史记录管理
│   │   ├── model/
│   │   │   ├── Board.kt                 # 棋盘核心数据结构
│   │   │   ├── Cell.kt                  # 格子数据（owner, count）
│   │   │   ├── Player.kt                # 玩家数据
│   │   │   ├── GameState.kt             # 完整游戏状态
│   │   │   ├── HistoryRecord.kt         # 历史记录实体
│   │   │   └── ChainStats.kt            # 连锁统计
│   │   ├── engine/
│   │   │   ├── GameEngine.kt            # 落子 + 连锁爆裂核心逻辑
│   │   │   ├── StrategyAi.kt            # 策略算法 AI
│   │   │   ├── AlphaBetaAi.kt           # Alpha-Beta 剪枝 AI
│   │   │   ├── PvsAi.kt                 # PVS NegaMax AI
│   │   │   └── MctsAi.kt                # MCTS AI
│   │   ├── data/
│   │   │   ├── AppDatabase.kt           # Room 数据库
│   │   │   ├── HistoryDao.kt            # 历史记录 DAO
│   │   │   ├── RoundHistoryDao.kt       # 回合历史 DAO
│   │   │   └── SavedGameDao.kt          # 保存游戏状态 DAO
│   │   ├── audio/
│   │   │   └── SoundEngine.kt           # 音效合成引擎
│   │   └── ui/
│   │       ├── theme/                   # Compose 主题
│   │       │   ├── Theme.kt
│   │       │   ├── Color.kt
│   │       │   └── Type.kt
│   │       ├── components/              # 可复用 UI 组件
│   │       │   ├── BoardView.kt         # 棋盘格子渲染
│   │       │   ├── PlayerBar.kt         # 玩家信息栏
│   │       │   ├── GameChart.kt         # 折线图组件
│   │       │   └── ColorPicker.kt       # 颜色选择器
│   │       └── animation/               # 动画
│   │           ├── CellExplosion.kt     # 格子爆炸动画
│   │           ├── RippleEffect.kt      # 点击波纹
│   │           └── ParticleEffect.kt    # 粒子特效
│   └── build.gradle.kts
├── build.gradle.kts
└── settings.gradle.kts
```

### 技术选型建议

| 层面 | 选型 | 原因 |
|------|------|------|
| UI | Jetpack Compose | 原生声明式 UI，与单 Activity 架构兼容 |
| 导航 | Navigation Compose | 声明式路由，支持传参和返回栈 |
| 状态管理 | ViewModel + StateFlow | 生命周期感知，配置变更不丢失 |
| 数据持久化 | Room | 类型安全的 SQLite ORM，支持 Flow 观察 |
| 异步 | Kotlin Coroutines + Flow | 原生协程支持 |
| 音效 | SoundPool / AudioTrack | 合成短音效 |
| 依赖注入 | Hilt / Koin | 推荐但非必需 |
| AI 多线程 | Kotlin Coroutines + Dispatchers.Default | 替代 Rayon |

---

## 页面/屏幕映射（13 屏）

### 页面层级与导航

```
1️⃣ 欢迎页 (Welcome)
├── 2️⃣ AI 对战大厅 (AiLobby)
├── 3️⃣ 本地对战大厅 (LocalLobby)
├── 4️⃣ AI 斗蛐蛐大厅 (EveLobby)
├── 5️⃣ 历史记录 (History)
│   └── 7️⃣ 历史详情 (Checkout)
└── 6️⃣ 关于 (About)
    ├── 10️⃣ AI 算法说明 (AboutAi)
    └── 11️⃣ 更新日志 (AboutChangelog) / 许可证 (AboutLicense)

8️⃣ 游戏主界面 (Game) — 从 2/3/4 进入
    └── 9️⃣ 暂停覆盖层 (PauseOverlay) — 覆盖在 Game 之上

从 Game 返回：
  - 游戏结束 → 7️⃣ 结算 (Checkout)
  - 点击暂停中的"结束游戏" → 7️⃣ 结算 (Checkout)
  - 正常返回 → 对应的大厅页

侧滑返回（Android 物理返回键）行为：
  欢迎页 → 退出确认弹窗
  各大厅/历史/关于 → 回到欢迎页
  结算 → 回到历史或对应大厅
  游戏 → 调出暂停/结束确认
```

### 各屏幕详细规格

#### 1. 欢迎页 (WelcomeScreen)

**UI**：
- 应用 logo："连锁棋"标题，`棋`字用主题色高亮
- 副标题："棋盘策略游戏"
- 六个入口卡片（Compose Card），竖向排列：
  - AI 对战（`🤖 图标`）
  - 本地对战（`👥 图标`）
  - AI 斗蛐蛐（`⚔️ 图标`）
  - 历史记录（`📋 图标`）
  - 关于（`ℹ️ 图标`）

#### 2. AI 对战大厅 (AiLobbyScreen)

**配置项**：

| 配置 | 可选项 | 默认 |
|------|--------|------|
| 棋盘大小 | 5×5 ~ 19×19 | 7×7 |
| AI 对手数 | 1~6 | 1 |
| 玩家颜色 | 10 种颜色选择 | 随机 |
| AI 算法 | 策略 / Alpha-Beta / PVS / MCTS | 策略 |
| 搜索深度 | 1~10（滑块） | 2 |

**交互**：
- 点击"开始游戏" → 跳转到 GameScreen（带全部配置参数）
- 返回键 → 回到欢迎页

#### 3. 本地对战大厅 (LocalLobbyScreen)

**配置项**：

| 配置 | 可选项 | 默认 |
|------|--------|------|
| 棋盘大小 | 5×5 ~ 19×19 | 7×7 |
| 玩家人数 | 2~7 | 2 |

#### 4. AI 斗蛐蛐大厅 (EveLobbyScreen)

**配置项**：

| 配置 | 可选项 | 默认 |
|------|--------|------|
| 棋盘大小 | 5×5 ~ 19×19 | 7×7 |
| AI 数量 | 2~10 | 3 |
| 每个 AI | 独立配置算法 + 深度 | 策略算法 / 深度 2 |

**UI 要求**：
- 每行一个 AI 配置条，显示颜色圆点 + AI 编号 + 算法下拉 + 深度 +/- 按钮
- 当 AI 数量变化时，行数动态增减

#### 5. 游戏主界面 (GameScreen) — 核心

**UI 布局**：

```
┌─────────────────────────┐
│ 悔棋 | 暂停 [⏩自动跳过] │  ← TopAppBar
├─────────────────────────┤
│ [p1] [AI2] [AI3] ...    │  ← PlayerBar（横向滚动）
├─────────────────────────┤
│                         │
│     棋 盘 (N×N)         │  ← BoardView（自定义 Compose）
│                         │
├─────────────────────────┤
│    [跳过动画]            │  ← 连爆时显示浮动按钮
└─────────────────────────┘
```

**核心功能**：

**落子流程**（精确复现）：
```
1. 检查 gameOver / aiThinking / isPaused → 阻止点击
2. 点击空位 → 检查首子限制区域（isInRestrictedZone）
3. saveUndoState() 保存悔棋快照
4. 执行落子逻辑：
   - 空位: owner=player, count=首子?3:1
   - 己方棋子: count++
5. 连锁爆裂 BFS 处理（含逐格动画）
6. 检查淘汰
7. 检查获胜
8. 切换下一玩家
9. 记录历史快照
10. 若轮到 AI → 触发 AI 走法
```

**悔棋**：
- 保存落子前棋盘完整快照到栈
- 支持多次悔棋
- AI 走法也应纳入悔棋（一步回到 AI 走法前）
- 栈上限建议 50~100 步

**暂停**：
- 覆盖层展示实时统计（各玩家棋子数/点数）
- 双折线图：棋子数变化、点数变化（使用自定义 Canvas 或三方图表库）
- "继续游戏"和"结束游戏"按钮

**连爆动画跳过**：
- 连锁进行时显示"跳过动画"按钮
- 自动跳过设置项（持久化到 SharedPreferences）

#### 6. 结算/历史详情 (CheckoutScreen)

**UI**：
- 获胜者标题 + 颜色高亮
- 每位存活玩家统计卡片（棋子数、点数）
- 双折线图（复用 GameScreen 的图表组件）
- "再来一局"按钮 → 回到对应大厅
- 若从历史进入：只展示图表，无"再来一局"

#### 7. 历史记录 (HistoryScreen)

**功能**：
- 列表显示所有历史记录
- 每条显示：时间、模式、棋盘大小、玩家数、胜者、AI 算法/深度
- 点击 → 进入 CheckoutScreen（历史详情模式）
- 长按 → 进入多选模式
- 多选支持：全选、导出选中、删除选中
- 底部操作：导出全部、导入、清空

#### 8. 关于页面

- 游戏版本号、简介
- 三个子页面入口：AI 算法说明、更新日志、许可证

### 暂停覆盖层 (PauseOverlay)

覆盖在 GameScreen 之上，不是独立的 Navigation 目的地：
- 半透明背景 + 居中卡片
- 实时统计 + 折线图
- "继续游戏" / "结束游戏" 按钮

### 退出确认弹窗

欢迎页按返回键时弹出，非独立屏。

---

## 状态管理规格

### 游戏核心数据模型

```kotlin
// Cell — 棋盘格子
data class Cell(
    val owner: Int?,        // null = 空, 0-based 玩家索引
    val count: Int          // 0~4, 棋子数量
)

// GameState — 完整游戏状态（ViewModel 中维护）
data class GameState(
    val board: List<List<Cell>>,    // N×N 棋盘
    val size: Int,                  // 棋盘尺寸 5~19
    val maxPlayers: Int,            // 最大玩家数
    val curPlayer: Int,             // 当前回合玩家 (0-based)
    val eliminatedPlayers: Set<Int>,// 已淘汰玩家
    val gameOver: Boolean,          // 游戏是否结束
    val winner: Int?,               // 获胜者（null 表示进行中）
    val gameMode: GameMode,         // LOCAL / AI / EVE
    val aiPlayers: Set<Int>,        // AI 玩家索引
    val gameHistory: List<TurnSnapshot>, // 逐回合快照
    val chainStats: Map<Int, ChainStats>, // 每位玩家连锁统计
    val maxChainOverall: MaxChain,  // 全局最长连锁
    val isPaused: Boolean,          // 暂停状态
    val aiThinking: Boolean,        // AI 计算中锁
    val autoSkipChain: Boolean,     // 自动跳过连爆动画
    val chainCount: Int             // 当前连爆计数
)

// TurnSnapshot — 回合快照（用于图表和历史）
data class TurnSnapshot(
    val pieces: Map<Int, Int>,  // playerIndex → pieces count
    val points: Map<Int, Int>   // playerIndex → points count (sum of count)
)

// ChainStats — 连锁统计
data class ChainStats(
    val triggered: Int,     // 触发连锁次数
    val maxChain: Int       // 单次最长连锁长度
)

data class MaxChain(
    val player: Int?,
    val length: Int
)

// AI 配置
data class AiConfig(
    val algorithm: AiAlgorithm, // STRATEGY / ALPHABETA / PVS / MCTS
    val depth: Int              // 搜索深度 1~10
)

enum class AiAlgorithm { STRATEGY, ALPHABETA, PVS, MCTS }
enum class GameMode { LOCAL, AI, EVE }
```

### ViewModel 职责

**GameViewModel**：
- 持有 `MutableStateFlow<GameState>`
- `placePiece(x, y)` — 落子 + 连锁爆裂 + 状态更新
- `undoMove()` — 悔棋
- `togglePause()` / `resumeGame()`
- `triggerAi()` — 调度 AI 计算（在 Dispatchers.Default 中运行）
- `skipChain()` — 跳过连爆动画
- 管理 undoStack（`List<GameState>`，每次落子前快照）

**LobbyViewModel**：
- 持有大厅配置状态
- `startGame()` → 创建 GameState 并导航到 GameScreen

**HistoryViewModel**：
- 从 Room 加载历史记录列表
- 导入/导出/删除/清空操作

### 颜色系统

10 种玩家颜色（0-based 索引）：

| 索引 | 颜色 | 色值 |
|------|------|------|
| 0 | 红色 | #E74C3C |
| 1 | 黄色 | #F1C40F |
| 2 | 蓝色 | #3498DB |
| 3 | 绿色 | #2ECC71 |
| 4 | 紫色 | #9B59B6 |
| 5 | 粉色 | #E91E63 |
| 6 | 青色 | #1ABC9C |
| 7 | 橙色 | #F39C12 |
| 8 | 亮白 | #FFF0D0 |
| 9 | 灰蓝 | #5D6D7E |

---

## AI 引擎移植规格

四种算法全部需要从 Rust 移植到 Kotlin。

### 通用数据结构

```kotlin
// 棋盘操作
fun getValidMoves(board: Board, player: Int, restricted: Boolean): List<Pair<Int,Int>>
fun executeMove(board: Board, x: Int, y: Int, player: Int): MoveResult
fun evaluateBoard(board: Board, aiPlayer: Int): Int
```

### 1. 策略算法 (StrategyAi)

**入口**：`fun findBestMove(board: Board, player: Int): Pair<Int,Int>?`

**优先级逻辑**（按顺序依次尝试）：

```
1. 三级棋子（count == 3）
   - 筛选己方 count==3 的棋子
   - 选择周围对手三级最多的那个（引爆连锁）
   - 若 count_opponent_level_3 >= 1 → 返回该棋子

2. 安全二级（count == 2，附近无对手三级）
   - 筛选己方 count==2 且附近无对手三级的棋子
   - 评分：避免边角，避免靠近对手
   - 选择评分最高的

3. 一进二（count == 1）
   - 升级一级棋子，评分：远离对手
   - 选择评分最高的

4. 下三级（count == 2）
   - 将二级升为三级
   - 靠近对手 + 非边角加分

5. 随机选 count < 4 的棋子

6. 保底：返回第一个棋子
```

**首步**：选最靠近棋盘中心且不在限制区域内的空位。

**伪随机**：加入确定性伪随机噪音（基于棋盘哈希），确保同等走法随机选择。

### 2. Alpha-Beta 剪枝 (AlphaBetaAi)

**入口**：`suspend fun findBestMove(board: Board, player: Int, depth: Int): Pair<Int,Int>?`

**核心逻辑**：

```
1. getValidMoves() 获取所有合法走法
2. 走法排序（order_moves）：
   - count 越高分数越高
   - 周围对手越多分数越高
   - 限制最多搜索 top K 个走法（K=10~15）
3. 根节点并行搜索（withContext(Dispatchers.Default)）
4. 每个子节点：cloneBoard → executeMove → alpha_beta(剩余深度)
5. 评估函数 evaluate()：
   - 终局：自己被淘汰→ -INF，自己获胜→ +INF
   - 非终局：计算己方棋子总分 - 对手棋子总分 + 位置/数量额外分
```

**关键**：使用**克隆式搜索**（每次走法复制棋盘），避免 set/unset 配对。

### 3. PVS NegaMax (PvsAi)

**入口**：`suspend fun findBestMove(board: Board, player: Int, depth: Int): Pair<Int,Int>?`

**核心逻辑**：

```
1. 主变例搜索（Principal Variation Search）
2. 假设首走法即最优 → 全窗口 `[alpha, beta]`
3. 后续走法 → 零窗口 `[alpha, alpha+1]` 试探
4. 若零窗口返回 > alpha → 重新全窗口搜索
```

**增强优化**：
- **Killer 启发**：每层维护 2 条杀招走法
- **History 启发**：全局走法效果表
- **QSearch 静止搜索**：搜索到底时只搜索爆炸性走法
- **走法排序**：优先尝试历史效果好的走法和杀招走法

### 4. MCTS 蒙特卡洛 (MctsAi)

**入口**：`suspend fun findBestMove(board: Board, player: Int, iterations: Int): Pair<Int,Int>?`

**核心循环**：

```
for _ in 1..iterations:
  1. SELECT → 从根节点开始，用 UCB1 选择子节点直到叶节点
  2. EXPAND → 叶节点展开（若未终局）
  3. PLAYOUT → 从新节点随机走法到终局
  4. BACKPROPAGATE → 更新路径上所有节点的 visits 和 wins
```

**UCB1 公式**：
```
score = wins/visits + C * sqrt(ln(parentVisits) / visits)
```
其中 C = sqrt(2)（探索常数）

**节点数据结构**：
```kotlin
data class MctsNode(
    val board: Board,
    val player: Int,
    val children: List<MctsNode>,
    val visits: Int,
    val wins: Int,
    val untriedMoves: List<Pair<Int,Int>>
)
```

---

## 数据持久化规格

### Room 数据库

#### 表 1：GameHistory（历史记录）

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | Long (PK) | 时间戳作为 ID |
| `time` | String | 格式 "yyyy-MM-dd HH:mm:ss" |
| `mode` | String | "local" / "ai" / "eve" |
| `aiAlgorithm` | String? | AI 算法名称 |
| `aiDepth` | Int | 搜索深度 |
| `gameCount` | Int | 游戏局数 |
| `playerCount` | Int | 玩家数 |
| `aiCount` | Int | AI 数量 |
| `boardSize` | Int | 棋盘尺寸 |
| `winner` | Int? | 获胜者索引 |
| `colorNames` | String | JSON 数组：颜色名称列表 |
| `chainStats` | String | JSON：每位玩家连锁统计 |
| `maxChain` | String | JSON：最长连锁记录 |
| `history` | String | JSON：紧凑格式回合历史 |
| `finished` | Boolean? | true=已完成, false=未完成 |
| `gameState` | String? | 未完成游戏的完整状态 JSON |

#### 表 2：RoundHistory（回合历史 — 溢出存储）

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | Long (PK, auto) | 自增 ID |
| `turn` | Int | 回合序号 |
| `data` | String | JSON 回合快照 |

#### 表 3：SavedGame（已保存游戏状态）

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | Int (PK) | 固定为 1（单行） |
| `stateJson` | String | 完整游戏状态 JSON |
| `updatedAt` | Long | 最后更新时间戳 |

### 历史紧凑存储

为节省存储空间，历史数据使用紧凑格式：
- 每回合仅存储各玩家的 pieces 和 points 数值数组
- 而非完整的 `{turn, snapshot: {playerId: {pieces, points}}}` 对象

### 内存→磁盘两层策略

- 游戏进行中：回合历史先驻留内存
- 内存上限 500 条 → 溢出时刷入 Room（RoundHistory 表）
- 游戏结束后：合并内存 + 磁盘数据，写入 GameHistory 表
- 每步落子后：自动保存当前游戏状态（SavedGame 表），支持异常恢复

---

## 音效规格

使用 Android `SoundPool` 或 `AudioTrack` 合成四种音效（**无需外部音频文件**）：

| 音效 | 触发时机 | 听觉描述 |
|------|---------|---------|
| 落子 (click) | 每次落子 | 短促高频音，约 100ms |
| 爆炸 (explosion) | 格子爆裂时 | 低频隆隆声，约 300ms |
| 淘汰 (elimination) | 玩家被淘汰时 | 中频下降音，约 400ms |
| 胜利 (gameOver) | 游戏结束时 | 上升音阶，约 800ms |

**实现方案**：
- 使用 `AudioTrack` 配合 `AudioFormat.ENCODING_PCM_16BIT` 生成正弦波/方波
- 或使用 `SoundPool` 加载预生成的 PCM 数据
- 音效开关持久化到 SharedPreferences

---

## 视觉主题规格

### 设计语言

- **深色主题**，背景色 #0f0f13
- **液态玻璃（Glassmorphism）** 卡片风格
  - 背景：半透明 rgba 层 + backdrop-filter blur
  - 边框：半透明白色边框
  - 阴影：多层投影
- **主题色系统**：
  - 主色（accent）：#F0B34B（暖金色）
  - 辅色（accent2）：#5FC3C3（青绿色）
  - 文字：主 #E8E6E3，辅 #7A7885
  - 表面色：rgba(26,26,34,0.85)
  - 卡片色：rgba(35,35,45,0.75)

### 棋盘渲染

- N×N 网格，格子大小自适应屏幕
- 每个格子：
  - 空位：半透明背景 + 边框
  - 己方棋子：背景色 + 圆点
  - 对方棋子：仅圆点（无背景色）
- 圆点绘制（1~4 个点）：
  - 1 个点：居中
  - 2 个点：左右排列
  - 3 个点：正三角排列
  - 4 个点：四角排列
- 圆点颜色：根据格子背景亮度自动选黑/白

### 动画

| 动画 | 实现方式 | 说明 |
|------|---------|------|
| 页面转场 | Compose AnimatedContent | fade + slide |
| 点击波纹 | Canvas 绘制扩散圆环 | 600ms |
| 爆炸冲击波 | Canvas 绘制放大圆环 | 600ms |
| 粒子飞溅 | Canvas 绘制运动粒子 | 700ms |
| 背景漂移 | Compose 无限动画 | 模拟流体效果 |

### 响应式

- 竖屏：列布局，棋盘占主要空间
- 横屏：行布局，信息面板在侧边
- 支持安全区（notch, status bar, navigation bar）

---

## Tauri 后端命令 → Kotlin 实现对照

| Tauri 命令 | Kotlin 位置 | 说明 |
|-----------|------------|------|
| `process_move` | `GameEngine.processMove()` | 落子 + BFS 连锁爆裂 |
| `ai_move` | `AlphaBetaAi.findBestMove()` | Alpha-Beta 搜索 |
| `ai_move_v2` | `PvsAi.findBestMove()` | PVS NegaMax 搜索 |
| `ai_move_mcts` | `MctsAi.findBestMove()` | MCTS 搜索 |
| `ai_move_strategy` | `StrategyAi.findBestMove()` | 策略算法 |
| `save_game_history` | `HistoryDao.insert()` | Room 插入 |
| `load_game_history` | `HistoryDao.getAll()` | Room 查询 |
| `import_game_history` | `HistoryDao.insertAll()` | 去重合并 |
| `export_game_history_dialog` | Android ShareSheet | 系统分享 |
| `delete_game_history_record(s)` | `HistoryDao.deleteByIds()` | Room 删除 |
| `clear_game_history` | `HistoryDao.deleteAll()` | Room 清空 |
| `save_game_state` | `SavedGameDao.upsert()` | 保存游戏状态 |
| `load_game_state` | `SavedGameDao.get()` | 加载游戏状态 |
| `clear_game_state` | `SavedGameDao.delete()` | 清除游戏状态 |
| `save_round_history` | `RoundHistoryDao.insertAll()` | 回合历史刷盘 |
| `load_round_history` | `RoundHistoryDao.getAll()` | 加载回合历史 |
| `clear_round_history` | `RoundHistoryDao.deleteAll()` | 清空回合历史 |
| `exit_app` | `Activity.finishAffinity()` | 退出应用 |

---

## 附录：现有实现文件索引（参考用）

> 当前 Tauri 实现在 `tauri/` 目录，重构期间保持不变，供参考。

### 前端（单文件 SPA）

| 文件 | 行数 | 内容 |
|------|------|------|
| `tauri/public/index.html` | 3978 | 全部 HTML/CSS/JavaScript |
| 1-228 | — | HTML 结构（13 个屏幕 div） |
| 229-1370 | — | CSS（液态玻璃主题、响应式、动画） |
| 1370-1595 | — | 常量、全局变量、DOM 事件绑定 |
| 1595-2497 | — | Router、页面 enter/leave、UI 渲染函数 |
| 2500-2694 | — | **核心游戏逻辑**（processClick、连锁爆裂） |
| 2710-2946 | — | 棋盘渲染、AI 调度 triggerAI |
| 2947-3400 | — | startLocalGame/startAIGame/startEveGame |
| 3400-3978 | — | 图表渲染、历史管理、popstate 导航 |

### Rust 后端

| 文件 | 行数 | 内容 |
|------|------|------|
| `tauri/src-tauri/src/lib.rs` | 1737 | 全部 Rust 后端代码 |
| 1-700 | — | 数据结构、Tauri 命令声明、process_click、has_pieces、get_moves |
| 700-900 | — | `GameState` + Alpha-Beta 实现（走法排序、并行根搜索） |
| 900-1340 | — | `PvsSearcher`（PVS NegaMax + Killer/History + QSearch） |
| 1340-1500 | — | `MctsSearcher`（完整四步循环 + UCB1） |
| 1500-1737 | — | `find_best_move`、`find_best_move_strategy`（策略算法）、辅助函数 |

### 构建与配置

| 文件 | 内容 |
|------|------|
| `tauri/src-tauri/Cargo.toml` | Rust 依赖配置 |
| `tauri/package.json` | 前端包管理（仅 Tauri CLI） |
| `build_apk.sh` | Android APK 构建脚本（后续可废弃） |
| `README.md` | 项目功能介绍 |
| `chain-chess-导航.md` | 本文档 |
