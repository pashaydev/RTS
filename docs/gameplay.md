# Gameplay Reference

Detailed gameplay documentation for the RTS Prototype. See the [README](../README.md) for project setup and architecture.

## Core Gameplay

### Match Flow

1. Start with 2 workers and no base
2. Use `Found Base` to establish the first settlement (Age I: Settlement)
3. Expand gathering, processing, storage, and production
4. Research Age II (Expansion) at the Base to unlock Workshop, Stable, Smelter, and more
5. Research Age III (Conquest) for Siege Works, Mage Tower, and elite defenses
6. Pressure camps for resource rewards, item drops, or attack enemy factions
7. Win by eliminating all enemy Bases (60s grace period before elimination)

### Victory Conditions

- **Elimination**: destroy all enemy Bases to win
- A faction that loses all Bases enters a 60-second grace period to rebuild
- If the grace period expires and the faction cannot afford a new Base, they are eliminated
- The last faction (or team) standing wins the match
- Victory and defeat are announced with a fullscreen overlay and return-to-menu option

### Ages / Eras

The tech tree is gated behind three ages, researched at the Base building:

| Age | Cost | Research Time | Unlocks |
|---|---|---|---|
| I: Settlement | — (starting age) | — | Base, Barracks, Sawmill, Mine, Storage, House, WatchTower, Outpost, Walls |
| II: Expansion | 150W 50Cu 80Fe 60Sn | 45s | Workshop, Stable, Smelter, Guard Tower, Oil Rig, Gatehouse, Tower |
| III: Conquest | 200W 100Cu 150Fe 50Go 100Sn | 60s | Siege Works, Mage Tower, Temple, Alchemist, Ballista Tower, Bombard Tower |

### Economy

- Raw resources: Wood, Copper, Iron, Gold, Oil, Stone
- Processed resources: Planks, Charcoal, Bronze, Steel, Gunpowder
- Workers can gather directly or be assigned to processor buildings
- Buildings can run recipes, buffer resources, and scale throughput through upgrades and staffing
- **Resource depletion**: mineral nodes (Copper, Iron, Gold, Oil) are finite and do not regrow; only Wood regrows after depletion
- **Population upkeep**: income scales down with army size (0-20 units: 100%, 21-40: 85%, 41-60: 70%, 60+: 50%)

### Combat

- Roster: Worker, Soldier, Archer, Tank, Knight, Mage, Priest, Cavalry, Scout, Catapult, Battering Ram
- Units support formation movement, patrol, attack-move, hold position, stances, and queued tasks
- Military units default to Aggressive stance; Workers and Priests default to Defensive
- Abilities include charge, shield bash, fireball, frost nova, heal, holy smite, and siege attacks
- Enemy camps include Goblin, Skeleton, Orc, and Demon factions with patrol and aggro behavior

### Scored Targeting

Units and towers select targets using a weighted scoring formula instead of always attacking the nearest enemy. Each unit type has a **Targeting Profile** that tunes how it weighs different factors. Lower scores are preferred.

**Scoring factors:**
- **Distance** — closer targets score better (normalized to scan range)
- **Low HP** — wounded targets are preferred (finish off weak enemies)
- **Threat** — high-threat targets are prioritized (each unit has a static threat value)
- **Counter advantage** — targets the attacker deals bonus damage against are preferred
- **Building penalty** — most units avoid buildings; siege units prefer them (negative penalty)
- **Reserved damage** — targets with enough incoming damage already committed are deprioritized

**Hard rejects** (never targeted):
- Dead or dying entities
- Outside scan range (scan range × 1.5 max)
- Non-hostile factions

**Stance modifiers:**
- **Passive** — never auto-engage
- **Defensive** — scan range = attack range × 1.35
- **Aggressive** — scan range = attack range × 2.25

**Threat values:**

| Unit | Threat |
|---|---:|
| Worker | 0.35 |
| Scout | 0.15 |
| Soldier | 0.90 |
| Archer | 1.15 |
| Tank | 1.45 |
| Knight | 1.30 |
| Mage | 1.55 |
| Priest | 1.10 |
| Cavalry | 1.20 |
| Catapult | 2.10 |
| Battering Ram | 1.90 |
| Watch Tower | 1.20 |
| Guard Tower | 1.45 |
| Ballista Tower | 1.70 |
| Bombard Tower | 1.85 |
| Economy buildings | 0.00 |

