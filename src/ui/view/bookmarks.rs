//! Signets de canal : barre de liens et de fichiers sous l'en-tête,
//! ajout d'un lien, suppression.

use gpui_kit::component::input::InputState;

use super::dialogs::Dialog;
use super::*;
use crate::api::Bookmark;
use crate::backend::Command;

impl ChatApp {
    /// Signets du canal ouvert, chargés une fois par canal.
    pub(crate) fn ensure_bookmarks(&mut self, channel_id: &str) {
        if !self.bookmarks.contains_key(channel_id) && !self.is_archived(channel_id) {
            self.bookmarks.insert(channel_id.to_string(), Vec::new());
            self.send(Command::LoadBookmarks { channel_id: channel_id.to_string() });
        }
    }

    pub(crate) fn on_bookmarks(&mut self, channel_id: String, bookmarks: Vec<Bookmark>) {
        self.bookmarks.insert(channel_id, bookmarks);
    }

    pub(crate) fn open_bookmark_dialog(&mut self, channel_id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = Some(Dialog::Bookmark { channel_id });
        self.dialog_comment.update(cx, |s: &mut InputState, cx| {
            s.set_placeholder(t!("bookmarks.title_placeholder"), window, cx);
            s.set_value("", window, cx);
        });
        self.dialog_input.update(cx, |s: &mut InputState, cx| {
            s.set_placeholder(t!("bookmarks.url_placeholder"), window, cx);
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn save_bookmark(&mut self, cx: &mut Context<Self>) {
        let Some(Dialog::Bookmark { channel_id }) = self.dialog.as_ref() else { return };
        let channel_id = channel_id.clone();
        let mut url = self.dialog_input.read(cx).value().trim().to_string();
        if !url.contains("://") && !url.is_empty() {
            url = format!("https://{url}");
        }
        if !is_safe_link(&url) {
            self.flash(t!("bookmarks.invalid").to_string(), cx);
            return;
        }
        let title = match self.dialog_comment.read(cx).value().trim() {
            "" => url.split("://").nth(1).unwrap_or(&url).trim_end_matches('/').to_string(),
            t => t.to_string(),
        };
        self.send(Command::AddBookmark { channel_id, title, url });
        self.dialog = None;
        cx.notify();
    }

    fn delete_bookmark(&mut self, channel_id: String, bookmark_id: String, cx: &mut Context<Self>) {
        if let Some(list) = self.bookmarks.get_mut(&channel_id) {
            list.retain(|b| b.id != bookmark_id);
        }
        self.send(Command::DeleteBookmark { channel_id, bookmark_id });
        cx.notify();
    }

    /// Ouvre un fichier : image dans la visionneuse, sinon avec l'application par défaut.
    pub(crate) fn open_file(&mut self, file: FileInfo, window: &mut Window, cx: &mut Context<Self>) {
        if file.is_image() {
            self.open_image(file, window, cx);
        } else if file.extension.eq_ignore_ascii_case("pdf") && crate::pdf::SUPPORTED {
            self.open_pdf(file, window, cx);
        } else {
            self.download(file, true, cx);
        }
    }

    fn open_bookmark(&mut self, bookmark: Bookmark, window: &mut Window, cx: &mut Context<Self>) {
        match bookmark.file {
            Some(file) if bookmark.kind == "file" => self.open_file(file, window, cx),
            _ => {
                if !self.open_internal_link(&bookmark.link_url, window, cx) {
                    open_link(&bookmark.link_url, cx);
                }
            }
        }
    }

    /// Barre des signets sous l'en-tête (rien si le canal n'en a pas).
    pub(crate) fn render_bookmarks_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let channel_id = self.channel_id.clone()?;
        let list = self.bookmarks.get(&channel_id).filter(|l| !l.is_empty())?;
        let p = self.palette;
        let entity = cx.entity();
        Some(
            h_flex()
                .id("bookmarks-bar")
                .flex_none()
                .h(px(38.))
                .px_4()
                .gap_1()
                .overflow_x_scroll()
                .border_b_1()
                .border_color(p.border())
                .children(list.iter().map(|b| {
                    let icon: AnyElement = if !b.emoji.is_empty() {
                        self.named_emoji(&b.emoji, 15.)
                    } else if b.kind == "file" {
                        let ext = b.file.as_ref().map(|f| f.extension.clone()).unwrap_or_default();
                        div().text_size(px(14.)).child(file_icon(&ext)).into_any_element()
                    } else {
                        Icon::new(IconName::Link).size(px(14.)).text_color(p.muted()).into_any_element()
                    };
                    let (open, menu_entity) = (b.clone(), entity.clone());
                    let (cid, bid, url) = (channel_id.clone(), b.id.clone(), b.link_url.clone());
                    let tip: SharedString =
                        if b.kind == "file" { b.display_name.clone() } else { b.link_url.clone() }.into();
                    h_flex()
                        .id(SharedString::from(format!("bm-{}", b.id)))
                        .flex_none()
                        .h(px(26.))
                        .px_2()
                        .gap_1p5()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .secondary_size()
                        .hover(|s| s.bg(p.hover()))
                        .child(icon)
                        .child(div().max_w(px(220.)).truncate().child(b.display_name.clone()))
                        .tooltip(move |window, cx| {
                            gpui_kit::component::tooltip::Tooltip::new(tip.clone()).build(window, cx)
                        })
                        .on_click(cx.listener(move |this, _, window, cx| this.open_bookmark(open.clone(), window, cx)))
                        .context_menu(move |menu, _, _| {
                            let (e1, e2) = (menu_entity.clone(), menu_entity.clone());
                            let (cid, bid, url) = (cid.clone(), bid.clone(), url.clone());
                            let mut menu = menu.min_w(px(200.));
                            if !url.is_empty() {
                                menu = menu.item(
                                    PopupMenuItem::new(t!("bookmarks.copy")).icon(Icon::new(IconName::Copy)).on_click(
                                        move |_, _, cx| {
                                            let url = url.clone();
                                            e1.update(cx, |this, cx| this.copy(url, &t!("clipboard.link"), cx))
                                        },
                                    ),
                                );
                            }
                            menu.item(
                                PopupMenuItem::new(t!("bookmarks.delete")).icon(Icon::new(IconName::Trash)).on_click(
                                    move |_, _, cx| {
                                        let (cid, bid) = (cid.clone(), bid.clone());
                                        e2.update(cx, |this, cx| this.delete_bookmark(cid, bid, cx))
                                    },
                                ),
                            )
                        })
                }))
                .child(
                    Button::new("bm-add")
                        .icon(Icon::new(IconName::Plus))
                        .xsmall()
                        .ghost()
                        .accessibility_label(t!("bookmarks.add"))
                        .tooltip(t!("bookmarks.add"))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.open_bookmark_dialog(channel_id.clone(), window, cx)
                        })),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn render_bookmark_dialog(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let label = |text: SharedString| div().caption_size().font_semibold().text_color(p.muted()).child(text);
        let content = v_flex()
            .px_4()
            .pb_4()
            .gap_2()
            .capture_action(cx.listener(|this, _: &Enter, _, cx| {
                this.save_bookmark(cx);
                cx.stop_propagation();
            }))
            .child(label(t!("bookmarks.url").into()))
            .child(Input::new(&self.dialog_input).prefix(Icon::new(IconName::Link).size(px(14.)).text_color(p.muted())))
            .child(div().h(px(4.)))
            .child(label(t!("bookmarks.title").into()))
            .child(Input::new(&self.dialog_comment))
            .into_any_element();
        let footer = h_flex()
            .flex_1()
            .min_w_0()
            .gap_2()
            .child(div().flex_1())
            .child(
                Button::new("bm-cancel")
                    .label(t!("common.cancel"))
                    .small()
                    .ghost()
                    .on_click(cx.listener(|this, _, _, cx| this.close_dialog(cx))),
            )
            .child(
                Button::new("bm-save")
                    .label(t!("common.add"))
                    .small()
                    .primary()
                    .on_click(cx.listener(|this, _, _, cx| this.save_bookmark(cx))),
            )
            .into_any_element();
        self.dialog_shell(IconName::Bookmark, t!("bookmarks.add").into(), 440., content, Some(footer), cx)
    }
}
