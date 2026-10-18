//! Edição cirúrgica de YAML aninhado (perfis do Quarto, frontmatter).
//!
//! Localiza uma chave pelo caminho (`website.navbar.left`), lê e troca valores
//! de uma linha e manipula listas de itens `- chave: valor`, sem tocar em
//! nenhuma outra linha do arquivo.

use crate::frontmatter as fm;

fn indent(l: &str) -> usize {
    l.len() - l.trim_start().len()
}

fn skip(l: &str) -> bool {
    let t = l.trim();
    t.is_empty() || t.starts_with('#')
}

/// Linha da chave no caminho dado (só mapeamentos; não entra em listas).
pub fn find(lines: &[String], path: &[&str]) -> Option<usize> {
    let mut stack: Vec<(usize, String)> = vec![];
    for (i, l) in lines.iter().enumerate() {
        if skip(l) {
            continue;
        }
        let ind = indent(l);
        while stack.last().is_some_and(|(n, _)| *n >= ind) {
            stack.pop();
        }
        let content = l.trim_start();
        if content.starts_with("- ") {
            continue;
        }
        let Some((key, _)) = fm::top_key(content) else { continue };
        if stack.len() + 1 == path.len() && stack.iter().zip(path).all(|((_, k), p)| k == p) && key == path[stack.len()] {
            return Some(i);
        }
        stack.push((ind, key.to_string()));
    }
    None
}

/// Fim (exclusivo) do bloco que pertence à chave na linha `at`.
pub fn block_end(lines: &[String], at: usize) -> usize {
    let base = indent(&lines[at]);
    let mut end = at + 1;
    for (i, l) in lines.iter().enumerate().skip(at + 1) {
        if skip(l) {
            continue;
        }
        let ind = indent(l);
        if ind > base || (ind == base && l.trim_start().starts_with("- ")) {
            end = i + 1;
        } else {
            break;
        }
    }
    end
}

pub fn get(lines: &[String], path: &[&str]) -> Option<String> {
    let i = find(lines, path)?;
    let (_, raw) = fm::top_key(lines[i].trim_start())?;
    let raw = raw.trim();
    (!raw.is_empty()).then(|| fm::scalar(raw))
}

/// Troca o valor de uma chave que já existe (só se ele mudou).
pub fn set(lines: &mut [String], path: &[&str], value: &str) -> Result<(), String> {
    let i = find(lines, path).ok_or_else(|| format!("campo {} não encontrado", path.join(".")))?;
    if get(lines, path).as_deref() == Some(value) {
        return Ok(());
    }
    let ind = " ".repeat(indent(&lines[i]));
    lines[i] = format!("{ind}{}: {}", path.last().unwrap(), fm::quote(value));
    Ok(())
}

/// Itens de uma lista em bloco: (início, fim, itens com suas linhas brutas).
pub fn items(lines: &[String], path: &[&str]) -> Option<(usize, usize, Vec<Vec<String>>)> {
    let at = find(lines, path)?;
    let end = block_end(lines, at);
    let mut out: Vec<Vec<String>> = vec![];
    let Some(dash) = lines[at + 1..end].iter().filter(|l| !skip(l)).map(|l| indent(l)).min() else {
        return Some((at + 1, end, out)); // lista vazia
    };
    for l in &lines[at + 1..end] {
        if indent(l) == dash && l.trim_start().starts_with("- ") {
            out.push(vec![l.clone()]);
        } else if let Some(last) = out.last_mut() {
            last.push(l.clone());
        }
    }
    Some((at + 1, end, out))
}

/// Indentação do traço dos itens (para criar itens novos no mesmo padrão).
pub fn dash_indent(items: &[Vec<String>], fallback: usize) -> String {
    " ".repeat(items.first().map(|it| indent(&it[0])).unwrap_or(fallback))
}

