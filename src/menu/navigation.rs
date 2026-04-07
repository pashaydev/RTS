use bevy::ecs::message::MessageWriter;
use bevy::prelude::*;

use super::helpers::*;
use super::multiplayer;
use super::*;
use crate::components::*;
use crate::multiplayer::{ClientNetState, HostNetState};
use crate::theme::{Theme, HIGHLIGHT, HIGHLIGHT_SUBTLE};
use crate::ui::core::interactions::UiClickEvent;
use crate::ui::core::text_input::ScrollablePanel;

pub(crate) fn menu_keyboard_nav(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut nav: ResMut<MenuNavFocus>,
    mut click_events: MessageWriter<UiClickEvent>,
    focusables: Query<(Entity, &NavFocusable)>,
    mut commands: Commands,
    focused_q: Query<Entity, With<NavFocused>>,
    text_focus: Query<&TextInputFocused>,
    menu_btns: Query<&MenuButton>,
    mut page: ResMut<MenuPage>,
    host_state: Option<Res<HostNetState>>,
    client_state: Option<Res<ClientNetState>>,
    snapshot: Option<Res<super::OptionsSnapshot>>,
    graphics: Res<GraphicsSettings>,
    audio_settings: Res<crate::audio::AudioSettings>,
    mut popup_state: ResMut<super::ConfirmPopupState>,
) {
    // Don't navigate if a text input is focused
    if text_focus.iter().next().is_some() {
        return;
    }

    // Block all keyboard nav while popup is active
    if popup_state.active {
        return;
    }

    // Escape → go back
    if keyboard.just_pressed(KeyCode::Escape) {
        // On Options page, check for unsaved changes before leaving
        if matches!(*page, MenuPage::Options) {
            if let Some(ref snap) = snapshot {
                let dirty = *graphics != snap.graphics || *audio_settings != snap.audio;
                if dirty {
                    popup_state.active = true;
                    return;
                }
            }
        }

        let new_page = match *page {
            MenuPage::NewGame | MenuPage::Options | MenuPage::Multiplayer => Some(MenuPage::Title),
            MenuPage::HostLobby => {
                #[cfg(not(target_arch = "wasm32"))]
                multiplayer::stop_hosting(&mut commands, &host_state);
                Some(MenuPage::Multiplayer)
            }
            MenuPage::JoinLobby => {
                multiplayer::stop_client(&mut commands, &client_state);
                Some(MenuPage::Multiplayer)
            }
            _ => None,
        };
        if let Some(p) = new_page {
            *page = p;
            return;
        }
    }

    let mut items: Vec<(Entity, usize)> = focusables.iter().map(|(e, nf)| (e, nf.0)).collect();
    if items.is_empty() {
        return;
    }
    items.sort_by_key(|&(_, order)| order);
    let count = items.len();

    let up = keyboard.just_pressed(KeyCode::ArrowUp) || keyboard.just_pressed(KeyCode::KeyW);
    let down = keyboard.just_pressed(KeyCode::ArrowDown) || keyboard.just_pressed(KeyCode::KeyS);
    let confirm =
        keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter);

    if up {
        nav.index = if nav.index == 0 {
            count - 1
        } else {
            nav.index - 1
        };
    }
    if down {
        nav.index = (nav.index + 1) % count;
    }

    // Clamp in case buttons changed
    nav.index = nav.index.min(count - 1);

    // Update NavFocused marker
    if up || down {
        for e in &focused_q {
            commands.entity(e).remove::<NavFocused>();
        }
        let (entity, _) = items[nav.index];
        commands.entity(entity).insert(NavFocused);
    }

    // Ensure focus marker exists even without input (first frame)
    if focused_q.is_empty() {
        let (entity, _) = items[nav.index];
        commands.entity(entity).insert(NavFocused);
    }

    // Enter on an action button (MenuButton) → emit click
    if confirm {
        let (entity, _) = items[nav.index];
        if menu_btns.get(entity).is_ok() {
            click_events.write(UiClickEvent { entity });
        }
    }
}

