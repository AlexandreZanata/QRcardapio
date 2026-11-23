(() => {
  "use strict";

  // 1. OCR Upload Dropzone & Submit Feedback
  const uploadForm = document.querySelector("[data-ocr-upload-form]");
  if (uploadForm) {
    const dropzone = uploadForm.querySelector("[data-ocr-dropzone]");
    const fileInput = uploadForm.querySelector("[data-ocr-file-input]");
    const fileLabel = uploadForm.querySelector("[data-ocr-filename]");
    const submitBtn = uploadForm.querySelector("[data-ocr-submit]");

    const showFilename = (files) => {
      if (!fileLabel || !files || files.length === 0) return;
      fileLabel.removeAttribute("data-i18n");
      fileLabel.textContent = files[0].name;
    };

    if (fileInput) {
      fileInput.addEventListener("change", () => showFilename(fileInput.files));
    }

    if (dropzone && fileInput) {
      ["dragenter", "dragover"].forEach((evt) => {
        dropzone.addEventListener(evt, (ev) => {
          ev.preventDefault();
          dropzone.classList.add("is-dragover");
        });
      });

      ["dragleave", "drop"].forEach((evt) => {
        dropzone.addEventListener(evt, (ev) => {
          ev.preventDefault();
          dropzone.classList.remove("is-dragover");
        });
      });

      dropzone.addEventListener("drop", (ev) => {
        const dt = ev.dataTransfer;
        if (!dt || !dt.files || dt.files.length === 0) return;
        try {
          fileInput.files = dt.files;
        } catch {
          // Fallback for browsers with read-only FileList
        }
        showFilename(dt.files);
      });
    }

    uploadForm.addEventListener("submit", () => {
      if (!submitBtn) return;
      submitBtn.disabled = true;
      const processing =
        (window.QRI18n && window.QRI18n.t("ocr.processing")) ||
        "Lendo cardápio... aguarde alguns segundos.";
      const span = submitBtn.querySelector("span") || submitBtn;
      span.setAttribute("data-i18n", "ocr.processing");
      span.textContent = processing;
    });
  }

  // 2. OCR Review Interactive Table (Select All, Live Count, Add/Remove Row)
  const reviewBox = document.querySelector("[data-ocr-review]");
  if (reviewBox) {
    const tbody = reviewBox.querySelector("[data-ocr-rows]");
    const selectAll = reviewBox.querySelector("[data-ocr-select-all]");
    const addBtn = reviewBox.querySelector("[data-ocr-add-row]");
    const tpl = document.getElementById("ocr-row-template");
    const countEl = document.querySelector("[data-ocr-selected-count]");
    let nextIdx = Number(reviewBox.dataset.nextIdx || "0");
    if (!Number.isFinite(nextIdx) || nextIdx < 0) nextIdx = 0;

    const refreshSelectionState = () => {
      if (!tbody) return;
      const boxes = Array.from(tbody.querySelectorAll("[data-ocr-row-include]"));
      const checked = boxes.filter((b) => b.checked).length;
      if (countEl) {
        countEl.textContent = String(checked);
      }
      if (selectAll) {
        selectAll.checked = boxes.length > 0 && checked === boxes.length;
        selectAll.indeterminate = checked > 0 && checked < boxes.length;
      }
    };

    if (selectAll && tbody) {
      selectAll.addEventListener("change", () => {
        tbody.querySelectorAll("[data-ocr-row-include]").forEach((cb) => {
          cb.checked = selectAll.checked;
        });
        refreshSelectionState();
      });
    }

    if (tbody) {
      tbody.addEventListener("change", (ev) => {
        if (ev.target.matches("[data-ocr-row-include]")) {
          refreshSelectionState();
        }
      });

      tbody.addEventListener("input", (ev) => {
        if (ev.target.classList.contains("ocr-input-missing") && ev.target.value.trim() !== "") {
          ev.target.classList.remove("ocr-input-missing");
        }
      });

      tbody.addEventListener("click", (ev) => {
        const removeBtn = ev.target.closest("[data-ocr-remove-row]");
        if (!removeBtn) return;
        const row = removeBtn.closest("[data-ocr-row]");
        if (row) {
          row.remove();
          refreshSelectionState();
        }
      });
    }

    if (addBtn && tbody && tpl) {
      addBtn.addEventListener("click", () => {
        const idx = nextIdx++;
        const frag = tpl.content.cloneNode(true);
        frag.querySelectorAll("[name]").forEach((el) => {
          el.setAttribute("name", el.getAttribute("name").replace(/__IDX__/g, String(idx)));
        });
        frag.querySelectorAll("[value]").forEach((el) => {
          el.setAttribute("value", el.getAttribute("value").replace(/__IDX__/g, String(idx)));
        });
        if (window.QRI18n && typeof window.QRI18n.t === "function") {
          frag.querySelectorAll("[data-i18n]").forEach((el) => {
            const translated = window.QRI18n.t(el.dataset.i18n);
            if (translated) el.textContent = translated;
          });
          const catInput = frag.querySelector(`input[name="category_${idx}"]`);
          const defCat = window.QRI18n.t("ocr.default_category");
          if (catInput && defCat) catInput.value = defCat;
        }
        const emptyHint = reviewBox.querySelector("[data-ocr-empty-hint]");
        if (emptyHint) emptyHint.remove();
        tbody.appendChild(frag);
        refreshSelectionState();
        const newName = tbody.querySelector(`input[name="name_${idx}"]`);
        if (newName) newName.focus();
      });
    }

    refreshSelectionState();
  }
})();
