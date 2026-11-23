const openers = new WeakMap();

for (const trigger of document.querySelectorAll("[data-panel-dialog-open]")) {
  const dialog = document.getElementById(trigger.dataset.panelDialogOpen);
  if (typeof dialog?.showModal !== "function") continue;

  trigger.setAttribute("aria-haspopup", "dialog");
  trigger.addEventListener("click", (event) => {
    if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return;

    event.preventDefault();
    if (dialog.open) return;

    openers.set(dialog, trigger);
    dialog.showModal();
    const focus = dialog.querySelector("[data-panel-dialog-focus]") || dialog.querySelector('input[type="text"]');
    focus?.focus();
  });
}

for (const dialog of document.querySelectorAll("dialog.panel-dialog")) {
  for (const button of dialog.querySelectorAll("[data-panel-dialog-close]")) {
    button.addEventListener("click", () => dialog.close());
  }
  dialog.addEventListener("close", () => openers.get(dialog)?.focus({ preventScroll: true }));

  const form = dialog.querySelector("form[data-panel-dialog-form]");
  if (form) enableDialogForm(dialog, form);
}

function enableDialogForm(dialog, form) {
  const feedback = form.querySelector("[data-panel-dialog-error]");
  const fallback = form.querySelector("[data-panel-dialog-error-copy]");
  const buttons = dialog.querySelectorAll("button");
  let busy = false;

  function setBusy(value) {
    busy = value;
    form.setAttribute("aria-busy", String(value));
    for (const button of buttons) button.disabled = value;
  }

  function showError(key, message) {
    feedback.dataset.i18n = key;
    feedback.textContent = window.QRI18n?.t(key) || message;
    feedback.hidden = false;
    feedback.focus();
  }

  dialog.addEventListener("cancel", (event) => {
    if (busy) event.preventDefault();
  });
  dialog.addEventListener("close", () => {
    form.reset();
    for (const details of form.querySelectorAll("details[open]")) details.open = false;
    for (const field of form.querySelectorAll("[data-character-count]")) field.dispatchEvent(new Event("input"));
    feedback.hidden = true;
  });

  form.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (busy) return;

    feedback.hidden = true;
    setBusy(true);
    const lang = document.documentElement.lang;
    const endpoint = new URL(form.action);
    endpoint.searchParams.set("lang", lang);

    try {
      const response = await fetch(endpoint, {
        method: form.method,
        headers: { Accept: "application/json" },
        body: new URLSearchParams(new FormData(form)),
      });

      if (response.redirected) {
        window.location.assign(response.url);
        return;
      }
      if (response.status === 204) {
        const destination = new URL(form.dataset.returnUrl, window.location.href);
        if (new URLSearchParams(window.location.search).has("lang")) destination.searchParams.set("lang", lang);
        window.location.assign(destination);
        return;
      }
      if (response.headers.get("Content-Type")?.startsWith("application/json")) {
        const error = await response.json();
        showError(error.key, error.message);
        setBusy(false);
        return;
      }
    } catch {
      // The same fallback covers connection errors and unexpected responses.
    }

    showError(fallback.dataset.i18n, fallback.textContent);
    setBusy(false);
  });
}
