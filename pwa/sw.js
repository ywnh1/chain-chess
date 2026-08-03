/* sw.js — 连锁棋 PWA Service Worker（离线缓存） */
const CACHE_NAME = 'chain-chess-v3.2.10';
const CORE_ASSETS = [
  './',
  './index.html',
  './style.css',
  './app.js',
  './engine.js',
  './manifest.webmanifest',
  './icons/icon-192.png',
  './icons/icon-512.png',
  './icons/icon-maskable-512.png',
  './audio/大狗.mp3',
  './audio/叫(中淡出).mp3',
  './audio/叫(无淡出).mp3',
  './audio/叫(长淡出).mp3',
  './pkg/chain_chess_engine.js',
  './pkg/chain_chess_engine_bg.wasm'
];

// 安装：预缓存核心资源
self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME)
      .then((cache) => cache.addAll(CORE_ASSETS))
      .then(() => self.skipWaiting())
  );
});

// 激活：清理旧版本缓存
self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(keys.filter((k) => k !== CACHE_NAME).map((k) => caches.delete(k)))
    ).then(() => self.clients.claim())
  );
});

// 请求：缓存优先，网络回退并更新缓存
self.addEventListener('fetch', (event) => {
  const req = event.request;
  if (req.method !== 'GET') return;
  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return;   // 跨域不缓存

  // ignoreSearch: HTML 引用带 ?v= 版本号（style.css?v=3.2.10），预缓存键无查询串。
  // 默认 Cache API 匹配区分查询串，会导致预缓存永不命中；忽略查询串后二者互通。
  event.respondWith(
    caches.match(req, { ignoreSearch: true }).then((cached) => {
      if (cached) {
        // 后台刷新缓存
        fetch(req).then((res) => {
          if (res && res.ok) {
            caches.open(CACHE_NAME).then((cache) => cache.put(req, res.clone()));
          }
        }).catch(() => {});
        return cached;
      }
      return fetch(req).then((res) => {
        if (res && res.ok) {
          const clone = res.clone();
          caches.open(CACHE_NAME).then((cache) => cache.put(req, clone));
        }
        return res;
      }).catch(() => {
        // 离线兜底：导航请求返回缓存的 index.html
        if (req.mode === 'navigate') return caches.match('./index.html');
      });
    })
  );
});
