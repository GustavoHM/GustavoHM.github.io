//! Configurações do site: título e descrição (_quarto.yml), rodapé de cada
//! língua (page-footer dos perfis), foto e links da página Sobre de cada
//! língua (`image:` e `about.links` no frontmatter).

use std::fs;
use std::path::Path;

use crate::frontmatter as fm;
use crate::languages::config_file;
use crate::manifest::Manifest;
use crate::yamlpath as yp;

#[derive(Clone, Debug, PartialEq)]
pub struct Footer {
    pub lang: String,
    /// Perfil da língua, ou _quarto.yml em sites sem perfis.
    pub file: String,
    pub left: Option<String>,
    pub right: Option<String>,
}

/// Um link da página Sobre: ícone e endereço comuns, texto por língua.
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub icon: String,
    pub href: String,
    /// (linhas brutas do item na língua, texto) — na ordem de `about_pages`.
    pub texts: Vec<(Option<Vec<String>>, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub title: String,
    pub description: String,
    pub footers: Vec<Footer>,
    /// (língua, página Sobre) das línguas que têm essa página.
    pub about_pages: Vec<(String, String)>,
    /// Foto, como caminho a partir da raiz do site (ex.: profile.svg).
    pub photo: String,
    pub links: Vec<Link>,
}

const LINKS: [&str; 2] = ["about", "links"];

