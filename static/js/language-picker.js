function createLanguageIcon() {
  const icon = document.createElement("span");
  icon.setAttribute("aria-hidden", "true");
  return icon;
}

function upgradePublishedPicker(options) {
  const picker = document.createElement("details");
  picker.className = "lang-picker";

  const trigger = document.createElement("summary");
  trigger.className = "lang-trigger";
  const label = document.createElement("span");
  label.className = "lang-trigger-label";
  label.dataset.i18n = "lang.switcher_label";
  label.textContent = options.getAttribute("aria-label");
  const code = document.createElement("span");
  code.className = "lang-current-code";
  trigger.append(label, createLanguageIcon(), code);

  options.before(picker);
  options.className = "lang-options";
  options.querySelectorAll("[data-lang-btn]").forEach((button) => {
    const icon = createLanguageIcon();
    icon.dataset.langIcon = button.dataset.langBtn;
    button.classList.add("lang-choice");
    button.prepend(icon);
  });
  picker.append(trigger, options);
}

const publishedPickers = document.querySelectorAll(".lang-switcher");
if (publishedPickers.length) {
  const stylesheet = document.createElement("link");
  stylesheet.rel = "stylesheet";
  stylesheet.href = "/static/css/language-picker.css";
  document.head.append(stylesheet);
  publishedPickers.forEach(upgradePublishedPicker);
}

document.querySelectorAll(".lang-picker").forEach((picker) => {
  const trigger = picker.querySelector("summary");
  const icon = trigger.querySelector("[aria-hidden]");
  const code = trigger.querySelector(".lang-current-code");

  const updateTrigger = () => {
    const active = picker.querySelector("[data-lang-btn].is-active");
    if (!active) return;
    icon.dataset.langIcon = active.dataset.langBtn;
    code.textContent = active.textContent.trim();
  };

  updateTrigger();
  window.addEventListener("qr:langchange", updateTrigger);
  picker.addEventListener("click", (event) => {
    if (event.target.closest("[data-lang-btn]")) {
      picker.open = false;
      trigger.focus();
    }
  });
  picker.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && picker.open) {
      picker.open = false;
      trigger.focus();
    }
  });
  document.addEventListener("click", (event) => {
    if (picker.open && !picker.contains(event.target)) {
      picker.open = false;
    }
  });
});
