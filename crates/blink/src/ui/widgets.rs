//! Small shared pieces: method labels, the icon button style of the title
//! bar and Browser header, and text with CSS letter spacing or a dotted
//! underline.

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::*;

use crate::theme;

#[cfg(test)]
mod ui_tests;

/// A method name in its method color, such as `GET` in success green.
pub fn method_label(method: &str, size: f32, cx: &App) -> Div {
    div()
        .font_family(theme::MONO)
        .text_size(px(size))
        .font_weight(FontWeight::BOLD)
        .text_color(theme::method_color(method, cx))
        .child(SharedString::from(method.to_string()))
}

/// A ghost icon button with a tooltip, as the Vue `variant="ghost"` icon buttons.
pub fn icon_button(
    id: impl Into<ElementId>,
    icon: impl Into<Icon>,
    tooltip: impl Into<SharedString>,
) -> Button {
    Button::new(id)
        .ghost()
        .small()
        .icon(icon.into())
        .tooltip(tooltip)
}

/// Tailwind `tracking-widest`.
pub const WIDEST: f32 = 0.1;

/// One line of text with CSS `letter-spacing` and an optional CSS dotted
/// underline. GPUI text has neither, so each character is shaped on its own
/// and painted with `em × font size` after it (CSS adds the space after every
/// character, the last one too).
///
/// The text style (font, size, weight, color, `text_ellipsis`) is inherited
/// from the parent, or set on this element with the `Styled` text methods.
/// The spacing uses the resolved font size, so zoom (the root rem size)
/// scales it with the text.
pub struct Tracked {
    text: SharedString,
    em: f32,
    underline: Option<Option<Hsla>>,
    style: StyleRefinement,
}

/// `tracking-[<em>em]` text.
pub fn tracked(text: impl Into<SharedString>, em: f32) -> Tracked {
    Tracked {
        text: text.into(),
        em,
        underline: None,
        style: StyleRefinement::default(),
    }
}

/// `underline decoration-dotted underline-offset-3` text in the text color.
pub fn dotted(text: impl Into<SharedString>) -> Tracked {
    tracked(text, 0.).dotted_underline(None)
}

/// CSS `underline-offset-3`: the underline top is 3 px below the baseline.
const UNDERLINE_OFFSET: f32 = 3.;

impl Tracked {
    /// `underline decoration-dotted underline-offset-3`. `None` is the text
    /// color (`currentColor`); `Some` is a `decoration-*` color.
    pub fn dotted_underline(mut self, color: Option<Hsla>) -> Self {
        self.underline = Some(color);
        self
    }
}

impl Styled for Tracked {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl IntoElement for Tracked {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

/// Shaped characters and metrics from layout, for paint.
pub struct TrackedLayout {
    glyphs: Vec<ShapedLine>,
    ellipsis: Option<ShapedLine>,
    spacing: Pixels,
    width: Pixels,
    line_height: Pixels,
    baseline: Pixels,
    unit: f32,
    color: Hsla,
}

impl Element for Tracked {
    type RequestLayoutState = TrackedLayout;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, TrackedLayout) {
        let mut text_style = window.text_style();
        text_style.refine(&self.style.text);
        let rem = window.rem_size();
        let font_size = text_style.font_size.to_pixels(rem);
        let line_height = text_style.line_height_in_pixels(rem);
        let spacing = font_size * self.em;
        let system = window.text_system().clone();
        let shape = |text: SharedString| {
            let run = text_style.to_run(text.len());
            system.shape_line(text, font_size, &[run], None)
        };
        let glyphs: Vec<ShapedLine> = self
            .text
            .chars()
            .map(|ch| shape(SharedString::from(ch.to_string())))
            .collect();
        let ellipsis = text_style
            .text_overflow
            .is_some()
            .then(|| shape("…".into()));
        let width = glyphs
            .iter()
            .fold(px(0.), |sum, glyph| sum + glyph.width() + spacing);
        let font_id = system.resolve_font(&text_style.font());
        let ascent = system.ascent(font_id, font_size);
        let descent = system.descent(font_id, font_size).abs();
        let baseline = (line_height - ascent - descent) / 2. + ascent;

        let mut style = Style::default();
        style.refine(&self.style);
        let truncates = ellipsis.is_some();
        let layout_id = window.request_measured_layout(style, move |known, available, _, _| {
            let mut fitted = known.width.unwrap_or(width);
            if truncates && let AvailableSpace::Definite(space) = available.width {
                fitted = fitted.min(space);
            }
            size(fitted, known.height.unwrap_or(line_height))
        });
        let layout = TrackedLayout {
            glyphs,
            ellipsis,
            spacing,
            width,
            line_height,
            baseline,
            unit: rem / px(theme::REM),
            color: text_style.color,
        };
        (layout_id, layout)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut TrackedLayout,
        _: &mut Window,
        _: &mut App,
    ) {
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        layout: &mut TrackedLayout,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        // `text-overflow: ellipsis`: as many characters as fit before `…`.
        let mut count = layout.glyphs.len();
        let mut ellipsis = None;
        if let Some(mark) = &layout.ellipsis
            && layout.width > bounds.size.width + px(0.5)
        {
            let room = bounds.size.width - mark.width();
            let mut used = px(0.);
            count = 0;
            for glyph in &layout.glyphs {
                if used + glyph.width() > room {
                    break;
                }
                used += glyph.width() + layout.spacing;
                count += 1;
            }
            ellipsis = Some(mark);
        }
        let mut x = bounds.origin.x;
        for glyph in layout.glyphs.iter().take(count).chain(ellipsis) {
            let origin = point(x, bounds.origin.y);
            glyph
                .paint(
                    origin,
                    layout.line_height,
                    TextAlign::Left,
                    None,
                    window,
                    cx,
                )
                .ok();
            x += glyph.width() + layout.spacing;
        }

        // CSS `decoration-dotted`: round dots of the decoration thickness,
        // one thickness apart, from the text start to its end.
        if let Some(color) = self.underline {
            let color = color.unwrap_or(layout.color);
            let thickness = px(layout.unit).max(px(1.));
            let top = bounds.origin.y + layout.baseline + px(UNDERLINE_OFFSET * layout.unit);
            let end = x.min(bounds.right());
            let mut dot = bounds.origin.x;
            while dot + thickness <= end + px(0.01) {
                let square = Bounds::new(point(dot, top), size(thickness, thickness));
                window.paint_quad(fill(square, color).corner_radii(thickness / 2.));
                dot += thickness * 2.;
            }
        }
    }
}
