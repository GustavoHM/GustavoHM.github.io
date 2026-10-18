//! Painel do Site — app nativo para macOS para editar o site Quarto:
//! postagens, páginas (línguas lado a lado), listas da página de Pesquisa e
//! working papers, em todas as línguas do _idiomas.toml. Salva no computador
//! e publica com commit + push.

mod frontmatter;
mod languages;
mod lists;
mod manifest;
mod menu;
mod pages;
mod papers;
mod posts;
mod publish;
mod settings;
#[cfg(test)]
mod tests;
mod yamlpath;

use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use lists::ListDoc;
use manifest::Manifest;
use menu::MenuDoc;
use settings::Settings;
use pages::PageDoc;
use papers::{Paper, PaperSummary};
use posts::{Post, PostSummary};
use publish::{Job, Msg};

const AUTOSAVE_AFTER: Duration = Duration::from_secs(2);
const ORANGE: Color32 = Color32::from_rgb(200, 140, 30);
const GREEN: Color32 = Color32::from_rgb(40, 160, 80);
const RED: Color32 = Color32::from_rgb(200, 60, 60);
const BLUE: Color32 = Color32::from_rgb(37, 99, 235);

fn config_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_default();
    Path::new(&home).join("Library/Application Support/Painel do Site")
}

fn is_site(p: &Path) -> bool {
    p.join("_quarto.yml").is_file()
}

/// Sites conhecidos, o atual primeiro (sites.txt, um caminho por linha).
/// Na primeira vez: $PAINEL_SITE, o site.txt antigo ou ~/site-pessoal.
fn load_sites() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let mut v: Vec<PathBuf> = std::env::var("PAINEL_SITE").ok().map(PathBuf::from).into_iter().collect();
    match std::fs::read_to_string(config_dir().join("sites.txt")) {
        Ok(text) => v.extend(text.lines().filter(|l| !l.trim().is_empty()).map(|l| PathBuf::from(l.trim()))),
        Err(_) => {
            v.extend(std::fs::read_to_string(config_dir().join("site.txt")).ok().map(|s| PathBuf::from(s.trim())));
            v.push(Path::new(&home).join("site-pessoal"));
        }
    }
    let mut out: Vec<PathBuf> = vec![];
    for p in v {
        if is_site(&p) && !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

fn save_sites(sites: &[PathBuf]) {
    let _ = std::fs::create_dir_all(config_dir());
    let text: String = sites.iter().map(|p| format!("{}\n", p.display())).collect();
    let _ = std::fs::write(config_dir().join("sites.txt"), text);
}

fn site_label(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string())
}

/// Endereço publicado de um .qmd (index.qmd → pasta, x.qmd → x.html).
fn html_of(rel_qmd: &str) -> String {
    let html = rel_qmd.trim_end_matches(".qmd").to_string() + ".html";
    html.strip_suffix("index.html").map(str::to_string).unwrap_or(html)
}

fn tracked(site: &Path, rel: &str) -> bool {
    std::process::Command::new("git")
        .args(["ls-files", "--error-unmatch", rel])
        .current_dir(site)
        .output()
        .is_ok_and(|o| o.status.success())
}

// --- documento aberto -------------------------------------------------------

#[derive(Clone, PartialEq)]
enum Doc {
    Post(Post),
    Page(PageDoc),
    List(ListDoc),
    Paper(Paper),
    Menu(MenuDoc),
    Settings(Settings),
}

#[derive(Clone, PartialEq, Debug)]
enum Target {
    NewPost(String),
    Post(String, String),
    Page(String),
    List(String),
    NewPaper,
    Paper(String),
    Menu,
    Settings,
}

impl Doc {
    fn target(&self) -> Target {
        match self {
            Doc::Post(p) if p.slug.is_empty() => Target::NewPost(p.lang.clone()),
            Doc::Post(p) => Target::Post(p.lang.clone(), p.slug.clone()),
            Doc::Page(p) => Target::Page(p.key.clone()),
            Doc::List(l) => Target::List(l.key.clone()),
            Doc::Paper(p) if p.slug.is_empty() => Target::NewPaper,
            Doc::Paper(p) => Target::Paper(p.slug.clone()),
            Doc::Menu(_) => Target::Menu,
            Doc::Settings(_) => Target::Settings,
        }
    }

    fn load(site: &Path, m: &Manifest, t: &Target) -> Result<Doc, String> {
        let post_dir = |lang: &str| m.lang(lang).and_then(|l| l.postagens.clone()).ok_or_else(|| format!("A língua {lang} não tem pasta de postagens."));
        Ok(match t {
            Target::NewPost(lang) => Doc::Post(Post { lang: lang.clone(), dir: post_dir(lang)?, date: posts::today(), draft: true, ..Default::default() }),
            Target::Post(lang, s) => Doc::Post(posts::load(site, lang, &post_dir(lang)?, s)?),
            Target::Page(k) => Doc::Page(pages::load(site, m, k)?),
            Target::List(k) => Doc::List(lists::load(site, m, k)?),
            Target::NewPaper => Doc::Paper(papers::new_paper(m, posts::today())),
            Target::Paper(s) => Doc::Paper(papers::load(site, m, s)?),
            Target::Menu => Doc::Menu(menu::load(site, m)?),
            Target::Settings => Doc::Settings(settings::load(site, m)?),
        })
    }

    fn is_new(&self) -> bool {
        matches!(self.target(), Target::NewPost(_) | Target::NewPaper)
    }

    /// Documento novo que ainda não tem nada escrito (pode ser descartado).
    fn is_blank(&self) -> bool {
        match self {
            Doc::Post(p) => p.slug.is_empty() && p.title.trim().is_empty() && p.body.trim().is_empty(),
            Doc::Paper(p) => p.slug.is_empty() && p.title.trim().is_empty(),
            _ => false,
        }
    }

    fn save(&mut self, site: &Path) -> Result<(), String> {
        match self {
            Doc::Post(p) => posts::save(site, p),
            Doc::Page(p) => pages::save(site, p),
            Doc::List(l) => lists::save(site, l),
            Doc::Paper(p) => papers::save(site, p),
            Doc::Menu(d) => menu::save(site, d),
            Doc::Settings(d) => settings::save(site, d),
        }
    }

    fn validate(&self) -> Result<(), String> {
        match self {
            Doc::Post(p) => posts::validate(p),
            Doc::List(l) => l.validate(),
            Doc::Paper(p) => papers::validate(p),
            Doc::Menu(d) => d.validate(),
            Doc::Settings(d) => d.validate(),
            Doc::Page(_) => Ok(()),
        }
    }

    fn title(&self) -> String {
        match self {
            Doc::Post(p) => p.title.clone(),
            Doc::Page(p) => manifest::page_name(&p.key),
            Doc::List(l) => manifest::list_name(&l.key),
            Doc::Paper(p) => p.title.clone(),
            Doc::Menu(_) => "Menu do site".into(),
            Doc::Settings(_) => "Configurações do site".into(),
        }
    }

    /// Arquivos que este documento grava (para commit e “não enviado”).
    fn paths(&self) -> Vec<String> {
        match self {
            Doc::Post(p) => vec![p.rel_dir()],
            Doc::Page(p) => p.sides.iter().map(|s| s.rel.clone()).collect(),
            Doc::List(l) => l.langs.iter().map(|x| x.rel.clone()).collect(),
            Doc::Paper(p) => [p.rel_qmd(), p.rel_files()].into_iter().chain(p.translations.iter().map(|t| t.list.clone())).collect(),
            Doc::Menu(d) => d.paths(),
            Doc::Settings(d) => d.paths(),
        }
    }

    fn url(&self, site: &Path) -> Option<String> {
        match self {
            Doc::Post(p) if !p.slug.is_empty() => publish::page_url(site, &format!("{}/", p.rel_dir())),
            Doc::Page(p) => p.sides.first().and_then(|s| publish::page_url(site, &html_of(&s.rel))),
            Doc::List(l) => l.langs.first().and_then(|x| publish::page_url(site, &html_of(&lists::page_of(&x.rel)))),
            Doc::Paper(p) if !p.slug.is_empty() => publish::page_url(site, &html_of(&p.rel_qmd())),
            Doc::Menu(_) | Doc::Settings(_) => publish::page_url(site, ""),
            _ => None,
        }
    }

