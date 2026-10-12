//! Direct typed deserialization avoids Serde Content's f64 incompatibility with
//! serde_json/arbitrary_precision (enabled by the Starlark frontend).
use crate::PathSegment;
use serde::Deserialize;

impl<'de> Deserialize<'de> for PathSegment {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = PathSegment;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a line_to or arc_to path segment")
            }
            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                use serde::de::Error;
                let (mut kind, mut end, mut center, mut clockwise) = (None, None, None, None);
                while let Some(key) = map.next_key::<String>()? {
                    match key.as_str() {
                        "kind" => {
                            if kind.is_some() {
                                return Err(M::Error::duplicate_field("kind"));
                            }
                            kind = Some(map.next_value::<String>()?);
                        }
                        "end" => {
                            if end.is_some() {
                                return Err(M::Error::duplicate_field("end"));
                            }
                            end = Some(map.next_value::<[f64; 2]>()?);
                        }
                        "center" => {
                            if center.is_some() {
                                return Err(M::Error::duplicate_field("center"));
                            }
                            center = Some(map.next_value::<[f64; 2]>()?);
                        }
                        "clockwise" => {
                            if clockwise.is_some() {
                                return Err(M::Error::duplicate_field("clockwise"));
                            }
                            clockwise = Some(map.next_value::<bool>()?);
                        }
                        _ => {
                            return Err(M::Error::unknown_field(
                                &key,
                                &["kind", "end", "center", "clockwise"],
                            ))
                        }
                    }
                }
                let kind = kind.ok_or_else(|| M::Error::missing_field("kind"))?;
                let end = end.ok_or_else(|| M::Error::missing_field("end"))?;
                match kind.as_str() {
                    "line_to" => {
                        if center.is_some() {
                            return Err(M::Error::unknown_field("center", &["kind", "end"]));
                        }
                        if clockwise.is_some() {
                            return Err(M::Error::unknown_field("clockwise", &["kind", "end"]));
                        }
                        Ok(PathSegment::LineTo { end })
                    }
                    "arc_to" => Ok(PathSegment::ArcTo {
                        end,
                        center: center.ok_or_else(|| M::Error::missing_field("center"))?,
                        clockwise: clockwise.unwrap_or(false),
                    }),
                    _ => Err(M::Error::unknown_variant(&kind, &["line_to", "arc_to"])),
                }
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}
