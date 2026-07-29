#!/usr/bin/env python3
"""
♟ 连锁棋 AI 战力测试
=====================
Round-robin 循环赛，tqdm 进度条，rich 美观输出。

用法:
  python3 test_ai.py                    # 默认 7x7, 每对 20 局
  python3 test_ai.py 9                  # 9x9, 每对 20 局
  python3 test_ai.py 9 50               # 9x9, 每对 50 局
  python3 test_ai.py 7 30 --rebuild     # 强制重新构建
"""

import subprocess
import json
import tempfile
import os
import sys
import time
import argparse
from dataclasses import dataclass


# ─── Rich & tqdm ───
_REQUIRED = (
    ("rich", "rich"),
    ("tqdm", "tqdm"),
)
for _pkg, _pip in _REQUIRED:
    try:
        __import__(_pkg)
    except ImportError:
        print(f"❌ 需要 {_pkg} 库: pip install {_pip}", file=sys.stderr)
        sys.exit(1)

from rich.console import Console
from rich.table import Table
from rich.panel import Panel
from rich import box
from rich.text import Text
from tqdm import tqdm


# ─── AI 参赛者配置 ───

AI_ALGORITHMS = {
    "strategy":   "策略",
    "alphabeta":  "Alpha-Beta",
    "pvs":        "PVS",
    "mcts":       "MCTS",
}


@dataclass
class AiEntry:
    """AI 参赛选手配置"""
    type: str                        # "alphabeta", "pvs", "mcts", "strategy"
    depth: int = 2
    use_ml_eval: bool = True
    name: str = ""

    def label(self) -> str:
        if self.name:
            return self.name
        algo = AI_ALGORITHMS.get(self.type, self.type)
        if self.type == "strategy":
            return f"策略"
        elif self.type == "mcts":
            return f"MCTS d={self.depth}"
        ml = "ML" if self.use_ml_eval else "手写"
        return f"{algo} d={self.depth} {ml}"

    def to_dict(self) -> dict:
        return {
            "type": self.type,
            "depth": self.depth,
            "use_ml_eval": self.use_ml_eval,
            "name": self.label(),
        }


@dataclass
class MatchupResult:
    """一组对战的结果"""
    ai0_name: str
    ai1_name: str
    games: int
    wins0: int
    wins1: int
    draws: int
    time_sec: float


# ─── Project root ───

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))


def build_binary(force: bool = False) -> str:
    """构建 Rust battle 二进制"""
    console = Console()

    binary = os.path.join(
        SCRIPT_DIR, "tauri", "src-tauri", "target", "release", "battle"
    )
    if not force and os.path.exists(binary):
        return binary

    with console.status("[yellow]🔨 构建 battle 二进制...[/yellow]", spinner="dots"):
        start = time.time()
        result = subprocess.run(
            ["cargo", "build", "--release", "--bin", "battle"],
            cwd=os.path.join(SCRIPT_DIR, "tauri", "src-tauri"),
            capture_output=True, text=True,
        )
        elapsed = time.time() - start

    if result.returncode != 0:
        console.print("[red]❌ 构建失败:[/red]")
        for line in result.stderr.split("\n")[-20:]:
            console.print(f"  [dim]{line}[/dim]")
        sys.exit(1)

    return binary


def run_matchup(binary: str, ai0: AiEntry, ai1: AiEntry,
                size: int, games: int) -> MatchupResult:
    """运行 AI 0 vs AI 1 的一轮比赛"""
    config = {
        "size": size,
        "times": games,
        "ai": [ai0.to_dict(), ai1.to_dict()],
    }

    with tempfile.NamedTemporaryFile(mode="w", suffix=".json", delete=False) as f:
        json.dump(config, f, ensure_ascii=False)
        temp_path = f.name

    try:
        result = subprocess.run(
            [binary, temp_path],
            capture_output=True, text=True, timeout=3600,
        )

        # 解析标准输出中的 RESULT 行
        for line in result.stdout.split("\n"):
            line = line.strip()
            if line.startswith("RESULT:"):
                parts = line.replace("RESULT:", "").strip().split()
                data = {}
                for p in parts:
                    k, v = p.split("=")
                    data[k] = v
                return MatchupResult(
                    ai0_name=ai0.label(),
                    ai1_name=ai1.label(),
                    games=int(data["TOTAL"]),
                    wins0=int(data["P0"]),
                    wins1=int(data["P1"]),
                    draws=int(data.get("DRAW", "0")),
                    time_sec=float(data["TIME"].rstrip("s")),
                )

        # 如果没找到 RESULT 行，尝试从 stderr 提取
        stderr_tail = "\n".join(result.stderr.split("\n")[-5:])
        raise ValueError(
            f"无法解析结果\nstdout: {result.stdout[:300]}\nstderr: {stderr_tail}"
        )
    finally:
        try:
            os.unlink(temp_path)
        except OSError:
            pass


