//! Línguas do site, lidas de `_idiomas.toml` na raiz do site.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

pub const FILE: &str = "_idiomas.toml";

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Lang {
    pub codigo: String,
    pub nome: String,
    #[serde(default)]
    pub paginas: BTreeMap<String, String>,
    #[serde(default)]
    pub listas: BTreeMap<String, String>,
    pub postagens: Option<String>,
    pub papers: Option<String>,
    pub lista_papers: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Manifest {
    #[serde(rename = "idioma")]
    pub langs: Vec<Lang>,
}

/// Nomes amigáveis e ordem de exibição das chaves conhecidas.
const PAGE_NAMES: [(&str, &str); 5] =
    [("sobre", "Sobre"), ("pesquisa", "Pesquisa (introdução)"), ("postagens", "Postagens (página da lista)"), ("ensino", "Ensino"), ("cv", "CV")];
const LIST_NAMES: [(&str, &str); 4] =
    [("artigos", "Artigos completos"), ("resumos", "Resumos e anais"), ("conclusao", "Trabalhos de conclusão"), ("outros", "Outras produções")];

fn ordered(keys: impl Iterator<Item = String>, known: &[(&str, &str)]) -> Vec<String> {
    let mut all: Vec<String> = keys.collect();
    all.sort();
    all.dedup();
    all.sort_by_key(|k| known.iter().position(|(n, _)| n == k).unwrap_or(usize::MAX));
    all
}

fn display(key: &str, known: &[(&str, &str)]) -> String {
    known.iter().find(|(k, _)| *k == key).map(|(_, n)| n.to_string()).unwrap_or_else(|| {
        let mut c = key.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    })
}

pub fn page_name(key: &str) -> String {
    display(key, &PAGE_NAMES)
}

pub fn list_name(key: &str) -> String {
    display(key, &LIST_NAMES)
}

impl Manifest {
    pub fn load(site: &Path) -> Result<Manifest, String> {
        let text = std::fs::read_to_string(site.join(FILE)).map_err(|e| format!("Não consegui ler {FILE}: {e}"))?;
        let m: Manifest = toml::from_str(&text).map_err(|e| format!("{FILE} inválido: {e}"))?;
        if m.langs.is_empty() {
            return Err(format!("{FILE} não lista nenhuma língua."));
        }
        Ok(m)
    }

    pub fn root(&self) -> &Lang {
        &self.langs[0]
    }

    pub fn lang(&self, code: &str) -> Option<&Lang> {
        self.langs.iter().find(|l| l.codigo == code)
    }

    pub fn page_keys(&self) -> Vec<String> {
        ordered(self.langs.iter().flat_map(|l| l.paginas.keys().cloned()), &PAGE_NAMES)
    }

    pub fn list_keys(&self) -> Vec<String> {
        ordered(self.langs.iter().flat_map(|l| l.listas.keys().cloned()), &LIST_NAMES)
    }

    /// Línguas com pasta de postagens: (código, pasta).
    pub fn post_dirs(&self) -> Vec<(String, String)> {
        self.langs.iter().filter_map(|l| l.postagens.clone().map(|d| (l.codigo.clone(), d))).collect()
    }

    pub fn papers_dir(&self) -> Option<&str> {
        self.root().papers.as_deref()
    }

}

/// Páginas que o site precisa e que o painel não deixa remover.
pub const PROTECTED_PAGES: [&str; 3] = ["sobre", "pesquisa", "postagens"];

/// Edita a tabela `paginas = { … }` de uma língua no texto do _idiomas.toml,
/// preservando comentários e a ordem das outras entradas.
pub fn edit_pages_line(text: &str, code: &str, key: &str, path: Option<&str>) -> Result<String, String> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let code_line = format!("codigo = \"{code}\"");
    let start = lines.iter().position(|l| l.trim() == code_line).ok_or_else(|| format!("língua {code} não está no {FILE}"))?;
    let end = lines[start..].iter().position(|l| l.trim() == "[[idioma]]").map(|n| start + n).unwrap_or(lines.len());
    let entry_of = |k: &str, v: &str| format!("{k} = {}", crate::frontmatter::quote(v));
    match lines[start..end].iter().position(|l| l.trim_start().starts_with("paginas = {")) {
        Some(n) => {
            let l = &lines[start + n];
            let inner = l.split_once('{').and_then(|(_, r)| r.rsplit_once('}')).map(|(i, _)| i.trim()).ok_or("linha paginas inválida")?;
            let mut entries: Vec<String> = inner.split(", ").map(str::trim).filter(|e| !e.is_empty()).map(str::to_string).collect();
            entries.retain(|e| e.split('=').next().map(str::trim) != Some(key));
            if let Some(p) = path {
                entries.push(entry_of(key, p));
            }
            lines[start + n] = format!("paginas = {{ {} }}", entries.join(", "));
        }
        None => {
            if let Some(p) = path {
                let after = lines[start..end].iter().position(|l| l.trim_start().starts_with("nome =")).map(|n| start + n + 1).unwrap_or(start + 1);
                lines.insert(after, format!("paginas = {{ {} }}", entry_of(key, p)));
            }
        }
    }
    let out = lines.join("\n") + "\n";
    toml::from_str::<Manifest>(&out).map_err(|e| format!("{FILE} ficaria inválido: {e}"))?;
    Ok(out)
}

