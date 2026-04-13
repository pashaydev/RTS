//! Generic text input, scroll, and clipboard infrastructure.
//! Used by both menu and in-game UI.
//!
//! Supports text selection (Shift+Arrow, Ctrl+A), clipboard (Ctrl+C/X/V),
//! and works cross-platform (macOS, Linux, Windows, WASM).

use bevy::ecs::hierarchy::ChildSpawnerCommands;
use bevy::ecs::message::MessageReader;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;

use crate::types::*;
use crate::ui::theme::{self, Theme};

use super::interactions::UiClickEvent;

// ── Constants ──

const SELECTION_COLOR: Color = theme::HIGHLIGHT;

// ── Scroll ──

#[derive(Component)]
pub struct ScrollablePanel;

pub const SCROLL_LINE_HEIGHT: f32 = 24.0;

pub fn scroll_panel_system(
    mut mouse_wheel: MessageReader<MouseWheel>,
    windows: Query<&Window>,
    mut panel_q: Query<
        (&mut ScrollPosition, &ComputedNode, &UiGlobalTransform),
        With<ScrollablePanel>,
    >,
) {
    let mut dy = 0.0;
    for ev in mouse_wheel.read() {
        dy += match ev.unit {
            MouseScrollUnit::Line => -ev.y * SCROLL_LINE_HEIGHT,
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

// ── Text Input Spawn Helper ──

/// Spawns the rich-text children for a `TextInputField` entity.
/// Structure: Text root (pre-text) → TextSpan children (sel_before, cursor, sel_after, post).
pub fn spawn_text_input_children(
    builder: &mut ChildSpawnerCommands,
    initial_value: &str,
    theme: &Theme,
) {
    let font = TextFont {
        font_size: theme.typography.medium,
        ..default()
    };
    builder
        .spawn((
            Text::new(initial_value),
            font.clone(),
            TextColor(theme.colors.text_primary),
            Pickable::IGNORE,
        ))
        .with_children(|text_root| {
            text_root.spawn((
                TextInputSelBefore,
                TextSpan::new(""),
                font.clone(),
                TextColor(Color::NONE),
            ));
            text_root.spawn((
                TextInputCursor,
                TextSpan::new("|"),
                font.clone(),
                TextColor(Color::NONE),
            ));
            text_root.spawn((
                TextInputSelAfter,
                TextSpan::new(""),
                font.clone(),
                TextColor(Color::NONE),
            ));
            text_root.spawn((
                TextInputPostText,
                TextSpan::new(""),
                font,
                TextColor(theme.colors.text_primary),
            ));
        });
}

// ── Text Input System ──

pub fn text_input_system(
    mut click_events: MessageReader<UiClickEvent>,
    input_targets: Query<(), With<TextInputField>>,
    mut inputs: Query<(
        Entity,
        &mut TextInputField,
        &Interaction,
        Option<&TextInputFocused>,
        Option<&crate::ui::menu::SessionCodeInput>,
        Option<&crate::ui::menu::helpers::SeedInput>,
    )>,
    mut commands: Commands,
    mut config: ResMut<GameSetupConfig>,
    mut profile: ResMut<crate::infrastructure::database::ActiveProfile>,
    db: Res<crate::infrastructure::database::GameDatabase>,
    mut keyboard_events: MessageReader<KeyboardInput>,
    keys: Res<ButtonInput<KeyCode>>,
    theme: Res<Theme>,
    role: Option<Res<crate::infrastructure::multiplayer::NetRole>>,
    mut socket: Option<ResMut<bevy_matchbox::prelude::MatchboxSocket>>,
    mut lobby: Option<ResMut<crate::infrastructure::multiplayer::LobbyState>>,
) {
    // ── Focus management via click ──
    let mut clicked_entity: Option<Entity> = None;
    for event in click_events.read() {
        if input_targets.get(event.entity).is_ok() {
            clicked_entity = Some(event.entity);
        }
    }

    if let Some(clicked) = clicked_entity {
        for (entity, _, _, focused, _, _) in &inputs {
            if entity == clicked {
                if focused.is_none() {
                    commands.entity(entity).insert(TextInputFocused);
                    commands
                        .entity(entity)
                        .insert(BorderColor::all(theme.colors.input_border_focused));
                }
            } else if focused.is_some() {
                commands.entity(entity).remove::<TextInputFocused>();
                commands
                    .entity(entity)
                    .insert(BorderColor::all(theme.colors.input_border));
            }
        }
    }

    // On WASM, the browser `paste` event may fire independently of keyboard events.
    // Drain any pending paste buffer into the focused input each frame.
    #[cfg(target_arch = "wasm32")]
    {
        if let Some(clip) = clipboard_read() {
            for (_entity, mut field, _, focused, is_session_code, is_seed_input) in &mut inputs {
                if focused.is_none() {
                    continue;
                }
                field.delete_selection();
                if is_seed_input.is_some() {
                    let digits: String = clip.chars().filter(|c| c.is_ascii_digit()).collect();
                    insert_text_at_cursor(&mut field, &digits);
                    config.map_seed = field.value.parse::<u64>().unwrap_or(0);
                } else {
                    insert_text_at_cursor(&mut field, &clip);
                }
                if is_session_code.is_none() && is_seed_input.is_none() {
                    config.player_name = field.value.clone();
                    profile.name = field.value.clone();
                    db.update_profile_name(&profile.id, &profile.name);
                    // Sync name to multiplayer lobby if connected
                    if let Some(ref role) = role {
                        use crate::infrastructure::multiplayer::NetRole;
                        if *role.as_ref() == NetRole::Client {
                            if let Some(ref mut socket) = socket {
                                let msg = game_state::message::ClientMessage::NameUpdate {
                                    seq: 0,
                                    timestamp: 0.0,
                                    player_name: field.value.clone(),
                                };
                                crate::infrastructure::multiplayer::matchbox_transport::send_to_host(socket, &msg);
                            }
                        }
                    }
                }
                break;
            }
        }
    }

    let events: Vec<_> = keyboard_events.read().cloned().collect();
    if events.is_empty() {
        return;
    }

    let cmd_key = if cfg!(target_os = "macos") {
        keys.pressed(KeyCode::SuperLeft) || keys.pressed(KeyCode::SuperRight)
    } else {
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight)
    };

    for (_entity, mut field, _, focused, is_session_code, is_seed_input) in &mut inputs {
        if focused.is_none() {
            continue;
        }
        let mut changed = false;
        for event in &events {
            if !event.state.is_pressed() {
                continue;
            }

            let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);

            // ── Clipboard shortcuts ──

            if cmd_key && event.key_code == KeyCode::KeyV {
                field.delete_selection();
                if let Some(clip) = clipboard_read() {
                    insert_text_at_cursor(&mut field, &clip);
                }
                changed = true;
                continue;
            }

            if cmd_key && event.key_code == KeyCode::KeyC {
                if let Some((start, end)) = field.selection_range() {
                    clipboard_write(&field.value[start..end]);
                }
                continue;
            }

            if cmd_key && event.key_code == KeyCode::KeyX {
                if let Some((start, end)) = field.selection_range() {
                    clipboard_write(&field.value[start..end]);
                    field.delete_selection();
                    changed = true;
                }
                continue;
            }

            if cmd_key && event.key_code == KeyCode::KeyA {
                if !field.value.is_empty() {
                    field.selection_anchor = Some(0);
                    field.cursor_pos = field.value.len();
                }
                continue;
            }

            // ── Navigation & editing keys ──

            match event.key_code {
                KeyCode::Backspace => {
                    if field.selection_range().is_some() {
                        field.delete_selection();
                    } else if field.cursor_pos > 0 {
                        field.cursor_pos -= 1;
                        let pos = field.cursor_pos;
                        field.value.remove(pos);
                    }
                    field.selection_anchor = None;
                    changed = true;
                }
                KeyCode::Delete => {
                    if field.selection_range().is_some() {
                        field.delete_selection();
                    } else {
                        let pos = field.cursor_pos;
                        if pos < field.value.len() {
                            field.value.remove(pos);
                        }
                    }
                    field.selection_anchor = None;
                    changed = true;
                }
                KeyCode::ArrowLeft => {
                    if shift {
                        if field.selection_anchor.is_none() {
                            field.selection_anchor = Some(field.cursor_pos);
                        }
                        if field.cursor_pos > 0 {
                            field.cursor_pos -= 1;
                        }
                    } else if field.selection_range().is_some() {
                        let (start, _) = field.selection_range().unwrap();
                        field.cursor_pos = start;
                        field.selection_anchor = None;
                    } else {
                        if field.cursor_pos > 0 {
                            field.cursor_pos -= 1;
                        }
                        field.selection_anchor = None;
                    }
                }
                KeyCode::ArrowRight => {
                    if shift {
                        if field.selection_anchor.is_none() {
                            field.selection_anchor = Some(field.cursor_pos);
                        }
                        if field.cursor_pos < field.value.len() {
                            field.cursor_pos += 1;
                        }
                    } else if field.selection_range().is_some() {
                        let (_, end) = field.selection_range().unwrap();
                        field.cursor_pos = end;
                        field.selection_anchor = None;
                    } else {
                        if field.cursor_pos < field.value.len() {
                            field.cursor_pos += 1;
                        }
                        field.selection_anchor = None;
                    }
                }
                KeyCode::Home => {
                    if shift {
                        if field.selection_anchor.is_none() {
                            field.selection_anchor = Some(field.cursor_pos);
                        }
                    } else {
                        field.selection_anchor = None;
                    }
                    field.cursor_pos = 0;
                }
                KeyCode::End => {
                    if shift {
                        if field.selection_anchor.is_none() {
                            field.selection_anchor = Some(field.cursor_pos);
                        }
                    } else {
                        field.selection_anchor = None;
                    }
                    field.cursor_pos = field.value.len();
                }
                KeyCode::Enter | KeyCode::Escape => {
                    field.selection_anchor = None;
                    commands.entity(_entity).remove::<TextInputFocused>();
                    commands
                        .entity(_entity)
                        .insert(BorderColor::all(theme.colors.input_border));
                }
                _ => {
                    // Use logical_key for character input (supports all keyboard layouts)
                    if cmd_key {
                        continue;
                    }
                    if let Key::Character(ref text) = event.logical_key {
                        for ch in text.chars() {
                            if ch.is_control() {
                                continue;
                            }
                            if is_seed_input.is_some() && !ch.is_ascii_digit() {
                                continue;
                            }
                            if field.value.len() >= field.max_len {
                                break;
                            }
                            field.delete_selection();
                            let pos = field.cursor_pos;
                            field.value.insert(pos, ch);
                            field.cursor_pos += 1;
                            changed = true;
                        }
                    } else if is_seed_input.is_none() && event.key_code == KeyCode::Space {
                        if field.value.len() < field.max_len {
                            field.delete_selection();
                            let pos = field.cursor_pos;
                            field.value.insert(pos, ' ');
                            field.cursor_pos += 1;
                            changed = true;
                        }
                    }
                }
            }
        }

        if changed {
            if is_seed_input.is_some() {
                config.map_seed = field.value.parse::<u64>().unwrap_or(0);
            } else if is_session_code.is_none() {
                config.player_name = field.value.clone();
                profile.name = field.value.clone();
                db.update_profile_name(&profile.id, &profile.name);

                // Sync name to multiplayer lobby if connected
                if let Some(ref role) = role {
                    use crate::infrastructure::multiplayer::NetRole;
                    match **role {
                        NetRole::Client => {
                            if let Some(ref mut socket) = socket {
                                let msg = game_state::message::ClientMessage::NameUpdate {
                                    seq: 0,
                                    timestamp: 0.0,
                                    player_name: field.value.clone(),
                                };
                                crate::infrastructure::multiplayer::matchbox_transport::send_to_host(socket, &msg);
                            }
                        }
                        NetRole::Host => {
                            if let Some(ref mut lobby) = lobby {
                                if let Some(host_player) = lobby.players.iter_mut().find(|p| p.is_host) {
                                    host_player.name = field.value.clone();
                                }
                                commands.insert_resource(crate::ui::menu::multiplayer::PendingLobbyBroadcast);
                            }
                        }
                        NetRole::Offline => {}
                    }
                }
            }
        }
    }
}

/// Insert text at cursor position, respecting max_len. ASCII-graphic + space only.
fn insert_text_at_cursor(field: &mut Mut<TextInputField>, text: &str) {
    for ch in text.chars() {
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

// ── Text Input Render System ──
// Updates the rich-text display (Text root + TextSpan children) based on field state.

pub fn text_input_render_system(
    inputs: Query<(&TextInputField, &Children), Changed<TextInputField>>,
    mut text_q: Query<&mut Text>,
    children_q: Query<&Children>,
    mut span_q: Query<&mut TextSpan>,
    mut color_q: Query<&mut TextColor>,
    sel_before: Query<(), With<TextInputSelBefore>>,
    cursor_q: Query<(), With<TextInputCursor>>,
    sel_after: Query<(), With<TextInputSelAfter>>,
    post_q: Query<(), With<TextInputPostText>>,
) {
    for (field, children) in &inputs {
        // Find the root text entity (first child that has Text but not TextInputCursor)
        let Some(text_root) = children
            .iter()
            .find(|c| text_q.get(*c).is_ok() && cursor_q.get(*c).is_err())
        else {
            continue;
        };

        let sel = field.selection_range();
        let anchor = field.selection_anchor.unwrap_or(field.cursor_pos);

        // Compute 5 segments:
        // 1. pre-text (root Text) - before selection or cursor
        // 2. sel_before_cursor - selected text before cursor (when cursor > anchor)
        // 3. cursor "|"
        // 4. sel_after_cursor - selected text after cursor (when cursor < anchor)
        // 5. post-text - after selection/cursor
        let (pre, sel_b, sel_a, post) = if let Some((start, end)) = sel {
            if anchor <= field.cursor_pos {
                // anchor ... cursor_pos: selection is before cursor
                (
                    &field.value[..start],
                    &field.value[start..end],
                    "",
                    &field.value[end..],
                )
            } else {
                // cursor_pos ... anchor: selection is after cursor
                (
                    &field.value[..start],
                    "",
                    &field.value[start..end],
                    &field.value[end..],
                )
            }
        } else {
            (
                &field.value[..field.cursor_pos],
                "",
                "",
                &field.value[field.cursor_pos..],
            )
        };

        // Update root text (pre-selection/cursor)
        if let Ok(mut text) = text_q.get_mut(text_root) {
            **text = pre.to_string();
        }

        // Update TextSpan children of the root text
        if let Ok(span_children) = children_q.get(text_root) {
            for child in span_children.iter() {
                if sel_before.get(child).is_ok() {
                    if let Ok(mut span) = span_q.get_mut(child) {
                        **span = sel_b.to_string();
                    }
                    if let Ok(mut c) = color_q.get_mut(child) {
                        c.0 = if sel_b.is_empty() {
                            Color::NONE
                        } else {
                            SELECTION_COLOR
                        };
                    }
                } else if cursor_q.get(child).is_ok() {
                    // cursor span is handled by blink system
                } else if sel_after.get(child).is_ok() {
                    if let Ok(mut span) = span_q.get_mut(child) {
                        **span = sel_a.to_string();
                    }
                    if let Ok(mut c) = color_q.get_mut(child) {
                        c.0 = if sel_a.is_empty() {
                            Color::NONE
                        } else {
                            SELECTION_COLOR
                        };
                    }
                } else if post_q.get(child).is_ok() {
                    if let Ok(mut span) = span_q.get_mut(child) {
                        **span = post.to_string();
                    }
                }
            }
        }
    }
}

pub fn animate_text_input_chrome(
    mut commands: Commands,
    theme: Res<Theme>,
    query: Query<
        (
            Entity,
            &Interaction,
            Option<&TextInputFocused>,
            &mut BackgroundColor,
            &mut BorderColor,
        ),
        With<TextInputField>,
    >,
) {
    for (entity, interaction, focused, mut bg, mut border) in query {
        let is_focused = focused.is_some();
        let is_hovered = *interaction == Interaction::Hovered;
        let is_pressed = *interaction == Interaction::Pressed;

        *bg = BackgroundColor(if is_pressed {
            theme::BG_ELEVATED.with_alpha(0.98)
        } else if is_focused {
            theme::BG_ELEVATED.with_alpha(0.96)
        } else if is_hovered {
            theme::BG_SURFACE.with_alpha(0.94)
        } else {
            theme.colors.input_bg
        });

        *border = BorderColor::all(if is_pressed || is_focused {
            theme::HIGHLIGHT.with_alpha(0.85)
        } else if is_hovered {
            theme::HIGHLIGHT_SUBTLE.with_alpha(0.55)
        } else {
            theme.colors.input_border
        });

        commands.entity(entity).insert(BoxShadow::new(
            if is_pressed || is_focused {
                theme::HIGHLIGHT.with_alpha(0.28)
            } else if is_hovered {
                theme::HIGHLIGHT_SUBTLE.with_alpha(0.16)
            } else {
                theme::OVERLAY.with_alpha(0.16)
            },
            Val::Px(0.0),
            Val::Px(2.0),
            Val::Px(0.0),
            Val::Px(if is_pressed || is_focused { 12.0 } else { 7.0 }),
        ));
    }
}

// ── Clipboard helpers ──

#[cfg(target_arch = "wasm32")]
mod wasm_clipboard {
    use std::sync::{Mutex, OnceLock};
    use wasm_bindgen::prelude::*;

    static PASTE_BUFFER: OnceLock<Mutex<Option<String>>> = OnceLock::new();
    static COPY_BUFFER: OnceLock<Mutex<String>> = OnceLock::new();

    fn paste_buffer() -> &'static Mutex<Option<String>> {
        PASTE_BUFFER.get_or_init(|| {
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                let cb = Closure::wrap(Box::new(|e: web_sys::ClipboardEvent| {
                    if let Some(dt) = e.clipboard_data() {
                        if let Ok(text) = dt.get_data("text/plain") {
                            if !text.is_empty() {
                                if let Ok(mut buf) =
                                    PASTE_BUFFER.get().expect("already init").lock()
                                {
                                    *buf = Some(text);
                                }
                            }
                        }
                    }
                    e.prevent_default();
                })
                    as Box<dyn FnMut(web_sys::ClipboardEvent)>);
                let _ = doc.add_event_listener_with_callback("paste", cb.as_ref().unchecked_ref());
                cb.forget();
            }
            Mutex::new(None)
        })
    }

    fn copy_buffer() -> &'static Mutex<String> {
        COPY_BUFFER.get_or_init(|| {
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                let cb = Closure::wrap(Box::new(|e: web_sys::ClipboardEvent| {
                    if let Some(dt) = e.clipboard_data() {
                        if let Ok(buf) = COPY_BUFFER.get().expect("already init").lock() {
                            if !buf.is_empty() {
                                let _ = dt.set_data("text/plain", &buf);
                                e.prevent_default();
                            }
                        }
                    }
                })
                    as Box<dyn FnMut(web_sys::ClipboardEvent)>);
                let _ = doc.add_event_listener_with_callback("copy", cb.as_ref().unchecked_ref());
                let _ = doc.add_event_listener_with_callback("cut", cb.as_ref().unchecked_ref());
                cb.forget();
            }
            Mutex::new(String::new())
        })
    }

    pub fn read() -> Option<String> {
        paste_buffer().lock().ok().and_then(|mut buf| buf.take())
    }

    pub fn write(text: &str) {
        // Store in copy buffer for browser copy/cut event listeners
        if let Ok(mut buf) = copy_buffer().lock() {
            *buf = text.to_string();
        }
        // Also try the async clipboard API (requires secure context)
        if let Some(w) = web_sys::window() {
            let clip = w.navigator().clipboard();
            let _ = clip.write_text(text);
        }
    }
}

