const TERMINAL = new Set(["completed", "cancelled", "rejected"]);
const trackedId = /^[0-9a-f]{32}$/i;

export function setupTracking({ $id, t, localized, money, request, orders, show }) {
  const section = $id("sf-tracking-section");
  const title = $id("sf-track-title");
  const content = $id("sf-track-body");
  const error = $id("sf-track-error");
  const pix = $id("sf-track-pix");
  const pixKey = $id("sf-track-pix-key");
  const copy = $id("sf-copy-pix");
  const copyStatus = $id("sf-pix-copy-status");
  let current;
  let timer;
  let controller;
  let view;
  let etag;
  let errorKey;
  let cancelling = false;

  function displayError(key) {
    errorKey = key;
    error.hidden = !key;
    error.textContent = key ? t(key) : "";
  }

  function close() {
    window.clearTimeout(timer);
    controller?.abort();
    current = undefined;
    view = undefined;
    etag = undefined;
    cancelling = false;
    pixKey.textContent = "";
    pix.hidden = true;
    copyStatus.hidden = true;
  }

  function render() {
    if (section.hidden) return;
    const key = current?.pix_key_display;
    pix.hidden = typeof key !== "string" || !key || view?.state === "cancelled" || view?.state === "rejected";
    pixKey.textContent = pix.hidden ? "" : key;
    copy.hidden = !navigator.clipboard?.writeText;
    if (!view) {
      title.textContent = t("storefront.tracking_heading");
      return;
    }
    title.textContent = "#" + view.order_number + " · " + t("storefront.state_" + view.state);
    content.replaceChildren();
    const details = document.createElement("p");
    details.textContent = t("commerce.hours." + view.fulfillment) + " · " + money(view.totals.total_centavos);
    content.append(details);
    const items = document.createElement("ul");
    for (const item of view.items) {
      const line = document.createElement("li");
      const variation = localized(item, "variation_name");
      line.textContent = item.quantity + " × " + localized(item) + (variation ? " · " + variation : "");
      items.append(line);
    }
    content.append(items);
    const cancelToken = current?.cancel_token;
    if (view.can_cancel && cancelToken) {
      const cancel = document.createElement("button");
      cancel.type = "button";
      cancel.className = "sf-btn-secondary";
      cancel.textContent = t("delivery.orders.cancel");
      cancel.disabled = cancelling;
      cancel.addEventListener("click", () => cancelOrder(cancelToken));
      content.append(cancel);
    }
  }

  async function cancelOrder(cancelToken) {
    if (cancelling || !current) return;
    if (!$id("sf-cancel-order-dialog").open) {
      $id("sf-cancel-order-dialog").showModal();
      return;
    }
    $id("sf-cancel-order-dialog").close();
    const order = current;
    cancelling = true;
    render();
    try {
      const { response, body } = await request("/orders/" + encodeURIComponent(order.order_id) + "/cancel",
        { cancel_token: cancelToken });
      if (current !== order) return;
      if (!response.ok) {
        displayError(body.code || "error.internal");
      } else {
        etag = undefined;
        await poll();
      }
    } catch {
      if (current === order) displayError(navigator.onLine ? "error.internal" : "storefront.offline");
    } finally {
      if (current === order) {
        cancelling = false;
        render();
      }
    }
  }

  async function poll() {
    window.clearTimeout(timer);
    controller?.abort();
    const order = current;
    if (!order) return;
    const next = new AbortController();
    controller = next;
    try {
      const headers = etag ? { "If-None-Match": etag } : {};
      const { response, body } = await request("/orders/" + encodeURIComponent(order.order_id),
        undefined, { headers, signal: next.signal });
      if (next.signal.aborted) return;
      if (response.ok) {
        etag = response.headers.get("ETag");
        view = body;
        displayError("");
        render();
      } else if (response.status !== 304) {
        displayError(body.code || "error.internal");
      }
    } catch {
      if (!next.signal.aborted) displayError(navigator.onLine ? "error.internal" : "storefront.offline");
    } finally {
      if (!next.signal.aborted && !TERMINAL.has(view?.state)) timer = window.setTimeout(poll, 15000);
    }
  }

  function open(order) {
    if (!trackedId.test(order?.order_id || "")) return;
    close();
    current = order;
    content.replaceChildren();
    displayError("");
    show("tracking");
    render();
    poll();
  }

  copy.addEventListener("click", async () => {
    try {
      await navigator.clipboard.writeText(pixKey.textContent);
      copyStatus.textContent = t("storefront.pix_key_copied");
    } catch {
      copyStatus.textContent = t("storefront.pix_key_select");
    }
    copyStatus.hidden = false;
  });
  $id("sf-cancel-confirm").addEventListener("click", () => {
    if (current?.cancel_token) cancelOrder(current.cancel_token);
  });
  $id("sf-cancel-dismiss").addEventListener("click", () => $id("sf-cancel-order-dialog").close());
  $id("sf-order-history").addEventListener("click", () => {
    if (orders()[0]) open(orders()[0]);
  });

  return { open, close, localize() { render(); displayError(errorKey); } };
}
