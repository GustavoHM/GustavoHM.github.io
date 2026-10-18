//! Envio para o site: testa a renderização com o Quarto, faz commit só dos
//! arquivos editados e dá push em main. O GitHub Actions publica o resultado.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;

use eframe::egui;

pub enum Msg {
    Log(String),
    Done { ok: bool },
}

/// O que enviar: arquivos/pastas a commitar e páginas a renderizar antes.
#[derive(Clone, Debug)]
pub struct Job {
    pub paths: Vec<String>,
    /// (perfil do Quarto, arquivo .qmd; vazio = o perfil inteiro)
    pub renders: Vec<(String, String)>,
    pub message: String,
    pub url: Option<String>,
}

/// Apps abertos pelo Finder herdam um PATH mínimo; acrescenta os lugares
/// onde ficam git, gh, quarto etc.
fn path_env() -> String {
    let home = std::env::var("HOME").unwrap_or_default();
    let base = std::env::var("PATH").unwrap_or_default();
    format!("{home}/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin:{base}")
}

fn cmd(site: &Path, program: &str) -> Command {
    let mut c = Command::new(program);
    c.current_dir(site).env("PATH", path_env());
    c
}

fn git_out(site: &Path, args: &[&str]) -> String {
    cmd(site, "git")
        .args(args)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

pub fn quarto_bin() -> Option<PathBuf> {
    path_env().split(':').map(|d| Path::new(d).join("quarto")).find(|p| p.is_file())
}

pub fn branch(site: &Path) -> String {
    git_out(site, &["rev-parse", "--abbrev-ref", "HEAD"])
}

/// Caminhos com alterações locais ou commits ainda não enviados.
fn changed_paths(site: &Path, pathspec: &[&str]) -> Vec<String> {
    let mut args = vec!["status", "--porcelain", "--untracked-files=all", "--"];
    args.extend(pathspec);
    let mut out: Vec<String> = git_out(site, &args)
        .lines()
        .filter_map(|l| l.get(3..))
        .map(|p| p.trim_matches('"').to_string())
        .collect();
    let mut args = vec!["log", "--format=", "--name-only", "@{upstream}..HEAD", "--"];
    args.extend(pathspec);
    out.extend(git_out(site, &args).lines().map(str::to_string));
    out
}

pub fn unsent(site: &Path, paths: &[String]) -> bool {
    let spec: Vec<&str> = paths.iter().map(String::as_str).collect();
    !changed_paths(site, &spec).is_empty()
}

/// Subpastas de `prefix` com alterações não enviadas (ex.: postagens).
pub fn unsent_subdirs(site: &Path, prefix: &str) -> Vec<String> {
    let lead = format!("{prefix}/");
    let mut v: Vec<String> = changed_paths(site, &[prefix])
        .iter()
        .filter_map(|p| p.strip_prefix(&lead))
        .filter(|p| p.contains('/'))
        .filter_map(|p| p.split('/').next())
        .map(str::to_string)
        .collect();
    v.sort();
    v.dedup();
    v
}

/// Arquivos .qmd apagados no computador mas ainda presentes no último commit
/// (ex.: páginas removidas cujo envio falhou).
pub fn deleted_files(site: &Path) -> Vec<String> {
    git_out(site, &["ls-files", "--deleted"]).lines().filter(|l| l.ends_with(".qmd")).map(str::to_string).collect()
}

/// Lê `site-url` do _quarto.yml.
pub fn site_url(site: &Path) -> Option<String> {
    let cfg = std::fs::read_to_string(site.join("_quarto.yml")).ok()?;
    let line = cfg.lines().find(|l| l.trim_start().starts_with("site-url:"))?;
    Some(line.split_once(':')?.1.trim().trim_matches('"').trim_end_matches('/').to_string())
}

pub fn page_url(site: &Path, rel_html: &str) -> Option<String> {
    site_url(site).map(|base| format!("{base}/{rel_html}"))
}

struct Runner<'a> {
    site: &'a Path,
    tx: &'a Sender<Msg>,
    ctx: &'a egui::Context,
}

