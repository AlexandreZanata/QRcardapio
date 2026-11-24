use serde::Deserialize;
use serde_json::{Value, json};

use crate::app::AppState;
use crate::application::mcp::{
    account,
    menu::{self, CHANGE_LIMIT},
};
use crate::domain::mcp::{READ, WRITE};
use crate::error::AppError;
use crate::infrastructure::database::queries::mcp_oauth::Grant;

pub const GET_ACCOUNT: &str = "get_account_settings";
pub const UPDATE_ACCOUNT: &str = "update_account_settings";
pub const GET_MENU: &str = "get_menu_draft";
pub const APPLY_MENU: &str = "apply_menu_changes";
pub const PUBLISH_MENU: &str = "publish_menu";
pub const UNPUBLISH_MENU: &str = "unpublish_menu";

pub struct ToolFailure {
    pub error: AppError,
    pub hint: Option<String>,
    pub extra: Option<Value>,
}

impl From<AppError> for ToolFailure {
    fn from(error: AppError) -> Self {
        Self {
            error,
            hint: None,
            extra: None,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EmptyArgs {}

pub fn scope(name: &str) -> Option<&'static str> {
    match name {
        GET_ACCOUNT | GET_MENU => Some(READ),
        UPDATE_ACCOUNT | APPLY_MENU | PUBLISH_MENU | UNPUBLISH_MENU => Some(WRITE),
        _ => None,
    }
}

pub async fn execute(
    state: &AppState,
    grant: &Grant,
    name: &str,
    args: Value,
) -> Result<Value, ToolFailure> {
    match name {
        GET_ACCOUNT => {
            let _: EmptyArgs = parse(args)?;
            Ok(account::read_settings(
                &state.pool,
                state.billing.enforcement(),
                grant,
                &state.config.public_base_url,
            )
            .await?)
        }
        UPDATE_ACCOUNT => Ok(account::update_settings(
            &state.pool,
            &state.publish_locks,
            state.billing.enforcement(),
            grant,
            parse(args)?,
            &state.config.public_base_url,
        )
        .await?),
        GET_MENU => Ok(menu::read(
            &state.pool,
            grant,
            parse(args)?,
            &state.config.public_base_url,
        )
        .await?),
        APPLY_MENU => {
            let result = menu::apply(&state.pool, &state.assets, grant, parse(args)?).await?;
            Ok(json!({
                "applied": result,
                "draft": true,
                "publication_required": true,
                "preview_url": format!("{}/dashboard/preview", state.config.public_base_url),
                "dashboard_url": format!("{}/dashboard", state.config.public_base_url),
            }))
        }
        PUBLISH_MENU => {
            let _: EmptyArgs = parse(args)?;
            match account::publish(
                &state.pool,
                &state.config,
                &state.publish_locks,
                state.billing.enforcement(),
                grant,
            )
            .await
            {
                Ok(data) => Ok(data),
                Err(err @ AppError::Forbidden("error.subscription_required")) => Err(ToolFailure {
                    error: err,
                    hint: None,
                    extra: Some(json!({
                        "billing_url": format!("{}/dashboard/billing", state.config.public_base_url)
                    })),
                }),
                Err(err) => Err(err.into()),
            }
        }
        UNPUBLISH_MENU => {
            let _: EmptyArgs = parse(args)?;
            Ok(account::unpublish(&state.pool, &state.config, &state.publish_locks, grant).await?)
        }
        _ => Err(AppError::NotFound("error.not_found").into()),
    }
}

fn parse<T: serde::de::DeserializeOwned>(args: Value) -> Result<T, ToolFailure> {
    serde_json::from_value(args).map_err(|err| {
        let cleaned: String = err
            .to_string()
            .chars()
            .filter(|c| !c.is_control())
            .take(180)
            .collect();
        ToolFailure {
            error: AppError::Validation("mcp.invalid_changes"),
            hint: (!cleaned.is_empty()).then_some(cleaned),
            extra: None,
        }
    })
}

pub fn catalog() -> Value {
    let read = json!([{"type": "oauth2", "scopes": [READ]}]);
    let write = json!([{"type": "oauth2", "scopes": [WRITE]}]);
    let empty_schema = json!({"type": "object", "additionalProperties": false, "properties": {}});
    json!({"tools": [
        {
            "name": GET_ACCOUNT,
            "title": "Consultar configurações da conta e do estabelecimento",
            "description": "Retorna os dados da conta, perfil do estabelecimento (nome, slug, se o slug está travado, descrições PT/EN/ES, cor da marca, links do Google Maps e avaliações), resumo do cardápio e estado de publicação/assinatura. Consulte antes de usar update_account_settings ou publish_menu.",
            "inputSchema": empty_schema,
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false},
            "securitySchemes": read, "_meta": {"securitySchemes": read}
        },
        {
            "name": UPDATE_ACCOUNT,
            "title": "Editar configurações da conta e do estabelecimento",
            "description": "Atualiza nome, slug (apenas antes da primeira publicação), descrições em português/inglês/espanhol, cor da marca (#RRGGBB), links do Google Maps e avaliações (envie null ou string vazia para limpar) e idioma preferido da conta (pt-BR, en ou es). Informe updated_at retornado por get_account_settings e uma operation_key única.",
            "inputSchema": account_schema(),
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false},
            "securitySchemes": write, "_meta": {"securitySchemes": write}
        },
        {
            "name": GET_MENU,
            "title": "Consultar rascunho do cardápio",
            "description": "Lista categorias do estabelecimento autorizado com posição, quantidade de itens e IDs. Para listar produtos, informe category_id. Continue pela página next_offset. Preserve updated_at para edições e exclusões.",
            "inputSchema": {
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "category_id": {"type": "string", "description": "ID de uma categoria para listar seus produtos"},
                    "offset": {"type": "integer", "minimum": 0, "default": 0},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 40}
                }
            },
            "annotations": {"readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false},
            "securitySchemes": read, "_meta": {"securitySchemes": read}
        },
        {
            "name": APPLY_MENU,
            "title": "Salvar alterações no rascunho do cardápio",
            "description": "Cria, edita, move entre categorias, reordena ou exclui categorias e produtos no rascunho em um lote atômico (até 80 operações). Use preço em centavos inteiros e confirme preços ou exclusões com o proprietário. Consulte get_menu_draft antes de alterar ou excluir. Reutilize operation_key somente ao repetir o mesmo lote. Após salvar, use publish_menu apenas se o proprietário pedir para publicar.",
            "inputSchema": write_schema(),
            "annotations": {"readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false},
            "securitySchemes": write, "_meta": {"securitySchemes": write}
        },
        {
            "name": PUBLISH_MENU,
            "title": "Publicar cardápio ao vivo",
            "description": "Gera e publica atomicamente o cardápio estático público (/c/{slug}) a partir do rascunho atual, respeitando as regras de assinatura. Confirme com o proprietário antes de publicar.",
            "inputSchema": empty_schema,
            "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false},
            "securitySchemes": write, "_meta": {"securitySchemes": write}
        },
        {
            "name": UNPUBLISH_MENU,
            "title": "Retirar cardápio do ar",
            "description": "Retira temporariamente o cardápio público do ar mantendo o rascunho e o QR Code preservados para futura republicação. Confirme explicitamente com o proprietário antes de executar.",
            "inputSchema": empty_schema,
            "annotations": {"readOnlyHint": false, "destructiveHint": true, "idempotentHint": true, "openWorldHint": false},
            "securitySchemes": write, "_meta": {"securitySchemes": write}
        }
    ]})
}

