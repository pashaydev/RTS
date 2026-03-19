use bevy::ecs::message::MessageReader;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::components::*;
use crate::theme;
use crate::ui::fonts::{self, UiFonts};

// ── Shared Components ──

#[derive(Component)]
pub struct MenuSelector {
    pub field: SelectorField,
    pub index: usize,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SelectorField {
    AiCount,
    HostAiCount,
    AiDifficulty(usize),
    TeamMode,
    MapSize,
    ResourceDensity,
    DayCycle,
    StartingRes,
    MapSeed,
    Resolution,
    Fullscreen,
    Shadows,
    EntityLights,
    UiScale,
    PlayerColor,
    HostPlayerColor,
}

#[derive(Component)]
pub struct SelectedOption;

#[derive(Component)]
pub struct SeedDisplay;

#[derive(Component)]
pub struct RandomizeSeedButton;

#[derive(Component)]
pub struct MenuPageContainer;

// ── Scroll ──

pub const MENU_SCROLL_LINE_HEIGHT: f32 = 24.0;

pub fn menu_scroll_system(
    mut mouse_wheel: MessageReader<MouseWheel>,
    windows: Query<&Window>,
    mut panel_q: Query<
        (&mut ScrollPosition, &ComputedNode, &UiGlobalTransform),
        With<MenuPageContainer>,
    >,
) {
    let mut dy = 0.0;
    for ev in mouse_wheel.read() {
        dy += match ev.unit {
            MouseScrollUnit::Line => -ev.y * MENU_SCROLL_LINE_HEIGHT,
            MouseScrollUnit::Pixel => -ev.y,
        };
    }

    if dy.abs() < 0.001 {
        return;
    }

    let Some(cursor_phys) = windows
        .single()
        .ok()
        .and_then(|w| w.physical_cursor_position())
    else {
        return;
    };

    for (mut scroll_pos, computed, ui_tf) in &mut panel_q {
        if !computed.contains_point(*ui_tf, cursor_phys) {
            continue;
        }
        let max_scroll = (computed.content_size().y - computed.size().y).max(0.0)
            * computed.inverse_scale_factor();
        scroll_pos.y = (scroll_pos.y + dy).clamp(0.0, max_scroll);
    }
}

// ── Panel ──

pub fn spawn_menu_panel(commands: &mut Commands) -> Entity {
    commands
        .spawn((
            MenuPageContainer,
            Interaction::None,
            ScrollPosition::default(),
            Node {
                width: Val::Px(560.0),
                max_height: Val::Percent(90.0),
                flex_direction: FlexDirection::Column,
                align_items: AlignItems::Center,
                padding: UiRect::all(Val::Px(24.0)),
                overflow: Overflow::scroll_y(),
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.07, 0.07, 0.07, 0.0)),
            BorderColor::all(theme::SEPARATOR),
            BoxShadow::new(
                Color::srgba(0.0, 0.0, 0.0, 0.6),
                Val::Px(0.0),
                Val::Px(4.0),
                Val::Px(0.0),
                Val::Px(24.0),
            ),
            UiFadeIn {
                timer: Timer::from_seconds(0.3, TimerMode::Once),
            },
            UiScaleIn {
                from: 0.96,
                timer: Timer::from_seconds(0.3, TimerMode::Once),
                elastic: false,
            },
        ))
        .id()
}

// ── Styled Button ──

