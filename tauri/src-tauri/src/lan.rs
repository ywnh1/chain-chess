// Copyright (c) 2026 ywnh1
// SPDX-License-Identifier: MIT

//! 局域网联机传输层（不经过任何第三方服务器）。
//!
//! 主机监听一个随机 TCP 端口，客户端直连；每条消息是一行 JSON。
//! 前端用 `lan_poll` 拉事件队列（轮询），不依赖 Tauri 事件 API ——
//! 少了 transformCallback 之类的内部细节，逻辑也更容易在浏览器里测试。
//!
//! 房间发现在 UDP `DISCOVERY_PORT` 上：客户端广播 `CHAINCHESS/1/DISCOVER`，
//! 主机回 `CHAINCHESS/1/ROOM <tcp 端口> <房间名>`。

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

const DISCOVERY_PORT: u16 = 39017;
const MAGIC: &str = "CHAINCHESS/1";
/// 一桌最多 10 人（含房主），所以客户端最多 9 个
const MAX_PEERS: usize = 9;

pub struct LanState {
    inner: Arc<Mutex<Inner>>,
}

pub(crate) struct Inner {
    /// 会话代际。每次 stop / 新建会话都 +1，后台线程带着自己的代际操作，
    /// 代际过期就直接退出——否则旧连接的读线程会在新会话里灌一个 closed，
    /// 或按旧 id 删掉新 peer 的写句柄。
    generation: u64,
    role: Option<&'static str>,
    stop: Option<Arc<AtomicBool>>,
    /// 客户端模式下的主机连接
    server: Option<TcpStream>,
    /// 主机模式下的客户端连接（id 由 accept 顺序分配）
    peers: HashMap<u32, TcpStream>,
    events: VecDeque<Value>,
    name: String,
}

impl LanState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                generation: 0,
                role: None,
                stop: None,
                server: None,
                peers: HashMap::new(),
                events: VecDeque::new(),
                name: String::new(),
            })),
        }
    }
}

impl Default for LanState {
    fn default() -> Self {
        Self::new()
    }
}

fn push(inner: &Arc<Mutex<Inner>>, ev: Value) {
    if let Ok(mut g) = inner.lock() {
        g.events.push_back(ev);
    }
}

/// 关闭当前会话：置停止位、断开所有连接、清空待处理事件。
fn shutdown(inner: &Arc<Mutex<Inner>>) {
    if let Ok(mut g) = inner.lock() {
        g.generation = g.generation.wrapping_add(1);
        if let Some(flag) = g.stop.take() {
            flag.store(true, Ordering::Relaxed);
        }
        // 必须 shutdown 而不是只 drop：读线程还阻塞在 read_line 上，
        // 关掉 socket 才能让它立刻返回 0 退出，不然每局泄漏一个线程。
        for s in g.peers.values() {
            let _ = s.shutdown(Shutdown::Both);
        }
        g.peers.clear();
        if let Some(s) = g.server.as_ref() {
            let _ = s.shutdown(Shutdown::Both);
        }
        g.server = None;
        g.role = None;
        g.events.clear();
    }
}

fn write_json(stream: &mut TcpStream, v: &Value) -> std::io::Result<()> {
    let mut line = serde_json::to_string(v).map_err(|e| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())
    })?;
    line.push('\n');
    stream.write_all(line.as_bytes())?;
    stream.flush()
}

/// 读循环：逐行读 JSON，推给前端队列；断开时通知。
/// `peer_id` 为 Some 表示这是主机侧的一条客户端连接。
fn read_loop(
    inner: Arc<Mutex<Inner>>,
    stop: Arc<AtomicBool>,
    gen: u64,
    stream: TcpStream,
    peer_id: Option<u32>,
) {
    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    // 单条连接最多读 8MB：防止对端一直发不换行的数据把内存吃满。
    // ponytail: 这是整条连接的总读取上限，不是单行上限；一局棋的消息量是
    // 几十 KB 量级，8MB 足够。升级路径：改成按行切分读取，单行超 64KB 丢弃。
    let mut reader = BufReader::new(reader_stream).take(8 * 1024 * 1024);
    let mut line = String::new();
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                let text = line.trim_end();
                if text.is_empty() {
                    continue;
                }
                // 只接受 JSON 对象：畸形行直接丢弃，不让垃圾进前端
                if let Ok(v) = serde_json::from_str::<Value>(text) {
                    if v.is_object() {
                        // 会话已换代：这条消息属于上一局，丢掉
                        if let Ok(g) = inner.lock() {
                            if g.generation != gen {
                                return;
                            }
                        }
                        push(&inner, json!({"type": "msg", "id": peer_id, "data": v}));
                    }
                }
            }
            Err(_) => break,
        }
    }
    if let Ok(mut g) = inner.lock() {
        if g.generation != gen {
            return;   // 陈旧的读线程：别动新会话的状态和事件队列
        }
        match peer_id {
            Some(id) => {
                g.peers.remove(&id);
                g.events.push_back(json!({"type": "peer", "event": "leave", "id": id}));
            }
            None => {
                g.server = None;
                g.events.push_back(json!({"type": "closed"}));
            }
        }
    }
}

