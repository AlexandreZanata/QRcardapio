use super::{SegmentContent, SegmentId};

pub fn get(id: SegmentId) -> &'static SegmentContent {
    match id {
        SegmentId::Restaurant => &RESTAURANT,
        SegmentId::BurgerShop => &BURGER_SHOP,
        SegmentId::CoffeeShop => &COFFEE_SHOP,
        SegmentId::SnackBar => &SNACK_BAR,
        SegmentId::Pizzeria => &PIZZERIA,
        SegmentId::AcaiShop => &ACAI_SHOP,
        SegmentId::Bakery => &BAKERY,
        SegmentId::BarPub => &BAR_PUB,
    }
}

const RESTAURANT: SegmentContent = SegmentContent {
    id: SegmentId::Restaurant,
    name: "Restaurantes",
    short_desc: "Menú del día, carta de bebidas y atención en tres idiomas en las mesas.",
    meta_title: "Menú Digital con Código QR para Restaurante | QR Cardápio",
    meta_description: "Crea el menú digital de tu restaurante con código QR permanente, actualización inmediata del plato del día, fotos nítidas y cambio instantáneo entre PT, EN y ES.",
    h1: "Menú digital con código QR para restaurantes",
    answer_summary: "Un menú digital con código QR para restaurante permite que tus clientes consulten platos, ingredientes y precios directamente desde el navegador del móvil, sin descargar aplicaciones ni ampliar archivos PDF pesados. Actualizas el menú del día y precios manteniendo el mismo QR impreso.",
    pain_heading: "Agilidad diaria en el salón",
    pain_points: [
        (
            "Menú del día sin reimprimir",
            "Cambia platos ejecutivos y precios desde el panel en segundos sin sustituir los carteles de mesa.",
        ),
        (
            "Platos agotados al instante",
            "Marca cualquier preparación como no disponible con un clic para evitar errores en sala.",
        ),
        (
            "Clientes internacionales sin barreras",
            "Publica traducciones en portugués, inglés y español bajo el mismo código QR.",
        ),
    ],
    blueprint_title: "Estructura recomendada de menú para restaurante",
    blueprint_categories: [
        (
            "Entradas y Para Compartir",
            &[
                "Bruschetta de la casa — R$ 28,00",
                "Croquetas de costilla (6 u.) — R$ 36,00",
                "Ensalada con burrata — R$ 44,00",
            ],
        ),
        (
            "Platos Principales",
            &[
                "Solomillo a la parrilla — R$ 68,00",
                "Salmón grillado con vegetales — R$ 74,00",
                "Risotto de hongos frescos — R$ 58,00",
            ],
        ),
        (
            "Bebidas y Postres",
            &[
                "Jugo natural 400 ml — R$ 14,00",
                "Copa de vino de la casa — R$ 32,00",
                "Flan casero — R$ 22,00",
            ],
        ),
    ],
    tips_heading: "Buenas prácticas en mesa",
    operational_tips: [
        "Imprime el código QR con al menos 3×3 cm y margen blanco visible en cada mesa.",
        "Añade fotos reales 1:1 en tus tres platos estrella de mayor margen.",
        "Conecta tu enlace de Google Maps para invitar reseñas reales al final de la comida.",
    ],
    faqs: [
        (
            "¿Debo reimprimir los códigos QR cuando cambie los precios?",
            "No. Tu dirección /c/tu-restaurante permanece fija tras la primera publicación.",
        ),
        (
            "¿Sirve para zonas turísticas con varios idiomas?",
            "Sí. Cada categoría y plato admite traducciones opcionales al inglés y español.",
        ),
        (
            "¿Puedo importar mi carta actual en PDF o foto?",
            "Sí. El importador OCR extrae categorías, platos y precios directamente al borrador.",
        ),
    ],
    related: [
        SegmentId::Pizzeria,
        SegmentId::BarPub,
        SegmentId::BurgerShop,
    ],
};

