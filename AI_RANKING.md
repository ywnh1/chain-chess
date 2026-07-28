# ♟ 连锁棋 AI 战力排行榜

> 测试条件: 7×7 棋盘，循环赛 10 局制，先手交替，**共 150 局**

## 最终排名

| 排名 | 选手 | 积分 | 胜率 | 说明 |
|------|------|------|------|------|
| 🥇 1 | **Alpha-Beta ML d2** | **147** | 49/50 (98%) | Alpha-Beta 深度2 + ML 评估 |
| 🥈 2 | **Alpha-Beta 手写 d2** | **123** | 41/50 (82%) | Alpha-Beta 深度2 + 手写评估 |
| 🥉 3 | **Alpha-Beta ML d1** | **78** | 26/50 (52%) | Alpha-Beta 深度1 + ML 评估 |
| 4 | 策略算法 | 42 | 14/50 (28%) | 启发式规则算法 |
| 5 | Alpha-Beta 手写 d1 | 39 | 13/50 (26%) | Alpha-Beta 深度1 + 手写评估 |
| 6 | MCTS d2 | 21 | 7/50 (14%) | 蒙特卡洛树搜索 深度2 |

## 对战详情

| 胜方 | 负方 | 比分 | 用时 |
|------|------|------|------|
| Alpha-Beta 手写 d2 | 策略算法 | 10–0 | 0.8s |
| Alpha-Beta ML d2 | 策略算法 | 10–0 | 0.6s |
| 策略算法 | Alpha-Beta 手写 d1 | 7–3 | 1.1s |
| Alpha-Beta ML d1 | 策略算法 | 7–3 | 1.3s |
| MCTS d2 | 策略算法 | 6–4 | 10.4s |
| Alpha-Beta ML d2 | Alpha-Beta 手写 d2 | 9–1 | 5.4s |
| Alpha-Beta 手写 d2 | Alpha-Beta 手写 d1 | 10–0 | 1.6s |
| Alpha-Beta 手写 d2 | Alpha-Beta ML d1 | 10–0 | 1.9s |
| Alpha-Beta 手写 d2 | MCTS d2 | 10–0 | 12.9s |
| Alpha-Beta ML d2 | Alpha-Beta 手写 d1 | 10–0 | 0.5s |
| Alpha-Beta ML d2 | Alpha-Beta ML d1 | 10–0 | 1.0s |
| Alpha-Beta ML d2 | MCTS d2 | 10–0 | 7.7s |
| Alpha-Beta ML d1 | Alpha-Beta 手写 d1 | 9–1 | 1.8s |
| Alpha-Beta 手写 d1 | MCTS d2 | 9–1 | 13.3s |
| Alpha-Beta ML d1 | MCTS d2 | 10–0 | 14.6s |

## 分析

### ML vs 手写评估

- **ML 评估总胜场**: 75（AB ML d2: 49, AB ML d1: 26）
- **手写评估总胜场**: 54（AB 手写 d2: 41, AB 手写 d1: 13）
- **结论**: ML 评估显著优于手写评估，同深度下 ML 胜率约 **9:1**（AB ML d2 vs AB 手写 d2 = 9–1）

### 搜索深度影响

- **深度 2**: AB ML d2 以 98% 胜率碾压所有对手，是当前最强 AI
- **深度 1**: AB ML d1 表现中等（52%），仅优于策略算法和 MCTS
- 深度翻倍带来约 **2 倍** 的棋力提升（ML d2 98% vs ML d1 52%）

### 算法对比

- **Alpha-Beta ML d2** 以 98% 的压倒性胜率登顶，仅输给同算法的手写版 1 局
- **Alpha-Beta 手写 d2** 以 82% 位居第二，是 ML 之外的最强选择
- **策略算法** 在深度 1 的 AI 中仍有竞争力（击败 AB 手写 d1 7–3）
- **MCTS d2** 整体表现不佳（14%），主要受限于迭代次数和随机性
- **Alpha-Beta 手写 d1** 排名垫底（26%），仅能战胜 MCTS

### 综合排名

```
🥇 Alpha-Beta ML d2   98%  ─── 当前最强
🥈 Alpha-Beta 手写 d2 82%
🥉 Alpha-Beta ML d1   52%
④ 策略算法           28%
⑤ Alpha-Beta 手写 d1 26%
⑥ MCTS d2            14%
```

> 生成时间: 2026-07-28 16:18