/// Spawn a styled menu button with any marker component.
pub fn spawn_styled_button(
    commands: &mut Commands,
    label: &str,
    marker: impl Bundle,
    accent: bool,
    fonts: &UiFonts,
) -> Entity {
    let bg = if accent {
        theme::ACCENT
    } else {
        theme::BTN_PRIMARY
    };

    let mut entity_commands = commands.spawn((
        marker,
        Button,
        ButtonAnimState::new(bg.to_srgba().to_f32_array()),
        ButtonStyle::Filled,
        Node {
            width: Val::Px(240.0),
            height: Val::Px(44.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(4.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(bg),
        BorderColor::all(Color::NONE),
    ));
    if accent {
        entity_commands.insert((
            UiGlowPulse {
                color: theme::ACCENT,
                intensity: 0.5,
            },
            BoxShadow::new(
                Color::srgba(0.29, 0.62, 1.0, 0.2),
                Val::Px(0.0),
                Val::Px(0.0),
                Val::Px(0.0),
                Val::Px(6.0),
            ),
        ));
    }
    entity_commands.with_children(|parent| {
        parent.spawn((
            Text::new(label),
            fonts::heading(fonts, theme::FONT_BUTTON),
            TextColor(Color::WHITE),
            Pickable::IGNORE,
        ));
    });
    entity_commands.id()
}

// ── Page Header ──

pub fn spawn_page_header<B: Bundle>(
    commands: &mut Commands,
    container: Entity,
    title: &str,
    back_marker: B,
    fonts: &UiFonts,
) {
    let row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            margin: UiRect::bottom(Val::Px(16.0)),
            ..default()
        })
        .with_children(|parent| {
            parent
                .spawn((
                    back_marker,
                    Button,
                    ButtonAnimState::new([0.0, 0.0, 0.0, 0.0]),
                    ButtonStyle::Ghost,
                    Node {
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("<< BACK"),
                        fonts::body_emphasis(fonts, theme::FONT_MEDIUM),
                        TextColor(theme::TEXT_SECONDARY),
                        Pickable::IGNORE,
                    ));
                });

            parent.spawn((
                Text::new(title),
                fonts::heading(fonts, theme::FONT_HEADING),
                TextColor(Color::WHITE),
            ));
        })
        .id();
    commands.entity(container).add_child(row);
}

// ── Section Divider ──

pub fn spawn_animated_section_divider(
    commands: &mut Commands,
    container: Entity,
    label: &str,
    fonts: &UiFonts,
) {
    let row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(10.0)),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                UiLineExpand {
                    target_width: 40.0,
                    timer: Timer::from_seconds(0.4, TimerMode::Once),
                },
                Node {
                    width: Val::Px(0.0),
                    height: Val::Px(1.0),
                    margin: UiRect::right(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(theme::SEPARATOR),
            ));

            parent.spawn((
                Text::new(label),
                fonts::heading(fonts, theme::FONT_SMALL),
                TextColor(theme::TEXT_SECONDARY),
                Node {
                    margin: UiRect::horizontal(Val::Px(4.0)),
                    ..default()
                },
            ));

            parent.spawn((
                UiLineExpand {
                    target_width: 400.0,
                    timer: Timer::from_seconds(0.5, TimerMode::Once),
                },
                Node {
                    width: Val::Px(0.0),
                    height: Val::Px(1.0),
                    flex_grow: 1.0,
                    margin: UiRect::left(Val::Px(8.0)),
                    ..default()
                },
                BackgroundColor(theme::SEPARATOR),
            ));
        })
        .id();
    commands.entity(container).add_child(row);
}

// ── Selector Row ──

pub fn spawn_selector_row(
    commands: &mut Commands,
    container: Entity,
    label: &str,
    options: &[&str],
    selected: usize,
    field: SelectorField,
) {
    let row = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(6.0)),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text::new(label),
                TextFont {
                    font_size: theme::FONT_MEDIUM,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
                Node {
                    width: Val::Px(120.0),
                    ..default()
                },
            ));

            for (i, &opt) in options.iter().enumerate() {
                let is_selected = i == selected;
                let bg = if is_selected {
                    theme::ACCENT
                } else {
                    theme::BTN_PRIMARY
                };
                let text_color = if is_selected {
                    Color::WHITE
                } else {
                    theme::TEXT_SECONDARY
                };

                let mut btn = parent.spawn((
                    MenuSelector { field, index: i },
                    Button,
                    ButtonAnimState::new(bg.to_srgba().to_f32_array()),
                    ButtonStyle::Filled,
                    Node {
                        padding: UiRect::axes(Val::Px(14.0), Val::Px(7.0)),
                        margin: UiRect::horizontal(Val::Px(2.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        ..default()
                    },
                    BackgroundColor(bg),
                    BorderColor::all(if is_selected {
                        Color::srgba(0.29, 0.62, 1.0, 0.3)
                    } else {
                        Color::NONE
                    }),
                ));
                if is_selected {
                    btn.insert(SelectedOption);
                }
                btn.with_children(|btn_parent| {
                    btn_parent.spawn((
                        Text::new(opt),
                        TextFont {
                            font_size: theme::FONT_MEDIUM,
                            ..default()
                        },
                        TextColor(text_color),
                        Pickable::IGNORE,
                    ));
                });
            }
        })
        .id();
    commands.entity(container).add_child(row);
}

