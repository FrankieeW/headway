//! `--ai` output: the JSON result rendered as compact XML.
//!
//! Short scalars become attributes, records become elements and lists get a
//! wrapper with singular children, e.g. `<apps><app id="rg" status="up_to_date"/></apps>`.
//! Long or multi-line text (commit messages, logs) becomes element text.

use std::fmt::Write as _;

use serde_json::Value;

const ATTRIBUTE_MAX: usize = 160;

pub fn document(command: &str, ok: bool, data: &Value, next: &[String]) -> String {
    let mut attributes = vec![
        ("command".to_owned(), command.to_owned()),
        ("ok".to_owned(), ok.to_string()),
    ];
    let mut children = String::new();
    match data {
        Value::Object(map) => {
            for (key, value) in map {
                field(key, value, 1, &mut attributes, &mut children);
            }
        }
        Value::Null => {}
        other => element("data", other, 1, &mut children),
    }
    if !next.is_empty() {
        children.push_str("  <next>\n");
        for command in next {
            let _ = writeln!(children, "    <command>{}</command>", escape(command));
        }
        children.push_str("  </next>\n");
    }
    let mut out = format!("<headway{}", attributes_text(&attributes));
    if children.is_empty() {
        out.push_str("/>");
    } else {
        let _ = write!(out, ">\n{children}</headway>");
    }
    out
}

fn field(
    key: &str,
    value: &Value,
    depth: usize,
    attributes: &mut Vec<(String, String)>,
    children: &mut String,
) {
    match value {
        Value::Null => {}
        Value::Bool(flag) => attributes.push((key.to_owned(), flag.to_string())),
        Value::Number(number) => attributes.push((key.to_owned(), number.to_string())),
        Value::String(text) if text.chars().count() <= ATTRIBUTE_MAX && !text.contains('\n') => {
            attributes.push((key.to_owned(), text.clone()));
        }
        other => element(key, other, depth, children),
    }
}

fn element(name: &str, value: &Value, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    match value {
        Value::Null => {}
        Value::Bool(_) | Value::Number(_) => {
            let _ = writeln!(out, "{indent}<{name}>{value}</{name}>");
        }
        Value::String(text) => {
            let _ = writeln!(out, "{indent}<{name}>{}</{name}>", escape(text));
        }
        Value::Array(items) if items.is_empty() => {
            let _ = writeln!(out, "{indent}<{name} count=\"0\"/>");
        }
        Value::Array(items) => {
            let item_name = singular(name);
            let _ = writeln!(out, "{indent}<{name} count=\"{}\">", items.len());
            for item in items {
                element(&item_name, item, depth + 1, out);
            }
            let _ = writeln!(out, "{indent}</{name}>");
        }
        Value::Object(map) => {
            let mut attributes = Vec::new();
            let mut children = String::new();
            for (key, child) in map {
                field(key, child, depth + 1, &mut attributes, &mut children);
            }
            let attributes = attributes_text(&attributes);
            if children.is_empty() {
                let _ = writeln!(out, "{indent}<{name}{attributes}/>");
            } else {
                let _ = write!(
                    out,
                    "{indent}<{name}{attributes}>\n{children}{indent}</{name}>\n"
                );
            }
        }
    }
}

fn attributes_text(attributes: &[(String, String)]) -> String {
    attributes
        .iter()
        .fold(String::new(), |mut out, (key, value)| {
            let _ = write!(out, " {key}=\"{}\"", escape(value));
            out
        })
}

fn singular(name: &str) -> String {
    let known = match name {
        "pending" => Some("candidate"),
        "history" => Some("entry"),
        "binaries" => Some("binary"),
        "next" | "commands" | "subcommands" => Some("command"),
        "details" | "lines" => Some("line"),
        "possible_values" => Some("value"),
        "args" => Some("arg"),
        _ => None,
    };
    match known {
        Some(word) => word.to_owned(),
        None if name.len() > 2 && name.ends_with('s') => name[..name.len() - 1].to_owned(),
        None => "item".to_owned(),
    }
}

/// Escapes markup and drops characters XML 1.0 cannot contain.
pub fn escape(text: &str) -> String {
    text.chars()
        .filter(|c| matches!(c, '\t' | '\n' | '\r') || !c.is_control())
        .fold(String::with_capacity(text.len()), |mut out, c| {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                '\'' => out.push_str("&apos;"),
                c => out.push(c),
            }
            out
        })
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn renders_records_lists_and_text() {
        let data = json!({
            "apps": [
                {"id": "rg", "status": "up_to_date", "pending_count": 0, "busy": false, "error": null,
                 "installed": {"id": "abc", "version": "14.1.1"}},
                {"id": "zed", "body": "line one\nline <two>"}
            ],
            "total": 2
        });
        let xml = document("list", true, &data, &["headway check".to_owned()]);
        assert!(xml.starts_with("<headway command=\"list\" ok=\"true\" total=\"2\">"));
        assert!(xml.contains("<apps count=\"2\">"));
        assert!(
            xml.contains(
                "<app id=\"rg\" status=\"up_to_date\" pending_count=\"0\" busy=\"false\">"
            )
        );
        assert!(xml.contains("<installed id=\"abc\" version=\"14.1.1\"/>"));
        assert!(xml.contains("<body>line one\nline &lt;two&gt;</body>"));
        assert!(xml.contains("<next>\n    <command>headway check</command>\n  </next>"));
        assert!(!xml.contains("error"), "nulls are omitted");
        assert!(xml.ends_with("</headway>"));
    }

    #[test]
    fn empty_and_scalar_documents() {
        assert_eq!(
            document("paths", true, &Value::Null, &[]),
            "<headway command=\"paths\" ok=\"true\"/>"
        );
        assert!(document("x", true, &json!({"items": []}), &[]).contains("<items count=\"0\"/>"));
        assert!(document("x", true, &json!(["a"]), &[]).contains("<data count=\"1\">"));
    }

    #[test]
    fn escapes_and_strips_control_characters() {
        assert_eq!(
            escape("a & b < \"c\" 'd'\u{1}\u{7}"),
            "a &amp; b &lt; &quot;c&quot; &apos;d&apos;"
        );
        assert_eq!(singular("apps"), "app");
        assert_eq!(singular("pending"), "candidate");
        assert_eq!(singular("data"), "item");
    }
}