const BURGER_SHOP: SegmentContent = SegmentContent {
    id: SegmentId::BurgerShop,
    name: "Hamburgueserías",
    short_desc: "Portada apetitosa, burgers artesanales, combos y reseñas en Google Maps.",
    meta_title: "Menú Digital con Código QR para Hamburguesería | QR Cardápio",
    meta_description: "Crea el menú digital de tu hamburguesería con fotos atractivas, combos, código QR permanente en las mesas e invitación a reseñas en Google Maps.",
    h1: "Menú digital con código QR para hamburgueserías",
    answer_summary: "Para hamburgueserías artesanales y smash burgers, un menú digital con QR muestra fotografías de portada irresistibles, organiza combos y acompañamientos en pestañas claras e invita a los clientes satisfechos a dejar más reseñas en Google Maps.",
    pain_heading: "Ventajas para tu hamburguesería",
    pain_points: [
        (
            "Fotos que abren el apetito",
            "Tu primera foto visible se convierte automáticamente en la portada de tu menú móvil.",
        ),
        (
            "Burger edición limitada sin imprenta",
            "Prepara recetas mensuales en borrador y publícalas antes del fin de semana.",
        ),
        (
            "Más visibilidad en Google Maps",
            "Invita a calificar tu local directamente desde el pie del menú digital.",
        ),
    ],
    blueprint_title: "Estructura recomendada para hamburguesería",
    blueprint_categories: [
        (
            "Burgers Artesanales",
            &[
                "Double Smash Cheddar — R$ 34,90",
                "Bacon Ahumado 180g — R$ 41,90",
                "Burger Veggie — R$ 36,00",
            ],
        ),
        (
            "Papas y Entradas",
            &[
                "Papas rústicas con paprika — R$ 22,00",
                "Aros de cebolla — R$ 26,00",
                "Tiras de pollo crujientes — R$ 28,00",
            ],
        ),
        (
            "Batidos y Bebidas",
            &[
                "Milkshake de avellana 400 ml — R$ 27,00",
                "Refresco en lata — R$ 8,50",
                "Cerveza artesanal IPA — R$ 24,00",
            ],
        ),
    ],
    tips_heading: "Consejos para hamburgueserías",
    operational_tips: [
        "Ubica bebidas y malteadas justo después de las burgers para impulsar combos.",
        "Describe brevemente gramos de carne, queso y salsa de la casa.",
        "Usa soportes lavables o acrílicos resistentes en las mesas del salón.",
    ],
    faqs: [
        (
            "¿Abre rápido incluso con mala señal?",
            "Sí. Los menús publicados se sirven como archivos estáticos ultraligeros sin consultas a base de datos.",
        ),
        (
            "¿Puedo usar los colores y el logotipo de mi marca?",
            "Sí. El sistema adapta automáticamente contraste accesible con tu color principal.",
        ),
        (
            "¿Puedo poner el enlace en Instagram además de las mesas?",
            "Sí. La misma URL permanente funciona en mesas, folletos y redes sociales.",
        ),
    ],
    related: [SegmentId::SnackBar, SegmentId::BarPub, SegmentId::Pizzeria],
};