    fn job(&self, site: &Path, m: &Manifest, first_time: bool) -> Job {
        let verb = if first_time { "Publica" } else { "Atualiza" };
        let root = m.root().codigo.clone();
        let (renders, message): (Vec<(String, String)>, String) = match self {
            Doc::Post(p) => (vec![(p.lang.clone(), format!("{}/index.qmd", p.rel_dir()))], format!("{verb} postagem: {}", p.title)),
            Doc::Page(p) => (p.sides.iter().map(|s| (s.lang.clone(), s.rel.clone())).collect(), format!("Atualiza página: {}", manifest::page_name(&p.key))),
            Doc::List(l) => (
                l.langs.iter().map(|x| (x.lang.clone(), lists::page_of(&x.rel))).collect(),
                format!("Atualiza lista de pesquisa: {}", manifest::list_name(&l.key)),
            ),
            Doc::Paper(p) => {
                let mut r = vec![(root.clone(), p.rel_qmd()), (root.clone(), lists::page_of(&p.dir))];
                r.extend(p.translations.iter().map(|t| (t.lang.clone(), lists::page_of(&t.list))));
                (r, format!("{verb} working paper: {}", p.title))
            }
            Doc::Menu(_) | Doc::Settings(_) => (
                m.langs.iter().map(|l| (l.codigo.clone(), String::new())).collect(),
                format!("Atualiza {}", if matches!(self, Doc::Menu(_)) { "o menu do site" } else { "as configurações do site" }),
            ),
        };
        let mut paths = self.paths();
        match self {
            // página nova: o manifesto e o menu também mudaram
            Doc::Page(p) if p.sides.iter().any(|s| !tracked(site, &s.rel)) => {
                paths.push(manifest::FILE.into());
                paths.extend(m.langs.iter().map(|l| languages::config_file(site, &l.codigo)));
            }
            // o menu leva junto páginas removidas que ainda não foram enviadas
            Doc::Menu(_) => {
                paths.push(manifest::FILE.into());
                paths.extend(publish::deleted_files(site));
            }
            _ => {}
        }
        Job { paths, renders, message, url: self.url(site) }
    }
}

// --- app --------------------------------------------------------------------

#[derive(PartialEq, Clone, Copy)]
enum Section {
    Posts,
    Pages,
    Research,
    Papers,
    Site,
}

#[derive(PartialEq, Clone, Copy)]
enum View {
    Edit,
    Split,
    Preview,
}

enum Ask {
    Publish,
    PublishLang(String),
    Discard(Target),
    DeleteItem(usize),
    RemovePage(String),
}

/// Formulário “Nova página”.
struct PageForm {
    slug: String,
    titles: Vec<String>,
    in_menu: bool,
    message: Option<String>,
}

/// Formulário da janela “Línguas”.
struct LangForm {
    code: String,
    name: String,
    source: String,
    labels: Vec<(String, String)>,
    labels_for: (String, String),
    message: Option<(String, bool)>,
}

