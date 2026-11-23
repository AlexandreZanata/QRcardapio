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
    title: "Como criar um cardápio digital com QR Code em 5 minutos",
    short_desc: "Passo a passo prático do cadastro à impressão do QR Code permanente.",
    meta_title: "Como Criar Cardápio Digital com QR Code em 5 Minutos | QR Cardápio",
    meta_description: "Aprenda como criar seu cardápio digital com QR Code permanente: prepare categorias, importe seu PDF via OCR, teste no celular e imprima para as mesas.",
    read_minutes: 4,
    published_date: "2026-11-23",
    answer_summary: "Para criar um cardápio digital com QR Code que nunca precisa ser reimpresso, cadastre seu estabelecimento com um endereço fixo, organize categorias curtas com preços em reais e fotos 1:1 (ou importe seu PDF atual), publique em página estática rápida e baixe o QR Code vetorial SVG para as mesas.",
    sections: &[
        GuideSection {
            heading: "1. Escolha um endereço curto e fácil de lembrar",
            paragraphs: &[
                "O endereço do seu cardápio deve representar o nome pelo qual seus clientes conhecem a casa — por exemplo, qrcardapio.com.br/c/cafe-da-praca. Endereços curtos funcionam melhor tanto no QR Code impresso quanto quando compartilhados no Instagram ou WhatsApp.",
                "Evite mudar o endereço depois de colocar o material nas mesas: manter um link permanente protege seu investimento em displays e adesivos.",
            ],
            bullets: &[
                "Prefira o nome direto da marca sem palavras desnecessárias.",
                "Defina a cor principal e envie a logo logo no início para manter identidade consistente.",
            ],
        },
        GuideSection {
            heading: "2. Importe seu cardápio atual ou comece pelas categorias principais",
            paragraphs: &[
                "Não é preciso digitar tudo do zero. Se você já tem um PDF do Canva ou uma foto nítida da carta impressa, o importador OCR reconhece nomes e preços automaticamente para revisão em rascunho.",
                "Organize em quatro ou cinco categorias objetivas (Entradas, Principais, Porções, Bebidas, Sobremesas) para evitar rolagem excessiva.",
            ],
            bullets: &[
                "Inclua fotos reais nos itens mais rentáveis ou mais pedidos.",
                "Escreva descrições de uma a duas linhas destacando ingredientes-chave e peso da porção.",
            ],
        },
        GuideSection {
            heading: "3. Imprima o QR Code com contraste e borda de segurança",
            paragraphs: &[
                "Após revisar na prévia e publicar, baixe o arquivo SVG para gráfica ou PNG de 1024 px para impressão local. A leitura rápida pela câmera depende de contraste alto (preto no branco) e respeito à borda branca externa (quiet zone).",
            ],
            bullets: &[
                "Tamanho mínimo nas mesas: 3×3 cm sem dobras sobre o código.",
                "Prefira laminação fosca ou acrílico antirreflexo sob luminárias fortes.",
            ],
        },
    ],
    comparison_table: None,
    faqs: [
        (
            "Preciso de cartão ou aplicativo especial para começar?",
            "Você cria sua conta, monta o rascunho e visualiza a prévia do cardápio direto pelo navegador do celular ou computador.",
        ),
        (
            "Posso editar preços depois de imprimir o QR Code?",
            "Sim. Após publicado, qualquer alteração de preço, foto ou disponibilidade atualiza o mesmo QR Code.",
        ),
        (
            "Quantos idiomas consigo publicar no mesmo link?",
            "Até três idiomas (Português, Inglês e Espanhol) com troca instantânea sem recarregar a página.",
        ),
    ],
    related_segment: SegmentId::Restaurant,
    related_tool: ToolId::QrGenerator,
};

