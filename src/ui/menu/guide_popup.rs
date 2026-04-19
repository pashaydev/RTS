//! "How to Play" popup that appears every time a new game starts.
//!
//! Paged wizard (4 pages): Basics, Resources, Victory, Controls.
//! Simulation is never paused — the overlay is purely informational.

use bevy::ecs::message::MessageReader;
use bevy::prelude::*;

use crate::infrastructure::multiplayer::NetRole;
use crate::types::*;
use crate::ui::core::interactions::UiClickEvent;
use crate::ui::fonts::{self, UiFonts};
use crate::ui::theme::Theme;

const TOTAL_PAGES: usize = 6;

pub struct GuidePopupPlugin;

impl Plugin for GuidePopupPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppState::InGame), spawn_guide_on_enter)
            .add_systems(OnExit(AppState::InGame), cleanup_guide_on_exit)
            .add_systems(
                Update,
                (
                    handle_guide_escape.before(super::pause_menu::handle_escape_key),
                    handle_guide_buttons,
                    rerender_on_page_change,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

// ── Spawn / Cleanup ──

fn spawn_guide_on_enter(
    mut commands: Commands,
    mut overlay: ResMut<InGameOverlay>,
    net_role: Res<NetRole>,
) {
    let is_multiplayer = *net_role != NetRole::Offline;
    commands.insert_resource(GuideState {
        page: 0,
        is_multiplayer,
    });
    *overlay = InGameOverlay::HowToPlay;
}

fn cleanup_guide_on_exit(
    mut commands: Commands,
    roots: Query<Entity, With<GuideOverlayRoot>>,
) {
    for e in &roots {
        commands.entity(e).try_despawn();
    }
    commands.remove_resource::<GuideState>();
}

// ── Escape ──

fn handle_guide_escape(
    mut commands: Commands,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut overlay: ResMut<InGameOverlay>,
    roots: Query<Entity, With<GuideOverlayRoot>>,
) {
    if *overlay != InGameOverlay::HowToPlay {
        return;
    }
    if !keyboard.just_pressed(KeyCode::Escape) {
        return;
    }
    close_guide(&mut commands, &mut overlay, &roots);
}

// ── Button Clicks ──

fn handle_guide_buttons(
    mut commands: Commands,
    mut click_events: MessageReader<UiClickEvent>,
    buttons: Query<&GuideButton>,
    state: Option<ResMut<GuideState>>,
    mut overlay: ResMut<InGameOverlay>,
    roots: Query<Entity, With<GuideOverlayRoot>>,
    mut ui_clicked: ResMut<UiClickedThisFrame>,
) {
    let Some(mut state) = state else { return };
    for event in click_events.read() {
        let Ok(btn) = buttons.get(event.entity) else {
            continue;
        };
        ui_clicked.0 = 2;
        match btn {
            GuideButton::Back => {
                if state.page > 0 {
                    state.page -= 1;
                }
            }
            GuideButton::Next => {
                if state.page + 1 < TOTAL_PAGES {
                    state.page += 1;
                } else {
                    close_guide(&mut commands, &mut overlay, &roots);
                    return;
                }
            }
            GuideButton::Close => {
                close_guide(&mut commands, &mut overlay, &roots);
                return;
            }
        }
    }
}

fn close_guide(
    commands: &mut Commands,
    overlay: &mut InGameOverlay,
    roots: &Query<Entity, With<GuideOverlayRoot>>,
) {
    for e in roots {
        commands.entity(e).try_despawn();
    }
    commands.remove_resource::<GuideState>();
    *overlay = InGameOverlay::None;
}

// ── Re-render on page change ──

fn rerender_on_page_change(
    state: Option<Res<GuideState>>,
    roots: Query<(Entity, Option<&Children>), With<GuideOverlayRoot>>,
    mut commands: Commands,
    fonts: Res<UiFonts>,
    theme: Res<Theme>,
    config: Res<GameSetupConfig>,
) {
    let Some(state) = state else {
        return;
    };
    let has_root = !roots.is_empty();
    if !state.is_changed() && has_root {
        return;
    }

    if !has_root {
        let root = spawn_root(&mut commands);
        let panel = spawn_panel(
            &mut commands,
            &fonts,
            &theme,
            &config,
            state.page,
            state.is_multiplayer,
            true,
        );
        commands.entity(root).add_child(panel);
    } else {
        for (root, children) in &roots {
            if let Some(children) = children {
                for child in children.iter() {
                    commands.entity(child).despawn();
                }
            }
            let panel = spawn_panel(
                &mut commands,
                &fonts,
                &theme,
                &config,
                state.page,
                state.is_multiplayer,
                false,
            );
            commands.entity(root).add_child(panel);
        }
    }
}

// ── Overlay Root ──

fn spawn_root(commands: &mut Commands) -> Entity {
    commands
        .spawn((
            GuideOverlayRoot,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                position_type: PositionType::Absolute,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            ZIndex(110),
            UiFadeIn {
                timer: Timer::from_seconds(0.25, TimerMode::Once),
            },
        ))
        .id()
}

// ── Panel ──

fn spawn_panel(
    commands: &mut Commands,
    fonts: &UiFonts,
    theme: &Theme,
    config: &GameSetupConfig,
    page: usize,
    is_multiplayer: bool,
    with_entrance: bool,
) -> Entity {
    let mut panel_cmd = commands.spawn((
        Node {
            width: Val::Px(680.0),
            min_height: Val::Px(500.0),
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(28.0)),
            row_gap: Val::Px(18.0),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(Color::srgba(
            theme.colors.bg_panel.to_srgba().red,
            theme.colors.bg_panel.to_srgba().green,
            theme.colors.bg_panel.to_srgba().blue,
            0.97,
        )),
        BorderColor::all(theme.colors.separator),
        BoxShadow::new(
            Color::srgba(0.0, 0.0, 0.0, 0.6),
            Val::Px(0.0),
            Val::Px(4.0),
            Val::Px(0.0),
            Val::Px(24.0),
        ),
    ));
    if with_entrance {
        panel_cmd.insert(UiScaleIn {
            from: 0.92,
            timer: Timer::from_seconds(0.3, TimerMode::Once),
            elastic: false,
        });
    }
    let panel = panel_cmd.id();

    spawn_header(commands, panel, fonts, theme, page);
    spawn_content_area(commands, panel, fonts, theme, config, page, is_multiplayer);
    spawn_footer(commands, panel, fonts, theme, page);

    panel
}

// ── Header ──

fn spawn_header(
    commands: &mut Commands,
    panel: Entity,
    fonts: &UiFonts,
    theme: &Theme,
    page: usize,
) {
    let header = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            padding: UiRect::bottom(Val::Px(10.0)),
            border: UiRect::bottom(Val::Px(1.0)),
            ..default()
        })
        .insert(BorderColor::all(theme.colors.separator.with_alpha(0.5)))
        .id();

    let title_col = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(2.0),
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Text::new("HOW TO PLAY"),
                fonts::heading(fonts, 22.0),
                TextColor(theme.colors.prestige),
            ));
            parent.spawn((
                Text::new(format!("STEP {} / {}", page + 1, TOTAL_PAGES)),
                fonts::body(fonts, 9.0),
                TextColor(theme.colors.text_disabled.with_alpha(0.85)),
            ));
        })
        .id();

    let close_btn = commands
        .spawn((
            GuideButton::Close,
            Button,
            ButtonAnimState::new(theme.colors.bg_elevated.to_srgba().to_f32_array()),
            ButtonStyle::Filled,
            Node {
                width: Val::Px(28.0),
                height: Val::Px(28.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                border: UiRect::all(Val::Px(1.0)),
                ..default()
            },
            BackgroundColor(theme.colors.bg_elevated),
            BorderColor::all(theme.colors.separator.with_alpha(0.75)),
        ))
        .with_children(|btn| {
            btn.spawn((
                Text::new("X"),
                fonts::body_emphasis(fonts, 11.0),
                TextColor(theme.colors.text_secondary),
                Pickable::IGNORE,
            ));
        })
        .id();

    commands.entity(header).add_children(&[title_col, close_btn]);
    commands.entity(panel).add_child(header);
}

