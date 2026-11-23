use crate::i18n::Locale;

use super::guides::GuideId;
use super::segments::SegmentId;
use super::tools::ToolId;
use super::{HreflangLink, absolute_url};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteCluster {
    Home,
    SolutionsHub,
    Segment(SegmentId),
    ToolsHub,
    Tool(ToolId),
    GuidesHub,
    Guide(GuideId),
}

impl RouteCluster {
    pub fn paths(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Home => ("/", "/en", "/es"),
            Self::SolutionsHub => ("/solucoes", "/en/solutions", "/es/soluciones"),
            Self::Segment(id) => (
                id.full_path(Locale::PtBr),
                id.full_path(Locale::En),
                id.full_path(Locale::Es),
            ),
            Self::ToolsHub => ("/ferramentas", "/en/tools", "/es/herramientas"),
            Self::Tool(id) => (
                id.full_path(Locale::PtBr),
                id.full_path(Locale::En),
                id.full_path(Locale::Es),
            ),
            Self::GuidesHub => ("/guias", "/en/guides", "/es/guias"),
            Self::Guide(id) => (
                id.full_path(Locale::PtBr),
                id.full_path(Locale::En),
                id.full_path(Locale::Es),
            ),
        }
    }

    pub fn path_for(self, locale: Locale) -> &'static str {
        let (pt, en, es) = self.paths();
        match locale {
            Locale::PtBr => pt,
            Locale::En => en,
            Locale::Es => es,
        }
    }

    pub fn alternates(self, base: &str) -> Vec<HreflangLink> {
        let (pt, en, es) = self.paths();
        let pt_url = absolute_url(base, pt);
        vec![
            HreflangLink {
                hreflang: "pt-BR",
                href: pt_url.clone(),
            },
            HreflangLink {
                hreflang: "en",
                href: absolute_url(base, en),
            },
            HreflangLink {
                hreflang: "es",
                href: absolute_url(base, es),
            },
            HreflangLink {
                hreflang: "x-default",
                href: pt_url,
            },
        ]
    }

    pub fn all_sitemap_clusters() -> Vec<(RouteCluster, &'static str, &'static str)> {
        let mut out = vec![
            (Self::Home, "weekly", "1.0"),
            (Self::SolutionsHub, "weekly", "0.9"),
            (Self::ToolsHub, "weekly", "0.9"),
            (Self::GuidesHub, "weekly", "0.9"),
        ];
        for id in SegmentId::ALL {
            out.push((Self::Segment(id), "monthly", "0.85"));
        }
        for id in ToolId::ALL {
            out.push((Self::Tool(id), "monthly", "0.85"));
        }
        for id in GuideId::ALL {
            out.push((Self::Guide(id), "monthly", "0.8"));
        }
        out
    }
}
