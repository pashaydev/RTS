use bevy::prelude::*;
use bevy::text::Font;

#[derive(Resource, Clone)]
pub struct UiFonts {
    pub heading: Handle<Font>,
    pub body: Handle<Font>,
    pub body_emphasis: Handle<Font>,
    pub edit_icon: Handle<Image>,
    pub close_icon: Handle<Image>,
    pub sliders_icon: Handle<Image>,
    pub terminal_icon: Handle<Image>,
}

impl FromWorld for UiFonts {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>();
        Self {
            heading: asset_server.load("fonts/Oxanium.ttf"),
            body: asset_server.load("fonts/Rajdhani-Medium.ttf"),
            body_emphasis: asset_server.load("fonts/Rajdhani-SemiBold.ttf"),
            edit_icon: asset_server.load("icons/edit_pencil.png"),
            close_icon: asset_server.load("icons/close_cross.png"),
            sliders_icon: asset_server.load("icons/sliders.png"),
            terminal_icon: asset_server.load("icons/terminal.png"),
        }
    }
}

pub fn apply_default_fonts(fonts: Res<UiFonts>, mut query: Query<&mut TextFont, Added<TextFont>>) {
    for mut text_font in &mut query {
        if text_font.font == Handle::default() {
            text_font.font = fonts.body.clone();
        }
    }
}

pub fn heading(fonts: &UiFonts, font_size: f32) -> TextFont {
    TextFont {
        font: fonts.heading.clone(),
        font_size,
        ..default()
    }
}

pub fn body(fonts: &UiFonts, font_size: f32) -> TextFont {
    TextFont {
        font: fonts.body.clone(),
        font_size,
        ..default()
    }
}

pub fn body_emphasis(fonts: &UiFonts, font_size: f32) -> TextFont {
    TextFont {
        font: fonts.body_emphasis.clone(),
        font_size,
        ..default()
    }
}

pub fn toolbar(fonts: &UiFonts) -> TextFont {
    // caption = Typography::default().caption (10.0)
    body_emphasis(fonts, 10.0)
}
