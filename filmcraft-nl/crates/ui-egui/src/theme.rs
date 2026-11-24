//! Design tokens. Every widget reads colours/sizes from [`Tokens`] so themes apply everywhere.
//!
//! The default theme keeps the professional dark panel layout of the reference NLE and uses the
//! NL Video Suite palette (violet, magenta and neon cyan) taken from the owner's mark. Fonts are
//! Inter + JetBrains Mono (OFL).

use std::sync::Arc;

use egui::{Color32, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle, Visuals};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ThemeKind {
    /// Premiere-style darkest (default).
    #[default]
    Dark,
    /// Slightly lighter grey panels (Premiere's brightness slider mid position).
    Medium,
    Light,
}

impl ThemeKind {
    /// Settings ▸ Appearance ▸ Color Theme value (`darkest`, `dark`, `light`).
    pub fn pref_name(self) -> &'static str {
        match self {
            ThemeKind::Dark => "darkest",
            ThemeKind::Medium => "dark",
            ThemeKind::Light => "light",
        }
    }
    pub fn from_pref(s: &str) -> ThemeKind {
        match s {
            "dark" => ThemeKind::Medium,
            "light" => ThemeKind::Light,
            _ => ThemeKind::Dark,
        }
    }
    pub fn from_name(s: &str) -> Option<ThemeKind> {
        match s.to_ascii_lowercase().as_str() {
            "dark" | "darkest" => Some(ThemeKind::Dark),
            "medium" | "grey" | "gray" => Some(ThemeKind::Medium),
            "light" => Some(ThemeKind::Light),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tokens {
    pub kind: ThemeKind,
    /// Space between panels / app background.
    pub app_bg: Color32,
    /// Header bar (top).
    pub header_bg: Color32,
    /// Panel body.
    pub panel_bg: Color32,
    /// Panel tab strip.
    pub tab_bg: Color32,
    pub tab_text: Color32,
    pub tab_text_active: Color32,
    /// Violet focus rectangle around the active panel.
    pub focus: Color32,
    pub accent: Color32,
    pub accent_hover: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub icon: Color32,
    pub icon_active: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub field_bg: Color32,
    pub field_border: Color32,
    pub separator: Color32,
    pub row_alt: Color32,
    pub row_selected: Color32,
    /// Scrubby (hot-text) numeric value colour.
    pub hot_text: Color32,
    /// Selected pills and compact toggle backgrounds.
    pub pill_active_bg: Color32,
    /// The selected clip bar in Effect Controls.
    pub clip_bar_bg: Color32,
    /// Raised audio control surfaces, such as mixer channel strips.
    pub control_surface: Color32,
    /// Recessed audio control tracks and panner wells.
    pub control_well: Color32,
    pub control_border: Color32,
    pub control_handle_bg: Color32,
    pub control_handle: Color32,
    pub control_handle_dim: Color32,
    /// Data-plot surfaces (keyframe, EQ and dynamics graphs), distinct from media canvases.
    pub plot_bg: Color32,
    pub plot_grid: Color32,
    pub plot_axis: Color32,
    pub plot_handle: Color32,
    pub plot_handle_dim: Color32,
    /// Meter wells and the separate loudness readout surface.
    pub meter_bg: Color32,
    pub meter_readout_bg: Color32,
    pub meter_clip_off: Color32,
    pub panner_bg: Color32,
    pub panner_border: Color32,
    pub panner_speaker: Color32,
    pub fader_knob: Color32,
    pub eq_track: Color32,
    pub eq_tick: Color32,
    pub eq_knob: Color32,
    pub eq_knob_active: Color32,
    pub eq_node: Color32,
    pub eq_node_off: Color32,
    pub crossover_handle: Color32,
    pub keyframe_plot_bg: Color32,
    pub keyframe_plot_grid: Color32,
    pub keyframe_plot_axis: Color32,
    pub keyframe_handle: Color32,
    pub switch_fill: Color32,
    pub switch_knob: Color32,
    pub switch_border: Color32,
    pub slider_disabled_track: Color32,
    pub slider_disabled_knob: Color32,
    pub button_hover: Color32,
    pub button_border: Color32,
    pub caption_track_bg: Color32,
    pub scroll_thumb: Color32,
    pub scroll_thumb_hover: Color32,
    pub vertical_scroll_thumb: Color32,
    pub disabled_clip_bg: Color32,
    // timeline
    pub tl_bg: Color32,
    pub tl_track_bg: Color32,
    pub tl_track_bg_alt: Color32,
    pub tl_header_bg: Color32,
    pub tl_ruler_bg: Color32,
    pub tl_ruler_tick: Color32,
    pub tl_ruler_text: Color32,
    pub playhead: Color32,
    /// Fill of a track header's source-patch / track-targeting button while it is on.
    pub track_target: Color32,
    pub in_out_shade: Color32,
    pub clip_selected_border: Color32,
    pub render_red: Color32,
    pub render_yellow: Color32,
    pub render_green: Color32,
    pub monitor_bg: Color32,
    pub timecode: Color32,
    pub danger: Color32,
    pub radius: f32,
    pub radius_sm: f32,
    pub gap: f32,
    pub tab_h: f32,
}

impl Tokens {
    pub fn for_kind(kind: ThemeKind) -> Self {
        // NL Video Suite palette, taken from the owner's mark: electric violet, magenta and
        // neon cyan on near-black plum. The panel layout stays the professional dark one.
        let dark = Tokens {
            kind,
            app_bg: Color32::from_rgb(0x08, 0x05, 0x0d),
            header_bg: Color32::from_rgb(0x14, 0x0c, 0x1f),
            panel_bg: Color32::from_rgb(0x17, 0x10, 0x21),
            tab_bg: Color32::from_rgb(0x14, 0x0c, 0x1f),
            tab_text: Color32::from_rgb(0xb9, 0xa9, 0xcc),
            tab_text_active: Color32::from_rgb(0xf3, 0xec, 0xf9),
            focus: Color32::from_rgb(0xc9, 0xa6, 0xff),
            accent: Color32::from_rgb(0x75, 0x31, 0xfe),
            accent_hover: Color32::from_rgb(0x8b, 0x4d, 0xff),
            text: Color32::from_rgb(0xed, 0xe6, 0xf5),
            text_dim: Color32::from_rgb(0xb4, 0xa6, 0xc6),
            text_faint: Color32::from_rgb(0x7a, 0x6a, 0x8c),
            icon: Color32::from_rgb(0xb4, 0xa6, 0xc6),
            icon_active: Color32::from_rgb(0xf3, 0xec, 0xf9),
            hover: Color32::from_rgb(0x24, 0x18, 0x32),
            pressed: Color32::from_rgb(0x32, 0x20, 0x3f),
            field_bg: Color32::from_rgb(0x0e, 0x08, 0x16),
            field_border: Color32::from_rgb(0x36, 0x25, 0x43),
            separator: Color32::from_rgb(0x2c, 0x1e, 0x3b),
            row_alt: Color32::from_rgb(0x1b, 0x12, 0x26),
            row_selected: Color32::from_rgb(0x33, 0x20, 0x4a),
            hot_text: Color32::from_rgb(0xff, 0x6f, 0xb5),
            pill_active_bg: Color32::from_rgb(0x3a, 0x23, 0x52),
            clip_bar_bg: Color32::from_rgb(0x3b, 0x2a, 0x52),
            control_surface: Color32::from_rgb(0x1e, 0x14, 0x29),
            control_well: Color32::from_rgb(0x0d, 0x08, 0x13),
            control_border: Color32::from_rgb(0x4b, 0x35, 0x60),
            control_handle_bg: Color32::from_rgb(0x25, 0x19, 0x36),
            control_handle: Color32::from_rgb(0xe8, 0xdf, 0xf2),
            control_handle_dim: Color32::from_rgb(0xb4, 0xa6, 0xc6),
            plot_bg: Color32::from_rgb(0x13, 0x0c, 0x1c),
            plot_grid: Color32::from_rgb(0x2a, 0x1c, 0x3a),
            plot_axis: Color32::from_rgb(0x46, 0x30, 0x5c),
            plot_handle: Color32::from_rgb(0xe8, 0xdf, 0xf2),
            plot_handle_dim: Color32::from_rgb(0x8c, 0x7a, 0xa0),
            meter_bg: Color32::from_rgb(0x08, 0x05, 0x0d),
            meter_readout_bg: Color32::from_rgb(0x0f, 0x09, 0x16),
            meter_clip_off: Color32::from_rgb(0x33, 0x24, 0x3f),
            panner_bg: Color32::from_rgb(0x15, 0x0e, 0x20),
            panner_border: Color32::from_rgb(0x5a, 0x40, 0x72),
            panner_speaker: Color32::from_rgb(0x9c, 0x86, 0xb4),
            fader_knob: Color32::from_rgb(0x5e, 0x45, 0x76),
            eq_track: Color32::from_rgb(0x2e, 0x20, 0x40),
            eq_tick: Color32::from_rgb(0x5e, 0x45, 0x76),
            eq_knob: Color32::from_rgb(0xc4, 0xb4, 0xd6),
            eq_knob_active: Color32::from_rgb(0xf3, 0xec, 0xf9),
            eq_node: Color32::from_rgb(0xe8, 0xdf, 0xf2),
            eq_node_off: Color32::from_rgb(0x6e, 0x5a, 0x82),
            crossover_handle: Color32::from_rgb(0xf5, 0xef, 0xfa),
            keyframe_plot_bg: Color32::from_rgb(0x13, 0x0c, 0x1c),
            keyframe_plot_grid: Color32::from_rgb(0x2a, 0x1c, 0x3a),
            keyframe_plot_axis: Color32::from_rgb(0x46, 0x30, 0x5c),
            keyframe_handle: Color32::from_rgb(0xe8, 0xdf, 0xf2),
            switch_fill: Color32::from_rgb(0xd9, 0xc9, 0xe8),
            switch_knob: Color32::from_rgb(0x1b, 0x12, 0x26),
            switch_border: Color32::from_rgb(0x8a, 0x72, 0xa0),
            slider_disabled_track: Color32::from_rgb(0x4a, 0x38, 0x5c),
            slider_disabled_knob: Color32::from_rgb(0x6e, 0x5a, 0x82),
            button_hover: Color32::from_rgb(0x2a, 0x1b, 0x3b),
            button_border: Color32::from_rgb(0x4b, 0x35, 0x60),
            caption_track_bg: Color32::from_rgb(0x24, 0x16, 0x31),
            scroll_thumb: Color32::from_rgb(0x4b, 0x35, 0x60),
            scroll_thumb_hover: Color32::from_rgb(0x6a, 0x4a, 0x86),
            vertical_scroll_thumb: Color32::from_rgb(0x50, 0x3a, 0x66),
            disabled_clip_bg: Color32::from_rgb(0x24, 0x18, 0x32),
            tl_bg: Color32::from_rgb(0x14, 0x0c, 0x1f),
            tl_track_bg: Color32::from_rgb(0x14, 0x0c, 0x1f),
            tl_track_bg_alt: Color32::from_rgb(0x17, 0x10, 0x21),
            tl_header_bg: Color32::from_rgb(0x17, 0x10, 0x21),
            tl_ruler_bg: Color32::from_rgb(0x14, 0x0c, 0x1f),
            tl_ruler_tick: Color32::from_rgb(0x8c, 0x7a, 0xa0),
            tl_ruler_text: Color32::from_rgb(0xb4, 0xa6, 0xc6),
            playhead: Color32::from_rgb(0x8f, 0xf0, 0xff),
            track_target: Color32::from_rgb(0x5a, 0x27, 0xb8),
            in_out_shade: Color32::from_rgb(0x3a, 0x2a, 0x4a),
            clip_selected_border: Color32::from_rgb(0xf5, 0xef, 0xfa),
            render_red: Color32::from_rgb(0xff, 0x54, 0x70),
            render_yellow: Color32::from_rgb(0xf5, 0xd7, 0x6e),
            render_green: Color32::from_rgb(0x47, 0xd7, 0xa0),
            monitor_bg: Color32::from_rgb(0x08, 0x05, 0x0d),
            timecode: Color32::from_rgb(0x8f, 0xf0, 0xff),
            danger: Color32::from_rgb(0xff, 0x6b, 0x81),
            radius: 0.0,
            radius_sm: 4.0,
            gap: 4.0,
            tab_h: 32.0,
        };
        match kind {
            ThemeKind::Dark => dark,
            ThemeKind::Medium => Tokens {
                app_bg: Color32::from_rgb(0x1a, 0x12, 0x26),
                header_bg: Color32::from_rgb(0x24, 0x18, 0x32),
                panel_bg: Color32::from_rgb(0x24, 0x18, 0x32),
                tab_bg: Color32::from_rgb(0x24, 0x18, 0x32),
                field_bg: Color32::from_rgb(0x17, 0x10, 0x21),
                tl_bg: Color32::from_rgb(0x24, 0x18, 0x32),
                tl_track_bg: Color32::from_rgb(0x24, 0x18, 0x32),
                tl_track_bg_alt: Color32::from_rgb(0x2a, 0x1d, 0x3a),
                tl_header_bg: Color32::from_rgb(0x2a, 0x1d, 0x3a),
                tl_ruler_bg: Color32::from_rgb(0x24, 0x18, 0x32),
                monitor_bg: Color32::from_rgb(0x24, 0x18, 0x32),
                row_alt: Color32::from_rgb(0x2a, 0x1d, 0x3a),
                hover: Color32::from_rgb(0x33, 0x23, 0x44),
                ..dark
            },
            ThemeKind::Light => Tokens {
                app_bg: Color32::from_rgb(0xe9, 0xe2, 0xf2),
                header_bg: Color32::from_rgb(0xf3, 0xee, 0xf9),
                panel_bg: Color32::from_rgb(0xf7, 0xf3, 0xfb),
                tab_bg: Color32::from_rgb(0xf7, 0xf3, 0xfb),
                tab_text: Color32::from_rgb(0x6e, 0x5a, 0x82),
                tab_text_active: Color32::from_rgb(0x2a, 0x12, 0x45),
                text: Color32::from_rgb(0x24, 0x18, 0x32),
                text_dim: Color32::from_rgb(0x6e, 0x5a, 0x82),
                text_faint: Color32::from_rgb(0x9c, 0x86, 0xb4),
                icon: Color32::from_rgb(0x5a, 0x44, 0x72),
                hover: Color32::from_rgb(0xea, 0xe0, 0xf5),
                pressed: Color32::from_rgb(0xdc, 0xcb, 0xee),
                field_bg: Color32::from_rgb(255, 255, 255),
                field_border: Color32::from_rgb(0xc9, 0xb6, 0xdd),
                separator: Color32::from_rgb(0xdc, 0xcb, 0xee),
                row_alt: Color32::from_rgb(0xf0, 0xe9, 0xf7),
                row_selected: Color32::from_rgb(0xd9, 0xc9, 0xf2),
                pill_active_bg: Color32::from_rgb(0xdc, 0xcb, 0xee),
                clip_bar_bg: Color32::from_rgb(0xd9, 0xc9, 0xf2),
                control_surface: Color32::from_rgb(0xef, 0xe7, 0xf7),
                control_well: Color32::from_rgb(0xdc, 0xcb, 0xee),
                control_border: Color32::from_rgb(0xb4, 0x9a, 0xc9),
                control_handle_bg: Color32::from_rgb(0xe6, 0xda, 0xf2),
                control_handle: Color32::from_rgb(0x2a, 0x12, 0x45),
                control_handle_dim: Color32::from_rgb(0x5a, 0x44, 0x72),
                plot_bg: Color32::from_rgb(0xfb, 0xf8, 0xfe),
                plot_grid: Color32::from_rgb(0xe6, 0xda, 0xf2),
                plot_axis: Color32::from_rgb(0xc9, 0xb6, 0xdd),
                plot_handle: Color32::from_rgb(0x2a, 0x12, 0x45),
                plot_handle_dim: Color32::from_rgb(0x6e, 0x5a, 0x82),
                meter_bg: Color32::from_rgb(0xdc, 0xcb, 0xee),
                meter_readout_bg: Color32::from_rgb(0xef, 0xe7, 0xf7),
                meter_clip_off: Color32::from_rgb(0xb4, 0x9a, 0xc9),
                panner_bg: Color32::from_rgb(0xdc, 0xcb, 0xee),
                panner_border: Color32::from_rgb(0xb4, 0x9a, 0xc9),
                panner_speaker: Color32::from_rgb(0x5a, 0x44, 0x72),
                fader_knob: Color32::from_rgb(0x8c, 0x7a, 0xa0),
                eq_track: Color32::from_rgb(0xdc, 0xcb, 0xee),
                eq_tick: Color32::from_rgb(0xb4, 0x9a, 0xc9),
                eq_knob: Color32::from_rgb(0x5a, 0x44, 0x72),
                eq_knob_active: Color32::from_rgb(0x2a, 0x12, 0x45),
                eq_node: Color32::from_rgb(0x2a, 0x12, 0x45),
                eq_node_off: Color32::from_rgb(0x8c, 0x7a, 0xa0),
                crossover_handle: Color32::from_rgb(0x2a, 0x12, 0x45),
                keyframe_plot_bg: Color32::from_rgb(0xfb, 0xf8, 0xfe),
                keyframe_plot_grid: Color32::from_rgb(0xe6, 0xda, 0xf2),
                keyframe_plot_axis: Color32::from_rgb(0xc9, 0xb6, 0xdd),
                keyframe_handle: Color32::from_rgb(0x2a, 0x12, 0x45),
                switch_fill: Color32::from_rgb(0x6e, 0x5a, 0x82),
                switch_knob: Color32::from_rgb(0xf7, 0xf3, 0xfb),
                switch_border: Color32::from_rgb(0x8c, 0x7a, 0xa0),
                slider_disabled_track: Color32::from_rgb(0xd9, 0xc9, 0xe3),
                slider_disabled_knob: Color32::from_rgb(0x9c, 0x86, 0xb4),
                button_hover: Color32::from_rgb(0xea, 0xe0, 0xf5),
                button_border: Color32::from_rgb(0xc9, 0xb6, 0xdd),
                icon_active: Color32::from_rgb(0x2a, 0x12, 0x45),
                caption_track_bg: Color32::from_rgb(0xef, 0xe7, 0xf7),
                scroll_thumb: Color32::from_rgb(0xc9, 0xb6, 0xdd),
                scroll_thumb_hover: Color32::from_rgb(0xb4, 0x9a, 0xc9),
                vertical_scroll_thumb: Color32::from_rgb(0xc9, 0xb6, 0xdd),
                disabled_clip_bg: Color32::from_rgb(0xdc, 0xcb, 0xee),
                in_out_shade: Color32::from_rgb(0xd9, 0xc9, 0xe3),
                tl_bg: Color32::from_rgb(0xef, 0xe7, 0xf7),
                tl_track_bg: Color32::from_rgb(0xf7, 0xf3, 0xfb),
                tl_track_bg_alt: Color32::from_rgb(0xfb, 0xf8, 0xfe),
                tl_header_bg: Color32::from_rgb(0xf3, 0xee, 0xf9),
                tl_ruler_bg: Color32::from_rgb(0xf7, 0xf3, 0xfb),
                tl_ruler_text: Color32::from_rgb(0x6e, 0x5a, 0x82),
                hot_text: Color32::from_rgb(0xc4, 0x2b, 0x86),
                focus: Color32::from_rgb(0x75, 0x31, 0xfe),
                playhead: Color32::from_rgb(0x75, 0x31, 0xfe),
                timecode: Color32::from_rgb(0x6b, 0x2b, 0xd9),
                danger: Color32::from_rgb(0xc4, 0x2b, 0x86),
                ..dark
            },
        }
    }

    /// Settings ▸ Appearance on top of a theme: the highlight colour (selections, primary buttons,
    /// the focus outline, the playhead, timecodes, drag values and track targeting) and accessible
    /// colour contrast (brighter secondary text and borders).
    pub fn with_appearance(mut self, highlight: Option<[u8; 3]>, accessible_contrast: bool) -> Self {
        if let Some([r, g, b]) = highlight {
            let c = Color32::from_rgb(r, g, b);
            // A highlight of the user's own also colours the theme's other highlight tokens: the
            // same hue, as much more or less saturated and bright as each of them is than the
            // theme's highlight. The theme's own highlight leaves them exactly as designed.
            if c != self.accent {
                let tint = |s: f32, v: f32| {
                    let mut h = egui::ecolor::HsvaGamma::from(c);
                    h.s = (h.s * s).clamp(0.0, 1.0);
                    h.v = (h.v * v).clamp(0.0, 1.0);
                    Color32::from(h)
                };
                self.focus = tint(0.92, 1.06);
                self.playhead = self.focus;
                self.timecode = tint(0.98, 1.09);
                self.hot_text = self.timecode;
                self.track_target = tint(1.0, 0.865);
                let lift = |v: u8| v.saturating_add(16);
                self.accent = c;
                self.accent_hover = Color32::from_rgb(lift(r), lift(g), lift(b));
            }
        }
        if accessible_contrast {
            if self.kind == ThemeKind::Light {
                self.text_dim = Color32::from_rgb(40, 40, 40);
                self.text_faint = Color32::from_rgb(80, 80, 80);
                self.field_border = Color32::from_rgb(110, 110, 110);
            } else {
                self.text_dim = self.text;
                self.text_faint = Color32::from_rgb(0xa8, 0xa8, 0xa8);
                self.tab_text = self.tab_text_active;
                self.field_border = Color32::from_rgb(0x6a, 0x6a, 0x6a);
                self.separator = Color32::from_rgb(0x50, 0x50, 0x50);
            }
        }
        self
    }

    /// Mono font for timecode.
    pub fn mono(size: f32) -> FontId {
        FontId::new(size, FontFamily::Monospace)
    }
    pub fn ui(size: f32) -> FontId {
        FontId::new(size, FontFamily::Proportional)
    }
    pub fn semibold(size: f32) -> FontId {
        FontId::new(size, FontFamily::Name("semibold".into()))
    }
    /// The big cyan timecode over the timeline and under the monitors: regular weight, as in
    /// Premiere (Premiere's reads 10 px tall and 76 px wide for 00:00:16:05 at 1x).
    pub fn timecode() -> FontId {
        FontId::new(16.0, FontFamily::Proportional)
    }
}

/// Menu text: Premiere's menus are the system's, larger and brighter than panel text, one item
/// every 24 px.
pub const MENU_TEXT_SIZE: f32 = 13.0;
pub const MENU_TEXT: Color32 = Color32::from_rgb(0xde, 0xde, 0xde);

/// Style of the menu bar's menus and their submenus (on top of egui's own menu style).
pub fn menu_style(s: &mut egui::Style) {
    egui::containers::menu::menu_style(s);
    s.text_styles.insert(TextStyle::Button, FontId::new(MENU_TEXT_SIZE, FontFamily::Proportional));
    s.text_styles.insert(TextStyle::Body, FontId::new(MENU_TEXT_SIZE, FontFamily::Proportional));
    s.spacing.item_spacing.y = 0.0;
    if s.visuals.dark_mode {
        s.visuals.override_text_color = Some(MENU_TEXT);
    }
}

/// Every font family the theme defines (fallback fonts such as the Japanese fonts are added to each
/// of them).
pub fn font_families() -> Vec<FontFamily> {
    vec![FontFamily::Proportional, FontFamily::Monospace, FontFamily::Name("semibold".into()), FontFamily::Name("medium".into())]
}

/// Install the Latin UI fonts, Chinese fallback (craft-fonts or an installed face), Japanese
/// craft-fonts, Arabic and Hebrew fallbacks (craft-fonts or installed faces, `crate::rtl`) and
/// egui visuals. Document names keep their glyphs in every interface language.
pub fn install(ctx: &egui::Context, t: &Tokens) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert("inter".into(), Arc::new(FontData::from_static(filmcraft_text::fonts::INTER_REGULAR)));
    fonts.font_data.insert("inter-medium".into(), Arc::new(FontData::from_static(filmcraft_text::fonts::INTER_MEDIUM)));
    fonts.font_data.insert("inter-semibold".into(), Arc::new(FontData::from_static(filmcraft_text::fonts::INTER_SEMIBOLD)));
    fonts.font_data.insert("jbmono".into(), Arc::new(FontData::from_static(filmcraft_text::fonts::JETBRAINS_MONO_REGULAR)));
    fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "inter".into());
    fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "jbmono".into());
    fonts.families.insert(FontFamily::Name("semibold".into()), vec!["inter-semibold".into(), "inter".into()]);
    fonts.families.insert(FontFamily::Name("medium".into()), vec!["inter-medium".into(), "inter".into()]);
    // Japanese before Chinese: the two share code points, and Japanese text must keep Japanese
    // glyph forms; Chinese faces still cover the hanzi Japanese fonts lack
    add_craft_fonts(&mut fonts);
    crate::cjk::install(&mut fonts);
    crate::rtl::install(&mut fonts);
    ctx.set_fonts(fonts);
    apply_visuals(ctx, t);
}

