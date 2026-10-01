//! The Browser menus. Port of `GroupMenuItems.vue`, `GroupMenuTree.vue`, the
//! request row, UNGROUPED, and blank-space menus of `RequestBrowser.vue`,
//! and the `EnvironmentBadge.vue` menu.

use std::rc::Rc;

use blink_core::environments::active_environment;
use blink_core::model::RequestGroup;
use blink_core::preferences::transport_options;
use blink_core::session_curl::session_curl;
use gpui_kit::assets::IconName;
use gpui_kit::component::Icon;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::actions::{DuplicateRequest, OpenGroupSettings};
use crate::store::Store;
use crate::theme;

use super::rows::{self, AuthMode, count_label};
use super::{Browser, css, update};

type Pick = Rc<dyn Fn(Option<u64>, &mut Window, &mut App)>;

/// A destructive item: `variant="destructive"`.
fn destructive(label: impl Into<SharedString>) -> PopupMenuItem {
    let label = label.into();
    PopupMenuItem::element(move |_, cx| {
        div()
            .text_color(theme::colors(cx).destructive)
            .child(label.clone())
    })
}

/// A move target: the current location is a checked, disabled item.
fn target(
    label: impl Into<SharedString>,
    current: bool,
    id: Option<u64>,
    pick: &Pick,
) -> PopupMenuItem {
    let pick = pick.clone();
    PopupMenuItem::new(label)
        .checked(current)
        .disabled(current)
        .on_click(move |_, window, cx| pick(id, window, cx))
}

/// `GroupMenuTree`: a "Move to" menu that follows the group tree. A group
/// with children opens a submenu; its first item moves into the group itself.
/// `current`: None for no current location, `Some(None)` for the root.
/// `parent`: None for the top level of the menu.
#[allow(clippy::too_many_arguments)]
pub fn group_menu_tree(
    mut menu: PopupMenu,
    groups: Rc<Vec<RequestGroup>>,
    root_label: &'static str,
    current: Option<Option<u64>>,
    exclude: Option<u64>,
    parent: Option<Option<u64>>,
    pick: Pick,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    let children = |parent_id: Option<u64>| -> Vec<RequestGroup> {
        groups
            .iter()
            .filter(|group| group.parent_id == parent_id && Some(group.id) != exclude)
            .cloned()
            .collect()
    };
    if parent.is_none() {
        menu = menu.item(target(root_label, current == Some(None), None, &pick));
        if !children(None).is_empty() {
            menu = menu.separator();
        }
    }
    for group in children(parent.flatten()) {
        let is_current = current == Some(Some(group.id));
        if children(Some(group.id)).is_empty() {
            menu = menu.item(target(
                group.name.clone(),
                is_current,
                Some(group.id),
                &pick,
            ));
            continue;
        }
        let groups = groups.clone();
        let pick = pick.clone();
        let name = group.name.clone();
        let id = group.id;
        menu = menu.submenu(group.name.clone(), window, cx, move |menu, window, cx| {
            let menu = menu
                .item(target(
                    format!("Move into {name}"),
                    is_current,
                    Some(id),
                    &pick,
                ))
                .separator();
            group_menu_tree(
                menu,
                groups.clone(),
                root_label,
                current,
                exclude,
                Some(Some(id)),
                pick.clone(),
                window,
                cx,
            )
        });
    }
    menu
}

