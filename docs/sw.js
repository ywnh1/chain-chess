/* sw.js — 连锁棋 PWA Service Worker（离线缓存） */
const CACHE_NAME = 'chain-chess-v4.1.0';
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

// 安装：预缓存核心资源。
// 不再无条件 skipWaiting()：新版装完后停在 waiting 状态，由页面里的更新提示条
// 在用户点击时发来 {type:'SKIP_WAITING'} 才接管，实现「原地更新、不跳下载中心」。
self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.addAll(CORE_ASSETS))
  );
});

// 页面点击「有新版本，点击刷新」后才会走到这里
self.addEventListener('message', (event) => {
  const data = event.data;
  if (data && data.type === 'SKIP_WAITING') {
    self.skipWaiting();
  }
});

// 激活：清理旧版本缓存
self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys().then((keys) =>
      Promise.all(keys.filter((k) => k !== CACHE_NAME).map((k) => caches.delete(k)))
    ).then(() => self.clients.claim())
  );
});

// 外壳资源（HTML/CSS/JS）：网络优先，失败再回退缓存。
//
// 为什么不用缓存优先：CACHE_NAME 只随版本号变，而日常修 bug 不改版本号。
// 缓存优先会让浏览器一直用旧的 style.css / app.js —— 典型症状是新加的
// 按钮没有样式（HTML 是按钮，CSS 还是旧版），看起来像「功能没生效」。
// 网络优先后，在线用户每次刷新都拿到最新文件；离线仍走缓存兜底。
function isShellAsset(req, url) {
  if (req.mode === 'navigate') return true;
  return /\.(?:html|css|js)$/i.test(url.pathname);
}

self.addEventListener('fetch', (event) => {
  const req = event.request;
  if (req.method !== 'GET') return;
  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return;   // 跨域不缓存

  if (isShellAsset(req, url)) {
    event.respondWith(
      fetch(req).then((res) => {
        if (res && res.ok) {
          const clone = res.clone();
          caches.open(CACHE_NAME).then((cache) => cache.put(req, clone)).catch(() => {});
        }
        return res;
      }).catch(() => {
        // ignoreSearch：HTML 引用带 ?v= 版本号，预缓存键没有查询串
        return caches.match(req, { ignoreSearch: true }).then((cached) => {
          if (cached) return cached;
          if (req.mode === 'navigate') return caches.match('./index.html');
          return Response.error();
        });
      })
    );
    return;
  }

  // 其余资源（wasm / 图片 / 音频）：缓存优先，体积大且很少变
  event.respondWith(
    caches.match(req, { ignoreSearch: true }).then((cached) => {
      if (cached) return cached;
      return fetch(req).then((res) => {
        if (res && res.ok) {
          const clone = res.clone();
          caches.open(CACHE_NAME).then((cache) => cache.put(req, clone)).catch(() => {});
        }
        return res;
      }).catch(() => {
        if (req.mode === 'navigate') return caches.match('./index.html');
        return Response.error();
      });
    })
  );
});
