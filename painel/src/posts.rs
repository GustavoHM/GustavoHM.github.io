//! Postagens em <pasta da língua>/<slug>/index.qmd (ex.: pt/postagens).

use std::fs;
use std::path::{Path, PathBuf};

use std::process::Command;

use crate::frontmatter as fm;

pub const AUTHOR: &str = "Gustavo Hoffmann Moreira";
pub const IMAGE_EXT: [&str; 6] = ["png", "jpg", "jpeg", "gif", "webp", "svg"];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Post {
    /// Língua e pasta de postagens dela (do _idiomas.toml).
    pub lang: String,
    pub dir: String,
    /// Nome da pasta; vazio enquanto a postagem nova não foi salva.
    pub slug: String,
    pub title: String,
    pub description: String,
    pub date: String,
    pub categories: String,
    pub draft: bool,
    pub body: String,
}

#[derive(Clone, Debug)]
pub struct PostSummary {
    pub slug: String,
    pub title: String,
    pub date: String,
    pub draft: bool,
    /// Há alterações locais que ainda não foram enviadas ao GitHub.
    pub unsent: bool,
}

impl Post {
    pub fn rel_dir(&self) -> String {
        format!("{}/{}", self.dir, self.slug)
    }
}

pub fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 120
        && slug.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
        && slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

pub fn post_dir(site: &Path, dir: &str, slug: &str) -> Result<PathBuf, String> {
    if valid_slug(slug) {
        Ok(site.join(dir).join(slug))
    } else {
        Err(format!("Identificador de postagem inválido: {slug}"))
    }
}