impl Runner<'_> {
    fn log(&self, s: impl Into<String>) {
        let _ = self.tx.send(Msg::Log(s.into()));
        self.ctx.request_repaint();
    }

    fn run(&self, program: &str, args: &[&str]) -> Result<String, String> {
        let name = Path::new(program).file_name().unwrap_or_default().to_string_lossy();
        self.log(format!("$ {name} {}", args.join(" ")));
        let out = cmd(self.site, program).args(args).output().map_err(|e| format!("Não consegui executar {program}: {e}"))?;
        let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
        let text = text.trim();
        if !text.is_empty() {
            let skip = text.chars().count().saturating_sub(4000);
            self.log(text.chars().skip(skip).collect::<String>());
        }
        if out.status.success() {
            Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
        } else {
            Err(format!("O comando falhou ({}).", out.status))
        }
    }

    fn quiet_ok(&self, program: &str, args: &[&str]) -> bool {
        cmd(self.site, program).args(args).output().is_ok_and(|o| o.status.success())
    }
}

/// Executa o envio. Roda numa thread à parte e informa o progresso por `tx`.
pub fn publish(site: PathBuf, job: Job, tx: Sender<Msg>, ctx: egui::Context) {
    let r = Runner { site: &site, tx: &tx, ctx: &ctx };
    let result = (|| -> Result<(), String> {
        let b = branch(&site);
        if b != "main" {
            return Err(format!("O repositório está no branch '{b}', não em 'main'. Volte para main antes de publicar."));
        }
        match quarto_bin() {
            Some(q) => {
                r.log("Testando a renderização…");
                for (profile, file) in &job.renders {
                    // arquivo vazio = o perfil inteiro (ex.: uma língua nova)
                    let mut args = vec!["render"];
                    if !file.is_empty() {
                        args.push(file);
                    }
                    // sites sem perfis por língua renderizam sem --profile
                    if site.join(format!("_quarto-{profile}.yml")).is_file() {
                        args.extend(["--profile", profile]);
                    }
                    r.run(&q.to_string_lossy(), &args)?;
                }
            }
            None => r.log("Aviso: Quarto não encontrado; pulando o teste de renderização."),
        }
        // Só caminhos que existem (ou que o git conhece) entram no commit.
        let paths: Vec<&str> = job
            .paths
            .iter()
            .map(String::as_str)
            .filter(|p| site.join(p).exists() || r.quiet_ok("git", &["ls-files", "--error-unmatch", p]))
            .collect();
        let mut add = vec!["add", "-A", "--"];
        add.extend(&paths);
        r.run("git", &add)?;
        let mut diff = vec!["diff", "--cached", "--quiet", "--"];
        diff.extend(&paths);
        if r.quiet_ok("git", &diff) {
            r.log("Nada mudou nesses arquivos desde o último commit.");
        } else {
            let mut commit = vec!["commit", "-m", job.message.as_str(), "--"];
            commit.extend(&paths);
            r.run("git", &commit)?;
        }
        r.run("git", &["fetch", "origin", "main"])?;
        let behind = r.run("git", &["rev-list", "--count", "HEAD..origin/main"])?;
        if behind != "0" {
            r.run("git", &["pull", "--rebase", "--autostash", "origin", "main"])?;
        }
        r.run("git", &["push", "origin", "main"])?;
        Ok(())
    })();
    let ok = match result {
        Ok(()) => {
            r.log("Enviado! O GitHub Actions vai reconstruir o site em 1–2 minutos.");
            if let Some(url) = &job.url {
                r.log(format!("Endereço: {url}"));
            }
            true
        }
        Err(e) => {
            r.log(format!("ERRO: {e}"));
            false
        }
    };
    let _ = tx.send(Msg::Done { ok });
    ctx.request_repaint();
}

