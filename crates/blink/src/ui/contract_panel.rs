//! Retained OpenAPI source, parameter hints, examples, and explicit validation.

use blink_core::openapi_contract::{OpenApiContract, ParameterSuggestion, ValidationReport};
use blink_core::request::pair;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, Sizable as _};
use gpui_kit::*;

use crate::store::Store;
use crate::theme;
use crate::ui::request_pane::common::{replace_draft, session_context};

pub struct ContractPanel {
    store: Entity<Store>,
    session_id: u64,
    notice: String,
    report: Option<(String, ValidationReport)>,
    report_input: Option<(
        blink_core::model::Draft,
        Option<blink_core::model::ApiResponse>,
        String,
        bool,
    )>,
    refreshing: bool,
    _subscription: Subscription,
}

impl ContractPanel {
    pub fn new(store: Entity<Store>, session_id: u64, cx: &mut Context<Self>) -> Self {
        let subscription = cx.observe(&store, |this, _, cx| {
            // Save notifications do not invalidate a result. Request, response,
            // token, and contract changes do.
            if this.report.is_some() {
                let current = session_context(this.store.read(cx), this.session_id).map(
                    |(session, context)| {
                        let fingerprint =
                            blink_core::runner::prepare(&session.draft, Some(&context))
                                .fingerprint();
                        (session.draft, session.response, fingerprint, session.stale)
                    },
                );
                if current != this.report_input {
                    this.report = None;
                }
            }
            cx.notify();
        });
        Self {
            store,
            session_id,
            notice: String::new(),
            report: None,
            report_input: None,
            refreshing: false,
            _subscription: subscription,
        }
    }

    fn contract(&self, cx: &App) -> Option<OpenApiContract> {
        self.store
            .read(cx)
            .workspace
            .session(self.session_id)?
            .draft
            .openapi_contract
            .clone()
    }

    fn validate(&mut self, response: bool, cx: &mut Context<Self>) {
        let Some(contract) = self.contract(cx) else {
            return;
        };
        let Some((session, context)) = session_context(self.store.read(cx), self.session_id) else {
            return;
        };
        self.notice.clear();
        self.report_input = Some((
            session.draft.clone(),
            session.response.clone(),
            blink_core::runner::prepare(&session.draft, Some(&context)).fingerprint(),
            session.stale,
        ));
        if response {
            if session.stale {
                self.notice =
                    "The response is from an earlier request. Send the current request first."
                        .into();
                self.report = None;
            } else if let Some(response) = session.response.as_ref() {
                self.report = Some(("Response".into(), contract.validate_response(response)));
            } else {
                self.notice = "Send a request to validate its response.".into();
            }
        } else {
            match blink_core::runner::prepare(&session.draft, Some(&context)).request {
                Ok(request) => {
                    self.report = Some(("Request".into(), contract.validate_request(&request)))
                }
                Err(error) => {
                    self.report = Some((
                        "Request".into(),
                        ValidationReport {
                            errors: vec![error],
                            unsupported: Vec::new(),
                        },
                    ))
                }
            }
        }
        cx.notify();
    }

    fn apply_example(&mut self, cx: &mut Context<Self>) {
        let Some(contract) = self.contract(cx) else {
            return;
        };
        let mut outcome = Ok(());
        replace_draft(&self.store, self.session_id, cx, |draft| {
            outcome = contract.apply_example(draft)
        });
        self.notice = match outcome {
            Ok(()) => "Applied the JSON example. Check its values before sending.".into(),
            Err(error) => error,
        };
        cx.notify();
    }

    fn add_parameter(&mut self, suggestion: ParameterSuggestion, cx: &mut Context<Self>) {
        let value = suggestion.example.unwrap_or_default();
        let name = suggestion.name;
        replace_draft(&self.store, self.session_id, cx, |draft| {
            let rows = if suggestion.location == "query" {
                &mut draft.query
            } else {
                &mut draft.headers
            };
            if let Some(row) = rows.iter_mut().find(|row| {
                if suggestion.location == "header" {
                    row.key.eq_ignore_ascii_case(&name)
                } else {
                    row.key == name
                }
            }) {
                row.enabled = true;
                if row.value.is_empty() {
                    row.value = value;
                }
            } else {
                rows.push(pair(name, value));
            }
        });
        self.notice = "Applied the parameter suggestion.".into();
        cx.notify();
    }