**Targeting profile summary:**

| Unit class | Behavior |
|---|---|
| Worker / Scout | Strong distance bias, avoids buildings, low threat sensitivity |
| Soldier / Tank | Balanced — moderate focus on threats and counters |
| Archer | Finishes low-HP targets, high reserved-damage penalty (avoids wasted volleys) |
| Knight / Cavalry | Dives high-threat backline targets, moderate building avoidance |
| Mage | High threat and counter sensitivity, avoids buildings |
| Priest | Balanced auto-attack, strong building avoidance |
| Catapult | Prefers buildings (negative penalty), high threat weight, ignores reserved damage |
| Battering Ram | Strongly prefers buildings, ignores low-HP and reserved damage |
| Towers | Never targets buildings (99.0 penalty), prioritizes high-threat units, avoids overkill |

### Damage Reservation

When a unit begins an attack windup or a projectile is in flight, the expected damage is **reserved** against the target. Other units factor this reserved damage into their targeting score, avoiding overkill on nearly-dead targets.

- **On windup start:** reserve raw damage with TTL = windup duration + 2s
- **On projectile launch (ranged):** replace windup reservation with flight-time reservation (TTL = travel time + 0.35s)
- **On melee hit:** reservation cleared immediately (damage applied)
- **On projectile hit:** reservation cleared by matching source entity
- **On target death:** all reservations removed with the entity
- **TTL expiry:** reservations that outlive their TTL are cleaned up each frame

This system is host-authoritative — reservations are never synced to clients.

### Attack Timing

Each unit has an **Attack Timing** profile that defines the granular phases of their attack animation:

| Unit | Attack Point | Backswing | Turn Rate | Min Range | Move in Backswing |
|---|---:|---:|---:|---:|---|
| Worker | 0.22s | 0.26s | 10.0 | — | yes |
| Soldier | 0.24s | 0.30s | 9.0 | — | yes |
| Archer | 0.20s | 0.36s | 8.0 | — | yes |
| Tank | 0.34s | 0.42s | 6.5 | — | no |
| Knight | 0.26s | 0.28s | 8.5 | — | yes |
| Mage | 0.30s | 0.34s | 7.0 | — | no |
| Priest | 0.28s | 0.32s | 7.0 | — | no |
| Cavalry | 0.22s | 0.24s | 9.5 | — | yes |
| Catapult | 0.42s | 0.55s | 4.0 | 5.0 | no |
| Battering Ram | 0.36s | 0.46s | 5.0 | — | no |
| Watch Tower | 0.16s | 0.22s | ∞ | — | — |
| Guard Tower | 0.18s | 0.24s | ∞ | — | — |
| Ballista Tower | 0.26s | 0.34s | ∞ | — | — |
| Bombard Tower | 0.34s | 0.46s | ∞ | 3.5 | — |

- **Attack point**: time before damage is dealt / projectile launches — responsive units (Archer, Cavalry) can stutter-step
- **Backswing**: recovery time after the attack point — some units can move during this phase
- **Turn rate**: how fast a unit rotates to face its target (towers are instant)
- **Minimum range**: siege units cannot attack targets closer than this distance

These values are stored as components but not yet wired into the attack pipeline (currently uses `AttackProfile.windup_secs` / `recovery_secs`). They will replace those fields in a future update to enable stutter-step micro and siege dead-zones.

### Damage Counter System

All units and buildings have an **Armor Type** and a **Damage Type**. Damage is multiplied based on the attacker's damage type vs the target's armor type:

| | vs Light | vs Heavy | vs Siege | vs Structure |
|---|---|---|---|---|
| **Melee** | 1.0x | 0.75x | 1.5x | 0.5x |
| **Pierce** | 1.0x | 0.5x | 0.25x | 0.5x |
| **Magic** | 1.25x | 1.25x | 0.5x | 0.75x |
| **Siege** | 0.5x | 0.75x | 0.5x | 3.0x |

**Unit type assignments:**

| Unit | Damage Type | Armor Type |
|---|---|---|
| Worker | Melee | Light |
| Soldier | Melee | Heavy |
| Archer | Pierce | Light |
| Tank | Melee | Heavy |
| Knight | Melee | Heavy |
| Mage | Magic | Light |
| Priest | Magic | Light |
| Cavalry | Melee | Heavy |
| Scout | Melee | Light |
| Catapult | Siege | Siege |
| Battering Ram | Siege | Siege |
| Towers | Pierce | Structure |
| Buildings | — | Structure |

