//! Menu do site: os itens de `website.navbar.left` de cada perfil
//! (_quarto-<código>.yml), editados juntos, uma linha por página.

use std::fs;
use std::path::Path;

use crate::languages::config_file;
use crate::manifest::{self, Manifest};
use crate::yamlpath as yp;

const LEFT: [&str; 3] = ["website", "navbar", "left"];

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    /// Linhas brutas do item no perfil (None = ainda não está no menu).
    pub raw: Option<Vec<String>>,
    /// Destino nesta língua (caminho da página).
    pub href: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// Chave da página no manifesto, ou o próprio href para links avulsos.
    pub key: String,
    pub visible: bool,
    /// Uma por língua (na ordem de `MenuDoc::langs`); None = a página não existe nela.
    pub cells: Vec<Option<Cell>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuDoc {
    pub langs: Vec<String>,
    /// Arquivo do menu de cada língua (perfil, ou _quarto.yml).
    pub files: Vec<String>,
    pub rows: Vec<Row>,
}

fn key_of(m: &Manifest, lang: &str, href: &str) -> String {
    m.lang(lang)
        .and_then(|l| l.paginas.iter().find(|(_, v)| *v == href).map(|(k, _)| k.clone()))
        .unwrap_or_else(|| href.to_string())
}

pub fn row_name(key: &str) -> String {
    if key.contains('/') || key.contains('.') { key.to_string() } else { manifest::page_name(key) }
}

pub fn load(site: &Path, m: &Manifest) -> Result<MenuDoc, String> {
    let langs: Vec<String> = m.langs.iter().map(|l| l.codigo.clone()).collect();
    let files: Vec<String> = langs.iter().map(|l| config_file(site, l)).collect();
    let mut rows: Vec<Row> = vec![];
    for (li, lang) in langs.iter().enumerate() {
        let file = &files[li];
        let text = fs::read_to_string(site.join(file)).map_err(|e| format!("Não consegui ler {file}: {e}"))?;
        let lines = yp::file_lines(&text);
        let Some((_, _, items)) = yp::items(&lines, &LEFT) else { continue };
        // posição depois da qual entra um item que só existe nesta língua
        let mut after: Option<usize> = None;
        for item in items {
            let href = yp::item_get(&item, "href").unwrap_or_default();
            let label = yp::item_get(&item, "text").unwrap_or_default();
            let key = key_of(m, lang, &href);
            let cell = Cell { raw: Some(item), href, label };
            match rows.iter().position(|r| r.key == key) {
                Some(i) => {
                    rows[i].cells[li] = Some(cell);
                    after = Some(i);
                }
                None => {
                    let mut cells = vec![None; langs.len()];
                    cells[li] = Some(cell);
                    let at = after.map(|i| i + 1).unwrap_or(0);
                    rows.insert(at, Row { key, visible: true, cells });
                    after = Some(at);
                }
            }
        }
    }
    // páginas do manifesto que não estão no menu, como itens escondidos
    for key in m.page_keys() {
        if !rows.iter().any(|r| r.key == key) {
            rows.push(Row { key: key.clone(), visible: false, cells: vec![None; langs.len()] });
        }
    }
    // completa as células que faltam (página existe na língua mas não no menu)
    for row in &mut rows {
        for (li, lang) in langs.iter().enumerate() {
            if row.cells[li].is_none() {
                if let Some(href) = m.lang(lang).and_then(|l| l.paginas.get(&row.key)) {
                    let title = page_title(site, href).unwrap_or_else(|| row_name(&row.key));
                    row.cells[li] = Some(Cell { raw: None, href: href.clone(), label: title });
                }
            }
        }
    }
    Ok(MenuDoc { langs, files, rows })
}

fn page_title(site: &Path, rel: &str) -> Option<String> {
    let text = fs::read_to_string(site.join(rel)).ok()?;
    crate::frontmatter::get(&crate::frontmatter::split(&text).0, "title")
}

impl MenuDoc {
    pub fn move_row(&mut self, i: usize, up: bool) {
        let j = if up { i.checked_sub(1) } else { (i + 1 < self.rows.len()).then_some(i + 1) };
        if let Some(j) = j {
            self.rows.swap(i, j);
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        for r in self.rows.iter().filter(|r| r.visible) {
            for (li, c) in r.cells.iter().enumerate() {
                if c.as_ref().is_some_and(|c| c.label.trim().is_empty()) {
                    return Err(format!("O item “{}” está sem rótulo em {}.", row_name(&r.key), self.langs[li]));
                }
            }
        }
        Ok(())
    }

    pub fn paths(&self) -> Vec<String> {
        let mut v = self.files.clone();
        v.dedup();
        v
    }
}

pub fn save(site: &Path, doc: &MenuDoc) -> Result<(), String> {
    doc.validate()?;
    let mut writes = vec![];
    for (li, lang) in doc.langs.iter().enumerate() {
        let _ = lang;
        let file = doc.files[li].clone();
        let text = fs::read_to_string(site.join(&file)).map_err(|e| format!("Não consegui ler {file}: {e}"))?;
        let mut lines = yp::file_lines(&text);
        let Some((_, _, old)) = yp::items(&lines, &LEFT) else {
            return Err(format!("{file} não tem website.navbar.left."));
        };
        let dash = yp::dash_indent(&old, 6);
        let items: Vec<Vec<String>> = doc
            .rows
            .iter()
            .filter(|r| r.visible)
            .filter_map(|r| r.cells[li].as_ref())
            .map(|c| match &c.raw {
                Some(raw) => {
                    let mut it = raw.clone();
                    yp::item_set(&mut it, "text", c.label.trim());
                    it
                }
                None => yp::new_item(&dash, &[("href", &c.href), ("text", c.label.trim())]),
            })
            .collect();
        yp::replace_items(&mut lines, &LEFT, &items).map_err(|e| format!("{file}: {e}"))?;
        let out = yp::join_file(&lines);
        if out != text {
            writes.push((file, out));
        }
    }
    for (file, out) in writes {
        fs::write(site.join(&file), out).map_err(|e| format!("Não consegui salvar {file}: {e}"))?;
    }
    Ok(())
}
