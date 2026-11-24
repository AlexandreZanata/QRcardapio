use super::{ComparisonTable, GuideContent, GuideId, GuideSection};
use crate::seo::segments::SegmentId;
use crate::seo::tools::ToolId;

pub fn get(id: GuideId) -> &'static GuideContent {
    match id {
        GuideId::CreateQrMenu => &CREATE_QR_MENU,
        GuideId::PdfVsDigital => &PDF_VS_DIGITAL,
        GuideId::GoogleMapsReviews => &GOOGLE_MAPS_REVIEWS,
        GuideId::MenuEngineeringQr => &MENU_ENGINEERING_QR,
    }
}

const CREATE_QR_MENU: GuideContent = GuideContent {
    id: GuideId::CreateQrMenu,
    title: "Cómo crear un menú digital con código QR en 5 minutos",
    short_desc: "Guía paso a paso desde el registro y la importación OCR hasta la impresión.",
    meta_title: "Cómo Crear un Menú Digital con Código QR en 5 Minutos | QR Cardápio",
    meta_description: "Aprende a crear tu menú digital con código QR permanente: define un enlace corto, importa tu PDF con OCR, revisa en el móvil e imprime tus placas de mesa.",
    read_minutes: 4,
    published_date: "2026-11-23",
    answer_summary: "Para crear un menú digital con código QR que nunca debas reimprimir, elige una dirección corta /c/tu-local, organiza categorías breves con precios claros y fotos cuadradas (o importa tu PDF actual), publica una página estática rápida y descarga tu QR vectorial SVG para las mesas.",
    sections: &[
        GuideSection {
            heading: "1. Elige una dirección breve y reconocible",
            paragraphs: &[
                "La URL de tu menú debe reflejar el nombre comercial de tu local — por ejemplo, qrcardapio.com.br/c/cafe-da-praca. Los enlaces cortos generan códigos QR más simples de escanear y lucen mejor en redes sociales.",
                "Una vez publicado, el enlace permanece bloqueado para proteger tu inversión en acrílicos, carteles y adhesivos.",
            ],
            bullets: &[
                "Usa el nombre directo del negocio sin palabras de relleno.",
                "Configura tu color principal y logotipo desde el primer paso.",
            ],
        },
        GuideSection {
            heading: "2. Importa tu carta actual o carga categorías clave",
            paragraphs: &[
                "No necesitas copiar plato por plato manualmente. Sube un PDF o foto nítida para extraer categorías y precios automáticamente en borrador mediante OCR.",
                "Divide en cuatro o cinco secciones claras (Entradas, Principales, Guarniciones, Bebidas, Postres) para facilitar la lectura.",
            ],
            bullets: &[
                "Agrega fotos reales 1:1 en tus platos más rentables.",
                "Redacta descripciones breves de una o dos líneas con ingredientes clave.",
            ],
        },
        GuideSection {
            heading: "3. Imprime el código QR con buen contraste y margen blanco",
            paragraphs: &[
                "Al publicar, descarga SVG vectorial para imprenta o PNG de 1024 px para impresoras de oficina. Una lectura rápida requiere buen contraste oscuro sobre fondo claro y margen blanco intacto.",
            ],
            bullets: &[
                "Tamaño mínimo recomendado en mesas: 3×3 cm sin pliegues.",
                "Prefiere laminado mate para evitar reflejos de luces.",
            ],
        },
    ],
    comparison_table: None,
    faqs: [
        (
            "¿Puedo cambiar precios después de imprimir los códigos QR?",
            "Sí. Cada actualización publicada mantiene exactamente la misma URL y el mismo código impreso.",
        ),
        (
            "¿Cuántos idiomas admite un mismo código QR?",
            "Hasta tres idiomas (portugués, inglés y español) con cambio instantáneo sin recargar.",
        ),
        (
            "¿Puedo revisar cambios antes de mostrarlos a clientes?",
            "Sí. Todo cambio se guarda en borrador hasta que confirmas la publicación.",
        ),
    ],
    related_segment: SegmentId::Restaurant,
    related_tool: ToolId::QrGenerator,
};

