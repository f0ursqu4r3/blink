//! The request Browser sidebar. Port of `RequestBrowser.vue`, `GroupActionsMenu.vue`, `GroupMenuItems.vue`, `GroupMenuTree.vue`, `TreeGuides.vue`, `EnvironmentBadge.vue`, and `DragPreview.vue`.

mod drag;
mod menus;
mod rows;
#[cfg(test)]
mod ui_tests;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use blink_core::authorization::resolve_authorization;
use blink_core::drag_drop::{
    DragPayload, DropBox, DropZone, Point as DropPoint, RootPosition, RowKind, Tree, TreeCommand,
    TreeTarget, hit_zone, resolve_tree_drop, step_requests,
};
use blink_core::environments::active_environment;
use blink_core::groups::{GroupedSession, group_subtree};
use blink_core::model::AuthKind;
use blink_core::session::{LabelTokens, display_method, session_label};
use blink_core::tree_guides::{Elbow, TreeGuide};
use blink_core::workspace_state::{DeleteRequest, Workspace};
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{
    Disableable as _, Icon, Sizable as _, VirtualListScrollHandle, v_virtual_list,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::actions::{CollapseAllGroups, ImportFile, OpenGroupSettings};
use crate::store::Store;
use crate::theme;
use crate::ui::tabs::DraggedRequests;
use crate::ui::widgets::{method_label, tracked};

pub use drag::DragPreview;
use drag::{DraggedGroup, edge_speed};
use rows::TreeRow;

/// A size in CSS pixels at zoom 1. It scales with the root rem size, as the
/// webview zoom scales the Vue layout.
pub(super) fn css(value: f32) -> Rems {
    rems(value / 16.0)
}

/// Browser card width (`w-61`) and its minimum (`min-w-47`).
const WIDTH: f32 = 244.;
const MIN_WIDTH: f32 = 188.;
/// At or below this window width the Browser is an overlay without rounding.
const NARROW_WIDTH: f32 = 760.;

const REQUEST_ROW: f32 = 27.;
const GROUP_ROW: f32 = 28.;
const UNGROUPED_ROW: f32 = 28.;
const FORM_ROW: f32 = 36.;
const ROOT_END: f32 = 32.;
const LIST_PADDING: f32 = 8.;
/// The delete confirmation strip: 9 px text at `leading-[1.45]`, 9 px
/// buttons at the inherited 1.5 line height, `py-1.25`, `gap-1.25`.
const CONFIRM_TEXT: f32 = 9.;
const CONFIRM_LINE: f32 = CONFIRM_TEXT * 1.45;
const CONFIRM_BUTTONS: f32 = CONFIRM_TEXT * 1.5;
const CONFIRM_PADDING: f32 = 5.;
const CONFIRM_GAP: f32 = 5.;
const EXPAND_DELAY: Duration = Duration::from_millis(600);

/// Change the workspace through the store.
pub(super) fn update<R>(
    store: &Entity<Store>,
    cx: &mut App,
    change: impl FnOnce(&mut Workspace) -> R,
) -> R {
    store.update(cx, |store, cx| store.update_workspace(cx, change))
}

/// The text of a delete confirmation strip.
fn confirm_text(workspace: &Workspace, item: &Item) -> Option<String> {
    match item {
        Item::ConfirmDeleteRequests { id, ids, .. } => Some(if ids.len() > 1 {
            format!(
                "Delete {} requests? Their drafts and responses are lost.",
                ids.len()
            )
        } else {
            let label = workspace
                .session(*id)
                .map(|session| label_of(workspace, session))
                .unwrap_or_default();
            format!("Delete {label}? Its draft and response are lost.")
        }),
        Item::ConfirmDeleteGroup { id, .. } => workspace.group(*id).map(|group| {
            format!(
                "Delete {}? Requests move to {}. Child groups are promoted.",
                group.name,
                rows::parent_name(workspace, group.parent_id)
            )
        }),
        _ => None,
    }
}

/// Height of a confirmation strip with `lines` lines of text: `py-1.25`,
/// the `leading-[1.45]` text, a `gap-1.25`, the button row, and `border-b`.
fn confirm_height(lines: usize) -> f32 {
    CONFIRM_PADDING * 2. + lines.max(1) as f32 * CONFIRM_LINE + CONFIRM_GAP + CONFIRM_BUTTONS + 1.
}

fn label_of(workspace: &Workspace, session: &blink_core::model::RequestSession) -> String {
    session_label(
        session,
        Some(LabelTokens {
            groups: &workspace.groups,
            global_definitions: &workspace.global_definitions,
        }),
    )
}

/// Events for the root view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserEvent {
    /// A row opened a request (`select` in `App.vue`): the narrow overlay closes.
    Selected(u64),
}

/// One item of the virtual list.
#[derive(Debug, Clone, PartialEq)]
enum Item {
    Padding,
    Ungrouped,
    /// Index into `tree`.
    Tree(usize),
    /// The new child group form below a group row.
    CreateForm {
        group_id: u64,
        level: usize,
    },
    /// The delete confirmation below the request row `id`, for `ids`.
    ConfirmDeleteRequests {
        id: u64,
        ids: Vec<u64>,
        level: usize,
    },
    /// The delete confirmation below a group row and its child form.
    ConfirmDeleteGroup {
        id: u64,
        level: usize,
    },
    RootEnd,
}

/// A row that takes drops, and its bounds in the last frame.
#[derive(Debug, Clone, Copy)]
struct DropRow {
    target: TreeTarget,
    kind: RowKind,
    bounds: Bounds<Pixels>,
}

/// The drop the pointer targets.
#[derive(Debug, Clone)]
struct Hit {
    key: String,
    zone: DropZone,
    command: TreeCommand,
    /// A collapsed group to open after a hover.
    expand: Option<u64>,
}

