//! Credential actions for the current local/project scope.
use crate::store::Store;
use crate::theme;
use blink_core::engine::authentication::OAuthConfig;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{Sizable as _, TitleBar};
use gpui_kit::*;

pub fn open(store: Entity<Store>, session_id: u64, window: &mut Window, cx: &mut App) {
    let scope = store
        .read(cx)
        .workspace
        .session(session_id)
        .and_then(|s| store.read(cx).workspace.project_for_group(s.group_id))
        .map(|p| p.path.clone());
    let mut titlebar = TitleBar::title_bar_options();
    titlebar.title = Some("Credentials".into());
    let options = WindowOptions {
        titlebar: Some(titlebar),
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            window.display(cx).map(|d| d.id()),
            size(px(650.), px(750.)),
            cx,
        ))),
        ..TitleBar::window_options()
    };
    let _ = gpui_kit::open_window(options, cx, move |window, cx| {
        cx.new(|cx| CredentialsPanel::new(store, scope, window, cx))
    });
}
struct CredentialsPanel {
    store: Entity<Store>,
    scope: Option<String>,
    name: Entity<InputState>,
    secret: Entity<InputState>,
    authorization: Entity<InputState>,
    token: Entity<InputState>,
    client: Entity<InputState>,
    scopes: Entity<InputState>,
    origin: Entity<InputState>,
    message: String,
    busy: bool,
}
impl CredentialsPanel {
    fn new(
        store: Entity<Store>,
        scope: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut field =
            |placeholder: &str| cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let secret = field("Secret value");
        secret.update(cx, |state, cx| state.set_masked(true, window, cx));
        let mut field =
            |placeholder: &str| cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        Self {
            store,
            scope,
            name: field("api-token"),
            secret,
            authorization: field("https://provider.example/authorize"),
            token: field("https://provider.example/token"),
            client: field("Public client ID"),
            scopes: field("openid profile"),
            origin: field("https://api.example.com"),
            message: String::new(),
            busy: false,
        }
    }
    fn name(&self, cx: &App) -> String {
        self.name.read(cx).value().trim().to_owned()
    }
    fn complete(&mut self, result: Result<(), String>, success: &str, cx: &mut Context<Self>) {
        self.busy = false;
        self.message = result.map(|_| success.to_owned()).unwrap_or_else(|e| e);
        cx.notify();
    }
    fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let engine = self.store.read(cx).engine.clone();
        let future = engine.save_secret(
            self.scope.clone(),
            self.name(cx),
            self.secret.read(cx).value().to_string(),
        );
        self.secret.update(cx, |s, cx| s.set_value("", window, cx));
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = future.await;
            this.update(cx, |this, cx| {
                this.complete(
                    result,
                    "Secret saved. Use {{@name}} in a request field.",
                    cx,
                )
            })
            .ok();
        })
        .detach();
    }
    fn remove(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let future = self
            .store
            .read(cx)
            .engine
            .remove_secret(self.scope.clone(), self.name(cx));
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = future.await;
            this.update(cx, |this, cx| this.complete(result, "Secret removed.", cx))
                .ok();
        })
        .detach();
    }
    fn oauth(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let name = self.name(cx);
        if !blink_core::credentials::valid_name(&name) {
            self.message = "Enter a valid secret name.".into();
            cx.notify();
            return;
        }
        let config = OAuthConfig {
            authorization_url: self.authorization.read(cx).value().to_string(),
            token_url: self.token.read(cx).value().to_string(),
            client_id: self.client.read(cx).value().to_string(),
            scopes: self.scopes.read(cx).value().to_string(),
        };
        let engine = self.store.read(cx).engine.clone();
        let scope = self.scope.clone();
        let start = engine.begin_oauth(config);
        self.busy = true;
        self.message = "Opening browser sign-in. The callback waits for up to 3 minutes.".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = match start.await {
                Err(error) => Err(error),
                Ok(login) => {
                    let url = login.authorization_url.clone();
                    if this.update(cx, |_, cx| cx.open_url(&url)).is_err() {
                        return;
                    }
                    engine.finish_oauth(login, scope, name).await
                }
            };
            this.update(cx, |this, cx| {
                this.complete(
                    result,
                    "Signed in. Use {{@name}} as a Bearer token. Expiring tokens refresh on send.",
                    cx,
                )
            })
            .ok();
        })
        .detach();
    }
    fn identity(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let origin = self.origin.read(cx).value().to_string();
        let scope = self.scope.clone();
        let engine = self.store.read(cx).engine.clone();
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose PEM identity".into()),
        });
        let _ = window;
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = match picker.await {
                Ok(Ok(Some(paths))) => match paths.first() {
                    Some(path) => {
                        engine
                            .save_client_identity(scope, origin, path.clone())
                            .await
                    }
                    None => Ok(()),
                },
                Ok(Ok(None)) => Ok(()),
                _ => Err("Cannot open file picker.".into()),
            };
            this.update(cx, |this, cx| {
                this.complete(result, "Client identity selection finished.", cx)
            })
            .ok();
        })
        .detach();
    }
    fn remove_identity(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let future = self
            .store
            .read(cx)
            .engine
            .remove_client_identity(self.scope.clone(), self.origin.read(cx).value().to_string());
        self.busy = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = future.await;
            this.update(cx, |this, cx| {
                this.complete(result, "Client identity removed.", cx)
            })
            .ok();
        })
        .detach();
    }
}
impl Render for CredentialsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = theme::colors(cx);
        let field = |label: &'static str, input: &Entity<InputState>| {
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(label)
                .child(Input::new(input).small())
        };
        div().size_full().flex().flex_col().bg(colors.background).text_color(colors.foreground)
            .child(TitleBar::new().child("Credentials"))
            .child(div().id("credentials-form").flex_1().overflow_y_scroll().p_4().flex().flex_col().gap_3().text_size(px(12.))
                .child(format!("Scope: {}",self.scope.as_deref().unwrap_or("Local groups")))
                .child("Values stay in macOS Keychain. Requests contain {{@name}} references. Use the same name below to replace or remove a credential.")
                .child(field("Secret name",&self.name))
                .child(div().child("Secret value").child(Input::new(&self.secret).mask_toggle().small()))
                .child(div().flex().gap_2().child(Button::new("save-secret").label("Save secret").on_click(cx.listener(|this,_,window,cx|this.save(window,cx))))
                    .child(Button::new("remove-secret").ghost().label("Remove secret").on_click(cx.listener(|this,_,_,cx|this.remove(cx)))))
                .child("OAuth 2 · Public client · S256 PKCE")
                .child("Register a loopback redirect at http://127.0.0.1 with a dynamic port and /oauth/callback. Providers must allow native public clients.")
                .child(field("Authorization endpoint",&self.authorization)).child(field("Token endpoint",&self.token)).child(field("Client ID",&self.client)).child(field("Scopes",&self.scopes))
                .child(Button::new("oauth-login").label("Sign in with browser").on_click(cx.listener(|this,_,_,cx|this.oauth(cx))))
                .child("Client certificate · PEM certificate chain and private key")
                .child(field("HTTPS origin",&self.origin))
                .child(div().flex().gap_2().child(Button::new("client-identity").label("Choose identity…").on_click(cx.listener(|this,_,window,cx|this.identity(window,cx))))
                    .child(Button::new("remove-identity").ghost().label("Remove identity").on_click(cx.listener(|this,_,_,cx|this.remove_identity(cx)))))
                .child(self.message.clone()))
    }
}

#[cfg(test)]
mod ui_tests;
