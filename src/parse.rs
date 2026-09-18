use std::fs;
use std::path::{Path, PathBuf};

use rustc_hash::FxHashMap;

use crate::error::{MoldError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DepFormat {
    None,
    Gcc,
}

#[derive(Debug, Clone)]
pub struct Instruction {
    pub name: String,
    pub command: String,
    pub description: Option<String>,
    pub depformat: DepFormat,
    pub depfile: Option<String>,
    pub restat: bool,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Compile {
        instruction: String,
        inputs: Vec<String>,
        outputs: Vec<String>,
        line: u32,
    },
    Link {
        inputs: Vec<String>,
        outputs: Vec<String>,
        line: u32,
    },
}

#[derive(Debug, Clone)]
pub struct BuildFile {
    pub file: String,
    pub tools: FxHashMap<String, String>,
    pub flags: FxHashMap<String, Vec<String>>,
    pub vars: FxHashMap<String, String>,
    pub instructions: FxHashMap<String, Instruction>,
    pub arrays: FxHashMap<String, Vec<String>>,
    pub statements: Vec<Statement>,
    pub defaults: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Word,
    String,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Eq,
    Semi,
    Colon,
    Gt,
    Pipe,
    Comma,
    Eof,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    text: String,
    line: u32,
    col: u32,
}

struct Lexer<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    line: u32,
    col: u32,
    file: &'a str,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a str, file: &'a str) -> Self {
        Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            line: 1,
            col: 1,
            file,
        }
    }

    fn err(&self, line: u32, col: u32, msg: impl Into<String>) -> MoldError {
        MoldError::parse(self.file, line, col, msg)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn bump(&mut self) -> Option<u8> {
        let b = self.peek()?;
        self.pos += 1;
        if b == b'\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(b)
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r' | b'\n') => {
                    self.bump();
                }
                Some(b'#') => {
                    while let Some(b) = self.peek() {
                        self.bump();
                        if b == b'\n' {
                            break;
                        }
                    }
                }
                Some(b'/') if self.bytes.get(self.pos + 1) == Some(&b'/') => {
                    while let Some(b) = self.peek() {
                        self.bump();
                        if b == b'\n' {
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
    }

    fn next_token(&mut self) -> Result<Token> {
        self.skip_ws_and_comments();
        let line = self.line;
        let col = self.col;
        let Some(b) = self.peek() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                text: String::new(),
                line,
                col,
            });
        };
        let kind = match b {
            b'{' => TokenKind::LBrace,
            b'}' => TokenKind::RBrace,
            b'[' => TokenKind::LBracket,
            b']' => TokenKind::RBracket,
            b'=' => TokenKind::Eq,
            b';' => TokenKind::Semi,
            b':' => TokenKind::Colon,
            b'>' => TokenKind::Gt,
            b'|' => TokenKind::Pipe,
            b',' => TokenKind::Comma,
            b'"' => return self.read_string(line, col),
            _ => return self.read_word(line, col),
        };
        self.bump();
        Ok(Token {
            kind,
            text: String::new(),
            line,
            col,
        })
    }

    fn read_string(&mut self, line: u32, col: u32) -> Result<Token> {
        self.bump();
        let mut text = String::new();
        loop {
            match self.bump() {
                None => return Err(self.err(line, col, "unterminated string")),
                Some(b'"') => {
                    return Ok(Token {
                        kind: TokenKind::String,
                        text,
                        line,
                        col,
                    });
                }
                Some(b'\\') => {
                    match self.bump() {
                        None => return Err(self.err(line, col, "unterminated string")),
                        Some(b'n') => text.push('\n'),
                        Some(b't') => text.push('\t'),
                        Some(b'r') => text.push('\r'),
                        Some(b'"') => text.push('"'),
                        Some(b'\\') => text.push('\\'),
                        Some(b'$') => text.push_str("$$"),
                        Some(c) => {
                            text.push('\\');
                            text.push(c as char);
                        }
                    }
                }
                Some(c) => text.push(c as char),
            }
        }
    }

    fn read_word(&mut self, line: u32, col: u32) -> Result<Token> {
        let start = self.pos;
        let first = self.peek().unwrap();
        if !is_word_start(first) {
            return Err(self.err(
                line,
                col,
                format!("unexpected character '{}'", first as char),
            ));
        }
        let flag_like = first == b'-';
        self.bump();
        while let Some(c) = self.peek() {
            if is_basic_word(c) {
                self.bump();
                continue;
            }
            if c == b'=' && flag_like {
                self.bump();
                continue;
            }
            break;
        }
        let text = self.src[start..self.pos].to_string();
        if text.is_empty() {
            return Err(self.err(line, col, "expected a name or path"));
        }
        Ok(Token {
            kind: TokenKind::Word,
            text,
            line,
            col,
        })
    }
}

