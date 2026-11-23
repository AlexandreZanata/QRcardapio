(() => {
  "use strict";

  function parseCurrencyCentavos(raw) {
    if (!raw) return 0;
    const cleaned = String(raw).trim().replace(/[^\d.,-]/g, "");
    if (!cleaned) return 0;
    const normalized = cleaned.includes(",")
      ? cleaned.replace(/\./g, "").replace(",", ".")
      : cleaned;
    const parts = normalized.split(".");
    if (parts.length > 2 || !/^-?\d+$/.test(parts[0] || "0")) return NaN;
    const whole = parseInt(parts[0] || "0", 10);
    const fracRaw = (parts[1] || "").slice(0, 2).padEnd(2, "0");
    if (parts[1] && !/^\d+$/.test(parts[1])) return NaN;
    const frac = parseInt(fracRaw || "0", 10);
    return whole < 0 ? whole * 100 - frac : whole * 100 + frac;
  }

  function formatMoney(centavos, locale) {
    if (window.QRI18n && typeof window.QRI18n.formatPriceCentavos === "function") {
      return window.QRI18n.formatPriceCentavos(centavos, locale);
    }
    const abs = Math.abs(Math.trunc(centavos));
    const whole = Math.floor(abs / 100);
    const cents = String(abs % 100).padStart(2, "0");
    const dec = locale === "en" ? "." : ",";
    return "R$ " + whole + dec + cents;
  }

  function initQrTool() {
    const root = document.querySelector('[data-seo-tool="qr"]');
    if (!root) return;
    const input = document.getElementById("seo-qr-url");
    const preview = document.getElementById("seo-qr-preview");
    const caption = document.getElementById("seo-qr-caption");
    const svgBtn = document.getElementById("seo-qr-download-svg");
    const pngBtn = document.getElementById("seo-qr-download-png");
    if (!input || !preview || !svgBtn || !pngBtn) return;

    let timer = null;
    const sync = () => {
      const val = input.value.trim();
      if (!/^https?:\/\/\S+/i.test(val)) return;
      const enc = encodeURIComponent(val);
      preview.src = "/api/tools/qr.svg?url=" + enc;
      svgBtn.href = "/api/tools/qr.svg?url=" + enc + "&download=1";
      pngBtn.href = "/api/tools/qr.png?url=" + enc + "&download=1";
      if (caption) caption.textContent = val;
    };

    input.addEventListener("input", () => {
      clearTimeout(timer);
      timer = setTimeout(sync, 240);
    });
  }

  function initCmvTool() {
    const root = document.querySelector('[data-seo-tool="cmv"]');
    if (!root) return;
    const locale = root.dataset.locale || "pt-BR";
    const ingEl = document.getElementById("cmv-ingredients");
    const packEl = document.getElementById("cmv-packaging");
    const targetEl = document.getElementById("cmv-target");
    const feesEl = document.getElementById("cmv-fees");
    const outCost = document.getElementById("cmv-out-cost");
    const outPrice = document.getElementById("cmv-out-price");
    const outMargin = document.getElementById("cmv-out-margin");
    const outProfit100 = document.getElementById("cmv-out-profit100");
    const warnEl = document.getElementById("cmv-out-warning");

    const compute = () => {
      const ing = parseCurrencyCentavos(ingEl.value);
      const pack = parseCurrencyCentavos(packEl.value);
      const targetBps = Math.round(Number(targetEl.value || "0") * 100);
      const feesBps = Math.round(Number(feesEl.value || "0") * 100);
      const totalCost = ing + pack;
      const netShareBps = 10000 - feesBps;

      const invalid =
        !Number.isFinite(totalCost) ||
        totalCost <= 0 ||
        targetBps <= 0 ||
        feesBps < 0 ||
        netShareBps <= 0 ||
        targetBps >= netShareBps;

      if (warnEl) warnEl.hidden = !invalid;
      if (invalid) return;

      // Exact target food-cost share over net revenue after payment fees.
      const denomBps = Math.round((targetBps * netShareBps) / 10000);
      if (denomBps <= 0) {
        if (warnEl) warnEl.hidden = false;
        return;
      }
      const priceCentavos = Math.round((totalCost * 10000) / denomBps);
      const feeCentavos = Math.round((priceCentavos * feesBps) / 10000);
      const marginCentavos = priceCentavos - totalCost - feeCentavos;
      const marginPct = priceCentavos > 0 ? Math.round((marginCentavos * 100) / priceCentavos) : 0;

      outCost.textContent = formatMoney(totalCost, locale);
      outPrice.textContent = formatMoney(priceCentavos, locale);
      outMargin.textContent = formatMoney(marginCentavos, locale) + " (" + marginPct + "%)";
      outProfit100.textContent = formatMoney(marginCentavos * 100, locale);
    };

    [ingEl, packEl, targetEl, feesEl].forEach((el) => {
      if (el) el.addEventListener("input", compute);
    });
    root.querySelectorAll("[data-cmv-preset]").forEach((btn) => {
      btn.addEventListener("click", () => {
        targetEl.value = btn.dataset.cmvPreset;
        compute();
      });
    });
    compute();
  }

  function initPlacardTool() {
    const root = document.querySelector('[data-seo-tool="placard"]');
    if (!root) return;
    const nameIn = document.getElementById("placard-name");
    const ctaIn = document.getElementById("placard-cta");
    const urlIn = document.getElementById("placard-url");
    const nameOut = document.getElementById("placard-preview-name");
    const ctaOut = document.getElementById("placard-preview-cta");
    const urlOut = document.getElementById("placard-preview-url");
    const qrOut = document.getElementById("placard-preview-qr");
    const printBtn = document.getElementById("placard-print-btn");

    if (nameIn && nameOut) {
      nameIn.addEventListener("input", () => {
        nameOut.textContent = nameIn.value.trim() || "QR Cardápio";
      });
    }
    if (ctaIn && ctaOut) {
      ctaIn.addEventListener("input", () => {
        ctaOut.textContent = ctaIn.value.trim();
      });
    }
    let timer = null;
    if (urlIn && qrOut) {
      urlIn.addEventListener("input", () => {
        const val = urlIn.value.trim();
        if (urlOut) urlOut.textContent = val;
        if (!/^https?:\/\/\S+/i.test(val)) return;
        clearTimeout(timer);
        timer = setTimeout(() => {
          qrOut.src = "/api/tools/qr.svg?url=" + encodeURIComponent(val);
        }, 240);
      });
    }
    if (printBtn) {
      printBtn.addEventListener("click", () => window.print());
    }
  }

  initQrTool();
  initCmvTool();
  initPlacardTool();
})();
