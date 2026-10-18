//! Listas da página de Pesquisa (research/*.yml e pt/pesquisa/*.yml).
//!
//! Cada arquivo é um cabeçalho de comentários seguido de itens no formato
//!
//! ```yaml
//! - title: "…"
//!   author: "…"
//!   date: 2022-10-28
//!   description: "…"
//!   path: "…"
//! ```
//!
//! O painel só edita arquivos nesse formato simples e confere, ao abrir, que
//! regravar sem mudanças reproduz o arquivo byte a byte. Assim, um campo que
//! você não mexeu nunca muda de forma.

use std::fs;
use std::path::Path;

use crate::frontmatter as fm;
use crate::manifest::Manifest;

/// Um item de um arquivo: pares (chave, valor bruto como está no arquivo).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Item {
    pub fields: Vec<(String, String)>,
}

impl Item {
    pub fn get(&self, key: &str) -> String {
        self.fields.iter().find(|(k, _)| k == key).map(|(_, v)| fm::scalar(v)).unwrap_or_default()
    }

    /// Só reescreve o valor bruto se o valor de fato mudou.
    pub fn set(&mut self, key: &str, value: &str) {
        if self.get(key) == value && self.fields.iter().any(|(k, _)| k == key) {
            return;
        }
        let raw = if key == "date" { fm::value(value) } else { fm::quote(value) };
        match self.fields.iter_mut().find(|(k, _)| k == key) {
            Some(f) => f.1 = raw,
            None => self.fields.push((key.to_string(), raw)),
        }
    }

