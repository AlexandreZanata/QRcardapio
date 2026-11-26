const CART_TTL = 24 * 60 * 60 * 1000;

function uuid() {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  bytes[6] = (bytes[6] & 15) | 64;
  bytes[8] = (bytes[8] & 63) | 128;
  const hex = Array.from(bytes, byte => byte.toString(16).padStart(2, "0")).join("");
  return [hex.slice(0, 8), hex.slice(8, 12), hex.slice(12, 16), hex.slice(16, 20), hex.slice(20)].join("-");
}

export function readLocal(key, fallback) {
  try {
    return JSON.parse(localStorage.getItem(key)) ?? fallback;
  } catch {
    return fallback;
  }
}

export function writeLocal(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
    return true;
  } catch {
    return false;
  }
}

export function createCart(establishmentId, catalogVersion) {
  const prefix = "qrc:cart:" + establishmentId + ":v";
  const key = prefix + catalogVersion;
  let olderKeys = [];
  try {
    olderKeys = Object.keys(localStorage)
      .filter(candidate => candidate.startsWith(prefix))
      .map(candidate => [candidate, Number(candidate.slice(prefix.length))])
      .filter(([, version]) => Number.isSafeInteger(version) && version < catalogVersion)
      .sort((a, b) => b[1] - a[1])
      .map(([candidate]) => candidate);
  } catch {
    // Storage can be unavailable in restrictive browser modes.
  }
  const saved = readLocal(key, null);
  const previous = saved ?? olderKeys.reduce((found, olderKey) => found ?? readLocal(olderKey, null), null);
  const keep = previous && Array.isArray(previous.lines) && Number.isFinite(previous.updated_at) &&
    Date.now() - previous.updated_at < CART_TTL;
  let lines = keep ? previous.lines : [];
  let idempotencyKey = keep && typeof previous.idempotency_key === "string"
    ? previous.idempotency_key : uuid();

  function persist() {
    return writeLocal(key, { lines, idempotency_key: idempotencyKey, updated_at: Date.now() });
  }

  const migrated = !keep || Boolean(saved) || persist();
  try {
    if (migrated) olderKeys.forEach(olderKey => localStorage.removeItem(olderKey));
  } catch {
    // A newer storefront will still prefer its own versioned key.
  }

  return {
    get lines() { return lines; },
    get idempotencyKey() { return idempotencyKey; },
    update(nextLines) {
      lines = nextLines;
      idempotencyKey = uuid();
      persist();
    },
    renewKey() {
      idempotencyKey = uuid();
      persist();
    },
    persist,
  };
}

export function showProductOptions(dialog, product, labels, onAdd) {
  const { t, localized, money } = labels;
  const form = dialog.querySelector("form");
  const options = dialog.querySelector("#sf-modal-options");
  const error = dialog.querySelector("#sf-modal-error");
  options.replaceChildren();
  error.hidden = true;
  dialog.querySelector("#sf-modal-title").textContent = localized(product);
  const fieldset = (label) => {
    const set = document.createElement("fieldset");
    const legend = document.createElement("legend");
    legend.textContent = label;
    set.append(legend);
    options.append(set);
    return set;
  };
  const addField = (set, caption, field) => {
    const label = document.createElement("label");
    label.className = "sf-option";
    label.append(field, document.createTextNode(caption));
    set.append(label);
  };
  const variations = product.variations.filter(variation => variation.is_available);
  if (variations.length) {
    const set = fieldset(t("commerce.catalog.variations"));
    variations.forEach((variation, index) => {
      const radio = document.createElement("input");
      radio.type = "radio";
      radio.name = "variation";
      radio.value = variation.id;
      radio.checked = index === 0;
      addField(set, localized(variation) + " · " + money(variation.price_centavos), radio);
    });
  }
  product.addon_groups.forEach(group => {
    const set = fieldset(localized(group));
    const hint = document.createElement("small");
    hint.textContent = t("storefront.option_range")
      .replace("{min}", group.min_select)
      .replace("{max}", group.max_select);
    set.append(hint);
    group.options.filter(option => option.is_available).forEach(option => {
      const field = document.createElement("input");
      field.type = option.max_quantity > 1 ? "number" : "checkbox";
      if (field.type === "number") {
        field.min = 0;
        field.max = option.max_quantity;
        field.value = 0;
      }
      field.dataset.optionId = option.id;
      addField(set, localized(option) + " · +" + money(option.price_centavos), field);
    });
  });
  form.onsubmit = event => {
    event.preventDefault();
    const optionsById = new Map();
    options.querySelectorAll("[data-option-id]").forEach(field => {
      const quantity = field.type === "checkbox" ? Number(field.checked) : Number(field.value);
      if (quantity > 0) optionsById.set(field.dataset.optionId, quantity);
    });
    const valid = product.addon_groups.every(group => {
      const count = group.options.reduce((total, option) => total + Number(optionsById.has(option.id)), 0);
      return count >= group.min_select && count <= group.max_select;
    });
    if (!valid || (product.variations.length && !variations.length)) {
      error.textContent = t("storefront.invalid_options");
      error.hidden = false;
      return;
    }
    const variation = new FormData(form).get("variation");
    const selected = Array.from(optionsById, ([option_id, quantity]) => ({ option_id, quantity }));
    if (onAdd({ product_id: product.id, variation_id: variation, quantity: 1, options: selected })) {
      dialog.close();
    }
  };
  dialog.showModal();
}