struct App {
    sites: Vec<PathBuf>,
    site: Option<PathBuf>,
    manifest: Option<Manifest>,
    section: Section,
    post_lang: String,
    posts: Vec<PostSummary>,
    papers: Vec<PaperSummary>,
    page_unsent: Vec<(String, bool)>,
    list_unsent: Vec<(String, bool)>,
    branch: String,
    /// Documento aberto e a versão dele que está gravada em disco.
    doc: Option<Doc>,
    saved: Option<Doc>,
    last_edit: Option<Instant>,
    cursor: Option<usize>,
    view: View,
    page_preview: bool,
    /// Línguas mostradas lado a lado no editor de páginas.
    compare: [String; 2],
    list_sel: Option<usize>,
    status: Option<(String, bool)>,
    log: Vec<String>,
    show_log: bool,
    publishing: Option<Receiver<Msg>>,
    ask: Option<Ask>,
    lang_form: Option<LangForm>,
    page_form: Option<PageForm>,
    md_cache: CommonMarkCache,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        cc.egui_ctx.all_styles_mut(|s| {
            for (style, size) in [
                (egui::TextStyle::Body, 15.0),
                (egui::TextStyle::Button, 14.0),
                (egui::TextStyle::Monospace, 14.0),
                (egui::TextStyle::Heading, 22.0),
                (egui::TextStyle::Small, 11.5),
            ] {
                if let Some(f) = s.text_styles.get_mut(&style) {
                    f.size = size;
                }
            }
            s.url_in_tooltip = true;
        });
        install_cjk_font(&cc.egui_ctx);
        let sites = load_sites();
        let mut app = App {
            site: sites.first().cloned(),
            sites,
            manifest: None,
            section: Section::Posts,
            post_lang: String::new(),
            posts: vec![],
            papers: vec![],
            page_unsent: vec![],
            list_unsent: vec![],
            branch: String::new(),
            doc: None,
            saved: None,
            last_edit: None,
            cursor: None,
            view: View::Split,
            page_preview: false,
            compare: [String::new(), String::new()],
            list_sel: None,
            status: None,
            log: vec![],
            show_log: false,
            publishing: None,
            ask: None,
            lang_form: None,
            page_form: None,
            md_cache: CommonMarkCache::default(),
        };
        app.refresh();
        app
    }

    fn refresh(&mut self) {
        let Some(site) = self.site.clone() else { return };
        if !site.join(manifest::FILE).exists() {
            self.manifest = None; // site ainda não preparado: a tela própria explica
            return;
        }
        let m = match Manifest::load(&site) {
            Ok(m) => m,
            Err(e) => {
                self.manifest = None;
                return self.set_status(e, true);
            }
        };
        let dirs = m.post_dirs();
        if !dirs.iter().any(|(c, _)| *c == self.post_lang) {
            self.post_lang = dirs.first().map(|(c, _)| c.clone()).unwrap_or_default();
        }
        self.posts = dirs.iter().find(|(c, _)| *c == self.post_lang).map(|(_, d)| posts::list(&site, d)).unwrap_or_default();
        self.papers = papers::list(&site, &m);
        let unsent_of = |paths: Vec<String>| publish::unsent(&site, &paths);
        self.page_unsent =
            m.page_keys().into_iter().map(|k| (k.clone(), unsent_of(m.langs.iter().filter_map(|l| l.paginas.get(&k).cloned()).collect()))).collect();
        self.list_unsent =
            m.list_keys().into_iter().map(|k| (k.clone(), unsent_of(m.langs.iter().filter_map(|l| l.listas.get(&k).cloned()).collect()))).collect();
        let codes: Vec<String> = m.langs.iter().map(|l| l.codigo.clone()).collect();
        for (i, c) in self.compare.iter_mut().enumerate() {
            if !codes.contains(c) {
                *c = codes.get(i).or(codes.first()).cloned().unwrap_or_default();
            }
        }
        self.branch = publish::branch(&site);
        self.manifest = Some(m);
    }

    fn lang_name(&self, code: &str) -> String {
        self.manifest.as_ref().and_then(|m| m.lang(code)).map(|l| l.nome.clone()).unwrap_or_else(|| code.to_string())
    }

    fn dirty(&self) -> bool {
        self.doc != self.saved
    }

    fn doc_unsent(&self) -> bool {
        match (&self.site, &self.doc) {
            (Some(site), Some(doc)) if !doc.is_new() => publish::unsent(site, &doc.paths()),
            _ => false,
        }
    }

    fn set_status(&mut self, msg: impl Into<String>, error: bool) {
        self.status = Some((msg.into(), error));
    }

    fn save(&mut self, quiet: bool) -> bool {
        let (Some(site), Some(doc)) = (self.site.clone(), self.doc.as_mut()) else { return false };
        match doc.save(&site) {
            Ok(()) => {
                self.saved = self.doc.clone();
                self.last_edit = None;
                if !quiet {
                    self.set_status("Salvo no computador.", false);
                }
                self.refresh();
                true
            }
            Err(e) => {
                if !quiet {
                    self.set_status(e, true);
                }
                false
            }
        }
    }

    /// Abre outro documento. Salva o atual antes, se puder.
    fn open(&mut self, t: Target) {
        if self.dirty() && !self.save(true) && !self.doc.as_ref().is_some_and(Doc::is_blank) {
            self.ask = Some(Ask::Discard(t));
            return;
        }
        self.load(t);
    }

    fn load(&mut self, t: Target) {
        let (Some(site), Some(m)) = (self.site.clone(), self.manifest.clone()) else { return };
        match Doc::load(&site, &m, &t) {
            Ok(doc) => {
                self.saved = if doc.is_new() { None } else { Some(doc.clone()) };
                self.doc = Some(doc);
                self.cursor = None;
                self.last_edit = None;
                self.status = None;
                self.list_sel = None;
                self.md_cache = CommonMarkCache::default();
            }
            Err(e) => self.set_status(e, true),
        }
    }

    /// Copia um arquivo para a pasta do documento e insere o link no texto.
    fn attach(&mut self) {
        if (self.doc.as_ref().is_some_and(Doc::is_new) || self.dirty()) && !self.save(false) {
            return;
        }
        let site = self.site.clone().unwrap();
        let (dir, prefix, allowed, title): (PathBuf, String, &[&str], &str) = match self.doc.as_ref() {
            Some(Doc::Post(p)) => (site.join(p.rel_dir()), String::new(), &posts::IMAGE_EXT, "Escolha uma imagem para a postagem"),
            Some(Doc::Paper(p)) => (p.files_dir(&site), format!("files/{}/", p.slug), &["pdf", "png", "jpg", "jpeg", "zip"], "Escolha o PDF (ou outro arquivo) do paper"),
            _ => return,
        };
        let Some(file) = rfd::FileDialog::new().set_title(title).add_filter("Arquivos", allowed).pick_file() else { return };
        let _ = std::fs::create_dir_all(&dir);
        let name = match posts::copy_into(&dir, &file, "", allowed) {
            Ok(n) => n,
            Err(e) => return self.set_status(e, true),
        };
        let link = format!("{prefix}{name}");
        let at_cursor = self.cursor;
        let (body, snippet) = match self.doc.as_mut() {
            Some(Doc::Post(p)) => (&mut p.body, format!("\n![Legenda da imagem]({link})\n")),
            Some(Doc::Paper(p)) => (&mut p.body, format!("\n- [PDF]({link})\n")),
            _ => return,
        };
        let at = at_cursor.unwrap_or(body.chars().count());
        let byte = body.char_indices().nth(at).map(|(i, _)| i).unwrap_or(body.len());
        body.insert_str(byte, &snippet);
        self.last_edit = Some(Instant::now());
        self.set_status(format!("Arquivo copiado: {link}"), false);
    }

    fn run_job(&mut self, ctx: &egui::Context, title: String, job: Job) {
        let Some(site) = self.site.clone() else { return };
        let (tx, rx) = channel();
        self.publishing = Some(rx);
        self.log = vec![format!("Enviando “{title}”…")];
        self.show_log = true;
        self.status = None;
        let ctx = ctx.clone();
        std::thread::spawn(move || publish::publish(site, job, tx, ctx));
    }

    fn start_publish(&mut self, ctx: &egui::Context) {
        let (Some(site), Some(m)) = (self.site.clone(), self.manifest.clone()) else { return };
        let mut first_time = false;
        let mut was_draft = false;
        match self.doc.as_mut() {
            Some(Doc::Post(p)) => {
                was_draft = p.draft;
                first_time = p.slug.is_empty() || !tracked(&site, &format!("{}/index.qmd", p.rel_dir()));
                p.draft = false;
            }
            Some(Doc::Paper(p)) => first_time = p.slug.is_empty() || !tracked(&site, &p.rel_qmd()),
            _ => {}
        }
        if !self.save(false) {
            if let Some(Doc::Post(p)) = self.doc.as_mut() {
                p.draft = was_draft;
            }
            return;
        }
        let doc = self.doc.clone().unwrap();
        self.run_job(ctx, doc.title(), doc.job(&site, &m, first_time));
    }

    fn poll_publish(&mut self) {
        let Some(rx) = &self.publishing else { return };
        let mut done = None;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Msg::Log(s) => self.log.push(s),
                Msg::Done { ok } => done = Some(ok),
            }
        }
        if let Some(ok) = done {
            self.publishing = None;
            self.refresh();
            if ok {
                self.set_status("Enviado! O site atualiza em 1–2 minutos.", false);
            } else {
                self.set_status("O envio falhou — veja o registro abaixo.", true);
            }
        }
    }

    fn add_site(&mut self) {
        let Some(dir) = rfd::FileDialog::new().set_title("Escolha a pasta de um site Quarto (com _quarto.yml)").pick_folder() else {
            return;
        };
        if !is_site(&dir) {
            return self.set_status("Essa pasta não parece ser um site Quarto (falta _quarto.yml).", true);
        }
        self.switch_site(dir);
    }

    /// Troca de site, salvando antes o que estiver aberto.
    fn switch_site(&mut self, dir: PathBuf) {
        if self.dirty() {
            self.save(true);
        }
        self.sites.retain(|p| *p != dir);
        self.sites.insert(0, dir.clone());
        save_sites(&self.sites);
        self.site = Some(dir);
        self.doc = None;
        self.saved = None;
        self.status = None;
        self.lang_form = None;
        self.page_form = None;
        self.log.clear();
        self.md_cache = CommonMarkCache::default();
        self.refresh();
    }

    fn forget_site(&mut self) {
        let Some(cur) = self.site.clone() else { return };
        self.sites.retain(|p| *p != cur);
        save_sites(&self.sites);
        match self.sites.first().cloned() {
            Some(next) => self.switch_site(next),
            None => {
                self.site = None;
                self.manifest = None;
                self.doc = None;
                self.saved = None;
            }
        }
    }

    /// Cria um _idiomas.toml inicial para um site que ainda não tem.
    fn prepare_site(&mut self) {
        let Some(site) = self.site.clone() else { return };
        match manifest::bootstrap(&site).and_then(|text| std::fs::write(site.join(manifest::FILE), text).map_err(|e| e.to_string())) {
            Ok(()) => {
                self.status = None;
                self.refresh();
                self.set_status(format!("Criei {}. Confira as páginas encontradas; você pode ajustar o arquivo à mão.", manifest::FILE), false);
            }
            Err(e) => self.set_status(e, true),
        }
    }
}

/// Fonte do sistema com japonês/chinês/coreano como reserva, para que esses
/// textos apareçam no editor (as fontes padrão do egui não têm esses glifos).
fn install_cjk_font(ctx: &egui::Context) {
    let candidates = [
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
    ];
    let mut fonts = egui::FontDefinitions::default();
    for (i, path) in candidates.iter().enumerate() {
        let Ok(bytes) = std::fs::read(path) else { continue };
        let name = format!("cjk-{i}");
        fonts.font_data.insert(name.clone(), egui::FontData::from_owned(bytes).into());
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push(name.clone());
        }
    }
    ctx.set_fonts(fonts);
}

// --- peças de interface reutilizadas ----------------------------------------

/// Campo de texto ligado a um valor derivado; devolve o novo valor se mudou.
fn field(ui: &mut egui::Ui, id: impl Hash + std::fmt::Debug, value: String, rows: usize, hint: &str) -> Option<String> {
    let mut v = value.clone();
    let edit = if rows > 1 { egui::TextEdit::multiline(&mut v).desired_rows(rows) } else { egui::TextEdit::singleline(&mut v) };
    ui.add(edit.id_salt(id).hint_text(hint).desired_width(f32::INFINITY));
    (v != value).then_some(v)
}

fn md_editor(ui: &mut egui::Ui, id: &str, text: &mut String, cursor: &mut Option<usize>, hint: &str) {
    egui::ScrollArea::vertical().id_salt(id).auto_shrink(false).show(ui, |ui| {
        let out = egui::TextEdit::multiline(text)
            .id_salt(id)
            .font(egui::TextStyle::Monospace)
            .hint_text(hint)
            .desired_width(f32::INFINITY)
            .desired_rows(30)
            .lock_focus(true)
            .show(ui);
        if let Some(r) = out.cursor_range {
            *cursor = Some(r.primary.index.0);
        }
    });
}

