#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""AI 实力一键测试（tauri 端引擎）

用 Rust 基准引擎 ai_bench 逐局模拟 AI 对局，实时进度条，最后输出 Markdown 报告。

用法（先编译引擎，见 README 说明）:
    cargo build --release --bin ai_bench        # 在 tauri/src-tauri 下

    python3 tests/ai_strength.py                                   # 默认 7×7、每组合 6 局、4 算法混战
    python3 tests/ai_strength.py --games 10 --size 9
    python3 tests/ai_strength.py --mode duel --algs strategy,alphabeta
    python3 tests/ai_strength.py --caps 3,4 --borders default,wrap
    python3 tests/ai_strength.py --out report.md --out-json data.json
"""
import argparse
import json
import os
import subprocess
import sys
from collections import defaultdict
from datetime import datetime

# 管道/tee 下 stdout 默认块缓冲，导致长任务期间看不到输出；强制行缓冲逐行刷新
if hasattr(sys.stdout, 'reconfigure'):
    try:
        sys.stdout.reconfigure(line_buffering=True)
    except Exception:
        pass

HERE = os.path.dirname(os.path.abspath(__file__))
DEFAULT_BIN = os.path.normpath(os.path.join(HERE, '..', 'src-tauri', 'target', 'release', 'ai_bench'))

try:
    from tqdm import tqdm
except ImportError:
    tqdm = None

ALG_LABEL = {'strategy': '策略', 'alphabeta': 'A-B', 'pvs': 'PVS', 'mcts': 'MCTS'}
BM_LABEL = {'default': '默认', 'wrap': '回环', 'bounce': '反弹', 'degrade': '降级'}
CAP_LABEL = {'3': '速爆3', '4': '标准4', '5': '重炮5', 'mixed': '混合'}
ALL_BORDERS = ['default', 'wrap', 'bounce', 'degrade']
ALL_CAPS = ['3', '4', '5', 'mixed']
ALL_ALGS = ['strategy', 'alphabeta', 'pvs', 'mcts']


def parse_args():
    ap = argparse.ArgumentParser(description='AI 实力一键测试（tauri 端引擎）')
    ap.add_argument('--bin', default=DEFAULT_BIN, help='ai_bench 二进制路径')
    ap.add_argument('--size', type=int, default=7, help='棋盘大小（默认 7）')
    ap.add_argument('--games', type=int, default=6, help='每组合对局数（默认 6）')
    ap.add_argument('--mode', choices=['ffa', 'duel'], default='ffa', help='ffa=多算法混战(默认)，duel=两两对阵')
    ap.add_argument('--borders', default=','.join(ALL_BORDERS), help='边界模式列表，逗号分隔')
    ap.add_argument('--caps', default=','.join(ALL_CAPS), help='爆炸阈值列表，逗号分隔（3/4/5/mixed）')
    ap.add_argument('--algs', default=','.join(ALL_ALGS), help='算法列表，逗号分隔（strategy/alphabeta/pvs/mcts）')
    ap.add_argument('--depth', type=int, default=2, help='A-B/PVS 搜索深度（默认 2）')
    ap.add_argument('--mcts-depth', type=int, default=1, help='MCTS 深度（默认 1，迭代=depth*800）')
    ap.add_argument('--random', type=int, default=5, help='搜索随机刻度 %%（默认 5）')
    ap.add_argument('--seed', type=int, default=0, help='对局种子偏移（默认 0）')
    ap.add_argument('--out', default=os.path.join(HERE, 'ai_report.md'), help='Markdown 报告输出路径')
    ap.add_argument('--out-json', default=os.path.join(HERE, 'ai_report.json'), help='对局数据 JSON 输出路径')
    return ap.parse_args()


def build_config(a):
    return {
        'size': a.size,
        'games': a.games,
        'mode': a.mode,
        'borders': [x.strip() for x in a.borders.split(',') if x.strip()],
        'caps': [x.strip() for x in a.caps.split(',') if x.strip()],
        'algs': [x.strip() for x in a.algs.split(',') if x.strip()],
        'depth': a.depth,
        'mctsDepth': a.mcts_depth,
        'randomScale': a.random,
        'useMlEval': True,
        'seed': a.seed,
    }


def run_bench(bin_path, config):
    """启动 ai_bench，逐行产出 (meta, results, done)。"""
    proc = subprocess.Popen(
        [bin_path], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        text=True, bufsize=1, encoding='utf-8', errors='replace',
    )
    try:
        proc.stdin.write(json.dumps(config) + '\n')
        proc.stdin.close()
        meta = None
        results = []
        done = False
        for line in proc.stdout:
            line = line.strip()
            if not line:
                continue
            try:
                obj = json.loads(line)
            except Exception:
                continue
            if 'meta' in obj:
                if 'total' in obj['meta']:
                    meta = obj['meta']
                elif obj['meta'].get('done'):
                    done = True
            elif 'fatal' in obj:
                raise RuntimeError(obj['fatal'])
            else:
                results.append(obj)
        proc.wait()
        if not done:
            raise RuntimeError('ai_bench 提前退出（exit=%s）' % proc.returncode)
        return meta, results
    finally:
        if proc.poll() is None:
            proc.kill()


def pct(n, d):
    return (n / d * 100) if d else 0.0


def fmt_pct(n, d):
    return f'{pct(n, d):.1f}%'


def main():
    a = parse_args()
    if not os.path.exists(a.bin):
        print(f'[错误] 找不到引擎二进制: {a.bin}\n'
              f'请先编译: cd tauri/src-tauri && cargo build --release --bin ai_bench', file=sys.stderr)
        sys.exit(1)

    config = build_config(a)
    print(f'引擎: {a.bin}', flush=True)
    print(f'配置: {json.dumps(config, ensure_ascii=False)}', flush=True)

    print('启动 ai_bench ...', flush=True)
    meta, results = run_bench(a.bin, config)
    total = (meta or {}).get('total', len(results))

    # 统计
    wins = defaultdict(int)          # alg -> 胜场
    games_by_alg = defaultdict(int)
    elim_rank = defaultdict(list)    # alg -> [平均淘汰名次]（存活者名次=玩家数）
    steps_by_key = defaultdict(list) # (bm,cm) -> steps
    cell_winner = defaultdict(lambda: defaultdict(int))  # (bm,cm) -> alg -> wins
    cell_games = defaultdict(int)
    duel_rows = []                   # (algA, algB) -> {a,b,tie}
    duel_stat = defaultdict(lambda: {'a': 0, 'b': 0})

    players_map = {}
    for r in results:
        players = r.get('players') or []
        np = len(players)
        key = (r['bm'], r['cm'])
        cell_games[key] += 1
        steps_by_key[key].append(r['steps'])
        w = r['winner']
        order = r.get('order') or []
        for idx, alg in enumerate(players):
            games_by_alg[alg] += 1
            # 淘汰名次：order 里第 i 个被淘汰 → 名次 = i+1；存活者 = np
            rank = np
            for i, victim in enumerate(order):
                if players[victim] == alg:
                    rank = i + 1
                    break
            elim_rank[alg].append(rank)
            if w is not None and players[w] == alg:
                wins[alg] += 1
                cell_winner[key][alg] += 1
        if r['mode'] == 'duel' and len(players) == 2:
            aalg, balg = players[0], players[1]
            if w is not None:
                if players[w] == aalg:
                    duel_stat[(aalg, balg)]['a'] += 1
                else:
                    duel_stat[(aalg, balg)]['b'] += 1
            else:
                duel_stat[(aalg, balg)]['a'] += 0.5
                duel_stat[(aalg, balg)]['b'] += 0.5

    # ---- 生成 Markdown 报告 ----
    L = []
    L.append('# 🤖 AI 实力测试报告（tauri 端引擎）')
    L.append('')
    L.append(f'- **测试时间**: {datetime.now().strftime("%Y-%m-%d %H:%M:%S")}')
    L.append(f'- **棋盘**: {config["size"]}×{config["size"]} · 模式: '
             f'{"混战（每算法一名）" if config["mode"] == "ffa" else "两两对阵"}')
    L.append(f'- **组合**: {len(config["borders"])} 边界 × {len(config["caps"])} 阈值 = '
             f'{len(config["borders"]) * len(config["caps"])} 组 · 每组合 {config["games"]} 局 · 共 {total} 局')
    L.append(f'- **搜索**: A-B/PVS 深度 {config["depth"]} · MCTS 深度 {config["mctsDepth"]} · '
             f'随机刻度 {config["randomScale"]}% · 种子 {config["seed"]}')
    L.append('')

    # 总体排名
    L.append('## 一、算法总体实力排名')
    L.append('')
    L.append('| 排名 | 算法 | 胜场 | 胜率 | 平均淘汰名次 |')
    L.append('|:--:|:--:|:--:|:--:|:--:|')
    ranked = sorted(games_by_alg.keys(), key=lambda x: -wins[x])
    for i, alg in enumerate(ranked, 1):
        g = games_by_alg[alg]
        avg_rank = sum(elim_rank[alg]) / len(elim_rank[alg]) if elim_rank[alg] else 0
        bar = '█' * round(pct(wins[alg], g) / 5)
        L.append(f'| {i} | {ALG_LABEL.get(alg, alg)} | {wins[alg]} | {fmt_pct(wins[alg], g)} | {avg_rank:.2f} |')
    L.append('')

    # 分组合胜率矩阵
    L.append('## 二、分棋盘组合胜率（胜者算法 + 胜率）')
    L.append('')
    header = '| 边界 \\ 阈值 | ' + ' | '.join(CAP_LABEL.get(c, c) for c in config['caps']) + ' |'
    L.append(header)
    L.append('|' + '---|' * (len(config['caps']) + 1))
    for bm in config['borders']:
        cells = []
        for cm in config['caps']:
            key = (bm, cm)
            cw = cell_winner[key]
            g = cell_games[key]
            if g == 0 or not cw:
                cells.append('-')
                continue
            best = max(cw.items(), key=lambda kv: kv[1])
            cells.append(f'{ALG_LABEL.get(best[0], best[0])} {fmt_pct(best[1], g)}')
        L.append(f'| {BM_LABEL.get(bm, bm)} | ' + ' | '.join(cells) + ' |')
    L.append('')

    # 平均步数矩阵
    L.append('## 三、平均对局步数（组合维度）')
    L.append('')
    L.append(header)
    L.append('|' + '---|' * (len(config['caps']) + 1))
    for bm in config['borders']:
        cells = []
        for cm in config['caps']:
            st = steps_by_key.get((bm, cm), [])
            cells.append(f'{sum(st) / len(st):.0f}' if st else '-')
        L.append(f'| {BM_LABEL.get(bm, bm)} | ' + ' | '.join(cells) + ' |')
    L.append('')

    # 观察
    L.append('## 四、简要观察')
    L.append('')
    L.append('- 综合胜率最高：**' + ALG_LABEL.get(ranked[0], ranked[0]) + '**（' + fmt_pct(wins[ranked[0]], games_by_alg[ranked[0]]) + '）')
    L.append('- 综合胜率最低：' + ALG_LABEL.get(ranked[-1], ranked[-1]) + '（' + fmt_pct(wins[ranked[-1]], games_by_alg[ranked[-1]]) + '）')
    avg_all = sum(sum(v) for v in steps_by_key.values()) / max(1, len(results))
    L.append(f'- 平均对局步数：{avg_all:.0f} 步/局')
    L.append('')

    if config['mode'] == 'duel':
        L.append('## 五、两两对阵')
        L.append('')
        L.append('| 对阵 | 胜A | 胜B | 平局折半 |')
        L.append('|:--:|:--:|:--:|:--:|')
        for (aalg, balg), st in sorted(duel_stat.items()):
            total_g = st['a'] + st['b']
            L.append(f'| {ALG_LABEL.get(aalg, aalg)} vs {ALG_LABEL.get(balg, balg)} | '
                     f'{st["a"]:.0f} ({fmt_pct(st["a"], total_g)}) | {st["b"]:.0f} ({fmt_pct(st["b"], total_g)}) | '
                     f'{total_g - int(st["a"]) - int(st["b"])} |')
        L.append('')

    report = '\n'.join(L)
    with open(a.out, 'w', encoding='utf-8') as f:
        f.write(report)
    with open(a.out_json, 'w', encoding='utf-8') as f:
        json.dump({'config': config, 'results': results}, f, ensure_ascii=False, indent=1)

    print('\n' + '=' * 60)
    print(f'完成！共 {total} 局，报告已写入:')
    print(f'  Markdown: {a.out}')
    print(f'  数据 JSON: {a.out_json}')
    print('=' * 60)


if __name__ == '__main__':
    main()
