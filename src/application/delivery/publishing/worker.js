self.addEventListener("install", event => {
  event.waitUntil((async () => {
    const cache = await caches.open(CACHE_NAME);
    await cache.addAll(SHELL.map(url => new Request(url, { cache: "reload" })));
    await self.skipWaiting();
  })());
});

self.addEventListener("activate", event => {
  event.waitUntil((async () => {
    const stale = (await caches.keys()).filter(key => key.startsWith(CACHE_PREFIX) && key !== CACHE_NAME);
    await Promise.all(stale.map(key => caches.delete(key)));
    await self.clients.claim();
  })());
});

async function page(request) {
  const cache = await caches.open(CACHE_NAME);
  try {
    const response = await fetch(request, { cache: "no-cache" });
    if (response.ok && !response.redirected) {
      try {
        await cache.put(SCOPE, response.clone());
      } catch {
        // A full cache must not block the online response.
      }
    } else if (response.status === 404 || response.status === 410) {
      await caches.delete(CACHE_NAME);
      await self.registration.unregister();
    }
    return response;
  } catch {
    return await cache.match(SCOPE) || Response.error();
  }
}

async function asset(request) {
  const cache = await caches.open(CACHE_NAME);
  const cached = await cache.match(request);
  if (cached) return cached;
  const response = await fetch(request);
  if (response.ok && !response.redirected) {
    try {
      await cache.put(request, response.clone());
    } catch {
      // A full cache must not block the online response.
    }
  }
  return response;
}

self.addEventListener("fetch", event => {
  const request = event.request;
  if (request.method !== "GET") return;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin) return;

  if (request.mode === "navigate" && (url.pathname === SCOPE || url.pathname === SCOPE + "index.html")) {
    event.respondWith(page(request));
  } else if (SHELL.includes(url.pathname + url.search) ||
      (url.pathname.startsWith(SCOPE + "media/") && request.destination === "image")) {
    event.respondWith(asset(request));
  }
});
