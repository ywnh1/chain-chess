#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""AI 实力一键测试（tauri 端引擎）

用 Rust 基准引擎 ai_bench 逐局模拟 AI 对局，实时进度条，最后输出 Markdown 报告。

用法（先编译引擎，见 README 说明）:
    cargo build --release --bin ai_bench        # 在 tauri/src-tauri 下

    python3 tests/ai_strength.py                                   # 默认 7×7、每组合 6 局、4 算法混战
    python3 tests/ai_strength.py --mode duel                       # 应用内评测页口径：432 局两两对阵
    python3 tests/ai_strength.py --mode duel --shapes all          # 加上 5 种棋盘形状（含异形）
    python3 tests/ai_strength.py --quick                           # 快速冒烟：1 局/组合、形状全测
    python3 tests/ai_strength.py --no-ml                           # 手写评估函数对照
    python3 tests/ai_strength.py --games 10 --size 9
    python3 tests/ai_strength.py --mode duel --algs strategy,alphabeta
    python3 tests/ai_strength.py --caps 3,4 --borders default,wrap
    python3 tests/ai_strength.py --out report.md --out-json data.json

注意：引擎的 CapMode 只接受 3/4/5/random；旧的「混合（每格固定阈值）」
已从产品与引擎中移除，传 mixed 会让 ai_bench 配置解析失败。
"""
import argparse
import hashlib
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
MODEL_PATH = os.path.normpath(os.path.join(HERE, '..', '..', 'engine', 'xgb_model_board.json'))

try:
    from tqdm import tqdm
except ImportError:
    tqdm = None

ALG_LABEL = {'strategy': '策略', 'alphabeta': 'A-B', 'pvs': 'PVS', 'mcts': 'MCTS'}
BM_LABEL = {'default': '默认', 'wrap': '回环', 'bounce': '反弹', 'degrade': '降级'}
CAP_LABEL = {'3': '速爆3', '4': '标准4', '5': '重炮5'}
SHAPE_LABEL = {
    'square': '正方形', 'rect': '长方形', 'holes': '挖洞',
    'ring': '口字', 'cross': '十字',
}
ALL_BORDERS = ['default', 'wrap', 'bounce', 'degrade']
ALL_CAPS = ['3', '4', '5']
ALL_ALGS = ['strategy', 'alphabeta', 'pvs', 'mcts']
ALL_SHAPES = ['square', 'rect', 'holes', 'ring', 'cross']


def parse_args():
    ap = argparse.ArgumentParser(description='AI 实力一键测试（tauri 端引擎）')
    ap.add_argument('--bin', default=DEFAULT_BIN, help='ai_bench 二进制路径')
    ap.add_argument('--size', type=int, default=7, help='棋盘大小（默认 7）')
    ap.add_argument('--games', type=int, default=6, help='每组合对局数（默认 6）')
    ap.add_argument('--shapes', default='square',
                    help='棋盘形状，逗号分隔（square/rect/holes/ring/cross），all=全部。默认 square')
    ap.add_argument('--quick', action='store_true',
                    help='快速模式：两两对阵、每组合 1 局、全部形状（360 局，覆盖最广）')
    ap.add_argument('--mode', choices=['ffa', 'duel'], default='ffa', help='ffa=多算法混战(默认)，duel=两两对阵')
    ap.add_argument('--borders', default=','.join(ALL_BORDERS), help='边界模式列表，逗号分隔')
    ap.add_argument('--caps', default=','.join(ALL_CAPS), help='爆炸阈值列表，逗号分隔（3/4/5）')
    ap.add_argument('--algs', default=','.join(ALL_ALGS), help='算法列表，逗号分隔（strategy/alphabeta/pvs/mcts）')
    ap.add_argument('--depth', type=int, default=2, help='A-B/PVS 搜索深度（默认 2）')
    ap.add_argument('--mcts-depth', type=int, default=1, help='MCTS 深度（默认 1，迭代=depth*800）')
    ap.add_argument('--random', type=int, default=5, help='搜索随机刻度 %%（默认 5）')
    ap.add_argument('--ml', dest='use_ml', action='store_true', default=True,
                    help='A-B / PVS 用 ML 模型评估（默认）')
    ap.add_argument('--no-ml', dest='use_ml', action='store_false',
                    help='A-B / PVS 改用手写评估，做对照')
    ap.add_argument('--tag', default='', help='报告标题后缀，例如 “ML 模型” / “手写评估”')
    ap.add_argument('--seed', type=int, default=0, help='对局种子偏移（默认 0）')
    ap.add_argument('--from-json', default='',
                    help='不跑对局，直接从已有 JSON 重建报告（改报告格式时用）')
    ap.add_argument('--out', default=os.path.join(HERE, 'ai_report.md'), help='Markdown 报告输出路径')
    ap.add_argument('--out-json', default=os.path.join(HERE, 'ai_report.json'), help='对局数据 JSON 输出路径')
    return ap.parse_args()


def model_fingerprint():
    """模型文件指纹：换个模型跑出来的胜率才可比。"""
    try:
        st = os.stat(MODEL_PATH)
        with open(MODEL_PATH, 'rb') as f:
            digest = hashlib.sha256(f.read()).hexdigest()[:12]
        mtime = datetime.fromtimestamp(st.st_mtime).strftime('%Y-%m-%d')
        return f'{digest} · {st.st_size / 1048576:.1f} MB · 文件时间 {mtime}'
    except OSError:
        return '未找到 engine/xgb_model_board.json'


def resolve_shapes(a):
    """--shapes 解析：all → 全部；逗号分隔；校验未知值。"""
    raw = [x.strip() for x in a.shapes.split(',') if x.strip()]
    if 'all' in raw:
        return list(ALL_SHAPES)
    bad = [x for x in raw if x not in ALL_SHAPES]
    if bad:
        raise SystemExit(f'未知形状: {", ".join(bad)}（可选: {", ".join(ALL_SHAPES)} 或 all）')
    return raw or ['square']


def build_config(a):
    return {
        'size': a.size,
        'games': a.games,
        'mode': a.mode,
        'shapes': resolve_shapes(a),
        'borders': [x.strip() for x in a.borders.split(',') if x.strip()],
        'caps': [x.strip() for x in a.caps.split(',') if x.strip()],
        'algs': [x.strip() for x in a.algs.split(',') if x.strip()],
        'depth': a.depth,
        'mctsDepth': a.mcts_depth,
        'randomScale': a.random,
        'useMlEval': a.use_ml,
        'seed': a.seed,
    }


def run_bench(bin_path, config):
    """启动 ai_bench，逐行读取结果并实时刷新进度条，返回 (meta, results)。

    进度信息写 stderr，stdout 只留最终报告，`... | tee` 时也能看到进度。
    """
    proc = subprocess.Popen(
        [bin_path], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
        text=True, bufsize=1, encoding='utf-8', errors='replace',
    )
    meta = None
    results = []
    done = False
    bar = None
    last_pct = -1
    try:
        proc.stdin.write(json.dumps(config) + '\n')
        proc.stdin.close()
        if tqdm is not None:
            bar = tqdm(total=None, desc='AI 对局', unit='局',
                       dynamic_ncols=True, file=sys.stderr, leave=True)
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
                    if bar is not None:
                        bar.total = meta['total']
                        bar.refresh()
                elif obj['meta'].get('done'):
                    done = True
            elif 'fatal' in obj:
                if bar is not None:
                    bar.close()
                raise RuntimeError(obj['fatal'])
            else:
                results.append(obj)
                total = (meta or {}).get('total') or 0
                if bar is not None:
                    bar.update(1)
                elif total:
                    pct = len(results) * 100 // total
                    if pct != last_pct and pct % 5 == 0:
                        last_pct = pct
                        print(f'\r  {len(results)}/{total} 局（{pct}%）', end='', file=sys.stderr, flush=True)
        proc.wait()
        if bar is not None:
            bar.close()
        elif results:
            print('', file=sys.stderr)
        if not done:
            raise RuntimeError('ai_bench 提前退出（exit=%s）' % proc.returncode)
        return meta, results
    finally:
        if bar is not None:
            try:
                bar.close()
            except Exception:
                pass
        if proc.poll() is None:
            proc.kill()


def pct(n, d):
    return (n / d * 100) if d else 0.0


def fmt_pct(n, d):
    return f'{pct(n, d):.1f}%'


def main():
    a = parse_args()
    if a.quick:
        a.games = 1
        a.shapes = 'all'
        a.mode = 'duel'
    if a.from_json:
        # 复用已有数据：报告格式改了也不用重跑几千局
        with open(a.from_json, encoding='utf-8') as f:
            saved = json.load(f)
        config = saved['config']
        results = saved['results']
        total = len(results)
        a.use_ml = config.get('useMlEval', True)
        print(f'从 {a.from_json} 重建报告（{total} 局）', flush=True)
    else:
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
    shape_wins = defaultdict(lambda: defaultdict(int))   # shape -> alg -> wins
    shape_games = defaultdict(int)                        # shape -> 局数
    shape_steps = defaultdict(list)                       # shape -> steps
    shape_bm_wins = defaultdict(lambda: defaultdict(int)) # (shape,bm) -> alg -> wins
    shape_bm_games = defaultdict(int)                     # (shape,bm) -> 局数
    capped = []                                           # 打到步数上限、无人被淘汰的对局
    duel_rows = []                   # (algA, algB) -> {a,b,tie}
    duel_stat = defaultdict(lambda: {'a': 0, 'b': 0})

    players_map = {}
    for r in results:
        players = r.get('players') or []
        np = len(players)
        key = (r['bm'], r['cm'])
        cell_games[key] += 1
        steps_by_key[key].append(r['steps'])
        shape = r.get('shape') or 'square'
        shape_games[shape] += 1
        shape_steps[shape].append(r['steps'])
        shape_bm_games[(shape, r['bm'])] += 1
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
                shape_wins[shape][alg] += 1
                shape_bm_wins[(shape, r['bm'])][alg] += 1
        # 打到上限：引擎此时按"第一个存活玩家"判胜，先后手直接决定胜负，
        # 不是真实对局结果。单独统计，避免把这种局当正常胜负算进胜率。
        if r['steps'] >= 1000 and not (r.get('order') or []):
            capped.append(r)
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
    title = '# 🤖 AI 实力测试报告（tauri 端引擎）'
    if a.tag:
        title += f' · {a.tag}'
    L.append(title)
    L.append('')
    L.append(f'- **测试时间**: {datetime.now().strftime("%Y-%m-%d %H:%M:%S")}')
    if a.use_ml:
        L.append(f'- **评估函数**: ML 模型（{model_fingerprint()}）；策略 / MCTS 不使用评估函数，不受此影响')
    else:
        L.append('- **评估函数**: 手写规则（--no-ml）；策略 / MCTS 不使用评估函数，不受此影响')
    shapes = config.get('shapes') or ['square']
    shape_txt = ', '.join(SHAPE_LABEL.get(x, x) for x in shapes)
    L.append(f'- **棋盘**: {config["size"]}×{config["size"]}（基准边长）· 形状: {shape_txt} · 模式: '
             f'{"混战（每算法一名）" if config["mode"] == "ffa" else "两两对阵"}')
    L.append(f'- **组合**: {len(shapes)} 形状 × {len(config["borders"])} 边界 × {len(config["caps"])} 阈值 = '
             f'{len(shapes) * len(config["borders"]) * len(config["caps"])} 组 · 每组合 {config["games"]} 局 · 共 {total} 局')
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

    # 形状维度（只有一种形状时省略，避免报告变长）
    if len(shapes) > 1:
        L.append('## 三、分棋盘形状')
        L.append('')
        L.append('| 形状 | 局数 | 平均步数 | 胜场最多 | 胜率 |')
        L.append('|:--:|:--:|:--:|:--:|:--:|')
        for sh in shapes:
            g = shape_games.get(sh, 0)
            if g == 0:
                continue
            st = shape_steps.get(sh) or [0]
            cw = shape_wins.get(sh) or {}
            best = max(cw.items(), key=lambda kv: kv[1]) if cw else ('-', 0)
            L.append(f'| {SHAPE_LABEL.get(sh, sh)} | {g} | {sum(st) / len(st):.0f} | '
                     f'{ALG_LABEL.get(best[0], best[0])} | {fmt_pct(best[1], g)} |')
        L.append('')

        L.append('### 形状 × 边界（胜场最多者）')
        L.append('')
        L.append('| 形状 \\ 边界 | ' + ' | '.join(BM_LABEL.get(b, b) for b in config['borders']) + ' |')
        L.append('|' + '---|' * (len(config['borders']) + 1))
        for sh in shapes:
            cells = []
            for bm in config['borders']:
                g = shape_bm_games.get((sh, bm), 0)
                cw = shape_bm_wins.get((sh, bm)) or {}
                if g == 0 or not cw:
                    cells.append('-')
                else:
                    best = max(cw.items(), key=lambda kv: kv[1])
                    cells.append(f'{ALG_LABEL.get(best[0], best[0])} {fmt_pct(best[1], g)}')
            L.append(f'| {SHAPE_LABEL.get(sh, sh)} | ' + ' | '.join(cells) + ' |')
        L.append('')
        n_extra = 1
    else:
        n_extra = 0

    # 平均步数矩阵
    CN = ['三', '四', '五', '六', '七']
    sec = 0 if n_extra == 0 else 1   # 形状小节占用「三」之后的下标

    L.append(f'## {CN[0 + sec]}、平均对局步数（组合维度）')
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
    if capped:
        from collections import Counter as _C
        by_alg = _C()
        for x in capped:
            if x['winner'] is not None:
                by_alg[x['players'][x['winner']]] += 1
        L.append(f'## {CN[1 + sec]}、未分胜负的对局（打到 {1000} 步上限）')
        L.append('')
        L.append(f'共 **{len(capped)} / {total}** 局（{pct(len(capped), total):.1f}%）没有任何玩家被淘汰。')
        L.append('引擎在步数上限处按「第一个存活玩家」判胜，先后手直接决定结果，'
                 '所以这些局的胜负不是真实实力，统计时请单独看待。')
        L.append('')
        L.append('| 形状 | 边界 | 阈值 | 对阵 | 被判胜 |')
        L.append('|:--:|:--:|:--:|:--:|:--:|')
        for x in capped:
            L.append(f"| {SHAPE_LABEL.get(x['shape'], x['shape'])} | {BM_LABEL.get(x['bm'], x['bm'])} | "
                     f"cap{x['cm']} | {ALG_LABEL.get(x['players'][0], x['players'][0])} vs "
                     f"{ALG_LABEL.get(x['players'][1], x['players'][1])} | "
                     f"{ALG_LABEL.get(x['players'][x['winner']], '—') if x['winner'] is not None else '—'} |")
        L.append('')
        L.append('其中被判胜的分布：' + '、'.join(f'{ALG_LABEL.get(a, a)} {n} 局' for a, n in by_alg.most_common())
                 + '。这些局全部落在对局偏长的组合（长方形 / 口字 / 挖洞 × 慢节奏阈值），'
                   '说明长局僵持是当前规则下的真实短板，值得单独研究终结机制。')
        L.append('')
        sec += 1

    L.append(f'## {CN[1 + sec]}、简要观察')
    L.append('')
    L.append('- 综合胜率最高：**' + ALG_LABEL.get(ranked[0], ranked[0]) + '**（' + fmt_pct(wins[ranked[0]], games_by_alg[ranked[0]]) + '）')
    L.append('- 综合胜率最低：' + ALG_LABEL.get(ranked[-1], ranked[-1]) + '（' + fmt_pct(wins[ranked[-1]], games_by_alg[ranked[-1]]) + '）')
    avg_all = sum(sum(v) for v in steps_by_key.values()) / max(1, len(results))
    L.append(f'- 平均对局步数：{avg_all:.0f} 步/局')
    L.append('')

    if config['mode'] == 'duel':
        L.append(f'## {CN[2 + sec]}、两两对阵')
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
    if not a.from_json:
        with open(a.out_json, 'w', encoding='utf-8') as f:
            json.dump({'config': config, 'results': results}, f, ensure_ascii=False, indent=1)

    print('\n' + '=' * 60)
    print(f'完成！共 {total} 局，报告已写入:')
    print(f'  Markdown: {a.out}')
    print(f'  数据 JSON: {a.out_json}' if not a.from_json else '  （数据 JSON 未改动，来自 --from-json）')
    print('=' * 60)


if __name__ == '__main__':
    main()