fn md_preview(ui: &mut egui::Ui, id: &str, cache: &mut CommonMarkCache, base_dir: &Path, title: &str, subtitle: &str, text: &str) {
    egui::ScrollArea::vertical().id_salt(id).auto_shrink(false).show(ui, |ui| {
        ui.heading(if title.is_empty() { "Sem título" } else { title });
        if !subtitle.is_empty() {
            ui.weak(subtitle);
        }
        ui.add_space(8.0);
        CommonMarkViewer::new()
            .default_implicit_uri_scheme(format!("file://{}/", base_dir.display()))
            .max_image_width(Some(720))
            .show(ui, cache, text);
    });
}

fn publish_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> bool {
    let btn = egui::Button::new(RichText::new(label).strong().color(Color32::WHITE)).fill(BLUE).min_size([0.0, 28.0].into());
    ui.add_enabled(enabled, btn).clicked()
}

fn status_badge(ui: &mut egui::Ui, color: Color32, label: &str) {
    ui.colored_label(color, format!("●  {label}"));
}

const MD_HINT: &str = "Escreva aqui em Markdown…\n\n## Subtítulo\n\nTexto com *itálico*, **negrito** e [links](https://…).";

// --- barra lateral ----------------------------------------------------------

impl App {
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let Some(m) = self.manifest.clone() else { return };
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            for (s, label) in [(Section::Posts, "Postagens"), (Section::Pages, "Páginas"), (Section::Research, "Pesquisa"), (Section::Papers, "Papers"), (Section::Site, "Site")] {
                ui.selectable_value(&mut self.section, s, label);
            }
        });
        ui.add_space(6.0);
        let current = self.doc.as_ref().map(Doc::target);
        let mut clicked: Option<Target> = None;
        let mut item = |ui: &mut egui::Ui, t: Target, text: String| {
            if ui.selectable_label(current.as_ref() == Some(&t), text).clicked() && current.as_ref() != Some(&t) {
                clicked = Some(t);
            }
        };
        match self.section {
            Section::Posts => {
                let dirs = m.post_dirs();
                if dirs.len() > 1 {
                    let before = self.post_lang.clone();
                    ui.horizontal_wrapped(|ui| {
                        for (code, _) in &dirs {
                            let name = m.lang(code).map(|l| l.nome.clone()).unwrap_or_default();
                            ui.selectable_value(&mut self.post_lang, code.clone(), name);
                        }
                    });
                    if self.post_lang != before {
                        self.refresh();
                    }
                }
                if dirs.is_empty() {
                    ui.weak("Nenhuma língua tem pasta de postagens no _idiomas.toml.");
                    return;
                }
                if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("＋  Nova postagem")).clicked() {
                    item(ui, Target::NewPost(self.post_lang.clone()), String::new());
                }
                ui.separator();
                let lang = self.post_lang.clone();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    if current == Some(Target::NewPost(lang.clone())) {
                        let _ = ui.selectable_label(true, "✏  Nova postagem");
                    }
                    for p in &self.posts {
                        let status = if p.draft { "Rascunho" } else { "Publicado" };
                        let unsent = if p.unsent && !p.draft { " · não enviado" } else { "" };
                        let title = if p.title.is_empty() { &p.slug } else { &p.title };
                        item(ui, Target::Post(lang.clone(), p.slug.clone()), format!("{title}\n{} · {status}{unsent}", p.date));
                    }
                    if self.posts.is_empty() {
                        ui.weak("Nenhuma postagem ainda.");
                    }
                });
            }
            Section::Pages => {
                if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("＋  Nova página")).clicked() {
                    self.page_form = Some(PageForm { slug: String::new(), titles: vec![String::new(); m.langs.len()], in_menu: true, message: None });
                }
                ui.weak("Uma versão por língua, lado a lado.");
                ui.separator();
                for (key, unsent) in &self.page_unsent {
                    let mark = if *unsent { "\nalterações não enviadas" } else { "" };
                    item(ui, Target::Page(key.clone()), format!("{}{mark}", manifest::page_name(key)));
                }
            }
            Section::Research => {
                ui.weak("Listas da página de Pesquisa.");
                ui.separator();
                for (key, unsent) in &self.list_unsent {
                    let mark = if *unsent { "\nalterações não enviadas" } else { "" };
                    item(ui, Target::List(key.clone()), format!("{}{mark}", manifest::list_name(key)));
                }
                ui.add_space(8.0);
                ui.weak("Working papers ficam na aba Papers.");
            }
            Section::Papers => {
                if ui.add_sized([ui.available_width(), 30.0], egui::Button::new("＋  Novo working paper")).clicked() {
                    item(ui, Target::NewPaper, String::new());
                }
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    if current == Some(Target::NewPaper) {
                        let _ = ui.selectable_label(true, "✏  Novo working paper");
                    }
                    for p in &self.papers {
                        let unsent = if p.unsent { " · não enviado" } else { "" };
                        item(ui, Target::Paper(p.slug.clone()), format!("{}\n{}{unsent}", p.title, &p.date[..p.date.len().min(4)]));
                    }
                });
            }
            Section::Site => {
                ui.weak("Partes comuns a todas as páginas.");
                ui.separator();
                item(ui, Target::Menu, "Menu\nitens, ordem e rótulos".into());
                item(ui, Target::Settings, "Configurações\ntítulo, rodapé, foto e links".into());
            }
        }
        if let Some(t) = clicked {
            self.open(t);
        }
    }
}

// --- editores ---------------------------------------------------------------