/// Name of the egui font for a craft-fonts entry.
fn craft_font_name(f: &filmcraft_text::fonts::CraftFont) -> String {
    format!("craft:{} {}", f.family, f.style)
}

/// Append the Japanese craft-fonts (empty unless built with `CRAFT_FONTS_DIR`) as the last fallbacks
/// of every font family, after the app's own fonts: BIZ UDPGothic first (bold before regular in
/// the "semibold" and "medium" families), then the other Japanese faces.
fn add_craft_fonts(fonts: &mut FontDefinitions) {
    let jpan: Vec<_> = filmcraft_text::fonts::craft_japanese().collect();
    for f in &jpan {
        fonts.font_data.insert(craft_font_name(f), Arc::new(FontData::from_static(f.bytes)));
    }
    for (family, stack) in fonts.families.iter_mut() {
        let heavy = matches!(family, FontFamily::Name(n) if matches!(n.as_ref(), "semibold" | "medium"));
        let mut order = jpan.clone();
        order.sort_by_key(|f| (!f.family.contains("Gothic"), (f.style == "Bold") != heavy));
        stack.extend(order.iter().map(|f| craft_font_name(f)));
    }
}

pub fn apply_visuals(ctx: &egui::Context, t: &Tokens) {
    let light = t.kind == ThemeKind::Light;
    // FilmCraft decides between light and dark itself (Settings ▸ Appearance ▸ Appearance Mode), so
    // egui must not switch to its other built-in style when the system appearance changes.
    ctx.set_theme(if light { egui::Theme::Light } else { egui::Theme::Dark });
    let mut v = if light { Visuals::light() } else { Visuals::dark() };
    v.panel_fill = t.panel_bg;
    v.window_fill = t.panel_bg;
    v.extreme_bg_color = t.field_bg;
    v.faint_bg_color = t.row_alt;
    v.override_text_color = Some(t.text);
    v.selection.bg_fill = t.accent;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.hyperlink_color = t.accent;
    v.window_stroke = Stroke::new(1.0, t.field_border);
    v.window_corner_radius = egui::CornerRadius::same(6);
    v.menu_corner_radius = egui::CornerRadius::same(6);
    v.popup_shadow = egui::epaint::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(140) };
    v.window_shadow = v.popup_shadow;
    for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
        w.corner_radius = egui::CornerRadius::same(t.radius_sm as u8);
    }
    v.widgets.noninteractive.bg_fill = t.panel_bg;
    v.widgets.noninteractive.weak_bg_fill = t.panel_bg;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.separator);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.inactive.bg_fill = t.field_bg;
    v.widgets.inactive.weak_bg_fill = t.field_bg;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, t.field_border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.hovered.bg_fill = t.hover;
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, t.field_border);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, t.tab_text_active);
    v.widgets.active.bg_fill = t.pressed;
    v.widgets.active.weak_bg_fill = t.pressed;
    v.widgets.active.fg_stroke = Stroke::new(1.0, t.tab_text_active);
    v.widgets.open.bg_fill = t.hover;
    v.widgets.open.weak_bg_fill = t.hover;
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
        s.spacing.interact_size.y = 24.0;
        s.spacing.menu_margin = egui::Margin::same(4);
        s.text_styles.insert(TextStyle::Body, FontId::new(12.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(12.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Small, FontId::new(11.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Heading, FontId::new(15.0, FontFamily::Name("semibold".into())));
        s.text_styles.insert(TextStyle::Monospace, FontId::new(12.0, FontFamily::Monospace));
        s.animation_time = ANIMATION_TIME;
    });
}