### Neutral Camp Rewards

Clearing mob camps grants resources to the killing faction:

| Camp | Reward |
|---|---|
| Goblin (inner) | 30W, 15Cu |
| Skeleton/Orc (mid) | 50W, 30Fe, 20Go |
| Demon (outer) | 80W, 50Fe, 40Go |

### Unit Inventory and Item Drops

Units can now carry combat loot from neutral camps. This adds map objectives beyond raw resources, creates stronger timing windows after camp clears, and gives late-game armies more composition variety without replacing the core counter matrix.

**Inventory capacity by production tier:**

| Production source | Slot cap | Notes |
|---|---:|---|
| Age I production | 1 | Base, Barracks, Archery-range equivalent, basic spawns |
| Age II production | 2 | Stable, Workshop, advanced military production |
| Age III production | 3 | Siege Works, Mage Tower, Temple, elite production |

- Slot cap is fixed when the unit is trained or summoned
- Workers can pick up items but only gain passive effects; they cannot trigger weapon actives
- One item per category can be equipped at a time (`armor`, `helmet`, `ring`, `sword`, `staff`, `bow`)
- Melee units can equip `sword`; caster units can equip `staff`; ranged bow units can equip `bow`
- Rings are universal utility items and can be mixed with any valid main-hand category
- Items drop to the ground and must be manually picked up; they do not teleport into faction inventory
- Dropped items persist briefly after camp death, then decay if no faction claims them

**Drop structure:**

| Camp band | Expected drops | Item quality rule |
|---|---|---|
| Goblin inner camps | 0-1 item | Mostly utility rings, basic helmets, padded armor |
| Skeleton / Orc mid camps | 1 item | Standard armor and class weapons |
| Demon outer camps | 1-2 items | Rare helmets, stronger weapons, double-ring rolls |

- Camp bosses use weighted drop tables instead of guaranteed fixed rewards
- Last-hit faction owns pickup priority for a short grace window
- Neutral loot is visible to all factions after the ownership window expires

### Starter Item Pool

Each starter item is intentionally narrow: one item, one mechanic, one tactical reason to equip it.

#### Armor

| Item | Drop band | Mechanic |
|---|---|---|
| Padded Vest | Goblin | Reduces ranged projectile damage taken by 1 |
| Bronze Cuirass | Skeleton / Orc | First melee hit received every 12s deals 50% damage |
| Plate Cuirass | Demon | Wearer cannot be crit-bursted below 15% HP in one hit |

#### Helmets

| Item | Drop band | Mechanic |
|---|---|---|
| Kettle Helm | Goblin | Immune to bonus damage from high-ground ranged attacks |
| Viking Helm | Skeleton / Orc | Gain 20% move speed for 2s after getting a kill |
| Crusader Helm | Demon | Shortens silence, stun, and slow durations by 20% |

#### Rings

| Item | Drop band | Mechanic |
|---|---|---|
| Plain Band | Goblin | +1 inventory slot for carried consumable-style drops only |
| Golden Band | Goblin | Killing a neutral refunds a small amount of missing energy/mana |
| Wedding Band | Skeleton / Orc | While near one allied unit, both holders gain +1 armor |
| Linked Rings | Skeleton / Orc | Share 20% of overheal from allied priests into a small shield |
| Twin Rings | Demon | First ability cast after pickup has 35% shorter cooldown |
| Jewel Ring | Demon | Reveals hidden / stealthed neutral ambushers within a small radius |

#### Swords

| Item | Drop band | Mechanic |
|---|---|---|
| Arming Sword | Skeleton / Orc | Every third basic attack applies a short bleed |
| Viking Blade | Demon | Critically wounds targets below 35% HP for bonus execute damage |

#### Staves

| Item | Drop band | Mechanic |
|---|---|---|
| Battle Staff | Skeleton / Orc | Basic attacks splash a small amount of magic damage |
| Mage Crozier | Demon | First spell cast after standing still for 2s has bonus range |

#### Bows