impl App {
    /// Linha de status + botões comuns. Devolve (salvar, anexar, publicar).
    fn toolbar(&mut self, ui: &mut egui::Ui, attach_label: Option<&str>, views: bool) -> (bool, bool, bool) {
        let site = self.site.clone().unwrap();
        let busy = self.publishing.is_some();
        let dirty = self.dirty();
        let unsent = self.doc_unsent();
        let doc = self.doc.as_ref().unwrap();
        let draft = matches!(doc, Doc::Post(p) if p.draft);
        let is_new = doc.is_new();
        let url = (!draft).then(|| doc.url(&site)).flatten();
        ui.horizontal(|ui| {
            match (is_new, draft) {
                (true, _) => status_badge(ui, ui.visuals().weak_text_color(), "Novo — ainda não salvo"),
                (false, true) => status_badge(ui, ORANGE, "Rascunho — só no seu computador"),
                _ if unsent || dirty => status_badge(ui, ORANGE, "Alterações ainda não enviadas ao site"),
                _ => status_badge(ui, GREEN, "Igual ao site"),
            }
            if dirty {
                ui.weak("(editando…)");
            }
        });
        ui.add_space(4.0);
        let (mut save, mut attach, mut publish) = (false, false, false);
        ui.horizontal(|ui| {
            save = ui.add_enabled(!busy, egui::Button::new("Salvar  ⌘S")).clicked();
            if let Some(label) = attach_label {
                attach = ui.add_enabled(!busy, egui::Button::new(label)).clicked();
            }
            if let Some(url) = url {
                ui.hyperlink_to("Ver no site ↗", url);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                publish = publish_button(ui, if draft || is_new { "Publicar no site" } else { "Enviar ao site" }, !busy);
                if busy {
                    ui.spinner();
                }
                if views {
                    ui.add_space(12.0);
                    ui.selectable_value(&mut self.view, View::Preview, "Visualizar");
                    ui.selectable_value(&mut self.view, View::Split, "Lado a lado");
                    ui.selectable_value(&mut self.view, View::Edit, "Editar");
                }
            });
        });
        ui.separator();
        (save, attach, publish)
    }

    fn body_area(&mut self, ui: &mut egui::Ui, base_dir: &Path) {
        let view = self.view;
        let (cursor, cache) = (&mut self.cursor, &mut self.md_cache);
        let (body, title, subtitle) = match self.doc.as_mut() {
            Some(Doc::Post(p)) => (&mut p.body, p.title.clone(), p.description.clone()),
            Some(Doc::Paper(p)) => (&mut p.body, p.title.clone(), p.authors.clone()),
            _ => return,
        };
        match view {
            View::Edit => md_editor(ui, "body", body, cursor, MD_HINT),
            View::Preview => md_preview(ui, "preview", cache, base_dir, &title, &subtitle, body),
            View::Split => {
                ui.columns(2, |cols| {
                    md_editor(&mut cols[0], "body", body, cursor, MD_HINT);
                    md_preview(&mut cols[1], "preview", cache, base_dir, &title, &subtitle, body);
                });
            }
        }
    }

    fn post_editor(&mut self, ui: &mut egui::Ui) {
        let lang_name = match &self.doc {
            Some(Doc::Post(p)) => self.lang_name(&p.lang),
            _ => return,
        };
        let multi = self.manifest.as_ref().is_some_and(|m| m.post_dirs().len() > 1);
        let Some(Doc::Post(post)) = self.doc.as_mut() else { return };
        if multi {
            ui.weak(format!("Postagem em {lang_name}"));
        }
        egui::Grid::new("meta").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label("Título");
            ui.add(egui::TextEdit::singleline(&mut post.title).hint_text("Título da postagem").desired_width(f32::INFINITY));
            ui.end_row();
            ui.label("Resumo");
            ui.add(egui::TextEdit::singleline(&mut post.description).hint_text("Uma ou duas frases que aparecem na lista de postagens").desired_width(f32::INFINITY));
            ui.end_row();
            ui.label("Data");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut post.date).hint_text("AAAA-MM-DD").desired_width(110.0));
                ui.add_space(16.0);
                ui.label("Categorias");
                ui.add(egui::TextEdit::singleline(&mut post.categories).hint_text("separadas por vírgula").desired_width(f32::INFINITY));
            });
            ui.end_row();
        });
        ui.add_space(6.0);
        let dir = if post.slug.is_empty() { post.dir.clone() } else { post.rel_dir() };
        let (save, attach, publish) = self.toolbar(ui, Some("Inserir imagem…"), true);
        let site = self.site.clone().unwrap();
        self.body_area(ui, &site.join(dir));
        self.actions(save, attach, publish);
    }

    fn paper_editor(&mut self, ui: &mut egui::Ui) {
        let names: Vec<String> = match &self.doc {
            Some(Doc::Paper(p)) => p.translations.iter().map(|t| self.lang_name(&t.lang)).collect(),
            _ => return,
        };
        let root = self.manifest.as_ref().map(|m| m.root().nome.clone()).unwrap_or_default();
        let Some(Doc::Paper(p)) = self.doc.as_mut() else { return };
        egui::Grid::new("paper-meta").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
            ui.label(format!("Título ({root})"));
            ui.add(egui::TextEdit::singleline(&mut p.title).hint_text("Título da página do paper").desired_width(f32::INFINITY));
            ui.end_row();
            ui.label("Autores");
            ui.add(egui::TextEdit::singleline(&mut p.authors).hint_text("separados por vírgula").desired_width(f32::INFINITY));
            ui.end_row();
            ui.label("Data");
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut p.date).hint_text("AAAA-MM-DD").desired_width(110.0));
                ui.add_space(16.0);
                ui.label("Categorias");
                ui.add(egui::TextEdit::singleline(&mut p.categories).hint_text("working paper, energy economics").desired_width(f32::INFINITY));
            });
            ui.end_row();
            ui.label(format!("Resumo ({root})"));
            ui.add(egui::TextEdit::multiline(&mut p.description).desired_rows(2).hint_text("Uma ou duas frases para a lista da página de Pesquisa").desired_width(f32::INFINITY));
            ui.end_row();
            for (t, name) in p.translations.iter_mut().zip(&names) {
                ui.label(format!("Título ({name})"));
                ui.add(egui::TextEdit::singleline(&mut t.title).id_salt(("tt", &t.lang)).hint_text("vazio = usa o título acima").desired_width(f32::INFINITY));
                ui.end_row();
                ui.label(format!("Resumo ({name})"));
                ui.add(egui::TextEdit::multiline(&mut t.description).id_salt(("td", &t.lang)).desired_rows(2).desired_width(f32::INFINITY));
                ui.end_row();
            }
        });
        ui.add_space(6.0);
        let dir = p.dir.clone();
        let (save, attach, publish) = self.toolbar(ui, Some("Anexar PDF…"), true);
        let site = self.site.clone().unwrap();
        self.body_area(ui, &site.join(dir));
        self.actions(save, attach, publish);
    }

    fn page_editor(&mut self, ui: &mut egui::Ui) {
        let Some(Doc::Page(page)) = &self.doc else { return };
        ui.heading(manifest::page_name(&page.key));
        ui.add_space(4.0);
        let available: Vec<(String, String)> = page.sides.iter().map(|s| (s.lang.clone(), self.lang_name(&s.lang))).collect();
        let key = page.key.clone();
        let (save, _, publish) = self.toolbar(ui, None, false);
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.page_preview, false, "Editar");
            ui.selectable_value(&mut self.page_preview, true, "Visualizar");
            ui.weak("   Foto e links: aba Site › Configurações. Menu: aba Site › Menu.");
            if !manifest::PROTECTED_PAGES.contains(&key.as_str()) {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(RichText::new("Remover página…").color(RED)).clicked() {
                        self.ask = Some(Ask::RemovePage(key.clone()));
                    }
                });
            }
        });
        ui.add_space(4.0);
        let site = self.site.clone().unwrap();
        let preview = self.page_preview;
        let (cursor, cache, compare) = (&mut self.cursor, &mut self.md_cache, &mut self.compare);
        let Some(Doc::Page(page)) = self.doc.as_mut() else { return };
        ui.columns(2, |cols| {
            for (col, ui) in cols.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt(("lang", col))
                        .selected_text(available.iter().find(|(c, _)| *c == compare[col]).map(|(_, n)| n.as_str()).unwrap_or("—"))
                        .show_ui(ui, |ui| {
                            for (code, name) in &available {
                                ui.selectable_value(&mut compare[col], code.clone(), name);
                            }
                        });
                });
                let Some(side) = page.sides.iter_mut().find(|s| s.lang == compare[col]) else {
                    ui.weak("Esta página não existe nessa língua.");
                    continue;
                };
                ui.weak(&side.rel);
                ui.add(egui::TextEdit::singleline(&mut side.title).id_salt((&side.lang, "title")).desired_width(f32::INFINITY));
                ui.add_space(4.0);
                let file = site.join(&side.rel);
                let base = file.parent().unwrap_or(&site);
                if preview {
                    md_preview(ui, &format!("prev-{col}"), cache, base, &side.title, "", &side.body);
                } else {
                    md_editor(ui, &format!("body-{}", side.lang), &mut side.body, cursor, "");
                }
            }
        });
        self.actions(save, false, publish);
    }

    fn list_editor(&mut self, ui: &mut egui::Ui) {
        let names: Vec<String> = match &self.doc {
            Some(Doc::List(l)) => l.langs.iter().map(|x| self.lang_name(&x.lang)).collect(),
            _ => return,
        };
        if let Some(Doc::List(list)) = &self.doc {
            ui.heading(manifest::list_name(&list.key));
        }
        ui.add_space(4.0);
        let (save, _, publish) = self.toolbar(ui, None, false);
        let list_sel = &mut self.list_sel;
        let Some(Doc::List(list)) = self.doc.as_mut() else { return };
        let mut new_item = false;
        let mut delete = None;
        ui.columns(2, |cols| {
            let (left, right) = cols.split_at_mut(1);
            let ui = &mut left[0];
            if ui.button("＋  Novo item").clicked() {
                new_item = true;
            }
            ui.add_space(4.0);
            egui::ScrollArea::vertical().id_salt("items").auto_shrink(false).show(ui, |ui| {
                for i in 0..list.len() {
                    let (title, year) = list.label(i);
                    if ui.selectable_label(*list_sel == Some(i), format!("{year}  {title}")).clicked() {
                        *list_sel = Some(i);
                    }
                }
            });

            let ui = &mut right[0];
            let Some(i) = list_sel.filter(|i| *i < list.len()) else {
                ui.add_space(40.0);
                ui.weak("Escolha um item à esquerda ou crie um novo.");
                return;
            };
            egui::ScrollArea::vertical().id_salt("item-form").auto_shrink(false).show(ui, |ui| {
                ui.strong("Comum a todas as línguas");
                egui::Grid::new(("shared", i)).num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                    for (k, label, hint) in [("date", "Data", "AAAA-MM-DD"), ("path", "Link", "https://… (destino do título)")] {
                        ui.label(label);
                        if let Some(v) = field(ui, (k, i), list.shared(i, k), 1, hint) {
                            list.set_shared(i, k, &v);
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(8.0);
                for li in 0..list.langs.len() {
                    ui.strong(&names[li]);
                    let lang = list.langs[li].lang.clone();
                    if let Some(it) = list.langs[li].file.items.get_mut(i) {
                        egui::Grid::new((&lang, i)).num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                            for (k, label, rows) in [("title", "Título", 2), ("author", "Autores", 1), ("description", "Descrição", 3)] {
                                ui.label(label);
                                if let Some(v) = field(ui, (&lang, k, i), it.get(k), rows, "") {
                                    it.set(k, &v);
                                }
                                ui.end_row();
                            }
                        });
                    } else {
                        ui.horizontal(|ui| {
                            ui.weak("Este item não existe nesta língua.");
                            if ui.button("Copiar de outra língua").clicked() {
                                list.copy_into(i, li);
                            }
                        });
                    }
                    ui.add_space(8.0);
                }
                if ui.button(RichText::new("Excluir item").color(RED)).clicked() {
                    delete = Some(i);
                }
            });
        });
        if new_item {
            list.add(&posts::today());
            self.list_sel = Some(0);
        }
        if let Some(i) = delete {
            self.ask = Some(Ask::DeleteItem(i));
        }
        self.actions(save, false, publish);
    }

    fn actions(&mut self, save: bool, attach: bool, publish: bool) {
        if save {
            self.save(false);
        }
        if attach {
            self.attach();
        }
        if publish {
            match self.doc.as_ref().map(Doc::validate) {
                Some(Err(e)) => self.set_status(e, true),
                _ => self.ask = Some(Ask::Publish),
            }
        }
    }
}

