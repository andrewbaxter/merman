use serde::{
    Deserialize,
    Serialize,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reference {
    pub id: Option<i64>,
    pub path: Vec<Segment>,
    pub range: Option<(usize, usize)>,
}

impl Reference {
    pub fn reference_format(&self) -> String {
        let mut out = String::from("#");
        if let Some(id) = self.id {
            out.push_str(&id.to_string());
        } else if self.path.is_empty() {
            out.push('.');
        }
        for segment in &self.path {
            match segment {
                Segment::Key(key) => {
                    let identifier =
                        !key.is_empty() &&
                            key
                                .chars()
                                .enumerate()
                                .all(|(i, c)| c == '_' || c.is_ascii_alphabetic() || (i > 0 && c.is_ascii_digit()));
                    if identifier {
                        out.push('.');
                        out.push_str(key);
                    } else {
                        out.push_str(&format!(".[{}]", serde_json::to_string(key).unwrap()));
                    }
                },
                Segment::Index(index) => out.push_str(&format!("[{}]", index)),
            }
        }
        if let Some((begin, end)) = self.range {
            out.push_str(&format!("[{}:{}]", begin, end));
        }
        return out;
    }

    pub fn reference_parse(text: &str) -> Result<Reference, String> {
        let text = text.strip_prefix('#').unwrap_or(text);
        let id_len = text.chars().take_while(|c| c.is_ascii_digit()).count();
        let id = if id_len == 0 {
            None
        } else {
            Some(text[..id_len].parse::<i64>().map_err(|e| format!("Bad element id: {}", e))?)
        };
        let mut rest = &text[id_len..];
        if id.is_none() && rest == "." {
            rest = "";
        }
        let mut path = vec![];
        let mut range = None;
        while !rest.is_empty() {
            if range.is_some() {
                return Err(format!("A slice must come last, found `{}` after it", rest));
            }
            if let Some(after) = rest.strip_prefix(".[") {
                let string_end = 'end: {
                    let mut chars = after.char_indices();
                    let Some((_, '"')) = chars.next() else {
                        break 'end None;
                    };
                    let mut escaped = false;
                    for (i, c) in chars {
                        if escaped {
                            escaped = false;
                        } else if c == '\\' {
                            escaped = true;
                        } else if c == '"' {
                            break 'end Some(i + 1);
                        }
                    }
                    None
                }.ok_or_else(|| format!("Unterminated key string in `{}`", rest))?;
                let key: String =
                    serde_json::from_str(&after[..string_end]).map_err(|e| format!("Bad key string: {}", e))?;
                let after = &after[string_end..];
                rest = after.strip_prefix(']').ok_or_else(|| format!("Expected `]` in `{}`", rest))?;
                path.push(Segment::Key(key));
            } else if let Some(after) = rest.strip_prefix('.') {
                let len = after.chars().take_while(|c| *c == '_' || c.is_ascii_alphanumeric()).count();
                if len == 0 || after.as_bytes()[0].is_ascii_digit() {
                    return Err(format!("Expected a key name in `{}`", rest));
                }
                path.push(Segment::Key(after[..len].to_string()));
                rest = &after[len..];
            } else if let Some(after) = rest.strip_prefix('[') {
                let close = after.find(']').ok_or_else(|| format!("Expected `]` in `{}`", rest))?;
                let inside = &after[..close];
                match inside.split_once(':') {
                    Some((begin, end)) => {
                        let begin =
                            begin.parse::<usize>().map_err(|e| format!("Bad slice start `{}`: {}", begin, e))?;
                        let end = end.parse::<usize>().map_err(|e| format!("Bad slice end `{}`: {}", end, e))?;
                        if end < begin {
                            return Err(format!("Slice end {} is before its start {}", end, begin));
                        }
                        range = Some((begin, end));
                    },
                    None => {
                        path.push(
                            Segment::Index(
                                inside.parse::<usize>().map_err(|e| format!("Bad index `{}`: {}", inside, e))?,
                            ),
                        );
                    },
                }
                rest = &after[close + 1..];
            } else {
                return Err(format!("Expected `.` or `[` in `{}`", rest));
            }
        }
        return Ok(Reference {
            id: id,
            path: path,
            range: range,
        });
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Segment {
    Index(usize),
    Key(String),
}
