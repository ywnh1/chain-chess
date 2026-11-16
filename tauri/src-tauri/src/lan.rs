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
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, Shutdown, SocketAddr, TcpListener, TcpStream, UdpSocket};
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

    // 监听要同时吃 IPv4 与 IPv6：
    //   只绑 0.0.0.0  → 有公网 IPv6 的设备也接不了远程连接
    //   只绑 [::]     → Windows 的 IPV6_V6ONLY 默认是 1，会**只收 IPv6**，
    //                   把局域网 IPv4 联机弄坏（Linux/Android 默认是双栈）
    // 所以先绑 [::]，再探一下 IPv4 是否已被覆盖；没覆盖就补一个 IPv4 监听。
    //
    // 探测办法：在同一个端口上再绑一次 0.0.0.0。
    //   成功   → v6only=1，IPv4 没被覆盖，这个 socket 留下当第二个监听
    //   EADDRINUSE → 双栈已经覆盖 IPv4，丢弃即可
    let want_port = port.unwrap_or(0);
    // match 直接返回 (监听列表, 端口)：两个分支各自构造完整结果，
    // 避免先声明一个从未被读取的初值（那样会触发 unused_assignments）。
    let (listeners, bound_port): (Vec<TcpListener>, u16) =
        match TcpListener::bind(("::", want_port)) {
            Ok(l) => {
                let p = l.local_addr().map_err(|e| format!("读取端口失败: {}", e))?.port();
                let mut v = vec![l];
                // 同端口再绑一次 IPv4：能绑上说明系统是 v6only，得补第二个监听
                if let Ok(v4) = TcpListener::bind(("0.0.0.0", p)) {
                    eprintln!("[lan] 系统为 v6only，额外启用 IPv4 监听（端口 {}）", p);
                    v.push(v4);
                }
                (v, p)
            }
            Err(e6) => {
                eprintln!("[lan] IPv6 监听失败（{}），退回 IPv4", e6);
                let l = TcpListener::bind(("0.0.0.0", want_port))
                    .map_err(|e| format!("监听端口失败: {}", e))?;
                let p = l.local_addr().map_err(|e| format!("读取端口失败: {}", e))?.port();
                (vec![l], p)
            }
        };
    for l in &listeners {
        l.set_nonblocking(true)
            .map_err(|e| format!("设置非阻塞失败: {}", e))?;
    }

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

    // accept 线程：每个监听一个。peer id 用共享计数器，避免两个监听撞号。
    let next_id = Arc::new(std::sync::atomic::AtomicU32::new(1));
    for listener in listeners {
        let inner = inner_arc.clone();
        let stop = stop.clone();
        let next_id = next_id.clone();
        thread::spawn(move || {
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
                        let id = next_id.fetch_add(1, Ordering::Relaxed).max(1);
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

/// 解析 `ip:port` / `[v6]:port`。
///
/// 用户从聊天窗口粘过来的 IPv6 常常**不带方括号** ——
/// `2409:8a02:...:d93f:41234` 这种。裸 IPv6 本身就用冒号分段，
/// 标准库的 `SocketAddr` 解析必然失败。这里做一次容错：
/// 解析失败时按**最后一个**冒号切开，左边若含冒号就补成 `[left]:port`。
/// 最后一段是端口，这也符合 IPv6 地址不能以单独数字结尾当端口的惯例。
fn parse_endpoint(addr: &str) -> Result<SocketAddr, String> {
    let t = addr.trim();
    if let Ok(sa) = t.parse::<SocketAddr>() {
        return Ok(sa);
    }
    if let Some((host, port)) = t.rsplit_once(':') {
        if host.contains(':') && !host.starts_with('[') {
            let fixed = format!("[{}]:{}", host, port);
            if let Ok(sa) = fixed.parse::<SocketAddr>() {
                return Ok(sa);
            }
        }
    }
    Err(format!("地址格式不对（要 IP:端口 或 [IPv6]:端口）: {}", addr))
}

/// 客户端：连到主机。`addr` 支持 `ip:port` 与 `[v6]:port`。
pub(crate) fn join(inner_arc: &Arc<Mutex<Inner>>, addr: &str, name: String) -> Result<(), String> {
    shutdown(inner_arc);

    let sock_addr: SocketAddr = parse_endpoint(addr)?;
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

/// 本机有没有「能出公网」的 IPv6，有就返回地址。
///
/// 判据用的是 UDP connect 的路由查找（不发包）：能连通一个公网 IPv6
/// 目标，说明系统确实有 `2000::/3` 的地址且配了默认路由。
/// 这比遍历网卡更准 —— 网卡上挂个 fe80:: 或 fd00:: 并不代表能对外通信，
/// 而有些系统同时有 EUI-64 地址和临时地址，这里拿到的是实际会用的那个。
///
/// 注意：这只证明**出站**可达。对端能不能连进来还取决于路由器/防火墙
/// 是否放行入站，这点无法在设备侧自证，界面上要如实提示。
/// 探测结果：成功拿到地址，或失败原因（分开报告，便于用户判断是
/// 「系统不支持 IPv6」还是「网络没下发地址」，两种情况处理方式完全不同）。
enum V6Probe {
    Found(Ipv6Addr),
    /// 连 [::] 都绑不上 —— 系统层面就没有 IPv6 栈
    NoStack,
    /// socket 能创建，但没有到公网 IPv6 的路由 —— 网络没给地址
    NoRoute,
}

/// 候选目标只用国内可达的 IPv6 DNS。
/// 之前用的是 Google DNS(2001:4860::8888)，国内网络下它的路由经常
/// 被黑洞掉，UDP connect 会直接返回 ENETUNREACH —— 明明有公网 IPv6
/// 却被判成「没有」，这是真实踩到的坑。
const V6_PROBES: &[&str] = &[
    "2400:3200::1",        // 阿里 DNS
    "2402:4e00::",         // 腾讯 DNS
    "240c::6666",          // 下一代互联网国家工程中心
    "2001:4860:4860::8888",// Google，仅作最后兜底
];

fn probe_ipv6() -> V6Probe {
    // 先把「系统有没有 IPv6 栈」和「网络有没有路由」分开，
    // 否则两种失败都变成一句「没有」，用户没法自查。
    // 必须用 SocketAddr 构造，不能写 ("[::]:0", 0)。
    // ("[::]:0", 0) 会被当成「主机名 [::]:0 + 端口 0」去做 DNS 查询，
    // 带方括号的字面量既不是合法 IPv6 也不是合法主机名 → 直接
    // "failed to lookup address information" 失败，于是每次都被判成
    // 「没有 IPv6 协议栈」。这是真实踩到的坑。
    let any_bind = UdpSocket::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0)));
    if any_bind.is_err() {
        return V6Probe::NoStack;
    }
    drop(any_bind);

    for target in V6_PROBES {
        let Ok(sock) = UdpSocket::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))) else { continue };
        // UDP connect 只做路由查找，不会真的发包
        if sock.connect((*target, 53)).is_err() {
            continue;
        }
        if let Ok(addr) = sock.local_addr() {
            if let IpAddr::V6(ip) = addr.ip() {
                // ::1 / fe80:: 这种也算「有栈有路由但不该拿来联机」，
                // 交给上层按 2000::/3 判定，这里先如实返回。
                return V6Probe::Found(ip);
            }
        }
    }
    V6Probe::NoRoute
}

