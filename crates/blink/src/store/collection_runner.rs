use super::Store;
use blink_core::collection_runner::{RunControl, RunOptions, run};
use gpui_kit::*;

impl Store {
    pub fn start_collection(&mut self, options: RunOptions, cx: &mut Context<Self>) {
        if self.collection_running || !self.ready || self.closing {
            return;
        }
        let control = RunControl::default();
        self.collection_control = Some(control.clone());
        self.collection_running = true;
        self.collection_error.clear();
        self.collection_run = None;
        let engine = self.engine.clone();
        let workspace = self.workspace.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = run(engine, workspace, options, control).await;
            this.update(cx, |this, cx| {
                this.collection_running = false;
                this.collection_control = None;
                if this.error == "Finish or cancel the collection run before changing projects." {
                    this.error.clear();
                }
                match result {
                    Ok(report) => this.collection_run = Some(report),
                    Err(error) => this.collection_error = error,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn cancel_collection(&mut self, cx: &mut Context<Self>) {
        if let Some(control) = &self.collection_control {
            control.cancel();
        }
        cx.notify();
    }
}