# ─── 默认参赛阵容 ───

DEFAULT_ENTRIES = [
    AiEntry(type="strategy",   depth=0, use_ml_eval=False, name="策略"),
    AiEntry(type="mcts",       depth=2, use_ml_eval=False, name="MCTS d=2"),
    AiEntry(type="alphabeta",  depth=2, use_ml_eval=True,  name="AB d=2 ML"),
    AiEntry(type="pvs",        depth=2, use_ml_eval=True,  name="PVS d=2 ML"),
    AiEntry(type="alphabeta",  depth=2, use_ml_eval=False, name="AB d=2 手写"),
    AiEntry(type="pvs",        depth=2, use_ml_eval=False, name="PVS d=2 手写"),
    AiEntry(type="alphabeta",  depth=3, use_ml_eval=True,  name="AB d=3 ML"),
    AiEntry(type="pvs",        depth=3, use_ml_eval=True,  name="PVS d=3 ML"),
]


def header(text: str, style: str = "cyan") -> Panel:
    return Panel.fit(Text(text, style=f"bold {style}"), border_style=style)


def build_tournament_table(entries: list[tuple[int, AiEntry]],
                           results: dict[str, dict[str, MatchupResult]]) -> Table:
    """构建排行榜表格"""
    n = len(entries)

    # 汇总每个 AI 的总战绩
    scores: dict[str, dict] = {}
    for _, entry in entries:
        label = entry.label()
        scores[label] = {"wins": 0, "losses": 0, "draws": 0, "games": 0}

    for i in range(n):
        for j in range(i + 1, n):
            p0 = entries[i][1].label()
            p1 = entries[j][1].label()
            r = results.get(p0, {}).get(p1)
            if not r:
                continue
            scores[p0]["wins"]   += r.wins0
            scores[p0]["losses"] += r.wins1
            scores[p0]["draws"]  += r.draws
            scores[p0]["games"]  += r.games
            scores[p1]["wins"]   += r.wins1
            scores[p1]["losses"] += r.wins0
            scores[p1]["draws"]  += r.draws
            scores[p1]["games"]  += r.games

    # 按胜率排序
    sorted_ais = sorted(
        scores.items(),
        key=lambda x: (x[1]["wins"] / max(x[1]["games"], 1), x[1]["wins"]),
        reverse=True,
    )

    medal = {0: "🥇", 1: "🥈", 2: "🥉"}

    table = Table(
        title="🏆 排行榜",
        box=box.ROUNDED,
        title_style="bold cyan",
        border_style="cyan",
    )
    table.add_column("排名", width=4, style="cyan", no_wrap=True)
    table.add_column("AI", style="white", no_wrap=True)
    table.add_column("  胜", style="green", justify="right")
    table.add_column("  负", style="red", justify="right")
    table.add_column("  平", style="yellow", justify="right")
    table.add_column(" 总", style="white", justify="right")
    table.add_column(" 胜率", justify="right")

    for rank, (label, s) in enumerate(sorted_ais):
        rank_str = medal.get(rank, f"  {rank + 1}")
        win_rate = s["wins"] / max(s["games"], 1) * 100
        bar_len = int(win_rate / 5)
        bar = "█" * bar_len + "░" * (20 - bar_len)

        table.add_row(
            rank_str,
            f"{label} {bar}",
            str(s["wins"]),
            str(s["losses"]),
            str(s["draws"]),
            str(s["games"]),
            f"[bold]{win_rate:.1f}%[/bold]",
        )

    return table