/// How long egui's fades and slides take (menus, collapsing sections, tooltips); 0 when
/// Settings ▸ Appearance ▸ "Animate zoom, scrolling and menus" is off (#2325).
pub const ANIMATION_TIME: f32 = 0.12;

#[cfg(test)]
mod tests {
    use super::*;

    const JAPANESE: &str = "日本語の文字";

    fn installed() -> egui::Context {
        let ctx = egui::Context::default();
        install(&ctx, &Tokens::for_kind(ThemeKind::default()));
        let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
        out.textures_delta.clear();
        ctx
    }

    #[test]
    fn light_theme_has_light_chrome_and_plot_surfaces() {
        let dark = Tokens::for_kind(ThemeKind::Dark);
        let light = Tokens::for_kind(ThemeKind::Light);
        let luminance = |c: Color32| u16::from(c.r()) + u16::from(c.g()) + u16::from(c.b());

        for (name, dark_surface, light_surface) in [
            ("active pill", dark.pill_active_bg, light.pill_active_bg),
            ("clip bar", dark.clip_bar_bg, light.clip_bar_bg),
            ("control surface", dark.control_surface, light.control_surface),
            ("control well", dark.control_well, light.control_well),
            ("plot", dark.plot_bg, light.plot_bg),
            ("keyframe plot", dark.keyframe_plot_bg, light.keyframe_plot_bg),
            ("panner", dark.panner_bg, light.panner_bg),
            ("caption lane", dark.caption_track_bg, light.caption_track_bg),
            ("active toggle", dark.pressed, light.pressed),
            ("meter", dark.meter_bg, light.meter_bg),
            ("meter readout", dark.meter_readout_bg, light.meter_readout_bg),
        ] {
            assert!(luminance(light_surface) > luminance(dark_surface), "{name} did not adapt to Light");
        }
        assert!(luminance(light.control_handle) < luminance(light.control_handle_bg));
        assert!(luminance(light.plot_handle) < luminance(light.plot_bg));
        assert!(luminance(light.icon_active) < luminance(light.pressed));
        assert_eq!(Tokens::for_kind(ThemeKind::Medium).plot_bg, dark.plot_bg, "Medium keeps the established dark plot treatment");
    }

