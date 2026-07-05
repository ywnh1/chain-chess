#!/home/ywnh1/Programs/game/venv/bin/python3
"""连锁棋 - FastAPI WebSocket Game Server"""
import json, hashlib, os, asyncio, datetime, sys
from fastapi import FastAPI, WebSocket, WebSocketDisconnect
from fastapi.responses import HTMLResponse
from fastapi.staticfiles import StaticFiles

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
STATIC_DIR = os.path.join(BASE_DIR, "static")
LOG_DIR = os.path.join(BASE_DIR, "logs")

app = FastAPI(title="连锁棋")
app.mount("/static", StaticFiles(directory=STATIC_DIR), name="static")

# ─── logging ───
def log(msg: str, level: str = "INFO"):
    ts = datetime.datetime.now().strftime("%H:%M:%S.%f")[:11]
    line = f"[{ts}] [{level}] {msg}"
    print(line, file=sys.stderr if level == "ERROR" else sys.stdout)
    try:
        os.makedirs(LOG_DIR, exist_ok=True)
        with open(os.path.join(LOG_DIR, "server.log"), "a") as f:
            f.write(line + "\n")
    except OSError:
        pass

# ─── game logic ───
def capacity(i, j, size):
    return 4

def neighbors(i, j, size):
    res = []
    if i > 0: res.append((i - 1, j))
    if i < size - 1: res.append((i + 1, j))
    if j > 0: res.append((i, j - 1))
    if j < size - 1: res.append((i, j + 1))
    return res

def neighbors8(i, j, size):
    res = []
    for di in (-1, 0, 1):
        for dj in (-1, 0, 1):
            if di == 0 and dj == 0: continue
            ni, nj = i + di, j + dj
            if 0 <= ni < size and 0 <= nj < size:
                res.append((ni, nj))
    return res

def has_any_cells(board, player):
    for row in board:
        for c in row:
            if c["owner"] == player:
                return True
    return False

def is_near_any(board, size, x, y):
    for nx, ny in neighbors8(x, y, size):
        if board[nx][ny]["owner"] is not None:
            return True
    return False

def make_board(size):
    return [[{"owner": None, "count": 0} for _ in range(size)] for _ in range(size)]

def process_click(board, size, x, y, player):
    cell = board[x][y]
    if cell["owner"] is None:
        first = not has_any_cells(board, player)
        cell["owner"] = player
        cell["count"] = 3 if first else 1
    elif cell["owner"] == player:
        cell["count"] += 1
    else:
        return set()
    before = set()
    for row in board:
        for c in row:
            if c["owner"] is not None:
                before.add(c["owner"])
    chain = [(x, y)]
    while chain:
        cx, cy = chain.pop(0)
        cell = board[cx][cy]
        cap = capacity(cx, cy, size)
        if cell["count"] >= cap:
            cell["count"] = 0; cell["owner"] = None
            for nx, ny in neighbors(cx, cy, size):
                nc = board[nx][ny]
                nc["owner"] = player; nc["count"] += 1
                chain.append((nx, ny))
    after = set()
    for row in board:
        for c in row:
            if c["owner"] is not None:
                after.add(c["owner"])
    return before - after

# ─── data store ───
class GameRoom:
    def __init__(self, room_id, room_name, password, size, max_players):
        self.room_id = room_id
        self.room_name = room_name
        self.password = password
        self.size = size
        self.max_players = max_players
        self.board = make_board(size)
        self.current_player = 0
        self.player_queue = []
        self.connections = {}
        self.eliminated = set()

    @property
    def current_players(self):
        return len(self.player_queue)

    def to_list_item(self):
        return {
            "roomId": self.room_id,
            "currentPlayers": self.current_players,
            "meta": {
                "roomName": self.room_name,
                "size": self.size,
                "maxPlayers": self.max_players,
                "password": self.password,
            }
        }

    def board_data(self, total_users=0):
        return {
            "board": [[{"owner": c["owner"], "count": c["count"]} for c in row] for row in self.board],
            "current": self.current_player,
            "size": self.size,
            "meta": {
                "currentPlayers": self.current_players,
                "maxPlayers": self.max_players,
                "roomName": self.room_name,
            },
            "totalUsers": total_users,
        }

    async def broadcast(self, data, exclude=None):
        payload = json.dumps(data)
        for pidx, conn in list(self.connections.items()):
            if pidx == exclude: continue
            try:
                await conn.send_text(payload)
            except Exception:
                pass

