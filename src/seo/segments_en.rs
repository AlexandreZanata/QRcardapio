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
    name: "Restaurants",
    short_desc: "Daily specials, full-course menus, and instant multilingual service at every table.",
    meta_title: "QR Code Digital Menu for Restaurants | QR Cardápio",
    meta_description: "Build a fast QR Code digital menu for your restaurant with instant dish updates, sold-out toggles, crisp food photos, and zero-reload PT, EN, and ES switching.",
    h1: "QR Code digital menu for restaurants",
    answer_summary: "A QR Code digital menu for restaurants lets seated guests browse courses, descriptions, and prices right in their mobile browser—without app downloads or pinch-to-zoom PDFs. Owners update daily specials, prices, and sold-out dishes from the dashboard while keeping the exact same printed table QR Code.",
    pain_heading: "Built for dining room speed",
    pain_points: [
        (
            "Daily specials without reprinting",
            "Update lunch specials or seasonal ingredients in seconds without replacing table displays.",
        ),
        (
            "Instant sold-out status",
            "Flag unavailable cuts or desserts in one tap so servers avoid awkward table follow-ups.",
        ),
        (
            "Seamless international hospitality",
            "Provide Portuguese, English, and Spanish translations under the same QR Code.",
        ),
    ],
    blueprint_title: "Recommended restaurant menu structure",
    blueprint_categories: [
        (
            "Starters & Shareables",
            &[
                "House Bruschetta — R$ 28.00",
                "Crispy Rib Croquettes (6 pc) — R$ 36.00",
                "Burrata & Greens Salad — R$ 44.00",
            ],
        ),
        (
            "Main Courses",
            &[
                "Wood-Fired Tenderloin — R$ 68.00",
                "Grilled Salmon & Seasonal Veg — R$ 74.00",
                "Wild Mushroom Risotto — R$ 58.00",
            ],
        ),
        (
            "Drinks & Desserts",
            &[
                "Fresh Pressed Juice 400 ml — R$ 14.00",
                "House Red Wine Glass — R$ 32.00",
                "Classic Milk Flan — R$ 22.00",
            ],
        ),
    ],
    tips_heading: "Table deployment tips",
    operational_tips: [
        "Print table QR codes at least 3×3 cm (1.2×1.2 in) with a clean white quiet-zone border.",
        "Add real 1:1 dish photos to your highest-margin signature plates.",
        "Connect your Google Maps review link at the bottom of the menu to grow local ratings.",
    ],
    faqs: [
        (
            "Do I need to print new QR codes after updating restaurant prices?",
            "No. Your /c/your-restaurant URL stays permanent once published; updates go live behind the same QR code.",
        ),
        (
            "Can tourists read the menu in English or Spanish?",
            "Yes. Every category and dish supports optional English and Spanish translations with instant switching.",
        ),
        (
            "Can I import my existing PDF menu?",
            "Yes. The built-in OCR importer extracts categories, items, and prices from PDFs, photos, or pasted text.",
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
    name: "Burger Shops",
    short_desc: "Appetizing cover photography, smash burgers, combos, and Google Maps reviews.",
    meta_title: "QR Code Digital Menu for Burger Shops | QR Cardápio",
    meta_description: "Create a QR Code digital menu for your burger shop with mouthwatering burger photos, combos, permanent table QR codes, and a built-in Google Maps review prompt.",
    h1: "QR Code digital menu for burger shops",
    answer_summary: "For craft and smash burger shops, a QR Code digital menu showcases high-resolution burger photography right at the top cover, organizes combos and sides into quick filter tabs, and turns happy dining-room guests into fresh 5-star Google Maps reviews.",
    pain_heading: "Why burger shops switch to QR Cardápio",
    pain_points: [
        (
            "Photography that sells the crust",
            "Your first visible burger photo automatically becomes the branded hero cover on mobile.",
        ),
        (
            "Limited-run burgers without print bills",
            "Launch weekend specials in draft mode and publish before Friday service.",
        ),
        (
            "Climb local Google Maps rankings",
            "Invite satisfied guests to leave a Google Maps review right inside the menu footer.",
        ),
    ],
    blueprint_title: "Recommended burger shop menu structure",
    blueprint_categories: [
        (
            "Craft & Smash Burgers",
            &[
                "Double Smash Cheddar — R$ 34.90",
                "Smoked Bacon 180g — R$ 41.90",
                "Crispy Chickpea Veggie — R$ 36.00",
            ],
        ),
        (
            "Fries & Sides",
            &[
                "Paprika Crinkle Fries — R$ 22.00",
                "Crispy Onion Rings — R$ 26.00",
                "House Chicken Tenders — R$ 28.00",
            ],
        ),
        (
            "Shakes & Drinks",
            &[
                "Hazelnut Milkshake 400 ml — R$ 27.00",
                "Craft Soda Can — R$ 8.50",
                "Draft IPA 500 ml — R$ 24.00",
            ],
        ),
    ],
    tips_heading: "Burger shop best practices",
    operational_tips: [
        "Place drinks and shakes immediately after burgers to drive combo add-ons.",
        "Keep descriptions concise: patty weight, cheese blend, and house sauce.",
        "Use wipeable acrylic table tents or laminated cards on greasy dining tables.",
    ],
    faqs: [
        (
            "Does the menu open fast over mobile data?",
            "Yes. Published menus are pure static HTML/WebP files with zero database queries per scan.",
        ),
        (
            "Can I match my burger brand colors and logo?",
            "Yes. Upload your logo and pick your accent color—contrast-safe button styles are generated automatically.",
        ),
        (
            "Can I share the same link on Instagram bio?",
            "Yes. One permanent URL works on tables, flyers, WhatsApp, and social bios.",
        ),
    ],
    related: [SegmentId::SnackBar, SegmentId::BarPub, SegmentId::Pizzeria],
};

const COFFEE_SHOP: SegmentContent = SegmentContent {
    id: SegmentId::CoffeeShop,
    name: "Coffee Shops",
    short_desc: "Pour-over brew methods, pastry case availability, and calm mobile reading.",
    meta_title: "QR Code Digital Menu for Coffee Shops | QR Cardápio",
    meta_description: "Clean QR Code digital menu for coffee shops and cafés: specialty espresso, pour-over notes, pastry availability toggles, and multilingual support.",
    h1: "QR Code digital menu for coffee shops",
    answer_summary: "In specialty coffee shops and cafés, a QR Code digital menu cleanly presents espresso drinks, pour-over origin notes, and fresh pastries. When the last almond croissant sells out mid-afternoon, toggle it as unavailable from your phone in seconds.",
    pain_heading: "Smooth flow from counter to tables",
    pain_points: [
        (
            "Live pastry-case control",
            "Mark sold-out cakes or croissants immediately so guests know what is still fresh.",
        ),
        (
            "Space for origin and roast notes",
            "Detail tasting notes, brew methods, and dairy-free milk options cleanly.",
        ),
        (
            "Warm hospitality styling",
            "Customize your menu palette to match your café's branding and atmosphere.",
        ),
    ],
    blueprint_title: "Recommended coffee shop menu structure",
    blueprint_categories: [
        (
            "Espresso & Brew Bar",
            &[
                "Double Espresso — R$ 9.50",
                "Velvet Cappuccino 200 ml — R$ 15.00",
                "Single-Origin Hario V60 — R$ 18.00",
            ],
        ),
        (
            "Savory Bakery",
            &[
                "French Butter Croissant — R$ 16.00",
                "Warm Cheese Bread (3 pc) — R$ 14.00",
                "Avocado Sourdough Toast — R$ 29.00",
            ],
        ),
        (
            "Pastries & Cold Brew",
            &[
                "Carrot Cake with Ganache — R$ 19.00",
                "Iced Vanilla Latte — R$ 18.50",
                "Espresso Affogato — R$ 21.00",
            ],
        ),
    ],
    tips_heading: "Coffee shop best practices",
    operational_tips: [
        "Create a seasonal iced drinks tab during warmer months.",
        "Place small corner QR stickers on compact two-top café tables.",
        "Invite remote-working regulars to leave a Google Maps review from the menu.",
    ],
    faqs: [
        (
            "Does it work well for international visitors?",
            "Yes. Add English and Spanish brew descriptions right alongside your default language.",
        ),
        (
            "Do I have to delete an item when a pastry sells out?",
            "No. Toggle availability off so it shows an unavailable badge while staying on your menu.",
        ),
        (
            "How fast can I publish my café menu?",
            "Under five minutes manually or by importing your existing PDF or menu photo via OCR.",
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
    name: "Snack Bars",
    short_desc: "Quick counter service, instant item search, and price edits from any phone.",
    meta_title: "QR Code Digital Menu for Snack Bars | QR Cardápio",
    meta_description: "Create a fast QR Code digital menu for your snack bar: instant keyword search, easy mobile price updates, and permanent printable QR codes.",
    h1: "QR Code digital menu for snack bars",
    answer_summary: "Busy snack bars thrive on speed: customers scan the QR Code at the counter or table and instantly search sandwiches, pastries, and fresh juices by name. Owners update ingredient-driven prices from their phone without sticky price labels.",
    pain_heading: "Built for high-turnover counters",
    pain_points: [
        (
            "Instant menu search",
            "Guests type a sandwich or juice name and jump straight to the right price.",
        ),
        (
            "No more taped-over price boards",
            "Keep counters tidy and updated whenever supplier costs change.",
        ),
        (
            "Quick import from existing menus",
            "Snap a photo or paste text from your current price list to draft categories fast.",
        ),
    ],
    blueprint_title: "Recommended snack bar menu structure",
    blueprint_categories: [
        (
            "Grilled Sandwiches",
            &[
                "Classic Pressed Cheese — R$ 13.00",
                "House Burger Melt — R$ 22.00",
                "Loaded Club Sandwich — R$ 31.00",
            ],
        ),
        (
            "Savory Snacks",
            &[
                "Crispy Beef Empanada — R$ 14.00",
                "Chicken Croquette — R$ 10.00",
                "Stuffed Baked Roll — R$ 11.00",
            ],
        ),
        (
            "Fresh Juices & Smoothies",
            &[
                "Fresh Orange Juice 500 ml — R$ 12.00",
                "Strawberry Oat Smoothie — R$ 15.00",
                "Bottled Soda 600 ml — R$ 9.00",
            ],
        ),
    ],
    tips_heading: "Snack bar best practices",
    operational_tips: [
        "Mount a larger QR poster near the queue entrance so guests decide before ordering.",
        "Put best-selling value combos in the top category.",
        "Check margins with our free food-cost calculator before updating menu prices.",
    ],
    faqs: [
        (
            "Can I edit prices from my smartphone?",
            "Yes. The entire dashboard works smoothly on mobile browsers.",
        ),
        (
            "Are there per-order commissions or scan limits?",
            "No. Scans are unlimited and static delivery handles traffic spikes effortlessly.",
        ),
        (
            "Do guests need a special scanner app?",
            "No. Every modern iPhone and Android camera opens QR links directly.",
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
    name: "Pizzerias",
    short_desc: "Dozens of toppings made searchable with clean category tabs and crisp QR print.",
    meta_title: "QR Code Digital Menu for Pizzerias | QR Cardápio",
    meta_description: "Organize dozens of pizza flavors, sweet pies, and drinks in a searchable QR Code digital menu with permanent vector SVG and high-res PNG printing.",
    h1: "QR Code digital menu for pizzerias",
    answer_summary: "Pizzeria menus often pack 30 to 60 flavors onto crowded paper or tiny PDFs. A searchable QR Code digital menu groups Classic, Specialty, Dessert Pizzas, and Drinks into sticky category pills so guests find toppings and allergens in seconds.",
    pain_heading: "Leave zoomed-in PDFs behind",
    pain_points: [
        (
            "Search by topping in one tap",
            "Guests craving pepperoni, arugula, or four-cheese filter the full list instantly.",
        ),
        (
            "Sticky category navigation",
            "Jump smoothly between Classic Pies, Chef Specials, Sweet Pizzas, and Wine.",
        ),
        (
            "Vector QR for tables and pizza boxes",
            "Download print-sharp SVG files for both table tents and delivery box inserts.",
        ),
    ],
    blueprint_title: "Recommended pizzeria menu structure",
    blueprint_categories: [
        (
            "Classic Pizzas",
            &[
                "Classic Margherita — R$ 58.00",
                "Calabresa & Red Onion — R$ 56.00",
                "Four Cheese Blend — R$ 64.00",
            ],
        ),
        (
            "Chef Specialties",
            &[
                "Prosciutto & Arugula — R$ 76.00",
                "Brie & Apricot Jam — R$ 74.00",
                "Smoked Pepperoni — R$ 69.00",
            ],
        ),
        (
            "Dessert Pies & Drinks",
            &[
                "Chocolate Strawberry Pie — R$ 54.00",
                "Cinnamon Banana Pie — R$ 52.00",
                "2L Family Soda — R$ 16.00",
            ],
        ),
    ],
    tips_heading: "Pizzeria best practices",
    operational_tips: [
        "Note pizza diameters and slice counts inside each category description.",
        "Paste your existing 40-flavor list into our text importer to pre-fill drafts fast.",
        "Include a Google Maps review prompt while tables wait for dessert or the check.",
    ],
    faqs: [
        (
            "Is it slow to enter 40+ pizza flavors?",
            "No. Paste your existing text list or upload a PDF to populate categories and items in bulk.",
        ),
        (
            "Can I print the QR code on large banners or pizza boxes?",
            "Yes. Download crisp SVG vector files alongside 1024px+ PNG images.",
        ),
        (
            "Can I preview weekend price adjustments safely?",
            "Yes. Edits remain in draft until you click Publish updates.",
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
    name: "Açaí & Ice Cream Shops",
    short_desc: "Rotating flavors, cup sizes, toppings, and bold custom brand colors.",
    meta_title: "QR Code Digital Menu for Açaí & Ice Cream Shops | QR Cardápio",
    meta_description: "Publish a vibrant QR Code digital menu for your açaí or ice cream shop: cup sizes, toppings, seasonal flavor toggles, and custom brand colors.",
    h1: "QR Code digital menu for açaí and ice cream shops",
    answer_summary: "Açaí and gelato counters rotate flavors and toppings daily. A QR Code digital menu lets you style the menu in your shop's signature accent color, display cup sizes and add-ons clearly, and toggle sold-out flavors off without crossing out menu boards.",
    pain_heading: "Designed for rotating flavors",
    pain_points: [
        (
            "Hide empty tubs instantly",
            "Toggle seasonal flavors unavailable the moment a batch finishes.",
        ),
        (
            "True-to-brand color palette",
            "Match your signature purple or pastel brand hue across buttons and headers.",
        ),
        (
            "Faster counter decisions",
            "Walk-in guests pick cup size and toppings on their phone before reaching the register.",
        ),
    ],
    blueprint_title: "Recommended açaí & ice cream menu structure",
    blueprint_categories: [
        (
            "Açaí Bowls & Cups",
            &[
                "Classic Açaí 300 ml — R$ 18.00",
                "Classic Açaí 500 ml — R$ 25.00",
                "Sharing Boat 1L — R$ 48.00",
            ],
        ),
        (
            "Toppings & Fresh Fruit",
            &[
                "Powdered Milk — R$ 4.00",
                "Hazelnut Cream — R$ 6.00",
                "Fresh Strawberry & Granola — R$ 5.00",
            ],
        ),
        (
            "Gelato & Shakes",
            &[
                "Two-Scoop Waffle Cone — R$ 16.00",
                "Tropical Milkshake — R$ 22.00",
                "Artisanal Popsicle — R$ 11.00",
            ],
        ),
    ],
    tips_heading: "Açaí & ice cream best practices",
    operational_tips: [
        "Create a dedicated Toppings section to boost average order add-ons.",
        "Place QR stickers on patio tables and storefront glass for queue browsing.",
        "Highlight loaded bowl photos to inspire premium topping choices.",
    ],
    faqs: [
        (
            "Can I use my shop's exact hex brand color?",
            "Yes. Pick any custom hex color in Settings and contrast-safe styling adapts automatically.",
        ),
        (
            "Can I hide winter desserts during summer?",
            "Yes. Uncheck category visibility to keep seasonal items saved in draft without deleting them.",
        ),
        (
            "Is it easy for families to browse on any phone?",
            "Yes. Large legible typography and clean cards work smoothly on any screen size.",
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
    name: "Bakeries",
    short_desc: "Breakfast combos, sourdough bakes, pastry slices, and bake-schedule notes.",
    meta_title: "QR Code Digital Menu for Bakeries & Patisseries | QR Cardápio",
    meta_description: "Create a QR Code digital menu for your bakery: breakfast plates, artisan loaves, cakes, and real-time counter availability.",
    h1: "QR Code digital menu for bakeries and patisseries",
    answer_summary: "Bakeries blend seated breakfast service, savory counter snacks, and patisserie cakes in one space. A QR Code digital menu organizes every time of day cleanly so seated guests browse breakfast plates and cakes immediately without sharing dog-eared binders.",
    pain_heading: "Organized from morning rush to afternoon tea",
    pain_points: [
        (
            "Clear sections by time of day",
            "Separate Breakfast Plates, Artisan Loaves, Cakes, and Hot Drinks cleanly.",
        ),
        (
            "Live slice and loaf availability",
            "Mark specialty loaves or tarts sold out before customers walk to the counter.",
        ),
        (
            "Build neighborhood Google Maps trust",
            "Turn regular morning guests into helpful Google Maps reviewers.",
        ),
    ],
    blueprint_title: "Recommended bakery menu structure",
    blueprint_categories: [
        (
            "Breakfast & Griddle",
            &[
                "Toasted Butter Brioche — R$ 10.50",
                "Complete Morning Combo — R$ 34.00",
                "Herb Cheese Omelet — R$ 22.00",
            ],
        ),
        (
            "Artisan Breads",
            &[
                "Country Sourdough Loaf — R$ 24.00",
                "Rustic Baguette — R$ 14.00",
                "Ham & Cheese Puff — R$ 13.00",
            ],
        ),
        (
            "Patisserie",
            &[
                "Dutch Chocolate Tart Slice — R$ 19.00",
                "Vanilla Cream Brioche — R$ 11.00",
                "Almond Croissant — R$ 15.00",
            ],
        ),
    ],
    tips_heading: "Bakery best practices",
    operational_tips: [
        "Mention daily bake times inside artisan bread descriptions.",
        "Keep QR cards on breakfast tables and near the custom-cake pickup counter.",
        "Pair tart and cake listings with crisp square photos to lift impulse orders.",
    ],
    faqs: [
        (
            "Can I use the same link on tables and window signs?",
            "Yes. Both the permanent /c/slug URL and printed QR code stay valid across updates.",
        ),
        (
            "Can I import a printed bakery price list?",
            "Yes. Upload a PDF or photo or paste plain text inside Menu Import.",
        ),
        (
            "Can I hide holiday items like panettone off-season?",
            "Yes. Toggle visibility off while keeping prices and photos stored for next season.",
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
    name: "Bars & Pubs",
    short_desc: "Craft taps, signature cocktails, shareable platters, and low-light mobile clarity.",
    meta_title: "QR Code Digital Menu for Bars & Pubs | QR Cardápio",
    meta_description: "Build a crisp QR Code digital menu for your bar or pub: cocktails, rotating craft beer taps, bar bites, and one-tap keg sold-out updates.",
    h1: "QR Code digital menu for bars and pubs",
    answer_summary: "In dimly lit bars and packed pubs, guests struggle to read dark paper menus while waiting for servers. A QR Code digital menu opens brightly on any phone screen, highlights craft cocktails and sharing platters, and lets staff mark kicked beer kegs unavailable in two seconds.",
    pain_heading: "Built for fast nightlife service",
    pain_points: [
        (
            "Keg kicked? Mark it in two seconds",
            "Flag rotating draft taps unavailable right from the bar phone before tables order them.",
        ),
        (
            "Readable cocktail tasting notes",
            "Showcase spirits, citrus profiles, and ABV clearly without cramped columns.",
        ),
        (
            "High-contrast table placards",
            "Print durable black-on-white QR codes that scan fast even under dim pub lighting.",
        ),
    ],
    blueprint_title: "Recommended bar & pub menu structure",
    blueprint_categories: [
        (
            "Draft Beer & Buckets",
            &[
                "Crisp Pilsner 473 ml — R$ 16.00",
                "American IPA 473 ml — R$ 24.00",
                "5-Bottle Ice Bucket — R$ 55.00",
            ],
        ),
        (
            "Signature Cocktails",
            &[
                "Citrus Gin & Tonic — R$ 34.00",
                "House Moscow Mule — R$ 36.00",
                "Classic Lime Caipirinha — R$ 26.00",
            ],
        ),
        (
            "Bar Bites & Platters",
            &[
                "Crispy Garlic Chicken — R$ 48.00",
                "Cheese & Pepper Cubes — R$ 34.00",
                "Charcuterie Board — R$ 68.00",
            ],
        ),
    ],
    tips_heading: "Bar & pub best practices",
    operational_tips: [
        "Maintain generous white quiet zones around printed QR codes on dark tabletops.",
        "Keep shareable platters easy to spot near the top navigation pills.",
        "Add your Google Maps review prompt in the menu footer to draw weekend groups.",
    ],
    faqs: [
        (
            "Do QR codes scan reliably in dim pub lighting?",
            "Yes, when printed at 3×3 cm or larger with crisp matte black-on-white contrast.",
        ),
        (
            "Can we rotate craft beer taps every week?",
            "Yes. Edit tap names and prices in the dashboard and republish onto the exact same QR code.",
        ),
        (
            "Do guests need to install an app?",
            "No. The menu opens directly in Safari or Chrome on any phone.",
        ),
    ],
    related: [
        SegmentId::BurgerShop,
        SegmentId::Pizzeria,
        SegmentId::Restaurant,
    ],
};
