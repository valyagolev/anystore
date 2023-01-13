use std::fmt::Display;

use derive_more::{Display, From, IntoIterator};
use thiserror::Error;

use crate::address::{primitive::UniqueRootAddress, Address, PathAddress, SubAddress};

#[derive(From, Display, Debug, Error)]
pub struct JsonPathParseError(String);

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum JsonPathPart {
    Key(String),
    Index(usize),
}

impl JsonPathPart {
    pub fn to_key(&self) -> String {
        match self {
            JsonPathPart::Key(key) => key.clone(),
            JsonPathPart::Index(ix) => ix.to_string(),
        }
    }
}

impl Display for JsonPathPart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonPathPart::Key(key) => write!(f, ".{key}"),
            JsonPathPart::Index(ix) => write!(f, "[{ix}]"),
        }
    }
}

#[derive(Debug, Clone, Hash, IntoIterator, PartialEq, Eq, PartialOrd, Ord)]
pub struct JsonPath(#[into_iterator(owned, ref, ref_mut)] pub Vec<JsonPathPart>);

impl JsonPath {
    pub fn last(self) -> Option<JsonPathPart> {
        self.0.into_iter().last()
    }
}

impl Display for JsonPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = self.into_iter().map(|p| p.to_string()).collect::<String>();

        f.write_str(s.trim_start_matches('.'))
    }
}

impl Address for JsonPath {
    fn own_name(&self) -> String {
        match self.0.last() {
            None => "".to_owned(),
            Some(JsonPathPart::Index(i)) => format!("[{i}]"),
            Some(JsonPathPart::Key(s)) => format!(".{s}"),
        }
    }

    fn as_parts(&self) -> Vec<String> {
        self.0.iter().map(|v| v.to_string()).collect()
    }
}

impl From<UniqueRootAddress> for JsonPath {
    fn from(_: UniqueRootAddress) -> Self {
        JsonPath(vec![])
    }
}
impl SubAddress<JsonPathPart> for JsonPath {
    type Output = JsonPath;

    fn sub(self, rhs: JsonPathPart) -> Self::Output {
        let mut path = self.0;
        path.push(rhs);
        JsonPath(path)
    }
}
impl SubAddress<JsonPath> for JsonPath {
    type Output = JsonPath;

    fn sub(self, rhs: JsonPath) -> Self::Output {
        let mut path = self.0;
        path.extend(rhs.0);
        JsonPath(path)
    }
}

impl PathAddress for JsonPath {
    type Error = JsonPathParseError;

    type Output = JsonPath;

    fn path(self, str: &str) -> Result<Self::Output, Self::Error> {
        let keys =
            str.split('.')
                .map(|chunk| {
                    let mut chars: Vec<char> = chunk.chars().collect();
                    let mut keys: Vec<JsonPathPart> = vec![];

                    'eatindex: while chars.last() == Some(&']') {
                        chars.pop();

                        let mut ix = vec![];
                        loop {
                            let chr = chars
                                .pop()
                                .ok_or(JsonPathParseError("mismatched ]".to_string()))?;

                            if chr == '[' {
                                keys.push(JsonPathPart::Index(
                                    ix.into_iter().rev().collect::<String>().parse().map_err(
                                        |_| JsonPathParseError("error parsing index".to_string()),
                                    )?,
                                ));
                                continue 'eatindex;
                            } else {
                                ix.push(chr);
                            }
                        }
                    }

                    if !chars.is_empty() {
                        keys.push(JsonPathPart::Key(chars.into_iter().collect()));
                    }

                    Ok(keys.into_iter().rev())
                })
                .collect::<Result<Vec<_>, JsonPathParseError>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();

        Ok(self.sub(JsonPath(keys)))
    }
}

impl From<JsonPath> for String {
    fn from(value: JsonPath) -> Self {
        value.to_string()
    }
}

impl From<JsonPathPart> for String {
    fn from(value: JsonPathPart) -> Self {
        value.to_string()
    }
}
