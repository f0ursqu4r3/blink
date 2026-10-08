//! The request as code: language buttons, copy, and a read-only highlighted
//! snippet. The code section of `RequestWorkspace.vue`, shown with a
//! read-only `CodeEditor.vue`.

use blink_core::codegen::code_language;
use blink_core::model::CodeTarget;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Editor, EditorState};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::store::Store;
use crate::theme;
use crate::ui::widgets::tracked;

/// The user closed the panel from its menu.
pub struct CloseCode;

pub struct CodePanel {
    store: Entity<Store>,
    editor: Entity<EditorState>,
    shown: Option<(CodeTarget, String)>,
}

impl EventEmitter<CloseCode> for CodePanel {}

impl CodePanel {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            let mut editor = EditorState::new(window, cx)
                .language("bash")
                .line_number(false)
                .folding(false)
                .soft_wrap(true);
            editor.set_readonly(true, cx);
            editor
        });
        CodePanel {
            store,
            editor,
            shown: None,
        }
    }

    /// Show `code` in `target`. No-op when it is already shown.
    pub fn show(
        &mut self,
        target: CodeTarget,
        code: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .shown
            .as_ref()
            .is_some_and(|(shown, text)| *shown == target && text == code)
        {
            return;
        }
        let language_changed = self
            .shown
            .as_ref()
            .is_none_or(|(shown, _)| *shown != target);
        let text = code.to_string();
        self.editor.update(cx, |editor, cx| {
            if language_changed {
                editor.set_highlighter(code_language(target), cx);
            }
            editor.set_value(text, window, cx);
        });
        self.shown = Some((target, code.to_string()));
        cx.notify();
    }

    fn target(&self, cx: &App) -> CodeTarget {
        self.store.read(cx).workspace.preferences.code_target
    }

    fn set_target(&mut self, target: CodeTarget, cx: &mut Context<Self>) {
        self.store.update(cx, |store, cx| {
            store.update_workspace(cx, |workspace| {
                let mut next = workspace.preferences.clone();
                next.code_target = target;
                workspace.set_preferences(next);
            })
        });
    }

    fn copy(&mut self, cx: &mut Context<Self>) {
        let code = self
            .shown
            .as_ref()
            .map(|(_, code)| code.clone())
            .unwrap_or_default();
        self.store.update(cx, |store, cx| store.copy(code, cx));
    }
}

impl Render for CodePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let target = self.target(cx);
        let copied = self.store.read(cx).copied;
        // The snippet shows whole; the panel scrolls past 200 px.
        let lines = self
            .shown
            .as_ref()
            .map_or(1, |(_, code)| code.lines().count().max(1));
        let entity = cx.entity().downgrade();
        let targets = CodeTarget::ALL.into_iter().map(|option| {
            let pressed = option == target;
            div()
                .id(SharedString::from(format!(
                    "code-target-{}",
                    option.label()
                )))
                .h(px(24.))
                .px_2()
                .flex()
                .items_center()
                .rounded(px(4.))
                .text_size(px(11.))
                .text_color(if pressed {
                    colors.foreground
                } else {
                    colors.muted_foreground
                })
                .when(pressed, |this| this.bg(colors.accent))
                .hover(|this| this.bg(colors.accent).text_color(colors.foreground))
                .cursor_pointer()
                .child(option.label())
                .on_click(cx.listener(move |this, _, _, cx| this.set_target(option, cx)))
        });
        div()
            .id("code-panel")
            .flex_none()
            .max_h(px(200.))
            .overflow_y_scroll()
            .px(px(14.))
            .pt_2()
            .pb_3()
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.muted)
            .when(
                window.viewport_size().width > px(crate::ui::app::NARROW_WIDTH),
                |this| this.rounded_b(px(7.)),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .flex_wrap()
                            .items_center()
                            .gap(px(2.))
                            .children(targets),
                    )
                    .child(
                        Button::new("copy-code")
                            .ghost()
                            .small()
                            .when(copied, |this| {
                                this.icon(Icon::new(IconName::Check).size(px(13.)))
                            })
                            .label(if copied { "Copied" } else { "Copy redacted" })
                            .on_click(cx.listener(|this, _, _, cx| this.copy(cx))),
                    ),
            )
            .child(
                div()
                    .mt_1()
                    .font_family(theme::MONO)
                    .text_size(px(11.))
                    .line_height(relative(1.8))
                    .child(
                        Editor::new(&self.editor)
                            .h(px(lines as f32 * 17. + 8.))
                            .appearance(false)
                            .bordered(false)
                            .font_family(theme::MONO)
                            .text_size(px(11.)),
                    ),
            )
            .child(
                div()
                    .id("code-credentials")
                    .mt_1()
                    .font_family(theme::MONO)
                    .text_size(px(9.))
                    .text_color(colors.warning)
                    .cursor_default()
                    // `tracking-[0.08em] underline decoration-dotted
                    // underline-offset-3`.
                    .child(tracked("CREDENTIAL HEADERS REDACTED", 0.08).dotted_underline(None))
                    .tooltip(|window, cx| {
                        Tooltip::new(
                            "Authorization and sensitive headers are masked. Credential references stay as placeholders. Other request fields can contain private data.",
                        )
                        .build(window, cx)
                    }),
            )
            .context_menu(move |menu, _, _| {
                let copy = entity.clone();
                let close = entity.clone();
                menu.item(
                    PopupMenuItem::new(format!("Copy {}", target.label())).on_click(
                        move |_, _, cx| {
                            copy.update(cx, |this, cx| this.copy(cx)).ok();
                        },
                    ),
                )
                .separator()
                .item(PopupMenuItem::new("Close").on_click(move |_, _, cx| {
                    close.update(cx, |_, cx| cx.emit(CloseCode)).ok();
                }))
            })
    }
}
