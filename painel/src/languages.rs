//! Criação de uma língua nova no site.
//!
//! A língua nova é uma cópia da estrutura de uma língua existente que não é a
//! da raiz (ex.: pt/ → ja/): mesmas páginas, listas e página de postagens,
//! com o conteúdo ainda na língua de origem, pronto para ser traduzido no
//! painel. Também cria o perfil do Quarto (_quarto-<código>.yml), acrescenta
//! o link da língua no menu de todas as versões e registra a língua no
//! _idiomas.toml. Nada é enviado ao site até você pedir.

use std::fs;
use std::path::Path;

use crate::frontmatter as fm;
use crate::manifest::{self, Lang, Manifest};

pub fn profile_file(code: &str) -> String {
    format!("_quarto-{code}.yml")
}

/// Arquivo com o menu e o rodapé de uma língua: o perfil dela, ou o
/// _quarto.yml em sites sem perfis por língua.
pub fn config_file(site: &Path, code: &str) -> String {
    let p = profile_file(code);
    if site.join(&p).is_file() { p } else { "_quarto.yml".into() }
}

/// Rótulos do menu já traduzidos para algumas línguas comuns.
fn known_label(code: &str, key: &str) -> Option<&'static str> {
    let table: &[(&str, [&str; 5])] = &[
        ("ja", ["プロフィール", "研究", "ブログ", "教育", "履歴書"]),
        ("en", ["About", "Research", "Posts", "Teaching", "CV"]),
        ("es", ["Sobre mí", "Investigación", "Publicaciones", "Docencia", "CV"]),
        ("fr", ["À propos", "Recherche", "Billets", "Enseignement", "CV"]),
        ("de", ["Über mich", "Forschung", "Beiträge", "Lehre", "Lebenslauf"]),
        ("it", ["Chi sono", "Ricerca", "Articoli", "Didattica", "CV"]),
        ("zh", ["关于", "研究", "文章", "教学", "简历"]),
        ("ko", ["소개", "연구", "글", "강의", "이력서"]),
    ];
    let idx = ["sobre", "pesquisa", "postagens", "ensino", "cv"].iter().position(|k| *k == key)?;
    let base = code.split('-').next().unwrap_or(code);
    table.iter().find(|(c, _)| *c == base).map(|(_, l)| l[idx])
}

/// Itens do menu da língua de origem: (href, rótulo sugerido para a nova).
pub fn nav_labels(site: &Path, m: &Manifest, source: &str, new_code: &str) -> Vec<(String, String)> {
    let Ok(text) = fs::read_to_string(site.join(profile_file(source))) else { return vec![] };
    let lang = m.lang(source);
    let lines: Vec<&str> = text.lines().collect();
    let mut out = vec![];
    for (i, l) in lines.iter().enumerate() {
        let Some(href) = l.trim_start().strip_prefix("- href: ") else { continue };
        let Some(label) = lines.get(i + 1).and_then(|n| n.trim_start().strip_prefix("text: ")) else { continue };
        let href = href.trim().to_string();
        let key = lang.and_then(|lg| {
            lg.paginas.iter().find(|(_, v)| **v == href).map(|(k, _)| k.clone()).or_else(|| {
                lg.postagens.as_ref().filter(|d| href == format!("{d}/index.qmd")).map(|_| "postagens".to_string())
            })
        });
        let suggestion = key.as_deref().and_then(|k| known_label(new_code, k)).map(str::to_string);
        out.push((href, suggestion.unwrap_or_else(|| fm::scalar(label))));
    }
    out
}

/// Valor de `lang:` do HTML. Sempre com região (ja-JP, como o pt-BR do
/// perfil português): o Quarto lê um `lang: ja` como caminho quando existe
/// uma pasta ja/ no projeto e gera lang="../ja".
fn html_lang(code: &str) -> String {
    if code.contains('-') {
        let (a, b) = code.split_once('-').unwrap();
        return format!("{a}-{}", b.to_uppercase());
    }
    let region = match code {
        "ja" => "JP",
        "en" => "US",
        "pt" => "BR",
        "zh" => "CN",
        "ko" => "KR",
        "es" => "ES",
        "fr" => "FR",
        "de" => "DE",
        "it" => "IT",
        _ => return format!("{code}-{}", code.to_uppercase()),
    };
    format!("{code}-{region}")
}