fn is_word_start(c: u8) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            b'_' | b'.' | b'/' | b'-' | b'+' | b'@' | b'~' | b'%' | b'$'
        )
}

fn is_basic_word(c: u8) -> bool {
    is_word_start(c) || matches!(c, b'(' | b')')
}

struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    file: &'a str,
    dir: PathBuf,
    included: &'a mut Vec<PathBuf>,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> &Token {
        &self.tokens[self.pos.min(self.tokens.len() - 1)]
    }

    fn peek_kind(&self) -> TokenKind {
        self.peek().kind
    }

    fn bump(&mut self) -> Token {
        let t = self.tokens[self.pos].clone();
        if self.pos + 1 < self.tokens.len() {
            self.pos += 1;
        }
        t
    }

    fn at(&self, kind: TokenKind) -> bool {
        self.peek_kind() == kind
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.at(kind) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn loc(&self) -> (u32, u32) {
        let t = self.peek();
        (t.line, t.col)
    }

    fn err_here(&self, msg: impl Into<String>) -> MoldError {
        let (line, col) = self.loc();
        MoldError::parse(self.file, line, col, msg)
    }

    fn expect(&mut self, kind: TokenKind, what: &str) -> Result<Token> {
        if self.peek_kind() == kind {
            Ok(self.bump())
        } else {
            Err(self.err_here(format!("expected {what}")))
        }
    }

    fn expect_word(&mut self, what: &str) -> Result<(String, u32, u32)> {
        let t = self.peek();
        match t.kind {
            TokenKind::Word | TokenKind::String => {
                let t = self.bump();
                Ok((t.text, t.line, t.col))
            }
            _ => Err(self.err_here(format!("expected {what}"))),
        }
    }

    fn parse_word_list_until(&mut self, stops: &[TokenKind]) -> Result<Vec<String>> {
        let mut words = Vec::new();
        loop {
            if stops.contains(&self.peek_kind()) || self.peek_kind() == TokenKind::Eof {
                break;
            }
            let (w, _, _) = self.expect_word("path or name")?;
            words.push(w);
        }
        Ok(words)
    }

    fn parse_comma_list(&mut self) -> Result<Vec<String>> {
        self.expect(TokenKind::LBracket, "'['")?;
        let mut items = Vec::new();
        if self.eat(TokenKind::RBracket) {
            return Ok(items);
        }
        loop {
            let (w, _, _) = self.expect_word("list item")?;
            items.push(w);
            if self.eat(TokenKind::Comma) {
                if self.eat(TokenKind::RBracket) {
                    break;
                }
                continue;
            }
            self.expect(TokenKind::RBracket, "',' or ']'")?;
            break;
        }
        Ok(items)
    }

    fn parse_file(&mut self, build: &mut BuildFile) -> Result<()> {
        while !self.at(TokenKind::Eof) {
            self.parse_stmt(build)?;
        }
        Ok(())
    }

    fn parse_stmt(&mut self, build: &mut BuildFile) -> Result<()> {
        let t = self.peek();
        if t.kind != TokenKind::Word {
            return Err(self.err_here("expected a statement"));
        }
        match t.text.as_str() {
            "tool" => self.parse_tool(build),
            "flags" => self.parse_flags(build),
            "instruction" => self.parse_instruction(build),
            "array" => self.parse_array(build),
            "compile" => self.parse_compile(build),
            "link" => self.parse_link(build),
            "var" => self.parse_var(build),
            "default" => self.parse_default(build),
            "include" => self.parse_include(build),
            other => Err(self.err_here(format!("unknown statement '{other}'"))),
        }
    }

    fn parse_tool(&mut self, build: &mut BuildFile) -> Result<()> {
        self.bump();
        let (name, line, col) = self.expect_word("tool name")?;
        self.expect(TokenKind::Eq, "'='")?;
        let (value, _, _) = self.expect_word("tool path")?;
        self.expect(TokenKind::Semi, "';'")?;
        if build.tools.contains_key(&name) {
            return Err(MoldError::parse(
                self.file,
                line,
                col,
                format!("tool '{name}' redefined"),
            ));
        }
        build.tools.insert(name, value);
        Ok(())
    }

    fn parse_flags(&mut self, build: &mut BuildFile) -> Result<()> {
        self.bump();
        let (name, line, col) = self.expect_word("flags name")?;
        self.expect(TokenKind::Eq, "'='")?;
        let items = self.parse_comma_list()?;
        self.expect(TokenKind::Semi, "';'")?;
        if build.flags.contains_key(&name) {
            return Err(MoldError::parse(
                self.file,
                line,
                col,
                format!("flags '{name}' redefined"),
            ));
        }
        build.flags.insert(name, items);
        Ok(())
    }

    fn parse_var(&mut self, build: &mut BuildFile) -> Result<()> {
        self.bump();
        let (name, line, col) = self.expect_word("variable name")?;
        self.expect(TokenKind::Eq, "'='")?;
        let (value, _, _) = self.expect_word("variable value")?;
        self.expect(TokenKind::Semi, "';'")?;
        if build.vars.contains_key(&name) {
            return Err(MoldError::parse(
                self.file,
                line,
                col,
                format!("variable '{name}' redefined"),
            ));
        }
        build.vars.insert(name, value);
        Ok(())
    }

    fn parse_array(&mut self, build: &mut BuildFile) -> Result<()> {
        self.bump();
        let (name, line, col) = self.expect_word("array name")?;
        self.expect(TokenKind::Eq, "'='")?;
        let items = self.parse_comma_list()?;
        self.expect(TokenKind::Semi, "';'")?;
        if build.arrays.contains_key(&name) {
            return Err(MoldError::parse(
                self.file,
                line,
                col,
                format!("array '{name}' redefined"),
            ));
        }
        build.arrays.insert(name, items);
        Ok(())
    }

    fn parse_instruction(&mut self, build: &mut BuildFile) -> Result<()> {
        self.bump();
        let (name, line, col) = self.expect_word("instruction name")?;
        self.expect(TokenKind::LBrace, "'{'")?;
        let mut inst = Instruction {
            name: name.clone(),
            command: String::new(),
            description: None,
            depformat: DepFormat::None,
            depfile: None,
            restat: false,
            line,
        };
        while !self.at(TokenKind::RBrace) && !self.at(TokenKind::Eof) {
            let (field, fl, fc) = self.expect_word("field name")?;
            self.expect(TokenKind::Colon, "':'")?;
            let (value, _, _) = self.expect_word("field value")?;
            self.expect(TokenKind::Semi, "';'")?;
            match field.as_str() {
                "command" => inst.command = value,
                "description" => inst.description = Some(value),
                "depfile" => inst.depfile = Some(value),
                "depformat" => {
                    inst.depformat = match value.as_str() {
                        "gcc" => DepFormat::Gcc,
                        "none" => DepFormat::None,
                        other => {
                            return Err(MoldError::parse(
                                self.file,
                                fl,
                                fc,
                                format!("unknown depformat '{other}' (use gcc or none)"),
                            ));
                        }
                    };
                }
                "restat" => {
                    inst.restat = match value.as_str() {
                        "true" | "yes" | "1" => true,
                        "false" | "no" | "0" => false,
                        other => {
                            return Err(MoldError::parse(
                                self.file,
                                fl,
                                fc,
                                format!("expected true or false, got '{other}'"),
                            ));
                        }
                    };
                }
                other => {
                    return Err(MoldError::parse(
                        self.file,
                        fl,
                        fc,
                        format!("unknown instruction field '{other}'"),
                    ));
                }
            }
        }
        self.expect(TokenKind::RBrace, "'}'")?;
        if inst.command.is_empty() {
            return Err(MoldError::parse(
                self.file,
                line,
                col,
                format!("instruction '{name}' is missing a command"),
            ));
        }
        if build.instructions.contains_key(&name) {
            return Err(MoldError::parse(
                self.file,
                line,
                col,
                format!("instruction '{name}' redefined"),
            ));
        }
        build.instructions.insert(name, inst);
        Ok(())
    }

    fn parse_compile(&mut self, build: &mut BuildFile) -> Result<()> {
        let kw = self.bump();
        let (instruction, _, _) = self.expect_word("instruction name")?;
        let inputs = self.parse_word_list_until(&[TokenKind::Gt, TokenKind::Semi])?;
        if inputs.is_empty() {
            return Err(MoldError::parse(
                self.file,
                kw.line,
                kw.col,
                "compile requires at least one input",
            ));
        }
        self.expect(TokenKind::Gt, "'>'")?;
        let outputs =
            self.parse_word_list_until(&[TokenKind::Pipe, TokenKind::Semi])?;
        if outputs.is_empty() {
            return Err(self.err_here("compile requires at least one output"));
        }
        let append_to = if self.eat(TokenKind::Pipe) {
            let (name, line, col) = self.expect_word("array name")?;
            Some((name, line, col))
        } else {
            None
        };
        self.expect(TokenKind::Semi, "';'")?;
        if let Some((arr, line, col)) = append_to {
            match build.arrays.get_mut(&arr) {
                Some(list) => list.extend(outputs.iter().cloned()),
                None => {
                    return Err(MoldError::parse(
                        self.file,
                        line,
                        col,
                        format!("unknown array '{arr}'"),
                    ));
                }
            }
        }
        build.statements.push(Statement::Compile {
            instruction,
            inputs,
            outputs,
            line: kw.line,
        });
        Ok(())
    }

    fn parse_link(&mut self, build: &mut BuildFile) -> Result<()> {
        let kw = self.bump();
        let raw_inputs = self.parse_word_list_until(&[TokenKind::Gt, TokenKind::Semi])?;
        if raw_inputs.is_empty() {
            return Err(MoldError::parse(
                self.file,
                kw.line,
                kw.col,
                "link requires at least one input or array",
            ));
        }
        self.expect(TokenKind::Gt, "'>'")?;
        let outputs =
            self.parse_word_list_until(&[TokenKind::Pipe, TokenKind::Semi])?;
        if outputs.is_empty() {
            return Err(self.err_here("link requires at least one output"));
        }
        let append_to = if self.eat(TokenKind::Pipe) {
            let (name, line, col) = self.expect_word("array name")?;
            Some((name, line, col))
        } else {
            None
        };
        self.expect(TokenKind::Semi, "';'")?;

        let mut inputs = Vec::new();
        for w in raw_inputs {
            if let Some(arr) = build.arrays.get(&w) {
                inputs.extend(arr.iter().cloned());
            } else {
                inputs.push(w);
            }
        }
        if let Some((arr, line, col)) = append_to {
            match build.arrays.get_mut(&arr) {
                Some(list) => list.extend(outputs.iter().cloned()),
                None => {
                    return Err(MoldError::parse(
                        self.file,
                        line,
                        col,
                        format!("unknown array '{arr}'"),
                    ));
                }
            }
        }
        build.statements.push(Statement::Link {
            inputs,
            outputs,
            line: kw.line,
        });
        Ok(())
    }

    fn parse_default(&mut self, build: &mut BuildFile) -> Result<()> {
        self.bump();
        let names = self.parse_word_list_until(&[TokenKind::Semi])?;
        self.expect(TokenKind::Semi, "';'")?;
        if names.is_empty() {
            return Err(self.err_here("default requires at least one target"));
        }
        build.defaults.extend(names);
        Ok(())
    }

    fn parse_include(&mut self, build: &mut BuildFile) -> Result<()> {
        let kw = self.bump();
        let (rel, _, _) = self.expect_word("include path")?;
        self.expect(TokenKind::Semi, "';'")?;
        let path = if Path::new(&rel).is_absolute() {
            PathBuf::from(&rel)
        } else {
            self.dir.join(&rel)
        };
        let canon = path.canonicalize().unwrap_or(path.clone());
        if self.included.iter().any(|p| p == &canon) {
            return Err(MoldError::parse(
                self.file,
                kw.line,
                kw.col,
                format!("include cycle involving '{}'", path.display()),
            ));
        }
        self.included.push(canon);
        load_into(path.as_path(), build, self.included)?;
        Ok(())
    }
}