// ── Content area ──

fn spawn_content_area(
    commands: &mut Commands,
    panel: Entity,
    fonts: &UiFonts,
    theme: &Theme,
    config: &GameSetupConfig,
    page: usize,
    is_multiplayer: bool,
) {
    let area = commands
        .spawn((
            GuideContentArea,
            Node {
                flex_grow: 1.0,
                width: Val::Percent(100.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(10.0),
                padding: UiRect::vertical(Val::Px(6.0)),
                ..default()
            },
        ))
        .id();

    match page {
        0 => spawn_page_basics(commands, area, fonts, theme, config),
        1 => spawn_page_economy(commands, area, fonts, theme, config),
        2 => spawn_page_armies(commands, area, fonts, theme),
        3 => spawn_page_night_waves(commands, area, fonts, theme, config),
        4 => spawn_page_victory(commands, area, fonts, theme, config, is_multiplayer),
        _ => spawn_page_controls(commands, area, fonts, theme),
    }

    commands.entity(panel).add_child(area);
}

// ── Page 0: BASICS ──

fn spawn_page_basics(
    commands: &mut Commands,
    parent: Entity,
    fonts: &UiFonts,
    theme: &Theme,
    config: &GameSetupConfig,
) {
    let section_title = spawn_section_title(commands, fonts, theme, "BASICS");
    let local_faction = Faction::PLAYERS[config.local_player_slot.min(3)].display_name();
    let intro = spawn_paragraph(
        commands,
        fonts,
        theme,
        &format!(
            "Commander {} — you lead {}. Build an economy, train an army, and destroy every opposing commander before nightfall buries you.",
            config.player_name, local_faction
        ),
    );
    let input_bullets = spawn_bullets(
        commands,
        fonts,
        theme,
        &[
            "Left-click to select a unit or building.",
            "Left-click + drag to box-select many units.",
            "Right-click to move, attack, gather, or assist construction (context-sensitive).",
            "WASD or Arrow keys pan the camera; mouse wheel zooms; push the cursor to a screen edge to edge-scroll.",
        ],
    );
    let fog_title = spawn_subsection_title(commands, fonts, theme, "FOG OF WAR");
    let fog_body = spawn_paragraph(
        commands,
        fonts,
        theme,
        "You only see what your units and buildings can see. The rest of the map is hidden — scout aggressively and spread vision before committing your army.",
    );
    commands
        .entity(parent)
        .add_children(&[section_title, intro, input_bullets, fog_title, fog_body]);
}

// ── Page 1: ECONOMY & BUILDINGS ──

fn spawn_page_economy(
    commands: &mut Commands,
    parent: Entity,
    fonts: &UiFonts,
    theme: &Theme,
    config: &GameSetupConfig,
) {
    let section_title = spawn_section_title(commands, fonts, theme, "ECONOMY & BUILDINGS");

    let intro = spawn_paragraph(
        commands,
        fonts,
        theme,
        "Ten resources power your war: five raw (Wood, Copper, Iron, Gold, Oil) and five processed (Planks, Charcoal, Bronze, Steel, Gunpowder). Every production building has its own storage pool — workers deposit there and resources flow into your faction total for spending.",
    );

    let cat_title = spawn_subsection_title(commands, fonts, theme, "BUILDING CATEGORIES");
    let cat_bullets = spawn_bullets(
        commands,
        fonts,
        theme,
        &[
            "Economy — Base, Sawmill, Mine, OilRig, Storage.",
            "Production — Smelter, Alchemist (refine raw goods into processed ones).",
            "Military — Barracks, Workshop, Stable, SiegeWorks.",
            "Defense — WatchTower, GuardTower, BallistaTower, BombardTower.",
        ],
    );

    let ages_title = spawn_subsection_title(commands, fonts, theme, "AGES — RESEARCH AT YOUR BASE");
    let ages_bullets = spawn_bullets(
        commands,
        fonts,
        theme,
        &[
            "I. Settlement (start) — Barracks, Sawmill, Mine, WatchTower, walls.",
            "II. Expansion — unlocks Workshop, Stable, Smelter, GuardTower, OilRig.",
            "III. Conquest — unlocks SiegeWorks, Alchemist, MageTower, BallistaTower, BombardTower.",
        ],
    );

    let chains_title = spawn_subsection_title(commands, fonts, theme, "PROCESSING & UPGRADES");
    let chains_bullets = spawn_bullets(
        commands,
        fonts,
        theme,
        &[
            "Sawmill turns Wood into Planks, and into Charcoal at level 2.",
            "Smelter forges Bronze at level 1, Steel at level 2 (needs a Mine).",
            "Alchemist brews Gunpowder (needs a Smelter).",
            "Upgrade buildings to level 3 for faster output and to unlock new recipes.",
            "Set a rally point on production buildings so fresh units gather where you need them.",
        ],
    );

    let density_label = match config.resource_density {
        ResourceDensity::Sparse => "Sparse",
        ResourceDensity::Normal => "Standard",
        ResourceDensity::Dense => "Dense",
    };
    let setup = spawn_paragraph(
        commands,
        fonts,
        theme,
        &format!(
            "This match: {:.1}x starting allocation on a {} resource map.",
            config.starting_resources_mult, density_label
        ),
    );

    commands.entity(parent).add_children(&[
        section_title,
        intro,
        cat_title,
        cat_bullets,
        ages_title,
        ages_bullets,
        chains_title,
        chains_bullets,
        setup,
    ]);
}

// ── Page 2: ARMIES ──

fn spawn_page_armies(commands: &mut Commands, parent: Entity, fonts: &UiFonts, theme: &Theme) {
    let section_title = spawn_section_title(commands, fonts, theme, "ARMIES");

    let roles_title = spawn_subsection_title(commands, fonts, theme, "UNIT ROLES");
    let roles = spawn_bullets(
        commands,
        fonts,
        theme,
        &[
            "Workers gather resources and build.",
            "Melee — Soldier, Knight, Tank.",
            "Ranged / support — Archer, Mage, Priest.",
            "Mobility — Cavalry for flanks, Scout for vision.",
            "Siege — Catapult and Battering Ram to crack walls and buildings.",
        ],
    );

    let stance_title = spawn_subsection_title(commands, fonts, theme, "STANCES — V TO CYCLE");
    let stances = spawn_bullets(
        commands,
        fonts,
        theme,
        &[
            "Passive — hold fire, obey manual orders only.",
            "Defensive (default) — auto-engage anything in scan range, then leash back.",
            "Aggressive — seek and chase enemies wherever they are.",
        ],
    );

    let form_title = spawn_subsection_title(commands, fonts, theme, "FORMATIONS — G TO CYCLE");
    let formations = spawn_bullets(
        commands,
        fonts,
        theme,
        &[
            "Line — wide front for direct pushes.",
            "Grid (default) — compact block for general movement.",
            "Chess — staggered rows that let ranged units shoot through.",
        ],
    );

    let abil_title = spawn_subsection_title(commands, fonts, theme, "ABILITIES");
    let abil = spawn_paragraph(
        commands,
        fonts,
        theme,
        "T fires the selected unit's first ability, Y the second. Abilities are per-unit — hover a unit portrait for details.",
    );

    commands.entity(parent).add_children(&[
        section_title,
        roles_title,
        roles,
        stance_title,
        stances,
        form_title,
        formations,
        abil_title,
        abil,
    ]);
}

// ── Page 3: NIGHT WAVES ──

fn spawn_page_night_waves(
    commands: &mut Commands,
    parent: Entity,
    fonts: &UiFonts,
    theme: &Theme,
    config: &GameSetupConfig,
) {
    let section_title = spawn_section_title(commands, fonts, theme, "NIGHT WAVES");

    let night_secs = config.day_cycle_secs * 0.40;
    let day_secs = config.day_cycle_secs * 0.60;
    let cycle = spawn_paragraph(
        commands,
        fonts,
        theme,
        &format!(
            "Every cycle — {} of daylight, about {} of night — goblins storm the map from the perimeter. Your first wave hits partway through the opening cycle; brace before the sun drops.",
            format_duration(day_secs),
            format_duration(night_secs),
        ),
    );

    let scaling_title = spawn_subsection_title(commands, fonts, theme, "SCALING");
    let scaling = spawn_bullets(
        commands,
        fonts,
        theme,
        &[
            "Wave size grows with each night (8 on night 1, +2 each night, capped at 80).",
            "Spawn cadence tightens from one every 8 seconds down to one every 2 seconds by night 15.",
            "Tiers unlock over time: Runners from night 1, Veterans from night 3, Champions from night 6.",
        ],
    );

    let behavior_title = spawn_subsection_title(commands, fonts, theme, "PRIORITIES & REWARDS");
    let behavior = spawn_bullets(
        commands,
        fonts,
        theme,
        &[
            "Targets are locked on spawn: Bases and Storage first, then production buildings, then your units.",
            "Survive with a standing base and the next dawn pays +50 Gold and +30 Wood.",
            "Higher-tier goblins drop weapons, armor, and rings — loot them before dawn sweeps survivors off the map.",
        ],
    );

    commands.entity(parent).add_children(&[
        section_title,
        cycle,
        scaling_title,
        scaling,
        behavior_title,
        behavior,
    ]);
}

fn format_duration(secs: f32) -> String {
    let total = secs.round().max(0.0) as u32;
    if total < 60 {
        format!("{total} s")
    } else if total < 120 {
        "1 min".to_string()
    } else {
        format!("{} min", (total + 30) / 60)
    }
}

// ── Page 4: VICTORY ──

fn spawn_page_victory(
    commands: &mut Commands,
    parent: Entity,
    fonts: &UiFonts,
    theme: &Theme,
    config: &GameSetupConfig,
    is_multiplayer: bool,
) {
    let section_title = spawn_section_title(commands, fonts, theme, "VICTORY");
    let mut kids = vec![section_title];

    let grace = spawn_paragraph(
        commands,
        fonts,
        theme,
        "If you lose your last base, a 60-second grace period begins. Rebuild before it expires or you are eliminated.",
    );
    kids.push(grace);

    match config.team_mode {
        TeamMode::FFA => {
            let p = spawn_paragraph(
                commands,
                fonts,
                theme,
                "Free-for-all. The last commander standing wins — every other faction is hostile.",
            );
            kids.push(p);
        }
        TeamMode::Teams | TeamMode::Custom => {
            let local = config.local_player_slot.min(3);
            let my_team = config.player_teams[local];
            let mut allies: Vec<&'static str> = Vec::new();
            let mut enemies: Vec<&'static str> = Vec::new();
            for slot in 0..4 {
                if slot == local {
                    continue;
                }
                if matches!(config.slots[slot], SlotOccupant::Closed) {
                    continue;
                }
                let name = Faction::PLAYERS[slot].display_name();
                if config.player_teams[slot] == my_team {
                    allies.push(name);
                } else {
                    enemies.push(name);
                }
            }
            let allies_str = if allies.is_empty() {
                "(none)".to_string()
            } else {
                allies.join(", ")
            };
            let enemies_str = if enemies.is_empty() {
                "(none)".to_string()
            } else {
                enemies.join(", ")
            };
            let rule = spawn_paragraph(
                commands,
                fonts,
                theme,
                "Team match. The game ends when every surviving commander shares a team — an ally's base can keep you alive during your grace period.",
            );
            kids.push(rule);
            let teams = spawn_paragraph(
                commands,
                fonts,
                theme,
                &format!("Allies: {}.  Opponents: {}.", allies_str, enemies_str),
            );
            kids.push(teams);
        }
    }

    if is_multiplayer {
        let mp = spawn_paragraph(
            commands,
            fonts,
            theme,
            "Multiplayer runs on deterministic lockstep. Every peer simulates the same world; a checksum mismatch halts the match for everyone.",
        );
        kids.push(mp);
    }

    commands.entity(parent).add_children(&kids);
}

// ── Page 5: CONTROLS ──

fn spawn_page_controls(commands: &mut Commands, parent: Entity, fonts: &UiFonts, theme: &Theme) {
    let section_title = spawn_section_title(commands, fonts, theme, "CONTROLS");

    let columns = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(28.0),
            width: Val::Percent(100.0),
            ..default()
        })
        .id();

    let left = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            flex_grow: 1.0,
            ..default()
        })
        .id();
    let left_title = spawn_column_title(commands, fonts, theme, "COMMANDS");
    commands.entity(left).add_child(left_title);
    for (key, action) in [
        ("RMB", "move / attack / gather"),
        ("H", "hold position"),
        ("X", "stop / scuttle"),
        ("V", "cycle stance (Passive/Defensive/Aggressive)"),
        ("F", "attack-move"),
        ("P", "patrol"),
        ("G", "cycle formation (Line/Grid/Chess)"),
        ("T", "ability 1"),
        ("Y", "ability 2"),
        ("L", "toggle unit labels"),
    ] {
        let row = spawn_key_row(commands, fonts, theme, key, action);
        commands.entity(left).add_child(row);
    }

    let right = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            flex_grow: 1.0,
            ..default()
        })
        .id();
    let right_title = spawn_column_title(commands, fonts, theme, "GROUPS & CAMERA");
    commands.entity(right).add_child(right_title);
    for (key, action) in [
        ("1-9", "select control group"),
        ("Ctrl+1-9", "assign selection to group"),
        ("Shift+1-9", "add selection to group"),
        ("Alt+1-9", "replace group with selection"),
        ("Tab", "cycle subgroup by unit type"),
        ("WASD / Arrows", "pan camera"),
        ("Scroll Wheel", "zoom"),
        ("Screen edge", "edge-scroll"),
        ("Esc", "cancel / close popups"),
    ] {
        let row = spawn_key_row(commands, fonts, theme, key, action);
        commands.entity(right).add_child(row);
    }

    commands.entity(columns).add_children(&[left, right]);
    commands
        .entity(parent)
        .add_children(&[section_title, columns]);
}

