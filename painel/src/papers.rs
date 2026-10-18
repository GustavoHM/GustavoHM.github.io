//! Working papers: a página na língua da raiz (<papers>/<slug>.qmd) e uma
//! entrada em cada lista de papers das outras línguas, mantidas juntas.

use std::fs;
use std::path::{Path, PathBuf};

use crate::frontmatter as fm;
use crate::lists::{self, Item};
use crate::manifest::Manifest;
use crate::posts::{split_list, valid_slug};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Translation {
    pub lang: String,
    /// Lista desta língua (ex.: pt/pesquisa/trabalhos.yml).
    pub list: String,
    pub title: String,
    pub description: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Paper {
    /// Pasta das páginas dos papers (ex.: research/working-papers).
    pub dir: String,
    /// Nome do arquivo sem .qmd; vazio enquanto o paper novo não foi salvo.
    pub slug: String,
    pub title: String,
    /// Autores separados por vírgula.
    pub authors: String,
    pub date: String,
    pub description: String,
    pub categories: String,
    pub body: String,
    pub translations: Vec<Translation>,
}

#[derive(Clone, Debug)]
pub struct PaperSummary {
    pub slug: String,
    pub title: String,
    pub date: String,
    pub unsent: bool,
}

impl Paper {
    pub fn rel_qmd(&self) -> String {
        format!("{}/{}.qmd", self.dir, self.slug)
    }

    /// Pasta dos anexos (PDFs) deste paper.
    pub fn rel_files(&self) -> String {
        format!("{}/files/{}", self.dir, self.slug)
    }

    pub fn files_dir(&self, site: &Path) -> PathBuf {
        site.join(self.rel_files())
    }
}

/// Link relativo da lista (numa subpasta de outra língua) até a página do paper.
fn link_from(list: &str, dir: &str, slug: &str) -> String {
    let depth = list.matches('/').count();
    format!("{}{dir}/{slug}.html", "../".repeat(depth))
}

fn find(file: &lists::ListFile, slug: &str) -> Option<usize> {
    let needle = format!("/{slug}.html");
    file.items.iter().position(|it| it.get("path").ends_with(&needle))
}

fn papers_dir(m: &Manifest) -> Result<String, String> {
    m.papers_dir().map(str::to_string).ok_or_else(|| "O _idiomas.toml não define a pasta dos working papers (papers).".into())
}

pub fn list(site: &Path, m: &Manifest) -> Vec<PaperSummary> {
    let Ok(dir) = papers_dir(m) else { return vec![] };
    let mut out: Vec<PaperSummary> = fs::read_dir(site.join(&dir))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let slug = name.strip_suffix(".qmd")?.to_string();
            if !valid_slug(&slug) {
                return None; // inclui _template.qmd
            }
            let text = fs::read_to_string(e.path()).ok()?;
            let (lines, _) = fm::split(&text);
            let unsent = crate::publish::unsent(site, &[format!("{dir}/{slug}.qmd"), format!("{dir}/files/{slug}")]);
            Some(PaperSummary {
                title: fm::get(&lines, "title").unwrap_or_default(),
                date: fm::get(&lines, "date").unwrap_or_default(),
                slug,
                unsent,
            })
        })
        .collect();
    out.sort_by(|a, b| b.date.cmp(&a.date).then(a.slug.cmp(&b.slug)));
    out
}

fn empty_translations(m: &Manifest) -> Vec<Translation> {
    m.langs
        .iter()
        .filter_map(|l| l.lista_papers.as_ref().map(|list| Translation { lang: l.codigo.clone(), list: list.clone(), ..Default::default() }))
        .collect()
}

