use regex::Regex;
use serde_json::{json, Value};

const BLOCKED_DOMAINS: &[&str] = &[
    "command_line",
    "hassio",
    "pyscript",
    "python_script",
    "rest_command",
    "shell_command",
];

pub fn ha_list_entities_response(
    states: &[Value],
    domain: Option<&str>,
    area: Option<&str>,
) -> Value {
    json!({"result": filter_and_summarize(states, domain, area)})
}

pub fn filter_and_summarize(states: &[Value], domain: Option<&str>, area: Option<&str>) -> Value {
    let area_lower = area.map(str::to_ascii_lowercase);
    let entities = states
        .iter()
        .filter(|state| {
            domain
                .map(|domain| {
                    state
                        .get("entity_id")
                        .and_then(Value::as_str)
                        .map(|entity_id| entity_id.starts_with(&format!("{domain}.")))
                        .unwrap_or(false)
                })
                .unwrap_or(true)
        })
        .filter(|state| {
            area_lower
                .as_ref()
                .map(|area| {
                    let attributes = state.get("attributes").and_then(Value::as_object);
                    let friendly_name = attributes
                        .and_then(|attrs| attrs.get("friendly_name"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    let entity_area = attributes
                        .and_then(|attrs| attrs.get("area"))
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_ascii_lowercase();
                    friendly_name.contains(area) || entity_area.contains(area)
                })
                .unwrap_or(true)
        })
        .map(|state| {
            let attributes = state.get("attributes").and_then(Value::as_object);
            json!({
                "entity_id": state.get("entity_id").and_then(Value::as_str).unwrap_or(""),
                "state": state.get("state").and_then(Value::as_str).unwrap_or(""),
                "friendly_name": attributes
                    .and_then(|attrs| attrs.get("friendly_name"))
                    .and_then(Value::as_str)
                    .unwrap_or(""),
            })
        })
        .collect::<Vec<_>>();
    json!({"count": entities.len(), "entities": entities})
}

pub fn ha_get_state_response(entity_id: &str, state: Option<&Value>) -> Value {
    if entity_id.is_empty() {
        return tool_error("Missing required parameter: entity_id");
    }
    if !entity_id_is_valid(entity_id) {
        return tool_error(&format!("Invalid entity_id format: {entity_id}"));
    }
    let Some(data) = state else {
        return tool_error(&format!("Failed to get state for {entity_id}: not found"));
    };
    json!({
        "result": {
            "entity_id": data.get("entity_id").and_then(Value::as_str).unwrap_or(""),
            "state": data.get("state").and_then(Value::as_str).unwrap_or(""),
            "attributes": data.get("attributes").cloned().unwrap_or_else(|| json!({})),
            "last_changed": data.get("last_changed").cloned().unwrap_or(Value::Null),
            "last_updated": data.get("last_updated").cloned().unwrap_or(Value::Null),
        }
    })
}

pub fn ha_list_services_response(services: &[Value], domain: Option<&str>) -> Value {
    let domains = services
        .iter()
        .filter(|svc_domain| {
            domain
                .map(|domain| {
                    svc_domain
                        .get("domain")
                        .and_then(Value::as_str)
                        .map(|value| value == domain)
                        .unwrap_or(false)
                })
                .unwrap_or(true)
        })
        .map(compact_service_domain)
        .collect::<Vec<_>>();
    json!({"result": {"count": domains.len(), "domains": domains}})
}

pub fn ha_call_service_response(
    domain: &str,
    service: &str,
    entity_id: Option<&str>,
    data: Option<&Value>,
    service_result: Option<&Value>,
) -> Value {
    if domain.is_empty() || service.is_empty() {
        return tool_error("Missing required parameters: domain and service");
    }
    if !service_name_is_valid(domain) {
        return tool_error(&format!(
            "Invalid domain format: {}",
            py_string_repr(domain)
        ));
    }
    if !service_name_is_valid(service) {
        return tool_error(&format!(
            "Invalid service format: {}",
            py_string_repr(service)
        ));
    }
    if BLOCKED_DOMAINS.contains(&domain) {
        return json!({
            "error": format!(
                "Service domain '{}' is blocked for security. Blocked domains: {}",
                domain,
                BLOCKED_DOMAINS.join(", ")
            )
        });
    }
    if let Some(entity_id) = entity_id {
        if !entity_id_is_valid(entity_id) {
            return tool_error(&format!("Invalid entity_id format: {entity_id}"));
        }
    }

    let parsed_data = match normalize_service_data(data) {
        Ok(value) => value,
        Err(error) => return tool_error(&error),
    };
    let _payload = build_service_payload(entity_id, parsed_data.as_ref());
    let empty_result = json!([]);
    let result = service_result.unwrap_or(&empty_result);
    json!({"result": parse_service_response(domain, service, result)})
}

pub fn build_service_payload(entity_id: Option<&str>, data: Option<&Value>) -> Value {
    let mut payload = data.and_then(Value::as_object).cloned().unwrap_or_default();
    if let Some(entity_id) = entity_id.filter(|value| !value.is_empty()) {
        payload.insert("entity_id".to_string(), json!(entity_id));
    }
    Value::Object(payload)
}

pub fn parse_service_response(domain: &str, service: &str, result: &Value) -> Value {
    let affected = result
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|state| {
                    json!({
                        "entity_id": state.get("entity_id").and_then(Value::as_str).unwrap_or(""),
                        "state": state.get("state").and_then(Value::as_str).unwrap_or(""),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    json!({
        "success": true,
        "service": format!("{domain}.{service}"),
        "affected_entities": affected,
    })
}

fn compact_service_domain(svc_domain: &Value) -> Value {
    let domain = svc_domain
        .get("domain")
        .and_then(Value::as_str)
        .unwrap_or("");
    let mut services_out = serde_json::Map::new();
    if let Some(services) = svc_domain.get("services").and_then(Value::as_object) {
        for (svc_name, svc_info) in services {
            let mut svc_entry = serde_json::Map::new();
            svc_entry.insert(
                "description".to_string(),
                json!(svc_info
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or("")),
            );
            let mut fields_out = serde_json::Map::new();
            if let Some(fields) = svc_info.get("fields").and_then(Value::as_object) {
                for (field_name, field_info) in fields {
                    if let Some(field_obj) = field_info.as_object() {
                        fields_out.insert(
                            field_name.clone(),
                            json!(field_obj
                                .get("description")
                                .and_then(Value::as_str)
                                .unwrap_or("")),
                        );
                    }
                }
            }
            if !fields_out.is_empty() {
                svc_entry.insert("fields".to_string(), Value::Object(fields_out));
            }
            services_out.insert(svc_name.clone(), Value::Object(svc_entry));
        }
    }
    json!({"domain": domain, "services": services_out})
}

fn normalize_service_data(data: Option<&Value>) -> Result<Option<Value>, String> {
    match data {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) if text.trim().is_empty() => Ok(None),
        Some(Value::String(text)) => serde_json::from_str::<Value>(text)
            .map(Some)
            .map_err(|error| format!("Invalid JSON string in 'data' parameter: {error}")),
        Some(value) => Ok(Some(value.clone())),
    }
}

fn entity_id_is_valid(entity_id: &str) -> bool {
    Regex::new(r"^[a-z_][a-z0-9_]*\.[a-z0-9_]+$")
        .expect("entity regex compiles")
        .is_match(entity_id)
}

fn service_name_is_valid(name: &str) -> bool {
    Regex::new(r"^[a-z][a-z0-9_]*$")
        .expect("service regex compiles")
        .is_match(name)
}

fn py_string_repr(value: &str) -> String {
    if value.contains('\'') && !value.contains('"') {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
    }
}

fn tool_error(message: &str) -> Value {
    json!({"error": message})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_id_validation_rejects_traversal() {
        assert!(!entity_id_is_valid("../../api"));
        assert!(entity_id_is_valid("light.living_room"));
    }

    #[test]
    fn service_payload_entity_takes_precedence() {
        assert_eq!(
            build_service_payload(
                Some("light.kitchen"),
                Some(&json!({"entity_id": "light.old", "brightness": 255})),
            ),
            json!({"entity_id": "light.kitchen", "brightness": 255})
        );
    }
}