    #[test]
    fn dark_and_medium_share_the_audio_and_effect_palette() {
        let dark = Tokens::for_kind(ThemeKind::Dark);
        let medium = Tokens::for_kind(ThemeKind::Medium);
        for (name, dark_value, medium_value) in [
            ("active pill", dark.pill_active_bg, medium.pill_active_bg),
            ("clip bar", dark.clip_bar_bg, medium.clip_bar_bg),
            ("control surface", dark.control_surface, medium.control_surface),
            ("control well", dark.control_well, medium.control_well),
            ("control handle", dark.control_handle, medium.control_handle),
            ("meter", dark.meter_bg, medium.meter_bg),
            ("plot", dark.plot_bg, medium.plot_bg),
            ("plot grid", dark.plot_grid, medium.plot_grid),
            ("keyframe plot", dark.keyframe_plot_bg, medium.keyframe_plot_bg),
            ("keyframe grid", dark.keyframe_plot_grid, medium.keyframe_plot_grid),
            ("keyframe axis", dark.keyframe_plot_axis, medium.keyframe_plot_axis),
            ("keyframe handle", dark.keyframe_handle, medium.keyframe_handle),
        ] {
            assert_eq!(dark_value, medium_value, "Medium changed the {name} colour");
        }
    }

    /// The colours the highlight drives besides the accent (#707).
    fn highlight_colours(t: &Tokens) -> [Color32; 5] {
        [t.focus, t.playhead, t.timecode, t.hot_text, t.track_target]
    }