// ── Content helpers ──

fn spawn_section_title(
    commands: &mut Commands,
    fonts: &UiFonts,
    theme: &Theme,
    text: &str,
) -> Entity {
    commands
        .spawn((
            Text::new(text.to_string()),
            fonts::heading(fonts, 15.0),
            TextColor(theme.colors.accent),
            Node {
                margin: UiRect::bottom(Val::Px(2.0)),
                ..default()
            },
        ))
        .id()
}

fn spawn_subsection_title(
    commands: &mut Commands,
    fonts: &UiFonts,
    theme: &Theme,
    text: &str,
) -> Entity {
    commands
        .spawn((
            Text::new(text.to_string()),
            fonts::body_emphasis(fonts, 10.0),
            TextColor(theme.colors.prestige.with_alpha(0.92)),
            Node {
                margin: UiRect::top(Val::Px(4.0)),
                ..default()
            },
        ))
        .id()
}

fn spawn_column_title(
    commands: &mut Commands,
    fonts: &UiFonts,
    theme: &Theme,
    text: &str,
) -> Entity {
    commands
        .spawn((
            Text::new(text.to_string()),
            fonts::body_emphasis(fonts, 10.0),
            TextColor(theme.colors.prestige.with_alpha(0.9)),
            Node {
                margin: UiRect::bottom(Val::Px(4.0)),
                ..default()
            },
        ))
        .id()
}

