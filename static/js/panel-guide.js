const guide = document.querySelector("#panel-guide");
const launch = document.querySelector("[data-guide-start]");

if (typeof guide?.showModal === "function" && launch && document.querySelector("[data-guide-key]")) {
  initializeGuide(guide, launch);
}

function initializeGuide(dialog, launchButton) {
  const message = dialog.querySelector("[data-guide-message]");
  const count = dialog.querySelector("[data-guide-count]");
  const previous = dialog.querySelector("[data-guide-back]");
  const next = dialog.querySelector("[data-guide-next]");
  const continueLabel = dialog.querySelector("[data-guide-continue]");
  const finishLabel = dialog.querySelector("[data-guide-finish]");
  const shade = dialog.querySelector("[data-guide-shade]");
  const outline = dialog.querySelector("[data-guide-outline]");
  const openedDetails = new Set();
  let steps = [];
  let index = 0;
  let scrollPosition = 0;

  function drawSpotlight() {
    if (!dialog.open) return;
    const bounds = steps[index].getBoundingClientRect();
    const width = dialog.clientWidth;
    const height = dialog.clientHeight;
    const left = Math.max(0, bounds.left - 5);
    const top = Math.max(0, bounds.top - 5);
    const right = Math.min(width, bounds.right + 5);
    const bottom = Math.min(height, bounds.bottom + 5);
    const visible = right > left && bottom > top;
    const opening = visible ? `M${left} ${top}H${right}V${bottom}H${left}Z` : "";
    shade.setAttribute("d", `M0 0H${width}V${height}H0Z ${opening}`);
    outline.setAttribute("x", String(left));
    outline.setAttribute("y", String(top));
    outline.setAttribute("width", String(visible ? right - left : 0));
    outline.setAttribute("height", String(visible ? bottom - top : 0));
  }

  function showStep(position) {
    index = position;
    const target = steps[index];
    for (let details = target.closest("details"); details; details = details.parentElement.closest("details")) {
      if (!details.open) {
        details.open = true;
        openedDetails.add(details);
      }
    }
    target.scrollIntoView({ block: "center", inline: "nearest", behavior: "instant" });
    message.textContent = window.QRI18n?.t(target.dataset.guideKey) || target.dataset.guideCopy;
    count.textContent = `${index + 1} / ${steps.length}`;
    previous.disabled = index === 0;
    const isLast = index === steps.length - 1;
    continueLabel.hidden = isLast;
    finishLabel.hidden = !isLast;
    requestAnimationFrame(drawSpotlight);
  }

  launchButton.hidden = false;
  launchButton.addEventListener("click", () => {
    steps = [...document.querySelectorAll("[data-guide-key]")];
    if (steps.length === 0) return;
    scrollPosition = window.scrollY;
    dialog.showModal();
    showStep(0);
  });
  previous.addEventListener("click", () => showStep(index - 1));
  next.addEventListener("click", () => {
    if (index === steps.length - 1) dialog.close();
    else showStep(index + 1);
  });
  dialog.querySelector("[data-guide-close]").addEventListener("click", () => dialog.close());
  dialog.addEventListener("close", () => {
    openedDetails.forEach((details) => { details.open = false; });
    openedDetails.clear();
    window.scrollTo({ top: scrollPosition, behavior: "instant" });
    launchButton.focus({ preventScroll: true });
  });
  window.addEventListener("resize", drawSpotlight);
  document.addEventListener("scroll", drawSpotlight, { passive: true, capture: true });
}