| Item | Drop band | Mechanic |
|---|---|---|
| War Bow | Skeleton / Orc | Attack-move shots slow targets briefly |
| Yew Longbow | Demon | Remaining stationary for 1.5s grants one extended-range shot |

### Itemization Design Rules

- Items should create micro-decisions, not raw stat inflation
- Neutral drops should reinforce map control and skirmish timing
- Most mechanics should be readable from combat logs and simple HUD tooltips
- Avoid stacking multiple sources of the same trigger on one unit
- Early drops should be useful on basic units; late drops should unlock sharper specialist play

### Onboarding

- Contextual tips appear during the first 3 minutes of a match
- An idle worker notification button appears when workers are idle

### World and Presentation

- 500x500 default world with Grassland, Forest, Desert, Beach, Wetland, Water, and Mountain biomes
- Skeletal animation with blend transitions and facing interpolation
- Day/night cycle, sun and ambient lighting control, entity lights, VFX, and attention overlays
- Frustum culling pauses off-screen animation work
- Terrain traffic gradually forms visible road wear between active structures and routes

### AI and Automation

- Up to 3 AI factions can fill remaining match seats
- AI runs strategy, economy, tactical, and military layers
- Friendly AI can respect allied space, gather, expand, scout, rally, defend, and attack
- Multiplayer disconnects enter a 30-second reconnection grace period before handing the faction to AI

## Match Setup

- AI opponents: `0-3`
- AI difficulty: per slot
- Team modes: `FFA`, `Teams`, `Custom`
- Map size: `Small 300`, `Medium 500`, `Large 700`
- Resource density: `Sparse`, `Normal`, `Dense`
- Day cycle length: `5`, `10`, or `20` minutes
- Starting resources: `0.5x`, `1x`, `2x`
- Map seed: fixed or random
- Player name and player color
- Graphics: resolution, fullscreen, VSync, anti-aliasing, shadow quality, bloom, brightness, auto exposure, depth of field, chromatic aberration, entity lights, UI scale

## Multiplayer

### Current Status

The project has playable multiplayer via Matchbox WebRTC, supporting LAN, VPN, and internet play with NAT traversal.

- Transport: Matchbox WebRTC data channels (`bevy_matchbox`) — unified for native and WASM, with reliable + unreliable channels
- Model: host runs the full simulation, clients send inputs and receive authoritative sync
- Signaling: embedded signaling server on the host (port 3536, `ClientServer` topology) — no external server needed for LAN
- Lobby: host game, join by signaling URL (`ws://IP:3536/rts_room`) or direct IP
- NAT traversal: WebRTC ICE with STUN (Google STUN servers by default) enables internet play without VPN
- Web clients: the host serves the WASM build's `dist/` folder over HTTP on port 7880 — browser players on the same network open `http://<host-ip>:7880`
- Replication: delta-compressed state sync at ~10Hz, entity spawn/despawn, building sync, resource node amounts via NeutralWorldDelta, player resources, day/night cycle, and victory events
- Recovery: 30-second reconnection grace period with session tokens before AI takeover
- VPN/Hamachi: auto-detects VPN adapters, shows all available IPs, application-level heartbeat prevents tunnel dropout
- See [multiplayer-architecture.md](multiplayer-architecture.md) for the full protocol and system topology

### VPN / Hamachi Play

The multiplayer stack also works through Hamachi, ZeroTier, WireGuard, and similar VPN tools (though WebRTC NAT traversal often makes VPN unnecessary):

1. All players install and join the same VPN network
2. Host opens `Multiplayer` → `Host Game`
3. The lobby shows all detected IPs — look for the one tagged **[VPN]** (green text)
4. Share that VPN IP with clients (the Copy button copies the displayed session code)
5. Clients enter the signaling URL or just the VPN IP

The host's signaling server binds on all interfaces (`0.0.0.0`), so any adapter — LAN, Hamachi, ZeroTier, WireGuard — will accept connections.

### Current Limits

- Client commands are fire-and-forget with no rollback or server reconciliation
- The match model assumes four total faction seats shared between humans and AI
- Internet play requires the signaling server to be reachable (port 3536); TURN relay is not yet configured for symmetric NAT

### Quick Start

#### Host

1. Open `Multiplayer`
2. Choose `Host Game`
3. Share the displayed session code (signaling URL)
4. Start once players are connected

#### Client (Native or WASM)

