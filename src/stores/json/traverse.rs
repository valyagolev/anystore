use derive_more::{Display, From};

use serde_json::Value;
use thiserror::Error;

use crate::stores::json::paths::*;

#[derive(From, Display, Debug, Error)]
pub enum JsonTraverseError {
    Serde(serde_json::Error),
    // ParseError(JsonPathParseError),
    Custom(String),
}

pub fn get_mut_subvalue<'a>(
    cur: &'a mut Value,
    next: &JsonPathPart,
    create_on_miss: bool,
) -> Result<Option<&'a mut Value>, JsonTraverseError> {
    match next {
        JsonPathPart::Key(key) => {
            if cur.is_null() {
                if !create_on_miss {
                    return Ok(None);
                }

                *cur = Value::Object(Default::default())
            }

            match cur {
                Value::Object(map) => {
                    if !map.contains_key(key) {
                        map.insert(key.to_owned(), Value::Null);
                    }

                    Ok(Some(&mut map[key]))
                }
                _ => return Err(format!("Incompatible value at key {next} of {cur}",).into()),
            }
        }
        JsonPathPart::Index(ix) => {
            if cur.is_null() {
                if !create_on_miss {
                    return Ok(None);
                }

                *cur = Value::Array(vec![]);
            }

            match cur {
                Value::Array(arr) => {
                    if !create_on_miss && arr.len() < *ix {
                        return Ok(None);
                    } else {
                        for _ in arr.len()..ix + 1 {
                            arr.push(Value::Null);
                        }
                    }

                    Ok(Some(&mut arr[*ix]))
                }
                _ => return Err(format!("Incompatible value at key {next} of {cur}",).into()),
            }
        }
    }
}

pub fn get_mut_pathvalue<'a>(
    cur: &'a mut Value,
    path: &[JsonPathPart],
    create_on_miss: bool,
) -> Result<Option<&'a mut Value>, JsonTraverseError> {
    let mut c = cur;

    for p in path {
        c = match get_mut_subvalue(c, p, create_on_miss)? {
            Some(c) => c,
            None => return Ok(None),
        };
    }

    Ok(Some(c))
}

pub fn get_subvalue<'a>(
    cur: &'a Value,
    next: &JsonPathPart,
) -> Result<Option<&'a Value>, JsonTraverseError> {
    match next {
        JsonPathPart::Key(key) => {
            if cur.is_null() {
                return Ok(None);
            }

            match cur {
                Value::Object(map) => {
                    if !map.contains_key(key) {
                        return Ok(None);
                    }

                    Ok(Some(&map[key]))
                }
                _ => return Err(format!("Incompatible value at key {next} of {cur}",).into()),
            }
        }
        JsonPathPart::Index(ix) => {
            if cur.is_null() {
                return Ok(None);
            }

            match cur {
                Value::Array(arr) => {
                    if arr.len() < *ix {
                        return Ok(None);
                    }

                    Ok(Some(&arr[*ix]))
                }
                _ => return Err(format!("Incompatible value at key {next} of {cur}",).into()),
            }
        }
    }
}

pub fn get_pathvalue<'a>(
    cur: &'a Value,
    path: &[JsonPathPart],
) -> Result<Option<&'a Value>, JsonTraverseError> {
    let mut c = cur;

    for p in path {
        c = match get_subvalue(c, p)? {
            Some(c) => c,
            None => return Ok(None),
        };
    }

    Ok(Some(c))
}
