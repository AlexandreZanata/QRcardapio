# Design System & Native i18n Frontend

## Visual Tokens (`static/css/tokens.css`)

- **Palette**: Warm off-white background (`#FAF7F2`), pure card surface (`#FFFFFF`), warm sand border (`#E7E0D5`), espresso charcoal ink (`#1C1917`), warm stone secondary ink (`#57534E`), default terracotta coral accent (`#D9532F`).
- **Brand Theming**: Published menus and owner previews load a generated external stylesheet (`theme-{hash8}.css` or `/dashboard/theme.css`) setting `--brand-accent`, `--brand-accent-hover`, `--brand-accent-ink` (computed via sRGB luminance for WCAG AA contrast), and `--brand-soft`.
- **Typography**: System UI sans-serif stack for instant first paint, zero external font requests, and crisp legibility on budget Android smartphones.
- **Touch Targets & Accessibility**: Interactive controls enforce `min-height: 44px`, keyboard `:focus-visible` outlines, skip links, semantic landmarks (`<header>`, `<main>`, `<nav>`, `<footer>`), and `aria-live="polite"` alerts.

## Landing Page and Dashboard

The landing page uses its own cream, blue, yellow, and pink palette. The device illustration uses real browser captures of the owner dashboard and public mobile menu for a fictional establishment. The enlarged WebP files are captured natively at 3840 × 2160 and 2160 × 3840 pixels. The landing selects smaller WebP derivatives through `srcset` and uses translated captions and alternative text. Keep personal information out of captures. The three original food photos under `static/images/` can be used to recreate the fictional menu.

Dashboard pages share a dark sidebar, a workspace header with a language switcher, and an active link marked with `aria-current="page"`. On narrow screens, `static/js/app.js` enables a collapsible sidebar with keyboard dismissal. The navigation remains visible without JavaScript. The operations page adds shortcuts to its establishment and publication tables.

The dashboard uses the original `static/icons/dashboard.svg` illustrations for navigation, metrics, and actions. The Guide button opens a keyboard-accessible dialog and highlights controls annotated by `components/guide_hint.html`. It scrolls each control into view, reveals collapsed fields for its explanation, and restores the previous view on close. The component reads translated instructions from the page catalog.

### Dashboard forms

Use the Askama macros in `templates/components/form_fields.html` for labeled text and URL fields, textareas, lists, checkboxes, and file inputs. Settings and onboarding also share the permanent address and brand color controls. Use the compact control variants for editable table rows. Pass catalog keys for labels, placeholders, and help text, and attach tutorial hints through `guide_key`.

The field stylesheet defines control heights, focus states, hint placement, and responsive field pairs. Textareas with an explicit limit show a live count. Preset swatches update the native color input and its displayed hexadecimal value. The permanent address remains selectable when locked.

For screens with a file field, include `components/file_picker.html` once in the dashboard layout's `file_picker` block. Set the accepted formats and limits on the field to match its server endpoint. The dialog stages one file and previews images locally using a data URL allowed by CSP. Applying the selection populates the form's native file input for its usual multipart submission. The native input remains available when JavaScript or dialog support is unavailable. The modal uses `static/images/file-picker-chef.webp`, an original illustration of a chef holding a yellow frame with a pink burger.

Dashboard action links, submit buttons, icon tiles, section headings, and illustrated tips share `components/panel_controls.html`. The primary variant uses the landing's blue from `tokens.css`. Menu editing and the overview reuse the CSRF-protected form in `components/panel_publication.html`. The overview groups preview and live links below the publish action. Menu import uses the featured file field alongside a native disclosure for pasted text and a list of reading tips.

### Dashboard dialogs

`components/panel_dialog.html` provides a common heading, close control, and form actions. `panel-dialog.js` opens native dialogs, sets initial focus, and restores focus to the opener. Category creation uses the same fields as the standalone form, reports validation inside the dialog, and returns to the current editor page after saving. The editor link opens the standalone form when dialog support is unavailable.

Product creation uses the same dialog controls. `components/product_fields.html` shares the fields with the standalone form and product edit page. The category action preselects its category, and the standard file picker stages the photo within the product dialog. The dialog keeps save and cancel in the footer while the fields scroll. Validation keeps the user's inputs and photo in the form.

`components/panel_confirmation.html` uses the same dialog controls for actions that need explicit confirmation. The unpublish confirmation focuses Cancel by default and explains how republishing restores the same link and QR code. Its trigger exposes a confirmation disclosure when dialog support is unavailable.

The unpublish artwork is saved in `static/images/unpublish-chef.webp`. It was generated with the built-in ImageGen tool and resized to a transparent 384 × 288 WebP.