    #[test]
    fn the_default_highlight_leaves_the_theme_colours_as_designed() {
        let default = filmcraft_engine::settings::parse_hex(filmcraft_engine::settings::DEFAULT_HIGHLIGHT);
        for k in [ThemeKind::Dark, ThemeKind::Medium, ThemeKind::Light] {
            assert_eq!(highlight_colours(&Tokens::for_kind(k).with_appearance(default, false)), highlight_colours(&Tokens::for_kind(k)), "{k:?}");
        }
    }

    #[test]
    fn a_highlight_of_ones_own_colours_the_playhead_timecodes_focus_and_targeting() {
        let pink = Color32::from_rgb(0xe0, 0x45, 0x7b);
        let t = Tokens::for_kind(ThemeKind::Dark).with_appearance(Some([0xe0, 0x45, 0x7b]), false);
        let hue = |c: Color32| egui::ecolor::HsvaGamma::from(c).h;
        for (c, was) in highlight_colours(&t).into_iter().zip(highlight_colours(&Tokens::for_kind(ThemeKind::Dark))) {
            assert_ne!(c, was);
            assert!((hue(c) - hue(pink)).abs() < 0.01, "{c:?} has the highlight's hue");
        }
        let v = |c: Color32| egui::ecolor::HsvaGamma::from(c).v;
        assert!(v(t.playhead) > v(pink) && v(t.timecode) > v(pink) && v(t.track_target) < v(pink), "lighter and darker as the default highlights are");
    }