pub fn today() -> String {
    Command::new("/bin/date")
        .arg("+%Y-%m-%d")
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

pub fn parse(slug: &str, text: &str) -> Post {
    let (lines, body) = fm::split(text);
    Post {
        lang: String::new(),
        dir: String::new(),
        slug: slug.to_string(),
        title: fm::get(&lines, "title").unwrap_or_default(),
        description: fm::get(&lines, "description").unwrap_or_default(),
        date: fm::get(&lines, "date").unwrap_or_default(),
        categories: fm::get_list(&lines, "categories").join(", "),
        draft: fm::get(&lines, "draft").is_none_or(|v| matches!(v.to_lowercase().as_str(), "true" | "yes")),
        body,
    }
}

pub fn split_list(s: &str) -> Vec<String> {
    s.split(',').map(|c| c.trim().to_string()).filter(|c| !c.is_empty()).collect()
}

pub fn render(post: &Post, old_lines: &[String]) -> String {
    let cats = split_list(&post.categories).iter().map(|c| fm::quote_if_needed(c)).collect::<Vec<_>>().join(", ");
    let mut fields = vec![
        ("title", format!(" {}", fm::quote(&post.title))),
        ("description", format!(" {}", fm::quote(&post.description))),
        ("date", format!(" {}", post.date)),
        ("categories", format!(" [{cats}]")),
        ("draft", if post.draft { " true".into() } else { " false".into() }),
    ];
    if !fm::has(old_lines, "author") {
        fields.insert(2, ("author", format!(" {}", fm::quote(AUTHOR))));
    }
    fm::join(&fm::set_fields(old_lines, &fields), &post.body)
}

pub fn list(site: &Path, dir: &str) -> Vec<PostSummary> {
    let unsent = crate::publish::unsent_subdirs(site, dir);
    let mut posts: Vec<PostSummary> = fs::read_dir(site.join(dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let slug = entry.file_name().to_string_lossy().to_string();
            if !valid_slug(&slug) {
                return None;
            }
            let text = fs::read_to_string(entry.path().join("index.qmd")).ok()?;
            let p = parse(&slug, &text);
            Some(PostSummary { unsent: unsent.contains(&slug), slug, title: p.title, date: p.date, draft: p.draft })
        })
        .collect();
    posts.sort_by(|a, b| b.date.cmp(&a.date).then(b.slug.cmp(&a.slug)));
    posts
}

pub fn load(site: &Path, lang: &str, dir: &str, slug: &str) -> Result<Post, String> {
    let f = post_dir(site, dir, slug)?.join("index.qmd");
    let text = fs::read_to_string(&f).map_err(|e| format!("Não consegui ler {}: {e}", f.display()))?;
    Ok(Post { lang: lang.into(), dir: dir.into(), ..parse(slug, &text) })
}

pub fn validate(post: &Post) -> Result<(), String> {
    if post.title.trim().is_empty() {
        return Err("A postagem precisa de um título.".into());
    }
    if !fm::is_date(&post.date) {
        return Err("A data deve estar no formato AAAA-MM-DD.".into());
    }
    Ok(())
}

/// Grava a postagem; se for nova, cria a pasta e preenche `post.slug`.
pub fn save(site: &Path, post: &mut Post) -> Result<(), String> {
    validate(post)?;
    post.title = post.title.trim().to_string();
    post.description = post.description.trim().to_string();
    let mut old_lines = vec![];
    if post.slug.is_empty() {
        let base = format!("{}-{}", post.date, fm::slugify(&post.title));
        let mut slug = base.clone();
        let mut n = 2;
        while site.join(&post.dir).join(&slug).exists() {
            slug = format!("{base}-{n}");
            n += 1;
        }
        post.slug = slug;
    } else if let Ok(text) = fs::read_to_string(post_dir(site, &post.dir, &post.slug)?.join("index.qmd")) {
        old_lines = fm::split(&text).0;
    }
    let dir = post_dir(site, &post.dir, &post.slug)?;
    fs::create_dir_all(&dir).map_err(|e| format!("Não consegui criar {}: {e}", dir.display()))?;
    fs::write(dir.join("index.qmd"), render(post, &old_lines)).map_err(|e| format!("Não consegui salvar: {e}"))
}

/// Copia um arquivo para `dir` com um nome limpo (prefixado, se pedido) e
/// devolve o nome usado.
pub fn copy_into(dir: &Path, src: &Path, prefix: &str, allowed: &[&str]) -> Result<String, String> {
    let ext = src.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !allowed.contains(&ext.as_str()) {
        return Err(format!("Formato não suportado (.{ext})."));
    }
    let stem = fm::slugify(&src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default());
    let stem = if prefix.is_empty() { stem } else { format!("{prefix}-{stem}") };
    let mut name = format!("{stem}.{ext}");
    let mut n = 2;
    while dir.join(&name).exists() {
        name = format!("{stem}-{n}.{ext}");
        n += 1;
    }
    fs::copy(src, dir.join(&name)).map_err(|e| format!("Não consegui copiar o arquivo: {e}"))?;
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_unknown_fields() {
        let text = "---\ntitle: \"Olá: \\\"mundo\\\"\"\nauthor: \"Fulano\"\nimage: capa.png\ndate: 2026-08-23\ncategories: [inteligência artificial, \"a, b\"]\ndraft: true\n---\n\nCorpo *aqui*.\n";
        let p = parse("x", text);
        assert_eq!(p.title, "Olá: \"mundo\"");
        assert_eq!(p.categories, "inteligência artificial, a, b");
        assert!(p.draft);
        let mut p2 = p.clone();
        p2.draft = false;
        p2.categories = "ia, metodologia".into();
        let (old, _) = fm::split(text);
        let out = render(&p2, &old);
        assert!(out.contains("image: capa.png"));
        assert!(out.contains("author: \"Fulano\""));
        assert!(out.contains("draft: false"));
        assert!(out.contains("categories: [ia, metodologia]"));
        assert!(out.ends_with("Corpo *aqui*.\n"));
        assert_eq!(parse("x", &out).title, p.title);
    }

    #[test]
    fn slugs() {
        assert_eq!(fm::slugify("IA e a Ciência: ação!"), "ia-e-a-ciencia-acao");
        assert!(valid_slug("2026-08-23-ia"));
        assert!(!valid_slug("../etc"));
    }
}