const PDF_VS_DIGITAL: GuideContent = GuideContent {
    id: GuideId::PdfVsDigital,
    title: "Menú en PDF vs. Menú Digital: ¿cuál vende más en mesa?",
    short_desc: "Comparativa de velocidad en 4G, lectura móvil, actualizaciones y SEO local.",
    meta_title: "Menú en PDF vs. Menú Digital: Comparativa Completa | QR Cardápio",
    meta_description: "Compara menú en PDF y menú digital HTML en velocidad 4G, lectura sin zoom, control de platos agotados, idiomas y visibilidad en Google.",
    read_minutes: 5,
    published_date: "2026-11-23",
    answer_summary: "Un menú digital HTML responsivo vende más que un PDF porque abre al instante sobre datos móviles sin descargas pesadas, adapta textos y fotografías al tamaño de pantalla sin hacer zoom, ofrece búsqueda rápida y permite marcar platos agotados en segundos.",
    sections: &[
        GuideSection {
            heading: "La fricción oculta de los archivos PDF en el móvil",
            paragraphs: &[
                "Los PDF pensados para imprenta suelen pesar entre 8 y 25 MB. En salones llenos con señal móvil moderada, la descarga lenta hace que el cliente pierda interés antes de ver las bebidas o entradas.",
                "Además, leer columnas diminutas haciendo zoom lateral resulta incómodo en teléfonos compactos.",
            ],
            bullets: &[
                "Obliga a ampliar y desplazar lateralmente en pantallas de 5 a 6 pulgadas.",
                "Un ajuste mínimo de precio exige exportar y volver a subir otro archivo.",
            ],
        },
        GuideSection {
            heading: "Páginas HTML estáticas: velocidad instantánea y SEO",
            paragraphs: &[
                "QR Cardápio compila cada publicación en archivos estáticos ligeros con cero consultas a base de datos, sumando datos estructurados Schema.org/Menu comprensibles para buscadores.",
            ],
            bullets: &[
                "Filtro instantáneo por categoría e ingrediente.",
                "Selector de idioma PT / EN / ES sin multiplicar enlaces QR.",
            ],
        },
    ],
    comparison_table: Some(ComparisonTable {
        title: "Comparativa directa: PDF estático vs. QR Cardápio digital",
        headers: ["Criterio", "Menú en PDF", "QR Cardápio Digital"],
        rows: &[
            [
                "Peso promedio móvil",
                "5 MB a 25 MB",
                "~40 KB HTML + miniaturas WebP",
            ],
            [
                "Lectura en pantalla pequeña",
                "Exige zoom y desplazamiento",
                "Diseño vertical en 1 columna",
            ],
            [
                "Cambio de precios",
                "Reexportar y sustituir enlace",
                "Editar y publicar en 1 clic",
            ],
            [
                "Platos agotados",
                "Avisar verbalmente en mesa",
                "Etiqueta 'No disponible' inmediata",
            ],
            [
                "Idiomas PT / EN / ES",
                "3 PDFs y 3 QR distintos",
                "Un solo QR con cambio instantáneo",
            ],
            [
                "Reseñas en Google Maps",
                "Enlace oculto o inexistente",
                "Tarjeta nativa de invitación al pie",
            ],
        ],
    }),
    faqs: [
        (
            "¿Pierdo mi diseño si ya tengo la carta en PDF?",
            "No. Sube tu PDF al importador OCR y convierte tus secciones y precios en borrador editable.",
        ),
        (
            "¿Consume muchos datos móviles a los comensales?",
            "Muy pocos: la página estática pesa una mínima fracción de un PDF gráfico.",
        ),
        (
            "¿Los buscadores entienden los platos publicados?",
            "Sí. Cada menú publicado incluye metadatos y marcado Schema.org/Menu.",
        ),
    ],
    related_segment: SegmentId::Pizzeria,
    related_tool: ToolId::TablePlacard,
};

