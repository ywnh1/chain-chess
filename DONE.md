# 连锁棋优化更新日志

## 1. Rust 后端安全与可靠性优化

### 1.1 修复 `unset()` 空实现破坏 trait 契约
- **问题**：`AlphaBeta` trait 的 `unset()` 是空操作（no-op），虽然通过重写 `run()`/`alpha_beta()` + 克隆方式规避，但若 trait 更新或被移除，搜索树状态会完全损坏。
- **优化**：改为 `panic!("unset() not supported — use clone-based approach")`，明确约束条件并附带迁移指引。

### 1.2 链式爆炸 `u8` 溢出防护
- **问题**：`board[nx][ny].count += 1` 为 `u8` 类型，极端棋盘配置下可能溢出。
- **优化**：改用 `saturating_add(1)`，确保数值安全。

### 1.3 文件写入原子化
- **问题**：所有文件写入使用 `fs::write()` 直接覆写，部分写入导致数据损坏。
- **优化**：新增 `atomic_write()` 辅助函数（写临时文件 → `rename` 原子重命名），替换全部 4 处 `fs::write()` 调用。

### 1.4 修复 `chain_count` 始终为 0
- **问题**：`process_move` 中 `chain_count` 硬编码为 0，注释写"将在别处计算"但从未实现。
- **优化**：`process_click` 返回类型从 `Vec<usize>` 改为 `(Vec<usize>, u32)`，正确统计连锁爆裂次数。

### 1.5 版本号统一
- **问题**：三处版本号不一致（Cargo.toml 1.3.2-beta / tauri.conf.json 2.2.0 / build_apk.sh 2.3.0-beta）。
- **优化**：以 Cargo.toml 为单一信源，统一为 `2.3.0-beta`。

### 1.6 代码整洁性
- **问题**：`use std::io::Write` 出现在函数体内，`board.clone()` 多余克隆。
- **优化**：将 `use` 移至文件顶部导入区，移除不必要的 `board.clone()`。

## 2. 安全性增强

### 2.1 密钥库密码移除硬编码
- **问题**：`build_apk.sh` 中 `PASSWORD="[redacted]"` 以明文形式存在于版本控制。
- **优化**：改为从命令行参数 `$1` 获取密码，`cp` 文件复制后立即清理。密码不再进入版本控制。

### 2.2 启用 CSP 安全策略
- **问题**：`tauri.conf.json` 中 `"csp": null` 完全禁用内容安全策略。
- **优化**：添加基础 CSP：`default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'`。

### 2.3 修复 XSS 向量
- **问题**：`exportHistory` 用 `innerHTML` + 拼接后端返回字节数字符串；`renderGameCharts` 和 `showHistoryDetail` 渲染玩家颜色名时使用 `innerHTML`。
- **优化**：全部改用 DOM 方法（`createElement`、`textContent`、`createTextNode`）替代 `innerHTML` 拼接，用户数据只通过 `textContent` 注入。

## 3. 前端 JavaScript 优化

### 3.1 消除静默错误吞噬
- **问题**：7 处 `.catch(()=>{})` 静默吞掉所有错误，用户无反馈故障。
- **优化**：全部替换为 `console.warn('描述信息:', e)`，至少保留日志记录。

### 3.2 内存优化
- **问题**：`_boardCache` 为每个格子存储全局 `curPlayer` 值，造成内存浪费。
- **优化**：不再逐格缓存 `curPlayer`，改用全局 `_lastRenderPlayer` 追踪渲染时的玩家切换。

## 4. 无障碍与可访问性优化

### 4.1 移除禁止缩放限制
- **问题**：`user-scalable=no` / `maximum-scale=1.0` 违反 WCAG 1.4.4（文字缩放）。
- **优化**：移除限制，允许用户自由缩放。

### 4.2 尊重系统减少动效设置
- **问题**：无 `prefers-reduced-motion` 降级，所有动画无条件运行。
- **优化**：新增 `@media (prefers-reduced-motion: reduce)` 规则，大幅缩短动画/过渡时长并停用背景漂流动画。