pub struct Browser {
    store: Entity<Store>,
    name_input: Entity<InputState>,
    /// The parent of the group form: `Some(None)` for top level.
    creating_parent: Option<Option<u64>>,
    editing_id: Option<u64>,
    /// The group whose delete confirmation is open (`deletingId`).
    deleting_group: Option<u64>,
    /// The request row whose delete confirmation is open, and the requests
    /// it deletes (`deletingRequestId`, `deletingRequestIds`).
    deleting_requests: Option<(u64, Vec<u64>)>,
    /// Width of the list in the last frame, for the confirmation text wrap.
    list_width: Rc<Cell<Pixels>>,
    grouping_selection: Option<Vec<u64>>,
    tree: Vec<TreeRow>,
    guides: Vec<Option<TreeGuide>>,
    items: Vec<Item>,
    scroll_handle: VirtualListScrollHandle,
    drop_rows: Rc<RefCell<Vec<DropRow>>>,
    drag_payload: Option<DragPayload>,
    hit: Option<Hit>,
    expand_task: Option<Task<()>>,
    scroll_speed: f32,
    scrolling: bool,
    focus_handles: HashMap<u64, FocusHandle>,
    /// Set when a row opens its own menu, so the list menu stays closed.
    row_menu: Rc<Cell<bool>>,
    /// The group whose ⋯ menu is open.
    menu_group: Option<u64>,
    /// Focus and scroll to this request row on the next frame.
    pending_focus: Option<u64>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<BrowserEvent> for Browser {}

impl Browser {
    pub fn new(store: Entity<Store>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name_input = cx.new(|cx| InputState::new(window, cx));
        let _subscriptions = vec![
            cx.observe(&store, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &name_input,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.submit_form(window, cx);
                    }
                },
            ),
            // Escape cancels a drag. Before the drag starts, Escape belongs
            // to the page.
            cx.intercept_keystrokes(|event, window, cx| {
                if event.keystroke.key == "escape" && cx.has_active_drag() {
                    cx.stop_active_drag(window);
                    cx.stop_propagation();
                    window.refresh();
                }
            }),
        ];
        Browser {
            store,
            name_input,
            creating_parent: None,
            editing_id: None,
            deleting_group: None,
            deleting_requests: None,
            list_width: Rc::new(Cell::new(Pixels::ZERO)),
            grouping_selection: None,
            tree: Vec::new(),
            guides: Vec::new(),
            items: Vec::new(),
            scroll_handle: VirtualListScrollHandle::new(),
            drop_rows: Rc::default(),
            drag_payload: None,
            hit: None,
            expand_task: None,
            scroll_speed: 0.,
            scrolling: false,
            focus_handles: HashMap::new(),
            row_menu: Rc::default(),
            menu_group: None,
            pending_focus: None,
            _subscriptions,
        }
    }

    /// Scroll to a request row and focus it: the UI half of "Reveal in
    /// Browser", after `Workspace::reveal`.
    #[allow(dead_code)]
    pub fn reveal(&mut self, id: u64, cx: &mut Context<Self>) {
        self.pending_focus = Some(id);
        cx.notify();
    }

    // ── Forms ──────────────────────────────────────────────────────────────

    pub(super) fn start_creating(
        &mut self,
        parent_id: Option<u64>,
        session_ids: Option<Vec<u64>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.creating_parent = Some(parent_id);
        self.editing_id = None;
        self.deleting_group = None;
        self.grouping_selection = session_ids.filter(|ids| !ids.is_empty());
        self.set_draft("", parent_id.is_none(), window, cx);
    }

    pub(super) fn start_rename(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self
            .store
            .read(cx)
            .workspace
            .group(id)
            .map(|group| group.name.clone())
        else {
            return;
        };
        self.editing_id = Some(id);
        self.creating_parent = None;
        self.deleting_group = None;
        self.set_draft(&name, false, window, cx);
    }

    // ── Delete confirmations ───────────────────────────────────────────────

    /// Delete from a request row menu. Several targets, or a draft, ask in
    /// a strip below the row first.
    pub(super) fn request_delete(&mut self, id: u64, cx: &mut Context<Self>) {
        let result = update(&self.store, cx, |workspace| workspace.request_delete(id));
        if let DeleteRequest::Confirm(ids) = result {
            self.deleting_requests = Some((id, ids));
            cx.notify();
        }
    }

    fn confirm_delete_requests(&mut self, cx: &mut Context<Self>) {
        let Some((_, ids)) = self.deleting_requests.take() else {
            return;
        };
        update(&self.store, cx, |workspace| {
            // A target that started sending since the question stays.
            let busy = ids
                .iter()
                .any(|id| workspace.session(*id).is_some_and(|session| session.busy));
            if !busy {
                workspace.remove_requests(&ids);
            }
        });
        cx.notify();
    }

    fn cancel_delete_requests(&mut self, cx: &mut Context<Self>) {
        self.deleting_requests = None;
        cx.notify();
    }

    /// Ask in a strip below the group row. Its requests and child groups
    /// move to its parent.
    pub(super) fn ask_delete_group(&mut self, id: u64, cx: &mut Context<Self>) {
        self.deleting_group = Some(id);
        cx.notify();
    }

    fn confirm_delete_group(&mut self, id: u64, cx: &mut Context<Self>) {
        update(&self.store, cx, |workspace| workspace.delete_group(id));
        self.deleting_group = None;
        cx.notify();
    }

    fn cancel_delete_group(&mut self, cx: &mut Context<Self>) {
        self.deleting_group = None;
        cx.notify();
    }

    fn set_draft(&mut self, value: &str, top: bool, window: &mut Window, cx: &mut Context<Self>) {
        let value = value.to_string();
        self.name_input.update(cx, |input, cx| {
            input.set_placeholder(if top { "Group name" } else { "" }, window, cx);
            input.set_value(value, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn draft_name(&self, cx: &App) -> String {
        self.name_input
            .read(cx)
            .value()
            .trim()
            .chars()
            .take(80)
            .collect()
    }

    fn submit_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.editing_id {
            self.submit_rename(id, window, cx);
        } else {
            self.submit_create(window, cx);
        }
    }

    fn submit_create(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.draft_name(cx);
        let Some(parent_id) = self.creating_parent else {
            return;
        };
        if name.is_empty() {
            return;
        }
        let ids = self.grouping_selection.take().unwrap_or_default();
        update(&self.store, cx, |workspace| {
            workspace.create_group(&name, parent_id, &ids);
        });
        self.creating_parent = None;
        self.clear_draft(window, cx);
    }

    fn submit_rename(&mut self, id: u64, window: &mut Window, cx: &mut Context<Self>) {
        let name = self.draft_name(cx);
        if name.is_empty() {
            return;
        }
        update(&self.store, cx, |workspace| {
            workspace.rename_group(id, &name)
        });
        self.editing_id = None;
        self.clear_draft(window, cx);
    }

    fn cancel_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.creating_parent = None;
        self.editing_id = None;
        self.grouping_selection = None;
        self.clear_draft(window, cx);
    }

    fn clear_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.name_input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    // ── Selection ──────────────────────────────────────────────────────────

    fn select_request(&mut self, id: u64, modifiers: Modifiers, cx: &mut Context<Self>) {
        let visible = rows::visible_request_ids(&self.tree);
        update(&self.store, cx, |workspace| {
            let (ids, anchor) = rows::click_selection(
                &workspace.selected_ids,
                workspace.selection_anchor_id,
                &visible,
                id,
                modifiers.shift,
                modifiers.platform || modifiers.control,
            );
            workspace.select(id);
            workspace.update_selection(ids, anchor);
        });
        cx.emit(BrowserEvent::Selected(id));
    }

    /// A context menu on a row outside the selection selects only that row,
    /// without opening it.
    fn context_select(&mut self, id: u64, cx: &mut Context<Self>) {
        self.row_menu.set(true);
        if self.store.read(cx).workspace.selected_ids.contains(&id) {
            return;
        }
        update(&self.store, cx, |workspace| {
            workspace.update_selection(vec![id], Some(id))
        });
    }

    /// The requests a drag of `id` moves: the selection when it holds the row.
    fn drag_ids(workspace: &Workspace, id: u64) -> Vec<u64> {
        if workspace.selected_ids.contains(&id) {
            workspace.selected_ids.clone()
        } else {
            vec![id]
        }
    }

    /// Alt+ArrowUp/Down moves the row (or the selection) inside its group.
    fn step_selection(&mut self, id: u64, direction: i32, cx: &mut Context<Self>) {
        let moved = update(&self.store, cx, |workspace| {
            let ids = Self::drag_ids(workspace, id);
            let sessions: Vec<GroupedSession> = workspace
                .sessions
                .iter()
                .map(GroupedSession::from)
                .collect();
            let Some(step) = step_requests(&sessions, &ids, direction) else {
                return false;
            };
            if let Some(focus) = rows::focused_group(workspace).map(|group| group.id)
                && step.group_id.is_none_or(|group_id| {
                    !group_subtree(&workspace.groups, focus).contains(&group_id)
                })
            {
                return false;
            }
            workspace.move_requests(&ids, step.group_id, step.before_id);
            true
        });
        if moved {
            self.pending_focus = Some(id);
            cx.notify();
        }
    }

    // ── Drag and drop ──────────────────────────────────────────────────────

    fn drag_moved(
        &mut self,
        payload: DragPayload,
        position: Point<Pixels>,
        list: Bounds<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.drag_payload.as_ref() != Some(&payload) {
            self.drag_payload = Some(payload.clone());
            cx.notify();
        }
        let inside = list.contains(&position);
        let speed = if inside {
            let zoom = window.rem_size().as_f32() / 16.;
            edge_speed(
                list.top().as_f32(),
                list.bottom().as_f32(),
                position.y.as_f32(),
            ) * zoom
        } else {
            0.
        };
        self.set_scroll_speed(speed, cx);
        let hit = if inside {
            self.resolve_drop(&payload, position, cx)
        } else {
            None
        };
        self.set_hit(hit, cx);
    }

    /// `resolveBrowserDrop`: the keyed row under the pointer. Below the last
    /// row counts as that row (root-end).
    fn resolve_drop(
        &self,
        payload: &DragPayload,
        position: Point<Pixels>,
        cx: &App,
    ) -> Option<Hit> {
        let rows = self.drop_rows.borrow();
        let row = rows
            .iter()
            .find(|row| position.y >= row.bounds.top() && position.y < row.bounds.bottom())
            .or_else(|| rows.last().filter(|row| position.y >= row.bounds.top()))?;
        let workspace = &self.store.read(cx).workspace;
        let focus = rows::focused_group(workspace).map(|group| group.id);
        // In focus, nothing can drop outside the focused group.
        let mut target = row.target;
        if let (Some(focus), TreeTarget::Root(_)) = (focus, target) {
            target = TreeTarget::Group(focus);
        }
        let zone = match target {
            TreeTarget::Root(_) => DropZone::Into,
            TreeTarget::Group(id) if Some(id) == focus => DropZone::Into,
            _ => hit_zone(
                DropBox {
                    left: row.bounds.left().as_f32() as f64,
                    top: row.bounds.top().as_f32() as f64,
                    width: row.bounds.size.width.as_f32() as f64,
                    height: row.bounds.size.height.as_f32() as f64,
                },
                DropPoint {
                    x: position.x.as_f32() as f64,
                    y: position.y.as_f32() as f64,
                },
                if matches!(target, TreeTarget::Group(_)) {
                    RowKind::Group
                } else {
                    row.kind
                },
            ),
        };
        let tree = Tree {
            sessions: workspace
                .sessions
                .iter()
                .map(GroupedSession::from)
                .collect(),
            groups: workspace.groups.clone(),
        };
        let drop = resolve_tree_drop(payload, target, zone, &tree)?;
        let expand = match target {
            TreeTarget::Group(id) => workspace
                .group(id)
                .filter(|group| !rows::is_open(group, focus))
                .map(|group| group.id),
            _ => None,
        };
        Some(Hit {
            key: drop.key,
            zone: drop.zone,
            command: drop.command,
            expand,
        })
    }

    fn set_hit(&mut self, hit: Option<Hit>, cx: &mut Context<Self>) {
        let same = match (&self.hit, &hit) {
            (Some(a), Some(b)) => a.key == b.key && a.zone == b.zone,
            (None, None) => true,
            _ => false,
        };
        if !same {
            self.expand_task = None;
            if let Some(group_id) = hit
                .as_ref()
                .filter(|hit| hit.zone == DropZone::Into)
                .and_then(|hit| hit.expand)
            {
                let store = self.store.clone();
                self.expand_task = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(EXPAND_DELAY).await;
                    this.update(cx, |this, cx| {
                        if this.drag_payload.is_some() {
                            update(&store, cx, |workspace| workspace.toggle_group(group_id));
                        }
                    })
                    .ok();
                }));
            }
            cx.notify();
        }
        self.hit = hit;
    }

    fn set_scroll_speed(&mut self, speed: f32, cx: &mut Context<Self>) {
        self.scroll_speed = speed;
        if speed == 0. || self.scrolling {
            return;
        }
        self.scrolling = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let go = this
                    .update(cx, |this, cx| {
                        if this.scroll_speed == 0. || !cx.has_active_drag() {
                            this.scrolling = false;
                            return false;
                        }
                        let offset = this.scroll_handle.offset();
                        let max = this.scroll_handle.max_offset();
                        let y = (offset.y - px(this.scroll_speed)).clamp(-max.y, px(0.));
                        if y != offset.y {
                            this.scroll_handle.set_offset(point(offset.x, y));
                            cx.notify();
                        }
                        true
                    })
                    .unwrap_or(false);
                if !go {
                    break;
                }
            }
        })
        .detach();
    }

    fn drop_commit(&mut self, cx: &mut Context<Self>) {
        let hit = self.hit.take();
        self.end_drag();
        if let Some(hit) = hit {
            update(&self.store, cx, |workspace| match hit.command {
                TreeCommand::MoveRequests {
                    ids,
                    group_id,
                    before_id,
                } => workspace.move_requests(&ids, group_id, before_id),
                TreeCommand::MoveGroup {
                    group_id,
                    parent_id,
                    before_group_id,
                } => workspace.move_group(group_id, parent_id, before_group_id),
            });
        }
        cx.notify();
    }

    fn end_drag(&mut self) {
        self.drag_payload = None;
        self.hit = None;
        self.expand_task = None;
        self.scroll_speed = 0.;
    }

    fn zone_for(&self, key: &str) -> Option<DropZone> {
        self.hit
            .as_ref()
            .filter(|hit| hit.key == key)
            .map(|hit| hit.zone)
    }

    // ── Rows ───────────────────────────────────────────────────────────────

    fn build_items(&mut self, cx: &App) {
        let workspace = &self.store.read(cx).workspace;
        self.tree = rows::tree_rows(workspace);
        self.guides = rows::row_guides(&self.tree);
        let focus = rows::focused_group(workspace).is_some();
        let mut items = vec![Item::Padding];
        if !focus {
            items.push(Item::Ungrouped);
        }
        for (index, row) in self.tree.iter().enumerate() {
            items.push(Item::Tree(index));
            match row {
                TreeRow::Request { id, level } => {
                    if let Some((row_id, ids)) = &self.deleting_requests
                        && row_id == id
                    {
                        items.push(Item::ConfirmDeleteRequests {
                            id: *id,
                            ids: ids.clone(),
                            level: *level,
                        });
                    }
                }
                TreeRow::Group { id, level } => {
                    if self.creating_parent == Some(Some(*id)) {
                        items.push(Item::CreateForm {
                            group_id: *id,
                            level: *level,
                        });
                    }
                    if self.deleting_group == Some(*id) {
                        items.push(Item::ConfirmDeleteGroup {
                            id: *id,
                            level: *level,
                        });
                    }
                }
            }
        }
        items.push(Item::RootEnd);
        items.push(Item::Padding);
        self.items = items;
        // Forget rows that are gone.
        if self.focus_handles.len() > workspace.sessions.len() * 2 {
            self.focus_handles
                .retain(|id, _| workspace.session(*id).is_some());
        }
    }

    /// Height in CSS pixels. A confirmation strip grows with its wrapped text.
    fn item_height(&self, item: &Item, window: &Window, cx: &App) -> f32 {
        match item {
            Item::Padding => LIST_PADDING,
            Item::Ungrouped => UNGROUPED_ROW,
            Item::Tree(index) => match self.tree[*index] {
                TreeRow::Request { .. } => REQUEST_ROW,
                TreeRow::Group { .. } => GROUP_ROW,
            },
            Item::CreateForm { .. } => FORM_ROW,
            Item::ConfirmDeleteRequests { level, .. } | Item::ConfirmDeleteGroup { level, .. } => {
                let text = confirm_text(&self.store.read(cx).workspace, item).unwrap_or_default();
                confirm_height(self.confirm_lines(&text, *level, window))
            }
            Item::RootEnd => ROOT_END,
        }
    }

    /// Lines of the confirmation text at the strip's text width, as the
    /// rendered text wraps it.
    fn confirm_lines(&self, text: &str, level: usize, window: &Window) -> usize {
        let zoom = window.rem_size().as_f32() / 16.;
        let mut width = self.list_width.get();
        if width <= Pixels::ZERO {
            // Before the first frame: the card width inside its border.
            width = px((WIDTH - 2.) * zoom);
        }
        let wrap = width - px((rows::indent(level + 1) + 9.) * zoom);
        if text.is_empty() || wrap <= Pixels::ZERO {
            return 1;
        }
        let run = TextRun {
            len: text.len(),
            font: font(theme::MONO),
            color: Hsla::default(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        window
            .text_system()
            .shape_text(
                SharedString::from(text.to_string()),
                px(CONFIRM_TEXT * zoom),
                &[run],
                Some(wrap),
                None,
            )
            .map_or(1, |lines| {
                lines
                    .iter()
                    .map(|line| line.wrap_boundaries().len() + 1)
                    .sum()
            })
    }

    fn render_items(
        &mut self,
        range: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let items: Vec<Item> = range
            .filter_map(|index| self.items.get(index).cloned())
            .collect();
        items
            .into_iter()
            .map(|item| match item {
                Item::Padding => div().h(css(LIST_PADDING)).into_any_element(),
                Item::Ungrouped => self.render_ungrouped(cx),
                Item::Tree(index) => match self.tree[index].clone() {
                    TreeRow::Request { id, level } => {
                        self.render_request(index, id, level, window, cx)
                    }
                    TreeRow::Group { id, level } => self.render_group(index, id, level, window, cx),
                },
                Item::CreateForm { group_id, level } => self.render_child_form(group_id, level, cx),
                Item::ConfirmDeleteRequests { .. } | Item::ConfirmDeleteGroup { .. } => {
                    self.render_confirm(&item, window, cx)
                }
                Item::RootEnd => div()
                    .h(css(ROOT_END))
                    .child(self.drop_recorder(TreeTarget::Root(RootPosition::End), RowKind::Group))
                    .into_any_element(),
            })
            .collect()
    }

    /// Records a row's bounds for the drop hit test, as `data-drop-key`.
    fn drop_recorder(&self, target: TreeTarget, kind: RowKind) -> impl IntoElement {
        let rows = self.drop_rows.clone();
        canvas(
            move |bounds, _, _| {
                rows.borrow_mut().push(DropRow {
                    target,
                    kind,
                    bounds,
                })
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    /// Records the list width. A change re-renders an open confirmation
    /// strip at its new text wrap.
    fn width_recorder(&self, cx: &Context<Self>) -> impl IntoElement {
        let width = self.list_width.clone();
        let confirming = self.deleting_group.is_some() || self.deleting_requests.is_some();
        let view = cx.entity().downgrade();
        canvas(
            move |bounds, _, cx| {
                if width.replace(bounds.size.width) != bounds.size.width && confirming {
                    view.update(cx, |_, cx| cx.notify()).ok();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0()
    }

    fn render_ungrouped(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = theme::colors(cx);
        let workspace = &self.store.read(cx).workspace;
        let what = if workspace.selected_ids.is_empty() {
            "active request"
        } else {
            "selected requests"
        };
        let disabled = rows::selection_already_in(workspace, None);
        let into = self.zone_for("root") == Some(DropZone::Into);
        let store = self.store.clone();
        let row_menu = self.row_menu.clone();
        div()
            .id("browser-ungrouped")
            .relative()
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap(css(6.))
            .h(css(UNGROUPED_ROW))
            .pl(css(12.))
            .pr(css(9.))
            .when(into, |this| this.bg(colors.accent))
            .child(self.drop_recorder(TreeTarget::Root(RootPosition::Start), RowKind::Group))
            .when(into, |this| this.child(into_ring(colors.primary)))
            .child(
                div()
                    .flex_1()
                    .text_color(colors.muted_foreground)
                    .font_family(theme::MONO)
                    .text_size(css(9.))
                    .child(tracked("UNGROUPED", 0.12)),
            )
            .child(
                small_button("browser-ungrouped-move", IconName::MoveRight, 13., cx)
                    .tooltip(format!("Move {what} here"))
                    .accessibility_label(format!("Move {what} to Ungrouped"))
                    .disabled(disabled)
                    .when(disabled, |this| this.opacity(0.3))
                    .on_click({
                        let store = self.store.clone();
                        move |_, _, cx| {
                            update(&store, cx, |workspace| {
                                rows::move_selection(workspace, None)
                            })
                        }
                    }),
            )
            .on_mouse_down(MouseButton::Right, move |_, _, _| row_menu.set(true))
            .context_menu(menus::ungrouped_menu(store))
            .into_any_element()
    }

    fn render_request(
        &mut self,
        index: usize,
        id: u64,
        level: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let handle = self
            .focus_handles
            .entry(id)
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let colors = theme::colors(cx);
        let workspace = &self.store.read(cx).workspace;
        let Some(session) = workspace.session(id) else {
            return div().into_any_element();
        };
        let label = label_of(workspace, session);
        let method = display_method(session).to_string();
        let active = workspace.shown_active_id() == Some(id);
        let selected = workspace.selected_ids.contains(&id);
        let key = format!("request-{id}");
        let dragged =
            matches!(&self.drag_payload, Some(DragPayload::Requests(ids)) if ids.contains(&id));
        let inside = self.inside_hit(&key);
        let zone = self.zone_for(&key);
        let locked = resolve_authorization(
            session.draft.local_auth.as_ref(),
            session.group_id,
            &workspace.groups,
        )
        .kind()
            != AuthKind::None;
        let drag_ids = Self::drag_ids(workspace, id);
        let preview = if drag_ids.len() > 1 {
            (format!("{} requests", drag_ids.len()), None)
        } else {
            (label.clone(), Some(method.clone()))
        };
        let guide = self.guides[index].clone();
        let store = self.store.clone();
        let this = cx.entity().downgrade();
        div()
            .id(("browser-request", id))
            .test_support()
            .relative()
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap(css(6.))
            .h(css(REQUEST_ROW))
            .pl(css(rows::indent(level)))
            .pr(css(9.))
            .overflow_hidden()
            .text_color(colors.muted_foreground)
            .font_family(theme::MONO)
            .text_size(css(10.))
            .track_focus(&handle)
            .tab_index(0)
            .when(inside, |this| this.bg(colors.accent.opacity(0.4)))
            .when(active, |this| {
                this.bg(colors.accent).text_color(colors.foreground)
            })
            .hover(|style| style.bg(colors.accent).text_color(colors.foreground))
            .focus_visible(|style| style.bg(colors.foreground.opacity(0.1)))
            .when(dragged, |this| this.opacity(0.4))
            .child(self.drop_recorder(TreeTarget::Request(id), RowKind::Request))
            .when(selected, |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .bg(colors.primary),
                )
            })
            .children(drop_line(zone, level, colors.primary))
            .children(guide.map(|guide| {
                tree_guides(
                    &guide,
                    level,
                    rows::indent(level) - 2.,
                    colors.muted_foreground,
                )
            }))
            .child(
                method_label(&method, 8., cx)
                    .w(css(34.))
                    .flex_shrink_0()
                    .text_size(css(8.)),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::MEDIUM)
                    .child(label),
            )
            .when(locked, |this| {
                this.child(
                    Icon::new(IconName::Lock)
                        .size(css(10.))
                        .flex_shrink_0()
                        .text_color(colors.muted_foreground),
                )
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                this.select_request(id, event.modifiers(), cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, _, _, cx| this.context_select(id, cx)),
            )
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                let keystroke = &event.keystroke;
                if !keystroke.modifiers.alt {
                    return;
                }
                let direction = match keystroke.key.as_str() {
                    "up" => -1,
                    "down" => 1,
                    _ => return,
                };
                cx.stop_propagation();
                this.step_selection(id, direction, cx);
            }))
            .on_drag(
                DraggedRequests { ids: drag_ids },
                move |_, offset, _, cx| {
                    // A drag of a row outside the selection selects it.
                    update(&store, cx, |workspace| {
                        if !workspace.selected_ids.contains(&id) {
                            workspace.update_selection(vec![id], Some(id));
                        }
                    });
                    this.update(cx, |this, cx| {
                        this.drag_payload = Some(DragPayload::Requests(Self::drag_ids(
                            &this.store.read(cx).workspace,
                            id,
                        )));
                        cx.notify();
                    })
                    .ok();
                    let (label, method) = preview.clone();
                    cx.new(|_| DragPreview {
                        label,
                        method,
                        folder: false,
                        offset,
                    })
                },
            )
            .context_menu(menus::request_menu(
                self.store.clone(),
                cx.entity().downgrade(),
                id,
            ))
            .into_any_element()
    }

    fn inside_hit(&self, key: &str) -> bool {
        let Some(hit) = self
            .hit
            .as_ref()
            .filter(|hit| hit.zone == DropZone::Into && hit.key.starts_with("group-"))
        else {
            return false;
        };
        rows::rows_inside(&self.tree, &hit.key)
            .iter()
            .any(|inner| inner == key)
    }

    fn render_group(
        &mut self,
        index: usize,
        id: u64,
        level: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = theme::colors(cx);
        let workspace = &self.store.read(cx).workspace;
        let Some(group) = workspace.group(id) else {
            return div().into_any_element();
        };
        let focus = rows::focused_group(workspace).map(|group| group.id);
        let focused = focus == Some(id);
        let open = rows::is_open(group, focus);
        let name: SharedString = group.name.clone().into();
        let key = format!("group-{id}");
        let dragged = self.drag_payload == Some(DragPayload::Group(id));
        let inside = self.inside_hit(&key);
        let zone = self.zone_for(&key);
        let editing = self.editing_id == Some(id);
        let locked = resolve_authorization(
            group.local_auth.as_ref(),
            group.parent_id,
            &workspace.groups,
        )
        .kind()
            != AuthKind::None;
        let environment = (group.parent_id.is_none()
            && group
                .environments
                .as_ref()
                .is_some_and(|list| !list.is_empty()))
        .then(|| {
            active_environment(Some(group))
                .map(|environment| (environment.name.clone(), environment.color))
        });
        let guide = self.guides[index].clone();
        let store = self.store.clone();
        let browser = cx.entity().downgrade();
        let row_menu = self.row_menu.clone();
        let group_name = format!("browser-group-{id}");
        let hover_group: SharedString = group_name.clone().into();

        let toggle = {
            let store = self.store.clone();
            Button::new(("browser-group-toggle", id))
                .ghost()
                .child(
                    Icon::new(if open {
                        IconName::FolderOpen
                    } else {
                        IconName::Folder
                    })
                    .size(css(14.)),
                )
                .size(css(22.))
                .p_0()
                .rounded(px(2.))
                .text_color(colors.muted_foreground)
                .accessibility_label(format!(
                    "{} {}",
                    if open { "Collapse" } else { "Expand" },
                    group.name
                ))
                .when(focused, |this| this.disabled(true).opacity(1.))
                .on_click(move |_, _, cx| {
                    update(&store, cx, |workspace| workspace.toggle_group(id))
                })
        };

        let title = if editing {
            let browser = browser.clone();
            let browser2 = browser.clone();
            div()
                .flex()
                .min_w_0()
                .flex_1()
                .items_center()
                .gap(css(5.))
                .child(self.render_input())
                .child(
                    text_button(("browser-rename-save", id), "Save", cx).on_click(
                        move |_, window, cx| {
                            browser
                                .update(cx, |this, cx| this.submit_rename(id, window, cx))
                                .ok();
                        },
                    ),
                )
                .child(
                    text_button(("browser-rename-cancel", id), "Cancel", cx).on_click(
                        move |_, window, cx| {
                            browser2
                                .update(cx, |this, cx| {
                                    this.editing_id = None;
                                    this.clear_draft(window, cx);
                                })
                                .ok();
                        },
                    ),
                )
                .into_any_element()
        } else {
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .items_center()
                .gap(css(4.))
                .overflow_hidden()
                .text_color(colors.foreground)
                .font_family(theme::MONO)
                .text_size(css(11.))
                .child(div().min_w_0().truncate().child(name.clone()))
                .when(locked, |this| {
                    this.child(
                        Icon::new(IconName::Lock)
                            .size(css(10.))
                            .flex_shrink_0()
                            .text_color(colors.muted_foreground),
                    )
                })
                .when_some(environment, |this, active| {
                    this.child(self.environment_badge(id, &group.name, active, cx))
                })
                .into_any_element()
        };

        let actions = (!editing).then(|| {
            let new_store = self.store.clone();
            let menu_open = self.menu_group == Some(id);
            let open_browser = browser.clone();
            div()
                .flex()
                .when(!menu_open, |this| {
                    this.opacity(0.)
                        .group_hover(hover_group.clone(), |style| style.opacity(1.))
                })
                .child(
                    small_button(("browser-group-settings", id), IconName::Settings, 12., cx)
                        .tooltip(format!("Group settings for {}", group.name))
                        .on_click(move |_, window, cx| {
                            window.dispatch_action(Box::new(OpenGroupSettings { group_id: id }), cx)
                        }),
                )
                .child(
                    small_button(("browser-group-new", id), IconName::Plus, 12., cx)
                        .tooltip(format!("New request in {}", group.name))
                        .on_click(move |_, _, cx| {
                            update(&new_store, cx, |workspace| {
                                workspace.create(Some(Some(id)));
                            });
                        }),
                )
                .child(
                    small_button(("browser-group-more", id), IconName::Ellipsis, 14., cx)
                        .accessibility_label(format!("More actions for {}", group.name))
                        .dropdown_menu_with_anchor(
                            Anchor::TopRight,
                            menus::group_menu(self.store.clone(), browser.clone(), id),
                        )
                        // Keep the actions shown while their menu is open.
                        .on_open_change(move |open, _, cx| {
                            open_browser
                                .update(cx, |this, cx| {
                                    this.menu_group = open.then_some(id);
                                    cx.notify();
                                })
                                .ok();
                        }),
                )
        });

        let preview_name = group.name.clone();
        let can_drag = !editing && !focused;
        div()
            .id(("browser-group", id))
            .test_support()
            .group(group_name)
            .relative()
            .flex()
            .w_full()
            .min_w_0()
            .items_center()
            .gap(css(6.))
            .h(css(GROUP_ROW))
            .pl(css(rows::indent(level)))
            .pr(css(7.))
            .text_color(colors.muted_foreground)
            .when(inside, |this| this.bg(colors.accent.opacity(0.4)))
            .when(zone == Some(DropZone::Into), |this| {
                this.bg(colors.accent).child(into_ring(colors.primary))
            })
            .when(dragged, |this| this.opacity(0.4))
            .child(self.drop_recorder(TreeTarget::Group(id), RowKind::Group))
            .children(drop_line(zone, level, colors.primary))
            .children(guide.map(|guide| {
                tree_guides(
                    &guide,
                    level,
                    rows::indent(level) - 1.,
                    colors.muted_foreground,
                )
            }))
            .child(toggle)
            .child(title)
            .children(actions)
            .on_mouse_down(MouseButton::Right, move |_, _, _| row_menu.set(true))
            .when(can_drag, |this| {
                this.on_drag(DraggedGroup { id }, move |_, offset, _, cx| {
                    browser
                        .update(cx, |this, cx| {
                            this.drag_payload = Some(DragPayload::Group(id));
                            cx.notify();
                        })
                        .ok();
                    cx.new(|_| DragPreview {
                        label: preview_name.clone(),
                        method: None,
                        folder: true,
                        offset,
                    })
                })
            })
            .context_menu(menus::group_menu(store, cx.entity().downgrade(), id))
            .into_any_element()
    }

    /// `EnvironmentBadge`: the active environment of a root group.
    fn environment_badge(
        &self,
        group_id: u64,
        group_name: &str,
        active: Option<(String, blink_core::model::EnvironmentColor)>,
        cx: &App,
    ) -> impl IntoElement {
        let colors = theme::colors(cx);
        let name = active.as_ref().map(|(name, _)| name.clone());
        let color = active
            .as_ref()
            .map_or(colors.muted_foreground, |(_, color)| {
                theme::environment_color(*color, cx)
            });
        let shown = name.clone().unwrap_or_else(|| "none".into());
        div()
            .ml(css(4.))
            .flex_shrink_0()
            // The switch menu is sans like every menu; the badge text sets mono.
            .font_family(theme::SANS)
            // No drag from the badge.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Button::new(("browser-environment", group_id))
                    .ghost()
                    .child(
                        div()
                            .font_family(theme::MONO)
                            .text_size(css(9.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(color)
                            .child(tracked(name.unwrap_or_else(|| "NO ENV".into()), 0.06)),
                    )
                    .h(css(14.))
                    .px(css(4.))
                    .rounded(px(2.))
                    .accessibility_label(format!(
                        "Environment for {group_name}: {shown}. Switch environment"
                    ))
                    .tooltip(format!("Environment: {shown}"))
                    .dropdown_menu_with_anchor(
                        Anchor::TopLeft,
                        menus::environment_menu(self.store.clone(), group_id),
                    ),
            )
    }

    fn render_input(&self) -> impl IntoElement {
        Input::new(&self.name_input)
            .xsmall()
            .h(css(25.))
            .flex_1()
            .min_w_0()
            .font_family(theme::MONO)
            .text_size(css(10.))
    }

    /// The top-level group form and the child group form.
    fn render_form_body(&self, label: &str, cx: &mut Context<Self>) -> Div {
        let colors = theme::colors(cx);
        div()
            .flex()
            .items_center()
            .gap(css(5.))
            .py(css(5.))
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.secondary)
            .child(self.render_input())
            .child(
                text_button(
                    SharedString::from(format!("browser-form-add-{label}")),
                    "Add",
                    cx,
                )
                .accessibility_label(label.to_string())
                .on_click(cx.listener(|this, _, window, cx| this.submit_create(window, cx))),
            )
            .child(
                text_button(
                    SharedString::from(format!("browser-form-cancel-{label}")),
                    "Cancel",
                    cx,
                )
                .accessibility_label("Cancel group creation")
                .on_click(cx.listener(|this, _, window, cx| this.cancel_form(window, cx))),
            )
    }

    fn render_child_form(&self, group_id: u64, level: usize, cx: &mut Context<Self>) -> AnyElement {
        let name = self
            .store
            .read(cx)
            .workspace
            .group(group_id)
            .map(|group| group.name.clone())
            .unwrap_or_default();
        self.render_form_body(&format!("Create group in {name}"), cx)
            .w_full()
            .h(css(FORM_ROW))
            .pl(css(rows::indent(level + 1)))
            .pr(css(9.))
            .into_any_element()
    }

    /// The delete confirmation strip below a request or group row.
    fn render_confirm(&self, item: &Item, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let colors = theme::colors(cx);
        let workspace = &self.store.read(cx).workspace;
        let text = confirm_text(workspace, item).unwrap_or_default();
        let height = self.item_height(item, window, cx);
        let (level, key, busy) = match item {
            Item::ConfirmDeleteRequests { id, ids, level } => (
                *level,
                ("browser-confirm-delete-request", *id),
                ids.iter()
                    .any(|id| workspace.session(*id).is_some_and(|session| session.busy)),
            ),
            Item::ConfirmDeleteGroup { id, level } => {
                (*level, ("browser-confirm-delete-group", *id), false)
            }
            _ => return div().into_any_element(),
        };
        let request = matches!(item, Item::ConfirmDeleteRequests { .. });
        let group_id = match item {
            Item::ConfirmDeleteGroup { id, .. } => Some(*id),
            _ => None,
        };
        let delete = text_button("browser-confirm-delete", "Delete", cx)
            .h(css(CONFIRM_BUTTONS))
            .disabled(busy)
            .on_click(cx.listener(move |this, _, _, cx| match group_id {
                Some(id) => this.confirm_delete_group(id, cx),
                None => this.confirm_delete_requests(cx),
            }));
        let cancel = text_button("browser-confirm-cancel", "Cancel", cx)
            .h(css(CONFIRM_BUTTONS))
            .on_click(cx.listener(move |this, _, _, cx| {
                if request {
                    this.cancel_delete_requests(cx);
                } else {
                    this.cancel_delete_group(cx);
                }
            }));
        div()
            .id(key)
            .test_support()
            .when(request, |this| this.aria_label("Confirm delete request"))
            .flex()
            .flex_col()
            .items_start()
            .gap(css(CONFIRM_GAP))
            .w_full()
            .h(css(height))
            .py(css(CONFIRM_PADDING))
            .pl(css(rows::indent(level + 1)))
            .pr(css(9.))
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.secondary)
            .font_family(theme::MONO)
            .text_size(css(CONFIRM_TEXT))
            .text_color(colors.muted_foreground)
            .child(div().w_full().line_height(css(CONFIRM_LINE)).child(text))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(css(CONFIRM_GAP))
                    .child(delete)
                    .child(cancel),
            )
            .into_any_element()
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let workspace = &self.store.read(cx).workspace;
        let focus = rows::focused_group(workspace);
        let top = focus.map(|group| group.id);
        let group_label = match focus {
            Some(group) => format!("Add group inside {}", group.name),
            None => "Add top-level group".to_string(),
        };
        let selected = workspace.selected_ids.clone();
        let active_group = workspace.active().and_then(|session| session.group_id);
        let has_groups = !workspace.groups.is_empty();
        let store = self.store.clone();
        div()
            .flex()
            .h(css(36.))
            .flex_shrink_0()
            .items_center()
            .gap(css(2.))
            .border_b_1()
            .border_color(colors.border)
            .pl(css(12.))
            .pr(css(6.))
            .child(
                div()
                    .mr_auto()
                    .font_family(theme::MONO)
                    .text_size(css(11.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(colors.foreground)
                    .child(tracked("REQUESTS", 0.08)),
            )
            .child(
                header_button("browser-new-request", IconName::FilePlus, cx)
                    .tooltip(format!(
                        "Add request · {}",
                        blink_core::shortcut::shortcut_label(
                            &["mod", "t"],
                            blink_core::shortcut::IS_MAC
                        )
                    ))
                    .accessibility_label("Add request")
                    .on_click(move |_, _, cx| {
                        update(&store, cx, |workspace| {
                            workspace.create(Some(active_group));
                        });
                    }),
            )
            .child(
                header_button("browser-new-group", IconName::FolderPlus, cx)
                    .tooltip("Add group")
                    .accessibility_label(group_label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.start_creating(top, None, window, cx)
                    })),
            )
            .when(!selected.is_empty(), |this| {
                this.child(
                    header_button("browser-group-selection", IconName::FolderInput, cx)
                        .tooltip("Group selected requests")
                        .accessibility_label("Group selected requests")
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.start_creating(top, Some(selected.clone()), window, cx)
                        })),
                )
            })
            .child(
                header_button("browser-collapse-all", IconName::ListCollapse, cx)
                    .tooltip("Collapse all groups")
                    .accessibility_label("Collapse all groups")
                    .disabled(!has_groups)
                    .when(!has_groups, |this| this.opacity(0.3))
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(CollapseAllGroups), cx)
                    }),
            )
            .child(
                header_button("browser-import", IconName::Import, cx)
                    .tooltip("Import OpenAPI, Postman, or .http file…")
                    .accessibility_label("Import requests")
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(ImportFile), cx)),
            )
    }

    fn render_focus_bar(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let colors = theme::colors(cx);
        let workspace = &self.store.read(cx).workspace;
        let group = rows::focused_group(workspace)?;
        let name: SharedString = group.name.clone().into();
        let store = self.store.clone();
        Some(
            div()
                .flex()
                .h(css(28.))
                .flex_shrink_0()
                .items_center()
                .gap(css(6.))
                .border_b_1()
                .border_color(colors.border)
                .pl(css(12.))
                .pr(css(6.))
                .font_family(theme::MONO)
                .text_size(css(10.))
                .text_color(colors.muted_foreground)
                .child(div().flex_shrink_0().child(tracked("FOCUSED", 0.08)))
                .child(
                    div()
                        .id("browser-focus-name")
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .text_color(colors.foreground)
                        .child(name.clone())
                        .tooltip(move |window, cx| Tooltip::new(name.clone()).build(window, cx)),
                )
                .child(
                    header_button("browser-unfocus", IconName::X, cx)
                        .tooltip("Unfocus · Esc")
                        .accessibility_label(format!("Unfocus {}", group.name))
                        .on_click(move |_, _, cx| {
                            update(&store, cx, |workspace| workspace.unfocus(true))
                        }),
                ),
        )
    }

    fn apply_pending_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.pending_focus.take() else {
            return;
        };
        let Some(index) = self.items.iter().position(|item| {
            matches!(item, Item::Tree(index) if matches!(self.tree[*index], TreeRow::Request { id: row, .. } if row == id))
        }) else {
            return;
        };
        self.scroll_handle
            .scroll_to_item(index, ScrollStrategy::Nearest);
        let handle = self
            .focus_handles
            .entry(id)
            .or_insert_with(|| cx.focus_handle())
            .clone();
        window.focus(&handle, cx);
    }
}

