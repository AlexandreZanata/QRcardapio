const openers = new WeakMap();

function openDialog(dialog, trigger) {
  const category = dialog.querySelector('select[name="category_id"]');
  if (category && trigger.dataset.panelDialogCategory) category.value = trigger.dataset.panelDialogCategory;
  openers.set(dialog, trigger);
  dialog.showModal();
  const focus = dialog.querySelector("[data-panel-dialog-focus]") || dialog.querySelector('input[type="text"]');
  focus?.focus();
}

function returnUrl(form) {
  const destination = new URL(form.dataset.returnUrl, window.location.href);
  const lang = new URLSearchParams(window.location.search).get("lang");
  if (lang) destination.searchParams.set("lang", document.documentElement.lang);
  return destination;
}

for (const trigger of document.querySelectorAll("[data-panel-dialog-open]")) {
  const dialog = document.getElementById(trigger.dataset.panelDialogOpen);
  if (typeof dialog?.showModal !== "function") continue;

  trigger.setAttribute("aria-haspopup", "dialog");
  trigger.addEventListener("click", (event) => {
    if (event.metaKey || event.ctrlKey || event.altKey || event.shiftKey) return;

    event.preventDefault();
    if (dialog.open) return;

    openDialog(dialog, trigger);
  });
}

for (const dialog of document.querySelectorAll("dialog.panel-dialog")) {
  for (const button of dialog.querySelectorAll("[data-panel-dialog-close]")) {
    button.addEventListener("click", () => dialog.close());
  }
  dialog.addEventListener("close", () => openers.get(dialog)?.focus({ preventScroll: true }));

  const form = dialog.querySelector("form[data-panel-dialog-form]");
  if (form) enableDialogForm(dialog, form);

  if (form && dialog.hasAttribute("data-panel-dialog-auto-open") && typeof dialog.showModal === "function") {
    const trigger = document.querySelector(`[data-panel-dialog-open="${dialog.id}"]`);
    if (trigger) {
      dialog.addEventListener("close", () => history.replaceState(history.state, "", returnUrl(form)), { once: true });
      openDialog(dialog, trigger);
    }
  }
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
    for (const field of form.querySelectorAll('input[type="file"]')) field.dispatchEvent(new Event("change"));
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
    const values = new FormData(form);
    const body = form.enctype === "multipart/form-data" ? values : new URLSearchParams(values);

    try {
      const response = await fetch(endpoint, {
        method: form.method,
        headers: { Accept: "application/json" },
        body,
      });

      if (response.redirected) {
        window.location.assign(response.url);
        return;
      }
      if (response.status === 204) {
        window.location.assign(returnUrl(form));
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