const COFFEE_SHOP: SegmentContent = SegmentContent {
    id: SegmentId::CoffeeShop,
    name: "Cafeterías",
    short_desc: "Métodos filtrados, vitrina de pastelería al día y lectura limpia.",
    meta_title: "Menú Digital con Código QR para Cafetería | QR Cardápio",
    meta_description: "Menú digital elegante con código QR permanente para cafeterías: espresso, métodos filtrados, pastelería del día y control rápido de existencias.",
    h1: "Menú digital con código QR para cafeterías",
    answer_summary: "En cafeterías de especialidad, un menú digital con código QR organiza bebidas de espresso, métodos filtrados y pastelería con lectura limpia. Cuando se termina el último croissant de la tarde, lo marcas como agotado desde el móvil en segundos.",
    pain_heading: "Del mostrador a la mesa sin fricciones",
    pain_points: [
        (
            "Control inmediato de la vitrina",
            "Avisa qué tartas o piezas se agotaron en tiempo real sin rehacer carteles.",
        ),
        (
            "Notas de origen y tueste",
            "Detalla variedades de grano, métodos de extracción y opciones de leche vegetal.",
        ),
        (
            "Estética cálida y cuidada",
            "Elige tu color de marca para reflejar la identidad de tu cafetería.",
        ),
    ],
    blueprint_title: "Estructura recomendada para cafetería",
    blueprint_categories: [
        (
            "Cafés y Métodos",
            &[
                "Espresso doble — R$ 9,50",
                "Cappuccino cremoso 200 ml — R$ 15,00",
                "Filtrado Hario V60 — R$ 18,00",
            ],
        ),
        (
            "Salados de Vitrina",
            &[
                "Croissant de mantequilla — R$ 16,00",
                "Pan de queso artesanal — R$ 14,00",
                "Tostada de aguacate — R$ 29,00",
            ],
        ),
        (
            "Pastelería y Fríos",
            &[
                "Pastel de zanahoria — R$ 19,00",
                "Iced Latte vainilla — R$ 18,50",
                "Affogato con helado — R$ 21,00",
            ],
        ),
    ],
    tips_heading: "Consejos para cafeterías",
    operational_tips: [
        "Crea una categoría de bebidas frías de temporada en los meses cálidos.",
        "Usa stickers discretos en la esquina de mesas pequeñas.",
        "Invita a clientes frecuentes a dejar reseña en Google Maps.",
    ],
    faqs: [
        (
            "¿Ayuda con turistas que hablan otros idiomas?",
            "Sí. Añade descripciones en inglés y español directamente desde el editor.",
        ),
        (
            "¿Debo borrar un dulce cuando se agota?",
            "No. Solo marca la disponibilidad como agotada para mostrar la etiqueta temporal.",
        ),
        (
            "¿Cuánto tarda en quedar publicado?",
            "Menos de cinco minutos manualmente o importando un PDF o foto mediante OCR.",
        ),
    ],
    related: [
        SegmentId::Bakery,
        SegmentId::AcaiShop,
        SegmentId::Restaurant,
    ],
};

const SNACK_BAR: SegmentContent = SegmentContent {
    id: SegmentId::SnackBar,
    name: "Cafeterías Rápidas",
    short_desc: "Sándwiches, empanadas, jugos naturales y búsqueda veloz por nombre.",
    meta_title: "Menú Digital con Código QR para Locales de Comida Rápida | QR Cardápio",
    meta_description: "Digitaliza el menú de tu local de comida rápida con código QR permanente: buscador instantáneo, cambios desde el móvil y cero comisiones.",
    h1: "Menú digital con código QR para locales de comida rápida",
    answer_summary: "En locales de alta rotación todo pasa por la rapidez: los clientes escanean el QR en fila o mesa y filtran sándwiches, empanadas y jugos por palabra clave. Actualiza precios desde tu móvil sin etiquetas pegadas encima del tablero.",
    pain_heading: "Pensado para alta rotación",
    pain_points: [
        (
            "Buscador en tiempo real",
            "Cada persona escribe el producto deseado y ve su precio en el acto.",
        ),
        (
            "Adiós parches de papel sobre precios",
            "Mantén tu cartel siempre prolijo cuando ajustas valores.",
        ),
        (
            "Carga masiva desde texto o foto",
            "Importa tu lista actual mediante OCR o texto pegado.",
        ),
    ],
    blueprint_title: "Estructura recomendada para comida rápida",
    blueprint_categories: [
        (
            "Sándwiches a la Plancha",
            &[
                "Tostado mixto — R$ 13,00",
                "Lomito clásico — R$ 22,00",
                "Sándwich completo — R$ 31,00",
            ],
        ),
        (
            "Empanadas y Salados",
            &[
                "Empanada de carne — R$ 14,00",
                "Croqueta de pollo — R$ 10,00",
                "Chipá artesanal — R$ 10,00",
            ],
        ),
        (
            "Jugos y Licuados",
            &[
                "Jugo de naranja 500 ml — R$ 12,00",
                "Licuado de fresa y avena — R$ 15,00",
                "Bebida 600 ml — R$ 9,00",
            ],
        ),
    ],
    tips_heading: "Consejos para comida rápida",
    operational_tips: [
        "Coloca un cartel QR grande junto a la fila del mostrador.",
        "Destaca combos económicos en la primera categoría visible.",
        "Usa nuestra calculadora gratuita de costos antes de ajustar tarifas.",
    ],
    faqs: [
        (
            "¿Puedo editar precios desde mi teléfono?",
            "Sí. Todo el panel funciona de forma fluida desde el navegador móvil.",
        ),
        (
            "¿Cobran comisión por cliente que escanea?",
            "No. Escaneos ilimitados y entrega estática sin consultas al servidor.",
        ),
        (
            "¿Necesitan descargar alguna app los clientes?",
            "No. Cualquier cámara moderna abre el menú al instante.",
        ),
    ],
    related: [
        SegmentId::BurgerShop,
        SegmentId::Bakery,
        SegmentId::AcaiShop,
    ],
};