impl Render for Browser {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A drag that ended elsewhere, or was cancelled.
        if !cx.has_active_drag() && (self.drag_payload.is_some() || self.hit.is_some()) {
            self.end_drag();
        }
        self.drop_rows.borrow_mut().clear();
        self.build_items(cx);
        self.apply_pending_focus(window, cx);
        let colors = theme::colors(cx);
        let zoom = window.rem_size().as_f32() / 16.;
        let sizes: Rc<Vec<Size<Pixels>>> = Rc::new(
            self.items
                .iter()
                .map(|item| size(px(0.), px(self.item_height(item, window, cx) * zoom)))
                .collect(),
        );
        let narrow = window.viewport_size().width <= px(NARROW_WIDTH);
        let top_form = (self.creating_parent == Some(None)).then(|| {
            self.render_form_body("Create top-level group", cx)
                .px(css(8.))
        });
        let view = cx.entity();
        let store = self.store.clone();
        let browser = cx.entity().downgrade();
        div()
            .id("request-browser")
            .flex()
            .flex_col()
            .overflow_hidden()
            .w(css(WIDTH))
            .min_w(css(MIN_WIDTH))
            .h_full()
            .border_1()
            .border_color(colors.border)
            .bg(colors.muted)
            .when(!narrow, |this| this.rounded(css(8.)))
            .child(self.render_header(cx))
            .children(self.render_focus_bar(cx))
            .children(top_form)
            .child(
                div()
                    .id("browser-list")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(self.width_recorder(cx))
                    .child(
                        v_virtual_list(view, "browser-rows", sizes, |this, range, window, cx| {
                            this.render_items(range, window, cx)
                        })
                        .track_scroll(&self.scroll_handle),
                    )
                    .vertical_scrollbar(&self.scroll_handle)
                    .on_drag_move(cx.listener(
                        |this, event: &DragMoveEvent<DraggedRequests>, window, cx| {
                            let payload = DragPayload::Requests(event.drag(cx).ids.clone());
                            this.drag_moved(
                                payload,
                                event.event.position,
                                event.bounds,
                                window,
                                cx,
                            );
                        },
                    ))
                    .on_drag_move(cx.listener(
                        |this, event: &DragMoveEvent<DraggedGroup>, window, cx| {
                            let payload = DragPayload::Group(event.drag(cx).id);
                            this.drag_moved(
                                payload,
                                event.event.position,
                                event.bounds,
                                window,
                                cx,
                            );
                        },
                    ))
                    .on_drop(cx.listener(|this, _: &DraggedRequests, _, cx| this.drop_commit(cx)))
                    .on_drop(cx.listener(|this, _: &DraggedGroup, _, cx| this.drop_commit(cx)))
                    .context_menu(menus::blank_menu(store, browser)),
            )
    }
}