fn spawn_paragraph(commands: &mut Commands, fonts: &UiFonts, theme: &Theme, text: &str) -> Entity {
    commands
        .spawn((
            Text::new(text.to_string()),
            fonts::body(fonts, 12.0),
            TextColor(theme.colors.text_primary.with_alpha(0.92)),
        ))
        .id()
}

fn spawn_bullets(
    commands: &mut Commands,
    fonts: &UiFonts,
    theme: &Theme,
    lines: &[&str],
) -> Entity {
    let container = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(4.0),
            padding: UiRect::left(Val::Px(4.0)),
            ..default()
        })
        .id();
    for line in lines {
        let row = commands
            .spawn(Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(8.0),
                align_items: AlignItems::FlexStart,
                ..default()
            })
            .with_children(|parent| {
                parent.spawn((
                    Text::new("›"),
                    fonts::body_emphasis(fonts, 12.0),
                    TextColor(theme.colors.accent),
                ));
                parent.spawn((
                    Text::new((*line).to_string()),
                    fonts::body(fonts, 12.0),
                    TextColor(theme.colors.text_primary.with_alpha(0.9)),
                ));
            })
            .id();
        commands.entity(container).add_child(row);
    }
    container
}

fn spawn_key_row(
    commands: &mut Commands,
    fonts: &UiFonts,
    theme: &Theme,
    key: &str,
    action: &str,
) -> Entity {
    commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            column_gap: Val::Px(10.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|parent| {
            parent
                .spawn((
                    Node {
                        min_width: Val::Px(96.0),
                        padding: UiRect::axes(Val::Px(6.0), Val::Px(2.0)),
                        border: UiRect::all(Val::Px(1.0)),
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(theme.colors.bg_elevated.with_alpha(0.7)),
                    BorderColor::all(theme.colors.separator.with_alpha(0.7)),
                ))
                .with_children(|kb| {
                    kb.spawn((
                        Text::new(key.to_string()),
                        fonts::body_emphasis(fonts, 10.0),
                        TextColor(theme.colors.prestige),
                    ));
                });
            parent.spawn((
                Text::new(action.to_string()),
                fonts::body(fonts, 11.0),
                TextColor(theme.colors.text_secondary.with_alpha(0.9)),
            ));
        })
        .id()
}

// ── Footer (dots + buttons) ──

fn spawn_footer(
    commands: &mut Commands,
    panel: Entity,
    fonts: &UiFonts,
    theme: &Theme,
    page: usize,
) {
    let footer = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            flex_direction: FlexDirection::Column,
            row_gap: Val::Px(12.0),
            padding: UiRect::top(Val::Px(10.0)),
            border: UiRect::top(Val::Px(1.0)),
            ..default()
        })
        .insert(BorderColor::all(theme.colors.separator.with_alpha(0.5)))
        .id();

    // Page dots
    let dots = commands
        .spawn((
            GuidePageDots,
            Node {
                flex_direction: FlexDirection::Row,
                column_gap: Val::Px(6.0),
                justify_content: JustifyContent::Center,
                width: Val::Percent(100.0),
                ..default()
            },
        ))
        .id();
    for i in 0..TOTAL_PAGES {
        let active = i == page;
        let color = if active {
            theme.colors.accent
        } else {
            theme.colors.text_disabled.with_alpha(0.4)
        };
        let dot = commands
            .spawn((
                Node {
                    width: Val::Px(8.0),
                    height: Val::Px(8.0),
                    ..default()
                },
                BackgroundColor(color),
            ))
            .id();
        commands.entity(dots).add_child(dot);
    }

    // Buttons row
    let buttons_row = commands
        .spawn(Node {
            flex_direction: FlexDirection::Row,
            justify_content: JustifyContent::SpaceBetween,
            align_items: AlignItems::Center,
            width: Val::Percent(100.0),
            ..default()
        })
        .id();

    let back_enabled = page > 0;
    let back = spawn_action_button(commands, fonts, theme, "< BACK", GuideButton::Back, false, !back_enabled);
    let is_last = page + 1 == TOTAL_PAGES;
    let next_label = if is_last { "CONTINUE" } else { "NEXT >" };
    let next = spawn_action_button(commands, fonts, theme, next_label, GuideButton::Next, true, false);
    commands.entity(buttons_row).add_children(&[back, next]);

    commands.entity(footer).add_children(&[dots, buttons_row]);
    commands.entity(panel).add_child(footer);
}