/// 主机：监听 TCP（port=None 时由系统分配），并启动 UDP 应答线程。
pub(crate) fn host(inner_arc: &Arc<Mutex<Inner>>, port: Option<u16>, name: String) -> Result<u16, String> {
    shutdown(inner_arc);

    let listener = TcpListener::bind(("0.0.0.0", port.unwrap_or(0)))
        .map_err(|e| format!("监听端口失败: {}", e))?;
    let bound_port = listener
        .local_addr()
        .map_err(|e| format!("读取端口失败: {}", e))?
        .port();
    listener
        .set_nonblocking(true)
        .map_err(|e| format!("设置非阻塞失败: {}", e))?;

    let stop = Arc::new(AtomicBool::new(false));
    let gen = {
        let mut g = inner_arc.lock().map_err(|e| e.to_string())?;
        g.generation = g.generation.wrapping_add(1);
        g.role = Some("host");
        g.stop = Some(stop.clone());
        g.name = name.clone();
        g.generation
    };
    push(
        inner_arc,
        json!({"type": "hosting", "port": bound_port, "name": name}),
    );

    // accept 线程：非阻塞 + 短暂休眠，lan_stop 时能及时退出
    {
        let inner = inner_arc.clone();
        let stop = stop.clone();
        thread::spawn(move || {
            let mut next_id: u32 = 1;
            while !stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, addr)) => {
                        // 换代后（lan_stop / 重新建房）接受的新连接不属于当前会话
                        if inner.lock().map(|g| g.generation != gen).unwrap_or(true) {
                            break;
                        }
                        let too_many = inner
                            .lock()
                            .map(|g| g.peers.len() >= MAX_PEERS)
                            .unwrap_or(true);
                        if too_many {
                            // 发一条说明再关，别让第 10 个人对着「已连接但没人」发愣
                            let mut s = stream;
                            let _ = write_json(&mut s, &json!({"t": "error", "msg": "房间已满"}));
                            let _ = s.shutdown(Shutdown::Both);
                            continue;
                        }
                        stream.set_nodelay(true).ok();
                        let write_half = stream.try_clone().ok();
                        let id = next_id;
                        next_id = next_id.wrapping_add(1).max(1);
                        if let Some(w) = write_half {
                            if let Ok(mut g) = inner.lock() {
                                g.peers.insert(id, w);
                                g.events.push_back(json!({
                                    "type": "peer", "event": "join", "id": id,
                                    "addr": addr.to_string()
                                }));
                            }
                        }
                        let inner2 = inner.clone();
                        let stop2 = stop.clone();
                        thread::spawn(move || read_loop(inner2, stop2, gen, stream, Some(id)));
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(60));
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // UDP 发现应答线程（端口被占用时静默跳过，不影响 TCP 联机）
    {
        let stop = stop.clone();
        // 房间名只保留可打印字符，避免换行把 UDP 应答协议拆坏
        let safe_name: String = name.chars().filter(|c| !c.is_control() && *c != ' ').collect();
        let reply = format!("{}/ROOM {} {}", MAGIC, bound_port, safe_name);
        thread::spawn(move || {
            // 发现端口是单例：上一个房间的 UDP 线程退出有最多 ~200ms 延迟
            // （recv 超时），stop 后立刻重新建房会撞端口。重试几次把它等出来，
            // 实在拿不到就放弃——前端还有「手动输入地址」这条路。
            let mut udp = None;
            for _ in 0..8 {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                if let Ok(s) = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)) {
                    udp = Some(s);
                    break;
                }
                thread::sleep(Duration::from_millis(80));
            }
            let udp = match udp {
                Some(s) => s,
                None => return,
            };
            udp.set_read_timeout(Some(Duration::from_millis(200))).ok();
            let query = format!("{}/DISCOVER", MAGIC);
            let mut buf = [0u8; 128];
            while !stop.load(Ordering::Relaxed) {
                if let Ok((n, from)) = udp.recv_from(&mut buf) {
                    if &buf[..n] == query.as_bytes() {
                        let _ = udp.send_to(reply.as_bytes(), from);
                    }
                }
            }
        });
    }

    Ok(bound_port)
}

