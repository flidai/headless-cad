//! Direct typed fields remain compatible with serde_json/arbitrary_precision.
use crate::{Constraint, ConstraintKind};
use serde::{
    de::{Error, MapAccess, Visitor},
    Deserialize,
};
impl<'de> Deserialize<'de> for Constraint {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Constraint;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a typed sketch constraint")
            }
            fn visit_map<M: MapAccess<'de>>(self, mut map: M) -> Result<Constraint, M::Error> {
                let (mut id, mut kind, mut point, mut position, mut a, mut b, mut value) =
                    (None, None, None, None, None, None, None);
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "id" => {
                            if id.is_some() {
                                return Err(M::Error::duplicate_field("id"));
                            }
                            id = Some(map.next_value::<String>()?);
                        }
                        "kind" => {
                            if kind.is_some() {
                                return Err(M::Error::duplicate_field("kind"));
                            }
                            kind = Some(map.next_value::<String>()?);
                        }
                        "point" => {
                            if point.is_some() {
                                return Err(M::Error::duplicate_field("point"));
                            }
                            point = Some(map.next_value::<String>()?);
                        }
                        "position" => {
                            if position.is_some() {
                                return Err(M::Error::duplicate_field("position"));
                            }
                            position = Some(map.next_value::<[f64; 2]>()?);
                        }
                        "a" => {
                            if a.is_some() {
                                return Err(M::Error::duplicate_field("a"));
                            }
                            a = Some(map.next_value::<String>()?);
                        }
                        "b" => {
                            if b.is_some() {
                                return Err(M::Error::duplicate_field("b"));
                            }
                            b = Some(map.next_value::<String>()?);
                        }
                        "value" => {
                            if value.is_some() {
                                return Err(M::Error::duplicate_field("value"));
                            }
                            value = Some(map.next_value::<f64>()?);
                        }
                        _ => {
                            return Err(M::Error::unknown_field(
                                &key,
                                &["id", "kind", "point", "position", "a", "b", "value"],
                            ))
                        }
                    }
                }
                let id = id.ok_or_else(|| M::Error::missing_field("id"))?;
                let kind = kind.ok_or_else(|| M::Error::missing_field("kind"))?;
                let invalid = || M::Error::custom("fields do not match constraint kind");
                let kind = if kind == "fixed" {
                    if a.is_some() || b.is_some() || value.is_some() {
                        return Err(invalid());
                    }
                    ConstraintKind::Fixed {
                        point: point.ok_or_else(|| M::Error::missing_field("point"))?,
                        position: position.ok_or_else(|| M::Error::missing_field("position"))?,
                    }
                } else {
                    if point.is_some() || position.is_some() {
                        return Err(invalid());
                    }
                    let a = a.ok_or_else(|| M::Error::missing_field("a"))?;
                    let b = b.ok_or_else(|| M::Error::missing_field("b"))?;
                    match kind.as_str() {
                        "distance" => ConstraintKind::Distance {
                            a,
                            b,
                            value: value.ok_or_else(|| M::Error::missing_field("value"))?,
                        },
                        "coincident" if value.is_none() => ConstraintKind::Coincident { a, b },
                        "horizontal" if value.is_none() => ConstraintKind::Horizontal { a, b },
                        "vertical" if value.is_none() => ConstraintKind::Vertical { a, b },
                        _ => return Err(invalid()),
                    }
                };
                Ok(Constraint { id, kind })
            }
        }
        d.deserialize_map(V)
    }
}
