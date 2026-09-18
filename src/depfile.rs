pub fn parse_depfile(text: &str, output: &str) -> Vec<String> {
    let logical = unfold(text);
    let mut matched = Vec::new();
    let mut fallback = Vec::new();
    let out_norm = norm(output);

    for line in logical.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some(colon) = find_unescaped_colon(line) else {
            continue;
        };
        let targets = split_make_words(&line[..colon]);
        let deps = split_make_words(&line[colon + 1..]);
        fallback.extend(deps.iter().cloned());
        if targets.iter().any(|t| norm(t) == out_norm) {
            matched.extend(deps);
        }
    }

    let mut src = if matched.is_empty() { fallback } else { matched };
    src.retain(|d| !d.is_empty() && norm(d) != out_norm);
    src.sort();
    src.dedup();
    src
}

fn unfold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'\r' {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'\n' {
                out.push(' ');
                i = j + 1;
                continue;
            }
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

fn find_unescaped_colon(line: &str) -> Option<usize> {
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' {
            i += 2;
            continue;
        }
        if b[i] == b':' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn split_make_words(s: &str) -> Vec<String> {
    let b = s.as_bytes();
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b' ' | b'\t' => {
                if !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                }
                i += 1;
            }
            b'\\' if i + 1 < b.len() => {
                cur.push(b[i + 1] as char);
                i += 2;
            }
            c => {
                cur.push(c as char);
                i += 1;
            }
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
}

fn norm(p: &str) -> String {
    p.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_rule() {
        let deps = parse_depfile("build/main.o: src/main.c include/lol.h\n", "build/main.o");
        assert_eq!(deps, vec!["include/lol.h", "src/main.c"]);
    }

    #[test]
    fn continuations_and_escapes() {
        let text = r#"build/main.o: src/main.c \
  include/foo.h \
  include/bar\ baz.h
"#;
        let deps = parse_depfile(text, "build/main.o");
        assert_eq!(
            deps,
            vec!["include/bar baz.h", "include/foo.h", "src/main.c"]
        );
    }

    #[test]
    fn ignores_self_target() {
        let deps = parse_depfile("a.o: a.o src/a.c\n", "a.o");
        assert_eq!(deps, vec!["src/a.c"]);
    }
}