const PIZZERIA: SegmentContent = SegmentContent {
    id: SegmentId::Pizzeria,
    name: "Pizzerías",
    short_desc: "Decenas de sabores fáciles de buscar con pestañas fijas e impresión SVG.",
    meta_title: "Menú Digital con Código QR para Pizzería | QR Cardápio",
    meta_description: "Organiza sabores clásicos, especiales, pizzas dulces y bebidas en un menú digital con código QR permanente y descarga vectorial SVG para cajas y mesas.",
    h1: "Menú digital con código QR para pizzerías",
    answer_summary: "Las cartas de pizzería suelen reunir decenas de sabores ilegibles en un PDF pequeño. Con un menú digital con QR, tus comensales navegan entre Pizzas Clásicas, Especiales, Dulces y Bebidas y buscan ingredientes al instante.",
    pain_heading: "Deja atrás los PDF con zoom infinito",
    pain_points: [
        (
            "Filtra sabores por ingrediente",
            "Encuentra opciones vegetarianas, cuatro quesos o rúcula desde la barra superior.",
        ),
        (
            "Navegación fija por secciones",
            "Salta rápido entre clásicas, especiales, postres y vinos.",
        ),
        (
            "SVG vectorial para mesas y cajas",
            "Imprime con nitidez perfecta tanto en carteles de mesa como en cajas.",
        ),
    ],
    blueprint_title: "Estructura recomendada para pizzería",
    blueprint_categories: [
        (
            "Pizzas Clásicas",
            &[
                "Margherita tradicional — R$ 58,00",
                "Calabresa con cebolla — R$ 56,00",
                "Cuatro quesos — R$ 64,00",
            ],
        ),
        (
            "Pizzas Especiales",
            &[
                "Jamón crudo y rúcula — R$ 76,00",
                "Brie con mermelada — R$ 74,00",
                "Pepperoni — R$ 69,00",
            ],
        ),
        (
            "Pizzas Dulces y Bebidas",
            &[
                "Chocolate con fresas — R$ 54,00",
                "Banana y canela — R$ 52,00",
                "Gaseosa 2L — R$ 16,00",
            ],
        ),
    ],
    tips_heading: "Consejos para pizzerías",
    operational_tips: [
        "Aclara tamaños y porciones en la descripción de la categoría.",
        "Pega tu listado completo en el importador para cargarlo en segundos.",
        "Muestra la invitación de Google Maps al pedir la cuenta.",
    ],
    faqs: [
        (
            "¿Es tedioso cargar 40 sabores distintos?",
            "No. Pega tu lista en texto o sube un PDF para crear el borrador completo en bloque.",
        ),
        (
            "¿Sirve el código QR para formatos grandes de imprenta?",
            "Sí. Descarga SVG vectorial además de PNG en alta resolución.",
        ),
        (
            "¿Puedo revisar cambios de precio antes del fin de semana?",
            "Sí. Todo queda en borrador hasta que pulsas Publicar actualizaciones.",
        ),
    ],
    related: [
        SegmentId::Restaurant,
        SegmentId::BurgerShop,
        SegmentId::BarPub,
    ],
};