fn spawn_action_button(
    commands: &mut Commands,
    fonts: &UiFonts,
    theme: &Theme,
    label: &str,
    kind: GuideButton,
    accent: bool,
    disabled: bool,
) -> Entity {
    let bg = if disabled {
        theme.colors.bg_elevated.with_alpha(0.4)
    } else if accent {
        theme.colors.accent
    } else {
        theme.colors.btn_primary
    };
    let text_color = if disabled {
        theme.colors.text_disabled
    } else if accent {
        Color::srgb(0.10, 0.16, 0.18)
    } else {
        theme.colors.text_primary
    };

    let mut ec = commands.spawn((
        kind,
        Button,
        ButtonAnimState::new(bg.to_srgba().to_f32_array()),
        ButtonStyle::Filled,
        Node {
            min_width: Val::Px(140.0),
            height: Val::Px(40.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            padding: UiRect::horizontal(Val::Px(18.0)),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        },
        BackgroundColor(bg),
        BorderColor::all(if accent && !disabled {
            theme.colors.accent
        } else {
            theme.colors.separator.with_alpha(0.8)
        }),
    ));
    if disabled {
        ec.insert(ButtonDisabled(None));
    }
    ec.with_child((
        Text::new(label.to_string()),
        fonts::body_emphasis(fonts, 12.0),
        TextColor(text_color),
        Pickable::IGNORE,
    ));
    ec.id()
}
