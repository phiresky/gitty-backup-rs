pub mod serde_compact_osstr {
    use serde::Deserialize;
    use serde::Deserializer;
    use serde::Serialize;
    use serde::Serializer;
    use std::ffi::OsStr;
    use std::ffi::OsString;
    pub fn serialize<S>(s: &OsStr, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        if let Some(s) = s.to_str() {
            serializer.serialize_str(s)
        } else {
            OsStr::serialize(s, serializer)
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<OsString, D::Error>
    where
        D: Deserializer<'de>,
    {
        String::deserialize(deserializer).and_then(|string| Ok(OsString::from(string)))
    }
}