// --- janelas ----------------------------------------------------------------

impl App {
    fn languages_window(&mut self, ctx: &egui::Context) {
        let (Some(site), Some(m)) = (self.site.clone(), self.manifest.clone()) else { return };
        let Some(form) = self.lang_form.as_mut() else { return };
        // sugere rótulos do menu quando muda o código ou a origem
        let key = (form.code.trim().to_lowercase(), form.source.clone());
        if form.labels_for != key {
            form.labels = languages::nav_labels(&site, &m, &form.source, &key.0);
            form.labels_for = key;
        }
        let busy = self.publishing.is_some();
        let mut close = false;
        let mut create = false;
        let mut send: Option<String> = None;
        egui::Window::new("Línguas do site").collapsible(false).resizable(true).default_width(560.0).show(ctx, |ui| {
            egui::Grid::new("langs").num_columns(3).spacing([16.0, 6.0]).show(ui, |ui| {
                for (i, l) in m.langs.iter().enumerate() {
                    ui.strong(&l.nome);
                    ui.weak(if i == 0 { "/  (raiz)".to_string() } else { format!("/{}/", l.codigo) });
                    if i > 0 && publish::unsent(&site, &languages::setup_paths(&m, &l.codigo)) {
                        if ui.add_enabled(!busy, egui::Button::new("Enviar configuração ao site")).clicked() {
                            send = Some(l.codigo.clone());
                        }
                    } else {
                        ui.label("");
                    }
                    ui.end_row();
                }
            });
            ui.separator();
            ui.heading("Adicionar língua");
            ui.weak("A língua nova começa como cópia de outra (páginas, listas e a página de postagens), para você traduzir aqui no painel. Nada vai ao site até você enviar.");
            ui.add_space(6.0);
            egui::Grid::new("new-lang").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Código");
                ui.add(egui::TextEdit::singleline(&mut form.code).hint_text("ja, es, fr…").desired_width(100.0));
                ui.end_row();
                ui.label("Nome no menu");
                ui.add(egui::TextEdit::singleline(&mut form.name).hint_text("日本語").desired_width(200.0));
                ui.end_row();
                ui.label("Copiar de");
                egui::ComboBox::from_id_salt("source").selected_text(m.lang(&form.source).map(|l| l.nome.as_str()).unwrap_or("—")).show_ui(ui, |ui| {
                    for l in &m.langs[1..] {
                        ui.selectable_value(&mut form.source, l.codigo.clone(), &l.nome);
                    }
                });
                ui.end_row();
            });
            if !form.labels.is_empty() {
                ui.add_space(6.0);
                ui.strong("Itens do menu");
                egui::Grid::new("labels").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    for (i, (href, label)) in form.labels.iter_mut().enumerate() {
                        ui.weak(href.as_str());
                        ui.add(egui::TextEdit::singleline(label).id_salt(("label", i)).desired_width(220.0));
                        ui.end_row();
                    }
                });
            }
            if let Some((msg, err)) = &form.message {
                ui.add_space(6.0);
                if *err {
                    ui.colored_label(RED, msg);
                } else {
                    ui.colored_label(GREEN, msg);
                }
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                create = ui.add_enabled(!busy, egui::Button::new("Criar língua")).clicked();
                close = ui.button("Fechar").clicked();
            });
        });
        if create {
            let r = languages::add(&site, &m, &form.code, &form.name, &form.source, &form.labels);
            form.message = Some(match r {
                Ok((_, warnings)) => {
                    let mut msg = format!(
                        "Língua criada no computador. Traduza as páginas, listas e papers no painel; depois use “Enviar configuração ao site”."
                    );
                    for w in warnings {
                        msg += &format!("\n⚠ {w}");
                    }
                    form.code.clear();
                    form.name.clear();
                    (msg, false)
                }
                Err(e) => (e, true),
            });
            self.refresh();
        }
        if let Some(code) = send {
            self.ask = Some(Ask::PublishLang(code));
        }
        if close {
            self.lang_form = None;
        }
    }

    fn open_languages(&mut self) {
        let source = self.manifest.as_ref().and_then(|m| m.langs.get(1)).map(|l| l.codigo.clone()).unwrap_or_default();
        self.lang_form = Some(LangForm { code: String::new(), name: String::new(), source, labels: vec![], labels_for: (String::new(), String::new()), message: None });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(ask) = &self.ask else { return };
        let mut result: Option<bool> = None;
        let title = self.doc.as_ref().map(Doc::title).unwrap_or_default();
        let paths = match ask {
            Ask::PublishLang(code) => self.manifest.as_ref().map(|m| languages::setup_paths(m, code)).unwrap_or_default(),
            _ => self.doc.as_ref().map(Doc::paths).unwrap_or_default(),
        };
        let lang_title = match ask {
            Ask::PublishLang(code) => self.lang_name(code),
            _ => String::new(),
        };
        let modal = egui::Modal::new(egui::Id::unique("ask")).show(ctx, |ui| {
            ui.set_max_width(480.0);
            let ok = match ask {
                Ask::Publish | Ask::PublishLang(_) => {
                    ui.heading("Enviar ao site?");
                    ui.add_space(6.0);
                    if matches!(ask, Ask::PublishLang(_)) {
                        ui.label(format!("A versão em {lang_title} vai ficar visível publicamente, com o link no menu de todas as línguas."));
                    } else {
                        ui.label(format!("“{title}” vai ficar visível publicamente."));
                    }
                    ui.add_space(4.0);
                    ui.weak("O painel faz commit só destes caminhos e envia para o GitHub:");
                    for p in &paths {
                        ui.label(RichText::new(p).monospace().size(12.0));
                    }
                    "Enviar"
                }
                Ask::Discard(_) => {
                    ui.heading("Descartar alterações?");
                    ui.add_space(6.0);
                    ui.label("Não deu para salvar (falta um campo obrigatório ou a data está inválida).");
                    "Descartar"
                }
                Ask::RemovePage(key) => {
                    ui.heading("Remover esta página?");
                    ui.add_space(6.0);
                    ui.label(format!("“{}” sai do site em todas as línguas, junto com o item do menu.", manifest::page_name(key)));
                    ui.weak("Os arquivos são apagados do computador e a remoção é enviada ao site em seguida. Uma página que já foi enviada antes continua no histórico do git.");
                    "Remover e enviar"
                }
                Ask::DeleteItem(_) => {
                    ui.heading("Excluir este item?");
                    ui.add_space(6.0);
                    ui.label("Ele sai da lista em todas as línguas. Até você enviar, a mudança fica só no computador.");
                    "Excluir"
                }
            };
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if ui.button(ok).clicked() {
                    result = Some(true);
                }
                if ui.button("Cancelar").clicked() {
                    result = Some(false);
                }
            });
        });
        if modal.should_close() && result.is_none() {
            result = Some(false);
        }
        if let Some(yes) = result {
            match self.ask.take() {
                Some(Ask::Publish) if yes => self.start_publish(ctx),
                Some(Ask::PublishLang(code)) if yes => {
                    if let Some(m) = &self.manifest {
                        let job = Job {
                            paths: languages::setup_paths(m, &code),
                            renders: vec![(code.clone(), String::new())],
                            message: format!("Adiciona/atualiza a versão em {}", self.lang_name(&code)),
                            url: self.site.as_deref().and_then(|s| publish::page_url(s, &format!("{code}/"))),
                        };
                        let name = self.lang_name(&code);
                        self.run_job(ctx, format!("versão em {name}"), job);
                    }
                }
                Some(Ask::Discard(t)) if yes => self.load(t),
                Some(Ask::RemovePage(key)) if yes => self.remove_page(ctx, &key),
                Some(Ask::DeleteItem(i)) if yes => {
                    if let Some(Doc::List(l)) = self.doc.as_mut() {
                        l.remove(i);
                        self.list_sel = None;
                        self.last_edit = Some(Instant::now());
                    }
                }
                _ => {}
            }
        }
    }
}

