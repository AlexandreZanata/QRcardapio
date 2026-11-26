const ADDRESS_FIELDS = ["street", "number", "complement", "city", "uf", "cep", "reference"];
const TOTAL_FIELDS = ["items_subtotal_centavos", "discount_centavos", "delivery_fee_centavos", "total_centavos"];

function parseCentavos(value) {
  if (!value) return null;
  const match = /^(\d{1,9})(?:[,.](\d{1,2}))?$/.exec(value);
  if (!match) return null;
  return Number(match[1]) * 100 + Number((match[2] ?? "").padEnd(2, "0"));
}

export function setupCheckout({ data, $id, t, money, localized, productFor, cart, request, configuration, online, complete, cartChanged, readLocal, writeLocal }) {
  const form = $id("sf-checkout-form");
  const section = $id("sf-checkout-section");
  const zone = $id("sf-zone-select");
  const payment = $id("sf-payment-method");
  const place = $id("sf-place-order-btn");
  const box = $id("sf-quote-box");
  const cartItems = $id("sf-cart-items");
  const error = $id("sf-checkout-error");
  const coupon = $id("sf-coupon-input");
  const mismatchNotice = $id("sf-idempotency-warning");
  const remember = $id("sf-remember-address");
  const forget = $id("sf-forget-address");
  const addressKey = "qrc:customer:" + data.establishment_id;
  let initializedAddress = false;
  let quote;
  let quoteController;
  let busy = false;
  let mismatchedAttempt = false;
  let errorKey = "";

  function setError(key) {
    errorKey = key;
    error.hidden = !key;
    error.textContent = key ? t(key) : "";
  }

  function updateButton() {
    place.disabled = !quote || !online() || busy || mismatchedAttempt;
    form.inert = busy;
  }

  function renderQuote(data, failure = "") {
    const valid = data && data.open_now && data.lines?.every(line => line.available) &&
      !data.issues?.length && TOTAL_FIELDS.every(key => Number.isSafeInteger(data[key]));
    quote = valid ? data : undefined;
    box.replaceChildren();
    if (data && Number.isSafeInteger(data.total_centavos)) {
      TOTAL_FIELDS.forEach((key, index) => {
        const row = document.createElement("div");
        row.className = "sf-cart-row";
        const label = document.createElement("span");
        label.textContent = t("storefront.quote_" + key.replace("_centavos", ""));
        const value = document.createElement(index === 3 ? "strong" : "span");
        value.textContent = money(data[key]);
        row.append(label, value);
        box.append(row);
      });
    }
    box.hidden = !box.childElementCount;
    setError(failure || (!data ? "" :
      data.issues?.[0] || (!data.open_now ? "delivery.closed" :
        data.lines?.some(line => !line.available) ? "storefront.cart_unavailable" :
          data.coupon?.status === "invalid" ? "storefront.invalid_coupon" : "")));
    updateButton();
  }

  function renderCart() {
    cartItems.replaceChildren();
    cart.lines.forEach((line, index) => {
      const product = productFor(line.product_id);
      const variation = product?.variations.find(item => item.id === line.variation_id);
      const row = document.createElement("div");
      row.className = "sf-cart-row";
      const label = document.createElement("span");
      label.textContent = line.quantity + " × " +
        (product ? localized(product) : t("storefront.cart_unavailable_item")) +
        (variation ? " · " + localized(variation) : "");
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "sf-btn-secondary";
      remove.textContent = t("commerce.action.remove");
      remove.addEventListener("click", () => {
        if (busy) return;
        cart.update(cart.lines.filter((_, itemIndex) => itemIndex !== index));
        cartChanged();
        renderCart();
        refreshQuote();
      });
      row.append(label, remove);
      cartItems.append(row);
    });
    if (!cart.lines.length) {
      const empty = document.createElement("p");
      empty.textContent = t("storefront.empty_cart");
      cartItems.append(empty);
    }
  }

  function quotePayload() {
    const { mode, catalogVersion } = configuration();
    return {
      catalog_version: catalogVersion,
      fulfillment: mode,
      zone_id: mode === "delivery" ? zone.value || null : null,
      coupon_code: coupon.value.trim() || null,
      lines: cart.lines,
    };
  }

  function abortQuote() {
    quoteController?.abort();
    quoteController = undefined;
    quote = undefined;
    updateButton();
  }

  async function refreshQuote() {
    if (busy || section.hidden) return;
    abortQuote();
    renderQuote(null);
    if (!cart.lines.length) return;
    if (!online()) {
      setError("storefront.offline");
      return;
    }
    const payload = quotePayload();
    if (payload.fulfillment === "delivery" && !payload.zone_id) {
      setError("storefront.select_zone");
      return;
    }
    const controller = new AbortController();
    quoteController = controller;
    try {
      const { response, body } = await request("/quotes", payload, { signal: controller.signal });
      if (controller.signal.aborted) return;
      const repriced = body.catalog_version !== payload.catalog_version && response.ok;
      renderQuote(response.ok ? body : null, response.ok
        ? repriced ? "commerce.checkout_repriced" : ""
        : body.code || "error.internal");
    } catch {
      if (!controller.signal.aborted) setError(online() ? "error.internal" : "storefront.offline");
    }
  }

  function setMode() {
    const isDelivery = configuration().mode === "delivery";
    $id("sf-delivery-fields").hidden = !isDelivery;
    ["zone_id", "street", "number", "city", "uf"].forEach(name => {
      form.elements.namedItem(name).required = isDelivery;
    });
    refreshQuote();
  }

  function populateSelects() {
    const previousZone = zone.value;
    const previousPayment = payment.value;
    zone.replaceChildren(new Option(t("storefront.select_zone"), ""));
    data.zones.forEach(item => {
      zone.add(new Option(item.neighborhood + " — " + item.city + " · " + money(item.fee_centavos), item.id));
    });
    zone.value = previousZone;
    const methods = [
      ["cash", data.payments.accept_cash],
      ["card_on_delivery", data.payments.accept_card_on_delivery],
      ["pix_on_delivery", data.payments.accept_pix_on_delivery],
    ];
    payment.replaceChildren();
    methods.filter(([, accepted]) => accepted).forEach(([method]) => {
      payment.add(new Option(t("delivery.payment." + method), method));
    });
    if (previousPayment) payment.value = previousPayment;
    toggleChangeField();
  }

  function prefillAddress() {
    const saved = readLocal(addressKey, null);
    remember.checked = Boolean(saved);
    forget.hidden = !saved;
    if (!saved) return;
    if (data.zones.some(item => item.id === saved.zone_id)) zone.value = saved.zone_id;
    for (const name of ADDRESS_FIELDS) {
      if (typeof saved[name] === "string") form.elements.namedItem(name).value = saved[name];
    }
  }

  function removeAddress() {
    try {
      localStorage.removeItem(addressKey);
    } catch {
      // Storage can be blocked by browser settings.
    }
    forget.hidden = true;
  }

  function toggleChangeField() {
    $id("sf-change-field").hidden = payment.value !== "cash";
    $id("sf-pix-hint").hidden = payment.value !== "pix_on_delivery";
  }

  async function submit(event) {
    event.preventDefault();
    if (!quote || busy || !online() || mismatchedAttempt) return;
    const fields = new FormData(form);
    const value = name => String(fields.get(name) || "").trim();
    const change = payment.value === "cash" ? value("change_for") : "";
    const changeFor = parseCentavos(change);
    if (change && (changeFor === null || changeFor < quote.total_centavos)) {
      setError("commerce.invalid_cash_change");
      return;
    }
    const payload = quotePayload();
    payload.catalog_version = quote.catalog_version;
    payload.expected_total_centavos = quote.total_centavos;
    payload.expected_totals = Object.fromEntries(TOTAL_FIELDS.map(key => [key, quote[key]]));
    payload.customer = { name: value("customer_name"), phone: value("customer_phone") };
    payload.address = payload.fulfillment === "delivery" ? {
      neighborhood_zone_id: payload.zone_id,
      ...Object.fromEntries(ADDRESS_FIELDS.map(key => [key, value(key)])),
    } : null;
    payload.payment = { method: payment.value, change_for_centavos: changeFor };
    payload.note = value("note") || null;
    payload.website = value("website");

    busy = true;
    cart.persist();
    setError("");
    updateButton();
    try {
      const headers = { "Idempotency-Key": cart.idempotencyKey };
      const { response, body } = await request("/orders", payload, { headers });
      if (response.status === 201) {
        if (remember.checked && payload.address) {
          writeLocal(addressKey, {
            zone_id: payload.zone_id,
            ...Object.fromEntries(ADDRESS_FIELDS.map(key => [key, payload.address[key]])),
          });
        }
        form.reset();
        prefillAddress();
        toggleChangeField();
        abortQuote();
        cart.update([]);
        cartChanged();
        complete(body);
      } else if (response.status === 409 && body.quote) {
        cart.renewKey();
        const invalidCoupon = body.quote.coupon?.status === "invalid";
        if (invalidCoupon) coupon.value = "";
        renderQuote(body.quote, invalidCoupon ? "storefront.invalid_coupon" : "commerce.checkout_repriced");
      } else if (body.code === "commerce.idempotency_mismatch") {
        mismatchedAttempt = true;
        mismatchNotice.hidden = false;
        requestAnimationFrame(() => $id("sf-prepare-new-attempt").focus());
      } else {
        setError(body.code || "storefront.unconfirmed_send");
      }
    } catch {
      setError("storefront.unconfirmed_send");
    } finally {
      busy = false;
      updateButton();
    }
  }

  form.addEventListener("submit", submit);
  $id("sf-prepare-new-attempt").addEventListener("click", () => {
    if (!mismatchedAttempt || busy) return;
    cart.renewKey();
    mismatchedAttempt = false;
    mismatchNotice.hidden = true;
    refreshQuote();
  });
  zone.addEventListener("change", () => {
    const selected = data.zones.find(item => item.id === zone.value);
    if (selected) {
      form.elements.namedItem("city").value = selected.city;
    }
    refreshQuote();
  });
  payment.addEventListener("change", toggleChangeField);
  remember.addEventListener("change", () => {
    if (!remember.checked) removeAddress();
  });
  forget.addEventListener("click", () => {
    removeAddress();
    remember.checked = false;
    zone.value = "";
    for (const name of ADDRESS_FIELDS) form.elements.namedItem(name).value = "";
    refreshQuote();
  });
  coupon.addEventListener("input", () => {
    abortQuote();
    renderQuote(null);
  });
  coupon.addEventListener("change", refreshQuote);
  $id("sf-apply-coupon").addEventListener("click", refreshQuote);

  return {
    get busy() { return busy; },
    open() {
      renderCart();
      refreshQuote();
    },
    setMode,
    refreshQuote,
    localize() {
      const previousError = errorKey;
      const visibleQuote = quote;
      populateSelects();
      if (!initializedAddress) {
        prefillAddress();
        initializedAddress = true;
      }
      if (!section.hidden) {
        renderCart();
        renderQuote(visibleQuote, previousError);
      }
    },
  };
}