/// Caminho relativo à página `page` → caminho a partir da raiz do site.
pub fn from_page(page: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = page.split('/').collect();
    parts.pop();
    for seg in rel.split('/') {
        match seg {
            ".." => {
                parts.pop();
            }
            "." | "" => {}
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Caminho a partir da raiz → relativo à página `page`.
pub fn to_page(page: &str, root_rel: &str) -> String {
    format!("{}{root_rel}", "../".repeat(page.matches('/').count()))
}

fn read_front(site: &Path, rel: &str) -> Result<(Vec<String>, String), String> {
    let text = fs::read_to_string(site.join(rel)).map_err(|e| format!("Não consegui ler {rel}: {e}"))?;
    Ok(fm::split(&text))
}

pub fn load(site: &Path, m: &Manifest) -> Result<Settings, String> {
    let cfg = yp::file_lines(&fs::read_to_string(site.join("_quarto.yml")).map_err(|e| format!("Não consegui ler _quarto.yml: {e}"))?);
    let mut footers = vec![];
    for l in &m.langs {
        let file = config_file(site, &l.codigo);
        let lines = yp::file_lines(&fs::read_to_string(site.join(&file)).unwrap_or_default());
        footers.push(Footer {
            lang: l.codigo.clone(),
            file,
            left: yp::get(&lines, &["website", "page-footer", "left"]),
            right: yp::get(&lines, &["website", "page-footer", "right"]),
        });
    }
    let about_pages: Vec<(String, String)> = m.langs.iter().filter_map(|l| l.paginas.get("sobre").map(|p| (l.codigo.clone(), p.clone()))).collect();
    let mut photo = String::new();
    let mut links: Vec<Link> = vec![];
    for (i, (_, page)) in about_pages.iter().enumerate() {
        let (lines, _) = read_front(site, page)?;
        if photo.is_empty() {
            if let Some(img) = fm::get(&lines, "image") {
                photo = from_page(page, &img);
            }
        }
        let items = yp::items(&lines, &LINKS).map(|(_, _, it)| it).unwrap_or_default();
        for (j, item) in items.into_iter().enumerate() {
            let text = yp::item_get(&item, "text").unwrap_or_default();
            if j >= links.len() {
                links.push(Link {
                    icon: yp::item_get(&item, "icon").unwrap_or_default(),
                    href: yp::item_get(&item, "href").unwrap_or_default(),
                    texts: vec![(None, String::new()); about_pages.len()],
                });
            }
            links[j].texts[i] = (Some(item), text);
        }
    }
    Ok(Settings {
        title: yp::get(&cfg, &["website", "title"]).unwrap_or_default(),
        description: yp::get(&cfg, &["website", "description"]).unwrap_or_default(),
        footers,
        about_pages,
        photo,
        links,
    })
}

impl Settings {
    pub fn add_link(&mut self) {
        let n = self.about_pages.len();
        self.links.push(Link { icon: "link-45deg".into(), href: "https://".into(), texts: vec![(None, String::new()); n] });
    }

    pub fn move_link(&mut self, i: usize, up: bool) {
        let j = if up { i.checked_sub(1) } else { (i + 1 < self.links.len()).then_some(i + 1) };
        if let Some(j) = j {
            self.links.swap(i, j);
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() {
            return Err("O site precisa de um título.".into());
        }
        for l in &self.links {
            if l.href.trim().is_empty() || l.texts.iter().any(|(_, t)| t.trim().is_empty()) {
                return Err("Cada link da página Sobre precisa de endereço e de texto em todas as línguas.".into());
            }
        }
        Ok(())
    }

    /// Arquivos que as configurações gravam.
    pub fn paths(&self) -> Vec<String> {
        let mut v = vec!["_quarto.yml".to_string()];
        v.extend(self.footers.iter().map(|f| f.file.clone()));
        v.sort();
        v.dedup();
        v.extend(self.about_pages.iter().map(|(_, p)| p.clone()));
        if !self.photo.is_empty() {
            v.push(self.photo.clone());
        }
        v
    }
}

pub fn save(site: &Path, s: &Settings) -> Result<(), String> {
    s.validate()?;
    // arquivos editados em memória (o mesmo arquivo pode ter vários campos)
    let mut files: Vec<(String, String, Vec<String>)> = vec![];
    fn open<'a>(files: &'a mut Vec<(String, String, Vec<String>)>, site: &Path, rel: &str) -> Result<&'a mut Vec<String>, String> {
        if let Some(i) = files.iter().position(|(r, _, _)| r == rel) {
            return Ok(&mut files[i].2);
        }
        let old = fs::read_to_string(site.join(rel)).map_err(|e| format!("Não consegui ler {rel}: {e}"))?;
        let lines = yp::file_lines(&old);
        files.push((rel.to_string(), old, lines));
        Ok(&mut files.last_mut().unwrap().2)
    }

    let cfg = open(&mut files, site, "_quarto.yml")?;
    yp::set(cfg, &["website", "title"], s.title.trim())?;
    yp::set(cfg, &["website", "description"], s.description.trim())?;
    for f in &s.footers {
        let lines = open(&mut files, site, &f.file)?;
        for (side, value) in [("left", &f.left), ("right", &f.right)] {
            if let Some(v) = value {
                yp::set(lines, &["website", "page-footer", side], v.trim()).map_err(|e| format!("{}: {e}", f.file))?;
            }
        }
    }
    let mut writes: Vec<(String, String)> =
        files.into_iter().map(|(rel, old, lines)| (rel, old, yp::join_file(&lines))).filter(|(_, old, new)| old != new).map(|(r, _, n)| (r, n)).collect();

    for (i, (_, page)) in s.about_pages.iter().enumerate() {
        let old = fs::read_to_string(site.join(page)).map_err(|e| e.to_string())?;
        let (lines, body) = fm::split(&old);
        let mut lines = lines;
        if !s.photo.is_empty() {
            let rel = to_page(page, &s.photo);
            if fm::get(&lines, "image").map(|img| from_page(page, &img)) != Some(s.photo.clone()) {
                lines = fm::set_fields(&lines, &[("image", format!(" {rel}"))]);
            }
        }
        if let Some((_, _, old_items)) = yp::items(&lines, &LINKS) {
            let dash = yp::dash_indent(&old_items, 4);
            let items: Vec<Vec<String>> = s
                .links
                .iter()
                .map(|l| {
                    let (raw, text) = &l.texts[i];
                    match raw {
                        Some(raw) => {
                            let mut it = raw.clone();
                            yp::item_set(&mut it, "icon", l.icon.trim());
                            yp::item_set(&mut it, "text", text.trim());
                            yp::item_set(&mut it, "href", l.href.trim());
                            it
                        }
                        None => yp::new_item(&dash, &[("icon", l.icon.trim()), ("text", text.trim()), ("href", l.href.trim())]),
                    }
                })
                .collect();
            yp::replace_items(&mut lines, &LINKS, &items)?;
        }
        let new = fm::join(&lines, &body);
        if new != old {
            writes.push((page.clone(), new));
        }
    }

    for (rel, text) in writes {
        fs::write(site.join(&rel), text).map_err(|e| format!("Não consegui salvar {rel}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths() {
        assert_eq!(from_page("pt/index.qmd", "../profile.svg"), "profile.svg");
        assert_eq!(from_page("index.qmd", "profile.svg"), "profile.svg");
        assert_eq!(to_page("pt/index.qmd", "foto.jpg"), "../foto.jpg");
        assert_eq!(to_page("index.qmd", "foto.jpg"), "foto.jpg");
    }

    #[test]
    fn reads_real_site() {
        let site = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let m = Manifest::load(site).unwrap();
        let s = load(site, &m).unwrap();
        assert_eq!(s.title, "Gustavo Hoffmann Moreira");
        assert_eq!(s.photo, "profile.svg");
        assert_eq!(s.links.len(), 5);
        assert_eq!(s.links[2].texts[1].1, "Currículo Lattes");
        assert!(s.footers.iter().all(|f| f.left.is_some() && f.right.is_some()));
    }
}