const PDF_VS_DIGITAL: GuideContent = GuideContent {
    id: GuideId::PdfVsDigital,
    title: "Cardápio em PDF vs. Cardápio Digital: qual converte mais nas mesas?",
    short_desc: "Comparativo de velocidade no 4G, leitura no celular, SEO e custos de atualização.",
    meta_title: "Cardápio em PDF vs. Cardápio Digital: Comparativo Completo | QR Cardápio",
    meta_description: "Compare cardápio em PDF e cardápio digital interativo no celular: tempo de carga no 4G, leitura sem zoom, itens esgotados, tradução e posicionamento no Google.",
    read_minutes: 5,
    published_date: "2026-11-23",
    answer_summary: "Um cardápio digital em HTML responsivo converte mais nas mesas do que um PDF porque abre em menos de 1 segundo no 4G sem download forçado, adapta fontes e fotos à tela do celular, oferece filtro por categoria e busca, e permite atualizar preços ou marcar itens esgotados sem trocar o arquivo.",
    sections: &[
        GuideSection {
            heading: "O problema escondido do PDF no celular",
            paragraphs: &[
                "Arquivos PDF criados para impressão em A4 ou A3 carregam fontes pesadas e imagens não otimizadas — muitas vezes passando de 8 a 20 MB. Em conexões 4G dentro do salão, o cliente espera vários segundos ou precisa autorizar download.",
                "Depois de aberto, o visitante precisa dar zoom com os dedos e arrastar horizontalmente para enxergar preços minúsculos, o que reduz o consumo de entradas, drinks e sobremesas.",
            ],
            bullets: &[
                "PDFs exigem pinça de zoom em telas de 5 a 6 polegadas.",
                "Qualquer alteração de R$ 2,00 obriga exportar e reenviar um novo arquivo.",
            ],
        },
        GuideSection {
            heading: "Por que páginas estáticas HTML dominam velocidade e Google",
            paragraphs: &[
                "Cada cardápio publicado no QR Cardápio vira uma página HTML + CSS + WebP otimizada, servida direto do disco com zero consultas ao banco de dados. Além de instantânea no celular, a página entrega dados estruturados Schema.org para buscadores.",
            ],
            bullets: &[
                "Busca em tempo real por nome de prato ou ingrediente.",
                "Troca de idioma PT/EN/ES instantânea para turistas sem recarregar.",
            ],
        },
    ],
    comparison_table: Some(ComparisonTable {
        title: "Comparativo direto: PDF estático vs. QR Cardápio digital",
        headers: ["Critério", "Cardápio em PDF", "QR Cardápio Digital"],
        rows: &[
            [
                "Peso médio no celular",
                "5 MB a 25 MB (lento no 4G)",
                "~40 KB + fotos WebP otimizadas",
            ],
            [
                "Leitura em tela pequena",
                "Exige zoom e rolagem lateral",
                "Layout responsivo em 1 coluna",
            ],
            [
                "Alteração de preços",
                "Reexportar PDF e subir novo link",
                "Editar no painel e publicar em 1 clique",
            ],
            [
                "Itens esgotados na hora",
                "Exige riscar ou avisar na mesa",
                "Selo 'Indisponível no momento' instantâneo",
            ],
            [
                "Tradução PT / EN / ES",
                "3 PDFs separados e 3 QRs diferentes",
                "Botão único sem recarregar a página",
            ],
            [
                "Avaliações no Google Maps",
                "Link perdido dentro de imagem",
                "Botão nativo com convite no rodapé",
            ],
        ],
    }),
    faqs: [
        (
            "Já tenho meu cardápio em PDF — perdi o trabalho feito nele?",
            "Não. Envie o PDF atual no módulo Importar cardápio e nossa leitura OCR extrai categorias, produtos e preços direto para o seu rascunho.",
        ),
        (
            "Um cardápio digital gasta muitos dados móveis do cliente?",
            "Pelo contrário: uma página estática consome uma fração mínima de kilobytes em comparação ao download de um PDF gráfico.",
        ),
        (
            "O Google consegue indexar os pratos do cardápio digital?",
            "Sim. O cardápio publicado traz metatags e JSON-LD Schema.org/Menu legíveis por mecanismos de busca.",
        ),
    ],
    related_segment: SegmentId::Pizzeria,
    related_tool: ToolId::TablePlacard,
};

