const menu = document.querySelector(".landing-topbar-menu");

if (menu) {
  const trigger = menu.querySelector("summary");

  menu.addEventListener("click", (event) => {
    const link = event.target.closest('a[href^="#"]');
    if (!link) return;
    menu.open = false;
    trigger.focus({ preventScroll: true });
  });

  document.addEventListener("click", (event) => {
    if (menu.open && !menu.contains(event.target)) menu.open = false;
  });

  menu.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || !menu.open) return;
    menu.open = false;
    trigger.focus();
  });
}
