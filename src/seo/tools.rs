use crate::i18n::Locale;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolId {
    QrGenerator,
    CmvCalculator,
    TablePlacard,
}

impl ToolId {
    pub const ALL: [ToolId; 3] = [Self::QrGenerator, Self::CmvCalculator, Self::TablePlacard];

    pub const fn art(self) -> &'static str {
        match self {
            Self::QrGenerator => "qr-generator",
            Self::CmvCalculator => "cmv-calculator",
            Self::TablePlacard => "table-placard",
        }
    }

    pub const fn slug(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::QrGenerator, Locale::PtBr) => "gerador-qr-code-cardapio",
            (Self::QrGenerator, Locale::En) => "menu-qr-code-generator",
            (Self::QrGenerator, Locale::Es) => "generador-qr-menu",
            (Self::CmvCalculator, Locale::PtBr) => "calculadora-cmv-cardapio",
            (Self::CmvCalculator, Locale::En) => "menu-food-cost-calculator",
            (Self::CmvCalculator, Locale::Es) => "calculadora-costo-menu",
            (Self::TablePlacard, Locale::PtBr) => "placa-qr-code-mesa",
            (Self::TablePlacard, Locale::En) => "table-qr-tent-generator",
            (Self::TablePlacard, Locale::Es) => "cartel-qr-mesa",
        }
    }

    pub const fn full_path(self, locale: Locale) -> &'static str {
        match (self, locale) {
            (Self::QrGenerator, Locale::PtBr) => "/ferramentas/gerador-qr-code-cardapio",
            (Self::QrGenerator, Locale::En) => "/en/tools/menu-qr-code-generator",
            (Self::QrGenerator, Locale::Es) => "/es/herramientas/generador-qr-menu",
            (Self::CmvCalculator, Locale::PtBr) => "/ferramentas/calculadora-cmv-cardapio",
            (Self::CmvCalculator, Locale::En) => "/en/tools/menu-food-cost-calculator",
            (Self::CmvCalculator, Locale::Es) => "/es/herramientas/calculadora-costo-menu",
            (Self::TablePlacard, Locale::PtBr) => "/ferramentas/placa-qr-code-mesa",
            (Self::TablePlacard, Locale::En) => "/en/tools/table-qr-tent-generator",
            (Self::TablePlacard, Locale::Es) => "/es/herramientas/cartel-qr-mesa",
        }
    }

    pub fn from_slug(locale: Locale, slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.slug(locale) == slug)
    }

    pub fn content(self, locale: Locale) -> &'static ToolContent {
        match (self, locale) {
            (Self::QrGenerator, Locale::PtBr) => &QR_GEN_PT,
            (Self::QrGenerator, Locale::En) => &QR_GEN_EN,
            (Self::QrGenerator, Locale::Es) => &QR_GEN_ES,
            (Self::CmvCalculator, Locale::PtBr) => &CMV_PT,
            (Self::CmvCalculator, Locale::En) => &CMV_EN,
            (Self::CmvCalculator, Locale::Es) => &CMV_ES,
            (Self::TablePlacard, Locale::PtBr) => &PLACARD_PT,
            (Self::TablePlacard, Locale::En) => &PLACARD_EN,
            (Self::TablePlacard, Locale::Es) => &PLACARD_ES,
        }
    }

    pub fn is_qr_generator(self) -> bool {
        self == Self::QrGenerator
    }

    pub fn is_cmv_calculator(self) -> bool {
        self == Self::CmvCalculator
    }

    pub fn is_table_placard(self) -> bool {
        self == Self::TablePlacard
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ToolContent {
    pub id: ToolId,
    pub name: &'static str,
    pub short_desc: &'static str,
    pub meta_title: &'static str,
    pub meta_description: &'static str,
    pub h1: &'static str,
    pub answer_summary: &'static str,
    pub steps_heading: &'static str,
    pub how_to_steps: [&'static str; 3],
    pub bridge_title: &'static str,
    pub bridge_body: &'static str,
    pub faqs: [(&'static str, &'static str); 3],
}

pub fn all_tools(locale: Locale) -> Vec<&'static ToolContent> {
    ToolId::ALL
        .into_iter()
        .map(|id| id.content(locale))
        .collect()
}

const QR_GEN_PT: ToolContent = ToolContent {
    id: ToolId::QrGenerator,
    name: "Gerador de QR Code para Cardápio",
    short_desc: "Gere QR Code em SVG vetorial e PNG 1024px pronto para gráfica sem cadastro.",
    meta_title: "Gerador de QR Code para Cardápio Grátis (SVG e PNG) | QR Cardápio",
    meta_description: "Gere grátis o QR Code do seu cardápio ou site em SVG vetorial e PNG de alta resolução (1024px) com alto contraste e sem marca d'água.",
    h1: "Gerador gratuito de QR Code para cardápio",
    answer_summary: "Gere instantaneamente um QR Code estático de alto contraste para qualquer endereço HTTPS do seu restaurante, cardápio em PDF ou página do Instagram. Baixe sem cadastro em formato vetorial SVG (para gráficas e banners) ou PNG de 1024 px com margem de leitura segura.",
    steps_heading: "Como gerar seu QR Code",
    how_to_steps: [
        "Cole o endereço HTTPS do seu cardápio digital ou link de avaliação do Google Maps.",
        "Confira o código gerado ao lado e teste apontando a câmera do seu celular.",
        "Baixe em SVG vetorial para impressão gráfica ou PNG em alta resolução.",
    ],
    bridge_title: "Quer um cardápio bonito que nunca muda de QR Code?",
    bridge_body: "Com o QR Cardápio você não precisa depender de PDF lento: crie seu cardápio interativo com fotos e preços atualizáveis atrás de um link permanente.",
    faqs: [
        (
            "O QR Code gerado aqui expira?",
            "Não. É um QR Code direto contendo o endereço informado por você, sem redirecionadores nem limite de leituras.",
        ),
        (
            "Qual formato escolher: SVG ou PNG?",
            "Use SVG ao enviar para gráfica ou diagramação (escala infinita sem serrilhado) e PNG 1024 px para redes sociais e impressoras comuns.",
        ),
        (
            "Qual o tamanho mínimo recomendado na mesa?",
            "Imprima o símbolo com pelo menos 3×3 cm e preserve a margem branca externa para leitura rápida sob qualquer luz.",
        ),
    ],
};

const QR_GEN_EN: ToolContent = ToolContent {
    id: ToolId::QrGenerator,
    name: "Menu QR Code Generator",
    short_desc: "Create crisp vector SVG and 1024px PNG QR codes free with no watermarks.",
    meta_title: "Free Menu QR Code Generator (SVG & PNG) | QR Cardápio",
    meta_description: "Generate a free high-contrast QR code for your restaurant menu in scalable SVG vector or 1024px PNG format with zero watermarks or expiration.",
    h1: "Free menu QR code generator",
    answer_summary: "Create a crisp, high-contrast static QR code for any restaurant menu URL or Google Maps review link without signing up. Download print-ready vector SVG files for designers and 1024 px PNG files with built-in quiet-zone borders that scan smoothly on every smartphone.",
    steps_heading: "How to generate your QR code",
    how_to_steps: [
        "Paste the HTTPS link to your restaurant menu or Google Maps review form.",
        "Preview the live QR code and verify it with your smartphone camera.",
        "Download scalable SVG for print shops or 1024 px PNG for instant printing.",
    ],
    bridge_title: "Want an interactive menu behind a permanent QR code?",
    bridge_body: "Skip hard-to-read PDFs: build a fast, mobile-first digital menu with photos, sold-out toggles, and three languages using QR Cardápio.",
    faqs: [
        (
            "Does this QR code ever expire?",
            "No. It encodes your URL directly with zero tracking redirects or scan quotas.",
        ),
        (
            "Should I download SVG or PNG?",
            "Send SVG to print shops for crisp scaling at any size; use 1024 px PNG for office printers and social graphics.",
        ),
        (
            "How big should I print a table QR code?",
            "Print at least 3×3 cm (1.2×1.2 in) and never crop the white margin around the squares.",
        ),
    ],
};

const QR_GEN_ES: ToolContent = ToolContent {
    id: ToolId::QrGenerator,
    name: "Generador de Código QR para Menú",
    short_desc: "Descarga QR en SVG vectorial y PNG 1024px sin marca de agua ni registro.",
    meta_title: "Generador Gratis de Código QR para Menú (SVG y PNG) | QR Cardápio",
    meta_description: "Crea gratis el código QR de tu menú en formato SVG vectorial y PNG de 1024 px de alta resolución, sin marcas de agua ni vencimiento.",
    h1: "Generador gratuito de código QR para menú",
    answer_summary: "Genera al instante un código QR estático de alto contraste para la URL de tu menú o enlace de reseñas de Google Maps sin necesidad de registrarte. Descárgalo en SVG vectorial para imprenta o en PNG de 1024 px con borde blanco seguro.",
    steps_heading: "Cómo generar tu código QR",
    how_to_steps: [
        "Pega la dirección HTTPS de tu menú digital o ficha de Google Maps.",
        "Comprueba el código generado apuntando con la cámara de tu teléfono.",
        "Descarga SVG vectorial para imprenta o PNG de alta resolución.",
    ],
    bridge_title: "¿Prefieres un menú ágil detrás de un código QR permanente?",
    bridge_body: "Olvida los PDF pesados: publica tu menú digital con fotos, precios actualizables y tres idiomas en minutos con QR Cardápio.",
    faqs: [
        (
            "¿El código QR generado caduca?",
            "No. Codifica tu dirección directamente sin redirecciones intermedias ni límite de escaneos.",
        ),
        (
            "¿Conviene SVG o PNG?",
            "Elige SVG para imprenta o cartelería grande; usa PNG de 1024 px para impresión rápida y redes sociales.",
        ),
        (
            "¿Qué tamaño mínimo se recomienda en mesas?",
            "Imprime al menos 3×3 cm manteniendo intacto el margen blanco exterior.",
        ),
    ],
};

const CMV_PT: ToolContent = ToolContent {
    id: ToolId::CmvCalculator,
    name: "Calculadora de CMV e Preço de Cardápio",
    short_desc: "Calcule custo de mercadoria vendida, taxas e preço ideal de venda em reais.",
    meta_title: "Calculadora de CMV e Preço de Cardápio Grátis | QR Cardápio",
    meta_description: "Calcule grátis o CMV (Custo de Mercadoria Vendida), taxas de cartão e o preço de venda ideal de cada prato ou lanche do seu restaurante.",
    h1: "Calculadora gratuita de CMV e preço de cardápio",
    answer_summary: "A calculadora de CMV para restaurantes determina rapidamente o preço de venda ideal de cada prato somando custo de ingredientes, embalagem, taxas de maquininha e o percentual de CMV desejado (geralmente entre 25% e 35%). Todo o cálculo usa valores exatos em centavos direto no navegador.",
    steps_heading: "Como precificar seus pratos",
    how_to_steps: [
        "Informe o custo de ficha técnica dos insumos e da embalagem (se houver).",
        "Escolha a meta de CMV saudável (ex.: 30% para restaurantes, 25% para drinks/cafés).",
        "Adicione taxas de pagamento para descobrir o preço sugerido e a margem por unidade.",
    ],
    bridge_title: "Atualizou sua ficha técnica? Atualize o cardápio em segundos.",
    bridge_body: "Com o QR Cardápio você ajusta preços com precisão em centavos e publica na hora sem gastar com reimpressão nas mesas.",
    faqs: [
        (
            "Qual o CMV ideal para restaurantes e hamburguerias?",
            "Na maioria dos bares e restaurantes saudáveis, o CMV fica entre 28% e 34%. Cafeterias e bebidas costumam operar entre 18% e 26%.",
        ),
        (
            "Qual a fórmula do CMV sobre o faturamento?",
            "CMV (%) = (Custo dos Insumos / Preço de Venda) × 100. Para achar o preço a partir da meta, divide-se o custo total pelo percentual desejado descontando taxas.",
        ),
        (
            "Devo incluir embalagem no custo unitário?",
            "Para itens de balcão ou viagem sim, pois descartáveis reduzem diretamente a margem líquida de contribuição.",
        ),
    ],
};

const CMV_EN: ToolContent = ToolContent {
    id: ToolId::CmvCalculator,
    name: "Menu Food Cost & Pricing Calculator",
    short_desc: "Compute recipe food cost %, payment fees, and target menu selling price.",
    meta_title: "Free Restaurant Food Cost & Menu Pricing Calculator | QR Cardápio",
    meta_description: "Calculate food cost percentage, packaging costs, card fees, and ideal selling price per dish for your restaurant or café menu.",
    h1: "Free menu food cost and pricing calculator",
    answer_summary: "This restaurant food cost calculator finds your ideal menu selling price from recipe ingredient cost, packaging, payment processing fees, and your target food-cost percentage (typically 25%–35%). Every calculation uses exact cent-level math right in your browser.",
    steps_heading: "How to price a menu item",
    how_to_steps: [
        "Enter ingredient cost per portion plus any takeaway packaging cost.",
        "Pick your target food cost percentage (30% is common for main dishes; 22% for drinks).",
        "Include card or fee percentages to see suggested price and contribution margin.",
    ],
    bridge_title: "Ingredient prices shifted? Update your digital menu in seconds.",
    bridge_body: "Adjust menu prices in your QR Cardápio dashboard anytime with zero reprinting costs across your tables.",
    faqs: [
        (
            "What is a healthy restaurant food cost percentage?",
            "Most profitable full-service restaurants aim for 28%–34% food cost, while coffee bars and beverages often run between 18% and 26%.",
        ),
        (
            "How is menu food cost percentage calculated?",
            "Food Cost % = (Ingredient Cost / Menu Selling Price) × 100.",
        ),
        (
            "Should packaging be included in unit cost?",
            "Yes—if an item regularly uses boxes, cups, or bags, including packaging protects your net contribution margin.",
        ),
    ],
};

const CMV_ES: ToolContent = ToolContent {
    id: ToolId::CmvCalculator,
    name: "Calculadora de Costo y Precio de Menú",
    short_desc: "Calcula el costo de insumos, comisiones y precio sugerido de cada plato.",
    meta_title: "Calculadora Gratis de Costo y Precio de Menú | QR Cardápio",
    meta_description: "Calcula gratis el porcentaje de costo de ingredientes (CMV), comisiones y el precio ideal de venta de cada plato de tu restaurante.",
    h1: "Calculadora gratuita de costo y precio de menú",
    answer_summary: "La calculadora de costo de menú determina el precio ideal de venta de cada plato considerando ingredientes, empaque, comisiones de pago y el porcentaje objetivo de costo (habitualmente entre 25% y 35%). Todo el cálculo se realiza de forma exacta directamente en tu navegador.",
    steps_heading: "Cómo calcular el precio de tus platos",
    how_to_steps: [
        "Ingresa el costo de receta por porción y el empaque si aplica.",
        "Elige tu objetivo de costo sobre ventas (ej. 30% en platos, 25% en bebidas).",
        "Suma comisiones de tarjeta para conocer precio sugerido y margen unitario.",
    ],
    bridge_title: "¿Subieron tus insumos? Actualiza tu menú en segundos.",
    bridge_body: "Con QR Cardápio ajustas precios sin errores de redondeo y los publicas al instante manteniendo el mismo QR.",
    faqs: [
        (
            "¿Cuál es un porcentaje de costo saludable en gastronomía?",
            "La mayoría de los restaurantes rentables trabajan entre 28% y 34% de costo sobre ventas, y cafeterías o barras entre 18% y 26%.",
        ),
        (
            "¿Cómo se calcula el costo sobre el precio de venta?",
            "Costo (%) = (Costo de Ingredientes / Precio de Venta) × 100.",
        ),
        (
            "¿Conviene sumar envases al costo unitario?",
            "Sí, sobre todo en porciones para llevar o mostrador donde el envase reduce el margen real.",
        ),
    ],
};

const PLACARD_PT: ToolContent = ToolContent {
    id: ToolId::TablePlacard,
    name: "Criador de Placa QR Code para Mesa",
    short_desc: "Monte e imprima placa A5/mesa personalizada com nome da loja, chamada e QR Code.",
    meta_title: "Criador Grátis de Placa QR Code para Mesa | QR Cardápio",
    meta_description: "Crie e imprima grátis sua placa de mesa com QR Code para cardápio ou avaliação no Google Maps, com o nome do seu negócio e chamada pronta.",
    h1: "Criador gratuito de placa QR Code para mesa",
    answer_summary: "Monte uma placa limpa e legível com o nome do seu estabelecimento, uma frase de chamada (como 'Aponte a câmera para ver o cardápio') e o QR Code em alta definição. Clique em imprimir para gerar folha A4/A5 ou PDF direto pelo navegador.",
    steps_heading: "Como imprimir sua placa",
    how_to_steps: [
        "Digite o nome do seu estabelecimento e a frase de chamada para a mesa.",
        "Informe o endereço HTTPS do cardápio digital ou do link de avaliação do Google.",
        "Clique em Imprimir placa para abrir a visualização limpa de impressão em papel.",
    ],
    bridge_title: "Já tem conta no QR Cardápio? Sua folha de impressão já sai pronta.",
    bridge_body: "No painel do proprietário você baixa SVG vetorial, PNG 1024 px e abre a folha de impressão oficial com sua identidade visual.",
    faqs: [
        (
            "Qual papel usar para imprimir placas de mesa?",
            "Recomendamos papel couchê ou sulfite firme de 180g a 240g e plastificação fosca para evitar reflexo de luminárias.",
        ),
        (
            "Posso criar uma placa exclusiva para pedir avaliações no Google Maps?",
            "Sim! Basta colar o link direto de avaliação do Perfil da Empresa e mudar a chamada para 'Avalie sua visita no Google'.",
        ),
        (
            "Preciso instalar algum programa para salvar em PDF?",
            "Não. Ao clicar em Imprimir placa, escolha 'Salvar como PDF' na janela padrão do navegador.",
        ),
    ],
};

const PLACARD_EN: ToolContent = ToolContent {
    id: ToolId::TablePlacard,
    name: "Printable Table QR Tent Generator",
    short_desc: "Customize and print a clean tabletop QR placard for your menu or Google reviews.",
    meta_title: "Free Printable Table QR Code Placard Generator | QR Cardápio",
    meta_description: "Design and print a crisp tabletop QR code placard with your business name and scan call-to-action directly from your browser—100% free.",
    h1: "Free printable table QR placard generator",
    answer_summary: "Create a crisp, scan-ready tabletop placard showing your business name, a clear call-to-action, and a high-contrast QR code. Print straight onto cardstock or save as PDF right from your browser with clean print-only margins.",
    steps_heading: "How to print your table placard",
    how_to_steps: [
        "Enter your business name and short table call-to-action.",
        "Paste your menu URL or Google Maps review link.",
        "Click Print placard to print directly or save as a crisp PDF.",
    ],
    bridge_title: "Want automatic brand colors and permanent QR sheets?",
    bridge_body: "Inside the QR Cardápio owner dashboard, your printable sheet, vector SVG, and 1024 px PNG stay synced with your permanent URL.",
    faqs: [
        (
            "What paper weight works best for table cards?",
            "Use 180–240 gsm cardstock with matte lamination so overhead lights do not cause camera glare.",
        ),
        (
            "Can I use this for a Google Maps review table tent?",
            "Yes—paste your Google Business review URL and set the headline to 'Review us on Google Maps'.",
        ),
        (
            "Can I export it as a PDF?",
            "Yes. Click Print placard and choose Save as PDF in your browser print dialog.",
        ),
    ],
};

const PLACARD_ES: ToolContent = ToolContent {
    id: ToolId::TablePlacard,
    name: "Creador de Cartel QR para Mesa",
    short_desc: "Personaliza e imprime un cartel limpio con tu nombre, llamada y código QR.",
    meta_title: "Creador Gratis de Cartel QR para Mesa para Imprimir | QR Cardápio",
    meta_description: "Diseña e imprime gratis tu cartel de mesa con código QR para menú digital o reseñas en Google Maps directamente desde el navegador.",
    h1: "Creador gratuito de cartel QR para mesa",
    answer_summary: "Prepara un cartel limpio y de alto contraste con el nombre de tu local, un mensaje breve para los clientes y tu código QR nítido. Imprímelo en cartulina o guárdalo como PDF desde tu navegador con diseño adaptado para papel.",
    steps_heading: "Cómo imprimir tu cartel",
    how_to_steps: [
        "Escribe el nombre de tu negocio y la frase de llamada para las mesas.",
        "Pega el enlace HTTPS de tu menú digital o reseñas de Google Maps.",
        "Pulsa Imprimir cartel para imprimir o guardar en PDF con un clic.",
    ],
    bridge_title: "¿Prefieres una hoja oficial conectada a tu menú permanente?",
    bridge_body: "Desde tu panel de QR Cardápio descargas SVG vectorial, PNG de 1024 px y hoja de impresión lista para tus mesas.",
    faqs: [
        (
            "¿Qué papel conviene para carteles de mesa?",
            "Usa cartulina de 180 a 240 g con laminado mate para evitar reflejos de luces sobre el código.",
        ),
        (
            "¿Sirve también para pedir reseñas en Google Maps?",
            "Sí. Pega el enlace directo de reseñas de tu perfil de empresa y cambia el texto superior.",
        ),
        (
            "¿Puedo guardarlo como archivo PDF?",
            "Sí. Al pulsar Imprimir cartel, selecciona Guardar como PDF en la ventana de impresión.",
        ),
    ],
};
