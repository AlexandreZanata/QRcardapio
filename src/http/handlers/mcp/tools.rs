use serde_json::{Value, json};

use crate::app::AppState;
use crate::application::mcp::menu::{self, CHANGE_LIMIT};
use crate::domain::mcp::{READ, WRITE};
use crate::error::{AppError, AppResult};
use crate::infrastructure::database::queries::mcp_oauth::Grant;

pub const GET_MENU: &str = "get_menu_draft";
pub const APPLY_MENU: &str = "apply_menu_changes";

pub fn scope(name: &str) -> Option<&'static str> {
    match name {
        GET_MENU => Some(READ),
        APPLY_MENU => Some(WRITE),
        _ => None,
    }
}

pub async fn execute(state: &AppState, grant: &Grant, name: &str, args: Value) -> AppResult<Value> {
    match name {
        GET_MENU => {
            menu::read(
                &state.pool,
                grant,
                parse(args)?,
                &state.config.public_base_url,
            )
            .await
        }
        APPLY_MENU => {
            let result = menu::apply(&state.pool, grant, parse(args)?).await?;
            Ok(
                json!({"applied": result, "draft": true, "publication_required": true,
                "preview_url": format!("{}/dashboard/preview", state.config.public_base_url),
                "dashboard_url": format!("{}/dashboard", state.config.public_base_url)}),
            )
        }
        _ => Err(AppError::NotFound("error.not_found")),
    }
}

fn parse<T: serde::de::DeserializeOwned>(args: Value) -> AppResult<T> {
    serde_json::from_value(args).map_err(|_| AppError::Validation("mcp.invalid_changes"))
}

pub fn catalog() -> Value {
    let read = json!([{"type": "oauth2", "scopes": [READ]}]);
    let write = json!([{"type": "oauth2", "scopes": [WRITE]}]);
    json!({"tools": [
        {"name": GET_MENU, "title": "Consultar rascunho do cardápio",
         "description": "Lista categorias do estabelecimento autorizado e seus IDs. Para listar produtos, informe category_id. Continue pela página next_offset. Preserve updated_at para edições.",
         "inputSchema": {"type":"object", "additionalProperties":false, "properties":{
             "category_id":{"type":"string","description":"ID de uma categoria para listar seus produtos"},
             "offset":{"type":"integer","minimum":0,"default":0}, "limit":{"type":"integer","minimum":1,"maximum":40}}},
         "annotations": {"readOnlyHint":true, "destructiveHint":false, "idempotentHint":true, "openWorldHint":false},
         "securitySchemes":read, "_meta":{"securitySchemes":read}},
        {"name": APPLY_MENU, "title": "Salvar alterações no rascunho",
         "description": "Cria categorias e produtos ou altera dados existentes em um lote atômico. Use preço em centavos inteiros e confira os valores com o proprietário. Consulte o rascunho antes de editar. Reutilize operation_key somente ao repetir o mesmo lote. A publicação é feita pelo proprietário no painel.",
         "inputSchema": write_schema(),
         "annotations": {"readOnlyHint":false, "destructiveHint":false, "idempotentHint":true, "openWorldHint":false},
         "securitySchemes":write, "_meta":{"securitySchemes":write}}
    ]})
}

fn write_schema() -> Value {
    let text_props = json!({"name":{"type":"string"}, "name_en":{"type":"string"}, "name_es":{"type":"string"},
         "description":{"type":"string"}, "description_en":{"type":"string"}, "description_es":{"type":"string"}});
    let text = json!({"type":"object","properties":text_props,"additionalProperties":false});
    let mut new_text = text.clone();
    new_text["required"] = json!(["name"]);
    let price = json!({"type":"integer","minimum":0,"maximum":crate::domain::product::MAX_PRICE_CENTAVOS,"description":"Preço em centavos BRL. 2990 significa R$ 29,90."});
    let new_products = json!({"type":"array","maxItems":CHANGE_LIMIT,"items":{"type":"object","additionalProperties":false,
        "required":["text","price_centavos"],"properties":{"text":new_text,"price_centavos":price,
        "available":{"type":"boolean"},"visible":{"type":"boolean"}}}});
    let patches = |product: bool| {
        let mut properties = json!({"id":{"type":"string"},"updated_at":{"type":"string","description":"Versão exata retornada pela leitura"},
            "text":text,"visible":{"type":"boolean"}});
        if product {
            properties["available"] = json!({"type":"boolean"});
            properties["price_centavos"] = price.clone();
        }
        json!({"type":"array","maxItems":CHANGE_LIMIT,"items":{"type":"object","additionalProperties":false,
            "required":["id","updated_at"],"properties":properties}})
    };
    json!({"type":"object", "additionalProperties":false, "required":["operation_key"], "properties":{
        "operation_key":{"type":"string","minLength":4,"maxLength":96,"description":"Chave única gerada para a operação, por exemplo um UUID."},
        "new_categories":{"type":"array","maxItems":CHANGE_LIMIT,"items":{"type":"object","additionalProperties":false,
             "required":["text"],"properties":{"text":new_text,"visible":{"type":"boolean"},"products":new_products}}},
        "add_products":{"type":"array","maxItems":CHANGE_LIMIT,"items":{"type":"object","additionalProperties":false,
             "required":["category_id","products"],"properties":{"category_id":{"type":"string"},"products":new_products}}},
        "category_updates":patches(false), "product_updates":patches(true)}})
}
