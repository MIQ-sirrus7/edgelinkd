use crate::runtime::model::Variant;
use std::cmp::Ordering;

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
// The caller has at least one BigInt operand. JS relational comparisons preserve its bits.
pub(super) fn compare_bigint(a: &Variant, b: &Variant) -> Option<Ordering> {
    let (integer, other, reverse) = match (a, b) {
        (Variant::BigInt(x), Variant::BigInt(y)) => return Some(x.value().cmp(&y.value())),
        (Variant::BigInt(n), other) => (n.value(), other, false),
        (other, Variant::BigInt(n)) => (n.value(), other, true),
        _ => return None,
    };
    let order = match other {
        Variant::String(s) => compare_integer_text(integer, s)?,
        Variant::Array(_) | Variant::Bytes(_) => compare_integer_text(integer, &js_string(other))?,
        _ => {
            let number = js_number(Some(other));
            if number.is_nan() {
                return None;
            }
            // Saturating conversion is safe: every supported BigInt lies strictly inside i128.
            integer.cmp(&(number as i128)).then_with(|| 0.0_f64.partial_cmp(&number.fract()).unwrap_or(Ordering::Equal))
        }
    };
    Some(if reverse { order.reverse() } else { order })
}

fn compare_integer_text(integer: i128, text: &str) -> Option<Ordering> {
    let text = text.trim();
    if text.is_empty() {
        return Some(integer.cmp(&0));
    }
    let (radix, digits) = match text.get(..2) {
        Some("0x" | "0X") => (16, &text[2..]),
        Some("0o" | "0O") => (8, &text[2..]),
        Some("0b" | "0B") => (2, &text[2..]),
        _ => (10, text.strip_prefix(['+', '-']).unwrap_or(text)),
    };
    if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
        return None;
    }
    let value = i128::from_str_radix(digits, radix).unwrap_or(i128::MAX);
    Some(integer.cmp(&(if text.starts_with('-') { -value } else { value })))
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
        Some(Variant::BigInt(n)) => n.value() as f64,
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
