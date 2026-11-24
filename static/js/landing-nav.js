// The fixed QR on the landing encodes this fragment. Browsers keep it out of HTTP requests.
const demoLink = document.querySelector("[data-public-demo]");
if (demoLink) {
  const openDemoMenu = () => {
    if (window.location.hash === "#demonstracao") {
      window.location.replace(demoLink.href);
    }
  };
  window.addEventListener("hashchange", openDemoMenu);
  openDemoMenu();
}

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