pub fn valid_code(code: &str) -> bool {
    let mut parts = code.splitn(2, '-');
    let base = parts.next().unwrap_or("");
    let ok_base = (2..=3).contains(&base.len()) && base.chars().all(|c| c.is_ascii_lowercase());
    let ok_rest = parts.next().is_none_or(|r| (2..=8).contains(&r.len()) && r.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
    ok_base && ok_rest
}

/// Índices (linha do `- text:`) dos links de língua de um perfil.
fn lang_links(lines: &[String]) -> Vec<usize> {
    (0..lines.len().saturating_sub(1))
        .filter(|&i| {
            let href = lines[i + 1].trim();
            lines[i].trim_start().starts_with("- text:")
                && href.strip_prefix("href: /").is_some_and(|rest| {
                    rest.is_empty() || (rest.ends_with('/') && valid_code(&rest[..rest.len() - 1]))
                })
        })
        .collect()
}

fn link_lines(indent: &str, l: &Lang, root: &Lang) -> [String; 2] {
    let href = if l.codigo == root.codigo { "/".to_string() } else { format!("/{}/", l.codigo) };
    [format!("{indent}- text: {}", fm::quote(&l.nome)), format!("{indent}  href: {href}")]
}

/// Perfil da língua nova a partir do perfil da língua de origem.
fn new_profile(src_text: &str, src: &Lang, new: &Lang, m: &Manifest, labels: &[(String, String)]) -> Result<String, String> {
    let (s, c) = (&src.codigo, &new.codigo);
    let mut lines: Vec<String> = src_text
        .lines()
        .skip_while(|l| l.starts_with('#'))
        .map(|l| {
            l.replace(&format!("_site-{s}"), &format!("_site-{c}"))
                .replace(&format!("\"{s}/\""), &format!("\"{c}/\""))
                .replace(&format!("href: {s}/"), &format!("href: {c}/"))
        })
        .collect();
    // rótulos do menu, na ordem
    let mut next = labels.iter();
    for i in 0..lines.len().saturating_sub(1) {
        if lines[i].trim_start().starts_with("- href: ") && lines[i + 1].trim_start().starts_with("text: ") {
            if let Some((_, label)) = next.next() {
                let indent: String = lines[i + 1].chars().take_while(|c| c.is_whitespace()).collect();
                lines[i + 1] = format!("{indent}text: {}", fm::quote(label));
            }
        }
    }
    // links de língua: todos menos a própria
    let links = lang_links(&lines);
    let Some(&first) = links.first() else {
        return Err(format!("{} não tem links de língua no menu; não sei onde pôr os da língua nova.", profile_file(s)));
    };
    let indent: String = lines[first].chars().take_while(|c| c.is_whitespace()).collect();
    for &i in links.iter().rev() {
        lines.drain(i..i + 2);
    }
    let fresh: Vec<String> = m.langs.iter().filter(|l| l.codigo != *c).flat_map(|l| link_lines(&indent, l, m.root())).collect();
    lines.splice(first..first, fresh);
    for l in &mut lines {
        if l.trim_start().starts_with("lang: ") {
            let indent: String = l.chars().take_while(|c| c.is_whitespace()).collect();
            *l = format!("{indent}lang: {}", html_lang(c));
        }
    }
    let header = format!(
        "# Versão em {} — renderizada em /{c}/ (perfil \"{c}\").\n# Criada pelo Painel do Site a partir da versão \"{s}\"; o build.sh copia\n# _site-{c}/{c}/ para _site/{c}/.\n",
        new.nome
    );
    Ok(header + &lines.join("\n") + "\n")
}

/// Perfil existente com o link da língua nova acrescentado.
fn add_link(text: &str, new: &Lang, root: &Lang) -> Option<String> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let last = *lang_links(&lines).last()?;
    let indent: String = lines[last].chars().take_while(|c| c.is_whitespace()).collect();
    lines.splice(last + 2..last + 2, link_lines(&indent, new, root));
    Some(lines.join("\n") + "\n")
}

fn retarget(path: &str, src: &str, code: &str) -> String {
    path.strip_prefix(&format!("{src}/")).map(|rest| format!("{code}/{rest}")).unwrap_or_else(|| path.to_string())
}

pub fn manifest_block(l: &Lang) -> String {
    let table = |m: &std::collections::BTreeMap<String, String>| {
        m.iter().map(|(k, v)| format!("{k} = {}", fm::quote(v))).collect::<Vec<_>>().join(", ")
    };
    let mut out = format!(
        "\n[[idioma]]\ncodigo = {}\nnome = {}\npaginas = {{ {} }}\nlistas = {{ {} }}\n",
        fm::quote(&l.codigo),
        fm::quote(&l.nome),
        table(&l.paginas),
        table(&l.listas)
    );
    if let Some(d) = &l.postagens {
        out += &format!("postagens = {}\n", fm::quote(d));
    }
    if let Some(d) = &l.lista_papers {
        out += &format!("lista_papers = {}\n", fm::quote(d));
    }
    out
}