1. Open `Multiplayer`
2. Choose `Join Game`
3. Enter the session code (signaling URL like `ws://IP:3536/rts_room` or just the host IP)
4. Wait for host start

#### Client (Web Browser on LAN)

1. Open the URL shown in the host lobby (e.g., `http://192.168.1.5:7880`)
2. Choose `Join Game`
3. Enter the host session code
4. Wait for host start

The host automatically serves the WASM client when a `dist/` directory is present. Web and native clients can play together in the same lobby.

### Network Debug Tap

The local network debug tap exposes recent events over HTTP on `127.0.0.1`, defaulting to ports `8787-8795`.

- `GET /health`
- `GET /events`
- `GET /events?since=<id>`
- `POST /clear`

Use `RTS_NET_DEBUG_PORT` to pin the port.

## Persistence and Tooling

- Local save/load exists and serializes entities, fog exploration, resource nodes, explosive props, rally points, upgrades, and training state
- Save path: `saves/save.json`
- Graphics settings persist to `config/graphics_settings.json`
- Debug tweak values persist to `config/debug_tweaks.json`
- Save/load controls currently live inside the in-game debug flow

## Controls

### Camera

| Input | Action |
|---|---|
| `W / A / S / D` | Pan camera |
| Arrow keys | Alternate pan |
| `Q / E` | Rotate camera |
| Scroll wheel | Zoom |
| `+ / -` | Alternate zoom |
| Screen edges | Edge-scroll |
| `Space` | Focus on selection (or last selection) |

### Selection

| Input | Action |
|---|---|
| Left click | Select unit or building |
| Left drag | Box-select units |
| `Shift` + click | Add or remove from selection |
| `Ctrl + 1-9` | Assign control group |
| `Shift + 1-9` | Add to control group |
| `Alt + 1-9` | Steal into control group (assign selected, remove from other groups) |
| `1-9` | Recall control group |
| `Tab` | Cycle through subgroups in current selection |

### Orders

| Input | Action |
|---|---|
| Right click ground | Move |
| Right click enemy | Attack |
| Right click resource | Gather with workers |
| Right click construction | Assign workers to build |
| Right click processor | Assign workers to processor |
| `F` + left click | Attack-move |
| `P` + left click | Patrol |
| `H` | Hold position |
| `X` | Stop (cancel all orders) |
| `V` | Cycle stance |
| `G` | Cycle formation |
| `L` | Toggle unit labels |
| `Q` | Use first ability |
| `W` | Use second ability |
| `Escape` | Cancel command mode |

### Building and UI

| Input | Action |
|---|---|
| Build button | Enter placement preview |
| `Found Base` | Start first-base placement |
| `Wall` | Start wall plotting |
| `Gatehouse` | Convert wall segment |
| Left click | Confirm placement |
| Right click / `Escape` | Cancel placement |
| `H` / `J` | Rotate placement left / right 90° |
| Rally button | Enter rally mode |
| `F1` | Resources |
| `F2` | Army Overview |
| `F3` | Selection |
| `F4` | Actions |
| `F5` | Minimap (also Quicksave when no overlay) |
| `F6` | Production Queue |
| `F7` | Tech Tree |
| `F8` | Control Groups |
| `F9` | Event Log (also Quickload when no overlay) |
| `F10` | Debug |

## Reference Data

### Biomes

| Biome | Terrain Color | Primary Resource | Secondary Resource |
|---|---|---|---|
| Grassland | Green | Stone | Copper, Iron |
| Forest | Dark Green | Wood | Copper |
| Desert | Sandy yellow | Copper | Iron |
| Beach | Sand | Stone | — |
| Wetland | Olive green | Iron | Copper |
| Water | Blue | Oil | — |
| Mountain | Gray/white | Stone | Iron, Copper |

### Buildings