class SaveData:
    def __init__(self, save_id, board, size, players, remark, password_hash, create_time):
        self.save_id = save_id
        self.board = board
        self.size = size
        self.players = players
        self.remark = remark
        self.password_hash = password_hash
        self.create_time = create_time

    @property
    def display_name(self):
        return self.remark or f"存档 {self.save_id}"

    def to_list_item(self):
        return {
            "id": self.save_id,
            "displayName": self.display_name,
            "size": self.size,
            "players": self.players,
            "createTime": int(self.create_time * 1000),
            "hasPassword": bool(self.password_hash),
        }

rooms = {}
saves = {}
next_room_id = 1
next_save_id = 1
global_total_users = 0

# ─── websocket endpoint ───
@app.websocket("/ws")
async def websocket_endpoint(ws: WebSocket):
    global global_total_users, next_room_id, next_save_id
    await ws.accept()
    global_total_users += 1
    peer = f"{ws.client.host}:{ws.client.port}" if ws.client else "?"
    log(f"WS 连接 [{peer}] (在线: {global_total_users})")

    current_room = None
    player_index = None

    try:
        while True:
            raw = await ws.receive_text()
            try:
                data = json.loads(raw)
            except json.JSONDecodeError:
                continue

            msg_type = data.get("type", "")

            if msg_type == "getRoomList":
                await ws.send_text(json.dumps({
                    "type": "roomList",
                    "list": [r.to_list_item() for r in rooms.values()],
                }))
                continue

            if msg_type == "getSaveList":
                await ws.send_text(json.dumps({
                    "type": "saveList",
                    "list": [s.to_list_item() for s in saves.values()],
                }))
                continue

            if msg_type == "createRoom":
                size = int(data.get("size", 7))
                players = int(data.get("players", 2))
                room_name = data.get("roomName", "我的房间")
                password = data.get("password", "")
                if size < 5 or size > 19:
                    await ws.send_text(json.dumps({"type": "error", "msg": "棋盘大小 5-19"}))
                    continue
                if players < 2 or players > 7:
                    await ws.send_text(json.dumps({"type": "error", "msg": "玩家数 2-7"}))
                    continue

                rid = next_room_id; next_room_id += 1
                room = GameRoom(rid, room_name, password, size, players)
                rooms[rid] = room
                current_room = room; player_index = 0
                room.player_queue.append(0); room.connections[0] = ws
                await ws.send_text(json.dumps(room.board_data(global_total_users)))
                log(f"房间 {rid} 创建: {room_name} {size}x{size} {players}人")
                continue

            if msg_type == "join":
                rid = data.get("room")
                join_pwd = data.get("joinPassword", "")
                if rid not in rooms:
                    await ws.send_text(json.dumps({"type": "error", "msg": "房间不存在"}))
                    continue
                room = rooms[rid]
                if room.password and room.password != join_pwd:
                    await ws.send_text(json.dumps({"type": "error", "msg": "密码错误"}))
                    continue
                if room.current_players >= room.max_players:
                    await ws.send_text(json.dumps({"type": "error", "msg": "房间已满"}))
                    continue

                pidx = 0
                while pidx in room.connections:
                    pidx += 1
                current_room = room; player_index = pidx
                room.player_queue.append(pidx); room.connections[pidx] = ws
                await ws.send_text(json.dumps(room.board_data(global_total_users)))
                await room.broadcast(room.board_data(global_total_users), exclude=pidx)
                continue

            if msg_type == "click":
                if current_room is None or player_index is None:
                    await ws.send_text(json.dumps({"type": "error", "msg": "未加入房间"}))
                    continue
                if current_room.current_player != player_index:
                    await ws.send_text(json.dumps({"type": "error", "msg": "还没轮到你"}))
                    continue

                x, y = int(data["x"]), int(data["y"])
                if x < 0 or x >= current_room.size or y < 0 or y >= current_room.size:
                    continue
                cell = current_room.board[x][y]
                if cell["owner"] is not None and cell["owner"] != player_index:
                    continue

                # 落子规则校验
                if has_any_cells(current_room.board, player_index):
                    if cell["owner"] != player_index:
                        await ws.send_text(json.dumps({"type": "error", "msg": "只能点击自己的棋子"}))
                        continue
                else:
                    if cell["owner"] is not None:
                        await ws.send_text(json.dumps({"type": "error", "msg": "不能抢占别人的格子"}))
                        continue
                    if is_near_any(current_room.board, current_room.size, x, y):
                        await ws.send_text(json.dumps({"type": "error", "msg": "不能在已有棋子旁边落子"}))
                        continue

                elim = process_click(current_room.board, current_room.size, x, y, player_index)
                for e in elim:
                    current_room.eliminated.add(e)
                    if e in current_room.connections:
                        try:
                            await current_room.connections[e].send_text(
                                json.dumps({"type": "error", "msg": "你已被淘汰"}))
                        except Exception:
                            pass

                # 存活判定
                alive = [p for p in current_room.player_queue
                         if p in current_room.connections
                         and p not in current_room.eliminated]

                if len(alive) <= 1:
                    winner = alive[0] if alive else None
                    for pidx, c in list(current_room.connections.items()):
                        try:
                            bd = current_room.board_data(global_total_users)
                            bd["gameOver"] = True; bd["winner"] = winner
                            await c.send_text(json.dumps(bd))
                        except Exception:
                            pass
                else:
                    if player_index in alive:
                        idx = alive.index(player_index)
                        current_room.current_player = alive[(idx + 1) % len(alive)]
                    else:
                        if alive:
                            current_room.current_player = alive[0]
                    for pidx, c in list(current_room.connections.items()):
                        try:
                            await c.send_text(json.dumps(current_room.board_data(global_total_users)))
                        except Exception:
                            pass
                continue

            if msg_type == "reset":
                if current_room is None: continue
                new_size = int(data.get("size", current_room.size))
                new_players = int(data.get("players", current_room.max_players))
                if new_size < 5 or new_size > 19: continue
                if new_players < 2 or new_players > 7: continue
                current_room.size = new_size; current_room.max_players = new_players
                current_room.board = make_board(new_size)
                current_room.current_player = 0; current_room.eliminated = set()
                for pidx, c in list(current_room.connections.items()):
                    try:
                        await c.send_text(json.dumps(current_room.board_data(global_total_users)))
                    except Exception:
                        pass
                continue

            if msg_type == "saveGame":
                if current_room is None or player_index is None: continue
                remark = data.get("remark", "")
                save_password = data.get("savePassword", "")
                pw_hash = hashlib.sha256(save_password.encode()).hexdigest() if save_password else ""
                sid = next_save_id; next_save_id += 1
                sd = SaveData(sid,
                    board=[[{"owner": c["owner"], "count": c["count"]} for c in row] for row in current_room.board],
                    size=current_room.size, players=current_room.max_players,
                    remark=remark, password_hash=pw_hash,
                    create_time=asyncio.get_event_loop().time())
                saves[sid] = sd
                await ws.send_text(json.dumps({"type": "saveSuccess", "msg": f"存档成功 (ID: {sid})"}))
                continue

            if msg_type == "restoreSave":
                if current_room is None: continue
                sid = data.get("saveId")
                save_password = data.get("savePassword", "")
                if sid not in saves:
                    await ws.send_text(json.dumps({"type": "loadError", "msg": "存档不存在"}))
                    continue
                sd = saves[sid]
                if sd.password_hash:
                    pw_hash = hashlib.sha256(save_password.encode()).hexdigest()
                    if pw_hash != sd.password_hash:
                        await ws.send_text(json.dumps({"type": "loadError", "msg": "密码错误"}))
                        continue
                current_room.board = [[{"owner": c["owner"], "count": c["count"]} for c in row] for row in sd.board]
                current_room.size = sd.size; current_room.max_players = sd.players
                current_room.current_player = 0
                for pidx, c in list(current_room.connections.items()):
                    try:
                        await c.send_text(json.dumps(current_room.board_data(global_total_users)))
                    except Exception:
                        pass
                await ws.send_text(json.dumps({"type": "loadSuccess", "msg": "存档已恢复"}))
                continue

            if msg_type == "deleteSave":
                sid = data.get("saveId")
                save_password = data.get("savePassword", "")
                if sid not in saves: continue
                sd = saves[sid]
                if sd.password_hash:
                    pw_hash = hashlib.sha256(save_password.encode()).hexdigest()
                    if pw_hash != sd.password_hash:
                        await ws.send_text(json.dumps({"type": "error", "msg": "密码错误"}))
                        continue
                del saves[sid]
                await ws.send_text(json.dumps({"type": "saveDeleted", "msg": "存档已删除"}))
                continue

            if msg_type == "exitRoom":
                if current_room is not None and player_index is not None:
                    if player_index in current_room.connections:
                        del current_room.connections[player_index]
                    if player_index in current_room.player_queue:
                        current_room.player_queue.remove(player_index)
                    if not current_room.connections and current_room.room_id in rooms:
                        log(f"房间 {current_room.room_id} 已空，删除")
                        del rooms[current_room.room_id]
                    else:
                        await current_room.broadcast(current_room.board_data(global_total_users))
                current_room = None; player_index = None
                continue

    except WebSocketDisconnect:
        pass
    except Exception as e:
        log(f"WS 异常 [{peer}]: {e}", "ERROR")
    finally:
        global_total_users -= 1
        room_info = f"房间 {current_room.room_id} P{player_index}" if current_room and player_index is not None else "未加入"
        log(f"WS 断开 [{peer}] [{room_info}] (在线: {global_total_users})")
        if current_room is not None and player_index is not None:
            if player_index in current_room.connections:
                del current_room.connections[player_index]
            if player_index in current_room.player_queue:
                current_room.player_queue.remove(player_index)
            if not current_room.connections and current_room.room_id in rooms:
                log(f"房间 {current_room.room_id} 已空，删除")
                del rooms[current_room.room_id]
            else:
                try:
                    await current_room.broadcast(current_room.board_data(global_total_users))
                except Exception:
                    pass