### 4.3 触摸目标达标（WCAG 2.5.5）
- **问题**：`.btn-group .gb` 按钮 38px、`.color-opt` 36px、`.dm-btn` 24px，均低于 44px 推荐值。
- **优化**：分别提升至 44px/44px/32px。

### 4.4 屏幕阅读器支持
- **问题**：棋盘格子无 `role`、`tabindex`、`aria-label`，屏幕阅读器完全不可见。
- **优化**：棋盘容器添加 `role="grid"` 和 `aria-label="棋盘"`；每个格子添加 `role="gridcell"`、`tabindex="-1"`，并通过 `aria-label` 描述位置和棋子状态。

## 5. UI/CSS 一致性修复

### 5.1 补全缺失 CSS 变量
- **问题**：`--glass-w-015` 被 CSS 引用但未定义，阴影无声消失。
- **优化**：在深色模式 `:root` 中新增 `--glass-w-015:rgba(255,255,255,.015)`。

### 5.2 模态框背景适配浅色模式
- **问题**：模态框背景 `rgba(0,0,0,.6)` 硬编码，浅色模式下仍然深黑。
- **优化**：改为 `var(--modal-overlay, rgba(0,0,0,.6))`，支持未来主题变量替换。

## 6. 暂停页显示修复

### 6.1 防止 Tauri Android WebView 恢复旧状态
- **问题**：点击应用图标后，Tauri Android WebView 可能恢复上次会话的 DOM 状态，导致 `#pauseOverlay` 误显示；`#pauseOverlay.settlement` CSS 中的 `display:flex` 与 inline `display:none` 存在优先级不确定性。
- **优化**：inline style 添加 `!important` 确保覆盖；新增页面初始化 IIFE，在脚本执行第一时间强制隐藏 pauseOverlay 并激活 welcome 页面。

### 6.2 暂停层增加退出路径
- **问题**：暂停覆盖层只有「继续游戏」和「结束游戏」按钮，`endGameNow()` 中 `if(gameOver)return` 导致 `gameOver` 为 true 时用户被卡死。
- **优化**：新增「返回主菜单」按钮直接调用 `exitGame()`；`endGameNow()` 移除 `gameOver` 阻断，改为 gameOver 时直接回到主菜单。

## 7. 界面精简（减少 emoji 使用）

### 7.1 欢迎页模式卡片
- **变更**：`🤖` → `AI`，`👥` → `玩家`，`⚔️` → `对战`，`📋` → `记录`，`✕ 关闭应用` → `关闭应用`

### 7.2 页面标题
- **变更**：移除所有标题中的图标 emoji（历史记录、AI 对战、本地对战、AI 斗蛐蛐）

### 7.3 按钮文字
- **变更**：移除按钮中的 emoji（导出/导入/清空、悔棋、结束游戏、返回主菜单）

### 7.4 算法选择卡片
- **变更**：`⚡` → `策略`，`🧠` → `剪枝`

### 7.5 结算/统计页面
- **变更**：`🏆` → 移除（保留颜色标识），`📊` → 移除，`🔥` → 移除，`💥` → 移除，`👑` → `★`，`🕐` → 移除，`✅` → 移除，`🤔` → 移除

## 8. 关于页面

### 8.1 替换关闭按钮为关于按钮
- **变更**：欢迎页底部「关闭应用」按钮替换为「关于」按钮，点击跳转至关于页面
- **原因**：关闭应用功能在移动端无实际作用，改为信息展示入口更有价值

### 8.2 新增关于页面
- **内容**：版本号（v2.3.0-beta）、作者（ywnh1）、开源协议（MIT）、GitHub 仓库链接
- **游戏介绍**：简要说明连锁棋的玩法定位和技术架构
- **游戏规则**：落子、爆裂、连锁、淘汰、胜利五条核心规则
- **更新日志**：从 DONE.md 提取的 7 个版本的更新详情，每次进入关于页面动态渲染