    /// Chinese media and track names must render even when the interface stays in English.
    /// Compare the rasterized glyphs with the missing-glyph box, including custom weight families.
    #[test]
    fn chinese_names_render_in_every_theme_and_font_family() {
        const CHINESE: &str = "中文旁白音乐声轨测试简体汉语繁體漢語车";
        filmcraft_text::fonts::scan_system();
        let available = filmcraft_text::fonts::all_faces()
            .iter()
            .any(|f| matches!(f.info.origin, "system" | filmcraft_text::fonts::CRAFT_ORIGIN) && f.covers_text(CHINESE));
        if !available {
            eprintln!("SKIPPED: no craft-fonts or installed Chinese font");
            return;
        }
        let ctx = egui::Context::default();
        for kind in [ThemeKind::Dark, ThemeKind::Medium, ThemeKind::Light] {
            install(&ctx, &Tokens::for_kind(kind));
            ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
            ctx.fonts_mut(|fonts| {
                for family in font_families() {
                    let font = FontId::new(13.0, family);
                    let missing = fonts.layout_no_wrap("\u{fffd}".into(), font.clone(), Color32::WHITE);
                    let missing_uv = missing.rows[0].glyphs[0].uv_rect;
                    let names = fonts.layout_no_wrap(CHINESE.into(), font.clone(), Color32::WHITE);
                    assert_eq!(names.rows.iter().map(|r| r.glyphs.len()).sum::<usize>(), CHINESE.chars().count());
                    for glyph in names.rows.iter().flat_map(|r| &r.glyphs) {
                        assert!(!glyph.uv_rect.is_nothing(), "empty {} in {font:?}", glyph.chr);
                        assert_ne!(glyph.uv_rect, missing_uv, "missing {} in {kind:?} / {font:?}", glyph.chr);
                    }
                }
            });
        }
        // the layouts above put glyphs in the atlas: take that update so it isn't dropped unapplied
        ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
    }