const ACAI_SHOP: SegmentContent = SegmentContent {
    id: SegmentId::AcaiShop,
    name: "Heladerías y Açaí",
    short_desc: "Sabores rotativos, tamaños de vasos, toppings y colores vibrantes de marca.",
    meta_title: "Menú Digital con Código QR para Heladería y Tienda de Açaí | QR Cardápio",
    meta_description: "Publica un menú digital colorido con código QR permanente para tu heladería o tienda de açaí: tamaños, toppings y control rápido de sabores.",
    h1: "Menú digital con código QR para heladerías y tiendas de açaí",
    answer_summary: "Las heladerías y tiendas de açaí cambian sabores y toppings cada semana. Un menú digital con código QR aplica el color exacto de tu marca, organiza vasos, coberturas y barcas con claridad y permite desactivar sabores agotados con un toque.",
    pain_heading: "Pensado para vitrinas dinámicas",
    pain_points: [
        (
            "Oculta sabores terminados en segundos",
            "Marca un sabor como no disponible en cuanto se termina la cubeta.",
        ),
        (
            "Tu color de marca en todo el diseño",
            "Selecciona tu tono morado o pastel en el panel y obtén contraste accesible.",
        ),
        (
            "Filas más ágiles en mostrador",
            "Los clientes eligen tamaño y toppings desde la fila antes de pagar.",
        ),
    ],
    blueprint_title: "Estructura recomendada para heladería y açaí",
    blueprint_categories: [
        (
            "Vasos de Açaí",
            &[
                "Açaí Tradicional 300 ml — R$ 18,00",
                "Açaí Tradicional 500 ml — R$ 25,00",
                "Barca 1L — R$ 48,00",
            ],
        ),
        (
            "Toppings y Frutas",
            &[
                "Leche en polvo — R$ 4,00",
                "Crema de avellanas — R$ 6,00",
                "Fresa y granola — R$ 5,00",
            ],
        ),
        (
            "Helados y Batidos",
            &[
                "Cucurucho 2 bochas — R$ 16,00",
                "Batido tropical — R$ 22,00",
                "Paleta artesanal — R$ 11,00",
            ],
        ),
    ],
    tips_heading: "Consejos para heladerías",
    operational_tips: [
        "Crea una categoría exclusiva de toppings para subir el ticket medio.",
        "Coloca códigos QR en mesas exteriores y vidrieras.",
        "Usa fotos nítidas de los vasos armados para impulsar adicionales.",
    ],
    faqs: [
        (
            "¿Puedo poner el color hexadecimal exacto de mi marca?",
            "Sí. El tema calcula automáticamente el contraste legible de textos y botones.",
        ),
        (
            "¿Puedo ocultar postres de invierno en verano?",
            "Sí. Desactiva la visibilidad de la categoría sin perder tus platos guardados.",
        ),
        (
            "¿Resulta práctico para familias y niños?",
            "Sí. Tarjetas claras, tipografía legible y filtros simples desde cualquier móvil.",
        ),
    ],
    related: [
        SegmentId::CoffeeShop,
        SegmentId::SnackBar,
        SegmentId::Bakery,
    ],
};

const BAKERY: SegmentContent = SegmentContent {
    id: SegmentId::Bakery,
    name: "Panaderías",
    short_desc: "Desayunos, panes de masa madre, pastelería y horarios de horneado.",
    meta_title: "Menú Digital con Código QR para Panadería y Confitería | QR Cardápio",
    meta_description: "Crea el menú digital de tu panadería con código QR permanente: desayunos, panes artesanales, tartas y disponibilidad actualizada al momento.",
    h1: "Menú digital con código QR para panaderías y confiterías",
    answer_summary: "Las panaderías combinan servicio de desayuno en mesas, barra rápida, panes de masa madre y pastelería fina. Con un menú digital por QR, tus comensales consultan combos y porciones sin esperar cartas impresas compartidas.",
    pain_heading: "Orden del desayuno a la merienda",
    pain_points: [
        (
            "Secciones por momento del día",
            "Separa Desayunos, Panes Artesanales, Pastelería y Bebidas con claridad.",
        ),
        (
            "Porciones disponibles al instante",
            "Evita pedidos de tartas o piezas ya agotadas en vitrina.",
        ),
        (
            "Impulsa tu reputación local",
            "Consigue más reseñas en Google Maps de tus clientes habituales.",
        ),
    ],
    blueprint_title: "Estructura recomendada para panadería",
    blueprint_categories: [
        (
            "Desayunos",
            &[
                "Pan tostado con queso — R$ 10,50",
                "Combo desayuno completo — R$ 34,00",
                "Omelette con queso — R$ 22,00",
            ],
        ),
        (
            "Panadería Artesanal",
            &[
                "Hogaza de masa madre — R$ 24,00",
                "Baguette rústica — R$ 14,00",
                "Hojaldre de jamón y queso — R$ 13,00",
            ],
        ),
        (
            "Pastelería",
            &[
                "Porción de tarta de chocolate — R$ 19,00",
                "Brioche de crema — R$ 11,00",
                "Croissant de almendras — R$ 15,00",
            ],
        ),
    ],
    tips_heading: "Consejos para panaderías",
    operational_tips: [
        "Indica horarios de horneado de panes especiales en la descripción.",
        "Coloca códigos QR en las mesas de desayuno y mostrador de encargos.",
        "Añade fotos reales de tus porciones de torta para ventas por impulso.",
    ],
    faqs: [
        (
            "¿Sirve el mismo enlace en mesas y escaparate?",
            "Sí. Tanto /c/tu-panaderia como el QR impreso son permanentes.",
        ),
        (
            "¿Puedo importar mi lista impresa?",
            "Sí. Sube una foto/PDF o pega el texto en Importar menú.",
        ),
        (
            "¿Puedo ocultar productos navideños fuera de temporada?",
            "Sí. Solo desactiva su visibilidad sin eliminar el registro.",
        ),
    ],
    related: [
        SegmentId::CoffeeShop,
        SegmentId::SnackBar,
        SegmentId::AcaiShop,
    ],
};