/// The request row menu.
pub fn request_menu(
    store: Entity<Store>,
    browser: WeakEntity<Browser>,
    session_id: u64,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |menu, window, cx| {
        let workspace = &store.read(cx).workspace;
        let Some(session) = workspace.session(session_id) else {
            return menu;
        };
        let targets = workspace.menu_targets(session_id);
        let open = workspace.open_ids.contains(&session_id);
        let url = session.draft.url.clone();
        let curl = session_curl(
            session,
            &workspace.groups,
            &workspace.global_definitions,
            &transport_options(&workspace.preferences),
        );
        let busy = targets
            .iter()
            .any(|id| workspace.session(*id).is_some_and(|session| session.busy));
        let mode = rows::shared_auth_mode(workspace, &targets);
        let groups = Rc::new(workspace.groups.clone());
        let current = (targets.len() == 1).then_some(session.group_id);
        let single = targets.len() == 1;
        let count = targets.len();
        let targets = Rc::new(targets);

        let s = store.clone();
        let mut menu = menu
            .item(PopupMenuItem::new("Open").on_click(move |_, _, cx| {
                update(&s, cx, |workspace| workspace.select(session_id));
            }))
            .item({
                let s = store.clone();
                PopupMenuItem::new("Duplicate")
                    .action(Box::new(DuplicateRequest))
                    .on_click(move |_, _, cx| {
                        update(&s, cx, |workspace| {
                            workspace.duplicate(Some(session_id));
                        });
                    })
            })
            .when(open, |menu| {
                let s = store.clone();
                menu.item(PopupMenuItem::new("Close tab").on_click(move |_, _, cx| {
                    update(&s, cx, |workspace| workspace.close(session_id));
                }))
            });
        if single {
            let copy_url = {
                let s = store.clone();
                let url = url.clone();
                PopupMenuItem::new("Copy URL")
                    .disabled(url.is_empty())
                    .on_click(move |_, _, cx| s.update(cx, |store, cx| store.copy(url.clone(), cx)))
            };
            let copy_curl = {
                let s = store.clone();
                let curl = curl.clone();
                PopupMenuItem::new("Copy as cURL")
                    .disabled(curl.is_empty())
                    .on_click(move |_, _, cx| {
                        s.update(cx, |store, cx| store.copy(curl.clone(), cx))
                    })
            };
            menu = menu.separator().item(copy_url).item(copy_curl);
        }
        menu = menu.separator();
        {
            let s = store.clone();
            let targets = targets.clone();
            menu = menu.submenu("Authorization", window, cx, move |menu, _, _| {
                AuthMode::ALL.into_iter().fold(menu, |menu, item| {
                    let s = s.clone();
                    let targets = targets.clone();
                    menu.item(
                        PopupMenuItem::new(item.label())
                            .checked(mode == Some(item))
                            .on_click(move |_, _, cx| {
                                update(&s, cx, |workspace| {
                                    rows::set_auth_mode(workspace, &targets, item)
                                });
                            }),
                    )
                })
            });
        }
        {
            let s = store.clone();
            let targets = targets.clone();
            let pick: Pick = Rc::new(move |group_id, _, cx| {
                update(&s, cx, |workspace| {
                    rows::move_targets(workspace, &targets, group_id)
                });
            });
            menu = menu.submenu(
                count_label("Move", count, " to"),
                window,
                cx,
                move |menu, window, cx| {
                    group_menu_tree(
                        menu,
                        groups.clone(),
                        "Ungrouped",
                        current,
                        None,
                        None,
                        pick.clone(),
                        window,
                        cx,
                    )
                },
            );
        }
        let browser = browser.clone();
        menu.separator().item(
            destructive(count_label("Delete", count, ""))
                .disabled(busy)
                .on_click(move |_, _, cx| {
                    browser
                        .update(cx, |this, cx| this.request_delete(session_id, cx))
                        .ok();
                }),
        )
    }
}

/// `GroupMenuItems`: the group row context menu and its ⋯ dropdown.
pub fn group_menu(
    store: Entity<Store>,
    browser: WeakEntity<Browser>,
    group_id: u64,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |menu, window, cx| {
        let workspace = &store.read(cx).workspace;
        let Some(group) = workspace.group(group_id) else {
            return menu;
        };
        let focused = rows::focused_group(workspace).map(|group| group.id) == Some(group_id);
        let can_move = rows::has_movable_selection(workspace, Some(group_id));
        let collapsed = group.collapsed;
        let parent_id = group.parent_id;
        let groups = Rc::new(workspace.groups.clone());

        let s = store.clone();
        let b = browser.clone();
        let b2 = browser.clone();
        let mut menu = menu
            .item(PopupMenuItem::new("New request").on_click(move |_, _, cx| {
                update(&s, cx, |workspace| {
                    workspace.create(Some(Some(group_id)));
                });
            }))
            .item(
                PopupMenuItem::new("New group").on_click(move |_, window, cx| {
                    b.update(cx, |this, cx| {
                        this.start_creating(Some(group_id), None, window, cx)
                    })
                    .ok();
                }),
            )
            .separator()
            .item(PopupMenuItem::new("Rename").on_click(move |_, window, cx| {
                b2.update(cx, |this, cx| this.start_rename(group_id, window, cx))
                    .ok();
            }))
            .item(
                PopupMenuItem::new("Settings…").on_click(move |_, window, cx| {
                    window.dispatch_action(Box::new(OpenGroupSettings { group_id }), cx);
                }),
            )
            .separator();
        {
            let s = store.clone();
            menu = menu.item(
                PopupMenuItem::new(if focused { "Unfocus" } else { "Focus" }).on_click(
                    move |_, _, cx| {
                        update(&s, cx, |workspace| {
                            if focused {
                                workspace.unfocus(true);
                            } else {
                                workspace.focus_group(group_id);
                            }
                        });
                    },
                ),
            );
        }
        {
            let s = store.clone();
            menu = menu
                .item(
                    PopupMenuItem::new(if collapsed { "Expand" } else { "Collapse" }).on_click(
                        move |_, _, cx| {
                            update(&s, cx, |workspace| workspace.toggle_group(group_id));
                        },
                    ),
                )
                .item({
                    let s = store.clone();
                    PopupMenuItem::new("Collapse all").on_click(move |_, _, cx| {
                        update(&s, cx, |workspace| workspace.collapse_all_groups());
                    })
                })
                .separator();
        }
        if can_move {
            let s = store.clone();
            menu = menu.item(PopupMenuItem::new("Move selection here").on_click(
                move |_, _, cx| {
                    update(&s, cx, |workspace| {
                        rows::move_selection(workspace, Some(group_id))
                    });
                },
            ));
        }
        {
            let s = store.clone();
            let pick: Pick = Rc::new(move |parent, _, cx| {
                update(&s, cx, |workspace| {
                    workspace.move_group(group_id, parent, None)
                });
            });
            menu = menu.submenu("Move to", window, cx, move |menu, window, cx| {
                group_menu_tree(
                    menu,
                    groups.clone(),
                    "Top level",
                    Some(parent_id),
                    Some(group_id),
                    None,
                    pick.clone(),
                    window,
                    cx,
                )
            });
        }
        let browser = browser.clone();
        menu.separator()
            .item(destructive("Delete group").on_click(move |_, _, cx| {
                browser
                    .update(cx, |this, cx| this.ask_delete_group(group_id, cx))
                    .ok();
            }))
    }
}