pub fn clipboard_read() -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        match arboard::Clipboard::new() {
            Ok(mut clipboard) => clipboard.get_text().ok(),
            Err(_) => None,
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        wasm_clipboard::read()
    }
}

pub fn clipboard_write(text: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Ok(mut clipboard) = arboard::Clipboard::new() {
            let _ = clipboard.set_text(text.to_string());
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
    theme: Res<Theme>,
    focused: Query<&Children, With<TextInputFocused>>,
    not_focused: Query<&Children, (With<TextInputField>, Without<TextInputFocused>)>,
    children_q: Query<&Children>,
    mut span_color: Query<&mut TextColor, With<TextInputCursor>>,
) {
    // For focused inputs, find the cursor span inside the text root's children
    for input_children in &focused {
        for child in input_children.iter() {
            // Check direct children first (old structure fallback)
            if let Ok(mut color) = span_color.get_mut(child) {
                let t = time.elapsed_secs();
                let blink = (t * 3.0).sin() * 0.5 + 0.5;
                let c = theme.colors.accent.to_srgba();
                color.0 = Color::srgba(c.red, c.green, c.blue, blink);
                continue;
            }
            // Check grandchildren (new rich text structure)
            if let Ok(grandchildren) = children_q.get(child) {
                for gc in grandchildren.iter() {
                    if let Ok(mut color) = span_color.get_mut(gc) {
                        let t = time.elapsed_secs();
                        let blink = (t * 3.0).sin() * 0.5 + 0.5;
                        let c = theme.colors.accent.to_srgba();
                        color.0 = Color::srgba(c.red, c.green, c.blue, blink);
                    }
                }
            }
        }
    }

    // Hide cursor for unfocused inputs
    for input_children in &not_focused {
        for child in input_children.iter() {
            if let Ok(mut color) = span_color.get_mut(child) {
                color.0 = Color::NONE;
                continue;
            }
            if let Ok(grandchildren) = children_q.get(child) {
                for gc in grandchildren.iter() {
                    if let Ok(mut color) = span_color.get_mut(gc) {
                        color.0 = Color::NONE;
                    }
                }
            }
        }
    }
}