fn copy_tree(from: &Path, to: &Path, skip_under: Option<&Path>) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|e| format!("Não consegui criar {}: {e}", to.display()))?;
    for entry in fs::read_dir(from).map_err(|e| e.to_string())?.flatten() {
        let name = entry.file_name();
        if name == ".DS_Store" {
            continue;
        }
        let (src, dst) = (entry.path(), to.join(&name));
        if src.is_dir() {
            // pastas das postagens individuais não são copiadas
            if skip_under.is_some_and(|p| src.parent() == Some(p)) {
                continue;
            }
            copy_tree(&src, &dst, skip_under)?;
        } else {
            fs::copy(&src, &dst).map_err(|e| format!("Não consegui copiar {}: {e}", src.display()))?;
        }
    }
    Ok(())
}

/// Cria a língua. Devolve os caminhos alterados (para o envio ao site) e
/// avisos sobre perfis que não puderam receber o link automaticamente.
pub fn add(site: &Path, m: &Manifest, code: &str, name: &str, source: &str, labels: &[(String, String)]) -> Result<(Vec<String>, Vec<String>), String> {
    let code = code.trim().to_lowercase();
    if !valid_code(&code) {
        return Err("Use um código de língua como ja, es, fr ou zh-tw.".into());
    }
    if name.trim().is_empty() {
        return Err("Informe o nome da língua (como aparece no menu, ex.: 日本語).".into());
    }
    if m.lang(&code).is_some() || site.join(&code).exists() || site.join(profile_file(&code)).exists() {
        return Err(format!("Já existe uma língua ou pasta “{code}” no site."));
    }
    let src = m.lang(source).ok_or("Língua de origem desconhecida.")?;
    if src.codigo == m.root().codigo {
        return Err("Escolha como origem uma língua que fica numa subpasta (ex.: pt), para a estrutura ser a mesma.".into());
    }
    let src_profile = fs::read_to_string(site.join(profile_file(source))).map_err(|e| format!("Não consegui ler {}: {e}", profile_file(source)))?;

    let s = &src.codigo;
    let new = Lang {
        codigo: code.clone(),
        nome: name.trim().to_string(),
        paginas: src.paginas.iter().map(|(k, v)| (k.clone(), retarget(v, s, &code))).collect(),
        listas: src.listas.iter().map(|(k, v)| (k.clone(), retarget(v, s, &code))).collect(),
        postagens: src.postagens.as_ref().map(|v| retarget(v, s, &code)),
        papers: None,
        lista_papers: src.lista_papers.as_ref().map(|v| retarget(v, s, &code)),
    };
    let mut with_new = m.clone();
    with_new.langs.push(new.clone());
    // Tudo é montado em memória antes de gravar qualquer arquivo.
    let profile = new_profile(&src_profile, src, &new, &with_new, labels)?;
    let mut updates = vec![];
    let mut warnings = vec![];
    for l in &m.langs {
        let file = profile_file(&l.codigo);
        match fs::read_to_string(site.join(&file)).ok().and_then(|t| add_link(&t, &new, m.root())) {
            Some(text) => updates.push((file, text)),
            None => warnings.push(format!("Acrescente à mão o link “{}” no menu de {file}.", new.nome)),
        }
    }
    let manifest_text = fs::read_to_string(site.join(manifest::FILE)).map_err(|e| e.to_string())?;

    let skip = src.postagens.as_ref().map(|d| site.join(d));
    copy_tree(&site.join(s), &site.join(&code), skip.as_deref())?;
    fs::write(site.join(profile_file(&code)), profile).map_err(|e| e.to_string())?;
    let mut changed = vec![code.clone(), profile_file(&code), manifest::FILE.to_string()];
    for (file, text) in updates {
        fs::write(site.join(&file), text).map_err(|e| e.to_string())?;
        changed.push(file);
    }
    fs::write(site.join(manifest::FILE), manifest_text.trim_end().to_string() + "\n" + &manifest_block(&new)).map_err(|e| e.to_string())?;
    Ok((changed, warnings))
}

/// Caminhos que formam a configuração de uma língua (para enviar ao site).
pub fn setup_paths(m: &Manifest, code: &str) -> Vec<String> {
    let mut v = vec![code.to_string(), manifest::FILE.to_string()];
    v.extend(m.langs.iter().map(|l| profile_file(&l.codigo)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes() {
        assert!(valid_code("ja") && valid_code("zh-tw") && valid_code("pt-br"));
        assert!(!valid_code("JA") && !valid_code("j") && !valid_code("../x") && !valid_code("research"));
        assert_eq!((html_lang("ja"), html_lang("zh-tw"), html_lang("nl")), ("ja-JP".into(), "zh-TW".into(), "nl-NL".into()));
    }
}
