(() => {
  "use strict";

  const COOKIE_NAME = "qr_lang";
  const STORAGE_KEY = "qr_lang";

  function readBundle() {
    const el = document.getElementById("qr-menu-i18n");
    if (!el) return {};
    try {
      return JSON.parse(el.textContent || "{}");
    } catch {
      return {};
    }
  }

  const bundle = readBundle();

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
      if (i > 0 && (digits.length - i) % 3 === 0) grouped += thousandsSep;
      grouped += digits[i];
    }
    return (num < 0 ? "-R$ " : "R$ ") + grouped + decimalSep + cents;
  }

  function pickText(el, locale) {
    const pt = el.dataset.pt || "";
    if (locale === "en") return el.dataset.en || pt;
    if (locale === "es") return el.dataset.es || pt;
    return pt;
  }

  function applyLocale(nextRaw, persist) {
    const locale = normalizeLocale(nextRaw);
    const dict = bundle[locale] || bundle["pt-BR"] || {};

    document.documentElement.lang = locale;

    document.querySelectorAll("[data-i18n]").forEach((el) => {
      const k = el.dataset.i18n;
      if (k && dict[k]) el.textContent = dict[k];
    });

    document.querySelectorAll("[data-i18n-placeholder]").forEach((el) => {
      const k = el.dataset.i18nPlaceholder;
      if (k && dict[k]) el.setAttribute("placeholder", dict[k]);
    });

    document.querySelectorAll("[data-i18n-aria]").forEach((el) => {
      const k = el.dataset.i18nAria;
      if (k && dict[k]) el.setAttribute("aria-label", dict[k]);
    });

    document.querySelectorAll("[data-pt]").forEach((el) => {
      const txt = pickText(el, locale);
      el.textContent = txt;
      if (el.hasAttribute("data-hide-if-empty")) {
        el.classList.toggle("is-hidden", txt.trim().length === 0);
      }
    });

    document.querySelectorAll("[data-price-centavos]").forEach((el) => {
      el.textContent = formatPriceCentavos(el.dataset.priceCentavos, locale);
    });

    document.querySelectorAll("[data-lang-btn]").forEach((btn) => {
      const active = normalizeLocale(btn.dataset.langBtn) === locale;
      btn.classList.toggle("is-active", active);
      btn.setAttribute(btn.matches("a") ? "aria-current" : "aria-pressed", String(active));
    });

    if (persist) {
      try {
        localStorage.setItem(STORAGE_KEY, locale);
      } catch {
        // Ignore storage errors.
      }
      const secure = window.location.protocol === "https:" ? "; Secure" : "";
      document.cookie =
        COOKIE_NAME +
        "=" +
        encodeURIComponent(locale) +
        "; Path=/; Max-Age=31536000; SameSite=Lax" +
        secure;
    }

    window.dispatchEvent(new CustomEvent("qr:langchange", { detail: { locale, dict } }));
  }

  // Zero-reload instant language switcher inside static public menus
  document.addEventListener("click", (ev) => {
    const btn = ev.target.closest("[data-lang-btn]");
    if (!btn) return;
    ev.preventDefault();
    applyLocale(btn.dataset.langBtn, true);
  });

  // Category filter pills + live instant search
  const searchInput = document.getElementById("menu-search");
  const noResultsBox = document.getElementById("menu-no-results");
  let activeCategory = "all";

  function filterMenu() {
    const q = searchInput ? searchInput.value.trim().toLowerCase() : "";
    let totalVisible = 0;

    document.querySelectorAll("[data-cat-section]").forEach((sec) => {
      const catId = sec.dataset.catSection;
      const catMatches = activeCategory === "all" || activeCategory === catId;
      if (!catMatches) {
        sec.classList.add("is-hidden");
        return;
      }

      let secVisible = 0;
      sec.querySelectorAll("[data-menu-item]").forEach((card) => {
        const text = (card.textContent || "").toLowerCase();
        const show = q.length === 0 || text.includes(q);
        card.classList.toggle("is-hidden", !show);
        if (show) secVisible++;
      });

      sec.classList.toggle("is-hidden", secVisible === 0);
      totalVisible += secVisible;
    });

    if (noResultsBox) {
      const hasAnyItems = document.querySelectorAll("[data-menu-item]").length > 0;
      noResultsBox.classList.toggle("is-hidden", !hasAnyItems || totalVisible > 0);
    }
  }

  document.querySelectorAll("[data-cat-filter]").forEach((pill) => {
    pill.addEventListener("click", () => {
      activeCategory = pill.dataset.catFilter;
      document.querySelectorAll("[data-cat-filter]").forEach((p) => {
        const active = p === pill;
        p.classList.toggle("is-active", active);
        p.setAttribute("aria-pressed", active ? "true" : "false");
      });
      filterMenu();
    });
  });

  if (searchInput) {
    searchInput.addEventListener("input", filterMenu);
  }

  // Initialize from saved preference on static pages without reloading
  try {
    const params = new URLSearchParams(window.location.search);
    const paramLang = params.get("lang");
    const saved = localStorage.getItem(STORAGE_KEY);
    if (paramLang) {
      applyLocale(paramLang, true);
    } else if (saved) {
      applyLocale(saved, false);
    }
  } catch {
    // Ignore storage access errors.
  }

  if (document.querySelector(".lang-switcher")) {
    import("/static/js/language-picker.js").catch(() => {});
  }
})();