def build_pairwise_table(entries: list[tuple[int, AiEntry]],
                          results: dict[str, dict[str, MatchupResult]]) -> Table:
    """全网状对战结果矩阵"""
    n = len(entries)
    labels = [e[1].label() for e in entries]

    table = Table(
        title="📊 对战矩阵",
        box=box.ROUNDED,
        title_style="bold cyan",
        border_style="cyan",
    )
    table.add_column("AI ↓ 对手 →", style="cyan", width=14, no_wrap=True)

    # 用缩略标签
    abbrev = {}
    for label in labels:
        parts = label.split()
        short = parts[0] if len(parts) <= 2 else f"{parts[0][:3]} {parts[2][:4]}"
        abbrev[label] = short

    for label in labels:
        table.add_column(abbrev[label], justify="center", width=8)

    for i, label_i in enumerate(labels):
        row_cells = [label_i]
        for j, label_j in enumerate(labels):
            if i == j:
                row_cells.append("[dim]·[/dim]")
            else:
                key_i, key_j = (label_i, label_j) if i < j else (label_j, label_i)
                r = results.get(key_i, {}).get(key_j)
                if r:
                    if i < j:
                        w, l = r.wins0, r.wins1
                    else:
                        w, l = r.wins1, r.wins0
                    total = r.games
                    pct = w / max(total, 1) * 100
                    color = "green" if pct > 50 else ("red" if pct < 50 else "yellow")
                    row_cells.append(f"[{color}]{w}[dim]:[/dim]{l}[/{color}]")
                else:
                    row_cells.append("[dim]?[/dim]")
        table.add_row(*row_cells)

    return table


def build_details_table(results: list[MatchupResult]) -> Table:
    """各对战详情"""
    table = Table(
        title="📋 对战详情",
        box=box.SIMPLE,
        title_style="bold cyan",
        border_style="dim",
    )
    table.add_column("AI 0", style="cyan", no_wrap=True)
    table.add_column("", style="dim", width=3)
    table.add_column("AI 1", style="magenta", no_wrap=True)
    table.add_column("比分", justify="center", width=12)
    table.add_column("局数", justify="right", width=5)
    table.add_column("耗时", justify="right", width=8)
    table.add_column("均时", justify="right", width=8)

    sorted_results = sorted(
        results,
        key=lambda r: r.wins0 / max(r.games, 1),
        reverse=True,
    )

    for r in sorted_results:
        score_color = "green" if r.wins0 > r.wins1 else ("red" if r.wins1 > r.wins0 else "yellow")
        ratio = f"[{score_color}]{r.wins0}[dim]:[/dim]{r.wins1}[/{score_color}]"
        if r.draws > 0:
            ratio += f" [dim](平{r.draws})[/dim]"
        per_game = r.time_sec / max(r.games, 1)
        table.add_row(
            f"[bold]{r.ai0_name}[/bold]",
            "vs",
            f"[bold]{r.ai1_name}[/bold]",
            ratio,
            str(r.games),
            f"{r.time_sec:.1f}s",
            f"{per_game:.2f}s",
        )

    return table


def print_entries(console: Console, entries: list[tuple[int, AiEntry]]):
    """打印参赛名单"""
    table = Table(
        title="🤖 参赛 AI",
        box=box.SIMPLE,
        title_style="bold cyan",
        border_style="dim",
    )
    table.add_column("#", style="dim", width=3)
    table.add_column("AI 名称", style="white", no_wrap=True)
    table.add_column("算法", style="cyan")
    table.add_column("深度", style="yellow", justify="right")
    table.add_column("评估", style="magenta")

    for idx, entry in entries:
        alg = AI_ALGORITHMS.get(entry.type, entry.type)
        depth_s = str(entry.depth) if entry.depth > 0 else "—"
        eval_s = "ML" if entry.use_ml_eval else "手写"
        if entry.type == "strategy":
            eval_s = "—"
            depth_s = "—"
        table.add_row(str(idx + 1), entry.label(), alg, depth_s, eval_s)

    console.print(table)


# ─── 命令行参数 ───