// ── Name Input Row ──

pub fn spawn_name_input_row(commands: &mut Commands, current_name: &str) -> Entity {
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(6.0)),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text::new("Name:"),
                TextFont {
                    font_size: theme::FONT_MEDIUM,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
                Node {
                    width: Val::Px(120.0),
                    ..default()
                },
            ));

            parent
                .spawn((
                    TextInputField {
                        value: current_name.to_string(),
                        cursor_pos: current_name.len(),
                        max_len: 45,
                    },
                    Button,
                    Node {
                        width: Val::Px(280.0),
                        height: Val::Px(32.0),
                        padding: UiRect::axes(Val::Px(8.0), Val::Px(4.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        align_items: AlignItems::Center,
                        overflow: Overflow::clip(),
                        ..default()
                    },
                    BackgroundColor(theme::INPUT_BG),
                    BorderColor::all(theme::INPUT_BORDER),
                ))
                .with_children(|input| {
                    input.spawn((
                        Text::new(current_name),
                        TextFont {
                            font_size: theme::FONT_MEDIUM,
                            ..default()
                        },
                        TextColor(theme::TEXT_PRIMARY),
                        Pickable::IGNORE,
                    ));
                    input.spawn((
                        TextInputCursor,
                        Text::new("|"),
                        TextFont {
                            font_size: theme::FONT_MEDIUM,
                            ..default()
                        },
                        TextColor(Color::NONE),
                        Pickable::IGNORE,
                    ));
                });

            parent
                .spawn((
                    RandomNameButton,
                    Button,
                    ButtonAnimState::new(theme::BTN_PRIMARY.to_srgba().to_f32_array()),
                    ButtonStyle::Ghost,
                    Node {
                        padding: UiRect::axes(Val::Px(10.0), Val::Px(6.0)),
                        margin: UiRect::left(Val::Px(6.0)),
                        ..default()
                    },
                    BackgroundColor(Color::NONE),
                ))
                .with_children(|btn| {
                    btn.spawn((
                        Text::new("Random"),
                        TextFont {
                            font_size: theme::FONT_MEDIUM,
                            ..default()
                        },
                        TextColor(theme::ACCENT),
                        Pickable::IGNORE,
                    ));
                });
        })
        .id()
}

// ── Color Picker ──

pub fn spawn_color_picker(commands: &mut Commands, selected: usize, field: SelectorField) -> Entity {
    let colors = [
        Faction::Player1.color(),
        Faction::Player2.color(),
        Faction::Player3.color(),
        Faction::Player4.color(),
    ];

    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            align_items: AlignItems::Center,
            margin: UiRect::vertical(Val::Px(6.0)),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text::new("Color:"),
                TextFont {
                    font_size: theme::FONT_MEDIUM,
                    ..default()
                },
                TextColor(theme::TEXT_SECONDARY),
                Node {
                    width: Val::Px(120.0),
                    ..default()
                },
            ));

            for (i, &color) in colors.iter().enumerate() {
                let is_selected = i == selected;
                let size = if is_selected { 36.0 } else { 32.0 };
                let border_color = if is_selected {
                    Color::WHITE
                } else {
                    Color::NONE
                };
                let border_width = if is_selected { 3.0 } else { 2.0 };

                let mut dot = parent.spawn((
                    MenuSelector {
                        field,
                        index: i,
                    },
                    Button,
                    Node {
                        width: Val::Px(size),
                        height: Val::Px(size),
                        margin: UiRect::horizontal(Val::Px(5.0)),
                        border: UiRect::all(Val::Px(border_width)),
                        ..default()
                    },
                    BackgroundColor(color),
                    BorderColor::all(border_color),
                ));
                if is_selected {
                    let glow_color = color.to_srgba();
                    dot.insert((
                        BoxShadow::new(
                            Color::srgba(glow_color.red, glow_color.green, glow_color.blue, 0.5),
                            Val::Px(0.0),
                            Val::Px(0.0),
                            Val::Px(0.0),
                            Val::Px(8.0),
                        ),
                        SelectedOption,
                        UiGlowPulse {
                            color,
                            intensity: 0.8,
                        },
                    ));
                }
            }
        })
        .id()
}

