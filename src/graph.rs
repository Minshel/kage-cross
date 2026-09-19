use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rustc_hash::FxHashMap;

use crate::depfile;
use crate::error::{KageError, Result};
use crate::expand::{self, ExpandCtx};
use crate::parse::{BuildFile, DepFormat, Statement};

pub type NodeId = usize;
pub type EdgeId = usize;

#[derive(Debug, Clone)]
pub struct Node {
    pub path: String,
    pub producer: Option<EdgeId>,
    pub consumers: Vec<EdgeId>,
}

#[derive(Debug, Clone)]
pub struct Edge {
    pub ins: Vec<NodeId>,
    pub order_only_ins: Vec<NodeId>,
    pub outs: Vec<NodeId>,
    pub command: String,
    pub description: String,
    pub depformat: DepFormat,
    pub depfile: Option<String>,
    #[allow(dead_code)]
    pub restat: bool,
    pub instruction: String,
    pub command_hash: u64,
    pub line: u32,
}

#[derive(Debug, Clone)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub path_to_node: FxHashMap<String, NodeId>,
    pub defaults: Vec<NodeId>,
    pub workdir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub dirty: Vec<EdgeId>,
    pub wait: Vec<usize>,
    pub ready: Vec<EdgeId>,
    pub reasons: Vec<Option<String>>,
}

#[derive(Debug, Clone)]
pub struct FileMeta {
    pub mtime: SystemTime,
}

pub struct StatCache {
    cache: FxHashMap<String, Option<FileMeta>>,
    workdir: PathBuf,
}

impl StatCache {
    pub fn new(workdir: PathBuf) -> Self {
        Self {
            cache: FxHashMap::default(),
            workdir,
        }
    }

    pub fn full_path(&self, path: &str) -> PathBuf {
        let p = Path::new(path);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            self.workdir.join(p)
        }
    }

    pub fn stat(&mut self, path: &str) -> Option<FileMeta> {
        if let Some(v) = self.cache.get(path) {
            return v.clone();
        }
        let full = self.full_path(path);
        let meta = fs::metadata(&full).ok().and_then(|m| {
            let mtime = m.modified().ok()?;
            Some(FileMeta { mtime })
        });
        self.cache.insert(path.to_string(), meta.clone());
        meta
    }

    pub fn invalidate(&mut self, path: &str) {
        self.cache.remove(path);
    }
}

pub struct BuildLog {
    pub entries: FxHashMap<String, u64>,
    pub path: PathBuf,
}

impl BuildLog {
    pub fn load(workdir: &Path) -> Self {
        let path = workdir.join(".kage_log");
        let mut entries = FxHashMap::default();
        if let Ok(text) = fs::read_to_string(&path) {
            for line in text.lines() {
                if line.is_empty() || line.starts_with('#') {
                    continue;
                }
                let mut parts = line.splitn(2, '\t');
                let (Some(hash), Some(p)) = (parts.next(), parts.next()) else {
                    continue;
                };
                if let Ok(h) = u64::from_str_radix(hash, 16) {
                    entries.insert(p.to_string(), h);
                }
            }
        }
        Self { entries, path }
    }

    pub fn get(&self, path: &str) -> Option<u64> {
        self.entries.get(path).copied()
    }

    pub fn set(&mut self, path: &str, hash: u64) {
        self.entries.insert(path.to_string(), hash);
    }

    pub fn save(&self) -> Result<()> {
        let mut lines = vec!["# kage log v1".to_string()];
        let mut items: Vec<_> = self.entries.iter().collect();
        items.sort_by(|a, b| a.0.cmp(b.0));
        for (p, h) in items {
            lines.push(format!("{h:016x}\t{p}"));
        }
        let tmp = self.path.with_extension("log.tmp");
        fs::write(&tmp, lines.join("\n") + "\n").map_err(|e| KageError::io(&tmp, e))?;
        fs::rename(&tmp, &self.path).map_err(|e| KageError::io(&self.path, e))?;
        Ok(())
    }
}

impl Graph {
    pub fn from_ast(build: &BuildFile, workdir: PathBuf) -> Result<Self> {
        let mut g = Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            path_to_node: FxHashMap::default(),
            defaults: Vec::new(),
            workdir,
        };
        let empty_specials = FxHashMap::default();
        let ctx = ExpandCtx::from_build(build, &empty_specials);