/// 客户端：连到主机。`addr` 支持 `ip:port`。
pub(crate) fn join(inner_arc: &Arc<Mutex<Inner>>, addr: &str, name: String) -> Result<(), String> {
    shutdown(inner_arc);

    let sock_addr: SocketAddr = addr
        .trim()
        .parse()
        .map_err(|_| format!("地址格式不对（要 IP:端口）: {}", addr))?;
    let stream = TcpStream::connect_timeout(&sock_addr, Duration::from_secs(5))
        .map_err(|e| format!("连接 {} 失败: {}", addr, e))?;
    stream.set_nodelay(true).ok();

    let stop = Arc::new(AtomicBool::new(false));
    let gen = {
        let mut g = inner_arc.lock().map_err(|e| e.to_string())?;
        g.generation = g.generation.wrapping_add(1);
        g.role = Some("client");
        g.stop = Some(stop.clone());
        g.name = name;
        g.server = stream.try_clone().ok();
        g.generation
    };
    push(
        inner_arc,
        json!({"type": "connected", "addr": addr}),
    );

    let inner = inner_arc.clone();
    thread::spawn(move || read_loop(inner, stop, gen, stream, None));
    Ok(())
}

/// 主机广播 / 客户端发往主机。
pub(crate) fn send(inner_arc: &Arc<Mutex<Inner>>, msg: Value) -> Result<(), String> {
    // 收集要写的流，锁外做 IO（单个小包，但别让 IO 卡住事件队列）
    let mut targets: Vec<TcpStream> = Vec::new();
    {
        let g = inner_arc.lock().map_err(|e| e.to_string())?;
        match g.role {
            Some("host") => {
                for s in g.peers.values() {
                    if let Ok(c) = s.try_clone() {
                        targets.push(c);
                    }
                }
            }
            Some("client") => {
                if let Some(s) = g.server.as_ref() {
                    if let Ok(c) = s.try_clone() {
                        targets.push(c);
                    }
                }
            }
            _ => return Err("没有进行中的局域网会话".into()),
        }
    }
    if targets.is_empty() {
        return Err("没有已连接的对手".into());
    }
    let mut last_err = None;
    let mut ok = 0;
    for mut t in targets {
        match write_json(&mut t, &msg) {
            Ok(_) => ok += 1,
            Err(e) => last_err = Some(e.to_string()),
        }
    }
    if ok == 0 {
        return Err(last_err.unwrap_or_else(|| "发送失败".into()));
    }
    Ok(())
}

/// 主机发给指定客户端（分配座位号用）。
pub(crate) fn send_to(inner_arc: &Arc<Mutex<Inner>>, id: u32, msg: Value) -> Result<(), String> {
    let mut stream = {
        let g = inner_arc.lock().map_err(|e| e.to_string())?;
        let s = g.peers.get(&id).ok_or("连接已断开")?;
        s.try_clone().map_err(|e| e.to_string())?
    };
    write_json(&mut stream, &msg).map_err(|e| e.to_string())
}

pub(crate) fn poll(inner_arc: &Arc<Mutex<Inner>>) -> Vec<Value> {
    match inner_arc.lock() {
        Ok(mut g) => g.events.drain(..).collect(),
        Err(_) => Vec::new(),
    }
}

pub(crate) fn stop(inner_arc: &Arc<Mutex<Inner>>) {
    shutdown(inner_arc);
}

/// 本机在局域网里的 IPv4（UDP connect 只做路由查找，不发包）。
fn local_ipv4() -> Option<Ipv4Addr> {
    let sock = UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    sock.connect(("1.1.1.1", 80)).ok()?;
    match sock.local_addr().ok()?.ip() {
        IpAddr::V4(ip) => Some(ip),
        _ => None,
    }
}