// ── Text Input System ──

pub fn text_input_system(
    mut inputs: Query<(
        Entity,
        &mut TextInputField,
        &Interaction,
        &Children,
        Option<&TextInputFocused>,
        Option<&crate::menu::SessionCodeInput>,
    )>,
    mut commands: Commands,
    mut config: ResMut<GameSetupConfig>,
    mut keyboard_events: MessageReader<KeyboardInput>,
    mut text_query: Query<&mut Text, Without<TextInputCursor>>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    let mut clicked_entity: Option<Entity> = None;
    for (entity, _, interaction, _, _, _) in &inputs {
        if *interaction == Interaction::Pressed {
            clicked_entity = Some(entity);
        }
    }

    if let Some(clicked) = clicked_entity {
        for (entity, _, _, _, focused, _) in &inputs {
            if entity == clicked {
                if focused.is_none() {
                    commands.entity(entity).insert(TextInputFocused);
                    commands
                        .entity(entity)
                        .insert(BorderColor::all(theme::INPUT_BORDER_FOCUSED));
                }
            } else if focused.is_some() {
                commands.entity(entity).remove::<TextInputFocused>();
                commands
                    .entity(entity)
                    .insert(BorderColor::all(theme::INPUT_BORDER));
            }
        }
    }

    // On WASM, the browser `paste` event may fire independently of keyboard events.
    // Drain any pending paste buffer into the focused input each frame.
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(clip) = clipboard_read() {
            for (_entity, mut field, _, children, focused, is_session_code) in &mut inputs {
                if focused.is_none() {
                    continue;
                }
                for ch in clip.chars() {
                    if field.value.len() >= field.max_len {
                        break;
                    }
                    if ch.is_ascii_graphic() || ch == ' ' {
                        let pos = field.cursor_pos;
                        field.value.insert(pos, ch);
                        field.cursor_pos += 1;
                    }
                }
                // Update displayed text
                for child in children.iter() {
                    if let Ok(mut text) = text_query.get_mut(child) {
                        **text = field.value.clone();
                    }
                }
                // Sync to config if this is a player name input
                if is_session_code.is_none() {
                    config.player_name = field.value.clone();
                }
                break; // Only paste into first focused input
            }
        }
    }

    let events: Vec<_> = keyboard_events.read().cloned().collect();
    if events.is_empty() {
        return;
    }

    let cmd_key = keys.pressed(KeyCode::SuperLeft)
        || keys.pressed(KeyCode::SuperRight)
        || keys.pressed(KeyCode::ControlLeft)
        || keys.pressed(KeyCode::ControlRight);

    for (entity, mut field, _, children, focused, is_session_code) in &mut inputs {
        if focused.is_none() {
            continue;
        }
        for event in &events {
            if !event.state.is_pressed() {
                continue;
            }

            let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

            if cmd_key && event.key_code == KeyCode::KeyV {
                if let Some(clip) = clipboard_read() {
                    for ch in clip.chars() {
                        if field.value.len() >= field.max_len {
                            break;
                        }
                        if ch.is_ascii_graphic() || ch == ' ' {
                            let pos = field.cursor_pos;
                            field.value.insert(pos, ch);
                            field.cursor_pos += 1;
                        }
                    }
                }
                continue;
            }

            if cmd_key && event.key_code == KeyCode::KeyC {
                clipboard_write(&field.value);
                continue;
            }

            if cmd_key && event.key_code == KeyCode::KeyA {
                field.cursor_pos = field.value.len();
                continue;
            }

            match event.key_code {
                KeyCode::Backspace => {
                    if field.cursor_pos > 0 {
                        field.cursor_pos -= 1;
                        let pos = field.cursor_pos;
                        field.value.remove(pos);
                    }
                }
                KeyCode::Delete => {
                    let pos = field.cursor_pos;
                    if pos < field.value.len() {
                        field.value.remove(pos);
                    }
                }
                KeyCode::ArrowLeft => {
                    if field.cursor_pos > 0 {
                        field.cursor_pos -= 1;
                    }
                }
                KeyCode::ArrowRight => {
                    if field.cursor_pos < field.value.len() {
                        field.cursor_pos += 1;
                    }
                }
                KeyCode::Home => {
                    field.cursor_pos = 0;
                }
                KeyCode::End => {
                    field.cursor_pos = field.value.len();
                }
                KeyCode::Enter | KeyCode::Escape => {
                    commands.entity(entity).remove::<TextInputFocused>();
                    commands
                        .entity(entity)
                        .insert(BorderColor::all(theme::INPUT_BORDER));
                }
                KeyCode::Space => {
                    if field.value.len() < field.max_len {
                        let pos = field.cursor_pos;
                        field.value.insert(pos, ' ');
                        field.cursor_pos += 1;
                    }
                }
                code => {
                    if let Some(ch) = keycode_to_char(code, shift) {
                        if field.value.len() < field.max_len {
                            let pos = field.cursor_pos;
                            field.value.insert(pos, ch);
                            field.cursor_pos += 1;
                        }
                    }
                }
            }
        }

        if is_session_code.is_none() {
            config.player_name = field.value.clone();
        }
        for child in children.iter() {
            if let Ok(mut text) = text_query.get_mut(child) {
                **text = field.value.clone();
            }
        }
    }
}

