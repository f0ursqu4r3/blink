//! Collection order, datasets, execution, and report export.
use crate::store::Store;
use blink_core::collection_runner::{RunOptions, RunStatus, group_order, parse_dataset};
use blink_core::model::Definitions;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{Disableable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::*;

pub fn open(store: Entity<Store>, group_id: u64, window: &mut Window, cx: &mut App) {
    let view = cx.new(|cx| {
        let order = group_order(&store.read(cx).workspace, group_id);
        let subscription = cx.observe(&store, |_, _, cx| cx.notify());
        CollectionRunner {
            store,
            order,
            dataset: Vec::new(),
            dataset_name: String::new(),
            stop_on_failure: false,
            allow_protected: false,
            error: String::new(),
            last_options: None,
            _subscription: subscription,
        }
    });
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .title("Run collection")
            .w(px(760.))
            .max_h(relative(0.85))
            .child(view.clone())
    });
}

struct CollectionRunner {
    store: Entity<Store>,
    order: Vec<u64>,
    dataset: Vec<Definitions>,
    dataset_name: String,
    stop_on_failure: bool,
    allow_protected: bool,
    error: String,
    last_options: Option<RunOptions>,
    _subscription: Subscription,
}

impl CollectionRunner {
    fn start(&mut self, failed_only: bool, cx: &mut Context<Self>) {
        self.error.clear();
        let options = if failed_only {
            let Some(mut options) = self.last_options.clone() else {
                return;
            };
            let Some(report) = &self.store.read(cx).collection_run else {
                return;
            };
            let cases = report.failed_cases();
            if cases.is_empty() {
                return;
            }
            options.only = Some(cases);
            options.resume = report.resume.clone();
            options
        } else {
            RunOptions {
                request_ids: self.order.clone(),
                dataset: self.dataset.clone(),
                stop_on_failure: self.stop_on_failure,
                allow_protected: self.allow_protected,
                only: None,
                resume: Vec::new(),
            }
        };
        self.last_options = Some(options.clone());
        self.store
            .update(cx, |store, cx| store.start_collection(options, cx));
        cx.notify();
    }