def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="♟ 连锁棋 AI 战力测试",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=(
            "示例:\n"
            "  %(prog)s                          # 默认 7x7, 每对 20 局\n"
            "  %(prog)s 9                        # 9x9\n"
            "  %(prog)s 7 50                     # 7x7, 每对 50 局\n"
            "  %(prog)s 7 30 --rebuild           # 强制重构建\n"
            "  %(prog)s 7 20 --players 0 2 3     # 只跑指定选手(从0开始)\n"
        ),
    )
    parser.add_argument("size", nargs="?", type=int, default=7,
                        help="棋盘大小 (5-19, 默认 7)")
    parser.add_argument("games", nargs="?", type=int, default=20,
                        help="每对局数 (默认 20)")
    parser.add_argument("--rebuild", action="store_true",
                        help="强制重新构建 Rust 二进制")
    parser.add_argument("--players", type=int, nargs="+", default=None,
                        help="指定参赛选手序号 (从0开始, 不指定则全部)")
    return parser.parse_args()


# ─── 主流程 ───

def main():
    console = Console()
    args = parse_args()

    size = max(5, min(19, args.size))
    games_per = max(1, args.games)

    # 筛选参赛者
    all_entries = list(enumerate(DEFAULT_ENTRIES))
    if args.players is not None:
        entries = [(i, DEFAULT_ENTRIES[i]) for i in args.players
                    if 0 <= i < len(DEFAULT_ENTRIES)]
        if not entries:
            console.print("[red]❌ 无效的 --players 参数[/red]")
            sys.exit(1)
    else:
        entries = all_entries

    # ─── 打印 banner ───
    console.print()
    console.print(header("♟ 连锁棋 AI 战力测试", "cyan"))
    console.print()

    # ─── 打印配置 ───
    console.print(Panel.fit(
        f"[bold]棋盘[/bold]  {size}×{size}    "
        f"[bold]每对[/bold]  {games_per} 局    "
        f"[bold]选手[/bold]  {len(entries)} 位",
        border_style="dim",
    ))
    console.print()
    print_entries(console, entries)
    console.print()

    # 统计
    total_pairs = len(entries) * (len(entries) - 1) // 2
    total_games = total_pairs * games_per
    total_time_est = total_games * 0.3  # rough estimate per game
    console.print(
        f"  总对局: [bold]{total_games}[/bold] 局 ({total_pairs} 对)"
        f"  · 预估 ~{total_time_est:.0f}s"
    )
    console.print()

    # ─── 构建 ───
    binary = build_binary(args.rebuild)
    console.print(f"  [green]✓[/green] 二进制就绪: [dim]{binary}[/dim]")
    console.print()

    # ─── 开始对战 ───
    console.print(header("⚔️  对战进行中…", "yellow"))
    console.print()

    results: dict[str, dict[str, MatchupResult]] = {}
    matchup_list: list[MatchupResult] = []

    progress = tqdm(
        total=total_pairs,
        desc="对局中",
        unit="pair",
        bar_format=(
            "{l_bar}{bar:30}{r_bar}"
        ),
        ncols=80,
        leave=True,
        colour="cyan",
    )

    for i in range(len(entries)):
        for j in range(i + 1, len(entries)):
            entry_i = entries[i][1]
            entry_j = entries[j][1]
            label_i = entry_i.label()
            label_j = entry_j.label()

            # 更新进度条描述
            short_i = label_i[:16]
            short_j = label_j[:16]
            progress.set_description(f"{short_i} vs {short_j}")

            result = run_matchup(binary, entry_i, entry_j, size, games_per)

            # 双向存储，方便查询
            if label_i not in results:
                results[label_i] = {}
            if label_j not in results:
                results[label_j] = {}
            results[label_i][label_j] = result
            results[label_j][label_i] = result
            matchup_list.append(result)

            progress.update(1)

    progress.close()

    # ─── 结果输出 ───
    console.print()
    console.print(header("✅ 测试完成!", "green"))
    console.print()

    # 排行榜
    console.print(build_tournament_table(entries, results))
    console.print()

    # 对战矩阵
    console.print(build_pairwise_table(entries, results))
    console.print()

    # 各对战详情
    console.print(build_details_table(matchup_list))
    console.print()

    # 统计汇总
    total_elapsed = sum(r.time_sec for r in matchup_list)
    total_played = sum(r.games for r in matchup_list)
    console.print(Panel.fit(
        f"总耗时: [bold]{total_elapsed:.1f}s[/bold]  "
        f"总对局: [bold]{total_played}[/bold]  "
        f"平均: [bold]{total_elapsed / max(total_played, 1):.2f}s[/bold]/局",
        border_style="dim",
    ))
    console.print()


if __name__ == "__main__":
    main()
