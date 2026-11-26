const $id = id => document.getElementById(id);
const dataEl = $id("qr-storefront-data");
const i18nEl = $id("qr-storefront-i18n");

if (dataEl && i18nEl) {
  const assetVersion = new URL(import.meta.url).search;
  const [{ createCart, readLocal, writeLocal, showProductOptions }, { setupCheckout }, { setupTracking }] =
    await Promise.all([
      import("./storefront-cart.js" + assetVersion),
      import("./storefront-checkout.js" + assetVersion),
      import("./storefront-tracking.js" + assetVersion),
    ]);

  const data = JSON.parse(dataEl.textContent);
  const bundle = JSON.parse(i18nEl.textContent);
  const cart = createCart(data.establishment_id, data.catalog_version);
  const orderKey = "qrc:orders:" + data.establishment_id;
  const catalog = $id("sf-catalog-view");
  const offlineBanner = $id("sf-status-banner");
  const search = $id("sf-search");
  const modal = $id("sf-product-modal");
  const sections = { catalog, checkout: $id("sf-checkout-section"), tracking: $id("sf-tracking-section") };
  const productIndex = new Map(data.categories.flatMap(category => category.products.map(product => [product.id, product])));
  const modeButtons = document.querySelectorAll("[data-sf-mode]");
  const categoryControls = document.querySelectorAll("[data-sf-cat]");
  const formatter = new Map();
  let category = "all";
  let locale = "pt-BR";
  let mode = data.schedule.delivery ? "delivery" : "pickup";
  let screen = "catalog";
  let installPrompt;
  const install = $id("sf-install-app");
  window.addEventListener("beforeinstallprompt", event => {
    event.preventDefault();
    installPrompt = event;
    install.hidden = false;
  });
  install.addEventListener("click", async () => {
    if (!installPrompt) return;
    const prompt = installPrompt;
    installPrompt = undefined;
    install.hidden = true;
    try {
      await prompt.prompt();
    } catch {
      // The browser can discard a prompt when the installation state changes.
    }
  });
  window.addEventListener("appinstalled", () => {
    installPrompt = undefined;
    install.hidden = true;
  });

  function t(key) {
    return bundle[locale]?.[key] || bundle["pt-BR"]?.[key] || (key ? bundle[locale]["error.internal"] : "");
  }

  function money(centavos) {
    if (!formatter.has(locale)) {
      formatter.set(locale, new Intl.NumberFormat(locale, { minimumFractionDigits: 2, maximumFractionDigits: 2 }));
    }
    return "R$ " + formatter.get(locale).format(centavos / 100);
  }

  function localized(item, field = "name") {
    return (locale !== "pt-BR" && item[field + "_" + locale]) || item[field] || "";
  }

  function orders() {
    const saved = readLocal(orderKey, []);
    return Array.isArray(saved) ? saved : [];
  }

  function show(name) {
    screen = name;
    Object.entries(sections).forEach(([key, section]) => { section.hidden = key !== name; });
    document.querySelectorAll("[data-sf-catalog-control]").forEach(control => {
      control.hidden = name !== "catalog";
    });
    refreshCartUi();
    sections[name].focus({ preventScroll: true });
    sections[name].scrollIntoView({ block: "start" });
  }

  function refreshCartUi() {
    const quantity = cart.lines.reduce((total, line) => total + line.quantity, 0);
    $id("sf-cart-bar").hidden = quantity < 1 || screen !== "catalog";
    $id("sf-cart-summary").textContent = String(quantity);
    $id("sf-order-history").hidden = !orders().length;
  }

  function updateConnectivity() {
    const closed = !data.schedule.delivery && !data.schedule.pickup;
    offlineBanner.hidden = navigator.onLine && !closed;
    offlineBanner.dataset.i18n = closed ? "delivery.ordering_unavailable" : "storefront.offline";
    offlineBanner.textContent = t(offlineBanner.dataset.i18n);
    checkout.refreshQuote();
  }

  async function request(path, payload, options = {}) {
    const hasPayload = payload !== undefined;
    const response = await fetch("/api/storefront/v1/" + encodeURIComponent(data.slug) + path, {
      method: hasPayload ? "POST" : "GET",
      body: hasPayload ? JSON.stringify(payload) : undefined,
      headers: {
        "Accept-Language": locale,
        ...(hasPayload ? { "Content-Type": "application/json" } : {}),
        ...options.headers,
      },
      signal: options.signal,
      cache: "no-store",
    });
    const body = response.status === 304 ? {} : await response.json().catch(() => ({}));
    return { response, body };
  }

  function filter() {
    const normalize = value => value.normalize("NFD").toLowerCase().replace(/\p{Diacritic}/gu, "").trim();
    const query = normalize(search.value);
    let visible = 0;
    document.querySelectorAll("[data-sf-category]").forEach(section => {
      let shown = 0;
      section.querySelectorAll("[data-sf-product-id]").forEach(card => {
        const item = productIndex.get(card.dataset.sfProductId);
        const matches = (category === "all" || category === section.dataset.sfCategory) &&
          normalize(localized(item) + " " + localized(item, "description")).includes(query);
        card.hidden = !matches;
        if (matches) shown += 1;
      });
      section.hidden = shown === 0;
      visible += shown;
    });
    $id("sf-no-results").hidden = visible > 0 || !productIndex.size;
  }

  function applyLocale(nextLocale) {
    locale = Object.hasOwn(bundle, nextLocale) ? nextLocale : "pt-BR";
    document.documentElement.lang = locale;
    document.querySelectorAll("[data-i18n]").forEach(el => { el.textContent = t(el.dataset.i18n); });
    document.querySelectorAll("[data-i18n-placeholder]").forEach(el => { el.placeholder = t(el.dataset.i18nPlaceholder); });
    document.querySelectorAll("[data-i18n-aria]").forEach(el => { el.setAttribute("aria-label", t(el.dataset.i18nAria)); });
    document.querySelectorAll("[data-i18n-text-pt]").forEach(el => {
      const lang = locale === "pt-BR" ? "pt" : locale;
      el.textContent = el.getAttribute("data-i18n-text-" + lang) || el.dataset.i18nTextPt;
    });
    document.querySelectorAll("[data-price-centavos]").forEach(el => { el.textContent = money(Number(el.dataset.priceCentavos)); });
    document.querySelectorAll("[data-lang-btn]").forEach(el => {
      const active = el.dataset.langBtn === locale;
      el.classList.toggle("is-active", active);
      el.setAttribute("aria-current", String(active));
    });
    $id("sf-privacy-notice").textContent = t("storefront.privacy_notice").replace("{establishment}", data.name);
    window.dispatchEvent(new CustomEvent("qr:langchange", { detail: { locale, dict: bundle[locale] } }));
    checkout.localize();
    tracking.localize();
    filter();
  }

  const tracking = setupTracking({ $id, t, localized, money, request, orders, show });
  const checkout = setupCheckout({
    data, $id, t, money, localized, request, cart, readLocal, writeLocal,
    productFor: id => productIndex.get(id),
    configuration: () => ({ mode, catalogVersion: data.catalog_version }),
    online: () => navigator.onLine,
    cartChanged: refreshCartUi,
    complete(order) {
      const historyItem = { order_id: order.order_id, cancel_token: order.cancel_token };
      writeLocal(orderKey, [historyItem, ...orders().filter(item => item.order_id !== order.order_id)].slice(0, 20));
      refreshCartUi();
      tracking.open(order);
    },
  });

  function addLine(line) {
    const sameOptions = item => item.variation_id === line.variation_id &&
      item.product_id === line.product_id && JSON.stringify(item.options) === JSON.stringify(line.options);
    const index = cart.lines.findIndex(sameOptions);
    if (index >= 0 && cart.lines[index].quantity < 99) {
      cart.update(cart.lines.map((item, i) => i === index ? { ...item, quantity: item.quantity + 1 } : item));
    } else if (cart.lines.length < 50) {
      cart.update([...cart.lines, line]);
    } else {
      const error = modal.open ? $id("sf-modal-error") : $id("sf-catalog-error");
      error.textContent = t("storefront.cart_limit");
      error.hidden = false;
      return false;
    }
    $id("sf-catalog-error").hidden = true;
    refreshCartUi();
    return true;
  }

  document.querySelectorAll("[data-sf-open-product]").forEach(button => {
    button.disabled = !data.schedule.delivery && !data.schedule.pickup;
    button.addEventListener("click", () => {
      const product = productIndex.get(button.dataset.sfOpenProduct);
      if (!product?.is_available) return;
      if (product.variations.length || product.addon_groups.length) {
        showProductOptions(modal, product, { t, localized, money }, addLine);
      } else {
        addLine({ product_id: product.id, variation_id: null, quantity: 1, options: [] });
      }
    });
  });

  function updateMode() {
    modeButtons.forEach(button => {
      const active = button.dataset.sfMode === mode;
      button.classList.toggle("is-active", active);
      button.setAttribute("aria-pressed", String(active));
    });
    checkout.setMode();
  }

  modeButtons.forEach(button => button.addEventListener("click", () => {
    if (checkout.busy || screen === "tracking") return;
    mode = button.dataset.sfMode;
    updateMode();
  }));

  categoryControls.forEach(control => control.addEventListener("click", () => {
    category = control.dataset.sfCat;
    categoryControls.forEach(button => {
      const active = button === control;
      button.classList.toggle("is-active", active);
      button.setAttribute("aria-pressed", String(active));
    });
    filter();
  }));
  search.addEventListener("input", filter);
  $id("sf-modal-close").addEventListener("click", () => modal.close());
  modal.addEventListener("click", event => { if (event.target === modal) modal.close(); });
  $id("sf-open-checkout").addEventListener("click", () => { show("checkout"); checkout.open(); });
  $id("sf-back-to-menu").addEventListener("click", () => { if (!checkout.busy) show("catalog"); });
  $id("sf-track-back").addEventListener("click", () => { tracking.close(); show("catalog"); });

  const params = new URLSearchParams(location.search);
  applyLocale(params.get("lang") || readLocal("qr_lang", null));
  updateMode();
  refreshCartUi();
  updateConnectivity();
  window.addEventListener("offline", updateConnectivity);
  window.addEventListener("online", updateConnectivity);
  document.querySelectorAll("[data-lang-btn]").forEach(choice => choice.addEventListener("click", event => {
    event.preventDefault();
    const selected = choice.dataset.langBtn;
    writeLocal("qr_lang", selected);
    applyLocale(selected);
  }));
  const trackedOrder = params.get("order");
  if (trackedOrder) tracking.open(orders().find(item => item.order_id === trackedOrder) || { order_id: trackedOrder });

  if ("serviceWorker" in navigator) navigator.serviceWorker.register("./sw.js").catch(() => {});
}