/// Handles Left/Right (A/D) to change the selected option within a focused selector row.
pub(crate) fn menu_selector_keyboard_nav(
    keyboard: Res<ButtonInput<KeyCode>>,
    focused: Query<(Entity, &Children), With<NavFocused>>,
    selectors: Query<(Entity, &MenuSelector, Option<&SelectedOption>)>,
    sliders: Query<&RangeSlider>,
    mut click_events: MessageWriter<UiClickEvent>,
    mut audio_settings: ResMut<crate::audio::AudioSettings>,
    menu_btns: Query<&MenuButton>,
    text_focus: Query<&TextInputFocused>,
    mut nav: ResMut<MenuNavFocus>,
    focusables: Query<(Entity, &NavFocusable)>,
    focused_all: Query<Entity, With<NavFocused>>,
    mut commands: Commands,
) {
    if text_focus.iter().next().is_some() {
        return;
    }

    let left = keyboard.just_pressed(KeyCode::ArrowLeft) || keyboard.just_pressed(KeyCode::KeyA);
    let right = keyboard.just_pressed(KeyCode::ArrowRight) || keyboard.just_pressed(KeyCode::KeyD);
    let confirm =
        keyboard.just_pressed(KeyCode::Enter) || keyboard.just_pressed(KeyCode::NumpadEnter);

    if !left && !right && !confirm {
        return;
    }

    // Enter on a selector row → advance to next focusable row (confirm & move on)
    if confirm && !left && !right {
        for (entity, _) in &focused {
            if menu_btns.get(entity).is_ok() {
                continue; // Action buttons handled by menu_keyboard_nav
            }
            // This is a selector row — advance focus to next item
            let mut items: Vec<(Entity, usize)> =
                focusables.iter().map(|(e, nf)| (e, nf.0)).collect();
            items.sort_by_key(|&(_, order)| order);
            let count = items.len();
            if count > 0 {
                nav.index = (nav.index + 1) % count;
                for e in &focused_all {
                    commands.entity(e).remove::<NavFocused>();
                }
                let (next_entity, _) = items[nav.index];
                commands.entity(next_entity).insert(NavFocused);
            }
            return;
        }
    }

    for (entity, children) in &focused {
        // Skip action buttons — those are handled by menu_keyboard_nav
        if menu_btns.get(entity).is_ok() {
            continue;
        }

        // Collect selector children of this row (in spawn/visual order)
        let mut child_selectors: Vec<(Entity, usize, bool)> = Vec::new();
        for child in children.iter() {
            if let Ok((e, sel, selected)) = selectors.get(child) {
                child_selectors.push((e, sel.index, selected.is_some()));
            }
        }

        if child_selectors.is_empty() {
            let mut handled_slider = false;
            for child in children.iter() {
                let Ok(slider) = sliders.get(child) else {
                    continue;
                };

                match slider.field {
                    SelectorField::MusicVolume => {
                        let delta = if left { -0.01 } else { 0.01 };
                        audio_settings.music_volume =
                            (audio_settings.music_volume + delta).clamp(0.0, 1.0);
                        handled_slider = true;
                    }
                    SelectorField::SfxVolume => {
                        let delta = if left { -0.01 } else { 0.01 };
                        audio_settings.sfx_volume =
                            (audio_settings.sfx_volume + delta).clamp(0.0, 1.0);
                        handled_slider = true;
                    }
                    _ => {}
                }
            }

            if handled_slider {
                continue;
            }

            continue;
        }

        // Arrow selector pattern: no child has SelectedOption (e.g. Resolution < value >).
        // Use spawn order (not sorted by index) so left=first button, right=second button.
        let is_arrow_selector = child_selectors.iter().all(|&(_, _, sel)| !sel);
        if is_arrow_selector && child_selectors.len() >= 2 {
            if left || right {
                let target = if left { 0 } else { 1 };
                let (target_entity, _, _) = child_selectors[target];
                click_events.write(UiClickEvent {
                    entity: target_entity,
                });
            }
            continue;
        }

        // For regular option-button selectors, sort by index for left-right navigation
        child_selectors.sort_by_key(|&(_, idx, _)| idx);

        let current = child_selectors
            .iter()
            .position(|&(_, _, sel)| sel)
            .unwrap_or(0);

        let new = if left {
            if current == 0 {
                child_selectors.len() - 1
            } else {
                current - 1
            }
        } else if right {
            (current + 1) % child_selectors.len()
        } else {
            continue;
        };

        if new != current {
            let (target_entity, _, _) = child_selectors[new];
            click_events.write(UiClickEvent {
                entity: target_entity,
            });
        }
    }
}

