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
    short_desc: "Pratos executivos, carta de bebidas e atendimento trilíngue nas mesas.",
    meta_title: "Cardápio Digital com QR Code para Restaurante | QR Cardápio",
    meta_description: "Crie o cardápio digital do seu restaurante com QR Code permanente, atualização instantânea de pratos do dia, fotos nítidas e versão em PT, EN e ES.",
    h1: "Cardápio digital com QR Code para restaurantes",
    answer_summary: "Um cardápio digital com QR Code para restaurante permite que clientes leiam pratos, descrições e preços direto no navegador do celular, sem baixar aplicativo nem abrir PDF pesado. O dono atualiza pratos executivos, itens esgotados e valores no painel mantendo sempre o mesmo código impresso nas mesas.",
    pain_heading: "Como resolver a rotina do salão",
    pain_points: [
        (
            "Prato do dia sem reimpressão",
            "Altere valores ou o executivo da semana no painel e publique em segundos sem trocar displays das mesas.",
        ),
        (
            "Pratos em falta marcados na hora",
            "Sinalize um corte ou sobremesa esgotada com um toque para evitar retrabalho do garçom.",
        ),
        (
            "Turistas atendidos sem barreira",
            "Ofereça descrições em português, inglês e espanhol no mesmo QR Code com troca imediata.",
        ),
    ],
    blueprint_title: "Estrutura sugerida de cardápio para restaurante",
    blueprint_categories: [
        (
            "Entradas e Porções",
            &[
                "Bruschetta da casa — R$ 28,00",
                "Bolinho de costela (6 un.) — R$ 36,00",
                "Salada de folhas e burrata — R$ 44,00",
            ],
        ),
        (
            "Pratos Principais",
            &[
                "Filé ao molho madeira — R$ 68,00",
                "Salmão grelhado com legumes — R$ 74,00",
                "Risoto de cogumelos frescos — R$ 58,00",
            ],
        ),
        (
            "Bebidas e Sobremesas",
            &[
                "Suco natural 400 ml — R$ 14,00",
                "Taça de vinho da casa — R$ 32,00",
                "Pudim de leite condensado — R$ 22,00",
            ],
        ),
    ],
    tips_heading: "Boas práticas na mesa",
    operational_tips: [
        "Posicione o display de QR Code de pelo menos 3×3 cm próximo ao saleiro ou centro da mesa.",
        "Destaque os três pratos de maior margem com foto real 1:1 para acelerar a escolha do cliente.",
        "Conecte o link do Google Maps para convidar clientes satisfeitos a avaliar ao fim da refeição.",
    ],
    faqs: [
        (
            "Preciso imprimir novos QR Codes quando mudar os preços do restaurante?",
            "Não. O endereço /c/seu-restaurante é permanente após a publicação; você publica novos preços e fotos mantendo exatamente o mesmo código nas mesas.",
        ),
        (
            "Como funciona para restaurantes em áreas turísticas?",
            "Cada categoria e prato aceita nome e descrição opcionais em inglês e espanhol, permitindo troca instantânea no celular do visitante.",
        ),
        (
            "Posso importar meu cardápio atual em PDF?",
            "Sim. O importador OCR lê PDFs digitais, fotos ou texto colado para pré-cadastrar categorias e preços no rascunho.",
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
    name: "Hamburguerias",
    short_desc: "Fotos de capa irresistíveis, combos, artesanais e convite no Google Maps.",
    meta_title: "Cardápio Digital com QR Code para Hamburgueria | QR Cardápio",
    meta_description: "Monte o cardápio digital da sua hamburgueria com fotos apetitosas de burgers, combos e porções, QR Code permanente nas mesas e mais avaliações no Google Maps.",
    h1: "Cardápio digital com QR Code para hamburguerias",
    answer_summary: "Para hamburguerias artesanais e smash burgers, o cardápio digital em QR Code exibe fotos em alta definição dos lanches, organiza combos e bebidas por categorias rápidas e ajuda a transformar cada cliente do salão em uma nova avaliação positiva no Google Maps.",
    pain_heading: "O que muda na sua hamburgueria",
    pain_points: [
        (
            "Fotos que vendem o blend e o ponto",
            "Sua primeira foto de produto vira capa automática do cardápio mobile, abrindo o apetite logo no primeiro toque.",
        ),
        (
            "Burger do mês e edições limitadas",
            "Teste receitas sazonais em rascunho e publique na sexta-feira sem gastar com gráfica.",
        ),
        (
            "Mais estrelas no Google Maps da região",
            "Ative o convite de avaliação direto no rodapé do cardápio e supere concorrentes na busca local.",
        ),
    ],
    blueprint_title: "Estrutura sugerida de cardápio para hamburgueria",
    blueprint_categories: [
        (
            "Burgers Artesanais",
            &[
                "Smash Duplo Cheddar — R$ 34,90",
                "Bacon Defumado 180g — R$ 41,90",
                "Veggie Grão-de-Bico — R$ 36,00",
            ],
        ),
        (
            "Fritas e Acompanhamentos",
            &[
                "Batata rústica com páprica — R$ 22,00",
                "Onion rings crocantes — R$ 26,00",
                "Nuggets artesanais — R$ 28,00",
            ],
        ),
        (
            "Shakes e Bebidas",
            &[
                "Milkshake de Nutella 400 ml — R$ 27,00",
                "Refrigerante lata — R$ 8,50",
                "Chopp IPA 500 ml — R$ 24,00",
            ],
        ),
    ],
    tips_heading: "Boas práticas na hamburgueria",
    operational_tips: [
        "Coloque o banner de bebidas logo após os lanches principais para estimular o combo.",
        "Use descrições curtas listando gramatura do burger, queijo e molho especial.",
        "Coloque o QR Code na bandeja de papel ou em prisma de acrílico resistente a gordura.",
    ],
    faqs: [
        (
            "O cardápio abre rápido no 4G/5G sem Wi-Fi?",
            "Sim. Após publicado, o cardápio vira uma página estática ultraleve com zero consultas ao banco de dados.",
        ),
        (
            "Consigo colocar a cor e a logo da minha hamburgueria?",
            "Sim. O painel aplica sua logo e calcula automaticamente o contraste acessível para botões e categorias.",
        ),
        (
            "Posso usar o link na bio do Instagram além do QR da mesa?",
            "Sim. O mesmo link permanente funciona no QR Code impresso, no WhatsApp e na bio do Instagram.",
        ),
    ],
    related: [SegmentId::SnackBar, SegmentId::BarPub, SegmentId::Pizzeria],
};

const COFFEE_SHOP: SegmentContent = SegmentContent {
    id: SegmentId::CoffeeShop,
    name: "Cafeterias",
    short_desc: "Métodos filtrados, doces da vitrine e giro ágil no balcão e mesas.",
    meta_title: "Cardápio Digital com QR Code para Cafeteria | QR Cardápio",
    meta_description: "Cardápio digital elegante com QR Code permanente para cafeterias: cafés especiais, doces da vitrine, bebidas geladas e controle rápido de itens esgotados.",
    h1: "Cardápio digital com QR Code para cafeterias",
    answer_summary: "Em cafeterias e bistrôs, o cardápio digital via QR Code organiza cafés especiais, métodos coados e doces da vitrine com visual limpo e leitura instantânea. Quando o último croissant ou bolo acaba à tarde, basta marcar como esgotado no celular sem remover o item.",
    pain_heading: "Agilidade do balcão ao salão",
    pain_points: [
        (
            "Controle instantâneo da vitrine",
            "Marque fatias e salgados esgotados em tempo real para o cliente não pedir o que acabou.",
        ),
        (
            "Notas sensoriais e variações claras",
            "Explique origem do grão, preparo (V60, prensa, espresso) e opções com leite vegetal.",
        ),
        (
            "Identidade visual acolhedora",
            "Personalize a cor principal do cardápio combinando com o tom da sua cafeteria.",
        ),
    ],
    blueprint_title: "Estrutura sugerida de cardápio para cafeteria",
    blueprint_categories: [
        (
            "Cafés e Métodos",
            &[
                "Espresso duplo — R$ 9,50",
                "Cappuccino cremoso 200 ml — R$ 15,00",
                "Coado Hario V60 — R$ 18,00",
            ],
        ),
        (
            "Vitrine Salgada",
            &[
                "Croissant de manteiga francesa — R$ 16,00",
                "Pão de queijo artesanal (3 un.) — R$ 14,00",
                "Toast de avocado e ovo — R$ 29,00",
            ],
        ),
        (
            "Doces e Gelados",
            &[
                "Bolo de cenoura com ganache — R$ 19,00",
                "Iced Latte baunilha — R$ 18,50",
                "Affogato com sorvete — R$ 21,00",
            ],
        ),
    ],
    tips_heading: "Boas práticas na cafeteria",
    operational_tips: [
        "Crie uma categoria separada para bebidas geladas e sazonais nos meses quentes.",
        "Deixe um adesivo de QR Code discreto no canto esquerdo de cada mesa pequena.",
        "Incentive avaliações no Google Maps de clientes que trabalham ou tomam café no local.",
    ],
    faqs: [
        (
            "O cardápio funciona bem para clientes estrangeiros?",
            "Sim. Você pode traduzir descrições de métodos e grãos para inglês e espanhol no próprio painel.",
        ),
        (
            "Se um doce acabar no meio da tarde, preciso tirar do cardápio?",
            "Não. Use o botão de disponibilidade para exibir o selo 'Indisponível no momento' mantendo a presença no cardápio.",
        ),
        (
            "Quanto tempo leva para colocar no ar?",
            "Menos de 5 minutos com criação assistida ou importando seu PDF ou foto atual via OCR.",
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
    name: "Lanchonetes",
    short_desc: "Lanches prensados, sucos, combos econômicos e atualização pelo celular.",
    meta_title: "Cardápio Digital com QR Code para Lanchonete | QR Cardápio",
    meta_description: "Digitalize o cardápio da sua lanchonete com QR Code permanente: fácil de atualizar pelo celular, busca rápida por lanche e zero gasto com reimpressão.",
    h1: "Cardápio digital com QR Code para lanchonetes",
    answer_summary: "Uma lanchonete precisa de velocidade: o cliente escaneia o QR Code no balcão ou na mesa e encontra lanches, salgados, pastéis e sucos com busca instantânea por palavra-chave. O proprietário ajusta preços pelo celular em segundos sempre que o custo dos insumos mudar.",
    pain_heading: "Feito para alto giro diário",
    pain_points: [
        (
            "Busca rápida no cardápio extenso",
            "Clientes digitam 'x-bacon', 'pastel' ou 'suco' e filtram itens na hora, sem folhear páginas.",
        ),
        (
            "Adeus etiquetas coladas sobre preços",
            "Mantenha o balcão e as mesas sempre limpos e profissionais quando atualizar valores.",
        ),
        (
            "Cadastro sem complicação",
            "Tire foto do cardápio atual ou cole a lista de preços para importar tudo rápido.",
        ),
    ],
    blueprint_title: "Estrutura sugerida de cardápio para lanchonete",
    blueprint_categories: [
        (
            "Lanches na Chapa",
            &[
                "X-Salada tradicional — R$ 22,00",
                "X-Tudo completo — R$ 31,00",
                "Misto quente na chapa — R$ 13,00",
            ],
        ),
        (
            "Salgados e Pastéis",
            &[
                "Pastel de carne com ovo — R$ 14,00",
                "Coxinha de frango com catupiry — R$ 10,00",
                "Kibe recheado — R$ 10,00",
            ],
        ),
        (
            "Sucos e Vitaminas",
            &[
                "Suco de laranja 500 ml — R$ 12,00",
                "Vitamina de morango com aveia — R$ 15,00",
                "Refrigerante 600 ml — R$ 9,00",
            ],
        ),
    ],
    tips_heading: "Boas práticas na lanchonete",
    operational_tips: [
        "Fixe uma placa maior de QR Code na parede próxima ao caixa para filas rápidas.",
        "Agrupe combos promocionais na primeira categoria visível.",
        "Use a calculadora de CMV gratuita para revisar sua margem antes de publicar novos preços.",
    ],
    faqs: [
        (
            "Consigo editar o cardápio direto do meu celular?",
            "Sim. Todo o painel administrativo é responsivo e funciona perfeitamente em smartphones.",
        ),
        (
            "Existe comissão por cliente que acessa o QR Code?",
            "Não. O QR Cardápio cobra apenas assinatura fixa mensal quando habilitada, com acessos ilimitados.",
        ),
        (
            "Meus clientes mais simples conseguem abrir fácil?",
            "Sim. Basta apontar a câmera nativa de qualquer Android ou iPhone para o QR Code — abre direto no navegador.",
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
    name: "Pizzarias",
    short_desc: "Sabores tradicionais e especiais, bordas, calzones e carta de bebidas.",
    meta_title: "Cardápio Digital com QR Code para Pizzaria | QR Cardápio",
    meta_description: "Crie o cardápio digital da sua pizzaria com QR Code permanente: organize pizzas salgadas, doces, bordas e bebidas sem depender de PDF ilegível.",
    h1: "Cardápio digital com QR Code para pizzarias",
    answer_summary: "Cardápios de pizzaria costumam ter dezenas de sabores e ingredientes que ficam minúsculos em um PDF no celular. Com o cardápio digital em QR Code, o cliente navega por abas de Pizzas Tradicionais, Especiais, Doces e Bebidas, filtra por ingrediente na busca e lê tudo com conforto.",
    pain_heading: "Por que abandonar o PDF na pizzaria",
    pain_points: [
        (
            "Busca por ingrediente em segundos",
            "Quem evita cebola, quer rúcula ou procura quatro queijos filtra na barra de busca instantaneamente.",
        ),
        (
            "Categorias claras por faixa de sabor",
            "Separe Tradicionais, Especiais, Pizzas Doces e Carta de Vinhos em pílulas fixas no topo.",
        ),
        (
            "QR Code nas mesas e nos folhetos",
            "Use o SVG vetorial em displays de mesa e caixas sem nunca precisar trocar o código ao ajustar preços.",
        ),
    ],
    blueprint_title: "Estrutura sugerida de cardápio para pizzaria",
    blueprint_categories: [
        (
            "Pizzas Tradicionais",
            &[
                "Margherita clássica — R$ 58,00",
                "Calabresa com cebola roxa — R$ 56,00",
                "Portuguesa completa — R$ 62,00",
            ],
        ),
        (
            "Pizzas Especiais",
            &[
                "Parma com rúcula e parmesão — R$ 76,00",
                "Brie com geleia de damasco — R$ 74,00",
                "Pepperoni americano — R$ 69,00",
            ],
        ),
        (
            "Pizzas Doces e Bebidas",
            &[
                "Chocolate com morango — R$ 54,00",
                "Banana com canela e doce de leite — R$ 52,00",
                "Refrigerante 2L — R$ 16,00",
            ],
        ),
    ],
    tips_heading: "Boas práticas na pizzaria",
    operational_tips: [
        "Informe na descrição da categoria os tamanhos disponíveis (Média, Grande, Família).",
        "Importe sua lista atual de 30+ sabores em segundos colando o texto no importador OCR.",
        "Peça avaliação no Google Maps enquanto a mesa aguarda a conta ou a pizza doce.",
    ],
    faqs: [
        (
            "Tenho mais de 40 sabores de pizza. É trabalhoso cadastrar?",
            "Não. Use o importador de PDF, foto ou texto colado para transformar sua lista inteira em rascunho de uma vez.",
        ),
        (
            "Posso imprimir o QR Code em gráfica grande?",
            "Sim. Você baixa o QR Code em SVG vetorial infinito além de PNG de alta resolução (1024px+).",
        ),
        (
            "Posso testar mudanças de preço antes da sexta-feira?",
            "Sim. Tudo é salvo em rascunho até você clicar em Publicar atualizações.",
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
    name: "Açaiterias e Sorveterias",
    short_desc: "Tamanhos de copos, cremes, adicionais, picolés e visual colorido vibrante.",
    meta_title: "Cardápio Digital com QR Code para Açaiteria e Sorveteria | QR Cardápio",
    meta_description: "Crie o cardápio digital colorido da sua açaiteria ou sorveteria com QR Code permanente: tamanhos, adicionais, sabores do dia e busca rápida.",
    h1: "Cardápio digital com QR Code para açaiterias e sorveterias",
    answer_summary: "Em açaiterias e sorveterias, sabores sazonais e adicionais mudam toda semana. O cardápio digital em QR Code permite escolher uma cor vibrante da sua marca, organizar tamanhos de copos, cremes e toppings com clareza e desativar sabores que acabaram no freezer com um clique.",
    pain_heading: "Feito para sabores rotativos",
    pain_points: [
        (
            "Sabores do dia sem riscar lousa",
            "Oculte sabores sazonais ou marque como esgotado quando a cuba terminar.",
        ),
        (
            "Personalize na cor da sua loja",
            "Escolha o roxo do açaí ou o tom pastel da sua sorveteria no seletor de cor do painel.",
        ),
        (
            "Menos fila no balcão",
            "Clientes já decidem tamanho, creme e acompanhamentos pelo celular antes de chegar ao caixa.",
        ),
    ],
    blueprint_title: "Estrutura sugerida de cardápio para açaiteria",
    blueprint_categories: [
        (
            "Copos e Tigelas de Açaí",
            &[
                "Açaí Tradicional 300 ml — R$ 18,00",
                "Açaí Tradicional 500 ml — R$ 25,00",
                "Barca de Açaí 1L — R$ 48,00",
            ],
        ),
        (
            "Adicionais e Frutas",
            &[
                "Leite em pó Ninho — R$ 4,00",
                "Creme de avelã — R$ 6,00",
                "Morango, banana e granola — R$ 5,00",
            ],
        ),
        (
            "Sorvetes e Milkshakes",
            &[
                "Casquinha artesanal 2 bolas — R$ 16,00",
                "Milkshake de Cupuaçu — R$ 22,00",
                "Picolé gourmet — R$ 11,00",
            ],
        ),
    ],
    tips_heading: "Boas práticas na açaiteria",
    operational_tips: [
        "Crie uma categoria dedicada para 'Adicionais e Coberturas' para aumentar o ticket médio.",
        "Coloque adesivos com QR Code nas mesas externas e no vidro do balcão.",
        "Ative fotos reais dos copos montados para estimular pedidos de adicionais premium.",
    ],
    faqs: [
        (
            "Consigo colocar a cor roxa oficial da minha açaiteria?",
            "Sim. Basta selecionar qualquer cor hexadecimal no painel e o tema recalcula o contraste automaticamente.",
        ),
        (
            "Dá para esconder uma categoria só fora de temporada?",
            "Sim. Desmarque a visibilidade da categoria (ex.: Fondue de Inverno) sem excluir os produtos.",
        ),
        (
            "O cardápio funciona bem para crianças e famílias?",
            "Sim. O visual com fotos grandes e botões bem espaçados facilita a leitura em qualquer celular.",
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
    name: "Padarias",
    short_desc: "Pães artesanais, café da manhã, confeitaria e encomendas sazonais.",
    meta_title: "Cardápio Digital com QR Code para Padaria e Confeitaria | QR Cardápio",
    meta_description: "Monte o cardápio digital da sua padaria e confeitaria com QR Code permanente: café da manhã, lanches na chapa, bolos e horários de fornada.",
    h1: "Cardápio digital com QR Code para padarias e confeitarias",
    answer_summary: "Padarias reúnem café da manhã, balcão de lanches, confeitaria e pães artesanais no mesmo endereço. Com um cardápio digital via QR Code, clientes sentados nas mesas do café consultam combos, fatias de bolo e sucos sem precisar esperar o cardápio impresso único circular pelo salão.",
    pain_heading: "Organização do café da manhã ao lanche da tarde",
    pain_points: [
        (
            "Tudo separado por momento do dia",
            "Estruture Café da Manhã, Pães Artesanais, Confeitaria e Bebidas em abas claras.",
        ),
        (
            "Bolos e tortas disponíveis na hora",
            "Atualize os sabores disponíveis no balcão sem gerar frustração no pedido.",
        ),
        (
            "Mais avaliações locais no bairro",
            "Clientes satisfeitos no café da manhã deixam avaliações no Google Maps direto do cardápio.",
        ),
    ],
    blueprint_title: "Estrutura sugerida de cardápio para padaria",
    blueprint_categories: [
        (
            "Café da Manhã e Chapa",
            &[
                "Pão na chapa com requeijão — R$ 10,50",
                "Combo Café Completo — R$ 34,00",
                "Omelete com queijo minas — R$ 22,00",
            ],
        ),
        (
            "Pães e Folhados",
            &[
                "Pão italiano fermentação natural — R$ 24,00",
                "Baguete rústica — R$ 14,00",
                "Folhado de presunto e queijo — R$ 13,00",
            ],
        ),
        (
            "Confeitaria",
            &[
                "Fatia de torta holandesa — R$ 19,00",
                "Sonho tradicional de creme — R$ 11,00",
                "Quindim artesanal — R$ 10,00",
            ],
        ),
    ],
    tips_heading: "Boas práticas na padaria",
    operational_tips: [
        "Informe na descrição dos pães especiais os dias ou horários de fornada.",
        "Deixe QR Codes nas mesas da área de café e próximo ao balcão de encomendas.",
        "Inclua fotos nítidas das fatias de torta para impulsionar vendas por impulso.",
    ],
    faqs: [
        (
            "Posso usar o mesmo link no cardápio da mesa e na porta da padaria?",
            "Sim. O link /c/sua-padaria e o QR Code impresso são permanentes e atualizados pelo painel.",
        ),
        (
            "Consigo importar uma tabela de preços já pronta?",
            "Sim. Basta colar o texto ou enviar o PDF/foto pelo módulo Importar cardápio.",
        ),
        (
            "Posso tirar itens sazonais como panetone fora de época sem apagá-los?",
            "Sim. Basta desmarcar a visibilidade de um produto ou categoria inteira e republicar.",
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
    name: "Bares e Pubs",
    short_desc: "Drinks autorais, chopps, porções para compartilhar e leitura no escuro.",
    meta_title: "Cardápio Digital com QR Code para Bar e Pub | QR Cardápio",
    meta_description: "Crie o cardápio digital do seu bar ou pub com QR Code permanente: drinks, cervejas artesanais, porções e atualização rápida quando um barril acabar.",
    h1: "Cardápio digital com QR Code para bares e pubs",
    answer_summary: "Em bares e pubs com iluminação baixa e movimento intenso à noite, o cardápio digital via QR Code garante leitura imediata na tela do celular, evita filas esperando cardápio impresso e permite marcar em segundos quando um rótulo de cerveja artesanal ou barril de chopp acaba.",
    pain_heading: "Controle ágil na noite",
    pain_points: [
        (
            "Barril acabou? Marque em 2 segundos",
            "Sinalize chopps e rótulos esgotados direto do celular do balcão antes do garçom ir à mesa.",
        ),
        (
            "Drinks autorais descritos com clareza",
            "Liste destilado base, notas cítricas ou amargas e harmonizações sem apertar o texto.",
        ),
        (
            "QR Code resistente para mesas externas",
            "Gere placa imprimível ou adesivo de alto contraste que lê rápido mesmo com pouca luz.",
        ),
    ],
    blueprint_title: "Estrutura sugerida de cardápio para bar e pub",
    blueprint_categories: [
        (
            "Chopps e Cervejas",
            &[
                "Chopp Pilsen 473 ml — R$ 16,00",
                "American IPA 473 ml — R$ 24,00",
                "Balde com 5 long necks — R$ 55,00",
            ],
        ),
        (
            "Drinks Autorais",
            &[
                "Gin Tônica cítrico — R$ 34,00",
                "Moscow Mule da casa — R$ 36,00",
                "Caipirinha de limão tahiti — R$ 26,00",
            ],
        ),
        (
            "Porções para Compartilhar",
            &[
                "Frango a passarinho — R$ 48,00",
                "Dadinho de tapioca com geleia — R$ 34,00",
                "Tábua de frios e embutidos — R$ 68,00",
            ],
        ),
    ],
    tips_heading: "Boas práticas no bar e pub",
    operational_tips: [
        "Use QR Codes com borda branca generosa (quiet zone) para leitura rápida sob luz baixa.",
        "Deixe a categoria 'Porções para Compartilhar' fácil de alcançar no topo.",
        "Incentive avaliações no Google Maps no rodapé para atrair novos grupos no fim de semana.",
    ],
    faqs: [
        (
            "O QR Code lê bem em ambiente noturno ou de baixa iluminação?",
            "Sim, desde que impresso com alto contraste preto no branco e pelo menos 3×3 cm sem reflexo brilhante.",
        ),
        (
            "Posso trocar as torneiras de chopp toda semana?",
            "Sim. Edite nomes, IBU, ABV e preços no painel e clique em Publicar atualizações mantendo o mesmo QR.",
        ),
        (
            "Funciona sem o cliente precisar baixar aplicativo?",
            "Sim. O cardápio abre instantaneamente no navegador padrão de qualquer celular.",
        ),
    ],
    related: [
        SegmentId::BurgerShop,
        SegmentId::Pizzeria,
        SegmentId::Restaurant,
    ],
};
