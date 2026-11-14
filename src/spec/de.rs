//! Wide-but-typed deserializers: proportions, bar magnitudes, and `ratio`
//! weights arrive as numbers or numeric strings, so a live fumble costs no
//! push round. `parse_tracked` keeps the failing field path.

/// A structural proportion arriving as either a JSON number or a string.
///
/// `w`/`h` are percentages of the parent — `18`, `"18"`, and `"18%"` all mean
/// the same share. Models reach for the bare number first (observed in two
/// consecutive live runs); rejecting it costs a whole push round to repeat
/// what they already meant.
pub(super) fn de_opt_proportion<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct ProportionVisitor;

    impl<'de> serde::de::Visitor<'de> for ProportionVisitor {
        type Value = Option<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a proportion (number or string, like 18 or \"18%\")")
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_some<D2>(self, deserializer: D2) -> Result<Self::Value, D2::Error>
        where
            D2: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(ProportionVisitor)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value.to_string()))
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value.to_string()))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value.to_string()))
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value.to_string()))
        }
    }

    deserializer.deserialize_any(ProportionVisitor)
}

/// A bar magnitude: `3`, `3.5`, `"3.5"`, and `"42%"` all mean the same number.
/// Rankings are scored with decimals and percents (same writing accident as
/// `ratio` weights and `h` shares — observed live); rejecting the decimal
/// costs a whole push round to retype what they already meant.
pub(super) fn de_opt_number<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct NumberVisitor;

    impl<'de> serde::de::Visitor<'de> for NumberVisitor {
        type Value = Option<f64>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a bar value (number or numeric string, like 3.5 or \"42%\")")
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(None)
        }

        fn visit_some<D2>(self, deserializer: D2) -> Result<Self::Value, D2::Error>
        where
            D2: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(NumberVisitor)
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            let number = value.trim().strip_suffix('%').unwrap_or(value.trim());
            match number.parse::<f64>() {
                Ok(number) if number.is_finite() => Ok(Some(number)),
                _ => Err(E::invalid_value(serde::de::Unexpected::Str(value), &self)),
            }
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value as f64))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(value as f64))
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            if value.is_finite() {
                Ok(Some(value))
            } else {
                Err(E::invalid_value(serde::de::Unexpected::Float(value), &self))
            }
        }
    }

    deserializer.deserialize_any(NumberVisitor)
}

/// Canonical track weight: `3`, `"3"`, and `3.0` all print as `3`, while a
/// finer split keeps its decimals — `minmax(0, 3fr)` / `minmax(0, 0.7fr)`.
fn weight_text(value: f64) -> String {
    let text = format!("{value}");
    if let Some(stem) = text.strip_suffix(".0") {
        stem.to_string()
    } else {
        text
    }
}

/// `ratio` weights arriving as numbers or numeric strings.
///
/// Weights are proportions, not integers — `ratio: [1, 5, 0.7]` is the model
/// reaching for a finer split than whole numbers allow (observed live).
/// Rejecting the decimal cost a whole push round to re-type it as `[3, 14, 1]`.
pub(super) fn de_weights<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct WeightsVisitor;

    impl<'de> serde::de::Visitor<'de> for WeightsVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a list of weights (numbers or numeric strings, like [1, 0.7])")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: serde::de::SeqAccess<'de>,
        {
            let mut out = Vec::new();
            while let Some(weight) = seq.next_element_seed(WeightSeed)? {
                out.push(weight);
            }
            Ok(out)
        }
    }

    struct WeightSeed;
    impl<'de> serde::de::DeserializeSeed<'de> for WeightSeed {
        type Value = String;

        fn deserialize<D2>(self, deserializer: D2) -> Result<Self::Value, D2::Error>
        where
            D2: serde::Deserializer<'de>,
        {
            deserializer.deserialize_any(WeightVisitor)
        }
    }

    struct WeightVisitor;
    impl<'de> serde::de::Visitor<'de> for WeightVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a weight (a number like 1 or 0.7)")
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(weight_text(value as f64))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(weight_text(value as f64))
        }

        fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(weight_text(value))
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            match value.trim().parse::<f64>() {
                Ok(number) => Ok(weight_text(number)),
                Err(_) => Err(E::invalid_value(serde::de::Unexpected::Str(value), &self)),
            }
        }
    }

    deserializer.deserialize_seq(WeightsVisitor)
}

/// A deserialize failure with the field path serde gave up at.
pub(crate) struct FieldError {
    pub(crate) path: String,
    pub(crate) message: String,
}

impl std::fmt::Display for FieldError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.path.is_empty() || self.path == "." {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// Deserialize `value` into `T`, keeping the failing field path. A schema
/// fumble must name the field: a pathless "expected u32" makes the model guess
/// and re-push the whole screen (observed: two retries per wrong field).
pub(crate) fn parse_tracked<T: serde::de::DeserializeOwned>(
    value: &serde_json::Value,
) -> Result<T, FieldError> {
    use serde::de::IntoDeserializer;
    let de = value.clone().into_deserializer();
    match serde_path_to_error::deserialize(de) {
        Ok(parsed) => Ok(parsed),
        Err(err) => {
            let path = err.path().to_string();
            let inner = err.into_inner();
            let path = if path == "." { String::new() } else { path };
            Err(FieldError {
                path,
                message: inner.to_string(),
            })
        }
    }
}
