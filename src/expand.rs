use rustc_hash::FxHashMap;

use crate::error::{MoldError, Result};
use crate::parse::{BuildFile, DepFormat};

pub struct ExpandCtx<'a> {
    pub file: &'a str,
    pub tools: &'a FxHashMap<String, String>,
    pub flags: &'a FxHashMap<String, Vec<String>>,
    pub vars: &'a FxHashMap<String, String>,
    pub arrays: &'a FxHashMap<String, Vec<String>>,
    pub specials: &'a FxHashMap<String, String>,
}

impl<'a> ExpandCtx<'a> {
    pub fn from_build(build: &'a BuildFile, specials: &'a FxHashMap<String, String>) -> Self {
        Self {
            file: &build.file,
            tools: &build.tools,
            flags: &build.flags,
            vars: &build.vars,
            arrays: &build.arrays,
            specials,
        }
    }

    fn lookup(&self, name: &str) -> Result<String> {
        if let Some(v) = self.specials.get(name) {
            return Ok(v.clone());
        }
        if let Some(v) = self.tools.get(name) {
            return Ok(v.clone());
        }
        if let Some(v) = self.flags.get(name) {
            return Ok(shell_join(v));
        }
        if let Some(v) = self.vars.get(name) {
            return Ok(v.clone());
        }
        if let Some(v) = self.arrays.get(name) {
            return Ok(shell_join(v));
        }
        Err(MoldError::build(format!("unknown variable '{name}'")))
    }
}

pub fn expand(input: &str, ctx: &ExpandCtx<'_>) -> Result<String> {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        if i + 1 < chars.len() && chars[i + 1] == '$' {
            out.push('$');
            i += 2;
            continue;
        }
        if i + 1 >= chars.len() {
            out.push('$');
            break;
        }
        let (name, next) = parse_ref(&chars, i + 1)?;
        if name.is_empty() {
            out.push('$');
            i = next;
            continue;
        }
        out.push_str(&ctx.lookup(&name)?);
        i = next;
    }
    Ok(out)
}

fn parse_ref(chars: &[char], mut i: usize) -> Result<(String, usize)> {
    if i >= chars.len() {
        return Err(MoldError::build("dangling '$' in expansion".to_string()));
    }
    let closer = match chars[i] {
        '(' => Some(')'),
        '{' => Some('}'),
        _ => None,
    };
    if let Some(end) = closer {
        i += 1;
        let start = i;
        while i < chars.len() && chars[i] != end {
            if !is_ident_char(chars[i], i == start) {
                return Err(MoldError::build(format!(
                    "invalid variable name in '${}...'",
                    if end == ')' { '(' } else { '{' }
                )));
            }
            i += 1;
        }
        if i >= chars.len() || chars[i] != end {
            return Err(MoldError::build("unclosed variable reference".to_string()));
        }
        let name: String = chars[start..i].iter().collect();
        if name.is_empty() {
            return Err(MoldError::build("empty variable reference".to_string()));
        }
        Ok((name, i + 1))
    } else {
        if !is_ident_char(chars[i], true) {
            return Ok((String::new(), i));
        }
        let start = i;
        i += 1;
        while i < chars.len() && is_ident_char(chars[i], false) {
            i += 1;
        }
        let name: String = chars[start..i].iter().collect();
        Ok((name, i))
    }
}

fn is_ident_char(c: char, first: bool) -> bool {
    if first {
        c.is_ascii_alphabetic() || c == '_'
    } else {
        c.is_ascii_alphanumeric() || c == '_'
    }
}

pub fn shell_quote(s: &str) -> String {
    if s.is_empty() {
        return "''".to_string();
    }
    let safe = s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | '+' | '=' | ':' | '@' | '%'));
    if safe {
        return s.to_string();
    }
    let mut out = String::from("'");
    for c in s.chars() {
        if c == '\'' {
            out.push_str("'\\''");
        } else {
            out.push(c);
        }
    }
    out.push('\'');
    out
}

pub fn shell_join(parts: &[String]) -> String {
    parts
        .iter()
        .map(|p| shell_quote(p))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn hash_command(cmd: &str) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for b in cmd.as_bytes() {
        hash ^= *b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

pub fn default_depfile(out: &str, depformat: DepFormat) -> Option<String> {
    match depformat {
        DepFormat::Gcc => Some(format!("{out}.d")),
        DepFormat::None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustc_hash::FxHashMap;

    fn ctx<'a>(
        tools: &'a FxHashMap<String, String>,
        flags: &'a FxHashMap<String, Vec<String>>,
        vars: &'a FxHashMap<String, String>,
        arrays: &'a FxHashMap<String, Vec<String>>,
        specials: &'a FxHashMap<String, String>,
    ) -> ExpandCtx<'a> {
        ExpandCtx {
            file: "build.mold",
            tools,
            flags,
            vars,
            arrays,
            specials,
        }
    }

    #[test]
    fn expands_tools_flags_and_specials() {
        let mut tools = FxHashMap::default();
        tools.insert("cc".into(), "clang".into());
        let mut flags = FxHashMap::default();
        flags.insert(
            "cflags".into(),
            vec!["-Iinclude".into(), "-Wall".into(), "-O2".into()],
        );
        let vars = FxHashMap::default();
        let arrays = FxHashMap::default();
        let mut specials = FxHashMap::default();
        specials.insert("in".into(), "src/main.c".into());
        specials.insert("out".into(), "build/main.o".into());
        specials.insert("depfile".into(), "build/main.o.d".into());
        let c = ctx(&tools, &flags, &vars, &arrays, &specials);
        let got = expand(
            "$(cc) $(cflags) -MMD -MF $depfile -c $in -o $out",
            &c,
        )
        .unwrap();
        assert_eq!(
            got,
            "clang -Iinclude -Wall -O2 -MMD -MF build/main.o.d -c src/main.c -o build/main.o"
        );
    }

    #[test]
    fn dollar_dollar_is_literal() {
        let tools = FxHashMap::default();
        let flags = FxHashMap::default();
        let vars = FxHashMap::default();
        let arrays = FxHashMap::default();
        let specials = FxHashMap::default();
        let c = ctx(&tools, &flags, &vars, &arrays, &specials);
        assert_eq!(expand("price $$5", &c).unwrap(), "price $5");
    }

    #[test]
    fn unknown_var_errors() {
        let tools = FxHashMap::default();
        let flags = FxHashMap::default();
        let vars = FxHashMap::default();
        let arrays = FxHashMap::default();
        let specials = FxHashMap::default();
        let c = ctx(&tools, &flags, &vars, &arrays, &specials);
        assert!(expand("$(nope)", &c).is_err());
    }
}
