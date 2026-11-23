const dialog = document.querySelector("#dashboard-file-picker");

if (typeof dialog?.showModal === "function" && typeof DataTransfer === "function") {
  const candidate = dialog.querySelector("[data-file-candidate]");
  const apply = dialog.querySelector("[data-file-apply]");
  const drop = dialog.querySelector("[data-file-drop]");
  const details = dialog.querySelector("[data-file-preview]");
  const thumb = dialog.querySelector("[data-file-preview-image]");
  const error = dialog.querySelector("[data-file-error]");
  let activeField;
  let pendingFile;
  let reader;

  function message(key) {
    error.textContent = key ? dialog.querySelector(`[data-file-message="${key}"]`).textContent : "";
    error.hidden = !key;
  }

  function showPreview(file, isImage) {
    if (reader?.readyState === FileReader.LOADING) reader.abort();
    thumb.hidden = true;
    thumb.removeAttribute("src");
    details.hidden = !file;
    apply.disabled = !file;
    if (!file) return;

    details.querySelector("[data-file-preview-name]").textContent = file.name;
    const megabytes = file.size >= 1024 * 1024;
    const size = new Intl.NumberFormat(document.documentElement.lang, { maximumFractionDigits: 1 })
      .format(file.size / (megabytes ? 1024 * 1024 : 1024));
    details.querySelector("[data-file-preview-size]").textContent = `${size} ${megabytes ? "MB" : "KB"}`;

    if (isImage) {
      const preview = new FileReader();
      reader = preview;
      preview.addEventListener("load", () => {
        if (reader !== preview) return;
        thumb.src = String(preview.result);
        thumb.hidden = false;
      }, { once: true });
      preview.readAsDataURL(file);
    }
  }

  function selectFile(file, count = 1) {
    pendingFile = undefined;
    if (!file) {
      showPreview();
      return;
    }

    const isImage = ["image/jpeg", "image/png", "image/webp"].includes(file.type)
      || /\.(jpe?g|png|webp)$/i.test(file.name);
    const isPdf = file.type === "application/pdf" || /\.pdf$/i.test(file.name);
    const limit = isPdf ? Number(activeField.dataset.pdfLimit) : Number(activeField.dataset.imageLimit);
    let errorKey;
    if (count > 1) errorKey = "one";
    else if (!(isImage || isPdf) || limit <= 0) errorKey = "type";
    else if (file.size > limit * 1024 * 1024) errorKey = "size";
    if (errorKey) {
      showPreview();
      message(errorKey);
      return;
    }

    pendingFile = file;
    message();
    showPreview(file, isImage && !isPdf);
  }

  function setFieldLabel(field, input) {
    const label = field.querySelector("[data-file-name]");
    const clear = field.querySelector("[data-file-clear]");
    if (input.files.length) {
      delete label.dataset.i18n;
      label.textContent = input.files[0].name;
      clear.hidden = false;
    } else {
      label.dataset.i18n = "forms.no_file";
      label.textContent = window.QRI18n?.t("forms.no_file") || label.dataset.fileEmpty;
      clear.hidden = true;
    }
  }

  function openPicker(field) {
    if (dialog.open) return;
    activeField = field;
    const input = field.querySelector(".file-native");
    const hint = document.getElementById(input.getAttribute("aria-describedby"));
    candidate.accept = input.accept;
    candidate.value = "";
    dialog.querySelector("[data-file-purpose]").textContent = field.querySelector("label").textContent;
    dialog.querySelector("[data-file-hint]").textContent = hint?.textContent || "";
    message();
    selectFile(input.files[0]);
    dialog.showModal();
    dialog.querySelector("[data-file-browse]").focus();
  }

  document.querySelectorAll("[data-file-field]").forEach((field) => {
    const input = field.querySelector(".file-native");
    const trigger = field.querySelector("[data-file-open]");
    field.classList.add("file-field-enhanced");
    trigger.hidden = false;
    input.tabIndex = -1;

    trigger.addEventListener("click", () => openPicker(field));
    field.querySelector("label").addEventListener("click", (event) => {
      event.preventDefault();
      openPicker(field);
    });
    field.querySelector("[data-file-clear]").addEventListener("click", () => {
      input.value = "";
      input.dispatchEvent(new Event("change", { bubbles: true }));
      trigger.focus();
    });
    input.addEventListener("change", () => setFieldLabel(field, input));
    input.addEventListener("invalid", (event) => {
      if (!input.validity.valueMissing) return;
      event.preventDefault();
      if (!dialog.open) openPicker(field);
      message("required");
    });
  });

  dialog.querySelector("[data-file-browse]").addEventListener("click", () => {
    candidate.value = "";
    candidate.click();
  });
  candidate.addEventListener("change", () => {
    if (candidate.files.length) selectFile(candidate.files[0]);
  });

  drop.addEventListener("dragover", (event) => {
    event.preventDefault();
    drop.classList.add("file-picker-dragover");
  });
  drop.addEventListener("dragleave", () => drop.classList.remove("file-picker-dragover"));
  drop.addEventListener("drop", (event) => {
    event.preventDefault();
    drop.classList.remove("file-picker-dragover");
    const files = event.dataTransfer.files;
    if (files.length) selectFile(files[0], files.length);
  });

  apply.addEventListener("click", () => {
    if (!pendingFile) return;
    const input = activeField.querySelector(".file-native");
    const transfer = new DataTransfer();
    transfer.items.add(pendingFile);
    input.files = transfer.files;
    input.dispatchEvent(new Event("change", { bubbles: true }));
    dialog.close();
  });

  dialog.querySelectorAll("[data-file-close]").forEach((button) => {
    button.addEventListener("click", () => dialog.close());
  });
  dialog.addEventListener("close", () => {
    showPreview();
    candidate.value = "";
    pendingFile = undefined;
    activeField?.querySelector("[data-file-open]").focus();
    activeField = undefined;
  });
}
