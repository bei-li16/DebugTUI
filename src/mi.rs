//! Incremental GDB/MI record parsing. Values preserve duplicate keys and order.
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Value {
    Text(String),
    Tuple(Vec<(String, Value)>),
    List(Vec<Value>),
}

impl Value {
    pub fn text(&self) -> &str {
        if let Self::Text(s) = self { s } else { "" }
    }
    pub fn field(&self, key: &str) -> Option<&Value> {
        if let Self::Tuple(v) = self {
            v.iter().find(|(k, _)| k == key).map(|(_, v)| v)
        } else {
            None
        }
    }
    pub fn string(&self, key: &str) -> String {
        self.field(key)
            .map(Self::text)
            .unwrap_or_default()
            .to_owned()
    }
    pub fn items(&self) -> &[Value] {
        if let Self::List(v) = self { v } else { &[] }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Record {
    pub token: Option<u64>,
    pub kind: char,
    pub class: String,
    pub data: Value,
}

pub fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\{:03o}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn parse(line: &str) -> Result<Option<Record>, String> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.trim().is_empty() || line.trim() == "(gdb)" {
        return Ok(None);
    }
    let mut p = Parser {
        b: line.as_bytes(),
        i: 0,
    };
    let start = p.i;
    while p.peek().is_some_and(|c| c.is_ascii_digit()) {
        p.i += 1;
    }
    let token = if p.i > start {
        Some(line[start..p.i].parse().map_err(|_| "invalid token")?)
    } else {
        None
    };
    let kind = p.take().ok_or("missing record kind")? as char;
    if matches!(kind, '~' | '@' | '&') {
        let text = p.string()?;
        if p.i != p.b.len() {
            return Err("trailing stream data".into());
        }
        return Ok(Some(Record {
            token,
            kind,
            class: String::new(),
            data: Value::Text(text),
        }));
    }
    if !matches!(kind, '^' | '*' | '+' | '=') {
        return Err(format!("not an MI record: {line}"));
    }
    let class = p.word()?;
    let mut fields = Vec::new();
    while p.peek() == Some(b',') {
        p.i += 1;
        fields.push(p.result(0)?);
    }
    if p.i != p.b.len() {
        return Err(format!("trailing data at {}", p.i));
    }
    Ok(Some(Record {
        token,
        kind,
        class,
        data: Value::Tuple(fields),
    }))
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
}
impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.b.get(self.i).copied()
    }
    fn take(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.i += 1;
        Some(c)
    }
    fn expect(&mut self, c: u8) -> Result<(), String> {
        if self.take() == Some(c) {
            Ok(())
        } else {
            Err(format!("expected {} at {}", c as char, self.i))
        }
    }
    fn word(&mut self) -> Result<String, String> {
        let start = self.i;
        while self
            .peek()
            .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
        {
            self.i += 1;
        }
        if start == self.i {
            return Err(format!("expected identifier at {}", self.i));
        }
        Ok(String::from_utf8_lossy(&self.b[start..self.i]).into_owned())
    }
    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"')?;
        let mut out = Vec::new();
        loop {
            match self.take().ok_or("unterminated MI string")? {
                b'"' => return Ok(String::from_utf8_lossy(&out).into_owned()),
                b'\\' => {
                    let c = self.take().ok_or("trailing escape")?;
                    match c {
                        b'n' => out.push(b'\n'),
                        b'r' => out.push(b'\r'),
                        b't' => out.push(b'\t'),
                        b'a' => out.push(7),
                        b'b' => out.push(8),
                        b'f' => out.push(12),
                        b'v' => out.push(11),
                        b'0'..=b'7' => {
                            let mut n = (c - b'0') as u16;
                            for _ in 0..2 {
                                if let Some(d @ b'0'..=b'7') = self.peek() {
                                    self.i += 1;
                                    n = n * 8 + (d - b'0') as u16;
                                } else {
                                    break;
                                }
                            }
                            out.push(n as u8);
                        }
                        _ => out.push(c),
                    }
                }
                c => out.push(c),
            }
        }
    }
    fn result(&mut self, depth: usize) -> Result<(String, Value), String> {
        let key = self.word()?;
        self.expect(b'=')?;
        Ok((key, self.value(depth + 1)?))
    }
    fn value(&mut self, depth: usize) -> Result<Value, String> {
        if depth > 64 {
            return Err("MI nesting limit exceeded".into());
        }
        match self.peek() {
            Some(b'"') => Ok(Value::Text(self.string()?)),
            Some(b'{') => {
                self.i += 1;
                let mut v = Vec::new();
                while self.peek() != Some(b'}') {
                    v.push(self.result(depth + 1)?);
                    if self.peek() != Some(b',') {
                        break;
                    }
                    self.i += 1;
                }
                self.expect(b'}')?;
                Ok(Value::Tuple(v))
            }
            Some(b'[') => {
                self.i += 1;
                let mut v = Vec::new();
                while self.peek() != Some(b']') {
                    if matches!(self.peek(), Some(b'"' | b'{' | b'[')) {
                        v.push(self.value(depth + 1)?);
                    } else {
                        v.push(Value::Tuple(vec![self.result(depth + 1)?]));
                    }
                    if self.peek() != Some(b',') {
                        break;
                    }
                    self.i += 1;
                }
                self.expect(b']')?;
                Ok(Value::List(v))
            }
            _ => Err(format!("invalid value at {}", self.i)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_stack_and_duplicate_keys() {
        let r = parse(r#"42^done,stack=[frame={level="0",func="main"},frame={level="1",func="boot"}],x="a",x="b""#).unwrap().unwrap();
        assert_eq!(r.token, Some(42));
        assert_eq!(
            r.data.field("stack").unwrap().items()[1]
                .field("frame")
                .unwrap()
                .string("func"),
            "boot"
        );
        if let Value::Tuple(v) = r.data {
            assert_eq!(v.len(), 3);
        } else {
            panic!()
        }
    }
    #[test]
    fn octal_utf8_and_escapes() {
        let r = parse(r#"~"\344\270\255\346\226\207\nC:\\dir\\a.c\"""#)
            .unwrap()
            .unwrap();
        assert_eq!(r.data.text(), "中文\nC:\\dir\\a.c\"");
        let original = "中文 C:\\a b\"\n\t";
        let r = parse(&format!("~{}", quote(original))).unwrap().unwrap();
        assert_eq!(r.data.text(), original);
    }
    #[test]
    fn stopped_is_not_a_command_result() {
        let r = parse(r#"*stopped,reason="watchpoint-trigger",wpt={number="2",exp="flag"},value={old="1",new="0"}"#).unwrap().unwrap();
        assert_eq!(r.kind, '*');
        assert_eq!(r.token, None);
        assert_eq!(r.data.field("value").unwrap().string("new"), "0");
    }
    #[test]
    fn reject_truncation_and_trailing_data() {
        for s in [
            "1^done,x=",
            "1^done,x=\"bad",
            "1^done,x=[]garbage",
            "1^done,x={a=\"b\"",
        ] {
            assert!(parse(s).is_err(), "{s}");
        }
        assert!(parse("(gdb) ").unwrap().is_none());
    }
}