fn lang_display(code: &str) -> String {
    let base = code.split('-').next().unwrap_or(code);
    match base {
        "en" => "English",
        "pt" => "Português",
        "es" => "Español",
        "fr" => "Français",
        "de" => "Deutsch",
        "it" => "Italiano",
        "ja" => "日本語",
        "zh" => "中文",
        "ko" => "한국어",
        _ => code,
    }
    .to_string()
}

/// Páginas de uma pasta: `x.qmd` → chave x (index → sobre) e `d/index.qmd`
/// → chave d. Também devolve a pasta que parece ser de postagens (um
/// index.qmd com `listing:` e subpastas com index.qmd).
fn scan_pages(site: &Path, base: &str, skip: &[String]) -> (BTreeMap<String, String>, Option<String>) {
    let mut pages = BTreeMap::new();
    let mut posts = None;
    let prefix = if base.is_empty() { String::new() } else { format!("{base}/") };
    let Ok(entries) = std::fs::read_dir(site.join(base)) else { return (pages, posts) };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        if name.starts_with(['_', '.']) || skip.contains(&name) {
            continue;
        }
        let path = e.path();
        if path.is_file() {
            if let Some(stem) = name.strip_suffix(".qmd") {
                let key = if stem == "index" { "sobre".to_string() } else { stem.to_string() };
                pages.insert(key, format!("{prefix}{name}"));
            }
        } else if path.join("index.qmd").is_file() {
            let index = std::fs::read_to_string(path.join("index.qmd")).unwrap_or_default();
            let has_posts = std::fs::read_dir(&path).into_iter().flatten().flatten().any(|d| d.path().join("index.qmd").is_file());
            if posts.is_none() && index.contains("listing:") && has_posts {
                posts = Some(format!("{prefix}{name}"));
                pages.insert("postagens".to_string(), format!("{prefix}{name}/index.qmd"));
            } else {
                pages.insert(name.clone(), format!("{prefix}{name}/index.qmd"));
            }
        }
    }
    (pages, posts)
}

/// Texto de um _idiomas.toml inicial para um site que ainda não tem um:
/// a língua da raiz (perfil padrão ou `lang` do _quarto.yml) e uma língua
/// para cada perfil _quarto-<código>.yml com pasta <código>/.
pub fn bootstrap(site: &Path) -> Result<String, String> {
    use crate::yamlpath as yp;
    let cfg = yp::file_lines(&std::fs::read_to_string(site.join("_quarto.yml")).map_err(|e| format!("Não consegui ler _quarto.yml: {e}"))?);
    let root = yp::get(&cfg, &["profile", "default"])
        .or_else(|| yp::get(&cfg, &["format", "html", "lang"]).map(|l| l.split('-').next().unwrap_or("en").to_string()))
        .unwrap_or_else(|| "en".into());
    let mut others: Vec<String> = std::fs::read_dir(site)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            let code = n.strip_prefix("_quarto-")?.strip_suffix(".yml")?.to_string();
            (code != root && site.join(&code).is_dir()).then_some(code)
        })
        .collect();
    others.sort();
    let mut skip = others.clone();
    skip.extend(["painel", "site_libs"].map(String::from));
    let mut langs = vec![];
    for (code, base) in std::iter::once((root.clone(), String::new())).chain(others.iter().map(|c| (c.clone(), c.clone()))) {
        let (paginas, postagens) = scan_pages(site, &base, if base.is_empty() { &skip } else { &[] });
        langs.push(Lang { codigo: code.clone(), nome: lang_display(&code), paginas, listas: BTreeMap::new(), postagens, papers: None, lista_papers: None });
    }
    let mut out = String::from(
        "# Línguas do site, lidas pelo Painel do Site. Criado automaticamente a partir\n\
         # das páginas encontradas; revise à vontade. Para que uma página apareça lado a\n\
         # lado entre línguas, use a mesma chave nas duas (ex.: pesquisa = \"research/index.qmd\"\n\
         # na raiz e pesquisa = \"pt/pesquisa/index.qmd\" no português).\n",
    );
    for l in &langs {
        out += &crate::languages::manifest_block(l);
    }
    toml::from_str::<Manifest>(&out).map_err(|e| format!("manifesto gerado inválido: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_manifest() {
        let site = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let m = Manifest::load(site).unwrap();
        assert_eq!(m.root().codigo, "en");
        assert_eq!(m.page_keys(), vec!["sobre", "pesquisa", "postagens", "ensino", "cv"]);
        assert_eq!(m.list_keys(), vec!["artigos", "resumos", "conclusao", "outros"]);
        let text = std::fs::read_to_string(site.join(FILE)).unwrap();
        let added = edit_pages_line(&text, "pt", "contato", Some("pt/contato.qmd")).unwrap();
        assert!(added.contains("cv = \"pt/cv.qmd\", contato = \"pt/contato.qmd\" }"));
        assert_eq!(edit_pages_line(&added, "pt", "contato", None).unwrap(), text);
        // o manifesto inicial deste site encontra as mesmas línguas e páginas
        let boot: Manifest = toml::from_str(&bootstrap(site).unwrap()).unwrap();
        assert_eq!(boot.langs.iter().map(|l| l.codigo.as_str()).collect::<Vec<_>>(), vec!["en", "pt"]);
        assert_eq!(boot.root().paginas["sobre"], "index.qmd");
        assert_eq!(boot.root().paginas["research"], "research/index.qmd");
        assert_eq!(boot.lang("pt").unwrap().postagens.as_deref(), Some("pt/postagens"));
        for l in &m.langs {
            for rel in l.paginas.values().chain(l.listas.values()) {
                assert!(site.join(rel).is_file(), "{rel} não existe");
            }
        }
    }
}