fn tokenize(src: &str, file: &str) -> Result<Vec<Token>> {
    let mut lexer = Lexer::new(src, file);
    let mut tokens = Vec::new();
    loop {
        let t = lexer.next_token()?;
        let eof = t.kind == TokenKind::Eof;
        tokens.push(t);
        if eof {
            break;
        }
    }
    Ok(tokens)
}

fn load_into(path: &Path, build: &mut BuildFile, included: &mut Vec<PathBuf>) -> Result<()> {
    let src = fs::read_to_string(path).map_err(|e| MoldError::io(path, e))?;
    parse_into(&src, &path.display().to_string(), path, build, included)
}

fn parse_into(
    src: &str,
    file: &str,
    path: &Path,
    build: &mut BuildFile,
    included: &mut Vec<PathBuf>,
) -> Result<()> {
    let tokens = tokenize(src, file)?;
    let dir = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let mut parser = Parser {
        tokens,
        pos: 0,
        file,
        dir,
        included,
    };
    parser.parse_file(build)
}

pub fn parse_str(src: &str, file: &str) -> Result<BuildFile> {
    let mut build = BuildFile {
        file: file.to_string(),
        tools: FxHashMap::default(),
        flags: FxHashMap::default(),
        vars: FxHashMap::default(),
        instructions: FxHashMap::default(),
        arrays: FxHashMap::default(),
        statements: Vec::new(),
        defaults: Vec::new(),
    };
    let mut included = Vec::new();
    parse_into(src, file, Path::new(file), &mut build, &mut included)?;
    Ok(build)
}