    fn import_dataset(&mut self, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open CSV or JSON dataset".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let json = match path.extension().and_then(|e| e.to_str()) {
                        Some("json") => true,
                        Some("csv") => false,
                        _ => return Err("Choose a .csv or .json dataset.".to_string()),
                    };
                    let text = std::fs::read_to_string(path)
                        .map_err(|e| format!("Cannot read dataset: {e}"))?;
                    parse_dataset(&text, json)
                })
                .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(rows) => {
                        this.dataset = rows;
                        this.dataset_name = name;
                        this.error.clear();
                    }
                    Err(error) => this.error = error,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn export_report(&mut self, cx: &mut Context<Self>) {
        let Some(report) = &self.store.read(cx).collection_run else {
            return;
        };
        let text = match report.to_json() {
            Ok(text) => text,
            Err(error) => {
                self.error = error;
                cx.notify();
                return;
            }
        };
        let directory = std::env::current_dir().unwrap_or_default();
        let picked = cx.prompt_for_new_path(&directory, Some("blink-run-report.json"));
        let engine = self.store.read(cx).engine.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = picked.await else {
                return;
            };
            let result = engine.save_response_text(text, path).await;
            this.update(cx, |this, cx| {
                this.error = result.err().unwrap_or_default();
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}

impl Render for CollectionRunner {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let store = self.store.read(cx);
        let running = store.collection_running;
        let failed = store
            .collection_run
            .as_ref()
            .is_some_and(|r| !r.failed_cases().is_empty())
            && self.last_options.is_some();
        let has_report = store.collection_run.is_some();
        let error = if self.error.is_empty() {
            store.collection_error.clone()
        } else {
            self.error.clone()
        };
        let mut order = v_flex().gap(px(4.));
        for (index, id) in self.order.iter().enumerate() {
            let name = store
                .workspace
                .session(*id)
                .map(|s| {
                    format!(
                        "{} {}",
                        s.draft.method,
                        blink_core::session::session_label(s, None)
                    )
                })
                .unwrap_or_else(|| "Deleted request".into());
            order = order.child(
                h_flex()
                    .gap(px(8.))
                    .child(div().flex_1().child(format!("{}. {name}", index + 1)))
                    .child(
                        Button::new(("run-up", index))
                            .label("Up")
                            .xsmall()
                            .disabled(running || index == 0)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.order.swap(index, index - 1);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new(("run-down", index))
                            .label("Down")
                            .xsmall()
                            .disabled(running || index + 1 == self.order.len())
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.order.swap(index, index + 1);
                                cx.notify();
                            })),
                    ),
            );
        }
        let mut results = v_flex().gap(px(4.));
        if let Some(report) = &store.collection_run {
            let passed = report
                .results
                .iter()
                .filter(|r| r.status == RunStatus::Passed)
                .count();
            let failed = report
                .results
                .iter()
                .filter(|r| r.status == RunStatus::Failed)
                .count();
            results = results.child(format!(
                "{passed} passed · {failed} failed · {} total · {:.0} ms",
                report.results.len(),
                report.duration_ms
            ));
            for (index, result) in report.results.iter().enumerate() {
                let status = match result.status {
                    RunStatus::Passed => "PASS",
                    RunStatus::Failed => "FAIL",
                    RunStatus::Skipped => "SKIP",
                    RunStatus::Canceled => "CANCELED",
                };
                results = results.child(div().id(("run-result", index)).text_sm().child(format!("Row {} · {status} · {} · {} checks failed · {} captures failed · {} contract errors · {} unsupported rules{}", result.iteration + 1, result.name, result.assertions_failed, result.capture_errors, result.contract_errors, result.contract_unsupported, result.error.as_ref().map(|e| format!(" · {e}")).unwrap_or_default())));
                for failure in &result.failures {
                    results = results.child(div().pl(px(12.)).text_sm().child(failure.clone()));
                }
            }
        }
        v_flex().gap(px(12.)).text_sm()
            .child("Requests run in the order below. Captures apply to later requests in each dataset row.")
            .child(div().id("collection-order").max_h(px(190.)).overflow_y_scroll().child(order))
            .child(h_flex().gap(px(8.))
                .child(Button::new("run-dataset").label("Load dataset…").outline().small().disabled(running).on_click(cx.listener(|this, _, _, cx| this.import_dataset(cx))))
                .child(Button::new("run-clear-data").label("Clear").small().disabled(running || self.dataset.is_empty()).on_click(cx.listener(|this, _, _, cx| { this.dataset.clear(); this.dataset_name.clear(); cx.notify(); })))
                .child(if self.dataset.is_empty() { "One run · no dataset".to_string() } else { format!("{} · {} rows", self.dataset_name, self.dataset.len()) }))
            .child("CSV headers or JSON object keys set tokens such as {{name}}. Each row starts from the current workspace values.")
            .child(Checkbox::new("run-stop").label("Stop on first failure").checked(self.stop_on_failure).disabled(running).on_click(cx.listener(|this, checked, _, cx| { this.stop_on_failure = *checked; cx.notify(); })))
            .child(Checkbox::new("run-protected").label("Allow this run to use protected environments").checked(self.allow_protected).disabled(running).on_click(cx.listener(|this, checked, _, cx| { this.allow_protected = *checked; cx.notify(); })))
            .child(h_flex().gap(px(8.))
                .child(Button::new("run-start").label(if running { "Running…" } else { "Run" }).primary().small().disabled(running || self.order.is_empty()).on_click(cx.listener(|this, _, _, cx| this.start(false, cx))))
                .child(Button::new("run-cancel").label("Cancel").small().disabled(!running).on_click(cx.listener(|this, _, _, cx| this.store.update(cx, |store, cx| store.cancel_collection(cx)))))
                .child(Button::new("run-failed").label("Rerun failed").small().disabled(running || !failed).on_click(cx.listener(|this, _, _, cx| this.start(true, cx))))
                .child(Button::new("run-export").label("Export report…").small().disabled(running || !has_report).on_click(cx.listener(|this, _, _, cx| this.export_report(cx)))))
            .child(error)
            .child(div().id("collection-results").max_h(px(240.)).overflow_y_scroll().child(results))
    }
}