/// The UNGROUPED row menu.
pub fn ungrouped_menu(
    store: Entity<Store>,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |menu, _, cx| {
        let can_move = rows::has_movable_selection(&store.read(cx).workspace, None);
        let s = store.clone();
        let menu = menu.item(PopupMenuItem::new("New request").on_click(move |_, _, cx| {
            update(&s, cx, |workspace| {
                workspace.create(Some(None));
            });
        }));
        let s = store.clone();
        menu.when(can_move, |menu| {
            menu.item(
                PopupMenuItem::new("Move selection here").on_click(move |_, _, cx| {
                    update(&s, cx, |workspace| rows::move_selection(workspace, None));
                }),
            )
        })
    }
}

/// The blank-space menu of the list.
pub fn blank_menu(
    store: Entity<Store>,
    browser: WeakEntity<Browser>,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |menu, _, cx| {
        // A row menu opened from the same click: show that one only.
        if browser
            .read_with(cx, |this, _| this.row_menu.replace(false))
            .unwrap_or(false)
        {
            return menu;
        }
        let workspace = &store.read(cx).workspace;
        let top = rows::focused_group(workspace).map(|group| group.id);
        let has_groups = !workspace.groups.is_empty();
        let s = store.clone();
        let b = browser.clone();
        menu.item(PopupMenuItem::new("New request").on_click(move |_, _, cx| {
            update(&s, cx, |workspace| {
                workspace.create(Some(top));
            });
        }))
        .item(
            PopupMenuItem::new("New group").on_click(move |_, window, cx| {
                b.update(cx, |this, cx| this.start_creating(top, None, window, cx))
                    .ok();
            }),
        )
        .item({
            let s = store.clone();
            PopupMenuItem::new("Collapse all")
                .disabled(!has_groups)
                .on_click(move |_, _, cx| {
                    update(&s, cx, |workspace| workspace.collapse_all_groups());
                })
        })
    }
}

/// The `EnvironmentBadge` menu: switch the root group's environment.
pub fn environment_menu(
    store: Entity<Store>,
    group_id: u64,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |menu, _, cx| {
        let workspace = &store.read(cx).workspace;
        let Some(group) = workspace.group(group_id) else {
            return menu;
        };
        let active = active_environment(Some(group)).map(|environment| environment.id);
        let mut menu = menu
            .min_w(px(192.))
            .label(format!("{} environment", group.name));
        for environment in group.environments.iter().flatten() {
            let id = environment.id;
            let name: SharedString = environment.name.clone().into();
            let color = environment.color;
            let protected = environment.protected == Some(true);
            let checked = active == Some(id);
            let s = store.clone();
            menu = menu.item(
                PopupMenuItem::element(move |_, cx| {
                    let colors = theme::colors(cx);
                    div()
                        .flex()
                        .flex_1()
                        .items_center()
                        .gap(css(8.))
                        .child(
                            div()
                                .size(css(8.))
                                .flex_shrink_0()
                                .rounded_full()
                                .bg(theme::environment_color(color, cx)),
                        )
                        .child(div().flex_1().font_family(theme::MONO).child(name.clone()))
                        .when(protected, |this| {
                            this.child(
                                div()
                                    .text_size(css(10.))
                                    .text_color(colors.muted_foreground)
                                    .child("protected"),
                            )
                        })
                        .when(checked, |this| {
                            this.child(Icon::new(IconName::Check).size(css(13.)))
                        })
                })
                .on_click(move |_, _, cx| {
                    update(&s, cx, |workspace| {
                        workspace.switch_environment(group_id, Some(id))
                    });
                }),
            );
        }
        let s = store.clone();
        let none = active.is_none();
        menu.item(
            PopupMenuItem::element(move |_, _| {
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .gap(css(8.))
                    .child(div().size(css(8.)).flex_shrink_0())
                    .child(div().flex_1().child("No environment"))
                    .when(none, |this| {
                        this.child(Icon::new(IconName::Check).size(css(13.)))
                    })
            })
            .on_click(move |_, _, cx| {
                update(&s, cx, |workspace| {
                    workspace.switch_environment(group_id, None)
                });
            }),
        )
        .separator()
        .item(
            PopupMenuItem::new("Edit environments…")
                .icon(Icon::new(IconName::Settings))
                .on_click(move |_, window, cx| {
                    window.dispatch_action(Box::new(OpenGroupSettings { group_id }), cx);
                }),
        )
    }
}