// --- site: páginas novas, menu e configurações -------------------------------

impl App {
    fn new_page_window(&mut self, ctx: &egui::Context) {
        let (Some(site), Some(m)) = (self.site.clone(), self.manifest.clone()) else { return };
        let Some(form) = self.page_form.as_mut() else { return };
        let (mut create, mut close) = (false, false);
        egui::Window::new("Nova página").collapsible(false).resizable(false).default_width(480.0).show(ctx, |ui| {
            ui.weak("A página é criada em todas as línguas, com o título de cada uma e o texto vazio, para você escrever na aba Páginas.");
            ui.add_space(6.0);
            egui::Grid::new("new-page").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Nome do arquivo");
                ui.add(egui::TextEdit::singleline(&mut form.slug).hint_text("contato, palestras…").desired_width(200.0));
                ui.end_row();
                for (l, title) in m.langs.iter().zip(form.titles.iter_mut()) {
                    ui.label(format!("Título ({})", l.nome));
                    ui.add(egui::TextEdit::singleline(title).id_salt(("np", &l.codigo)).desired_width(260.0));
                    ui.end_row();
                }
                ui.label("");
                ui.checkbox(&mut form.in_menu, "Mostrar no menu");
                ui.end_row();
            });
            let slug = form.slug.trim().to_lowercase();
            if !slug.is_empty() {
                ui.weak(m.langs.iter().map(|l| pages::new_page_path(&m, &l.codigo, &slug)).collect::<Vec<_>>().join("  ·  "));
            }
            if let Some(msg) = &form.message {
                ui.colored_label(RED, msg);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                create = ui.button("Criar página").clicked();
                close = ui.button("Cancelar").clicked();
            });
        });
        if create {
            let slug = form.slug.trim().to_lowercase();
            match pages::create(&site, &m, &slug, &form.titles, form.in_menu) {
                Ok(()) => {
                    self.page_form = None;
                    self.refresh();
                    self.section = Section::Pages;
                    self.open(Target::Page(slug));
                    self.set_status("Página criada no computador. Escreva o texto e envie quando quiser.", false);
                }
                Err(e) => form.message = Some(e),
            }
        } else if close {
            self.page_form = None;
        }
    }

    fn remove_page(&mut self, ctx: &egui::Context, key: &str) {
        let (Some(site), Some(m)) = (self.site.clone(), self.manifest.clone()) else { return };
        match pages::remove(&site, &m, key) {
            Ok(paths) => {
                self.doc = None;
                self.saved = None;
                self.refresh();
                let renders = m.langs.iter().map(|l| (l.codigo.clone(), String::new())).collect();
                let job = Job { paths, renders, message: format!("Remove página: {}", manifest::page_name(key)), url: None };
                self.run_job(ctx, format!("remoção de {}", manifest::page_name(key)), job);
            }
            Err(e) => self.set_status(e, true),
        }
    }

    fn menu_editor(&mut self, ui: &mut egui::Ui) {
        let names: Vec<String> = match &self.doc {
            Some(Doc::Menu(d)) => d.langs.iter().map(|c| self.lang_name(c)).collect(),
            _ => return,
        };
        ui.heading("Menu do site");
        ui.weak("Itens do menu principal de cada língua. Desmarque para esconder; as setas mudam a ordem em todas as línguas.");
        ui.add_space(4.0);
        let (save, _, publish) = self.toolbar(ui, None, false);
        let Some(Doc::Menu(doc)) = self.doc.as_mut() else { return };
        let mut mv: Option<(usize, bool)> = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            egui::Grid::new("menu").num_columns(3 + names.len()).spacing([10.0, 8.0]).striped(true).show(ui, |ui| {
                ui.label("");
                ui.strong("Mostrar");
                ui.strong("Página");
                for n in &names {
                    ui.strong(n);
                }
                ui.end_row();
                let count = doc.rows.len();
                for (i, row) in doc.rows.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.add_enabled(i > 0, egui::Button::new("↑").small()).clicked() {
                            mv = Some((i, true));
                        }
                        if ui.add_enabled(i + 1 < count, egui::Button::new("↓").small()).clicked() {
                            mv = Some((i, false));
                        }
                    });
                    ui.checkbox(&mut row.visible, "");
                    ui.label(menu::row_name(&row.key));
                    for (li, cell) in row.cells.iter_mut().enumerate() {
                        match cell {
                            Some(c) => {
                                ui.add_enabled(row.visible, egui::TextEdit::singleline(&mut c.label).id_salt(("menu", i, li)).desired_width(150.0));
                            }
                            None => {
                                ui.weak("—");
                            }
                        }
                    }
                    ui.end_row();
                }
            });
        });
        if let Some((i, up)) = mv {
            doc.move_row(i, up);
        }
        self.actions(save, false, publish);
    }

    fn settings_editor(&mut self, ui: &mut egui::Ui) {
        let Some(Doc::Settings(st)) = &self.doc else { return };
        let page_langs: Vec<String> = st.about_pages.iter().map(|(c, _)| self.lang_name(c)).collect();
        let footer_langs: Vec<String> = st.footers.iter().map(|f| self.lang_name(&f.lang)).collect();
        ui.heading("Configurações do site");
        ui.add_space(4.0);
        let (save, _, publish) = self.toolbar(ui, None, false);
        let site = self.site.clone().unwrap();
        let mut pick_photo = false;
        let Some(Doc::Settings(st)) = self.doc.as_mut() else { return };
        let mut mv: Option<(usize, bool)> = None;
        let mut remove: Option<usize> = None;
        let mut add = false;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            ui.strong("Site");
            egui::Grid::new("site").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("Título");
                ui.add(egui::TextEdit::singleline(&mut st.title).desired_width(f32::INFINITY));
                ui.end_row();
                ui.label("Descrição");
                ui.add(egui::TextEdit::multiline(&mut st.description).desired_rows(2).hint_text("Aparece em buscadores e ao compartilhar o link").desired_width(f32::INFINITY));
                ui.end_row();
            });
            ui.add_space(12.0);

            ui.strong("Rodapé");
            egui::Grid::new("footer").num_columns(3).spacing([12.0, 6.0]).show(ui, |ui| {
                ui.label("");
                ui.weak("Esquerda");
                ui.weak("Direita");
                ui.end_row();
                for (f, name) in st.footers.iter_mut().zip(&footer_langs) {
                    ui.label(name);
                    for (side, value) in [("l", &mut f.left), ("r", &mut f.right)] {
                        match value {
                            Some(v) => {
                                ui.add(egui::TextEdit::singleline(v).id_salt(("foot", &f.lang, side)).desired_width(300.0));
                            }
                            None => {
                                ui.weak("(não definido no perfil)");
                            }
                        }
                    }
                    ui.end_row();
                }
            });
            ui.add_space(12.0);

            ui.strong("Foto da página Sobre");
            ui.horizontal(|ui| {
                if !st.photo.is_empty() {
                    ui.add(egui::Image::new(format!("file://{}", site.join(&st.photo).display())).max_size([96.0, 96.0].into()).corner_radius(48.0));
                }
                ui.vertical(|ui| {
                    ui.label(if st.photo.is_empty() { "Sem foto".to_string() } else { st.photo.clone() });
                    pick_photo = ui.button("Escolher foto…").clicked();
                    ui.weak("A mesma foto vale para todas as línguas.");
                });
            });
            ui.add_space(12.0);

            ui.strong("Links da página Sobre");
            ui.weak("Ícones: nomes do Bootstrap Icons (mortarboard-fill, person-vcard, linkedin, envelope, github…).");
            egui::Grid::new("links").num_columns(4 + page_langs.len()).spacing([8.0, 6.0]).striped(true).show(ui, |ui| {
                ui.label("");
                ui.weak("Ícone");
                ui.weak("Endereço");
                for n in &page_langs {
                    ui.weak(format!("Texto ({n})"));
                }
                ui.label("");
                ui.end_row();
                let count = st.links.len();
                for (i, l) in st.links.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.add_enabled(i > 0, egui::Button::new("↑").small()).clicked() {
                            mv = Some((i, true));
                        }
                        if ui.add_enabled(i + 1 < count, egui::Button::new("↓").small()).clicked() {
                            mv = Some((i, false));
                        }
                    });
                    ui.add(egui::TextEdit::singleline(&mut l.icon).id_salt(("icon", i)).desired_width(120.0));
                    ui.add(egui::TextEdit::singleline(&mut l.href).id_salt(("href", i)).desired_width(240.0));
                    for (li, (_, text)) in l.texts.iter_mut().enumerate() {
                        ui.add(egui::TextEdit::singleline(text).id_salt(("lt", i, li)).desired_width(130.0));
                    }
                    if ui.small_button(RichText::new("Remover").color(RED)).clicked() {
                        remove = Some(i);
                    }
                    ui.end_row();
                }
            });
            add = ui.button("＋  Novo link").clicked();
        });
        if let Some((i, up)) = mv {
            st.move_link(i, up);
        }
        if let Some(i) = remove {
            st.links.remove(i);
        }
        if add {
            st.add_link();
        }
        if pick_photo {
            if let Some(file) = rfd::FileDialog::new().set_title("Escolha a foto").add_filter("Imagens", &posts::IMAGE_EXT).pick_file() {
                match posts::copy_into(&site, &file, "", &posts::IMAGE_EXT) {
                    Ok(name) => {
                        st.photo = name;
                        self.last_edit = Some(Instant::now());
                    }
                    Err(e) => self.set_status(e, true),
                }
            }
        }
        self.actions(save, false, publish);
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll_publish();

        // Salvamento automático alguns segundos depois da última edição.
        if self.dirty() && self.publishing.is_none() {
            match self.last_edit {
                Some(t) if t.elapsed() >= AUTOSAVE_AFTER => {
                    if self.save(true) {
                        self.set_status("Salvo automaticamente.", false);
                    } else {
                        self.last_edit = None;
                    }
                }
                Some(t) => ctx.request_repaint_after(AUTOSAVE_AFTER.saturating_sub(t.elapsed())),
                None => {}
            }
        }

        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.strong("Painel do Site");
                let mut switch: Option<PathBuf> = None;
                let (mut add, mut forget) = (false, false);
                let current = self.site.as_deref().map(site_label).unwrap_or_else(|| "Nenhum site".into());
                egui::ComboBox::from_id_salt("site").selected_text(current).width(220.0).show_ui(ui, |ui| {
                    for p in &self.sites {
                        let selected = self.site.as_ref() == Some(p);
                        if ui.selectable_label(selected, site_label(p)).on_hover_text(p.display().to_string()).clicked() && !selected {
                            switch = Some(p.clone());
                        }
                    }
                    ui.separator();
                    add = ui.button("Adicionar site…").clicked();
                    if self.site.is_some() {
                        forget = ui.button("Tirar este site da lista").on_hover_text("A pasta não é apagada; só some do painel.").clicked();
                    }
                });
                if let Some(site) = &self.site {
                    ui.weak(site.display().to_string());
                }
                if self.publishing.is_none() {
                    if let Some(p) = switch {
                        self.switch_site(p);
                    } else if add {
                        self.add_site();
                    } else if forget {
                        self.forget_site();
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.manifest.is_some() && ui.small_button("Línguas…").clicked() {
                        self.open_languages();
                    }
                    if !self.branch.is_empty() && self.branch != "main" {
                        ui.colored_label(RED, format!("branch: {}", self.branch));
                    }
                    if ui.small_button("Atualizar").clicked() {
                        self.refresh();
                    }
                });
            });
            ui.add_space(2.0);
        });

        if self.site.is_none() || self.manifest.is_none() {
            let needs_setup = self.site.as_ref().is_some_and(|s| !s.join(manifest::FILE).exists());
            egui::CentralPanel::default_margins().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(120.0);
                    if self.site.is_none() {
                        ui.heading("Qual site você quer editar?");
                        ui.label("Escolha a pasta de um projeto Quarto (a que tem _quarto.yml).");
                        ui.add_space(12.0);
                        if ui.button("Adicionar site…").clicked() {
                            self.add_site();
                        }
                    } else if needs_setup {
                        ui.heading("Este site ainda não está preparado para o painel");
                        ui.label(format!("O painel precisa de um {} que diga quais são as línguas e as páginas.", manifest::FILE));
                        ui.label("Posso criar um a partir das páginas e perfis que já existem; nada é enviado ao site.");
                        ui.add_space(12.0);
                        if ui.button("Preparar este site").clicked() {
                            self.prepare_site();
                        }
                    }
                    if let Some((msg, err)) = &self.status {
                        ui.add_space(8.0);
                        ui.colored_label(if *err { RED } else { GREEN }, msg);
                    }
                });
            });
            return;
        }

        egui::Panel::bottom("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                match &self.status {
                    Some((msg, true)) => ui.colored_label(RED, msg),
                    Some((msg, false)) => ui.label(msg),
                    None => ui.weak("Tudo é salvo no computador; só vai para o site quando você clicar em Publicar/Enviar."),
                };
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.log.is_empty() {
                        let label = if self.show_log { "Ocultar registro" } else { "Mostrar registro" };
                        if ui.small_button(label).clicked() {
                            self.show_log = !self.show_log;
                        }
                    }
                });
            });
        });

        if self.show_log && !self.log.is_empty() {
            egui::Panel::bottom("log").resizable(true).default_size(180.0).show(ui, |ui| {
                egui::ScrollArea::vertical().stick_to_bottom(true).auto_shrink(false).show(ui, |ui| {
                    for line in &self.log {
                        ui.label(RichText::new(line).monospace().size(12.0));
                    }
                });
            });
        }

        egui::Panel::left("nav").resizable(true).default_size(280.0).min_size(210.0).show(ui, |ui| self.sidebar(ui));

        egui::CentralPanel::default_margins().show(ui, |ui| {
            let before = self.doc.clone();
            match &self.doc {
                Some(Doc::Post(_)) => self.post_editor(ui),
                Some(Doc::Page(_)) => self.page_editor(ui),
                Some(Doc::List(_)) => self.list_editor(ui),
                Some(Doc::Paper(_)) => self.paper_editor(ui),
                Some(Doc::Menu(_)) => self.menu_editor(ui),
                Some(Doc::Settings(_)) => self.settings_editor(ui),
                None => {
                    ui.vertical_centered(|ui| {
                        ui.add_space(140.0);
                        ui.heading("Escolha algo para editar na barra ao lado.");
                        ui.add_space(8.0);
                        ui.weak("Postagens, páginas, listas de pesquisa e working papers, em todas as línguas do site.");
                    });
                }
            }
            // Qualquer edição no mesmo documento reinicia o relógio do autosave.
            if self.doc != before && before.as_ref().map(Doc::target) == self.doc.as_ref().map(Doc::target) {
                self.last_edit = Some(Instant::now());
            }
        });

        let cmd_s = ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::S));
        if cmd_s && self.doc.is_some() && self.publishing.is_none() {
            self.save(false);
        }

        self.languages_window(&ctx);
        self.new_page_window(&ctx);
        self.dialogs(&ctx);
    }

    fn on_exit(&mut self) {
        if self.dirty() {
            self.save(true);
        }
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Painel do Site")
            .with_inner_size([1320.0, 860.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native("Painel do Site", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