fn account_schema() -> Value {
    let nullable_url = json!({"type": ["string", "null"], "maxLength": 2048});
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["operation_key", "updated_at"],
        "properties": {
            "operation_key": {"type": "string", "minLength": 4, "maxLength": 96, "description": "Chave única gerada para a operação, por exemplo um UUID."},
            "updated_at": {"type": "string", "description": "Valor exato de establishment.updated_at retornado por get_account_settings."},
            "name": {"type": "string", "minLength": 1, "maxLength": 100},
            "slug": {"type": "string", "minLength": 3, "maxLength": 50, "description": "Endereço público (/c/{slug}). Somente editável enquanto published_version == 0."},
            "description": {"type": "string", "maxLength": 280},
            "description_en": {"type": "string", "maxLength": 280},
            "description_es": {"type": "string", "maxLength": 280},
            "accent_color": {"type": "string", "pattern": "^#[0-9A-Fa-f]{6}$", "description": "Cor principal em hexadecimal (#RRGGBB)."},
            "google_maps_url": nullable_url,
            "google_review_url": nullable_url,
            "preferred_locale": {"type": "string", "enum": ["pt-BR", "en", "es"]}
        }
    })
}

fn write_schema() -> Value {
    let text_props = json!({
        "name": {"type": "string"},
        "name_en": {"type": "string"},
        "name_es": {"type": "string"},
        "description": {"type": "string"},
        "description_en": {"type": "string"},
        "description_es": {"type": "string"}
    });
    let text = json!({"type": "object", "properties": text_props, "additionalProperties": false});
    let mut new_text = text.clone();
    new_text["required"] = json!(["name"]);
    let price = json!({
        "type": "integer",
        "minimum": 0,
        "maximum": crate::domain::product::MAX_PRICE_CENTAVOS,
        "description": "Preço em centavos BRL. 2990 significa R$ 29,90."
    });
    let new_products = json!({
        "type": "array",
        "maxItems": CHANGE_LIMIT,
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["text", "price_centavos"],
            "properties": {
                "text": new_text,
                "price_centavos": price,
                "available": {"type": "boolean"},
                "visible": {"type": "boolean"}
            }
        }
    });
    let patches = |product: bool| {
        let mut properties = json!({
            "id": {"type": "string"},
            "updated_at": {"type": "string", "description": "Versão exata retornada pela leitura"},
            "text": text,
            "visible": {"type": "boolean"}
        });
        if product {
            properties["category_id"] = json!({"type": "string", "description": "Nova categoria de destino no mesmo estabelecimento"});
            properties["available"] = json!({"type": "boolean"});
            properties["price_centavos"] = price.clone();
        }
        json!({
            "type": "array",
            "maxItems": CHANGE_LIMIT,
            "items": {
                "type": "object",
                "additionalProperties": false,
                "required": ["id", "updated_at"],
                "properties": properties
            }
        })
    };
    let delete_refs = json!({
        "type": "array",
        "maxItems": CHANGE_LIMIT,
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["id", "updated_at"],
            "properties": {
                "id": {"type": "string"},
                "updated_at": {"type": "string", "description": "Versão exata retornada pela leitura"}
            }
        }
    });
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["operation_key"],
        "properties": {
            "operation_key": {"type": "string", "minLength": 4, "maxLength": 96, "description": "Chave única gerada para a operação, por exemplo um UUID."},
            "new_categories": {
                "type": "array",
                "maxItems": CHANGE_LIMIT,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["text"],
                    "properties": {
                        "text": new_text,
                        "visible": {"type": "boolean"},
                        "products": new_products
                    }
                }
            },
            "add_products": {
                "type": "array",
                "maxItems": CHANGE_LIMIT,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["category_id", "products"],
                    "properties": {
                        "category_id": {"type": "string"},
                        "products": new_products
                    }
                }
            },
            "category_updates": patches(false),
            "product_updates": patches(true),
            "delete_categories": delete_refs,
            "delete_products": delete_refs,
            "category_order": {
                "type": "array",
                "minItems": 1,
                "items": {"type": "string"},
                "description": "Lista completa e ordenada com todos os IDs de categorias do estabelecimento."
            },
            "product_orders": {
                "type": "array",
                "maxItems": CHANGE_LIMIT,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["category_id", "product_ids"],
                    "properties": {
                        "category_id": {"type": "string"},
                        "product_ids": {"type": "array", "minItems": 1, "items": {"type": "string"}}
                    }
                }
            }
        }
    })
}
