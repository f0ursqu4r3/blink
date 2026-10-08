use gpui_kit::{AppContext as _, TestAppContext};
use serde_json::json;

use super::ContractPanel;
use crate::test_support::{self, Reply, serve, wait};

#[gpui_kit::test]
fn applies_suggestions_validates_response_and_refreshes_retained_source(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let engine = test_support::engine(dir.path());
    test_support::init(cx, &engine);
    let (base, _requests) = serve(|_| Reply::ok("application/json", br#"{"name":4}"#.to_vec()));
    let source = dir.path().join("api.json");
    let spec = json!({"openapi":"3.1.0","info":{"title":"Contract test"},"servers":[{"url":base}],"paths":{"/pets":{"post":{
        "parameters":[{"name":"limit","in":"query","required":true,"schema":{"type":"integer","minimum":1,"example":2}}],
        "requestBody":{"required":true,"content":{"application/json":{"example":{"name":"Milo"},"schema":{"type":"object"}}}},
        "responses":{"200":{"content":{"application/json":{"schema":{"type":"object","properties":{"name":{"type":"string"}}}}}}}
    }}}});
    std::fs::write(&source, spec.to_string()).unwrap();
    futures::executor::block_on(engine.grant_file(source.clone())).unwrap();
    let imported =
        blink_core::import::parse_import(&spec.to_string(), source.to_str().unwrap()).unwrap();
    let harness = test_support::open(cx, &engine);
    harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.import(&imported);
        })
    });
    harness.draw(cx);
    let id = harness.active_id(cx);
    let panel = harness.update(cx, |_, cx| {
        cx.new(|cx| ContractPanel::new(harness.store.clone(), id, cx))
    });
    harness.edit_draft(cx, |draft| {
        draft.query.clear();
        draft.body.clear();
    });
    panel.update(cx, |panel, cx| panel.apply_example(cx));
    assert_eq!(
        harness.session(cx, |session| serde_json::from_str::<serde_json::Value>(
            &session.draft.body
        )
        .unwrap()),
        json!({"name":"Milo"})
    );
    panel.update(cx, |panel, cx| panel.validate(false, cx));
    assert!(cx.read(|cx| {
        panel
            .read(cx)
            .report
            .as_ref()
            .unwrap()
            .1
            .errors
            .iter()
            .any(|message| message.contains("query.limit"))
    }));
    panel.update(cx, |panel, cx| {
        let suggestion = panel.contract(cx).unwrap().suggestions().unwrap().remove(0);
        panel.add_parameter(suggestion, cx);
    });
    panel.update(cx, |panel, cx| panel.validate(false, cx));
    assert_eq!(
        cx.read(|cx| panel.read(cx).report.as_ref().unwrap().1.summary()),
        "Contract matched"
    );
    harness.send_draft(cx);
    panel.update(cx, |panel, cx| panel.validate(true, cx));
    assert!(cx.read(|cx| {
        panel
            .read(cx)
            .report
            .as_ref()
            .unwrap()
            .1
            .errors
            .iter()
            .any(|message| message.contains("$.name"))
    }));
    let other_id = harness.store.update(cx, |store, cx| {
        store.update_workspace(cx, |workspace| {
            workspace.import(&imported);
            let other = workspace.shown_active_id().unwrap();
            workspace.select(id);
            other
        })
    });
    std::fs::write(&source, r#"{"openapi":"3.1.0","paths":{}}"#).unwrap();
    panel.update(cx, |panel, cx| panel.refresh(cx));
    wait(cx, "contract refresh", |cx| !panel.read(cx).refreshing);
    assert!(cx.read(|cx| panel.read(cx).notice.contains("Spec changed")));
    assert!(cx.read(|cx| panel.read(cx).notice.contains("removed")));
    assert_eq!(
        harness.session(cx, |session| session
            .draft
            .openapi_contract
            .as_ref()
            .unwrap()
            .spec["paths"]
            .clone()),
        json!({})
    );
    // A source shared by another imported group keeps that request's snapshot.
    assert!(cx.read(|cx| {
        harness
            .store
            .read(cx)
            .workspace
            .session(other_id)
            .unwrap()
            .draft
            .openapi_contract
            .as_ref()
            .unwrap()
            .spec["paths"]
            .get("/pets")
            .is_some()
    }));
    // A pending refresh cannot overwrite a replacement contract.
    std::fs::write(&source, spec.to_string()).unwrap();
    panel.update(cx, |panel, cx| panel.refresh(cx));
    harness.edit_draft(cx, |draft| {
        draft.openapi_contract.as_mut().unwrap().source = "changed.json".into()
    });
    wait(cx, "discard changed contract refresh", |cx| {
        !panel.read(cx).refreshing
    });
    assert!(cx.read(|cx| panel.read(cx).notice.contains("changed during refresh")));
    assert_eq!(
        harness.session(cx, |session| session
            .draft
            .openapi_contract
            .as_ref()
            .unwrap()
            .source
            .clone()),
        "changed.json"
    );
}