    fn refresh(&mut self, cx: &mut Context<Self>) {
        let Some(mut contract) = self.contract(cx) else {
            return;
        };
        if self.refreshing {
            return;
        }
        if contract.source.is_empty() {
            self.notice = "Import the spec from a file to retain its source path.".into();
            cx.notify();
            return;
        }
        let original = contract.clone();
        let scope = self
            .store
            .read(cx)
            .workspace
            .session(self.session_id)
            .and_then(|session| {
                self.store
                    .read(cx)
                    .workspace
                    .project_for_group(session.group_id)
                    .map(|project| project.root_id)
            });
        self.refreshing = true;
        self.notice = "Reading the source spec…".into();
        cx.notify();
        let read = self
            .store
            .read(cx)
            .engine
            .read_contract_source(contract.source.clone());
        cx.spawn(async move |this, cx| {
            let outcome = read.await.and_then(|text| contract.refresh(&text));
            let _ = this.update(cx, |this, cx| {
                this.refreshing = false;
                match outcome {
                    Ok(changed) => {
                        let current_scope = this.store.read(cx).workspace.session(this.session_id).and_then(|session|
                            this.store.read(cx).workspace.project_for_group(session.group_id).map(|project| project.root_id));
                        if this.contract(cx).as_ref() != Some(&original) || current_scope != scope {
                            this.notice = "The request or project changed during refresh. Refresh the spec again.".into();
                            cx.notify();
                            return;
                        }
                        replace_draft(&this.store, this.session_id, cx, |draft| draft.openapi_contract = Some(contract.clone()));
                        this.notice = if changed { "Spec changed. Updated this request's contract. Validate the request again." } else { "This operation's contract has no changes." }.into();
                        if let Err(error) = contract.operation() { this.notice.push(' '); this.notice.push_str(&error); }
                        this.report = None;
                    }
                    Err(error) => this.notice = error,
                }
                cx.notify();
            });
        }).detach();
    }
}

impl Render for ContractPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let Some(contract) = self.contract(cx) else {
            return div()
                .p_3()
                .child("Import an OpenAPI 3.x spec to attach a contract.")
                .into_any_element();
        };
        let source = if contract.source.is_empty() {
            "Source path is unavailable.".into()
        } else {
            contract.source.clone()
        };
        let mut panel = div().flex().flex_col().gap_3().p_3().w_full().text_size(px(12.))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(format!("{} {}", contract.method, contract.path)))
            .child(div().text_color(colors.muted_foreground).child(source))
            .child(div().flex().flex_wrap().gap_2()
                .child(Button::new("contract-request").small().label("Validate request").on_click(cx.listener(|this, _, _, cx| this.validate(false, cx))))
                .child(Button::new("contract-response").small().label("Validate response").on_click(cx.listener(|this, _, _, cx| this.validate(true, cx))))
                .child(Button::new("contract-example").small().label("Apply body example").on_click(cx.listener(|this, _, _, cx| this.apply_example(cx))))
                .child(Button::new("contract-refresh").small().label("Refresh spec").disabled(self.refreshing).on_click(cx.listener(|this, _, _, cx| this.refresh(cx)))))
            .child(div().text_color(colors.muted_foreground).child("The operation and its referenced schemas are stored with this request. JSON bodies and references within the spec are supported. Unsupported rules produce an incomplete result. External references are not fetched."));
        match contract.suggestions() {
            Ok(suggestions) => {
                for (index, suggestion) in suggestions.into_iter().enumerate() {
                    let mut row = div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .border_b_1()
                        .border_color(colors.border)
                        .pb_2()
                        .child(format!(
                            "{} · {}{}",
                            suggestion.name,
                            suggestion.location,
                            if suggestion.required {
                                " · required"
                            } else {
                                " · optional"
                            }
                        ));
                    if !suggestion.description.is_empty() {
                        row = row.child(
                            div()
                                .text_color(colors.muted_foreground)
                                .child(suggestion.description.clone()),
                        );
                    }
                    if let Some(value) = &suggestion.example {
                        row = row.child(format!("Example: {value}"));
                    }
                    if matches!(suggestion.location.as_str(), "query" | "header") {
                        row = row.child(
                            Button::new(("contract-parameter", index))
                                .small()
                                .ghost()
                                .label("Apply suggestion")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.add_parameter(suggestion.clone(), cx)
                                })),
                        );
                    }
                    panel = panel.child(row);
                }
            }
            Err(error) => panel = panel.child(div().text_color(colors.destructive).child(error)),
        }
        if !self.notice.is_empty() {
            panel = panel.child(self.notice.clone());
        }
        if let Some((target, report)) = &self.report {
            panel = panel.child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("{target}: {}", report.summary())),
            );
            for error in &report.errors {
                panel = panel.child(div().text_color(colors.destructive).child(error.clone()));
            }
            for unsupported in &report.unsupported {
                panel = panel.child(format!("Not checked: {unsupported}"));
            }
        }
        panel.into_any_element()
    }
}

#[cfg(test)]
mod ui_tests;
