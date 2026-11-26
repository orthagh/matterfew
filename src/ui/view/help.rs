//! Aide à la mise en forme (Markdown) : ce qu'on écrit, ce qu'on obtient.
//! Et le panneau de droite agrandi (Ctrl+Maj+.).

use super::dialogs::Dialog;
use super::*;

/// Exemples : (clé de libellé, Markdown).
const EXAMPLES: [(&str, &str); 12] = [
    ("bold", "**gras**"),
    ("italic", "_italique_"),
    ("strike", "~~barré~~"),
    ("code", "`code`"),
    ("link", "[Mattermost](https://mattermost.com)"),
    ("heading", "### Titre"),
    ("quote", "> citation"),
    ("list", "- élément\n- autre"),
    ("numbered", "1. premier\n2. second"),
    ("task", "- [x] fait\n- [ ] à faire"),
    ("code_block", "```sql\nSELECT 1;\n```"),
    ("emoji", ":tada: :+1:"),
];

impl ChatApp {
    pub(crate) fn open_markdown_help(&mut self, cx: &mut Context<Self>) {
        self.dialog = Some(Dialog::MarkdownHelp);
        cx.notify();
    }

    pub(crate) fn render_markdown_help(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let height = (self.viewport_h - 220.).clamp(240., 520.);
        let mono = cx.theme().mono_font_family.clone();
        let rows = EXAMPLES.iter().enumerate().map(|(ix, (key, source))| {
            let source = localized_example(key, source);
            h_flex()
                .items_start()
                .gap_4()
                .px_4()
                .py_2()
                .when(ix % 2 == 1, |el| el.bg(p.center_color.opacity(0.03)))
                .child(div().w(px(150.)).flex_none().secondary_size().text_color(p.muted()).child({
                    let label_key = format!("markdown_help.{key}");
                    t!(label_key.as_str()).to_string()
                }))
                .child(
                    div()
                        .w(px(220.))
                        .flex_none()
                        .px_2()
                        .py_1()
                        .rounded(px(4.))
                        .bg(p.center_color.opacity(0.06))
                        .font_family(mono.clone())
                        .secondary_size()
                        .child(source.clone()),
                )
                .child(div().flex_1().min_w_0().secondary_size().child(TextView::markdown(
                    SharedString::from(format!("md-help-{key}")),
                    markdown::prepare(&source),
                )))
        });
        let body = v_flex()
            .child(
                h_flex()
                    .gap_4()
                    .px_4()
                    .pb_2()
                    .caption_size()
                    .font_semibold()
                    .text_color(p.muted())
                    .child(div().w(px(150.)).child(t!("markdown_help.what")))
                    .child(div().w(px(220.)).child(t!("markdown_help.write")))
                    .child(div().child(t!("markdown_help.result"))),
            )
            .child(
                v_flex()
                    .id("md-help-list")
                    .max_h(px(height))
                    .overflow_y_scroll()
                    .border_t_1()
                    .border_color(p.border())
                    .children(rows),
            )
            .child(
                div()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(p.border())
                    .caption_size()
                    .text_color(p.muted())
                    .child(t!("markdown_help.footer")),
            )
            .into_any_element();
        self.dialog_shell(IconName::CircleQuestionMark, t!("markdown_help.title").into(), 680., body, None, cx)
    }

    /// Largeur du panneau de droite (fil, recherche…), agrandi ou non.
    pub(crate) fn right_panel_width(&self) -> f32 {
        if !self.panel_expanded {
            return THREAD_WIDTH;
        }
        let team_bar = if self.teams.len() >= 2 { TEAM_BAR_WIDTH } else { 0. };
        // Le fil des messages garde au moins 520 px (en-tête du canal lisible).
        (self.viewport_w - self.settings.sidebar_width - team_bar - 520.).max(THREAD_WIDTH)
    }

    pub(crate) fn toggle_panel_expanded(&mut self, cx: &mut Context<Self>) {
        self.panel_expanded = !self.panel_expanded;
        self.scroller.update(cx, |s, cx| s.remeasure(cx));
        self.thread_scroller.update(cx, |s, cx| s.remeasure(cx));
        cx.notify();
    }

    /// Bouton « Agrandir / Réduire » de l'en-tête du panneau de droite.
    pub(crate) fn expand_button(&self, id: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let (icon, label) = if self.panel_expanded {
            (IconName::Minimize2, t!("panel.collapse"))
        } else {
            (IconName::Maximize2, t!("panel.expand"))
        };
        Button::new(id)
            .icon(Icon::new(icon))
            .small()
            .ghost()
            .accessibility_label(label.clone())
            .tooltip(format!("{label} (Ctrl+Shift+.)"))
            .on_click(cx.listener(|this, _, _, cx| this.toggle_panel_expanded(cx)))
    }
}

/// Exemple dans la langue de l'interface (les mots, pas la syntaxe).
fn localized_example(key: &str, source: &str) -> String {
    let words = |k: &str| {
        let key = format!("markdown_help.sample.{k}");
        t!(key.as_str()).to_string()
    };
    match key {
        "bold" => format!("**{}**", words("bold")),
        "italic" => format!("_{}_", words("italic")),
        "strike" => format!("~~{}~~", words("strike")),
        "heading" => format!("### {}", words("heading")),
        "quote" => format!("> {}", words("quote")),
        "list" => format!("- {}\n- {}", words("item"), words("other")),
        "numbered" => format!("1. {}\n2. {}", words("first"), words("second")),
        "task" => format!("- [x] {}\n- [ ] {}", words("done"), words("todo")),
        _ => source.to_string(),
    }
}