pub(crate) fn local_ip() -> String {
    local_ipv4()
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| "127.0.0.1".into())
}

/// UDP 广播搜索房间，`timeout_ms` 内收集应答。
pub(crate) fn discover(timeout_ms: u64) -> Vec<Value> {
    let sock = match UdpSocket::bind(("0.0.0.0", 0)) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    sock.set_broadcast(true).ok();
    let query = format!("{}/DISCOVER", MAGIC);
    // 全局广播 + 按本机 IP 推 /24 定向广播，部分路由器只放行其中一种
    let _ = sock.send_to(query.as_bytes(), ("255.255.255.255", DISCOVERY_PORT));
    if let Some(ip) = local_ipv4() {
        let o = ip.octets();
        let directed = Ipv4Addr::new(o[0], o[1], o[2], 255);
        let _ = sock.send_to(query.as_bytes(), (directed, DISCOVERY_PORT));
    }
    sock.set_read_timeout(Some(Duration::from_millis(60))).ok();

    let deadline = Instant::now() + Duration::from_millis(timeout_ms.clamp(120, 3000));
    let mut seen: HashSet<String> = HashSet::new();
    let mut found = Vec::new();
    let mut buf = [0u8; 256];
    while Instant::now() < deadline {
        if let Ok((n, from)) = sock.recv_from(&mut buf) {
            let text = String::from_utf8_lossy(&buf[..n]).to_string();
            let Some(rest) = text.strip_prefix(&format!("{}/ROOM ", MAGIC)) else {
                continue;
            };
            let mut parts = rest.splitn(2, ' ');
            let (Some(port), Some(name)) = (parts.next(), parts.next()) else {
                continue;
            };
            let Ok(port) = port.parse::<u16>() else {
                continue;
            };
            let addr = format!("{}:{}", from.ip(), port);
            if seen.insert(addr.clone()) {
                found.push(json!({
                    "addr": addr,
                    "ip": from.ip().to_string(),
                    "port": port,
                    "name": name.replace('_', " "),
                }));
            }
        }
    }
    found
}

// ─── Tauri commands ───

#[tauri::command]
pub async fn lan_host(
    state: tauri::State<'_, LanState>,
    port: Option<u16>,
    name: String,
) -> Result<u16, String> {
    let inner = state.inner.clone();
    host(&inner, port, name)
}

#[tauri::command]
pub async fn lan_join(
    state: tauri::State<'_, LanState>,
    addr: String,
    name: String,
) -> Result<(), String> {
    let inner = state.inner.clone();
    join(&inner, &addr, name)
}

#[tauri::command]
pub async fn lan_send(state: tauri::State<'_, LanState>, msg: Value) -> Result<(), String> {
    let inner = state.inner.clone();
    send(&inner, msg)
}

#[tauri::command]
pub async fn lan_send_to(
    state: tauri::State<'_, LanState>,
    id: u32,
    msg: Value,
) -> Result<(), String> {
    let inner = state.inner.clone();
    send_to(&inner, id, msg)
}

#[tauri::command]
pub async fn lan_poll(state: tauri::State<'_, LanState>) -> Result<Vec<Value>, String> {
    let inner = state.inner.clone();
    Ok(poll(&inner))
}

#[tauri::command]
pub async fn lan_stop(state: tauri::State<'_, LanState>) -> Result<(), String> {
    let inner = state.inner.clone();
    stop(&inner);
    Ok(())
}

#[tauri::command]
pub async fn lan_local_ip() -> Result<String, String> {
    Ok(local_ip())
}