const GOOGLE_MAPS_REVIEWS: GuideContent = GuideContent {
    id: GuideId::GoogleMapsReviews,
    title: "Cómo conseguir más reseñas en Google Maps desde tu menú digital",
    short_desc: "Convierte la satisfacción en mesa en posicionamiento local orgánico.",
    meta_title: "Cómo Conseguir Más Reseñas en Google Maps con tu Menú | QR Cardápio",
    meta_description: "Aprende a obtener tu enlace oficial de reseñas en Google y transformar comensales satisfechos en valoraciones reales en Google Maps.",
    read_minutes: 4,
    published_date: "2026-11-23",
    answer_summary: "Para sumar reseñas reales en Google Maps de forma constante, copia tu enlace oficial 'Solicitar reseñas' del Perfil de Empresa, guárdalo en la configuración de QR Cardápio y acompaña la tarjeta del pie del menú con un cartel QR junto a la caja.",
    sections: &[
        GuideSection {
            heading: "Por qué las reseñas recientes impulsan el mapa local",
            paragraphs: &[
                "El algoritmo local de Google pondera relevancia, distancia y notoriedad. Mantener un flujo constante de reseñas recientes con buena puntuación mejora tu visibilidad frente a locales cercanos.",
                "La mayoría de los clientes contentos no dejan opinión simplemente por olvidar buscar tu local más tarde.",
            ],
            bullets: &[
                "Un enlace directo elimina clics entre la buena experiencia y las estrellas.",
                "La constancia mensual pesa más que campañas aisladas.",
            ],
        },
        GuideSection {
            heading: "Configuración en dos pasos",
            paragraphs: &[
                "Copia el enlace corto oficial desde Google Business Profile, pégalo en Configuración dentro de QR Cardápio y publica. Al pie del menú aparecerá un botón claro de invitación.",
                "Solicita siempre opiniones honestas sin condicionar recompensas a una nota específica.",
            ],
            bullets: &[
                "Complementa el pie del menú con un cartel impreso junto a la salida o caja.",
                "Responde cada comentario con cordialidad mencionando tu especialidad.",
            ],
        },
    ],
    comparison_table: None,
    faqs: [
        (
            "¿Qué diferencia hay entre el enlace de ficha y el enlace directo?",
            "El enlace directo abre de inmediato el selector de estrellas para usuarios logueados en Google.",
        ),
        (
            "¿Puedo imprimir un cartel exclusivo para reseñas?",
            "Sí. Usa nuestro Creador de Cartel QR gratuito con tu enlace directo de reseñas.",
        ),
        (
            "¿Responder reseñas ayuda en las búsquedas?",
            "Sí, demuestra atención activa y aporta contexto natural sobre tus platos y zona.",
        ),
    ],
    related_segment: SegmentId::BurgerShop,
    related_tool: ToolId::TablePlacard,
};

const MENU_ENGINEERING_QR: GuideContent = GuideContent {
    id: GuideId::MenuEngineeringQr,
    title: "Ingeniería de menú e impresión óptima de códigos QR para mesas",
    short_desc: "Destaca tus platos más rentables e imprime QRs que escanean al instante.",
    meta_title: "Ingeniería de Menú y Estándares de Impresión QR | QR Cardápio",
    meta_description: "Aplica ingeniería de menú (margen vs. popularidad) junto con reglas técnicas de impresión QR para vender platos más rentables y evitar fallos de lectura.",
    read_minutes: 5,
    published_date: "2026-11-23",
    answer_summary: "La ingeniería de menú digital combina ubicar los platos 'estrella' de mayor margen al inicio de cada sección con fotos reales cuadradas y control estricto de costos, junto a códigos QR impresos desde 3×3 cm con margen blanco y acabado mate sin reflejos.",
    sections: &[
        GuideSection {
            heading: "Posiciona tus platos estrella al principio",
            paragraphs: &[
                "Clasifica tu carta cruzando volumen de ventas y margen de contribución (Precio menos costo de insumos y envase). Coloca los ganadores en las primeras líneas con fotografía nítida.",
                "Descripciones claras de técnica o salsa artesanal elevan el valor percibido del plato.",
            ],
            bullets: &[
                "Evita listas eternas de 30 productos en una sola categoría.",
                "Revisa tu CMV cada trimestre ante variaciones de ingredientes.",
            ],
        },
        GuideSection {
            heading: "Lista de verificación para impresión en papel o acrílico",
            paragraphs: &[
                "Casi todos los errores de escaneo ocurren por tamaños menores a 2,5 cm, fundas plásticas brillantes con reflejo o bordes blancos recortados.",
            ],
            bullets: &[
                "Mínimo 3×3 cm en mesas y 5×5 cm en mostradores.",
                "Contraste fuerte oscuro sobre blanco o crema claro.",
                "Prueba con cámaras iOS y Android antes de laminar todo el lote.",
            ],
        },
    ],
    comparison_table: None,
    faqs: [
        (
            "¿Puedo imprimir el QR de colores?",
            "Mantén los módulos del código muy oscuros sobre fondo blanco y aplica los colores de marca al marco exterior.",
        ),
        (
            "¿Dónde colocar los platos más rentables en el móvil?",
            "Entre los tres primeros puestos de las secciones iniciales con foto cuadrada.",
        ),
        (
            "¿Cómo verificar si un precio cubre costos y comisiones?",
            "Pruébalo en nuestra Calculadora gratuita de Costo y Precio de Menú.",
        ),
    ],
    related_segment: SegmentId::CoffeeShop,
    related_tool: ToolId::CmvCalculator,
};
