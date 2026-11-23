(() => {
  "use strict";

  function slugify(raw) {
    return raw
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .slice(0, 64);
  }

  // Onboarding automatic slug suggestion
  const nameInput = document.querySelector("[data-slug-source]");
  const slugInput = document.querySelector("[data-slug-target]");
  if (nameInput && slugInput) {
    let userEdited = slugInput.value.trim().length > 0;
    slugInput.addEventListener("input", () => {
      userEdited = slugInput.value.trim().length > 0;
    });
    nameInput.addEventListener("input", () => {
      if (!userEdited) {
        slugInput.value = slugify(nameInput.value);
      }
    });
  }

  // Accent color swatches synchronized with native color input
  const colorInput = document.querySelector('input[type="color"][name="accent_color"]');
  if (colorInput) {
    document.querySelectorAll("[data-swatch-hex]").forEach((btn) => {
      btn.addEventListener("click", () => {
        colorInput.value = btn.dataset.swatchHex;
      });
    });
  }

  // Copy public menu URL button
  document.querySelectorAll("[data-copy-target]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const target = document.getElementById(btn.dataset.copyTarget);
      if (!target) return;
      const text = target.textContent.trim();
      try {
        await navigator.clipboard.writeText(text);
        const label = btn.querySelector("[data-i18n]") || btn;
        const origKey = label.dataset.i18n || "common.copy";
        const copiedText = (window.QRI18n && window.QRI18n.t("common.copied")) || "Copied!";
        label.textContent = copiedText;
        setTimeout(() => {
          const origText = (window.QRI18n && window.QRI18n.t(origKey)) || origTextFallback(origKey);
          label.textContent = origText;
        }, 2000);
      } catch {
        // Ignore clipboard errors on unsupported origins.
      }
    });
  });

  function origTextFallback(key) {
    return key === "common.copy" ? "Copiar link" : key;
  }

  // Accessible translated confirmation for destructive forms
  document.addEventListener("submit", (ev) => {
    const form = ev.target.closest("form[data-confirm-key]");
    if (!form) return;
    const key = form.dataset.confirmKey;
    const fallback = form.dataset.confirmFallback || "Are you sure?";
    const msg = (window.QRI18n && window.QRI18n.t(key)) || fallback;
    if (!window.confirm(msg)) {
      ev.preventDefault();
    }
  });

  // Dismissible flash alerts
  document.addEventListener("click", (ev) => {
    const dismiss = ev.target.closest("[data-dismiss-alert]");
    if (dismiss) {
      const box = dismiss.closest(".alert");
      if (box) box.remove();
    }
  });

  // Print action button on printable QR sheet
  document.addEventListener("click", (ev) => {
    const printBtn = ev.target.closest('[data-action="print"]');
    if (printBtn) {
      window.print();
    }
  });
})();
