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
    title: "How to create a QR Code digital menu in 5 minutes",
    short_desc: "Practical step-by-step from setup and OCR import to print-ready table QR codes.",
    meta_title: "How to Create a QR Code Digital Menu in 5 Minutes | QR Cardápio",
    meta_description: "Step-by-step guide to creating a permanent QR Code digital menu: pick a short link, organize categories or import a PDF via OCR, and print crisp table QR sheets.",
    read_minutes: 4,
    published_date: "2026-11-23",
    answer_summary: "To launch a QR Code digital menu you never need to reprint, pick a short permanent /c/slug URL, organize concise categories with clear prices and square photos (or import an existing PDF), publish a lightweight static snapshot, and download print-ready vector SVG QR codes for your tables.",
    sections: &[
        GuideSection {
            heading: "1. Choose a short, memorable URL",
            paragraphs: &[
                "Your menu URL should reflect how locals search for your venue—for example, qrcardapio.com.br/c/cafe-da-praca. Short links scan cleanly from smaller QR codes and look great in social bios.",
                "Locking your published slug ensures table stands and stickers remain valid even after dozens of seasonal menu updates.",
            ],
            bullets: &[
                "Keep the slug short and recognizable without filler words.",
                "Set your brand accent color and logo right during setup.",
            ],
        },
        GuideSection {
            heading: "2. Import your existing menu or add key categories",
            paragraphs: &[
                "You rarely need to type every item manually. Upload a sharp photo or digital PDF to extract categories and prices directly into a draft via local OCR.",
                "Group items into 4–5 clear categories (Starters, Mains, Sides, Drinks, Desserts) so seated guests find what they crave quickly.",
            ],
            bullets: &[
                "Add crisp 1:1 product photos on your top signature dishes.",
                "Write concise 1–2 line descriptions noting key ingredients or portion sizes.",
            ],
        },
        GuideSection {
            heading: "3. Print high-contrast QR codes with a clean quiet zone",
            paragraphs: &[
                "After previewing and publishing, export vector SVG for print shops or 1024 px PNG for local printers. Fast scanning relies on dark-on-light contrast and an uncropped white border around the code.",
            ],
            bullets: &[
                "Print table QR squares at least 3×3 cm (1.2×1.2 in).",
                "Choose matte lamination or non-glare acrylic stands under overhead lighting.",
            ],
        },
    ],
    comparison_table: None,
    faqs: [
        (
            "Can I edit prices after printing the table QR codes?",
            "Yes. All edits publish behind the exact same permanent URL and QR code.",
        ),
        (
            "How many languages can one QR code serve?",
            "Up to three languages (Portuguese, English, Spanish) with instant zero-reload switching.",
        ),
        (
            "Can I preview draft changes before guests see them?",
            "Yes. Everything stays in draft until you click Publish updates.",
        ),
    ],
    related_segment: SegmentId::Restaurant,
    related_tool: ToolId::QrGenerator,
};

const PDF_VS_DIGITAL: GuideContent = GuideContent {
    id: GuideId::PdfVsDigital,
    title: "PDF Menu vs. Digital QR Menu: Which Converts Better at Tables?",
    short_desc: "Compare 4G load time, mobile readability, sold-out control, and update costs.",
    meta_title: "PDF Menu vs. Digital QR Menu: Full Comparison | QR Cardápio",
    meta_description: "Compare static PDF menus against responsive HTML digital menus on 4G load speed, readability without zooming, instant price edits, and local SEO.",
    read_minutes: 5,
    published_date: "2026-11-23",
    answer_summary: "A responsive HTML digital menu outperforms PDF menus at tables because it opens almost instantaneously over 4G without forced downloads, formats text cleanly for phone screens without pinch-zooming, supports keyword search, and lets owners toggle sold-out dishes or adjust prices in seconds.",
    sections: &[
        GuideSection {
            heading: "Why PDF menus frustrate mobile diners",
            paragraphs: &[
                "Print-ready PDFs routinely weigh 8–20 MB due to uncompressed artwork. Inside busy dining rooms with weak cell reception, guests either stall waiting for a download or abandon scanning.",
                "Even after loading, A4 print layouts force two-finger pinch-zooming and horizontal panning—hurting high-margin drink and dessert impulse orders.",
            ],
            bullets: &[
                "Tiny print text hurts accessibility on 5–6 inch phones.",
                "Changing one price requires re-exporting and re-uploading a new file.",
            ],
        },
        GuideSection {
            heading: "Static HTML speed plus search-friendly structure",
            paragraphs: &[
                "Published QR Cardápio menus render into self-contained HTML + CSS + WebP assets served directly from disk with zero database overhead, while exposing clean Schema.org/Menu metadata to search engines.",
            ],
            bullets: &[
                "Instant item filtering and search by ingredient.",
                "Built-in PT / EN / ES switching with zero extra PDFs.",
            ],
        },
    ],
    comparison_table: Some(ComparisonTable {
        title: "Side-by-side comparison: Static PDF vs. QR Cardápio",
        headers: ["Factor", "Static PDF Menu", "QR Cardápio Digital Menu"],
        rows: &[
            [
                "Mobile payload size",
                "5 MB to 25 MB download",
                "~40 KB HTML + WebP thumbnails",
            ],
            [
                "Phone screen legibility",
                "Requires pinch-zoom & panning",
                "Single-column responsive cards",
            ],
            [
                "Updating a dish price",
                "Edit source file, export, re-upload",
                "Save & publish in one click",
            ],
            [
                "Marking sold-out items",
                "Verbal server apology at table",
                "Instant 'Unavailable' badge",
            ],
            [
                "Multilingual menus",
                "3 separate PDFs & QR links",
                "Single QR with instant language toggle",
            ],
            [
                "Google Maps reviews",
                "Buried static link if any",
                "Native review invitation card in footer",
            ],
        ],
    }),
    faqs: [
        (
            "I already designed a PDF menu—do I lose that work?",
            "No. Upload your PDF in Menu Import to extract dishes and prices into your digital draft via OCR.",
        ),
        (
            "Does a static digital menu use much guest mobile data?",
            "Barely any—lightweight HTML and WebP thumbnails transfer a tiny fraction of a PDF file.",
        ),
        (
            "Can search engines index digital menus?",
            "Yes. Published menus include clean metadata and Menu schema markup.",
        ),
    ],
    related_segment: SegmentId::Pizzeria,
    related_tool: ToolId::TablePlacard,
};

