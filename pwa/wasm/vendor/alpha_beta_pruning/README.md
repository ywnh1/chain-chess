# Alpha-Beta Pruning

Rust 实现的泛型 α-β 剪枝搜索算法，支持并行评估、自定义游戏状态和走法。

## 特性

- **泛型设计**: 通过 `AlphaBeta<T>` trait 适配任意博弈游戏
- **α-β 剪枝**: 忽略不可能影响决策的搜索分支，将复杂度从 O(b^d) 降至 O(b^(d/2))
- **并行评估**: 根层走法使用 `rayon` 并行评估
- **零依赖核心**: 仅依赖 `rayon` 实现并发

## 用法

将你的游戏状态实现 `AlphaBeta<T>` trait：

```rust
use alpha_beta_pruning::{AlphaBeta, Grade};

#[derive(Clone)]
struct TicTacToe {
    board: [i8; 9],
    player: i8,  // 1 or -1
}

impl AlphaBeta<usize> for TicTacToe {
    fn evaluate(&self) -> Grade {
        // 返回当前局面的评分
        // Grade::Min  = 负无穷（输）
        // Grade::Max  = 正无穷（赢）
        // Grade::Score(n) = 具体评分
        Grade::Score(0)
    }

    fn get_moves(&self) -> Vec<usize> {
        // 返回所有合法走法（棋盘空位索引）
        (0..9).filter(|&i| self.board[i] == 0).collect()
    }

    fn set(&mut self, m: &usize) {
        self.board[*m] = self.player;
        self.player = -self.player;
    }

    fn unset(&mut self, m: &usize) {
        self.board[*m] = 0;
        self.player = -self.player;
    }
}

// 找到最佳走法
// let best = game.run(9);  // 深度 9 的搜索
```

## API

### `AlphaBeta<T>` Trait

| 方法 | 说明 | 必需 |
|------|------|------|
| `evaluate()` | 评估当前局面评分 | 是 |
| `get_moves()` | 返回当前可用的走法列表 | 是 |
| `set(&mut self, m: &T)` | 应用走法到局面 | 是 |
| `unset(&mut self, m: &T)` | 撤销走法 | 是 |
| `run(depth)` | 搜索最佳走法（带默认实现） | 否 |
| `alpha_beta(alpha, beta, depth, is_max)` | α-β 递归搜索（带默认实现） | 否 |

### `Grade` 枚举

```rust
pub enum Grade {
    Min,          // 负无穷（最差评分）
    Score(i64),   // 具体评分值
    Max,          // 正无穷（最佳评分）
}
```

通过 `Ord` 派生，`Grade` 支持比较：`Min < Score(n) < Max`。

## 许可证

MIT
