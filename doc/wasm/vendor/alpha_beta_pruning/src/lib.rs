//! A generic alpha-beta pruning search algorithm for adversarial game AI.
//!
//! This crate provides the [`AlphaBeta`] trait — implement it on your game state
//! to get a parallel alpha-beta minimax search with automatic pruning.
//!
//! # Quick start
//!
//! ```rust
//! use alpha_beta_pruning::{AlphaBeta, Grade};
//!
//! #[derive(Clone)]
//! struct MyGame;
//!
//! impl AlphaBeta<u8> for MyGame {
//!     fn evaluate(&self) -> Grade {
//!         Grade::Score(0)
//!     }
//!     fn get_moves(&self) -> Vec<u8> {
//!         vec![]
//!     }
//!     fn set(&mut self, _m: &u8) {}
//!     fn unset(&mut self, _m: &u8) {}
//! }
//! ```

#[cfg(feature = "parallel")]
use rayon::prelude::*;

/// Trait for game states that can be evaluated with alpha-beta pruning.
///
/// Implement this trait on your game state to enable parallel minimax search
/// with alpha-beta pruning.
///
/// # Required methods
///
/// You must implement four methods: [`evaluate`], [`get_moves`], [`set`], and
/// [`unset`]. The [`run`] and [`alpha_beta`] methods have default implementations
/// that call those four.
///
/// [`evaluate`]: Self::evaluate
/// [`get_moves`]: Self::get_moves
/// [`set`]: Self::set
/// [`unset`]: Self::unset
/// [`run`]: Self::run
/// [`alpha_beta`]: Self::alpha_beta
pub trait AlphaBeta<T: Clone + Sync + Send>: Clone + Sync + Send {
    /// Returns the evaluation of the current game state.
    ///
    /// The value must be consistent across calls when the state hasn't changed:
    /// *Higher scores must favour the **maximising** player (the one who calls
    /// [`run`]), and lower scores must favour the **minimising** player.*
    ///
    /// Use [`Grade::Min`] for a guaranteed loss and [`Grade::Max`] for a guaranteed win.
    ///
    /// [`run`]: Self::run
    fn evaluate(&self) -> Grade;

    /// Finds the best move by searching the game tree to the given depth.
    ///
    /// Each candidate move is applied to a **clone** of the current state and
    /// evaluated via [`alpha_beta`]. The search is performed in **parallel**
    /// across all root-level moves using `rayon`.
    ///
    /// Returns `None` when no moves are available.
    ///
    /// [`alpha_beta`]: Self::alpha_beta
    fn run(&self, depth: usize) -> Option<T> {
        #[cfg(feature = "parallel")]
        let iter = self.get_moves().into_par_iter();
        #[cfg(not(feature = "parallel"))]
        let iter = self.get_moves().into_iter();
        iter.map(|t| {
                let mut clone = self.clone();
                clone.set(&t);
                let grade = if depth > 0 {
                    clone.alpha_beta(Grade::Min, Grade::Max, depth - 1, false)
                } else {
                    clone.evaluate()
                };
                (grade, t)
            })
            .max_by(|(g1, _), (g2, _)| g1.cmp(g2))
            .map(|(_, t)| t)
    }
    /// Core recursive alpha-beta search.
    ///
    /// Returns the evaluation of the best reachable outcome from the current
    /// state, assuming optimal play from both sides.
    ///
    /// When `is_max` is `true` the current node belongs to the **maximising**
    /// player; when `false`, to the minimising player. The search stops when
    /// `depth` reaches zero or no moves remain, falling through to
    /// [`evaluate`].
    ///
    /// Pruning happens whenever the current branch is provably worse than an
    /// already-examined alternative — the function returns early and that
    /// subtree is never explored.
    ///
    /// [`evaluate`]: Self::evaluate
    fn alpha_beta(
        &mut self,
        mut alpha: Grade,
        mut beta: Grade,
        depth: usize,
        is_max: bool,
    ) -> Grade {
        let moves = self.get_moves();
        if depth == 0 || moves.is_empty() {
            return self.evaluate();
        }
        if is_max {
            let mut max = Grade::Min;
            for m in moves {
                self.set(&m);
                let eval = self.alpha_beta(alpha, beta, depth - 1, false);
                self.unset(&m);
                max = eval.max(max);
                alpha = alpha.max(eval);
                if beta <= alpha {
                    break;
                }
            }
            max
        } else {
            let mut min: Grade = Grade::Max;
            for m in moves {
                self.set(&m);
                let eval = self.alpha_beta(alpha, beta, depth - 1, true);
                self.unset(&m);
                min = eval.min(min);
                beta = beta.min(eval);
                if alpha >= beta {
                    break;
                }
            }
            min
        }
    }
    /// Returns all legal moves available in the current state.
    ///
    /// The order of moves does not affect correctness but **strongly affects**
    /// pruning efficiency. Implementing move ordering (best moves first) can
    /// dramatically reduce the number of nodes searched.
    fn get_moves(&self) -> Vec<T>;
    /// Applies a move to the game state (in-place).
    ///
    /// This is called before recursing into the child node. Every call to
    /// `set` will eventually be paired with a corresponding [`unset`] call
    /// that restores the state.
    ///
    /// [`unset`]: Self::unset
    fn set(&mut self, m: &T);
    /// Reverts a previously applied move.
    ///
    /// Must restore the state to exactly what it was before the matching
    /// [`set`] call.  See [`set`] for the usage contract.
    ///
    /// [`set`]: Self::set
    fn unset(&mut self, m: &T);
}