const GOOGLE_MAPS_REVIEWS: GuideContent = GuideContent {
    id: GuideId::GoogleMapsReviews,
    title: "How to Get More Google Maps Reviews Directly from Your Menu",
    short_desc: "Turn happy seated guests into authentic neighborhood proof without friction.",
    meta_title: "How to Get More Google Maps Reviews by Menu | QR Cardápio",
    meta_description: "Learn how to find your official Google Business review link and convert satisfied restaurant guests into steady Google Maps reviews.",
    read_minutes: 4,
    published_date: "2026-11-23",
    answer_summary: "To steadily earn authentic Google Maps reviews, copy your official 'Ask for reviews' URL from Google Business Profile, paste it into your QR Cardápio settings, and pair the native menu footer prompt with a clean table or register QR placard.",
    sections: &[
        GuideSection {
            heading: "Why fresh reviews drive Google Maps visibility",
            paragraphs: &[
                "Google local pack rankings weigh relevance, distance, and prominence. Steady weekly reviews from genuine visitors signal active hospitality quality and help nearby diners pick your restaurant over competitors.",
                "Most happy guests skip reviewing later simply because searching your business name after leaving feels like work.",
            ],
            bullets: &[
                "Direct one-tap review links remove friction right at peak satisfaction.",
                "Consistent monthly velocity matters more than a one-time burst.",
            ],
        },
        GuideSection {
            heading: "Two-step setup in QR Cardápio",
            paragraphs: &[
                "Grab your short review link from Google Business Profile (or your Maps share URL), save it under Dashboard Settings, and republish. Every guest scrolling your menu now sees a subtle review invitation card.",
                "Avoid offering discounts for 5-star ratings—simply invite honest feedback about the meal and atmosphere.",
            ],
            bullets: &[
                "Pair the menu footer card with a dedicated counter placard near checkout.",
                "Reply promptly to every review to reinforce local trust.",
            ],
        },
    ],
    comparison_table: None,
    faqs: [
        (
            "What is the difference between a Maps profile link and direct review link?",
            "A direct review link opens the star-rating prompt immediately for signed-in Google users, removing extra taps.",
        ),
        (
            "Can I print a standalone review placard for the cashier counter?",
            "Yes—use our free Table QR Placard tool with your direct Google review URL.",
        ),
        (
            "Does replying to reviews help local rankings?",
            "Yes, thoughtful replies show active customer care and naturally reinforce local context.",
        ),
    ],
    related_segment: SegmentId::BurgerShop,
    related_tool: ToolId::TablePlacard,
};

const MENU_ENGINEERING_QR: GuideContent = GuideContent {
    id: GuideId::MenuEngineeringQr,
    title: "Digital Menu Engineering & Table QR Code Printing Standards",
    short_desc: "Position high-margin dishes effectively and print codes that scan first try.",
    meta_title: "Menu Engineering & QR Code Printing Best Practices | QR Cardápio",
    meta_description: "Apply digital menu engineering (food cost vs. popularity) and physical QR printing guidelines (size, contrast, matte finish) to maximize table sales.",
    read_minutes: 5,
    published_date: "2026-11-23",
    answer_summary: "Digital menu engineering combines ordering high-margin 'star' dishes near the top of key categories with appetizing 1:1 photos and strict food-cost tracking, backed by physical QR codes printed at 3×3 cm or larger with clean white borders and matte glare-free finishes.",
    sections: &[
        GuideSection {
            heading: "Lead with high-margin star dishes",
            paragraphs: &[
                "Map every menu item by popularity and contribution margin (Selling Price minus Ingredient & Packaging Cost). Feature high-margin favorites at the top of each category with clear appetizing photos.",
                "Concise sensory notes—roasting method, house sauce, portion count—lift perceived quality and defend healthy margins.",
            ],
            bullets: &[
                "Break oversized lists into scannable categories of 6–10 items.",
                "Recalculate food cost quarterly when key protein or dairy prices shift.",
            ],
        },
        GuideSection {
            heading: "Physical printing checklist for effortless scanning",
            paragraphs: &[
                "Scan failures almost always stem from glossy sleeves catching ceiling glare, tiny codes under 2.5 cm, or designers trimming away the white border.",
            ],
            bullets: &[
                "Print at least 3×3 cm on dining tables and 5×5 cm at counters.",
                "Keep dark modules on clean light backgrounds (#161721 on white/cream).",
                "Test printed proofs with both iOS and Android cameras before batch laminating.",
            ],
        },
    ],
    comparison_table: None,
    faqs: [
        (
            "Can I print QR codes in bright brand colors?",
            "Keep the QR modules dark against a clean white inner box, and use your brand colors on the surrounding frame.",
        ),
        (
            "Where should profitable dishes sit on mobile?",
            "Among the first three items of your opening visible categories, ideally paired with square photography.",
        ),
        (
            "How do I check if a dish price protects my margin?",
            "Run it through our free Food Cost Calculator including packaging and card processing fees.",
        ),
    ],
    related_segment: SegmentId::CoffeeShop,
    related_tool: ToolId::CmvCalculator,
};