/// 给前端的 IPv6 概况：有没有、地址是什么、能不能用来被远程连。
/// 全局单播 2000::/3 才算「公网」。ULA(fd00::/8) 和 link-local(fe80::/10)
/// 都不能跨网络访问，不能拿来当远程地址。
fn is_global_v6(ip: &Ipv6Addr) -> bool {
    // 2000::/3 即前 3 位为 001，等价于首段落在 0x2000..=0x3fff
    (0x2000..0x4000).contains(&ip.segments()[0])
}

pub(crate) fn ipv6_info() -> Value {
    match probe_ipv6() {
        V6Probe::NoStack => json!({
            "available": false,
            "addr": Value::Null,
            "raw": Value::Null,
            "reason": "系统没有可用的 IPv6 协议栈（无法创建 IPv6 套接字）",
            "code": "no_stack",
        }),
        V6Probe::NoRoute => json!({
            "available": false,
            "addr": Value::Null,
            "raw": Value::Null,
            "reason": "网络没有下发 IPv6 地址（当前 Wi-Fi/移动数据未启用 IPv6）",
            "code": "no_route",
        }),
        V6Probe::Found(ip) => {
            let s = ip.to_string();
            if is_global_v6(&ip) {
                json!({
                    "available": true,
                    "addr": s,
                    "raw": s,
                    "reason": Value::Null,
                    "code": "ok",
                })
            } else {
                json!({
                    "available": false,
                    "addr": Value::Null,
                    "raw": s,
                    "reason": "只有内网 IPv6（fd00:: / fe80::），无法跨网络访问",
                    "code": "private_only",
                })
            }
        }
    }
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

/// 本机 IPv6 情况（供「远程联机」入口显示与一键复制）
#[tauri::command]
pub async fn lan_ipv6() -> Result<Value, String> {
    Ok(ipv6_info())
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
        // 没网 / 端口被别的进程占着的环境（CI 容器、共享机器）跳过硬性断言，
        // 这种环境里广播本来就收不到回声，不是代码坏了。
        if local_ipv4().is_none() || UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT)).is_err() {
            eprintln!("跳过 discover 用例：本机无局域网地址或发现端口被占用");
            return;
        }
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

    /// 只认 2000::/3 为公网；内网/链路本地/回环都必须判否。
    /// 这些值直接决定界面上说「能远程联机」还是「只能局域网」，
    /// 判错会让用户拿着 fd00:: 地址去找对面连，白折腾。
    #[test]
    fn global_v6_detection() {
        let global: Ipv6Addr = "2409:8a02:c22:99a0:3c62:c2ff:fefb:d93f".parse().unwrap();
        assert!(is_global_v6(&global), "中国移动 2409:: 段是公网地址");

        let global2: Ipv6Addr = "2001:4860:4860::8888".parse().unwrap();
        assert!(is_global_v6(&global2), "2001:: 段是公网地址");

        for (s, why) in [
            ("fd00::1", "ULA 私有"),
            ("fc00::1", "ULA 私有（fc00::/7）"),
            ("fe80::3c62:c2ff:fefb:d93f", "链路本地"),
            ("::1", "回环"),
            ("2001:db8::1", "文档地址虽在 2000::/3，但不会出现在真网卡上"),
        ] {
            let ip: Ipv6Addr = s.parse().unwrap();
            if why.starts_with("文档") {
                // 明确记录这个已知边界：我们不特判 2001:db8::/32。
                assert!(is_global_v6(&ip), "文档地址按前缀判定为公网（已知可接受）");
            } else {
                assert!(!is_global_v6(&ip), "{} 不应判为公网: {}", why, s);
            }
        }
    }

    /// 地址解析容错：用户粘来的 IPv6 经常不带方括号（裸 IPv6 本身就有冒号）。
    #[test]
    fn endpoint_parsing() {
        for (input, want) in [
            ("192.168.1.23:41234", "192.168.1.23:41234"),
            ("[2409:8a02:c22:99a0:3c62:c2ff:fefb:d93f]:41234",
             "[2409:8a02:c22:99a0:3c62:c2ff:fefb:d93f]:41234"),
            // 裸 IPv6 + 端口：按最后一个冒号切开
            ("2409:8a02:c22:99a0:3c62:c2ff:fefb:d93f:41234",
             "[2409:8a02:c22:99a0:3c62:c2ff:fefb:d93f]:41234"),
            // 前后空格
            ("  192.168.1.5:1234  ", "192.168.1.5:1234"),
        ] {
            let got = parse_endpoint(input).unwrap_or_else(|e| panic!("{} 解析失败: {}", input, e));
            assert_eq!(got.to_string(), want, "输入 {}", input);
        }
        assert!(parse_endpoint("not-an-address").is_err());
        assert!(parse_endpoint("2409:8a02::1").is_err(), "裸 IPv6 没有端口应报错");
    }

    /// 探测目标必须是国内可达的 IPv6 地址。
    /// Google DNS 在国内常被黑洞，UDP connect 直接 ENETUNREACH，
    /// 会把「有公网 IPv6」误判成「没有」。这条盯住别把它改回去。
    #[test]
    fn v6_probe_targets_are_reachable_in_cn() {
        assert!(!V6_PROBES.is_empty());
        assert!(
            !V6_PROBES[0].starts_with("2001:4860"),
            "首选探测目标不应是 Google DNS（国内路由常被黑洞）: {}",
            V6_PROBES[0]
        );
        // 每项都要能被解析，否则循环里会静默 continue 掉
        for t in V6_PROBES {
            assert!(t.parse::<Ipv6Addr>().is_ok(), "探测目标不是合法 IPv6: {}", t);
        }
        // 至少留一个国内运营商的 DNS
        assert!(
            V6_PROBES.iter().any(|t| t.starts_with("2400:") || t.starts_with("2402:") || t.starts_with("240c:")),
            "应包含国内可达的探测目标"
        );
    }

    /// 用户实际报过的地址，必须能正确判定为公网。
    #[test]
    fn real_world_cn_mobile_v6_is_global() {
        for a in [
            "2409:8903:f85:20d5:98b2:42ff:fea7:3f02",
            "2409:8a02:c22:99a0:3c62:c2ff:fefb:d93f",
        ] {
            let ip: Ipv6Addr = a.parse().unwrap();
            assert!(is_global_v6(&ip), "中国移动公网地址被判成非公网: {}", a);
        }
    }

    /// 真实探测：本机若有公网 IPv6，`ipv6_info` 必须报 available 并给出
    /// 2000::/3 地址。没有 IPv6 的环境（CI 容器）跳过。
    ///
    /// 这条是回归重点：曾经把 bind 写成 `("[::]:0", 0)`，探测第一步就
    /// 失败，明明有公网 IPv6 也报「没有协议栈」。用 SocketAddr 构造才对。
    #[test]
    fn ipv6_probe_reports_available_when_hosted() {
        let _guard = serial();
        // 用一个纯 SocketAddr 绑定判断本机有没有 IPv6 栈
        let has_v6 = UdpSocket::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))).is_ok();
        if !has_v6 {
            eprintln!("跳过：本机没有 IPv6 套接字支持");
            return;
        }
        // 有栈不代表有路由，两者都要才谈得上「可用」
        let has_route = {
            let s = UdpSocket::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))).unwrap();
            V6_PROBES.iter().any(|t| s.connect((*t, 53)).is_ok())
        };
        if !has_route {
            eprintln!("跳过：本机有 IPv6 栈但没有公网 IPv6 路由");
            return;
        }

        let info = ipv6_info();
        assert_eq!(info["available"], true, "有 IPv6 路由时应报可用: {}", info);
        let addr = info["addr"].as_str().expect("available 时必须给出地址");
        let ip: Ipv6Addr = addr.parse().expect("地址应可解析");
        assert!(is_global_v6(&ip), "给出的地址必须是 2000::/3: {}", addr);
        eprintln!("探测到的公网 IPv6: {}", addr);
    }

    /// 端到端：房主双栈监听 → 客户端用「公网 IPv6 地址」连进来 → 收发一条消息。
    ///
    /// 这是「远程联机」的核心链路。本机有公网 IPv6 时才会真正跑；
    /// 没有 IPv6 的环境跳过（本机若能连自己的公网地址，说明绑定与
    /// 地址格式都对）。
    #[test]
    fn remote_join_over_public_ipv6() {
        let _guard = serial();
        // 没有公网 IPv6 就跳过（CI 容器）
        let info = ipv6_info();
        let Some(v6) = info["addr"].as_str().map(|s| s.to_string()) else {
            eprintln!("跳过：本机没有公网 IPv6（{}）", info["reason"]);
            return;
        };

        let host_state = LanState::new();
        let port = host(&host_state.inner, None, "h".into()).expect("起主机失败");
        let joined = wait_event(&host_state.inner, 3000, |ev| {
            ev["type"] == "peer" && ev["event"] == "join"
        });

        // 关键：用公网 IPv6 拼接的端点去连（前端复制出去的就是这个格式）
        let endpoint = format!("[{}]:{}", v6, port);
        let client_state = LanState::new();
        join(&client_state.inner, &endpoint, "c".into())
            .unwrap_or_else(|e| panic!("用公网 IPv6 连接失败 ({}) — {}", endpoint, e));

        let joined = joined.or_else(|| {
            wait_event(&host_state.inner, 3000, |ev| {
                ev["type"] == "peer" && ev["event"] == "join"
            })
        });
        assert!(joined.is_some(), "主机没收到来自公网 IPv6 的连接: {}", endpoint);

        // 消息能通
        send(&client_state.inner, json!({"t":"hello","name":"v6"})).expect("发送失败");
        let got = wait_event(&host_state.inner, 3000, |ev| {
            ev["type"] == "msg" && ev["data"]["t"] == "hello"
        });
        assert!(got.is_some(), "公网 IPv6 连接上消息没通");

        stop(&host_state.inner);
        stop(&client_state.inner);
    }

    /// 双栈监听必须同时吃 IPv4 与 IPv6。
    /// 只绑 [::] 在 v6only=1 的系统（Windows 默认）上会丢掉 IPv4，
    /// 等于把局域网联机弄坏；这条确认两条路都通。
    #[test]
    fn host_accepts_both_ipv4_and_ipv6_clients() {
        let _guard = serial();
        let host_state = LanState::new();
        let port = host(&host_state.inner, None, "h".into()).expect("起主机失败");

        // IPv4 回环
        let c4 = LanState::new();
        join(&c4.inner, &format!("127.0.0.1:{}", port), "c4".into())
            .expect("IPv4 客户端连不上（双栈覆盖失败）");
        assert!(
            wait_event(&host_state.inner, 3000, |ev| ev["type"] == "peer").is_some(),
            "主机没收到 IPv4 连接"
        );

        // IPv6 回环（本机没 IPv6 栈时跳过）
        if UdpSocket::bind(SocketAddr::from((Ipv6Addr::UNSPECIFIED, 0))).is_ok() {
            let c6 = LanState::new();
            match join(&c6.inner, &format!("[::1]:{}", port), "c6".into()) {
                Ok(()) => {
                    assert!(
                        wait_event(&host_state.inner, 3000, |ev| {
                            ev["type"] == "peer" && ev["addr"].as_str().map_or(false, |a| a.contains("::1") || a.starts_with("::"))
                        })
                        .is_some(),
                        "主机没收到 IPv6 连接"
                    );
                    stop(&c6.inner);
                }
                Err(e) => eprintln!("跳过 IPv6 回环（{}）", e),
            }
        }

        stop(&c4.inner);
        stop(&host_state.inner);
    }

    #[test]
    fn join_rejects_bad_address() {
        let _guard = serial();
        let state = LanState::new();
        assert!(join(&state.inner, "not-an-address", "x".into()).is_err());
    }
}
