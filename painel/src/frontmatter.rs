//! YAML mínimo para o frontmatter dos .qmd e para as listas de pesquisa.
//!
//! Não é um parser YAML completo: lê só o que o site usa (escalares, listas
//! `[a, b]` e listas em bloco `- item`) e, ao gravar, reescreve apenas os
//! campos que o painel edita, preservando todas as outras linhas.

pub fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn scalar(raw: &str) -> String {
    let raw = raw.trim();
    if let Some(inner) = raw.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => break,
                '\\' => match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('t') => out.push('\t'),
                    Some('u') => {
                        let hex: String = chars.by_ref().take(4).collect();
                        if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                            out.push(ch);
                        }
                    }
                    Some(other) => out.push(other),
                    None => {}
                },
                c => out.push(c),
            }
        }
        return out;
    }
    if raw.len() >= 2 && raw.starts_with('\'') && raw.ends_with('\'') {
        return raw[1..raw.len() - 1].replace("''", "'");
    }
    raw.to_string()
}

pub fn flow_list(raw: &str) -> Vec<String> {
    let raw = raw.trim();
    let Some(inner) = raw.strip_prefix('[').and_then(|r| r.strip_suffix(']')) else {
        return if raw.is_empty() { vec![] } else { vec![scalar(raw)] };
    };
    let (mut items, mut buf, mut quote) = (Vec::new(), String::new(), None::<char>);
    for c in inner.chars() {
        match quote {
            Some(q) => {
                buf.push(c);
                if c == q {
                    quote = None;
                }
            }
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                buf.push(c);
            }
            None if c == ',' => items.push(std::mem::take(&mut buf)),
            None => buf.push(c),
        }
    }
    items.push(buf);
    items.iter().map(|i| scalar(i)).filter(|s| !s.is_empty()).collect()
}

/// Valor de uma data AAAA-MM-DD pode ir sem aspas; o resto vai entre aspas.
pub fn value(s: &str) -> String {
    if is_date(s) { s.to_string() } else { quote(s) }
}

pub fn is_date(s: &str) -> bool {
    let d = s.as_bytes();
    d.len() == 10 && d.iter().enumerate().all(|(i, c)| if i == 4 || i == 7 { *c == b'-' } else { c.is_ascii_digit() })
}

/// Devolve (chave, resto) se a linha for `chave: …` no nível de cima.
pub fn top_key(line: &str) -> Option<(&str, &str)> {
    let first = line.chars().next()?;
    if !(first.is_alphanumeric() || first == '_') {
        return None;
    }
    let (key, rest) = line.split_once(':')?;
    if key.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        Some((key, rest))
    } else {
        None
    }
}

/// Separa o frontmatter (linhas entre `---`) do corpo.
pub fn split(text: &str) -> (Vec<String>, String) {
    if let Some(rest) = text.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---") {
            let lines = rest[..end].lines().map(str::to_string).collect();
            let after = &rest[end + 4..];
            let body = after.split_once('\n').map(|(_, b)| b).unwrap_or("");
            return (lines, body.trim_start_matches('\n').to_string());
        }
    }
    (vec![], text.to_string())
}

fn is_continuation(line: &str) -> bool {
    line.starts_with(' ') || line.starts_with('\t') || line.starts_with('-')
}

pub fn get(lines: &[String], key: &str) -> Option<String> {
    lines.iter().find_map(|l| top_key(l).filter(|(k, _)| *k == key).map(|(_, v)| scalar(v)))
}

pub fn has(lines: &[String], key: &str) -> bool {
    lines.iter().any(|l| top_key(l).is_some_and(|(k, _)| k == key))
}

/// Lê `key: [a, b]`, `key: a` ou a lista em bloco `key:\n  - a\n  - b`.
pub fn get_list(lines: &[String], key: &str) -> Vec<String> {
    let Some(pos) = lines.iter().position(|l| top_key(l).is_some_and(|(k, _)| k == key)) else {
        return vec![];
    };
    let inline = top_key(&lines[pos]).unwrap().1.trim();
    if !inline.is_empty() {
        return flow_list(inline);
    }
    lines[pos + 1..]
        .iter()
        .take_while(|l| is_continuation(l) || l.trim().is_empty())
        .filter_map(|l| l.trim_start().strip_prefix("- "))
        .map(scalar)
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn block_list(items: &[String]) -> String {
    items.iter().map(|i| format!("\n  - {}", quote_if_needed(i))).collect()
}

/// Nomes simples ficam sem aspas (como no resto do site).
pub fn quote_if_needed(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars().all(|c| c.is_alphanumeric() || " .-'/_:@?=&%+~".contains(c))
        && !s.starts_with(['-', ' ', '\'', ':', '?', '@', '%', '&'])
        && !s.ends_with([' ', ':'])
        && !s.contains(": ")
        && !matches!(s, "true" | "false" | "yes" | "no" | "null" | "on" | "off");
    if plain { s.to_string() } else { quote(s) }
}

/// Reescreve os campos dados (`raw` é o texto que vem depois de `chave:`,
/// por exemplo ` "Título"` ou `\n  - a\n  - b`) e preserva todo o resto.
pub fn set_fields(old_lines: &[String], fields: &[(&str, String)]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    let mut skipping = false;
    for line in old_lines {
        if let Some((key, _)) = top_key(line) {
            skipping = false;
            if let Some((k, raw)) = fields.iter().find(|(k, _)| *k == key) {
                seen.push(k);
                out.extend(format!("{k}:{raw}").lines().map(str::to_string));
                skipping = true; // descarta as continuações do valor antigo
                continue;
            }
        } else if skipping && is_continuation(line) {
            continue;
        }
        out.push(line.clone());
    }
    for (k, raw) in fields {
        if !seen.contains(k) {
            out.extend(format!("{k}:{raw}").lines().map(str::to_string));
        }
    }
    out
}

pub fn join(lines: &[String], body: &str) -> String {
    format!("---\n{}\n---\n\n{}\n", lines.join("\n").trim_matches('\n'), body.trim_end())
}

pub fn slugify(title: &str) -> String {
    let mut s = String::new();
    for c in title.to_lowercase().chars() {
        let c = match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            c => c,
        };
        s.push(if c.is_ascii_alphanumeric() { c } else { '-' });
    }
    let words: Vec<&str> = s.split('-').filter(|w| !w.is_empty()).take(8).collect();
    if words.is_empty() { "item".into() } else { words.join("-") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_lists_roundtrip() {
        let lines: Vec<String> = "title: \"X\"\nauthor:\n  - Gustavo Hoffmann Moreira\n  - Daniel de Abreu Pereira Uhr\ndate: 2026-01-01"
            .lines()
            .map(String::from)
            .collect();
        assert_eq!(get_list(&lines, "author"), vec!["Gustavo Hoffmann Moreira", "Daniel de Abreu Pereira Uhr"]);
        let out = set_fields(&lines, &[("author", block_list(&["A B".into()])), ("title", " \"Y\"".into())]);
        assert_eq!(out, vec!["title: \"Y\"", "author:", "  - A B", "date: 2026-01-01"]);
        assert_eq!(quote_if_needed("pt/palestras.qmd"), "pt/palestras.qmd");
        assert_eq!(quote_if_needed("https://github.com/x"), "https://github.com/x");
        assert_eq!(quote_if_needed("a: b"), "\"a: b\"");
        assert_eq!(quote_if_needed("true"), "\"true\"");
    }
}