> Use case: stylized-concept. Original transparent sticker illustration for the QR Cardápio dashboard confirmation to temporarily unpublish a digital restaurant menu. A friendly young chef in a white toque and deep cobalt-blue apron calmly flips a small yellow storefront sign that shows a bold pause symbol (two bars, no text). Include a small intact black and cream QR menu placard nearby, clearly still usable after reopening. Single compact composition, centered, clearly legible at 180px. Match the brand's cream #fff9ee, cobalt #263be8, yellow #ffdc38, strawberry pink #ff5596 and bold dark navy #161721 outlines. Clean playful editorial sticker style, flat fills and subtle hand-drawn character warmth, similar to a contemporary Brazilian food SaaS landing page. Make the mood temporary and calm, not alarming. No lettering, words, brand symbols, drop shadows beyond the sticker, border, or background. Genuine alpha outside the sticker.

### Responsive dashboard tables

The menu editor shows a single table with category headers separating product rows. Desktop aligns the product, price, availability, and actions in columns. Narrow layouts put the product and price on one line and the availability and actions below. Icon actions use translated accessible labels and tooltips. The page groups at most 12 products or empty categories, and keeps the current page through editor actions.

Operational records, OCR history, and OCR review use the mobile table labels in `dashboard-tables.css`. Narrow screens show each row with its field labels so values and inputs can be read without horizontal scrolling. The QR sheet scales its symbol within the available screen width and keeps a separate print layout.

### Landing page assets