/// Valor de uma chave dentro de um item (`- chave:` ou `  chave:`).
pub fn item_get(item: &[String], key: &str) -> Option<String> {
    item.iter().find_map(|l| {
        let c = l.trim_start().trim_start_matches("- ");
        fm::top_key(c).filter(|(k, _)| *k == key).map(|(_, v)| fm::scalar(v))
    })
}

/// Troca (ou acrescenta) uma chave num item, só se o valor mudou.
pub fn item_set(item: &mut Vec<String>, key: &str, value: &str) {
    if item_get(item, key).as_deref() == Some(value) {
        return;
    }
    let raw = fm::quote_if_needed(value);
    for l in item.iter_mut() {
        let ind = indent(l);
        let content = l.trim_start();
        let (prefix, c) = match content.strip_prefix("- ") {
            Some(c) => ("- ", c),
            None => ("", content),
        };
        if fm::top_key(c).is_some_and(|(k, _)| k == key) {
            *l = format!("{}{prefix}{key}: {raw}", " ".repeat(ind));
            return;
        }
    }
    let ind = item.first().map(|l| indent(l) + 2).unwrap_or(2);
    item.push(format!("{}{key}: {raw}", " ".repeat(ind)));
}

/// Item novo a partir de pares (chave, valor).
pub fn new_item(dash: &str, fields: &[(&str, &str)]) -> Vec<String> {
    fields
        .iter()
        .enumerate()
        .map(|(i, (k, v))| format!("{dash}{}{k}: {}", if i == 0 { "- " } else { "  " }, fm::quote_if_needed(v)))
        .collect()
}

/// Substitui as linhas dos itens de uma lista.
pub fn replace_items(lines: &mut Vec<String>, path: &[&str], new_items: &[Vec<String>]) -> Result<(), String> {
    let (start, end, _) = items(lines, path).ok_or_else(|| format!("lista {} não encontrada", path.join(".")))?;
    lines.splice(start..end, new_items.iter().flatten().cloned());
    Ok(())
}

pub fn file_lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

pub fn join_file(lines: &[String]) -> String {
    lines.join("\n") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROFILE: &str = "website:\n  navbar:\n    left:\n      - href: index.qmd\n        text: About\n      - href: cv.qmd\n        text: CV\n    right:\n      - icon: envelope\n  page-footer:\n    left: \"© 2026\"\n    right: \"Built\"\n\nformat:\n  html:\n    lang: en\n";

    #[test]
    fn paths_and_items() {
        let mut l = file_lines(PROFILE);
        assert_eq!(get(&l, &["website", "page-footer", "right"]).as_deref(), Some("Built"));
        assert_eq!(get(&l, &["format", "html", "lang"]).as_deref(), Some("en"));
        assert!(find(&l, &["website", "left"]).is_none());
        let (_, _, its) = items(&l, &["website", "navbar", "left"]).unwrap();
        assert_eq!(its.len(), 2);
        assert_eq!(item_get(&its[1], "text").as_deref(), Some("CV"));
        let mut a = its[0].clone();
        item_set(&mut a, "text", "Sobre mim");
        let b = new_item(&dash_indent(&its, 6), &[("href", "x.qmd"), ("text", "X")]);
        replace_items(&mut l, &["website", "navbar", "left"], &[its[1].clone(), a, b]).unwrap();
        set(&mut l, &["website", "page-footer", "left"], "© 2027").unwrap();
        let out = join_file(&l);
        assert!(out.contains("    left:\n      - href: cv.qmd\n        text: CV\n      - href: index.qmd\n        text: Sobre mim\n      - href: x.qmd\n        text: X\n    right:"), "{out}");
        assert!(out.contains("    left: \"© 2027\""));
        // sem mudanças → idêntico
        let mut same = file_lines(PROFILE);
        let (_, _, its) = items(&same, &["website", "navbar", "left"]).unwrap();
        replace_items(&mut same, &["website", "navbar", "left"], &its).unwrap();
        set(&mut same, &["website", "page-footer", "right"], "Built").unwrap();
        assert_eq!(join_file(&same), PROFILE);
    }
}