pub fn keycode_to_char(code: KeyCode, shift: bool) -> Option<char> {
    let ch = match code {
        KeyCode::KeyA => 'a',
        KeyCode::KeyB => 'b',
        KeyCode::KeyC => 'c',
        KeyCode::KeyD => 'd',
        KeyCode::KeyE => 'e',
        KeyCode::KeyF => 'f',
        KeyCode::KeyG => 'g',
        KeyCode::KeyH => 'h',
        KeyCode::KeyI => 'i',
        KeyCode::KeyJ => 'j',
        KeyCode::KeyK => 'k',
        KeyCode::KeyL => 'l',
        KeyCode::KeyM => 'm',
        KeyCode::KeyN => 'n',
        KeyCode::KeyO => 'o',
        KeyCode::KeyP => 'p',
        KeyCode::KeyQ => 'q',
        KeyCode::KeyR => 'r',
        KeyCode::KeyS => 's',
        KeyCode::KeyT => 't',
        KeyCode::KeyU => 'u',
        KeyCode::KeyV => 'v',
        KeyCode::KeyW => 'w',
        KeyCode::KeyX => 'x',
        KeyCode::KeyY => 'y',
        KeyCode::KeyZ => 'z',
        KeyCode::Digit0 => return if shift { Some(')') } else { Some('0') },
        KeyCode::Digit1 => return if shift { Some('!') } else { Some('1') },
        KeyCode::Digit2 => return if shift { Some('@') } else { Some('2') },
        KeyCode::Digit3 => return if shift { Some('#') } else { Some('3') },
        KeyCode::Digit4 => return if shift { Some('$') } else { Some('4') },
        KeyCode::Digit5 => return if shift { Some('%') } else { Some('5') },
        KeyCode::Digit6 => return if shift { Some('^') } else { Some('6') },
        KeyCode::Digit7 => return if shift { Some('&') } else { Some('7') },
        KeyCode::Digit8 => return if shift { Some('*') } else { Some('8') },
        KeyCode::Digit9 => return if shift { Some('(') } else { Some('9') },
        KeyCode::Minus => return if shift { Some('_') } else { Some('-') },
        KeyCode::Period => return if shift { Some('>') } else { Some('.') },
        KeyCode::Semicolon => return if shift { Some(':') } else { Some(';') },
        KeyCode::Slash => return if shift { Some('?') } else { Some('/') },
        KeyCode::BracketLeft => return if shift { Some('{') } else { Some('[') },
        KeyCode::BracketRight => return if shift { Some('}') } else { Some(']') },
        KeyCode::Backquote => return if shift { Some('~') } else { Some('`') },
        KeyCode::Equal => return if shift { Some('+') } else { Some('=') },
        KeyCode::Backslash => return if shift { Some('|') } else { Some('\\') },
        KeyCode::Quote => return if shift { Some('"') } else { Some('\'') },
        KeyCode::Comma => return if shift { Some('<') } else { Some(',') },
        _ => return None,
    };
    if shift && ch.is_ascii_alphabetic() {
        Some(ch.to_ascii_uppercase())
    } else {
        Some(ch)
    }
}

