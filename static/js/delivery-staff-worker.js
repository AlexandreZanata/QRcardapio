self.addEventListener("push", (event) => {
  try {
    const { title, body, tag } = event.data.json();
    if (typeof title !== "string" || typeof body !== "string") return;
    event.waitUntil(self.registration.showNotification(title, {
      body, tag, icon: "/static/icons/favicon.svg",
    }));
  } catch {
    // Ignore malformed payloads.
  }
});

self.addEventListener("notificationclick", (event) => {
  event.notification.close();
  event.waitUntil((async () => {
    const destination = new URL("/dashboard/delivery/orders", self.location.origin).href;
    const tabs = await self.clients.matchAll({ type: "window", includeUncontrolled: true });
    const openCenter = tabs.find((tab) => tab.url.startsWith(destination));
    if (openCenter) await openCenter.focus();
    else await self.clients.openWindow(destination);
  })());
});