    /// Typed, pasted or IME-composed Hangul draws real glyphs, not the missing-glyph box (artcraft#2093).
    #[test]
    fn korean_text_renders_with_an_installed_korean_font() {
        const KOREAN: &str = "한국어 입력 테스트";
        filmcraft_text::fonts::scan_system();
        // `covers_text`, not `has_char`: probing keeps no font file loaded (see `cjk::find_system_font`)
        let available = filmcraft_text::fonts::all_faces().iter().any(|f| f.info.origin == "system" && f.covers_text(KOREAN));
        if !available {
            eprintln!("SKIPPED: no installed Korean font");
            return;
        }
        let ctx = egui::Context::default();
        install(&ctx, &Tokens::for_kind(ThemeKind::Dark));
        ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
        ctx.fonts_mut(|fonts| {
            for family in font_families() {
                let font = FontId::new(13.0, family);
                let missing = fonts.layout_no_wrap("\u{fffd}".into(), font.clone(), Color32::WHITE);
                let missing_uv = missing.rows[0].glyphs[0].uv_rect;
                let text = fonts.layout_no_wrap(KOREAN.into(), font.clone(), Color32::WHITE);
                for glyph in text.rows.iter().flat_map(|r| &r.glyphs).filter(|g| g.chr != ' ') {
                    assert_ne!(glyph.uv_rect, missing_uv, "missing {} in {font:?}", glyph.chr);
                }
            }
        });
        ctx.run_ui(egui::RawInput::default(), |_| {}).textures_delta.clear();
    }

