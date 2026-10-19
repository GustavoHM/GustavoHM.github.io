//! Testes de ponta a ponta numa cópia do site (nada toca o repositório real
//! nem a rede: o "GitHub" é um repositório local).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::channel;

use eframe::egui;

use crate::manifest::Manifest;
use crate::publish::{self, Job, Msg};
use crate::{languages, lists, menu, pages, papers, posts, settings};

fn sh(dir: &Path, args: &[&str]) {
    let ok = Command::new(args[0]).args(&args[1..]).current_dir(dir).output().unwrap();
    assert!(ok.status.success(), "{args:?}: {}", String::from_utf8_lossy(&ok.stderr));
}

/// Cópia do estado atual do site (incluindo o que ainda não foi commitado),
/// como repositório git com um remoto local.
fn copy_site(tag: &str) -> (PathBuf, PathBuf) {
    let real = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let tmp = std::env::temp_dir().join(format!("painel-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    let (remote, site) = (tmp.join("remote.git"), tmp.join("site"));
    fs::create_dir_all(&site).unwrap();
    Command::new("rsync")
        .args(["-a", "--exclude", ".git", "--exclude", "painel/target", "--exclude", "_site*", "--exclude", ".quarto"])
        .arg(format!("{}/", real.display()))
        .arg(&site)
        .status()
        .unwrap();
    sh(&tmp, &["git", "init", "-q", "--bare", "-b", "main", remote.to_str().unwrap()]);
    sh(&site, &["git", "init", "-q", "-b", "main"]);
    sh(&site, &["git", "remote", "add", "origin", remote.to_str().unwrap()]);
    sh(&site, &["git", "add", "-A"]);
    sh(&site, &["git", "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "base"]);
    sh(&site, &["git", "push", "-q", "-u", "origin", "main"]);
    // SAFETY: testes isolados; identidade git só para os commits de teste.
    unsafe {
        for (k, v) in [("GIT_AUTHOR_NAME", "t"), ("GIT_AUTHOR_EMAIL", "t@t"), ("GIT_COMMITTER_NAME", "t"), ("GIT_COMMITTER_EMAIL", "t@t")] {
            std::env::set_var(k, v);
        }
    }
    (tmp, site)
}

fn run_job(site: &Path, job: Job) -> Vec<String> {
    let (tx, rx) = channel();
    publish::publish(site.to_path_buf(), job, tx, egui::Context::default());
    let mut ok = false;
    let mut log = vec![];
    for m in rx.try_iter() {
        match m {
            Msg::Log(s) => log.push(s),
            Msg::Done { ok: o } => ok = o,
        }
    }
    assert!(ok, "envio falhou:\n{}", log.join("\n"));
    log
}

fn remote_subjects(tmp: &Path, n: usize) -> Vec<String> {
    let out = Command::new("git").args(["log", "--format=%s", &format!("-{n}"), "main"]).current_dir(tmp.join("remote.git")).output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines().map(str::to_string).collect()
}

/// Abrir e salvar sem editar não pode mudar nenhum arquivo do site.
#[test]
fn save_without_edits_changes_nothing() {
    let (tmp, site) = copy_site("rt");
    let m = Manifest::load(&site).unwrap();
    for k in m.page_keys() {
        pages::save(&site, &pages::load(&site, &m, &k).unwrap()).unwrap();
    }
    for k in m.list_keys() {
        lists::save(&site, &lists::load(&site, &m, &k).unwrap()).unwrap();
    }
    for p in papers::list(&site, &m) {
        papers::save(&site, &mut papers::load(&site, &m, &p.slug).unwrap()).unwrap();
    }
    menu::save(&site, &menu::load(&site, &m).unwrap()).unwrap();
    settings::save(&site, &settings::load(&site, &m).unwrap()).unwrap();
    for (lang, dir) in m.post_dirs() {
        for p in posts::list(&site, &dir) {
            posts::save(&site, &mut posts::load(&site, &lang, &dir, &p.slug).unwrap()).unwrap();
        }
    }
    let diff = Command::new("git").args(["status", "--porcelain"]).current_dir(&site).output().unwrap();
    let diff = String::from_utf8_lossy(&diff.stdout).to_string();
    let _ = fs::remove_dir_all(&tmp);
    assert!(diff.is_empty(), "salvar sem editar mudou arquivos:\n{diff}");
}

/// Edita postagem, página, lista e working paper e envia cada um.
#[test]
fn publish_end_to_end() {
    let (tmp, site) = copy_site("e2e");
    let m = Manifest::load(&site).unwrap();

    let mut post = posts::Post {
        lang: "pt".into(),
        dir: "pt/postagens".into(),
        title: "Teste de publicação: ação".into(),
        date: "2026-10-18".into(),
        categories: "teste".into(),
        draft: false,
        body: "Olá **mundo**.".into(),
        ..Default::default()
    };
    posts::save(&site, &mut post).unwrap();
    assert_eq!(publish::unsent_subdirs(&site, "pt/postagens"), vec![post.slug.clone()]);
    // um commit feito fora do painel aparece na confirmação e vai junto
    assert!(publish::unpushed_commits(&site).is_empty());
    fs::write(site.join("styles.css"), "/* mudança local */\n").unwrap();
    sh(&site, &["git", "commit", "-qam", "Mudança feita no terminal"]);
    let pending = publish::unpushed_commits(&site);
    assert_eq!(pending.len(), 1);
    assert!(pending[0].ends_with(" Mudança feita no terminal"), "{pending:?}");
    run_job(&site, Job {
        paths: vec![post.rel_dir()],
        renders: vec![("pt".into(), format!("{}/index.qmd", post.rel_dir()))],
        message: "Publica postagem".into(),
        url: None,
    });
    assert!(publish::unsent_subdirs(&site, "pt/postagens").is_empty());
    assert!(publish::unpushed_commits(&site).is_empty(), "o push leva os dois commits");

    let mut page = pages::load(&site, &m, "pesquisa").unwrap();
    let original = page.sides[0].body.clone();
    page.sides[0].body = original.replace(":::{#articles}", "");
    assert!(pages::save(&site, &page).unwrap_err().contains("#articles"));
    page.sides[0].body = original + "\nExtra line.\n";
    page.sides.iter_mut().find(|s| s.lang == "pt").unwrap().title = "Pesquisa acadêmica".into();
    pages::save(&site, &page).unwrap();
    run_job(&site, Job {
        paths: page.sides.iter().map(|s| s.rel.clone()).collect(),
        renders: page.sides.iter().map(|s| (s.lang.clone(), s.rel.clone())).collect(),
        message: "Atualiza página".into(),
        url: None,
    });

    let mut list = lists::load(&site, &m, "artigos").unwrap();
    let n = list.len();
    list.add("2026-10-01");
    for l in &mut list.langs {
        let title = if l.lang == "pt" { "Novo artigo".to_string() } else { format!("New article ({})", l.lang) };
        l.file.items[0].set("title", &title);
    }
    lists::save(&site, &list).unwrap();
    assert_eq!(lists::load(&site, &m, "artigos").unwrap().len(), n + 1);

    let mut wp = papers::new_paper(&m, "2026-10-18".into());
    wp.title = "A test paper".into();
    wp.authors = "Gustavo Hoffmann Moreira, Coauthor Name".into();
    let pt = wp.translations.iter().position(|t| t.lang == "pt").unwrap();
    wp.translations[pt].title = "Um paper de teste".into();
    papers::save(&site, &mut wp).unwrap();
    let back = papers::load(&site, &m, &wp.slug).unwrap();
    assert_eq!(back.translations[pt].title, "Um paper de teste");
    assert_eq!(back.authors, "Gustavo Hoffmann Moreira, Coauthor Name");
    let mut paths: Vec<String> = list.langs.iter().map(|l| l.rel.clone()).collect();
    paths.extend([wp.rel_qmd(), wp.rel_files()]);
    paths.extend(wp.translations.iter().map(|t| t.list.clone()));
    let log = run_job(&site, Job {
        paths,
        renders: vec![("en".into(), wp.rel_qmd()), ("en".into(), "research/index.qmd".into()), ("pt".into(), "pt/pesquisa/index.qmd".into())],
        message: "Atualiza pesquisa".into(),
        url: None,
    });
    assert!(log.iter().any(|l| l.contains("main -> main")), "{}", log.join("\n"));
    assert_eq!(remote_subjects(&tmp, 3), vec!["Atualiza pesquisa", "Atualiza página", "Publica postagem"]);
    let _ = fs::remove_dir_all(&tmp);
}

/// Cria o coreano a partir do português, edita, envia e confere o site gerado.
#[test]
fn add_language_and_publish() {
    let (tmp, site) = copy_site("ko");
    let m = Manifest::load(&site).unwrap();
    let before = m.langs.len();
    let labels = languages::nav_labels(&site, &m, "pt", "ko");
    assert_eq!(labels.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>(), vec!["소개", "연구", "글", "강의", "이력서"]);
    let (changed, warnings) = languages::add(&site, &m, "ko", "한국어", "pt", &labels).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    for l in &m.langs {
        assert!(changed.contains(&languages::profile_file(&l.codigo)), "{changed:?}");
    }
    assert!(languages::add(&site, &Manifest::load(&site).unwrap(), "ko", "x", "pt", &[]).is_err());

    let m = Manifest::load(&site).unwrap();
    assert_eq!(m.langs.len(), before + 1);
    let ko = m.lang("ko").unwrap();
    assert_eq!(ko.paginas["sobre"], "ko/index.qmd");
    assert_eq!(ko.postagens.as_deref(), Some("ko/postagens"));
    for rel in ko.paginas.values().chain(ko.listas.values()) {
        assert!(site.join(rel).is_file(), "{rel}");
    }
    assert!(!site.join("ko/postagens/2026-08-23-ia-ciencia-workflow").exists());
    let en = fs::read_to_string(site.join("_quarto-en.yml")).unwrap();
    assert!(en.contains("- text: \"한국어\"\n        href: /ko/"), "{en}");
    let ko_profile = fs::read_to_string(site.join("_quarto-ko.yml")).unwrap();
    assert!(ko_profile.contains("output-dir: _site-ko") && ko_profile.contains("lang: ko-KR"), "{ko_profile}");
    assert!(ko_profile.contains("href: /pt/") && ko_profile.contains("href: /\n") && !ko_profile.contains("href: /ko/"), "{ko_profile}");

    // páginas, listas e papers ganham a língua nova
    let mut page = pages::load(&site, &m, "sobre").unwrap();
    assert_eq!(page.sides.last().unwrap().lang, "ko");
    page.sides.last_mut().unwrap().body = "응용경제학 박사과정에 재학 중입니다.".into();
    pages::save(&site, &page).unwrap();
    assert_eq!(lists::load(&site, &m, "artigos").unwrap().langs.len(), before + 1);
    let mut wp = papers::load(&site, &m, "dg-tariff-incidence-brazil").unwrap();
    wp.translations.iter_mut().find(|t| t.lang == "ko").unwrap().title = "브라질 분산형 발전에 대한 최적 요금 부담".into();
    papers::save(&site, &mut wp).unwrap();
    let mut post = posts::Post { lang: "ko".into(), dir: "ko/postagens".into(), title: "첫 번째 글".into(), date: "2026-10-18".into(), draft: false, ..Default::default() };
    posts::save(&site, &mut post).unwrap();
    assert!(post.slug.starts_with("2026-10-18-"), "{}", post.slug);

    run_job(&site, Job {
        paths: languages::setup_paths(&m, "ko"),
        renders: vec![("ko".into(), String::new())],
        message: "Adiciona a versão em 한국어".into(),
        url: None,
    });
    // tudo o que a língua nova tocou entrou no commit
    let status = Command::new("git").args(["status", "--porcelain"]).current_dir(&site).output().unwrap();
    assert!(status.stdout.is_empty(), "{}", String::from_utf8_lossy(&status.stdout));

    if publish::quarto_bin().is_some() {
        let out = Command::new("bash").arg("build.sh").current_dir(&site).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let html = fs::read_to_string(site.join("_site/ko/index.html")).unwrap();
        for needle in ["lang=\"ko-KR\"", "소개", "응용경제학"] {
            assert!(html.contains(needle), "falta {needle} em _site/ko/index.html");
        }
        for other in m.langs.iter().filter(|l| l.codigo != "ko") {
            let index = if other.codigo == m.root().codigo { "_site/index.html".to_string() } else { format!("_site/{}/index.html", other.codigo) };
            assert!(fs::read_to_string(site.join(&index)).unwrap().contains("한국어"), "{index} sem link para o coreano");
        }
        assert!(fs::read_to_string(site.join("_site/ko/pesquisa/index.html")).unwrap().contains("브라질 분산형"));
    }
    let _ = fs::remove_dir_all(&tmp);
}

/// Página nova, menu, configurações e remoção de página, até o HTML final.
#[test]
fn site_admin_end_to_end() {
    let (tmp, site) = copy_site("admin");
    let m = Manifest::load(&site).unwrap();

    // página nova em todas as línguas, no fim do menu
    let titles: Vec<String> = m.langs.iter().map(|l| if l.codigo == "pt" { "Palestras".into() } else { format!("Talks {}", l.codigo) }).collect();
    pages::create(&site, &m, "palestras", &titles, true).unwrap();
    assert!(pages::create(&site, &m, "palestras", &titles, true).is_err());
    let m = Manifest::load(&site).unwrap();
    assert_eq!(m.lang("pt").unwrap().paginas["palestras"], "pt/palestras.qmd");
    let mut page = pages::load(&site, &m, "palestras").unwrap();
    page.sides.iter_mut().find(|s| s.lang == "pt").unwrap().body = "Lista de palestras.".into();
    pages::save(&site, &page).unwrap();

    // menu: "Talks" sobe para o segundo lugar, CV some, rótulo novo em PT
    let mut mn = menu::load(&site, &m).unwrap();
    let pos = mn.rows.iter().position(|r| r.key == "palestras").unwrap();
    for _ in 0..pos - 1 {
        mn.move_row(mn.rows.iter().position(|r| r.key == "palestras").unwrap(), true);
    }
    mn.rows.iter_mut().find(|r| r.key == "cv").unwrap().visible = false;
    let pt_col = mn.langs.iter().position(|c| c == "pt").unwrap();
    mn.rows.iter_mut().find(|r| r.key == "ensino").unwrap().cells[pt_col].as_mut().unwrap().label = "Aulas".into();
    menu::save(&site, &mn).unwrap();
    let pt = fs::read_to_string(site.join("_quarto-pt.yml")).unwrap();
    assert!(pt.contains("      - href: pt/index.qmd\n        text: Sobre\n      - href: pt/palestras.qmd\n        text: Palestras\n"), "{pt}");
    assert!(pt.contains("text: Aulas") && !pt.contains("href: pt/cv.qmd"), "{pt}");
    // escondido continua disponível para voltar
    let mn = menu::load(&site, &m).unwrap();
    assert!(!mn.rows.iter().find(|r| r.key == "cv").unwrap().visible);

    // configurações
    let mut st = settings::load(&site, &m).unwrap();
    st.title = "Gustavo H. Moreira".into();
    st.footers.iter_mut().find(|f| f.lang == "pt").unwrap().right = Some("Feito com Quarto e com o painel".into());
    fs::write(site.join("foto.png"), include_bytes!("../icone/icone-1024.png")).unwrap();
    st.photo = "foto.png".into();
    st.add_link();
    let last = st.links.len() - 1;
    st.links[last].icon = "mastodon".into();
    st.links[last].href = "https://mastodon.social/@painel".into();
    for t in &mut st.links[last].texts {
        t.1 = "Mastodon".into();
    }
    st.move_link(last, true);
    settings::save(&site, &st).unwrap();
    let about_pt = fs::read_to_string(site.join("pt/index.qmd")).unwrap();
    assert!(about_pt.contains("image: ../foto.png") && about_pt.contains("- icon: mastodon\n      text: Mastodon\n      href: https://mastodon.social/@painel\n    - icon: envelope"), "{about_pt}");
    assert_eq!(settings::load(&site, &m).unwrap(), {
        let mut s2 = st.clone();
        // as linhas brutas mudam ao salvar; compara só os valores
        for l in &mut s2.links {
            for t in &mut l.texts {
                t.0 = None;
            }
        }
        let mut loaded = settings::load(&site, &m).unwrap();
        for l in &mut loaded.links {
            for t in &mut l.texts {
                t.0 = None;
            }
        }
        assert_eq!(loaded, s2);
        settings::load(&site, &m).unwrap()
    });

    let all: Vec<(String, String)> = m.langs.iter().map(|l| (l.codigo.clone(), String::new())).collect();
    let mut paths = st.paths();
    paths.extend(page.sides.iter().map(|s| s.rel.clone()));
    paths.push(crate::manifest::FILE.into());
    run_job(&site, Job { paths, renders: all.clone(), message: "Site".into(), url: None });
    let status = Command::new("git").args(["status", "--porcelain"]).current_dir(&site).output().unwrap();
    assert!(status.stdout.is_empty(), "{}", String::from_utf8_lossy(&status.stdout));

    if publish::quarto_bin().is_some() {
        let out = Command::new("bash").arg("build.sh").current_dir(&site).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        let pt_html = fs::read_to_string(site.join("_site/pt/index.html")).unwrap();
        for needle in ["Palestras", "Aulas", "Feito com Quarto e com o painel", "mastodon.social/@painel", "../foto.png"] {
            assert!(pt_html.contains(needle), "falta {needle} em pt/index.html");
        }
        assert!(!pt_html.contains("pt/cv.html\" class=\"nav-link"), "CV deveria ter saído do menu");
        assert!(site.join("_site/pt/palestras.html").is_file() && site.join("_site/palestras.html").is_file());
    }

    // remoção: arquivos, manifesto e menu
    let m = Manifest::load(&site).unwrap();
    assert!(pages::remove(&site, &m, "sobre").is_err());
    let changed = pages::remove(&site, &m, "palestras").unwrap();
    assert!(!site.join("pt/palestras.qmd").exists() && !site.join("palestras.qmd").exists());
    assert!(Manifest::load(&site).unwrap().lang("pt").unwrap().paginas.get("palestras").is_none());
    assert!(!fs::read_to_string(site.join("_quarto-en.yml")).unwrap().contains("palestras"));
    run_job(&site, Job { paths: changed, renders: all, message: "Remove página".into(), url: None });
    let status = Command::new("git").args(["status", "--porcelain"]).current_dir(&site).output().unwrap();
    assert!(status.stdout.is_empty(), "{}", String::from_utf8_lossy(&status.stdout));
    assert_eq!(remote_subjects(&tmp, 2), vec!["Remove página", "Site"]);
    let _ = fs::remove_dir_all(&tmp);
}

/// Um site Quarto comum, de uma língua só e sem perfis: prepara, cria página,
/// mexe no menu e nas configurações e envia.
#[test]
fn plain_single_language_site() {
    let tmp = std::env::temp_dir().join(format!("painel-plain-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    let (remote, site) = (tmp.join("remote.git"), tmp.join("blog"));
    fs::create_dir_all(site.join("posts/primeiro")).unwrap();
    fs::write(site.join("_quarto.yml"), "project:\n  type: website\n\nwebsite:\n  title: \"Meu blog\"\n  description: \"Notas\"\n  navbar:\n    left:\n      - href: index.qmd\n        text: Início\n      - href: about.qmd\n        text: Sobre\n  page-footer:\n    left: \"© 2026\"\n    right: \"Quarto\"\n\nformat:\n  html:\n    theme: cosmo\n    lang: pt-BR\n").unwrap();
    fs::write(site.join("index.qmd"), "---\ntitle: \"Início\"\nlisting:\n  contents: posts\n---\n\nBem-vindo.\n").unwrap();
    fs::write(site.join("about.qmd"), "---\ntitle: \"Sobre\"\n---\n\nQuem sou.\n").unwrap();
    fs::write(site.join("posts/primeiro/index.qmd"), "---\ntitle: \"Primeiro\"\ndate: 2026-01-01\n---\n\nOi.\n").unwrap();
    // o Quarto acrescenta a última linha sozinho se ela faltar
    fs::write(site.join(".gitignore"), "/_site/\n/.quarto/\n**/*.quarto_ipynb\n").unwrap();
    sh(&tmp, &["git", "init", "-q", "--bare", "-b", "main", remote.to_str().unwrap()]);
    sh(&site, &["git", "init", "-q", "-b", "main"]);
    sh(&site, &["git", "remote", "add", "origin", remote.to_str().unwrap()]);
    sh(&site, &["git", "add", "-A"]);
    sh(&site, &["git", "-c", "user.name=t", "-c", "user.email=t@t", "commit", "-qm", "base"]);
    sh(&site, &["git", "push", "-q", "-u", "origin", "main"]);

    let text = crate::manifest::bootstrap(&site).unwrap();
    fs::write(site.join(crate::manifest::FILE), &text).unwrap();
    let m = Manifest::load(&site).unwrap();
    assert_eq!(m.langs.len(), 1);
    assert_eq!(m.root().codigo, "pt");
    assert_eq!(m.root().paginas["sobre"], "index.qmd");
    assert_eq!(m.root().paginas["about"], "about.qmd");

    // abrir e salvar sem mudar nada
    menu::save(&site, &menu::load(&site, &m).unwrap()).unwrap();
    settings::save(&site, &settings::load(&site, &m).unwrap()).unwrap();
    let st = Command::new("git").args(["status", "--porcelain"]).current_dir(&site).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&st.stdout), "?? _idiomas.toml\n");

    pages::create(&site, &m, "contato", &["Contato".into()], true).unwrap();
    let m = Manifest::load(&site).unwrap();
    let mut st = settings::load(&site, &m).unwrap();
    assert_eq!(st.footers[0].file, "_quarto.yml");
    st.title = "Meu blog novo".into();
    st.footers[0].right = Some("Feito com Quarto".into());
    settings::save(&site, &st).unwrap();
    let cfg = fs::read_to_string(site.join("_quarto.yml")).unwrap();
    assert!(cfg.contains("title: \"Meu blog novo\"") && cfg.contains("right: \"Feito com Quarto\"") && cfg.contains("href: contato.qmd"), "{cfg}");

    let mut paths = st.paths();
    paths.extend(["contato.qmd".to_string(), crate::manifest::FILE.to_string()]);
    run_job(&site, Job { paths, renders: vec![("pt".into(), String::new())], message: "Blog".into(), url: None });
    let st = Command::new("git").args(["status", "--porcelain"]).current_dir(&site).output().unwrap();
    assert!(st.stdout.is_empty(), "{}", String::from_utf8_lossy(&st.stdout));
    if publish::quarto_bin().is_some() {
        let html = fs::read_to_string(site.join("_site/index.html")).unwrap();
        assert!(html.contains("Contato") && html.contains("Feito com Quarto"));
    }
    let _ = fs::remove_dir_all(&tmp);
}