#[tauri::command]
pub async fn lan_discover(timeout_ms: Option<u64>) -> Result<Vec<Value>, String> {
    Ok(discover(timeout_ms.unwrap_or(600)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 这些用例共用固定的 UDP 发现端口（全局单例），cargo test 默认并行
    /// 会让它们互相抢端口。串行跑。
    static LAN_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn serial() -> std::sync::MutexGuard<'static, ()> {
        LAN_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 轮询事件队列，直到谓词成立或超时。返回命中的事件。
    fn wait_event<F>(inner: &Arc<Mutex<Inner>>, ms: u64, mut pred: F) -> Option<Value>
    where
        F: FnMut(&Value) -> bool,
    {
        let deadline = Instant::now() + Duration::from_millis(ms);
        loop {
            for ev in poll(inner) {
                if pred(&ev) {
                    return Some(ev);
                }
            }
            if Instant::now() >= deadline {
                return None;
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn host_and_client_exchange_messages() {
        let _guard = serial();
        let host_state = LanState::new();
        let port = host(&host_state.inner, None, "host".into()).expect("起主机失败");

        let client_state = LanState::new();
        join(&client_state.inner, &format!("127.0.0.1:{}", port), "client".into())
            .expect("连主机失败");

        let joined = wait_event(&host_state.inner, 3000, |ev| {
            ev["type"] == "peer" && ev["event"] == "join"
        })
        .expect("主机没收到 peer join");
        let peer_id = joined["id"].as_u64().expect("peer id 缺失") as u32;

        // 客户端 → 主机：前端发 hello，主机应原样收到 JSON
        send(&client_state.inner, json!({"t":"hello","name":"小明"})).expect("客户端发送失败");
        let got = wait_event(&host_state.inner, 3000, |ev| {
            ev["type"] == "msg" && ev["data"]["t"] == "hello"
        })
        .expect("主机没收到 hello");
        assert_eq!(got["data"]["name"], "小明");
        assert_eq!(got["id"].as_u64(), Some(peer_id as u64));

        // 主机 → 指定客户端：分配座位
        send_to(&host_state.inner, peer_id, json!({"t":"welcome","you":1})).expect("定向发送失败");
        let welcome = wait_event(&client_state.inner, 3000, |ev| {
            ev["type"] == "msg" && ev["data"]["t"] == "welcome"
        })
        .expect("客户端没收到 welcome");
        assert_eq!(welcome["data"]["you"], 1);

        // 主机断开后客户端应收到 closed（验证 shutdown 真的把读线程叫醒了）
        stop(&host_state.inner);
        let closed = wait_event(&client_state.inner, 3000, |ev| ev["type"] == "closed");
        assert!(closed.is_some(), "主机断开后客户端没收到 closed");
        stop(&client_state.inner);
    }

    /// 广播发现：主机起来后，discover 应该能在本机回环上收到自己的房间。
    /// 依赖本机允许 255.255.255.255 广播（Linux / Android / Windows 都允许）。
    #[test]
    fn discover_finds_local_room() {
        let _guard = serial();
        let host_state = LanState::new();
        let port = host(&host_state.inner, None, "测试房间".into()).expect("起主机失败");
        // UDP 应答线程是刚 spawn 的，给它一点时间完成 bind
        thread::sleep(Duration::from_millis(150));
        let found = discover(1200);
        stop(&host_state.inner);
        let hit = found.iter().any(|r| r["port"].as_u64() == Some(port as u64));
        assert!(hit, "discover 没找到本机房间（port={}）: {:?}", port, found);
    }

    /// 换代：停掉旧会话后立刻新建，旧读线程不能往新会话里塞 closed / leave。
    #[test]
    fn stale_session_does_not_leak_events() {
        let _guard = serial();
        let host_state = LanState::new();
        let port = host(&host_state.inner, None, "h1".into()).expect("起主机失败");
        let client_state = LanState::new();
        join(&client_state.inner, &format!("127.0.0.1:{}", port), "c1".into())
            .expect("连主机失败");
        let joined = wait_event(&host_state.inner, 3000, |ev| {
            ev["type"] == "peer" && ev["event"] == "join"
        })
        .expect("主机没收到 peer join");

        // 旧会话直接拆掉，立刻在同一实例上重建（模拟 lanStopNet → lan_host）
        stop(&client_state.inner);
        stop(&host_state.inner);
        let port2 = host(&host_state.inner, None, "h2".into()).expect("重建主机失败");
        // 给旧读线程足够时间醒来并尝试写事件
        thread::sleep(Duration::from_millis(300));
        let stale: Vec<Value> = poll(&host_state.inner);
        assert!(
            stale.iter().all(|ev| ev["type"] != "peer" && ev["type"] != "closed"),
            "旧会话的事件漏进了新会话: {:?} (port2={}, join={:?})",
            stale, port2, joined
        );

        stop(&host_state.inner);
    }

    #[test]
    fn join_rejects_bad_address() {
        let _guard = serial();
        let state = LanState::new();
        assert!(join(&state.inner, "not-an-address", "x".into()).is_err());
    }
}
