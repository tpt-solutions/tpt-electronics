// SPDX-License-Identifier: MIT OR Apache-2.0

//! Minimal, order-preserving edits to golden JSON documents.
//!
//! The goldens mix machine-checkable numbers with hand-written prose
//! (`description`, `physical_check`, `netlist`) and tolerances that encode
//! reviewer intent. Rewriting them through a typed struct would silently drop
//! or clobber anything not modelled, so edits are applied as targeted
//! mutations on the parsed document instead.

use serde_json::{Map, Value};

/// How a leaf's tolerance from the golden file should be interpreted.
///
/// The goldens encode both kinds: `tolerance_c`/`tolerance_db`/`tolerance_v`
/// are absolute (degC, dB, volts) while `*_tolerance_rel` is a fraction. The
/// in-crate tests apply them the same way, e.g. `(r.max_temp - golden).abs()
/// <= tol` for temperature but a relative bound for magnitude.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TolKind {
    /// Use the file's tolerance directly as an absolute bound.
    Absolute,
    /// Use it as a fraction of the value magnitude.
    Relative,
}

/// A single numeric leaf identified by its path from the document root.
#[derive(Clone, Debug, PartialEq)]
pub struct Leaf {
    /// Slash-separated path, e.g. `insertion_loss_db/100.0_MHz`. A segment may
    /// carry a bracket index, e.g. `ladder/shunt_C_F[0]`.
    pub path: &'static str,
    /// Recomputed reference value.
    pub value: f64,
    /// How to read the file's tolerance for this leaf.
    pub tol_kind: TolKind,
}

impl Leaf {
    /// Creates an absolutely-bounded leaf (temperature, dB, volts, ohms).
    pub fn new(path: &'static str, value: f64) -> Self {
        Self {
            path,
            value,
            tol_kind: TolKind::Absolute,
        }
    }

    /// Creates a relatively-bounded leaf, for dimensionless or very small
    /// quantities such as component values in farads.
    pub fn rel(path: &'static str, value: f64) -> Self {
        Self {
            path,
            value,
            tol_kind: TolKind::Relative,
        }
    }

    /// The largest allowed absolute difference from `current`, given the
    /// tolerance declared by the file.
    pub fn allowed(&self, current: f64, tol: f64) -> f64 {
        match self.tol_kind {
            TolKind::Absolute => tol,
            TolKind::Relative => tol * self.value.abs().max(current.abs()),
        }
    }
}

/// Splits a path into its segments.
///
/// Segments are separated by `/`. A segment of the form `key[3]` addresses
/// element 3 of the array (or object) at `key`; a segment may therefore be
/// `ladder/shunt_C_F[0]`. Bracket indices keep array elements reachable without
/// conflating them with object keys — `insertion_loss_db/10.0_MHz` is a key
/// containing dots and must not be split on them.
fn segments(path: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in path.split('/').filter(|s| !s.is_empty()) {
        match raw.split_once('[') {
            Some((key, idx)) => {
                out.push(key.to_string());
                let idx = idx.trim_end_matches(']');
                out.push(format!("#{idx}"));
            }
            None => out.push(raw.to_string()),
        }
    }
    out
}

/// Resolves one already-split segment.
fn resolve<'a>(cur: &'a Value, seg: &str) -> Option<&'a Value> {
    match seg.strip_prefix('#') {
        Some(i) => {
            let idx: usize = i.parse().ok()?;
            cur.get(idx)
        }
        None => cur.get(seg),
    }
}

/// Resolves one segment for writing, inserting an object/array as needed.
fn resolve_mut<'a>(cur: &'a mut Value, seg: &str) -> Result<&'a mut Value, String> {
    if let Some(i) = seg.strip_prefix('#') {
        let idx: usize = i.parse().map_err(|_| format!("bad array index {seg:?}"))?;
        let arr = cur
            .as_array_mut()
            .ok_or_else(|| format!("expected an array at {seg:?}"))?;
        return arr
            .get_mut(idx)
            .ok_or_else(|| format!("index {idx} out of range at {seg:?}"));
    }
    let obj = cur
        .as_object_mut()
        .ok_or_else(|| format!("expected an object at key {seg:?}"))?;
    if !obj.contains_key(seg) {
        obj.insert(seg.to_string(), Value::Object(Map::new()));
    }
    Ok(obj.get_mut(seg).expect("just inserted"))
}

/// Reads the value at `path`, if present and numeric.
pub fn get_number(doc: &Value, path: &str) -> Option<f64> {
    let mut cur = doc;
    for seg in segments(path) {
        cur = resolve(cur, &seg)?;
    }
    cur.as_f64()
}

/// Sets the numeric value at `path`.
///
/// Array slots must already exist: growing an array to reach index 7 would
/// silently invent `null` entries, so a bad path is reported instead.
pub fn set_number(doc: &mut Value, path: &str, value: f64) -> Result<(), String> {
    let segs = segments(path);
    if segs.is_empty() {
        return Err("empty path".to_string());
    }
    let (last, parents) = segs.split_last().expect("non-empty");
    let number = serde_json::Number::from_f64(value)
        .ok_or_else(|| format!("path {path:?}: {value} is not representable"))?;

    let mut cur = doc;
    for seg in parents {
        cur = resolve_mut(cur, seg).map_err(|e| format!("path {path:?}: {e}"))?;
    }
    let slot = resolve_mut(cur, last).map_err(|e| format!("path {path:?}: {e}"))?;
    match slot {
        v if v.is_number() => *v = Value::Number(number),
        v if v.is_null() => *v = Value::Number(number),
        other => {
            return Err(format!(
                "path {path:?}: existing value {other} is not a number; refusing to overwrite"
            ))
        }
    }
    Ok(())
}

