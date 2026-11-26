const soundButton = document.querySelector("[data-delivery-sound]");
const pushButton = document.querySelector("[data-delivery-push]");
const status = document.querySelector("[data-delivery-alert-status]");
let list = document.querySelector("[data-delivery-orders]");
let audio;
let etag;
let polling = false;
let knownOrders = orderIds(list);

function orderIds(root) {
  return new Set(Array.from(root.querySelectorAll("[data-order-id]"), (el) => el.dataset.orderId));
}

function showStatus(key) {
  const messageKey = `delivery.alert.${key}`;
  status.textContent = window.QRI18n?.t(messageKey) ?? status.dataset[key];
  status.setAttribute("data-i18n", messageKey);
}

function chime() {
  if (audio?.state !== "running") return;
  const start = audio.currentTime;
  [520, 660, 780].forEach((frequency, index) => {
    const speaker = audio.createOscillator();
    const volume = audio.createGain();
    const beat = start + index * 0.12;
    speaker.frequency.value = frequency;
    volume.gain.setValueAtTime(0.12, beat);
    volume.gain.exponentialRampToValueAtTime(0.001, beat + 0.17);
    speaker.connect(volume).connect(audio.destination);
    speaker.start(beat);
    speaker.stop(beat + 0.18);
  });
}

soundButton?.addEventListener("click", async () => {
  try {
    audio ||= new AudioContext();
    if (audio.state === "running") {
      await audio.suspend();
      soundButton.setAttribute("aria-pressed", "false");
    } else {
      await audio.resume();
      soundButton.setAttribute("aria-pressed", "true");
      chime();
    }
  } catch {
    showStatus("error");
  }
});

function formInUse() {
  return list.contains(document.activeElement) && document.activeElement.closest("form");
}

async function refresh() {
  if (document.hidden || polling || formInUse() || document.body.classList.contains("is-printing-delivery")) return;
  polling = true;
  const url = new URL(location.href);
  url.searchParams.set("fragment", "1");
  url.searchParams.set("lang", document.documentElement.lang);
  try {
    const response = await fetch(url, {
      headers: etag ? { "If-None-Match": etag } : {},
      cache: "no-store",
    });
    if (response.status === 304) return;
    if (!response.ok || response.redirected) return;
    const html = new DOMParser().parseFromString(await response.text(), "text/html");
    const next = html.querySelector("[data-delivery-orders]");
    if (!next || formInUse()) return;
    const incoming = Array.from(next.querySelectorAll('[data-order-state="placed"]'));
    if (incoming.some((order) => !knownOrders.has(order.dataset.orderId))) chime();
    knownOrders = orderIds(next);
    list.replaceWith(next);
    list = next;
    etag = response.headers.get("ETag");
  } catch {
    // Keep the current list and retry when the tab is visible again.
  } finally {
    polling = false;
  }
}

if (list) {
  window.setInterval(refresh, 5000);
  document.addEventListener("visibilitychange", refresh);
}

async function pushRequest(url, body) {
  const response = await fetch(url, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ ...body, _csrf: pushButton.dataset.csrf }),
  });
  if (!response.ok || response.redirected) throw new Error("subscription update failed");
  return response.status === 204 ? null : response.json();
}

if (pushButton && "serviceWorker" in navigator && "PushManager" in window && "Notification" in window) {
  const path = "/dashboard/delivery/push-subscriptions";
  const localKey = `qrc-delivery-push-${pushButton.dataset.scope}`;
  pushButton.hidden = false;
  try {
    pushButton.setAttribute("aria-pressed", localStorage.getItem(localKey) ? "true" : "false");
  } catch {
    pushButton.hidden = true;
  }
  pushButton.addEventListener("click", async () => {
    pushButton.disabled = true;
    try {
      const registeredId = localStorage.getItem(localKey);
      if (registeredId) {
        await pushRequest(`${path}/${encodeURIComponent(registeredId)}/revoke`, {});
        const registration = await navigator.serviceWorker.getRegistration("/dashboard/delivery/");
        const subscription = await registration?.pushManager.getSubscription();
        await subscription?.unsubscribe();
        localStorage.removeItem(localKey);
        pushButton.setAttribute("aria-pressed", "false");
        showStatus("disabled");
        return;
      }
      if (await Notification.requestPermission() !== "granted") {
        showStatus("error");
        return;
      }
      const registration = await navigator.serviceWorker.register("/dashboard/delivery/sw.js", {
        scope: "/dashboard/delivery/",
      });
      const active = registration.active
        ? registration
        : await Promise.race([
          navigator.serviceWorker.ready,
          new Promise((_, reject) => setTimeout(reject, 6000)),
        ]);
      const subscription = await active.pushManager.getSubscription()
        || await active.pushManager.subscribe({
          userVisibleOnly: true,
          applicationServerKey: Uint8Array.from(
            atob(pushButton.dataset.publicKey.replaceAll("-", "+").replaceAll("_", "/")),
            (char) => char.charCodeAt(0),
          ),
        });
      const { endpoint, keys } = subscription.toJSON();
      const { id } = await pushRequest(path, { endpoint, ...keys });
      localStorage.setItem(localKey, id);
      pushButton.setAttribute("aria-pressed", "true");
      showStatus("enabled");
    } catch {
      showStatus("error");
    } finally {
      pushButton.disabled = false;
    }
  });
}


document.addEventListener("click", (event) => {
  const trigger = event.target.closest("[data-print-order]");
  if (!trigger) return;
  const card = trigger.closest("[data-order-id]");
  document.body.classList.add("is-printing-delivery");
  card.classList.add("delivery-print-ticket");
  try {
    window.print();
  } finally {
    card.classList.remove("delivery-print-ticket");
    document.body.classList.remove("is-printing-delivery");
  }
});