/// A 22 px icon button of the tree rows.
fn small_button(id: impl Into<ElementId>, icon: IconName, icon_size: f32, cx: &App) -> Button {
    let colors = theme::colors(cx);
    Button::new(id)
        .ghost()
        .child(Icon::new(icon).size(css(icon_size)))
        .size(css(22.))
        .p_0()
        .rounded(px(0.))
        .text_color(colors.muted_foreground)
}

/// A 24 px header action: `.browser-action`.
fn header_button(id: impl Into<ElementId>, icon: IconName, cx: &App) -> Button {
    let colors = theme::colors(cx);
    Button::new(id)
        .ghost()
        .child(Icon::new(icon).size(css(14.)))
        .size(css(24.))
        .p_0()
        .rounded(px(4.))
        .text_color(colors.muted_foreground)
}

/// The Add, Save, and Cancel text buttons of the group forms.
fn text_button(id: impl Into<ElementId>, label: &'static str, cx: &App) -> Button {
    let colors = theme::colors(cx);
    let clear = gpui_kit::transparent_black();
    Button::new(id)
        .custom(
            ButtonCustomVariant::new(cx)
                .color(clear)
                .foreground(colors.muted_foreground)
                .hover(clear)
                .active(clear),
        )
        .child(
            div()
                .font_family(theme::MONO)
                .text_size(css(9.))
                .child(label),
        )
        .h_auto()
        .p_0()
}

