use axum::{Router, http::StatusCode, response::Response};
use serde_json::{Value, json};

use super::super::support::*;
use super::Contract;

pub(super) async fn verify_protocols(app: &Router, contract: &Contract, access: &str) {
    let bearer = format!("Bearer {access}");
    for version in &contract.legacy_protocol_versions {
        let initial = rpc(
            app,
            access,
            "initialize",
            json!({"protocolVersion": version}),
        )
        .await;
        assert_eq!(
            json_body(initial).await["result"]["protocolVersion"],
            *version
        );
        let notification = request(
            app,
            "POST",
            &contract.endpoint,
            &[
                ("content-type", "application/json"),
                ("accept", "application/json, text/event-stream"),
                ("authorization", &bearer),
                ("mcp-protocol-version", version),
            ],
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string(),
        )
        .await;
        assert_eq!(notification.status(), StatusCode::ACCEPTED);
        assert!(body(notification).await.is_empty());
        let catalog = request(
            app,
            "POST",
            &contract.endpoint,
            &[
                ("content-type", "application/json"),
                ("accept", "application/json, text/event-stream"),
                ("authorization", &bearer),
                ("mcp-protocol-version", version),
            ],
            json!({"jsonrpc":"2.0","id":21,"method":"tools/list","params":{}}).to_string(),
        )
        .await;
        assert_eq!(catalog.status(), StatusCode::OK);
        assert!(catalog.headers().get("mcp-session-id").is_none());
        let result = json_body(catalog).await;
        assert_eq!(result["id"], 21);
        verify_tool_schema(&result["result"]["tools"]);
    }

    let version = &contract.modern_protocol_version;
    let discovered = modern(app, &bearer, version, "server/discover", json!({}), None).await;
    assert_eq!(discovered.status(), StatusCode::OK);
    let result = json_body(discovered).await;
    assert_eq!(result["result"]["supportedVersions"], json!([version]));
    assert_eq!(
        result["result"]["_meta"]["io.modelcontextprotocol/serverInfo"]["name"],
        "qrcardapio"
    );
    let catalog = modern(app, &bearer, version, "tools/list", json!({}), None).await;
    verify_tool_schema(&json_body(catalog).await["result"]["tools"]);
    let args = json!({"name":"get_menu_draft","arguments":{}});
    let read = modern(
        app,
        &bearer,
        version,
        "tools/call",
        args.clone(),
        Some("get_menu_draft"),
    )
    .await;
    let response = json_body(read).await;
    assert_eq!(response["result"]["resultType"], "complete");
    assert!(response["result"]["structuredContent"]["categories"].is_array());
    let wrong_header = modern(
        app,
        &bearer,
        version,
        "tools/call",
        args,
        Some("different_tool"),
    )
    .await;
    assert_eq!(wrong_header.status(), StatusCode::BAD_REQUEST);
    assert_eq!(json_body(wrong_header).await["error"]["code"], -32600);
}

async fn modern(
    app: &Router,
    bearer: &str,
    version: &str,
    method: &str,
    mut params: Value,
    tool: Option<&str>,
) -> Response {
    let mut headers = vec![
        ("content-type", "application/json"),
        ("accept", "application/json, text/event-stream"),
        ("authorization", bearer),
        ("mcp-protocol-version", version),
        ("mcp-method", method),
    ];
    if let Some(tool) = tool {
        headers.push(("mcp-name", tool));
    }
    params["_meta"] = json!({"io.modelcontextprotocol/protocolVersion":version});
    request(
        app,
        "POST",
        "/mcp",
        &headers,
        json!({"jsonrpc":"2.0","id":22,"method":method,"params":params}).to_string(),
    )
    .await
}

fn verify_tool_schema(tools: &Value) {
    let contract: Value =
        serde_json::from_str(include_str!("../../../contracts/mcp/tools.json")).unwrap();
    let tools = tools.as_array().unwrap();
    assert_eq!(tools.len(), contract.as_object().unwrap().len());
    for tool in tools {
        let name = tool["name"].as_str().unwrap();
        let expected = &contract[name];
        assert!(!expected.is_null());
        assert_eq!(tool["annotations"]["readOnlyHint"], expected["read_only"]);
        assert_eq!(
            tool["annotations"]["destructiveHint"],
            expected["destructive"]
        );
        assert_eq!(tool["_meta"]["securitySchemes"], tool["securitySchemes"]);
        assert_eq!(
            tool["securitySchemes"],
            json!([{"type":"oauth2","scopes":[expected["scope"]]}])
        );
        let schema = &tool["inputSchema"];
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(
            schema.get("required").cloned().unwrap_or(json!([])),
            expected["required"]
        );
        let props: Vec<&String> = schema["properties"].as_object().unwrap().keys().collect();
        assert_eq!(json!(props), expected["properties"]);
        if let Some(pointers) = expected["integer_centavos"].as_array() {
            for pointer in pointers {
                let price = schema.pointer(pointer.as_str().unwrap()).unwrap();
                assert_eq!(price["type"], "integer");
                assert_eq!(price["minimum"], 0);
            }
        }
    }
}