    pub fn new(title: &str, author: &str, date: &str, description: &str, path: &str) -> Item {
        let mut it = Item::default();
        for (k, v) in [("title", title), ("author", author), ("date", date), ("description", description), ("path", path)] {
            it.set(k, v);
        }
        it
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListFile {
    pub header: String,
    pub items: Vec<Item>,
}

pub fn parse(text: &str) -> Result<ListFile, String> {
    let mut file = ListFile::default();
    let mut started = false;
    for (n, line) in text.lines().enumerate() {
        if !started && !line.starts_with("- ") {
            file.header.push_str(line);
            file.header.push('\n');
            continue;
        }
        started = true;
        let bad = || format!("linha {}: formato que o painel não sabe editar: {line}", n + 1);
        if line.trim().is_empty() {
            continue;
        }
        let (rest, new_item) = if let Some(r) = line.strip_prefix("- ") {
            (r, true)
        } else if let Some(r) = line.strip_prefix("  ") {
            (r, false)
        } else {
            return Err(bad());
        };
        let (key, val) = fm::top_key(rest).ok_or_else(bad)?;
        let val = val.trim();
        if val.is_empty() || val.starts_with('|') || val.starts_with('>') || val.starts_with('[') || val.starts_with('{') {
            return Err(bad());
        }
        if new_item {
            file.items.push(Item::default());
        }
        let item = file.items.last_mut().ok_or_else(bad)?;
        item.fields.push((key.to_string(), val.to_string()));
    }
    Ok(file)
}

pub fn render(file: &ListFile) -> String {
    let items: Vec<String> = file
        .items
        .iter()
        .map(|it| {
            it.fields
                .iter()
                .enumerate()
                .map(|(i, (k, v))| format!("{}{k}: {v}", if i == 0 { "- " } else { "  " }))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect();
    format!("{}{}\n", file.header, items.join("\n\n"))
}

/// Uma lista numa língua.
#[derive(Clone, Debug, PartialEq)]
pub struct LangList {
    pub lang: String,
    pub rel: String,
    pub file: ListFile,
}

/// A mesma lista em todas as línguas; os itens são pareados pela posição.
#[derive(Clone, Debug, PartialEq)]
pub struct ListDoc {
    pub key: String,
    pub langs: Vec<LangList>,
}

impl ListDoc {
    pub fn len(&self) -> usize {
        self.langs.iter().map(|l| l.file.items.len()).max().unwrap_or(0)
    }

    /// (título, ano) para a lista de itens: a primeira língua que tiver título.
    pub fn label(&self, i: usize) -> (String, String) {
        let pick = |k: &str| self.langs.iter().filter_map(|l| l.file.items.get(i)).map(|it| it.get(k)).find(|s| !s.is_empty());
        (pick("title").unwrap_or("(sem título)".into()), pick("date").unwrap_or_default().chars().take(4).collect())
    }

    /// Valor de um campo comum (data, link) do item `i`.
    pub fn shared(&self, i: usize, key: &str) -> String {
        self.langs.iter().find_map(|l| l.file.items.get(i)).map(|it| it.get(key)).unwrap_or_default()
    }

    pub fn set_shared(&mut self, i: usize, key: &str, value: &str) {
        for l in &mut self.langs {
            if let Some(it) = l.file.items.get_mut(i) {
                it.set(key, value);
            }
        }
    }

    pub fn add(&mut self, date: &str) {
        let item = Item::new("", "Gustavo H. Moreira", date, "", "");
        for l in &mut self.langs {
            l.file.items.insert(0, item.clone());
        }
    }

    pub fn remove(&mut self, i: usize) {
        for l in &mut self.langs {
            if i < l.file.items.len() {
                l.file.items.remove(i);
            }
        }
    }

    /// Cria o item `i` na língua `lang` copiando de outra língua que o tenha.
    pub fn copy_into(&mut self, i: usize, lang: usize) {
        let Some(src) = self.langs.iter().find_map(|l| l.file.items.get(i)).cloned() else { return };
        let items = &mut self.langs[lang].file.items;
        let at = i.min(items.len());
        items.insert(at, src);
    }

    pub fn validate(&self) -> Result<(), String> {
        for l in &self.langs {
            for (i, it) in l.file.items.iter().enumerate() {
                if it.get("title").trim().is_empty() {
                    return Err(format!("Item {} ({}) está sem título.", i + 1, l.lang));
                }
                if !fm::is_date(&it.get("date")) {
                    return Err(format!("Item {} ({}): a data deve estar no formato AAAA-MM-DD.", i + 1, l.lang));
                }
            }
        }
        Ok(())
    }
}

fn read(site: &Path, rel: &str) -> Result<ListFile, String> {
    let text = fs::read_to_string(site.join(rel)).map_err(|e| format!("Não consegui ler {rel}: {e}"))?;
    let file = parse(&text).map_err(|e| format!("{rel}, {e}"))?;
    if render(&file) != text {
        return Err(format!("{rel} tem uma formatação que o painel não reproduz com segurança; edite esse arquivo à mão."));
    }
    Ok(file)
}

pub fn load(site: &Path, m: &Manifest, key: &str) -> Result<ListDoc, String> {
    let mut langs = vec![];
    for l in &m.langs {
        if let Some(rel) = l.listas.get(key) {
            langs.push(LangList { lang: l.codigo.clone(), rel: rel.clone(), file: read(site, rel)? });
        }
    }
    Ok(ListDoc { key: key.to_string(), langs })
}

pub fn save(site: &Path, doc: &ListDoc) -> Result<(), String> {
    doc.validate()?;
    for l in &doc.langs {
        let out = render(&l.file);
        if fs::read_to_string(site.join(&l.rel)).ok().as_deref() != Some(out.as_str()) {
            fs::write(site.join(&l.rel), out).map_err(|e| format!("Não consegui salvar {}: {e}", l.rel))?;
        }
    }
    Ok(())
}

/// Página que mostra a lista (o index.qmd da mesma pasta).
pub fn page_of(rel: &str) -> String {
    match rel.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/index.qmd"),
        None => "index.qmd".into(),
    }
}

/// Lê/grava um único arquivo de lista (usado pelos working papers em PT).
pub fn read_file(site: &Path, rel: &str) -> Result<ListFile, String> {
    read(site, rel)
}

pub fn write_file(site: &Path, rel: &str, file: &ListFile) -> Result<(), String> {
    fs::write(site.join(rel), render(file)).map_err(|e| format!("Não consegui salvar {rel}: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_files_roundtrip() {
        let site = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let m = Manifest::load(site).unwrap();
        for key in m.list_keys() {
            assert_eq!(load(site, &m, &key).unwrap().langs.len(), m.langs.len());
        }
        read_file(site, "pt/pesquisa/trabalhos.yml").unwrap();
        assert_eq!(page_of("pt/pesquisa/artigos.yml"), "pt/pesquisa/index.qmd");
    }

    #[test]
    fn set_keeps_untouched_raw() {
        let mut f = parse("# c\n- title: 'Old'\n  date: 2020-01-01\n").unwrap();
        f.items[0].set("title", "Old");
        assert_eq!(render(&f), "# c\n- title: 'Old'\n  date: 2020-01-01\n");
        f.items[0].set("title", "Novo \"x\"");
        assert!(render(&f).contains("- title: \"Novo \\\"x\\\"\""));
        assert!(parse("- title:\n    - a\n").is_err());
    }
}