const BAR_PUB: SegmentContent = SegmentContent {
    id: SegmentId::BarPub,
    name: "Bares y Pubs",
    short_desc: "Cócteles de autor, grifos artesanales, picadas y lectura clara nocturna.",
    meta_title: "Menú Digital con Código QR para Bar y Pub | QR Cardápio",
    meta_description: "Crea un menú digital con código QR permanente para tu bar o pub: cócteles, cervezas tiradas, picadas y control rápido cuando se termina un barril.",
    h1: "Menú digital con código QR para bares y pubs",
    answer_summary: "En bares y pubs con luz tenue y mesas llenas, un menú digital con código QR se lee con total nitidez en la pantalla del teléfono, agiliza rondas de bebidas y permite marcar en dos segundos cuando se acaba un barril de cerveza artesanal.",
    pain_heading: "Control ágil durante la noche",
    pain_points: [
        (
            "¿Se acabó un barril? Márcalo al instante",
            "Desactiva grifos agotados desde el móvil de la barra antes de que el camarero tome el pedido.",
        ),
        (
            "Recetas de cócteles bien explicadas",
            "Muestra destilado base, notas cítricas o amargas sin textos apretados.",
        ),
        (
            "Placas QR de alto contraste",
            "Imprime carteles con margen blanco amplio que escanean rápido bajo luz nocturna.",
        ),
    ],
    blueprint_title: "Estructura recomendada para bar y pub",
    blueprint_categories: [
        (
            "Cervezas y Barriles",
            &[
                "Chop Pilsen 473 ml — R$ 16,00",
                "American IPA 473 ml — R$ 24,00",
                "Cubo con 5 botellas — R$ 55,00",
            ],
        ),
        (
            "Cócteles de Autor",
            &[
                "Gin Tonic cítrico — R$ 34,00",
                "Moscow Mule de la casa — R$ 36,00",
                "Caipirinha clásica — R$ 26,00",
            ],
        ),
        (
            "Picadas Para Compartir",
            &[
                "Pollo crujiente al ajo — R$ 48,00",
                "Bastones de queso — R$ 34,00",
                "Tabla de quesos y fiambres — R$ 68,00",
            ],
        ),
    ],
    tips_heading: "Consejos para bares y pubs",
    operational_tips: [
        "Conserva un margen blanco amplio alrededor del código QR sobre mesas oscuras.",
        "Mantén visibles las picadas en las pestañas superiores.",
        "Usa el pie de reseñas en Google Maps para atraer nuevos grupos el fin de semana.",
    ],
    faqs: [
        (
            "¿Escanea bien el QR con poca iluminación?",
            "Sí, impreso en negro mate sobre blanco de al menos 3×3 cm sin reflejo brillante.",
        ),
        (
            "¿Podemos rotar estilos de cerveza artesanal cada semana?",
            "Sí. Edita estilos y precios en el panel conservando exactamente el mismo código QR.",
        ),
        (
            "¿El cliente necesita descargar alguna aplicación?",
            "No. Abre instantáneamente en Safari o Chrome.",
        ),
    ],
    related: [
        SegmentId::BurgerShop,
        SegmentId::Pizzeria,
        SegmentId::Restaurant,
    ],
};