// ── Clipboard helpers ──

/// Shared buffer for WASM paste events. The browser `paste` listener writes here,
/// and `clipboard_read()` drains it.
#[cfg(target_arch = "wasm32")]
mod wasm_clipboard {
    use std::sync::{Mutex, OnceLock};
    use wasm_bindgen::prelude::*;

    static PASTE_BUFFER: OnceLock<Mutex<Option<String>>> = OnceLock::new();

    fn buffer() -> &'static Mutex<Option<String>> {
        PASTE_BUFFER.get_or_init(|| {
            // Install a document-level paste listener once
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                let cb = Closure::wrap(Box::new(|e: web_sys::ClipboardEvent| {
                    if let Some(dt) = e.clipboard_data() {
                        if let Ok(text) = dt.get_data("text/plain") {
                            if !text.is_empty() {
                                if let Ok(mut buf) = PASTE_BUFFER
                                    .get()
                                    .expect("already init")
                                    .lock()
                                {
                                    *buf = Some(text);
                                }
                            }
                        }
                    }
                    e.prevent_default();
                }) as Box<dyn FnMut(web_sys::ClipboardEvent)>);
                let _ = doc.add_event_listener_with_callback("paste", cb.as_ref().unchecked_ref());
                cb.forget();
            }
            Mutex::new(None)
        })
    }

    pub fn read() -> Option<String> {
        buffer().lock().ok().and_then(|mut buf| buf.take())
    }

    pub fn write(text: &str) {
        if let Some(w) = web_sys::window() {
            let clip = w.navigator().clipboard();
            let _ = clip.write_text(text);
        }
    }
}

pub fn clipboard_read() -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::process::Command::new("pbpaste")
            .output()
            .ok()
            .and_then(|o| {
                if o.status.success() {
                    String::from_utf8(o.stdout).ok()
                } else {
                    std::process::Command::new("xclip")
                        .args(["-selection", "clipboard", "-o"])
                        .output()
                        .ok()
                        .and_then(|o2| String::from_utf8(o2.stdout).ok())
                }
            })
    }
    #[cfg(target_arch = "wasm32")]
    {
        wasm_clipboard::read()
    }
}

pub fn clipboard_write(text: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::io::Write;
        if let Ok(mut child) = std::process::Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()
        {
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(text.as_bytes());
            }
            let _ = child.wait();
        } else {
            if let Ok(mut child) = std::process::Command::new("xclip")
                .args(["-selection", "clipboard"])
                .stdin(std::process::Stdio::piped())
                .spawn()
            {
                if let Some(ref mut stdin) = child.stdin {
                    let _ = stdin.write_all(text.as_bytes());
                }
                let _ = child.wait();
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        wasm_clipboard::write(text);
    }
}

// ── Text Input Cursor Blink ──

pub fn text_input_cursor_blink(
    time: Res<Time>,
    focused: Query<&Children, With<TextInputFocused>>,
    not_focused: Query<&Children, (With<TextInputField>, Without<TextInputFocused>)>,
    mut cursors: Query<&mut TextColor, With<TextInputCursor>>,
) {
    for children in &focused {
        for child in children.iter() {
            if let Ok(mut color) = cursors.get_mut(child) {
                let t = time.elapsed_secs();
                let blink = (t * 3.0).sin() * 0.5 + 0.5;
                let c = theme::ACCENT.to_srgba();
                color.0 = Color::srgba(c.red, c.green, c.blue, blink);
            }
        }
    }
    for children in &not_focused {
        for child in children.iter() {
            if let Ok(mut color) = cursors.get_mut(child) {
                color.0 = Color::NONE;
            }
        }
    }
}