/// Reads a document's top-level numeric tolerance, whichever spelling it uses.
///
/// Goldens name these `tolerance_db`, `tolerance_c`, `tolerance_v`,
/// `tolerance_ohm`, `tolerance_rel`, `magnitude_tolerance_rel`, … so we look
/// for any top-level key starting with `tolerance` (or containing
/// `tolerance`) whose value is a number.
pub fn tolerance(doc: &Value) -> Option<(String, f64)> {
    let obj = doc.as_object()?;
    for (k, v) in obj {
        if k.contains("tolerance") {
            if let Some(f) = v.as_f64() {
                return Some((k.clone(), f));
            }
        }
    }
    None
}

/// Serialises a document the way the checked-in goldens are formatted.
///
/// Key order is preserved because the crate enables serde_json's
/// `preserve_order` feature, so a rewrite only moves the values that actually
/// changed. The original line ending and trailing-newline convention are
/// reused so a rewrite does not turn into a whole-file diff on Windows.
pub fn to_string_pretty_like(doc: &Value, original: &str) -> String {
    let mut s = serde_json::to_string_pretty(doc).unwrap_or_else(|e| panic!("serialize: {e}"));
    // Reuse the source file's line ending and trailing-newline convention so a
    // rewrite does not turn into a whole-file diff on Windows.
    let nl = if original.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    if nl == "\r\n" {
        s = s.replace('\n', "\r\n");
    }
    if original.ends_with('\n') && !original.is_empty() && !s.ends_with(nl) {
        s.push_str(nl);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn round_trips_preserving_unknown_fields() {
        let src = r#"{
  "case": "x",
  "description": "keep me",
  "insertion_loss_db": {
    "10.0_MHz": -0.0003
  },
  "tolerance_db": 0.2
}
"#;
        let mut doc: Value = serde_json::from_str(src).expect("parse");
        set_number(&mut doc, "insertion_loss_db/10.0_MHz", -0.00034).expect("set");
        let out = to_string_pretty_like(&doc, src);
        assert!(out.contains("keep me"), "{out}");
        assert!(out.contains("\"tolerance_db\": 0.2"), "{out}");
        assert!(out.contains("-0.00034"), "{out}");
    }

    #[test]
    fn refuses_to_clobber_non_numeric_leaf() {
        let mut doc = json!({"a": "text"});
        let err = set_number(&mut doc, "a", 1.0).expect_err("must refuse");
        assert!(err.contains("not a number"), "{err}");
    }

    #[test]
    fn finds_tolerance_regardless_of_spelling() {
        let doc = json!({"magnitude_tolerance_rel": 0.01, "case": "y"});
        assert_eq!(
            tolerance(&doc),
            Some(("magnitude_tolerance_rel".into(), 0.01))
        );
    }

    #[test]
    fn reports_missing_paths_as_none() {
        let doc = json!({"a": {"b": 1.0}});
        assert_eq!(get_number(&doc, "a/b"), Some(1.0));
        assert_eq!(get_number(&doc, "a/z"), None);
        assert_eq!(get_number(&doc, "a/b/c"), None);
    }

    #[test]
    fn addresses_array_elements_with_bracket_syntax() {
        let mut doc = json!({
            "ladder": {"shunt_C_F": [1.0, 2.0, 3.0]},
            "insertion_loss_db": {"10.0_MHz": -0.5}
        });
        assert_eq!(get_number(&doc, "ladder/shunt_C_F[1]"), Some(2.0));
        // Keys containing dots must not be split.
        assert_eq!(get_number(&doc, "insertion_loss_db/10.0_MHz"), Some(-0.5));
        set_number(&mut doc, "ladder/shunt_C_F[2]", 30.0).expect("set");
        assert_eq!(doc["ladder"]["shunt_C_F"], json!([1.0, 2.0, 30.0]));
    }

    #[test]
    fn refuses_to_grow_arrays() {
        let mut doc = json!({"a": [1.0]});
        let err = set_number(&mut doc, "a[5]", 1.0).expect_err("must refuse");
        assert!(err.contains("out of range"), "{err}");
    }

    #[test]
    fn reuses_line_endings_and_trailing_newline() {
        let doc = json!({"a": 1.0, "b": "x"});
        // LF source with a trailing newline.
        let lf = to_string_pretty_like(&doc, "{\n  \"a\": 1.0\n}\n");
        assert!(lf.ends_with("}\n"), "{lf:?}");
        assert!(!lf.contains('\r'), "{lf:?}");
        // CRLF source with a trailing newline must not gain a bare CR.
        let crlf = to_string_pretty_like(&doc, "{\r\n  \"a\": 1.0\r\n}\r\n");
        assert!(crlf.ends_with("}\r\n"), "{crlf:?}");
        assert!(!crlf.contains("\r\r"), "{crlf:?}");
        // A source with no trailing newline stays that way.
        let none = to_string_pretty_like(&doc, "{}");
        assert!(!none.ends_with('\n'), "{none:?}");
    }

    #[test]
    fn preserves_key_order() {
        // `preserve_order` is what keeps rewrites to a minimal diff.
        let src = "{\n  \"z\": 1.0,\n  \"a\": 2.0\n}\n";
        let doc: Value = serde_json::from_str(src).expect("parse");
        let out = to_string_pretty_like(&doc, src);
        let z = out.find("\"z\"").expect("z");
        let a = out.find("\"a\"").expect("a");
        assert!(z < a, "key order changed:\n{out}");
    }

    #[test]
    fn formats_compactly() {
        assert_eq!(crate::num(1025.0), "1025");
        assert_eq!(crate::num(0.2), "0.2");
        assert!(crate::num(1.9673e-11).contains("e-11"));
        assert_eq!(crate::num(f64::NAN), "missing");
    }
}