| Type | Cost | Build Time | Requires | Age | Function |
|---|---|---|---|---|---|
| Base | 90W 15I 20Sn | 15s | — | I | Anchor, trains Workers, researches Ages |
| Barracks | 75W 30I | 12s | Base | I | Trains Workers, Soldiers, Scouts |
| Storage | 55W 15I 10Sn | 8s | Base | I | Depot with gather aura |
| Sawmill | 50W 15I | 12s | Base | I | Produces Planks and Charcoal |
| Mine | 70W 35I | 15s | Base | I | Ore processing and upgrades |
| House | 45W 10I 10Sn | 7s | Base | I | Increases unit cap (+4/6/8 per level) |
| Outpost | 20W 10I 8Sn | 6s | Base | I | Vision and wall-control anchor |
| Watch Tower | 35W 15I 15Sn | 8s | Base | I | Early defense |
| Wall Segment | 12W 8Sn | 4s | Base | I | Plotted wall section |
| Wall Post | 16W 10Sn | 5s | Base | I | Wall endpoint or junction |
| Workshop | 90W 25C 55I 15G 10Bronze | 18s | Mine | II | Tier 2 military tech |
| Stable | 85W 30C 45I | 14s | Barracks | II | Cavalry production |
| Smelter | 80W 20C 40I | 16s | Mine | II | Produces Bronze and Steel |
| Guard Tower | 60W 20C 45I 25Sn | 11s | Barracks | II | Durable general defense |
| Oil Rig | 75W 25C 35I | 14s | Workshop | II | Water-biome oil processing |
| Gatehouse | 40W 10C 35I 20Sn | 10s | Outpost | II | Replaces a wall segment |
| Siege Works | 100W 35C 90I 30G | 20s | Workshop | III | Siege production |
| Mage Tower | 80W 30C 40I 55G | 20s | Workshop | III | Mage and Priest production |
| Temple | 90W 20C 40I 70G | 22s | Mage Tower | III | Priest training and healing aura |
| Alchemist | 60W 30I 25G 15O | 18s | Smelter | III | Produces Gunpowder |
| Ballista Tower | 70W 55C 80I 30Sn | 14s | Siege Works | III | Anti-heavy tower |
| Bombard Tower | 85W 45C 65I 35G 25Sn | 15s | Mage Tower | III | Splash tower |

### Training Costs

| Unit | Cost | Train Time | Trained At | Damage/Armor |
|---|---|---|---|---|
| Worker | 30W | 5s | Base, Barracks | Melee / Light |
| Soldier | 20W 15I | 8s | Barracks | Melee / Heavy |
| Archer | 25W 10I | 7s | Barracks | Pierce / Light |
| Scout | 15W | 4s | Barracks | — / Light |
| Tank | 20C 50I 15G 5O 5Steel | 15s | Workshop | Melee / Heavy |
| Knight | 20W 15C 45I 20G 5Bronze | 12s | Stable | Melee / Heavy |
| Mage | 15W 50G | 15s | Mage Tower | Magic / Light |
| Priest | 15W 30G | 12s | Mage Tower, Temple | Magic / Light |
| Cavalry | 25W 10C 25I 10G | 10s | Stable | Melee / Heavy |
| Catapult | 80W 60I 20G 5Gunpowder | 20s | Siege Works | Siege / Siege |
| Battering Ram | 100W 40I 15Planks | 18s | Siege Works | Siege / Siege |

### Unit Stats

| Type | HP | Speed | Damage | Range | Cooldown | Abilities |
|---|---|---|---|---|---|---|
| Worker | 80 | 5.0 | 6 | 1.8 | 1.2s | — |
| Soldier | 120 | 4.5 | 12 | 2.0 | 1.0s | Upgrades to Knight |
| Archer | 100 | 5.5 | 10 | 12.0 | 1.5s | — |
| Scout | 40 | 8.0 | 0 | 0 | — | High vision (25), no combat |
| Tank | 250 | 3.0 | 18 | 2.5 | 2.0s | — |
| Knight | 200 | 6.0 | 18 | 2.5 | 0.8s | Charge, Shield Bash |
| Mage | 70 | 4.0 | 15 | 14.0 | 2.0s | Fireball, Frost Nova |
| Priest | 80 | 4.5 | 6 | 10.0 | 2.0s | Heal, Holy Smite |
| Cavalry | 150 | 7.0 | 14 | 2.0 | 0.9s | — |
| Catapult | 150 | 2.0 | 40 | 25.0 | 5.0s | Boulder Throw |
| Battering Ram | 200 | 2.5 | 50 | 2.0 | 4.0s | — |

### Population Upkeep

| Unit Count | Income Modifier |
|---|---|
| 0-20 | 100% |
| 21-40 | 85% |
| 41-60 | 70% |
| 60+ | 50% |