pub fn load(site: &Path, m: &Manifest, slug: &str) -> Result<Paper, String> {
    let dir = papers_dir(m)?;
    let rel = format!("{dir}/{slug}.qmd");
    let text = fs::read_to_string(site.join(&rel)).map_err(|e| format!("Não consegui ler {rel}: {e}"))?;
    let (lines, body) = fm::split(&text);
    let mut p = Paper {
        dir,
        slug: slug.to_string(),
        title: fm::get(&lines, "title").unwrap_or_default(),
        authors: fm::get_list(&lines, "author").join(", "),
        date: fm::get(&lines, "date").unwrap_or_default(),
        description: fm::get(&lines, "description").unwrap_or_default(),
        categories: fm::get_list(&lines, "categories").join(", "),
        body,
        translations: empty_translations(m),
    };
    for t in &mut p.translations {
        if let Ok(file) = lists::read_file(site, &t.list) {
            if let Some(i) = find(&file, slug) {
                t.title = file.items[i].get("title");
                t.description = file.items[i].get("description");
            }
        }
    }
    Ok(p)
}

pub fn new_paper(m: &Manifest, date: String) -> Paper {
    Paper {
        dir: m.papers_dir().unwrap_or("research/working-papers").to_string(),
        authors: crate::posts::AUTHOR.into(),
        date,
        categories: "working paper".into(),
        body: "## Abstract\n\n\n\n## Downloads\n\n".into(),
        translations: empty_translations(m),
        ..Default::default()
    }
}

pub fn validate(p: &Paper) -> Result<(), String> {
    if p.title.trim().is_empty() {
        return Err("O paper precisa de um título.".into());
    }
    if split_list(&p.authors).is_empty() {
        return Err("Informe pelo menos um autor.".into());
    }
    if !fm::is_date(&p.date) {
        return Err("A data deve estar no formato AAAA-MM-DD.".into());
    }
    Ok(())
}

/// Grava a página e as entradas nas outras línguas. Preenche `slug` se novo.
pub fn save(site: &Path, p: &mut Paper) -> Result<(), String> {
    validate(p)?;
    // Lê as listas antes de gravar qualquer coisa: se alguma não puder ser
    // editada com segurança, nada é alterado.
    let mut files = vec![];
    for t in &p.translations {
        files.push(lists::read_file(site, &t.list)?);
    }
    let mut old_lines = vec![];
    if p.slug.is_empty() {
        let base = fm::slugify(&p.title);
        let mut slug = base.clone();
        let mut n = 2;
        while site.join(format!("{}/{slug}.qmd", p.dir)).exists() {
            slug = format!("{base}-{n}");
            n += 1;
        }
        p.slug = slug;
    } else if let Ok(text) = fs::read_to_string(site.join(p.rel_qmd())) {
        old_lines = fm::split(&text).0;
    }
    let authors = split_list(&p.authors);
    let cats: Vec<String> = split_list(&p.categories).iter().map(|c| fm::quote_if_needed(c)).collect();
    let lines = fm::set_fields(
        &old_lines,
        &[
            ("title", format!(" {}", fm::quote(p.title.trim()))),
            ("author", fm::block_list(&authors)),
            ("date", format!(" {}", p.date)),
            ("description", format!(" {}", fm::quote(p.description.trim()))),
            ("categories", format!(" [{}]", cats.join(", "))),
        ],
    );
    fs::write(site.join(p.rel_qmd()), fm::join(&lines, &p.body)).map_err(|e| format!("Não consegui salvar {}: {e}", p.rel_qmd()))?;

    let author = authors.join(", ");
    for (t, mut file) in p.translations.iter().zip(files) {
        let title = if t.title.trim().is_empty() { p.title.trim() } else { t.title.trim() };
        match find(&file, &p.slug) {
            Some(i) => {
                let it = &mut file.items[i];
                it.set("title", title);
                it.set("author", &author);
                it.set("date", &p.date);
                it.set("description", t.description.trim());
            }
            None => file.items.insert(0, Item::new(title, &author, &p.date, t.description.trim(), &link_from(&t.list, &p.dir, &p.slug))),
        }
        let out = lists::render(&file);
        if fs::read_to_string(site.join(&t.list)).ok().as_deref() != Some(out.as_str()) {
            lists::write_file(site, &t.list, &file)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links() {
        assert_eq!(link_from("pt/pesquisa/trabalhos.yml", "research/working-papers", "x"), "../../research/working-papers/x.html");
        assert_eq!(link_from("ja/trabalhos.yml", "research/working-papers", "x"), "../research/working-papers/x.html");
    }
}