        let stmts: Vec<Statement> = build.statements.clone();
        for stmt in &stmts {
            match stmt {
                Statement::Compile {
                    instruction,
                    inputs,
                    order_only_inputs,
                    outputs,
                    line,
                } => {
                    let inst = build.instructions.get(instruction).ok_or_else(|| {
                        KageError::parse(
                            &build.file,
                            *line,
                            1,
                            format!("unknown instruction '{instruction}'"),
                        )
                    })?;
                    g.add_edge(
                        build,
                        &ctx,
                        instruction,
                        inst,
                        inputs,
                        order_only_inputs,
                        outputs,
                        *line,
                    )?;
                }
                Statement::Link {
                    inputs,
                    order_only_inputs,
                    outputs,
                    line,
                } => {
                    let inst = build.instructions.get("link").ok_or_else(|| {
                        KageError::parse(
                            &build.file,
                            *line,
                            1,
                            "link statement requires an 'instruction link { ... }'",
                        )
                    })?;
                    g.add_edge(
                        build,
                        &ctx,
                        "link",
                        inst,
                        inputs,
                        order_only_inputs,
                        outputs,
                        *line,
                    )?;
                }
            }
        }

        if build.defaults.is_empty() {
            g.defaults = g
                .nodes
                .iter()
                .enumerate()
                .filter(|(_, n)| n.producer.is_some() && n.consumers.is_empty())
                .map(|(i, _)| i)
                .collect();
        } else {
            for name in &build.defaults {
                let expanded = expand::expand(name, &ctx)?;
                let id = g.find_target(&expanded).ok_or_else(|| {
                    KageError::build(format!("unknown default target '{expanded}'"))
                })?;
                g.defaults.push(id);
            }
        }
        Ok(g)
    }

    fn add_edge(
        &mut self,
        _build: &BuildFile,
        path_ctx: &ExpandCtx<'_>,
        inst_name: &str,
        inst: &crate::parse::Instruction,
        inputs: &[String],
        order_only_inputs: &[String],
        outputs: &[String],
        line: u32,
    ) -> Result<()> {
        let mut ins_paths = Vec::new();
        for p in inputs {
            ins_paths.push(expand::expand(p, path_ctx)?);
        }
        let mut ooi_paths = Vec::new();
        for p in order_only_inputs {
            ooi_paths.push(expand::expand(p, path_ctx)?);
        }
        let mut out_paths = Vec::new();
        for p in outputs {
            out_paths.push(expand::expand(p, path_ctx)?);
        }
        if ins_paths.is_empty() || out_paths.is_empty() {
            return Err(KageError::build(format!(
                "edge at line {line} is missing inputs or outputs"
            )));
        }

        let depfile = match &inst.depfile {
            Some(t) => Some(expand::expand(t, path_ctx)?),
            None => expand::default_depfile(&out_paths[0], inst.depformat),
        };

        let mut specials = FxHashMap::default();
        specials.insert("in".into(), expand::shell_join(&ins_paths));
        specials.insert("out".into(), expand::shell_join(&out_paths));
        specials.insert(
            "in_newline".into(),
                        ins_paths
                        .iter()
                        .map(|s| expand::shell_quote(s))
                        .collect::<Vec<_>>()
                        .join("\n"),
        );
        if let Some(df) = &depfile {
            specials.insert("depfile".into(), expand::shell_quote(df));
        } else {
            specials.insert("depfile".into(), String::new());
        }

        let cmd_ctx = ExpandCtx {
            file: path_ctx.file,
            tools: path_ctx.tools,
            flags: path_ctx.flags,
            vars: path_ctx.vars,
            arrays: path_ctx.arrays,
            specials: &specials,
        };
        let command = expand::expand(&inst.command, &cmd_ctx)?;
        let description = if let Some(d) = &inst.description {
            expand::expand(d, &cmd_ctx)?
        } else if inst_name == "link" {
            format!("link {}", out_paths[0])
        } else {
            format!("{inst_name} {}", ins_paths[0])
        };

        let ins: Vec<NodeId> = ins_paths.iter().map(|p| self.intern(p)).collect();
        let order_only_ins: Vec<NodeId> =
        ooi_paths.iter().map(|p| self.intern(p)).collect();
        let outs: Vec<NodeId> = out_paths.iter().map(|p| self.intern(p)).collect();

        let eid = self.edges.len();
        for &o in &outs {
            if let Some(prev) = self.nodes[o].producer {
                return Err(KageError::build(format!(
                    "multiple rules produce '{}': lines {} and {line}",
                    self.nodes[o].path,
                    self.edges[prev].line
                )));
            }
            self.nodes[o].producer = Some(eid);
        }
        for &i in &ins {
            self.nodes[i].consumers.push(eid);
        }
        for &i in &order_only_ins {
            self.nodes[i].consumers.push(eid);
        }

        self.edges.push(Edge {
            ins,
            order_only_ins,
            outs,
            command_hash: expand::hash_command(&command),
                        command,
                        description,
                        depformat: inst.depformat,
                        depfile,
                        restat: inst.restat,
                        instruction: inst_name.to_string(),
                        line,
        });
        Ok(())
    }

    pub fn intern(&mut self, path: &str) -> NodeId {
        if let Some(&id) = self.path_to_node.get(path) {
            return id;
        }
        let id = self.nodes.len();
        self.path_to_node.insert(path.to_string(), id);
        self.nodes.push(Node {
            path: path.to_string(),
            producer: None,
            consumers: Vec::new(),
        });
        id
    }

    pub fn find_target(&self, name: &str) -> Option<NodeId> {
        if let Some(&id) = self.path_to_node.get(name) {
            return Some(id);
        }
        let mut hits = Vec::new();
        for (i, n) in self.nodes.iter().enumerate() {
            if n.producer.is_none() {
                continue;
            }
            if n.path == name
                || n.path.ends_with(&format!("/{name}"))
                || Path::new(&n.path).file_name().and_then(|s| s.to_str()) == Some(name)
            {
                hits.push(i);
            }
        }
        if hits.len() == 1 {
            Some(hits[0])
        } else {
            None
        }
    }

    pub fn resolve_targets(&self, names: &[String]) -> Result<Vec<NodeId>> {
        if names.is_empty() {
            if self.defaults.is_empty() {
                return Err(KageError::build("no targets to build"));
            }
            return Ok(self.defaults.clone());
        }
        let mut out = Vec::new();
        for n in names {
            let id = self
                .find_target(n)
                .ok_or_else(|| KageError::build(format!("unknown target '{n}'")))?;
            out.push(id);
        }
        Ok(out)
    }

    pub fn plan(
        &self,
        targets: &[NodeId],
        log: &BuildLog,
        stat: &mut StatCache,
        always_make: bool,
        explain: bool,
    ) -> Result<Plan> {
        let n_edges = self.edges.len();
        let mut wanted = vec![false; n_edges];
        let mut visiting = vec![0u8; self.nodes.len()];
        for &t in targets {
            self.walk_wanted(t, &mut wanted, &mut visiting)?;
        }

        let mut implicit_paths: Vec<Vec<String>> = vec![Vec::new(); n_edges];
        for (eid, edge) in self.edges.iter().enumerate() {
            if !wanted[eid] {
                continue;
            }
            if edge.depformat != DepFormat::Gcc {
                continue;
            }
            let Some(df) = &edge.depfile else { continue };
            let full = stat.full_path(df);
            if let Ok(text) = fs::read_to_string(&full) {
                let out_path = &self.nodes[edge.outs[0]].path;
                implicit_paths[eid] = depfile::parse_depfile(&text, out_path);
            }
        }

        let mut dirty = vec![false; n_edges];
        let mut reasons = vec![None; n_edges];
        let mut seen_edge = vec![false; n_edges];
        for (eid, w) in wanted.iter().enumerate() {
            if *w {
                self.compute_dirty(
                    eid,
                    always_make,
                    log,
                    stat,
                    &implicit_paths,
                    &mut dirty,
                    &mut reasons,
                    &mut seen_edge,
                    explain,
                )?;
            }
        }

        let mut wait = vec![0usize; n_edges];
        for (eid, edge) in self.edges.iter().enumerate() {
            if !dirty[eid] {
                continue;
            }
            let mut w = 0;
            for &i in &edge.ins {
                if let Some(p) = self.nodes[i].producer {
                    if dirty[p] {
                        w += 1;
                    }
                }
            }
            for &i in &edge.order_only_ins {
                if let Some(p) = self.nodes[i].producer {
                    if dirty[p] {
                        w += 1;
                    }
                }
            }
            wait[eid] = w;
        }
        let mut ready = Vec::new();
        let mut dirty_list = Vec::new();
        for (eid, d) in dirty.iter().enumerate() {
            if *d {
                dirty_list.push(eid);
                if wait[eid] == 0 {
                    ready.push(eid);
                }
            }
        }
        Ok(Plan {
            dirty: dirty_list,
            wait,
            ready,
            reasons,
        })
    }

    fn walk_wanted(
        &self,
        node: NodeId,
        wanted: &mut [bool],
        visiting: &mut [u8],
    ) -> Result<()> {
        match visiting[node] {
            1 => {
                return Err(KageError::build(format!(
                    "dependency cycle involving '{}'",
                    self.nodes[node].path
                )));
            }
            2 => return Ok(()),
            _ => {}
        }
        visiting[node] = 1;
        if let Some(eid) = self.nodes[node].producer {
            wanted[eid] = true;
            for &i in &self.edges[eid].ins {
                self.walk_wanted(i, wanted, visiting)?;
            }
            for &i in &self.edges[eid].order_only_ins {
                self.walk_wanted(i, wanted, visiting)?;
            }
        }
        visiting[node] = 2;
        Ok(())
    }

    fn compute_dirty(
        &self,
        eid: EdgeId,
        always_make: bool,
        log: &BuildLog,
        stat: &mut StatCache,
        implicit_paths: &[Vec<String>],
        dirty: &mut [bool],
        reasons: &mut [Option<String>],
        seen: &mut [bool],
        explain: bool,
    ) -> Result<()> {
        if seen[eid] {
            return Ok(());
        }
        seen[eid] = true;
        let edge = &self.edges[eid];

        for &i in &edge.ins {
            if let Some(p) = self.nodes[i].producer {
                self.compute_dirty(
                    p, always_make, log, stat, implicit_paths,
                    dirty, reasons, seen, explain,
                )?;
            } else if stat.stat(&self.nodes[i].path).is_none() {
                return Err(KageError::build(format!(
                    "missing input '{}' needed by '{}'",
                    self.nodes[i].path,
                    self.nodes[edge.outs[0]].path
                )));
            }
        }
        for &i in &edge.order_only_ins {
            if let Some(p) = self.nodes[i].producer {
                self.compute_dirty(
                    p, always_make, log, stat, implicit_paths,
                    dirty, reasons, seen, explain,
                )?;
            } else if stat.stat(&self.nodes[i].path).is_none() {
                return Err(KageError::build(format!(
                    "missing order-only input '{}' needed by '{}'",
                    self.nodes[i].path,
                    self.nodes[edge.outs[0]].path
                )));
            }
        }
        if always_make {
            dirty[eid] = true;
            if explain {
                reasons[eid] = Some("forced rebuild (-B)".into());
            }
            return Ok(());
        }

        let mut reason: Option<String> = None;
        for &o in &edge.outs {
            let path = &self.nodes[o].path;
            if stat.stat(path).is_none() {
                reason = Some(format!("output '{path}' is missing"));
                break;
            }
        }
        if reason.is_none() && edge.depformat == DepFormat::Gcc {
            if let Some(df) = &edge.depfile {
                if stat.stat(df).is_none() {
                    reason = Some(format!("depfile '{df}' is missing"));
                }
            }
        }
        if reason.is_none() {
            for &o in &edge.outs {
                let path = &self.nodes[o].path;
                match log.get(path) {
                    None => {
                        reason = Some(format!("'{path}' has no recorded command"));
                        break;
                    }
                    Some(h) if h != edge.command_hash => {
                        reason = Some(format!("command line changed for '{path}'"));
                        break;
                    }
                    _ => {}
                }
            }
        }
        if reason.is_none() {
            for &i in &edge.ins {
                if let Some(p) = self.nodes[i].producer {
                    if dirty[p] {
                        reason = Some(format!("input '{}' is dirty", self.nodes[i].path));
                        break;
                    }
                }
            }
        }
        if reason.is_none() {
            let mut newest_in: Option<SystemTime> = None;
            let mut newest_name = "";
            for &i in &edge.ins {
                let path = &self.nodes[i].path;
                match stat.stat(path) {
                    None => {
                        if self.nodes[i].producer.is_none() {
                            return Err(KageError::build(format!(
                                "missing input '{path}' needed by '{}'",
                                self.nodes[edge.outs[0]].path
                            )));
                        }
                    }
                    Some(meta) => {
                        if newest_in.map(|t| meta.mtime > t).unwrap_or(true) {
                            newest_in = Some(meta.mtime);
                            newest_name = path;
                        }
                    }
                }
            }
            for ip in &implicit_paths[eid] {
                match stat.stat(ip) {
                    None => {
                        reason = Some(format!("implicit input '{ip}' is missing"));
                        break;
                    }
                    Some(meta) => {
                        if newest_in.map(|t| meta.mtime > t).unwrap_or(true) {
                            newest_in = Some(meta.mtime);
                            newest_name = ip;
                        }
                    }
                }
            }
            if reason.is_none() {
                if let Some(tin) = newest_in {
                    for &o in &edge.outs {
                        let path = &self.nodes[o].path;
                        if let Some(meta) = stat.stat(path) {
                            if tin > meta.mtime {
                                reason = Some(format!(
                                    "input '{newest_name}' is newer than '{path}'"
                                ));
                                break;
                            }
                        }
                    }
                }
            }
        }

        if let Some(r) = reason {
            dirty[eid] = true;
            if explain {
                reasons[eid] = Some(r);
            }
        }
        Ok(())
    }

    pub fn outputs(&self) -> impl Iterator<Item = &str> {
        self.edges
            .iter()
            .flat_map(|e| e.outs.iter().map(|&id| self.nodes[id].path.as_str()))
    }
}
