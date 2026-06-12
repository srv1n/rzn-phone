use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct JsonRpcRequest {
    pub id: Value,
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone)]
pub struct JsonRpcNotification {
    pub method: String,
    pub params: Value,
}

#[derive(Debug, Clone)]
pub enum IncomingMessage {
    Request(JsonRpcRequest),
    Notification(JsonRpcNotification),
}

pub fn parse_incoming_line(line: &str) -> Result<IncomingMessage, String> {
    let value: Value = serde_json::from_str(line).map_err(|err| format!("invalid JSON: {err}"))?;
    let Some(obj) = value.as_object() else {
        return Err("JSON-RPC payload must be an object".to_string());
    };

    let method = obj
        .get("method")
        .and_then(Value::as_str)
        .ok_or_else(|| "missing string method".to_string())?
        .to_string();
    let params = obj.get("params").cloned().unwrap_or_else(|| json!({}));

    match obj.get("id") {
        Some(id) if !id.is_null() => Ok(IncomingMessage::Request(JsonRpcRequest {
            id: id.clone(),
            method,
            params,
        })),
        _ => Ok(IncomingMessage::Notification(JsonRpcNotification {
            method,
            params,
        })),
    }
}

pub fn build_result_response(id: Value, result: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result,
    })
}

pub fn build_error_response(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
            "data": data,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_request_with_string_id() {
        let msg =
            parse_incoming_line(r#"{"jsonrpc":"2.0","id":"abc","method":"ping","params":{}}"#)
                .expect("parsed");
        let IncomingMessage::Request(req) = msg else {
            panic!("expected request");
        };
        assert_eq!(req.id, json!("abc"));
        assert_eq!(req.method, "ping");
    }

    #[test]
    fn parses_request_with_numeric_id() {
        let msg = parse_incoming_line(r#"{"id":42,"method":"ping"}"#).expect("parsed");
        let IncomingMessage::Request(req) = msg else {
            panic!("expected request");
        };
        assert_eq!(req.id, json!(42));
    }

    #[test]
    fn parses_notification_without_id() {
        let msg = parse_incoming_line(r#"{"method":"initialized"}"#).expect("parsed");
        let IncomingMessage::Notification(note) = msg else {
            panic!("expected notification");
        };
        assert_eq!(note.method, "initialized");
    }

    #[test]
    fn parses_notification_with_null_id() {
        let msg = parse_incoming_line(r#"{"id":null,"method":"initialized"}"#).expect("parsed");
        let IncomingMessage::Notification(_) = msg else {
            panic!("expected notification");
        };
    }
}