# ─── HTTP ───
@app.get("/")
async def index():
    with open(os.path.join(STATIC_DIR, "index.html"), "r", encoding="utf-8") as f:
        return HTMLResponse(f.read())

if __name__ == "__main__":
    import uvicorn
    import logging
    LOG_FILE = os.path.join(BASE_DIR, "log.log")

    # uvicorn 日志同时输出到文件和 stderr
    log_config = {
        "version": 1,
        "disable_existing_loggers": False,
        "formatters": {
            "default": {"format": "%(asctime)s %(levelname)s %(message)s", "datefmt": "%Y-%m-%d %H:%M:%S"},
            "access": {"format": "%(asctime)s %(levelname)s %(message)s", "datefmt": "%Y-%m-%d %H:%M:%S"},
        },
        "handlers": {
            "default": {
                "formatter": "default", "class": "logging.StreamHandler", "stream": "ext://sys.stderr",
            },
            "access": {
                "formatter": "access", "class": "logging.StreamHandler", "stream": "ext://sys.stderr",
            },
            "file": {
                "formatter": "default", "class": "logging.handlers.RotatingFileHandler",
                "filename": LOG_FILE, "maxBytes": 10485760, "backupCount": 3,
            },
            "file_access": {
                "formatter": "access", "class": "logging.handlers.RotatingFileHandler",
                "filename": LOG_FILE, "maxBytes": 10485760, "backupCount": 3,
            },
        },
        "loggers": {
            "uvicorn": {"handlers": ["default", "file"], "level": "INFO", "propagate": False},
            "uvicorn.error": {"handlers": ["default", "file"], "level": "INFO", "propagate": False},
            "uvicorn.access": {"handlers": ["access", "file_access"], "level": "INFO", "propagate": False},
        },
    }

    log("🎮 连锁棋服务器启动")
    log(f"📍 http://0.0.0.0:8000  (静态: {STATIC_DIR})")
    log(f"🌐 ws://0.0.0.0:8000/ws")
    log(f"📋 日志: {LOG_FILE}")
    uvicorn.run(app, host="0.0.0.0", port=8000, log_config=log_config)
