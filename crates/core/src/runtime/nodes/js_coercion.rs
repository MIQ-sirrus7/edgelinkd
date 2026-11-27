use crate::runtime::model::Variant;

pub(super) fn js_string(value: &Variant) -> String {
    match value {
        Variant::String(s) => s.clone(),
        Variant::Null => "null".into(),
        Variant::Array(a) => a
            .iter()
            .map(|v| if matches!(v, Variant::Null) { String::new() } else { js_string(v) })
            .collect::<Vec<_>>()
            .join(","),
        Variant::Object(_) => "[object Object]".into(),
        Variant::Bytes(b) => String::from_utf8_lossy(b).into_owned(),
        Variant::Regexp(r) => format!("/{r}/"),
        _ => String::from(value),
    }
}
pub(super) fn js_number(value: Option<&Variant>) -> f64 {
    match value {
        None => f64::NAN,
        Some(Variant::Null) => 0.0,
        Some(Variant::Bool(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Some(Variant::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        Some(Variant::Date(d)) => match d.duration_since(std::time::UNIX_EPOCH) {
            Ok(duration) => duration.as_secs_f64() * 1000.0,
            Err(err) => -err.duration().as_secs_f64() * 1000.0,
        },
        Some(v @ (Variant::String(_) | Variant::Array(_))) => {
            let text = js_string(v);
            let text = text.trim();
            if text.is_empty() {
                return 0.0;
            }
            for (prefix, radix) in [("0x", 16), ("0X", 16), ("0b", 2), ("0B", 2), ("0o", 8), ("0O", 8)] {
                if let Some(digits) = text.strip_prefix(prefix) {
                    return u64::from_str_radix(digits, radix).map(|n| n as f64).unwrap_or(f64::NAN);
                }
            }
            if text.bytes().any(|c| c.is_ascii_alphabetic() && c != b'e' && c != b'E') {
                return match text {
                    "Infinity" | "+Infinity" => f64::INFINITY,
                    "-Infinity" => f64::NEG_INFINITY,
                    _ => f64::NAN,
                };
            }
            text.parse().unwrap_or(f64::NAN)
        }
        _ => f64::NAN,
    }
}
