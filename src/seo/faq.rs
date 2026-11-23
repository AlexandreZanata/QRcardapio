#[derive(Debug, Clone, Copy)]
pub struct FaqItem {
    pub q_key: &'static str,
    pub a_key: &'static str,
}

pub const LANDING_FAQS: &[FaqItem] = &[
    FaqItem {
        q_key: "landing.faq1_q",
        a_key: "landing.faq1_a",
    },
    FaqItem {
        q_key: "landing.faq2_q",
        a_key: "landing.faq2_a",
    },
    FaqItem {
        q_key: "landing.faq3_q",
        a_key: "landing.faq3_a",
    },
    FaqItem {
        q_key: "landing.faq4_q",
        a_key: "landing.faq4_a",
    },
    FaqItem {
        q_key: "landing.faq5_q",
        a_key: "landing.faq5_a",
    },
    FaqItem {
        q_key: "landing.faq6_q",
        a_key: "landing.faq6_a",
    },
    FaqItem {
        q_key: "landing.faq7_q",
        a_key: "landing.faq7_a",
    },
    FaqItem {
        q_key: "landing.faq8_q",
        a_key: "landing.faq8_a",
    },
];
