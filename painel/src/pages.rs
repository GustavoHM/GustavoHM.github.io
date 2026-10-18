//! Páginas fixas do site (Sobre, Pesquisa, Ensino, CV…), uma versão por língua.

use std::fs;
use std::path::Path;

use crate::frontmatter as fm;
use crate::manifest::Manifest;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Side {
    pub lang: String,
    pub rel: String,
    pub title: String,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageDoc {
    pub key: String,
    /// Uma por língua que tem esta página, na ordem do manifesto.
    pub sides: Vec<Side>,
}

pub fn load(site: &Path, m: &Manifest, key: &str) -> Result<PageDoc, String> {
    let mut sides = vec![];
    for l in &m.langs {
        let Some(rel) = l.paginas.get(key) else { continue };
        let text = fs::read_to_string(site.join(rel)).map_err(|e| format!("Não consegui ler {rel}: {e}"))?;
        let (lines, body) = fm::split(&text);
        sides.push(Side { lang: l.codigo.clone(), rel: rel.clone(), title: fm::get(&lines, "title").unwrap_or_default(), body });
    }
    Ok(PageDoc { key: key.to_string(), sides })
}

/// Blocos `:::{#id}` onde o Quarto encaixa as listas; não podem sumir.
pub fn listing_slots(body: &str) -> Vec<String> {
    body.lines().map(str::trim).filter(|l| l.starts_with(":::{#")).map(str::to_string).collect()
}

/// Monta o novo conteúdo do arquivo (ou None se nada mudou), sem gravar.
fn prepare(site: &Path, side: &Side) -> Result<Option<String>, String> {
    let rel = &side.rel;
    let text = fs::read_to_string(site.join(rel)).map_err(|e| format!("Não consegui ler {rel}: {e}"))?;
    let (lines, old_body) = fm::split(&text);
    for slot in listing_slots(&old_body) {
        if !side.body.lines().any(|l| l.trim() == slot) {
            return Err(format!(
                "{rel}: o bloco {slot} sumiu do texto. Ele é onde o site mostra uma lista automática — recoloque-o (seguido de uma linha com :::)."
            ));
        }
    }
    if side.title.trim().is_empty() {
        return Err(format!("{rel}: a página precisa de um título."));
    }
    let lines = fm::set_fields(&lines, &[("title", format!(" {}", fm::quote(side.title.trim())))]);
    let out = fm::join(&lines, &side.body);
    Ok(if out != text { Some(out) } else { None })
}

/// Valida todas as línguas antes de gravar qualquer uma.
pub fn save(site: &Path, doc: &PageDoc) -> Result<(), String> {
    let mut writes = vec![];
    for side in &doc.sides {
        writes.push((&side.rel, prepare(site, side)?));
    }
    for (rel, out) in writes {
        if let Some(out) = out {
            fs::write(site.join(rel), out).map_err(|e| format!("Não consegui salvar {rel}: {e}"))?;
        }
    }
    Ok(())
}

/// Caminho da página nova numa língua: na raiz, `<slug>.qmd`; nas outras,
/// `<código>/<slug>.qmd`.
pub fn new_page_path(m: &Manifest, code: &str, slug: &str) -> String {
    if code == m.root().codigo { format!("{slug}.qmd") } else { format!("{code}/{slug}.qmd") }
}

/// Cria uma página em todas as línguas, registra no manifesto e (se pedido)
/// põe no menu de cada perfil. `titles` vem na ordem de `m.langs`.
pub fn create(site: &Path, m: &Manifest, slug: &str, titles: &[String], in_menu: bool) -> Result<(), String> {
    use crate::languages::config_file;
    use crate::yamlpath as yp;
    if !crate::posts::valid_slug(slug) {
        return Err("Use só letras minúsculas, números e hífens no nome do arquivo (ex.: contato).".into());
    }
    if m.page_keys().iter().any(|k| k == slug) {
        return Err(format!("Já existe uma página “{slug}”."));
    }
    if titles.len() != m.langs.len() || titles.iter().any(|t| t.trim().is_empty()) {
        return Err("Dê um título à página em todas as línguas.".into());
    }
    let paths: Vec<String> = m.langs.iter().map(|l| new_page_path(m, &l.codigo, slug)).collect();
    if let Some(p) = paths.iter().find(|p| site.join(p).exists()) {
        return Err(format!("O arquivo {p} já existe."));
    }
    // monta tudo antes de gravar
    let mut manifest_text = fs::read_to_string(site.join(crate::manifest::FILE)).map_err(|e| e.to_string())?;
    for (l, p) in m.langs.iter().zip(&paths) {
        manifest_text = crate::manifest::edit_pages_line(&manifest_text, &l.codigo, slug, Some(p))?;
    }
    let mut profiles: Vec<(String, String)> = vec![];
    if in_menu {
        for ((l, p), title) in m.langs.iter().zip(&paths).zip(titles) {
            let file = config_file(site, &l.codigo);
            let text = match profiles.iter().find(|(f, _)| *f == file) {
                Some((_, t)) => t.clone(),
                None => fs::read_to_string(site.join(&file)).map_err(|e| format!("Não consegui ler {file}: {e}"))?,
            };
            let mut lines = yp::file_lines(&text);
            let left = ["website", "navbar", "left"];
            let (_, _, mut items) = yp::items(&lines, &left).ok_or_else(|| format!("{file} não tem website.navbar.left."))?;
            let dash = yp::dash_indent(&items, 6);
            items.push(yp::new_item(&dash, &[("href", p), ("text", title.trim())]));
            yp::replace_items(&mut lines, &left, &items)?;
            profiles.retain(|(f, _)| *f != file);
            profiles.push((file, yp::join_file(&lines)));
        }
    }
    for (p, title) in paths.iter().zip(titles) {
        if let Some(dir) = site.join(p).parent() {
            fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        fs::write(site.join(p), fm::join(&[format!("title: {}", fm::quote(title.trim()))], "")).map_err(|e| e.to_string())?;
    }
    for (file, text) in profiles {
        fs::write(site.join(file), text).map_err(|e| e.to_string())?;
    }
    fs::write(site.join(crate::manifest::FILE), manifest_text).map_err(|e| e.to_string())
}

/// Remove uma página de todas as línguas, do manifesto e do menu. Devolve os
/// caminhos alterados (para o envio ao site).
pub fn remove(site: &Path, m: &Manifest, key: &str) -> Result<Vec<String>, String> {
    use crate::languages::config_file;
    use crate::yamlpath as yp;
    if crate::manifest::PROTECTED_PAGES.contains(&key) {
        return Err("Esta página faz parte da estrutura do site e não pode ser removida.".into());
    }
    let mut manifest_text = fs::read_to_string(site.join(crate::manifest::FILE)).map_err(|e| e.to_string())?;
    let mut changed = vec![crate::manifest::FILE.to_string()];
    let mut files = vec![];
    let mut profiles: Vec<(String, String)> = vec![];
    for l in &m.langs {
        let Some(rel) = l.paginas.get(key) else { continue };
        manifest_text = crate::manifest::edit_pages_line(&manifest_text, &l.codigo, key, None)?;
        files.push(rel.clone());
        let file = config_file(site, &l.codigo);
        let text = match profiles.iter().find(|(f, _): &&(String, String)| *f == file) {
            Some((_, t)) => t.clone(),
            None => match fs::read_to_string(site.join(&file)) {
                Ok(t) => t,
                Err(_) => continue,
            },
        };
        let mut lines = yp::file_lines(&text);
        let left = ["website", "navbar", "left"];
        if let Some((_, _, items)) = yp::items(&lines, &left) {
            let kept: Vec<Vec<String>> = items.iter().filter(|it| yp::item_get(it, "href").as_deref() != Some(rel.as_str())).cloned().collect();
            if kept.len() != items.len() {
                yp::replace_items(&mut lines, &left, &kept)?;
                profiles.retain(|(f, _)| *f != file);
                profiles.push((file, yp::join_file(&lines)));
            }
        }
    }
    for rel in &files {
        if site.join(rel).exists() {
            fs::remove_file(site.join(rel)).map_err(|e| format!("Não consegui remover {rel}: {e}"))?;
        }
        changed.push(rel.clone());
    }
    for (file, text) in profiles {
        fs::write(site.join(&file), text).map_err(|e| e.to_string())?;
        changed.push(file);
    }
    fs::write(site.join(crate::manifest::FILE), manifest_text).map_err(|e| e.to_string())?;
    Ok(changed)
}