/// Applies visual highlight to the keyboard-focused item (button or selector row).
pub(crate) fn menu_nav_focus_visuals(
    focused: Query<Entity, Added<NavFocused>>,
    all_focusable: Query<(Entity, Option<&MenuButton>), With<NavFocusable>>,
    nav_focused_q: Query<(Entity, Option<&MenuButton>), With<NavFocused>>,
    children_q: Query<&Children>,
    value_bgs: Query<Entity, With<ArrowSelectorValueBg>>,
    mut bg_colors: Query<&mut BackgroundColor>,
    mut border_colors_q: Query<&mut BorderColor>,
    mut commands: Commands,
    theme: Res<Theme>,
) {
    if focused.is_empty() {
        return;
    }

    // Style newly focused items — only add focus ring (border + shadow).
    for entity in &focused {
        commands.entity(entity).insert((
            BorderColor::all(theme.colors.accent),
            BoxShadow::new(
                HIGHLIGHT,
                Val::Px(0.0),
                Val::Px(0.0),
                Val::Px(0.0),
                Val::Px(10.0),
            ),
        ));

        // Highlight ArrowSelectorValueBg children when row is focused
        if let Ok(children) = children_q.get(entity) {
            for child in children.iter() {
                if value_bgs.get(child).is_ok() {
                    if let Ok(mut bg) = bg_colors.get_mut(child) {
                        bg.0 = HIGHLIGHT;
                    }
                    if let Ok(mut bc) = border_colors_q.get_mut(child) {
                        *bc = BorderColor::all(HIGHLIGHT);
                    }
                }
            }
        }
    }

    // Reset unfocused items
    for (entity, _) in &all_focusable {
        if nav_focused_q.iter().any(|(e, _)| e == entity) {
            continue;
        }
        commands
            .entity(entity)
            .insert(BorderColor::all(Color::NONE));
        commands.entity(entity).remove::<BoxShadow>();

        // Reset ArrowSelectorValueBg to default
        if let Ok(children) = children_q.get(entity) {
            for child in children.iter() {
                if value_bgs.get(child).is_ok() {
                    if let Ok(mut bg) = bg_colors.get_mut(child) {
                        bg.0 = HIGHLIGHT_SUBTLE;
                    }
                    if let Ok(mut bc) = border_colors_q.get_mut(child) {
                        *bc = BorderColor::all(HIGHLIGHT);
                    }
                }
            }
        }
    }
}

/// Scrolls the menu panel to keep the keyboard-focused item visible.
///
/// Triggers on `Added<NavFocused>` so it runs the frame the marker actually appears
/// (commands that insert `NavFocused` are deferred, so `nav.is_changed()` would fire
/// one frame too early — before layout is available).
pub(crate) fn scroll_to_focused(
    focused_q: Query<(&ComputedNode, &GlobalTransform), Added<NavFocused>>,
    mut panels: Query<
        (&mut ScrollPosition, &ComputedNode, &GlobalTransform),
        With<ScrollablePanel>,
    >,
) {
    let Ok((focused_node, focused_gt)) = focused_q.single() else {
        return;
    };

    for (mut scroll_pos, panel_node, panel_gt) in &mut panels {
        let scale_inv = panel_node.inverse_scale_factor();
        let panel_height = panel_node.size().y * scale_inv;
        let content_height = panel_node.content_size().y * scale_inv;
        let max_scroll = (content_height - panel_height).max(0.0);
        if max_scroll < 1.0 {
            continue;
        }

        // GlobalTransform gives us screen-space positions. Compute focused item's
        // offset relative to the panel's top edge, then convert to content-space
        // by adding the current scroll offset.
        let panel_top_y = panel_gt.translation().y;
        let item_top_y = focused_gt.translation().y;
        let item_height = focused_node.size().y * scale_inv;

        let rel_top = item_top_y - panel_top_y;
        let item_content_top = rel_top + scroll_pos.y;
        let item_content_bottom = item_content_top + item_height;

        let visible_top = scroll_pos.y;
        let visible_bottom = scroll_pos.y + panel_height;

        if item_content_top < visible_top {
            scroll_pos.y = (item_content_top - 10.0).max(0.0);
        } else if item_content_bottom > visible_bottom {
            scroll_pos.y = (item_content_bottom - panel_height + 10.0).min(max_scroll);
        }
    }
}

/// Resets nav focus index when the menu page changes.
pub(crate) fn reset_nav_focus_on_page_change(
    page: Res<MenuPage>,
    mut nav: ResMut<MenuNavFocus>,
    focused_q: Query<Entity, With<NavFocused>>,
    mut commands: Commands,
) {
    if page.is_changed() {
        nav.index = 0;
        for e in &focused_q {
            commands.entity(e).remove::<NavFocused>();
        }
    }
}
