(() => {
  "use strict";

  const COOKIE_NAME = "qr_lang";
  const STORAGE_KEY = "qr_lang";
  const cache = Object.create(null);

  function normalizeLocale(raw) {
    if (!raw) return "pt-BR";
    const s = String(raw).trim().toLowerCase().replace("_", "-");
    if (s === "en" || s.startsWith("en-")) return "en";
    if (s === "es" || s.startsWith("es-")) return "es";
    return "pt-BR";
  }

  function formatPriceCentavos(centavosRaw, locale) {
    const num = Number(centavosRaw);
    if (!Number.isFinite(num)) return "";
    const abs = Math.abs(Math.trunc(num));
    const whole = Math.floor(abs / 100);
    const cents = String(abs % 100).padStart(2, "0");
    const thousandsSep = locale === "en" ? "," : ".";
    const decimalSep = locale === "en" ? "." : ",";
    const digits = String(whole);
    let grouped = "";
    for (let i = 0; i < digits.length; i++) {
      if (i > 0 && (digits.length - i) % 3 === 0) {
        grouped += thousandsSep;
      }
      grouped += digits[i];
    }
    const prefix = num < 0 ? "-R$ " : "R$ ";
    return prefix + grouped + decimalSep + cents;
  }

  function applyTemplate(text, argsJson) {
    if (!argsJson) return text;
    try {
      const args = JSON.parse(argsJson);
      return text.replace(/\{([a-zA-Z0-9_]+)\}/g, (_, k) =>
        Object.prototype.hasOwnProperty.call(args, k) ? String(args[k]) : "{" + k + "}"
      );
    } catch {
      return text;
    }
  }

  function persistLocale(locale) {
    try {
      localStorage.setItem(STORAGE_KEY, locale);
    } catch {
      // Ignore storage failures in restricted contexts.
    }
    const secure = window.location.protocol === "https:" ? "; Secure" : "";
    document.cookie =
      COOKIE_NAME +
      "=" +
      encodeURIComponent(locale) +
      "; Path=/; Max-Age=31536000; SameSite=Lax" +
      secure;
  }

  async function loadDictionary(locale) {
    if (cache[locale]) return cache[locale];
    const ver = document.documentElement.dataset.i18nVersion || "1";
    const res = await fetch("/static/i18n/" + locale + ".json?v=" + encodeURIComponent(ver), {
      credentials: "same-origin",
    });
    if (!res.ok) throw new Error("Failed to load i18n catalog");
    const dict = await res.json();
    cache[locale] = dict;
    return dict;
  }

  function updateButtons(locale) {
    document.querySelectorAll("[data-lang-btn]").forEach((btn) => {
      const active = normalizeLocale(btn.dataset.langBtn) === locale;
      btn.classList.toggle("is-active", active);
      btn.setAttribute("aria-current", String(active));
    });
  }

  function applyDictionary(dict, locale) {
    document.documentElement.lang = locale;

    const titleKey = document.body.dataset.pageTitleKey;
    if (titleKey && dict[titleKey]) {
      document.title = applyTemplate(dict[titleKey], document.body.dataset.pageTitleArgs);
    }

    document.querySelectorAll("[data-i18n]").forEach((el) => {
      const key = el.dataset.i18n;
      if (key && Object.prototype.hasOwnProperty.call(dict, key)) {
        el.textContent = applyTemplate(dict[key], el.dataset.i18nArgs);
      }
    });

    document.querySelectorAll("[data-i18n-placeholder]").forEach((el) => {
      const key = el.dataset.i18nPlaceholder;
      if (key && Object.prototype.hasOwnProperty.call(dict, key)) {
        el.setAttribute("placeholder", dict[key]);
      }
    });

    document.querySelectorAll("[data-i18n-aria]").forEach((el) => {
      const key = el.dataset.i18nAria;
      if (key && Object.prototype.hasOwnProperty.call(dict, key)) {
        el.setAttribute("aria-label", dict[key]);
      }
    });

    document.querySelectorAll("[data-i18n-title]").forEach((el) => {
      const key = el.dataset.i18nTitle;
      if (key && Object.prototype.hasOwnProperty.call(dict, key)) {
        el.setAttribute("title", dict[key]);
      }
    });

    document.querySelectorAll("[data-price-centavos]").forEach((el) => {
      el.textContent = formatPriceCentavos(el.dataset.priceCentavos, locale);
    });

    updateButtons(locale);
  }

  async function switchLanguage(nextRaw) {
    const next = normalizeLocale(nextRaw);
    persistLocale(next);
    try {
      const dict = await loadDictionary(next);
      applyDictionary(dict, next);
      window.dispatchEvent(new CustomEvent("qr:langchange", { detail: { locale: next, dict } }));
    } catch {
      // Progressive enhancement fallback: query param navigation works even if fetch fails.
      const url = new URL(window.location.href);
      url.searchParams.set("lang", next);
      window.location.href = url.toString();
    }
  }

  window.QRI18n = {
    switchLanguage,
    formatPriceCentavos,
    normalizeLocale,
    getActiveLocale: () => normalizeLocale(document.documentElement.lang),
    t: (key) => {
      const loc = normalizeLocale(document.documentElement.lang);
      return (cache[loc] && cache[loc][key]) || null;
    },
  };

  document.addEventListener("click", (ev) => {
    const btn = ev.target.closest("[data-lang-btn]");
    if (!btn) return;
    ev.preventDefault();
    switchLanguage(btn.dataset.langBtn);
  });

  // Prime current dictionary silently so dynamic confirmations (e.g. copy toast / confirm) can read translated strings.
  const current = normalizeLocale(document.documentElement.lang);
  loadDictionary(current).catch(() => {});
})();