/// The "into" drop ring: `shadow-[inset_0_0_0_1px_var(--color-primary)]`.
fn into_ring(color: Hsla) -> impl IntoElement {
    div().absolute().inset_0().border_1().border_color(color)
}

/// The before or after drop line.
fn drop_line(
    zone: Option<DropZone>,
    level: usize,
    color: Hsla,
) -> Option<impl IntoElement + use<>> {
    let zone = zone.filter(|zone| *zone != DropZone::Into)?;
    Some(
        div()
            .absolute()
            .right_0()
            .left(css(rows::indent(level)))
            .h(px(2.))
            .bg(color)
            .map(|this| {
                if zone == DropZone::Before {
                    this.top_0()
                } else {
                    this.bottom_0()
                }
            }),
    )
}

/// `TreeGuides`: faint tree lines inside a row.
fn tree_guides(guide: &TreeGuide, level: usize, end: f32, color: Hsla) -> impl IntoElement + use<> {
    let color = color.opacity(0.4);
    let x = rows::guide_x;
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .children(guide.through.iter().map(|depth| {
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .w(px(1.))
                .left(css(x(*depth)))
                .bg(color)
        }))
        .child(
            div()
                .absolute()
                .top_0()
                .w(px(1.))
                .left(css(x(level)))
                .bg(color)
                .map(|this| match guide.elbow {
                    Elbow::Mid => this.bottom_0(),
                    Elbow::Last => this.h_1_2(),
                }),
        )
        .child(
            div()
                .absolute()
                .top_1_2()
                .h(px(1.))
                .left(css(x(level)))
                .w(css((end - x(level)).max(2.)))
                .bg(color),
        )
}