pub fn parse_file(path: &Path) -> Result<BuildFile> {
    let mut build = BuildFile {
        file: path.display().to_string(),
        tools: FxHashMap::default(),
        flags: FxHashMap::default(),
        vars: FxHashMap::default(),
        instructions: FxHashMap::default(),
        arrays: FxHashMap::default(),
        statements: Vec::new(),
        defaults: Vec::new(),
    };
    let mut included = Vec::new();
    if let Ok(canon) = path.canonicalize() {
        included.push(canon);
    }
    load_into(path, &mut build, &mut included)?;
    Ok(build)
}

pub const SPEC_EXAMPLE: &str = r#"
tool cc = clang;
tool nasm = nasm;
tool ld = ld.lld;
flags cflags = [
    -Iinclude,
    -Wall,
    -Wextra,
    -O2
];
flags ldflags = [];
instruction c {
    command: "$(cc) $(cflags) -MMD -MF $depfile -c $in -o $out";
    depformat: gcc;
}
instruction asm {
    command: "$(nasm) -f elf64 -g -F dwarf $in -o $out";
}
instruction link {
    command: "$(ld) $(ldflags) $in -o $out";
}
array compiled = [];
compile c src/main.c > build/main.o | compiled;
compile c src/lol.c  > build/lol.o | compiled;
compile asm src/start.asm > build/start.o | compiled;
link compiled > "example_project";
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_user_spec() {
        let b = parse_str(SPEC_EXAMPLE, "build.mold").unwrap();
        assert_eq!(b.tools.get("cc").unwrap(), "clang");
        assert_eq!(b.tools.get("nasm").unwrap(), "nasm");
        assert_eq!(b.tools.get("ld").unwrap(), "ld.lld");
        assert_eq!(
            b.flags.get("cflags").unwrap(),
            &vec![
                "-Iinclude".to_string(),
                "-Wall".to_string(),
                "-Wextra".to_string(),
                "-O2".to_string()
            ]
        );
        assert!(b.flags.get("ldflags").unwrap().is_empty());
        let c = b.instructions.get("c").unwrap();
        assert_eq!(
            c.command,
            "$(cc) $(cflags) -MMD -MF $depfile -c $in -o $out"
        );
        assert_eq!(c.depformat, DepFormat::Gcc);
        assert!(b.instructions.contains_key("asm"));
        assert!(b.instructions.contains_key("link"));
        assert_eq!(
            b.arrays.get("compiled").unwrap(),
            &vec![
                "build/main.o".to_string(),
                "build/lol.o".to_string(),
                "build/start.o".to_string()
            ]
        );
        assert_eq!(b.statements.len(), 4);
        match &b.statements[3] {
            Statement::Link { inputs, outputs, .. } => {
                assert_eq!(
                    inputs,
                    &vec![
                        "build/main.o".to_string(),
                        "build/lol.o".to_string(),
                        "build/start.o".to_string()
                    ]
                );
                assert_eq!(outputs, &vec!["example_project".to_string()]);
            }
            _ => panic!("expected link"),
        }
    }

    #[test]
    fn parse_comments_and_vars() {
        let src = r#"
            # comment
            // also a comment
            var builddir = build;
            tool cc = gcc;
            flags cflags = [-O2, -std=c11];
            instruction c {
                command: "$(cc) $(cflags) -c $in -o $out";
            }
            compile c src/a.c > $(builddir)/a.o;
        "#;
        let b = parse_str(src, "t.mold").unwrap();
        assert_eq!(b.vars.get("builddir").unwrap(), "build");
        assert_eq!(b.flags.get("cflags").unwrap()[1], "-std=c11");
        match &b.statements[0] {
            Statement::Compile { outputs, .. } => {
                assert_eq!(outputs[0], "$(builddir)/a.o");
            }
            _ => panic!(),
        }
    }

    #[test]
    fn missing_semicolon_errors() {
        let src = r#"
            tool cc = gcc
            flags cflags = [];
        "#;
        let err = parse_str(src, "bad.mold").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("bad.mold:"), "{msg}");
        assert!(msg.contains("expected ';'"), "{msg}");
    }

    #[test]
    fn unknown_array_errors() {
        let src = r#"
            tool cc = gcc;
            instruction c { command: "gcc -c $in -o $out"; }
            compile c src/a.c > a.o | nope;
        "#;
        let err = parse_str(src, "bad.mold").unwrap_err();
        assert!(err.to_string().contains("unknown array 'nope'"));
    }
}
