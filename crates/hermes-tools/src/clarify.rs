use serde_json::{json, Value};

pub const MAX_CHOICES: usize = 4;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClarifyCallback {
    Unavailable,
    Response(String),
    Error(String),
}

pub fn clarify_response(
    question: &str,
    choices: Option<&[Value]>,
    callback: ClarifyCallback,
) -> Value {
    let question = question.trim();
    if question.is_empty() {
        return json!({"error": "Question text is required."});
    }

    let choices = normalize_choices(choices);
    match callback {
        ClarifyCallback::Unavailable => {
            json!({"error": "Clarify tool is not available in this execution context."})
        }
        ClarifyCallback::Error(error) => {
            json!({"error": format!("Failed to get user input: {error}")})
        }
        ClarifyCallback::Response(response) => json!({
            "question": question,
            "choices_offered": choices,
            "user_response": response.trim(),
        }),
    }
}

fn normalize_choices(choices: Option<&[Value]>) -> Option<Vec<String>> {
    let choices = choices?;
    let normalized = choices
        .iter()
        .map(value_to_string)
        .map(|choice| choice.trim().to_string())
        .filter(|choice| !choice.is_empty())
        .take(MAX_CHOICES)
        .collect::<Vec<_>>();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_question_and_unavailable_callback() {
        assert_eq!(
            clarify_response(" ", None, ClarifyCallback::Unavailable),
            json!({"error": "Question text is required."})
        );
        assert_eq!(
            clarify_response("Question?", None, ClarifyCallback::Unavailable),
            json!({"error": "Clarify tool is not available in this execution context."})
        );
    }

    #[test]
    fn trims_and_limits_choices() {
        assert_eq!(
            clarify_response(
                "  Pick one  ",
                Some(&[
                    json!(" A "),
                    json!(""),
                    json!(2),
                    json!("C"),
                    json!("D"),
                    json!("E"),
                ]),
                ClarifyCallback::Response("  A  ".to_string()),
            ),
            json!({
                "question": "Pick one",
                "choices_offered": ["A", "2", "C", "D"],
                "user_response": "A",
            })
        );
    }
}