    /// Built with craft-fonts: every font family ends with the Japanese faces (BIZ UDPGothic
    /// first), and Japanese text gets real glyphs, not the replacement box.
    #[test]
    fn japanese_renders_with_craft_fonts() {
        if filmcraft_text::fonts::craft_japanese().next().is_none() {
            eprintln!("SKIPPED: built without craft-fonts (set CRAFT_FONTS_DIR to run)");
            return;
        }
        let ctx = installed();
        ctx.fonts_mut(|fonts| {
            let defs = fonts.definitions().clone();
            for (family, stack) in &defs.families {
                // installed faces come after the craft run: right-to-left ones (Hebrew is not in
                // craft-fonts) and the Hangul face (none is in craft-fonts)
                let stack: Vec<&String> = stack
                    .iter()
                    .filter(|n| !crate::rtl::is_system_face(n) && n.as_str() != crate::cjk::KOREAN_FACE && n.as_str() != crate::cjk::THAI_FACE)
                    .collect();
                let first = stack.iter().position(|n| n.starts_with("craft:")).unwrap_or(stack.len());
                assert!(first > 0 && stack[first..].iter().all(|n| n.starts_with("craft:")), "{family:?}: {stack:?}");
                let first_japanese = stack.iter().find(|n| n.starts_with("craft:BIZ") || n.starts_with("craft:Shippori"));
                assert!(first_japanese.is_some_and(|n| n.starts_with("craft:BIZ UDPGothic")), "{family:?}: {stack:?}");
            }
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                let font = FontId::new(13.0, family);
                for ch in JAPANESE.chars() {
                    assert!(fonts.has_glyph(&font, ch), "missing {ch} in {font:?}");
                }
            }
        });
        // laid out and drawn: the replacement box (U+FFFD) would show as one glyph per character
        // from the fallback face; a real face gives each character its own width
        let galley = ctx.fonts_mut(|f| f.layout_no_wrap(JAPANESE.into(), FontId::new(13.0, FontFamily::Proportional), Color32::WHITE));
        assert_eq!(galley.rows.iter().map(|r| r.glyphs.len()).sum::<usize>(), JAPANESE.chars().count());
        assert!(galley.size().x > 13.0 * 4.0, "{:?}", galley.size());
    }

    /// Latin text keeps its UI faces, with or without craft-fonts and system Chinese fonts.
    #[test]
    fn works_without_craft_fonts() {
        let ctx = installed();
        let n = filmcraft_text::fonts::craft_japanese().count()
            + crate::cjk::craft_fonts().count()
            + crate::rtl::craft_fonts().count()
            + crate::cjk::thai_craft_fonts().count();
        ctx.fonts_mut(|fonts| {
            let defs = fonts.definitions().clone();
            assert_eq!(defs.font_data.keys().filter(|k| k.starts_with("craft:")).count(), n);
            assert_eq!(defs.families[&FontFamily::Proportional].first().map(String::as_str), Some("inter"));
            assert_eq!(defs.families[&FontFamily::Monospace].first().map(String::as_str), Some("jbmono"));
            assert!(fonts.has_glyph(&FontId::new(13.0, FontFamily::Proportional), 'A'));
            if n == 0 {
                assert!(defs.families.values().flatten().all(|name| !name.starts_with("craft:")));
            }
        });
    }
}
