(() => {
  "use strict";

  const sidebarToggle = document.querySelector("[data-sidebar-toggle]");
  if (sidebarToggle) {
    const sidebar = sidebarToggle.closest(".dashboard-sidebar");
    const setSidebarOpen = (open) => {
      sidebar.classList.toggle("is-open", open);
      sidebarToggle.setAttribute("aria-expanded", String(open));
    };

    document.body.dataset.sidebarReady = "";
    sidebarToggle.addEventListener("click", () => {
      setSidebarOpen(sidebarToggle.getAttribute("aria-expanded") === "false");
    });
    sidebar.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && sidebar.classList.contains("is-open")) {
        setSidebarOpen(false);
        sidebarToggle.focus();
      }
    });
    sidebar.querySelectorAll(".sidebar-section").forEach((link) => {
      link.addEventListener("click", () => setSidebarOpen(false));
    });
  }

  document.querySelectorAll("[data-dashboard-color]").forEach((field) => {
    const input = field.querySelector('input[type="color"]');
    const output = field.querySelector("[data-color-value]");
    const swatches = field.querySelectorAll("[data-swatch-hex]");
    function updateColor() {
      const hex = input.value.toUpperCase();
      output.value = hex;
      swatches.forEach((swatch) => {
        swatch.setAttribute("aria-pressed", String(swatch.dataset.swatchHex === hex));
      });
    }
    swatches.forEach((swatch) => {
      swatch.addEventListener("click", () => {
        input.value = swatch.dataset.swatchHex;
        input.dispatchEvent(new Event("input", { bubbles: true }));
      });
    });
    input.addEventListener("input", updateColor);
    updateColor();
  });

  document.querySelectorAll("[data-character-count]").forEach((input) => {
    const output = input.closest(".dashboard-field").querySelector("[data-character-output]");
    function updateCount() {
      output.textContent = `${input.value.length} / ${input.maxLength}`;
    }
    input.addEventListener("input", updateCount);
    updateCount();
    output.hidden = false;
  });

  // Copy a displayed address. Hidden controls require clipboard support.
  document.querySelectorAll("[data-copy-target]").forEach((btn) => {
    if (btn.hidden && navigator.clipboard?.writeText) btn.hidden = false;
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
