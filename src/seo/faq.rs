#[derive(Debug, Clone, Copy)]
pub struct FaqItem {
    pub q_key: &'static str,
    pub a_key: &'static str,
    pub art: &'static str,
}

pub const LANDING_FAQS: &[FaqItem] = &[
    FaqItem {
        q_key: "landing.faq1_q",
        a_key: "landing.faq1_a",
        art: "scan-menu",
    },
    FaqItem {
        q_key: "landing.faq2_q",
        a_key: "landing.faq2_a",
        art: "browser-menu",
    },
    FaqItem {
        q_key: "landing.faq3_q",
        a_key: "landing.faq3_a",
        art: "update-qr",
    },
    FaqItem {
        q_key: "landing.faq4_q",
        a_key: "landing.faq4_a",
        art: "import-menu",
    },
    FaqItem {
        q_key: "landing.faq5_q",
        a_key: "landing.faq5_a",
        art: "languages",
    },
    FaqItem {
        q_key: "landing.faq6_q",
        a_key: "landing.faq6_a",
        art: "maps-review",
    },
    FaqItem {
        q_key: "landing.faq7_q",
        a_key: "landing.faq7_a",
        art: "subscription",
    },
    FaqItem {
        q_key: "landing.faq8_q",
        a_key: "landing.faq8_a",
        art: "print-size",
    },
];
