use crate::domain::publication::MenuItemChoice;

const CHEDDAR: MenuItemChoice = extra(("Cheddar extra", "Extra cheddar", "Cheddar extra"), 350);
const BACON: MenuItemChoice = extra(("Bacon crocante", "Crispy bacon", "Beicon crujiente"), 500);
const EXTRA_SAUCE: MenuItemChoice =
    extra(("Molho da casa", "House sauce", "Salsa de la casa"), 250);
const HERB_MAYO: MenuItemChoice = extra(
    ("Maionese de ervas", "Herb mayo", "Mayonesa de hierbas"),
    250,
);

pub(super) fn for_product(id: &str) -> Vec<MenuItemChoice> {
    match id {
        "classico" | "combo-praca" => vec![
            remove(("Picles", "Pickles", "Pepinillos")),
            remove(("Tomate", "Tomato", "Tomate")),
            remove(("Alface", "Lettuce", "Lechuga")),
            CHEDDAR,
            BACON,
        ],
        "duplo-bacon" => vec![
            remove(("Maionese defumada", "Smoky mayo", "Mayonesa ahumada")),
            remove(("Bacon", "Bacon", "Beicon")),
            CHEDDAR,
            BACON,
        ],
        "vegetal" => vec![
            remove(("Cebola em conserva", "Pickled onion", "Cebolla encurtida")),
            remove(("Maionese de ervas", "Herb mayo", "Mayonesa de hierbas")),
            HERB_MAYO,
        ],
        "fritas" | "aneis-de-cebola" => vec![CHEDDAR, EXTRA_SAUCE],
        "milkshake-morango" => vec![remove(("Chantilly", "Whipped cream", "Nata montada"))],
        _ => Vec::new(),
    }
}

const fn remove(name: (&'static str, &'static str, &'static str)) -> MenuItemChoice {
    MenuItemChoice {
        name,
        price_centavos: None,
    }
}

const fn extra(name: (&'static str, &'static str, &'static str), price: i64) -> MenuItemChoice {
    MenuItemChoice {
        name,
        price_centavos: Some(price),
    }
}