The Google Maps section pairs the landing's highlighted headline and buttons with two HTML mockups. The profile shows Brasa 128, a fictional hamburger restaurant with a cream façade, cobalt frames, and a yellow awning. The phone illustrates the review invitation shown in a published menu. The profile panel combines the storefront, business details, and a labeled comparison of simulated averages, totals, and 1–5 star distributions. The before card uses light gray and the after card uses the landing's cobalt blue with white text and yellow rating bars. The overlapping phone shows the invitation above a burger photograph. Narrow screens place the rating comparison below the overlap inside the profile panel. Profile views are an owner performance metric and sit outside the public profile demonstration. [Google Business Profile performance documentation](https://support.google.com/business/answer/9918094?hl=pt-BR)

The review phone uses `landing-review-frame.svg` as the artwork source. Its transparent WebP exports are 540, 1080, and 2160 pixels wide, and the browser selects the frame through `srcset`. The screen renders its translated text and SVG icons in HTML. The complete Portuguese mockup is exported to `landing-review-4k.webp` at 2160 × 3840 pixels by capturing the straight phone at 270 × 480 CSS pixels and a device scale of eight.

See the [guide to applying the landing style](GUIA_ESTILO_LANDING.md) for the visual tokens, page structure, and reusable examples for this repository and other projects.

The brand mark and icon sprites live under `static/icons/`. The landing's features and how-it-works sections share the illustrated SVG sprite, with bold outlines and fills from the landing palette. Keep decorative icons hidden from assistive technology when the adjacent text already names the action. Landing assets are served from `static/images/` and CSS stays scoped to the landing and dashboard layouts.

### Integrações de IA na landing

A seção `#integracoes-ia` explica as operações no rascunho, a revisão do proprietário e a ativação por MCP. Os cartões usam SVGs originais como identificação dos clientes. O Gemini aparece como opção de modelo dentro do Google Antigravity. O atalho para o painel depende de `MCP_ENABLED` e a área de ativação mostra o endereço do conector, o runbook e o estado da homologação.

- O cartão do ChatGPT usa o Blossom preto do [pacote oficial da OpenAI](https://cdn.openai.com/brand/openai-logos.zip), com cor fixa para o fundo claro. [Guia da marca OpenAI](https://openai.com/brand/)
- [Ícone oficial do Claude](https://platform.claude.com/docs/en/manage-claude/oauth-apps/branding-samples#download-the-logo).
- [Kit do Google Antigravity](https://antigravity.google/press) e [SVG do Gemini](https://www.gstatic.com/lamda/images/gemini_sparkle_aurora_33f86dc0c0257da337c63.svg).

O bloco de ativação organiza as etapas de configuração e autorização ao lado do endereço MCP. O botão de copiar usa o controle do painel quando a API de área de transferência está disponível. O endereço permanece selecionável. O guia abre pelo botão amarelo e a validação técnica fica em uma seção expansível.

A faixa branca abaixo das integrações usa ilustrações próprias do sprite da landing para nove tipos de comércio. Duas cópias visuais da lista formam o ciclo contínuo, e a segunda fica oculta para leitores de tela. O controle de pausa fica acima da faixa. A preferência por movimento reduzido exibe a lista completa, parada e com quebra de linha.

A ilustração foi criada com o ImageGen integrado e exportada como WebP transparente em 620 e 1240 pixels de largura, em `static/images/landing-ai-menu-620.webp` e `static/images/landing-ai-menu-1240.webp`.

> Use case: illustration-story. Asset type: original transparent hero illustration for the QR Cardápio landing section about MCP integrations. Draw a friendly Brazilian restaurant owner with a white chef hat, creamy shirt and cobalt-blue apron reviewing a digital menu draft on a tablet at a small counter. On the tablet, show a clean food photo thumbnail of a burger and abstract short rows with one pink edit accent. Beside it, show a clear printed yellow restaurant QR tabletop sign with a decorative QR-like pattern, plus a small separate cream review card with a bold blue check. The owner is actively reviewing the tablet before publishing. Medium shot, composition contained entirely in the center, generous clear silhouette, landscape 4:3, readable at 350 pixels wide. Match QR Cardápio's modern Brazilian editorial sticker illustration style: bold almost-black navy outlines #161721, warm cream #fff9ee, ultramarine #263be8, golden yellow #ffdc38, strawberry pink #ff5596. Flat filled shapes, a little screen-printed texture, subtle depth and white sticker highlights. Transparent outer background with true alpha, crisp edges. No letters, numbers, text, wordmarks, third-party logos, watermarks, or drawn interface from an existing app. Do not depict automatic publication.

## Public Menu

The public menu uses a branded cover and a centered establishment identity. The cover uses the first visible, available product photo in menu order when the snapshot provides one. An illustrated cover uses the same brand color for menus without photos. The layout supports the shared static stylesheet on previously published snapshots, including empty menus and menus with no logo. A sticky search and category strip precedes product cards. Cards use one column on phones and two on wider screens.

## Illustrated States

Menu editing, import history, and operations show an illustrated empty state when there are no records. The error page uses the same chef character and the artwork for missing pages, restricted access, or service failures. The status code remains visible and the text is translated through the existing catalog.

## Zero Inline Scripts or Styles (Strict CSP)

`Content-Security-Policy` sets `script-src 'self'; style-src 'self'`. Consequently:
- No `style="..."` attributes exist in any HTML template (palette swatches use `.swatch-*` CSS classes).
- No executable inline `<script>` tags exist anywhere (only inert `<script type="application/json" id="qr-menu-i18n">` in static menus).

## Full Native Zero-Reload Multilingual i18n (`pt-BR`, `en`, `es`)

1. **Server-Side Resolution (`src/http/middleware/locale.rs`)**: `?lang=` query > `qr_lang` cookie > `Accept-Language` quality header > `pt-BR` default. Every page renders SSR in the active locale with zero flash of untranslated text.
2. **Dynamic Pages (`static/js/i18n.js`)**:
   - The illustrated language picker shows the active language in a native disclosure. Choosing an option fetches `/static/i18n/{locale}.json?v={hash}` once and updates page text, prices, the document language, and the active icon in place. It preserves unsaved form inputs and stores the choice in the `qr_lang` cookie and local storage.
   - Without JS, links still resolve cleanly via `?lang={locale}`.
3. **Static Public Menus (`/c/{slug}`, `static/js/menu.js`)**:
   - Published HTML embeds ~1.6 KB `#qr-menu-i18n` JSON for chrome strings and `data-pt` / `data-en` / `data-es` attributes for owner-provided translations.
   - Switching `PT | EN | ES` updates the interface, formatted BRL prices, and translated dishes in place from the embedded catalog.
   - Existing snapshots adopt the same compact picker when `static/js/menu.js` enhances their previous language controls.

### Assinatura

A seção do Plano Essencial usa a mesma faixa de fundo de `.landing-how`. O cartão combina a cor creme, os contornos escuros, a coroa amarela e as formas de fundo azul e rosa da landing. O botão abre um `<dialog>` com os controles de foco e fechamento de `panel-dialog.js`. O modal mostra três cards, com artes próprias no canto feitas no sprite da landing. O mensal usa azul, o semestral usa rosa com a etiqueta de 5% e o anual usa amarelo com a estrela de 10%. Os cards ficam lado a lado em telas largas e empilhados em telas estreitas. Cada card exibe o total e o intervalo de cobrança. A periodicidade e as ações usam os catálogos PT, EN e ES.

Cada selo tem uma coluna própria no cartão e mantém o percentual centralizado sobre a forma. Os contornos ficam nas bordas da arte para preservar a leitura do número.

O modal de preços fecha pelo botão de fechar, pela tecla Escape ou por um clique no fundo escuro. O atributo `data-panel-dialog-backdrop-close` habilita o clique no fundo, e o foco volta ao botão que abriu o modal.

O painel agrupa preço, situação da assinatura e ações. O proprietário inicia o checkout pelo formulário local do painel e segue por um link para o Asaas, preservando a diretiva `form-action 'self'`.

### Link do cardápio no cadastro

O campo de endereço inicial mostra o prefixo `/c/` e explica a geração automática quando o campo está vazio. O endereço atribuído pode receber um código curto para nomes repetidos. Antes de publicar, o proprietário pode escolher um endereço personalizado. Após publicar, o painel exibe o endereço como somente leitura.

### Integrações de IA

Configurações oferece acesso a **Integrações de IA** quando o MCP está habilitado. A página reaproveita os controles de copiar endereço e os cards do painel. As telas de autorização reutilizam o layout de login e exibem o aplicativo, o estabelecimento e o acesso solicitado nos três idiomas do produto.