/// Evaluation value for a game state.
///
/// `Grade` forms a total order with meaningful comparisons:
/// - [`Grade::Min`] < [`Grade::Score`]`(n)` for any `n`
/// - [`Grade::Score`]`(a)` < [`Grade::Score`]`(b)` when `a < b`
/// - [`Grade::Score`]`(n)` < [`Grade::Max`] for any `n`
///
/// Use [`Grade::Min`] for a guaranteed loss and [`Grade::Max`] for a guaranteed win.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, Hash, PartialOrd)]
pub enum Grade {
    /// Negative infinity — the worst possible evaluation (a guaranteed loss
    /// for the maximising player).
    Min,
    /// A concrete numeric score. Higher values favour the maximising player.
    Score(i64),
    /// Positive infinity — the best possible evaluation (a guaranteed win
    /// for the maximising player).
    Max,
}

/// Converts a [`Grade`] reference to its underlying `i64`.
///
/// [`Grade::Min`] maps to [`i64::MIN`] and [`Grade::Max`] to [`i64::MAX`].
impl From<&Grade> for i64 {
    fn from(value: &Grade) -> Self {
        match *value {
            Grade::Max => Self::MAX,
            Grade::Min => Self::MIN,
            Grade::Score(n) => n,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl AlphaBeta<i64> for Vec<i64> {
        fn evaluate(&self) -> Grade {
            let score = self.iter().sum();
            Grade::Score(score)
        }
        fn set(&mut self, m: &i64) {
            self.push(*m);
        }
        fn unset(&mut self, m: &i64) {
            let i = self.iter().position(|x| x == m).unwrap();
            self.remove(i);
        }
        fn get_moves(&self) -> Vec<i64> {
            vec![-111, -34, -4, 53, 0, 94, -6, 957]
        }
    }

    #[test]
    fn test_func() {
        let v = vec![9, 8, 5, 7];
        let num = v.run(5).unwrap();
        assert_eq!(num, 957);
    }

    #[test]
    fn test_vec_depth_zero_returns_some() {
        let v = vec![0];
        assert!(v.run(0).is_some());
    }

    #[test]
    fn test_vec_depth_one_picks_best() {
        let v = vec![0];
        // Moves: [-111, -34, -4, 53, 0, 94, -6, 957]
        // MAX picks 957 (highest immediate sum)
        assert_eq!(v.run(1).unwrap(), 957);
    }

    #[test]
    fn test_vec_depth_two_also_picks_best() {
        let v = vec![0];
        // Depth 2: MAX→MIN. MIN always picks -111 (most negative).
        // 957 + (-111) = 846 beats all others
        assert_eq!(v.run(2).unwrap(), 957);
    }

    // --- ConfigurableGame: state with configurable move list ---

    #[derive(Clone, Debug)]
    struct ConfigurableGame {
        moves: Vec<i64>,
        current: Vec<i64>,
    }

    impl ConfigurableGame {
        fn with_moves(moves: Vec<i64>) -> Self {
            ConfigurableGame { moves, current: vec![] }
        }
    }

    impl AlphaBeta<i64> for ConfigurableGame {
        fn evaluate(&self) -> Grade {
            Grade::Score(self.current.iter().sum())
        }
        fn get_moves(&self) -> Vec<i64> {
            self.moves.clone()
        }
        fn set(&mut self, m: &i64) {
            self.current.push(*m);
        }
        fn unset(&mut self, _m: &i64) {
            self.current.pop();
        }
    }

    #[test]
    fn test_empty_moves_returns_none() {
        let g = ConfigurableGame::with_moves(vec![]);
        assert_eq!(g.run(5), None);
    }

    #[test]
    fn test_single_move() {
        let g = ConfigurableGame::with_moves(vec![42]);
        assert_eq!(g.run(5).unwrap(), 42);
    }

    #[test]
    fn test_max_picks_highest_negative() {
        let g = ConfigurableGame::with_moves(vec![-10, -5, -1]);
        assert_eq!(g.run(1).unwrap(), -1);
    }

    #[test]
    fn test_mixed_moves_depth_one() {
        let g = ConfigurableGame::with_moves(vec![-100, 0, 50]);
        assert_eq!(g.run(1).unwrap(), 50);
    }

    // --- TreeGame: binary tree with known leaf values ---

    #[derive(Clone, Debug)]
    struct TreeGame {
        path: Vec<u8>,
        leaf_values: Vec<i64>,
        max_depth: usize,
    }

    impl AlphaBeta<u8> for TreeGame {
        fn evaluate(&self) -> Grade {
            if self.path.len() == self.max_depth {
                let mut idx = 0usize;
                for (i, &dir) in self.path.iter().enumerate() {
                    if dir == 1 {
                        idx |= 1 << (self.max_depth - 1 - i);
                    }
                }
                Grade::Score(self.leaf_values[idx])
            } else {
                Grade::Score(0)
            }
        }
        fn get_moves(&self) -> Vec<u8> {
            if self.path.len() < self.max_depth {
                vec![0, 1]
            } else {
                vec![]
            }
        }
        fn set(&mut self, m: &u8) {
            self.path.push(*m);
        }
        fn unset(&mut self, _m: &u8) {
            self.path.pop();
        }
    }

    #[test]
    fn test_tree_left_branch_better() {
        //        root (MAX)
        //       /         \
        //      0           1 (MIN)
        //     / \         / \
        //    3   8       1   6
        // MIN: left=3, right=1 → MAX picks left (0)
        let tree = TreeGame {
            path: vec![],
            leaf_values: vec![3, 8, 1, 6],
            max_depth: 2,
        };
        assert_eq!(tree.run(2).unwrap(), 0);
    }

    #[test]
    fn test_tree_right_branch_better() {
        //        root (MAX)
        //       /         \
        //      0           1 (MIN)
        //     / \         / \
        //    1   4       7   2
        // MIN: left=1, right=2 → MAX picks right (1)
        let tree = TreeGame {
            path: vec![],
            leaf_values: vec![1, 4, 7, 2],
            max_depth: 2,
        };
        assert_eq!(tree.run(2).unwrap(), 1);
    }

    #[test]
    fn test_tree_empty_moves() {
        let tree = TreeGame {
            path: vec![],
            leaf_values: vec![],
            max_depth: 0,
        };
        assert_eq!(tree.run(1), None);
    }

    #[test]
    fn test_tree_depth_one_returns_some() {
        let tree = TreeGame {
            path: vec![],
            leaf_values: vec![3, 8, 1, 6],
            max_depth: 2,
        };
        // At depth 1, both children evaluate to Score(0) (not leaves)
        // Returns Some(0) (first move, all equal)
        assert!(tree.run(1).is_some());
    }

    #[test]
    fn test_tree_deterministic() {
        let tree = TreeGame {
            path: vec![],
            leaf_values: vec![3, 8, 1, 6],
            max_depth: 2,
        };
        // Multiple calls with same depth should return same result
        assert_eq!(tree.run(2), tree.run(2));
    }
}