const GOOGLE_MAPS_REVIEWS: GuideContent = GuideContent {
    id: GuideId::GoogleMapsReviews,
    title: "Como conseguir mais avaliações no Google Maps pelo cardápio",
    short_desc: "Estratégia prática para transformar visitas no salão em prova social local.",
    meta_title: "Como Conseguir Mais Avaliações no Google Maps pelo Cardápio | QR Cardápio",
    meta_description: "Aprenda a obter seu link oficial de avaliação do Google e usar o cardápio digital e placas de mesa para multiplicar avaliações 5 estrelas no Google Maps.",
    read_minutes: 4,
    published_date: "2026-11-23",
    answer_summary: "Para conseguir mais avaliações reais no Google Maps sem parecer invasivo, cole o link oficial 'Pedir avaliações' do Perfil da Empresa nas configurações do seu cardápio digital e posicione o convite ao fim da jornada de leitura ou na entrega da conta.",
    sections: &[
        GuideSection {
            heading: "Por que avaliações recentes dominam o mapa local",
            paragraphs: &[
                "O algoritmo de busca local do Google avalia três pilares: relevância, distância e destaque. Entre negócios na mesma região, frequência de novas avaliações com texto e nota média alta determinam quem aparece no top 3 do mapa.",
                "A maioria dos clientes satisfeitos não avalia apenas porque precisa procurar o restaurante no Maps manualmente depois que sai.",
            ],
            bullets: &[
                "Remover cliques entre o desejo de elogiar e o formulário de estrelas multiplica a conversão.",
                "Avaliações mensais constantes sinalizam que o estabelecimento está ativo e bem atendido.",
            ],
        },
        GuideSection {
            heading: "Como configurar o link oficial em dois passos",
            paragraphs: &[
                "No Perfil da Empresa no Google, acesse 'Pedir avaliações' para copiar seu link curto oficial (ou copie o link de compartilhamento da ficha no Google Maps). Em seguida, cole esse endereço em Configurações no QR Cardápio e publique.",
                "O cardápio passa a exibir um cartão de convite limpo no rodapé levando o cliente diretamente à tela de avaliação.",
            ],
            bullets: &[
                "Nunca ofereça brindes em troca de nota 5: peça apenas opinião sincera sobre a experiência.",
                "Combine o rodapé do cardápio com uma placa QR dedicada junto ao caixa.",
            ],
        },
    ],
    comparison_table: None,
    faqs: [
        (
            "Qual a diferença entre link do perfil e link direto de avaliação?",
            "O link direto abre imediatamente a caixa de estrelas para o cliente logado, enquanto o link do perfil abre a ficha completa do estabelecimento.",
        ),
        (
            "Posso gerar uma placa separada só para avaliações?",
            "Sim! Use nosso Criador de Placa QR Code gratuito colando o link oficial de avaliações.",
        ),
        (
            "Responder avaliações também ajuda no posicionamento?",
            "Sim. Responder educadamente com palavras naturais sobre o bairro e os pratos reforça sinais locais positivos.",
        ),
    ],
    related_segment: SegmentId::BurgerShop,
    related_tool: ToolId::TablePlacard,
};

const MENU_ENGINEERING_QR: GuideContent = GuideContent {
    id: GuideId::MenuEngineeringQr,
    title: "Engenharia de cardápio e tamanho ideal de QR Code para impressão",
    short_desc: "Como posicionar pratos lucrativos, fotos 1:1 e imprimir QRs que leem de primeira.",
    meta_title: "Engenharia de Cardápio e Impressão de QR Code para Mesas | QR Cardápio",
    meta_description: "Combine engenharia de cardápio (CMV e margem) com regras técnicas de impressão de QR Code (quiet zone, tamanho mínimo e material) para vender mais.",
    read_minutes: 5,
    published_date: "2026-11-23",
    answer_summary: "A engenharia de cardápio no ambiente digital combina ordenação estratégica das categorias mais rentáveis no topo, fotos reais em pratos 'estrela' e cálculo rigoroso de CMV, aliados a QR Codes impressos com pelo menos 3×3 cm, borda branca intacta e acabamento fosco.",
    sections: &[
        GuideSection {
            heading: "Destaque pratos estrela com foto e descrição sensorial",
            paragraphs: &[
                "Classifique seu cardápio cruzando popularidade e margem de contribuição (Lucro = Preço - CMV). Itens de alta margem e alta saída devem abrir suas categorias, acompanhados de fotos nítidas.",
                "Descrições que mencionam preparo ('assado lentamente', 'molho artesanal da casa') aumentam a percepção de valor e justificam preços mais saudáveis.",
            ],
            bullets: &[
                "Evite listar 30 itens na mesma categoria sem subdivisão.",
                "Revise o CMV trimestralmente quando proteínas ou laticínios subirem.",
            ],
        },
        GuideSection {
            heading: "Checklist físico de impressão do QR Code",
            paragraphs: &[
                "Muitos clientes desistem de ler um QR quando ele é pequeno demais, fica dentro de plástico brilhante que reflete luminárias ou teve a margem branca recortada na gráfica.",
            ],
            bullets: &[
                "Dimensão mínima do código: 3×3 cm para mesas e 5×5 cm para balcão/parede.",
                "Mantenha contraste máximo (preto #161721 sobre fundo branco ou creme claro).",
                "Teste com pelo menos um iPhone e um Android antes de laminar todo o lote.",
            ],
        },
    ],
    comparison_table: None,
    faqs: [
        (
            "Posso imprimir o QR Code colorido para combinar com a parede?",
            "Você pode usar moldura colorida ao redor, mas mantenha os módulos do QR Code bem escuros sobre fundo claro para garantir leitura rápida.",
        ),
        (
            "Onde posicionar itens com melhor margem?",
            "Logo no topo das primeiras categorias visíveis e com fotografia de produto em destaque.",
        ),
        (
            "Como saber se meu preço atual está cobrindo o custo?",
            "Use nossa Calculadora de CMV gratuita para conferir margem líquida descontando embalagem e taxas.",
        ),
    ],
    related_segment: SegmentId::CoffeeShop,
    related_tool: ToolId::CmvCalculator,
};
