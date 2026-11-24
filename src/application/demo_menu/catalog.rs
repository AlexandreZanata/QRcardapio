type Text = (&'static str, &'static str, &'static str);

pub(super) struct Section {
    pub id: &'static str,
    pub name: Text,
    pub description: Text,
    pub items: &'static [Item],
}

pub(super) struct Item {
    pub id: &'static str,
    pub name: Text,
    pub description: Text,
    pub price_centavos: i64,
    pub image: &'static [u8],
}

pub(super) const DESCRIPTION: Text = (
    "Cardápio demonstrativo de uma hamburgueria fictícia. Hambúrgueres artesanais, porções e bebidas geladas.",
    "Sample menu for a fictional burger shop. Craft burgers, sides and cold drinks.",
    "Menú de demostración de una hamburguesería ficticia. Hamburguesas artesanales, acompañamientos y bebidas frías.",
);

pub(super) const CATEGORIES: &[Section] = &[
    Section {
        id: "hamburgueres",
        name: ("Hambúrgueres", "Burgers", "Hamburguesas"),
        description: (
            "Da chapa, no brioche tostado.",
            "Fresh off the grill on toasted brioche.",
            "Recién hechas a la plancha en brioche tostado.",
        ),
        items: &[
            Item {
                id: "classico",
                name: (
                    "Hambúrguer da casa",
                    "House Burger",
                    "Hamburguesa de la casa",
                ),
                description: (
                    "Blend bovino de 150 g, cheddar, alface, tomate, picles e molho da casa.",
                    "150 g beef patty, cheddar, lettuce, tomato, pickles and house sauce.",
                    "Hamburguesa de res de 150 g, cheddar, lechuga, tomate, pepinillos y salsa de la casa.",
                ),
                price_centavos: 3_290,
                image: include_bytes!("images/house-burger.jpg"),
            },
            Item {
                id: "duplo-bacon",
                name: (
                    "Duplo Smash Bacon",
                    "Double Bacon Smash",
                    "Smash doble con beicon",
                ),
                description: (
                    "Dois smash de 90 g, cheddar derretido, bacon crocante e maionese defumada.",
                    "Two 90 g smash patties, melted cheddar, crispy bacon and smoky mayo.",
                    "Dos hamburguesas smash de 90 g, cheddar fundido, beicon crujiente y mayonesa ahumada.",
                ),
                price_centavos: 4_290,
                image: include_bytes!("images/double-bacon.jpg"),
            },
            Item {
                id: "vegetal",
                name: (
                    "Burger da Horta",
                    "Garden Burger",
                    "Hamburguesa de la huerta",
                ),
                description: (
                    "Hambúrguer de grão-de-bico, repolho roxo, cebola em conserva, alface e maionese de ervas.",
                    "Chickpea patty, red cabbage, pickled onion, lettuce and herb mayo.",
                    "Hamburguesa de garbanzos, col morada, cebolla encurtida, lechuga y mayonesa de hierbas.",
                ),
                price_centavos: 3_490,
                image: include_bytes!("images/vegetal.jpg"),
            },
        ],
    },
    Section {
        id: "combos",
        name: ("Combos", "Combos", "Combos"),
        description: (
            "Lanche, acompanhamento e bebida.",
            "Burger, a side and a drink.",
            "Hamburguesa, acompañamiento y bebida.",
        ),
        items: &[Item {
            id: "combo-praca",
            name: ("Combo da Praça", "Praça Combo", "Combo de la Praça"),
            description: (
                "Hambúrguer da casa, porção de fritas de 120 g e refrigerante de cola de 350 ml.",
                "House Burger, a 120 g serving of fries and 350 ml of cola.",
                "Hamburguesa de la casa, porción de papas fritas de 120 g y refresco de cola de 350 ml.",
            ),
            price_centavos: 4_990,
            image: include_bytes!("images/combo.jpg"),
        }],
    },
    Section {
        id: "porcoes",
        name: ("Porções", "Sides", "Acompañamientos"),
        description: (
            "Crocantes e feitas na hora.",
            "Crisp sides, cooked to order.",
            "Crujientes y preparados al momento.",
        ),
        items: &[
            Item {
                id: "fritas",
                name: ("Batata frita", "French Fries", "Papas fritas"),
                description: (
                    "Porção de 200 g de fritas douradas, com maionese da casa.",
                    "200 g of golden fries with house mayo.",
                    "Porción de 200 g de papas fritas doradas con mayonesa de la casa.",
                ),
                price_centavos: 1_790,
                image: include_bytes!("images/fries.jpg"),
            },
            Item {
                id: "aneis-de-cebola",
                name: ("Anéis de cebola", "Onion Rings", "Aros de cebolla"),
                description: (
                    "Oito anéis empanados e crocantes com maionese de páprica.",
                    "Eight crispy breaded rings with paprika mayo.",
                    "Ocho aros empanados crujientes con mayonesa de pimentón.",
                ),
                price_centavos: 1_990,
                image: include_bytes!("images/onion-rings.jpg"),
            },
        ],
    },
    Section {
        id: "bebidas",
        name: ("Bebidas", "Drinks", "Bebidas"),
        description: (
            "Para acompanhar o lanche.",
            "Pair with a burger.",
            "Para acompañar la hamburguesa.",
        ),
        items: &[
            Item {
                id: "limonada",
                name: ("Limonada fresca", "Fresh Limeade", "Limonada fresca"),
                description: (
                    "Limão-siciliano e limão-taiti espremidos na hora, com gelo. 400 ml.",
                    "Fresh-squeezed lemon and lime over ice. 400 ml.",
                    "Limón y lima recién exprimidos con hielo. 400 ml.",
                ),
                price_centavos: 1_190,
                image: include_bytes!("images/limeade.jpg"),
            },
            Item {
                id: "milkshake-morango",
                name: (
                    "Milkshake de morango",
                    "Strawberry Milkshake",
                    "Batido de fresa",
                ),
                description: (
                    "Sorvete de creme batido com morangos e finalizado com chantilly. 400 ml.",
                    "Vanilla ice cream blended with strawberries and topped with whipped cream. 400 ml.",
                    "Helado de vainilla batido con fresas y coronado con nata montada. 400 ml.",
                ),
                price_centavos: 2_290,
                image: include_bytes!("images/shake.jpg"),
            },
        ],
    },
    Section {
        id: "sobremesas",
        name: ("Sobremesas", "Desserts", "Postres"),
        description: (
            "Para terminar a refeição.",
            "A sweet finish.",
            "Para terminar la comida.",
        ),
        items: &[Item {
            id: "brownie",
            name: (
                "Brownie com sorvete",
                "Brownie with Ice Cream",
                "Brownie con helado",
            ),
            description: (
                "Brownie morno de chocolate, sorvete de baunilha e calda de chocolate.",
                "Warm chocolate brownie, vanilla ice cream and chocolate sauce.",
                "Brownie de chocolate tibio, helado de vainilla y salsa de chocolate.",
            ),
            price_centavos: 2_190,
            image: include_bytes!("images/brownie.jpg"),
        }],
    },
];
