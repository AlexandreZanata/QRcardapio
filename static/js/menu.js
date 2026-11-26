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

    document.querySelectorAll("[data-menu-localized-link]").forEach((link) => {
      const url = new URL(link.href);
      url.searchParams.set("lang", locale);
      link.href = url.pathname + url.search + url.hash;
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
  const categoryFilters = Array.from(document.querySelectorAll("[data-cat-filter]"));
  const sections = Array.from(document.querySelectorAll("[data-cat-section]"));
  const hasAnyItems = Boolean(document.querySelector("[data-menu-item]"));
  const menuPath = window.location.pathname.replace(/\/$/, "");
  const returnKey = "qr_menu_return:" + menuPath;
  let activeCategory = "all";

  function searchText(value) {
    return value.normalize("NFD").replace(/\p{Diacritic}/gu, "").toLowerCase().trim();
  }

  function filterMenu() {
    const q = searchText(searchInput?.value || "");
    let totalVisible = 0;

    sections.forEach((sec) => {
      const catId = sec.dataset.catSection;
      const catMatches = activeCategory === "all" || activeCategory === catId;
      if (!catMatches) {
        sec.classList.add("is-hidden");
        return;
      }

      let secVisible = 0;
      sec.querySelectorAll("[data-menu-item]").forEach((card) => {
        const title = card.querySelector(".catalog-product-name");
        const description = card.querySelector("[data-menu-search-description]");
        const text = title
          ? (title.textContent || "") + " " + (description?.textContent || "")
          : card.textContent || "";
        const show = !q || searchText(text).includes(q);
        card.classList.toggle("is-hidden", !show);
        if (show) secVisible++;
      });

      sec.classList.toggle("is-hidden", secVisible === 0);
      totalVisible += secVisible;
    });

    if (noResultsBox) {
      noResultsBox.classList.toggle("is-hidden", !hasAnyItems || totalVisible > 0);
    }
  }

  function selectCategory(category) {
    activeCategory = category;
    categoryFilters.forEach((control) => {
      const active = control.dataset.catFilter === category;
      control.classList.toggle("is-active", active);
      if (control.matches("a")) {
        if (active) control.setAttribute("aria-current", "true");
        else control.removeAttribute("aria-current");
      } else {
        control.setAttribute("aria-pressed", String(active));
      }
    });
    filterMenu();
  }

  categoryFilters.forEach((control) => {
    control.addEventListener("click", (event) => {
      if (event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
      event.preventDefault();
      selectCategory(control.dataset.catFilter);
      if (control.matches("a")) {
        document.getElementById("menu-content")?.scrollIntoView({ block: "start" });
      }
    });
  });

  if (searchInput) searchInput.addEventListener("input", filterMenu);
  if (sections.length) window.addEventListener("qr:langchange", filterMenu);
  document.querySelectorAll("[data-menu-search-control]").forEach((control) => {
    control.hidden = false;
  });

  document.querySelectorAll("[data-menu-product-link]").forEach((link) => {
    link.addEventListener("click", (event) => {
      if (event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
      try {
        sessionStorage.setItem(returnKey, JSON.stringify({
          productPath: new URL(link.href).pathname,
          productId: link.id,
          query: searchInput?.value || "",
          category: activeCategory,
        }));
      } catch {
        // The product link still works when session storage is unavailable.
      }
    });
  });

  function restoreMenu() {
    if (!searchInput || !document.querySelector("[data-menu-product-link]")) return;
    let saved;
    try {
      saved = JSON.parse(sessionStorage.getItem(returnKey));
      if (!saved) return;
      const referrer = document.referrer && new URL(document.referrer);
      const isProductReturn = referrer?.origin === location.origin &&
        referrer.pathname === saved.productPath;
      const isHistoryReturn = performance.getEntriesByType("navigation")[0]?.type === "back_forward";
      if (!isProductReturn && !isHistoryReturn) return;
      sessionStorage.removeItem(returnKey);
    } catch {
      return;
    }

    searchInput.value = typeof saved.query === "string" ? saved.query : "";
    const category = categoryFilters.some((control) => control.dataset.catFilter === saved.category)
      ? saved.category
      : "all";
    selectCategory(category);
    const product = document.getElementById(saved.productId);
    if (product && product.getClientRects().length) {
      // The browser can reset focus while resolving the category fragment on load.
      const focusProduct = () => requestAnimationFrame(() => {
        product.focus({ preventScroll: true });
        product.scrollIntoView({ block: "center" });
      });
      if (document.readyState === "complete") focusProduct();
      else window.addEventListener("pageshow", focusProduct, { once: true });
    }
  }

  // Initialize from saved preference on static pages without reloading
  const paramLang = new URLSearchParams(window.location.search).get("lang");
  let savedLocale;
  try {
    savedLocale = localStorage.getItem(STORAGE_KEY);
  } catch {
    // Ignore storage access errors.
  }
  const initialLocale = paramLang || savedLocale;
  if (initialLocale) applyLocale(initialLocale, Boolean(paramLang));
  restoreMenu();

  if (document.querySelector(".lang-switcher")) {
    import("/static/js/language-picker.js").catch(() => {});
  }
})();
