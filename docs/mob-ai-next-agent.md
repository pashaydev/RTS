# Mob AI Rewrite — Handoff for the Next Agent

## Who this is for

You're picking up a deterministic-lockstep RTS and rewriting the mob AI into a
**tower-defense-style night attack** system. Current mobs spawn in 3–5 unit
clusters, patrol around a cluster center, aggro on the closest player entity
within range, and leash back to the cluster after 40u. That produces cave-camp
RPG gameplay, not TD. You're replacing it.

Priority: **simple, solid, engaging TD feel.** Predictable enough that the
player can plan defenses, varied enough to stay tense across nights.

---

## Project context (non-negotiable)

- **Bevy 0.18**, Rust stable, deterministic lockstep RTS. See
  `/Users/pashayakubovsky/.claude/projects/-Users-pashayakubovsky-Desktop-Gamedev-rts/memory/MEMORY.md`
  for Bevy 0.18 API notes.
- Sim runs in `FixedUpdate` at 30 Hz. Visuals in `Update`.
- **Determinism rules — no exceptions:**
  - All damage routes through `src/simulation/combat/damage.rs::apply_damage`.
  - No `Instant::now()` / `SystemTime::now()` / `rand::rng()` (thread-local) in
    simulation. Seed every RNG from `MapSeed` + tick or entity id.
  - `BTreeMap` (not `HashMap`) for any collection whose iteration drives sim state.
  - Systems touching sim live in `FixedUpdate`, not `Update`.
  - Use `SimClock.tick` (not `Time::elapsed`) for tick-indexed decisions.
- `cargo check` must stay clean at every commit.
- Don't name anything "V2" / "New" / "Legacy". Replace the code, don't shim it.

---

## Target design — what "good" looks like

A TD night feels like this:

1. **Warning window.** At Dusk, show a pre-wave banner: "Night N — M
   attackers incoming from the {north/south/east/west/all sides}." Gives the
   player time to rally units and repair walls.
2. **Distributed spawn at map perimeter.** Mobs appear **one at a time** at
   the map edge, at random angles. No clusters, no camps.
3. **Spawn stream over the night.** A wave of 20 mobs across a 4-minute night
   drips them in one every ~10s (with jitter). The front of the wave can start
   fighting the player while the rest are still arriving.
4. **Every mob commits to an attack target immediately on spawn.** It picks
   the **nearest player entity** (heuristic: building > unit; see "Target
   selection" below) and charges. It does not patrol, does not wander, does
   not leash. It fights until dead or the night ends.
5. **Dawn cleanup.** At Dawn, any surviving mobs despawn (or retreat and
   despawn off-screen). The night is over; players rebuild.
6. **Difficulty ramps.** Night N+1 spawns more mobs, spawns faster, mixes in
   tougher types (goblin → orc → ogre-equivalent). The curve should reward
   defending successfully so pressure increases but not collapse.

Numbers are illustrative — tune in playtest. Ballpark start:
- Night 1: 8 mobs, 8s between spawns, all goblins.
- Night 5: 25 mobs, 4s between spawns, goblins + 1 heavy unit near the end.
- Night 10: 50 mobs, 2s between spawns, mixed composition.

---

## Current state — what you're replacing

```
src/simulation/mobs.rs
  ├── NightWaveState resource (keep — refactor the fields)
  ├── generate_wave_clusters()          ❌ DELETE — no more clusters
  ├── spawn_night_cluster()             ❌ DELETE
  ├── night_wave_spawn_system()         🔁 REWRITE — per-night schedule, not one-shot
  ├── spawn_mob_member()                🔁 REWRITE — just spawn, no PatrolState
  ├── mob_patrol()                      ❌ DELETE — no patrolling
  ├── mob_aggro()                       🔁 REWRITE — run once on spawn, not per-tick scan
  ├── mob_leash()                       ❌ DELETE — mobs don't leash, they commit
  ├── cluster_reward()                  ❌ DELETE — rewards move to per-mob or per-wave bounty
  └── cluster_item_drops()              🔁 REPURPOSE — drop per-mob by rarity table
```

`PatrolState` / `PatrolStateKind` components → can be deleted or repurposed.

Types to revisit / delete:
- `CampReward` (was attached to cluster leader) → either delete or turn into
  a per-night completion bounty paid to all surviving players at Dawn.
- `CampItemDrops` (was attached to cluster leader) → turn into a per-mob
  low-probability drop or a per-night loot reward.

Keep using `Mob`, `Faction::Neutral`, `AggroRange`, `Goblin` / mob kinds.
`UnitBrain` + `Abilities` are already wired on spawn in
`src/blueprints/spawn.rs::EntityCategory::Mob`.

---

## Concrete implementation sketch

### 1. `NightWaveState` (rewritten)

```rust
#[derive(Resource)]
pub struct NightWaveState {
    pub enabled: bool,
    pub night_count: u32,
    pub prev_phase: DayPhase,
    pub active: Option<ActiveWave>,   // Some() during night, None during day
    pub force_wave: bool,
}

pub struct ActiveWave {
    pub night: u32,
    pub total_to_spawn: u32,
    pub spawned_so_far: u32,
    pub next_spawn_tick: u64,         // SimClock.tick when the next mob emerges
    pub spawn_interval_ticks: u64,    // base interval; add jitter per spawn
    pub composition: Vec<(EntityKind, u32)>,  // (kind, count); consumed as mobs spawn
}
```

### 2. Spawn scheduler (replaces `night_wave_spawn_system`)

Runs every FixedUpdate tick in `SimSet::Ai`:

- **On Day→Night transition**: compute the wave (`total_to_spawn`,
  `spawn_interval_ticks`, `composition`) from `night_count`. Seed the wave RNG
  from `map_seed ^ night_count.mul(PHI_U64)` for determinism.
- **While `active` is Some**: if `SimClock.tick >= next_spawn_tick` and
  `spawned_so_far < total_to_spawn`, spawn one mob at the map perimeter.
  Advance `next_spawn_tick`.
- **On Night→Dawn transition**: clear `active`. Optionally despawn or flee
  all surviving mobs.

### 3. Perimeter spawn point selection

For each spawn:

```rust
let angle = rng.random_range(0.0..TAU);      // deterministic per-mob
let r = half_map - rng.random_range(1.5..4.0);  // slightly inside the border
let mut pos = Vec3::new(angle.cos() * r, 0.0, angle.sin() * r);
// Retry sampling if pos falls on Water/Mountain (bounded attempts).
```

Optionally bias `angle` — e.g. concentrate most spawns on one side of the map
to create a "direction of threat" per night. Keep deterministic.

### 4. Target selection (on spawn)

Each mob picks a target once, on spawn. Heuristic:

```rust
// Pseudo-code
let candidates = all_player_buildings + all_player_units;
let scored = candidates.map(|e| {
    let d = mob_pos.distance(e.pos);
    let kind_bonus = match e.kind {
        Base | Storage => -20.0,      // most attractive
        EntityKind::Sawmill | Mine | OilRig | Smelter | Alchemist => -10.0,
        Building => -5.0,
        _ => 0.0,                      // units neutral
    };
    d + kind_bonus
});
let target = scored.min_by(...);
apply_auto_attack_intent(commands, mob, target, mob_pos, now);
```

**Why buildings over units**: TD feel. Mobs walking past workers to smash the
Base gives players a reason to build walls/towers, not just units. If the
player's units intercept along the way, combat auto-aggro + retaliation
handles the fight; mob_aggro doesn't need to re-scan mid-path.

### 5. Ongoing retarget (per-mob)

After the initial target is chosen, mobs should NOT constantly rescan. But
they should handle two cases:

- **Target died**: re-pick nearest from same heuristic, once. Use a
  think-timer to avoid thrashing: at most one rescan per mob per second.
- **Blocked by an enemy unit**: let the existing combat pipeline handle it —
  `retaliation.rs` already flips the mob to attacking whoever hit it, and
  wall-redirect in `approach_target` handles walls in the line. No extra
  mob-specific code needed.

### 6. Wave composition

Start simple. A table indexed by night number:

```rust
fn compose_wave(night: u32) -> (u32, u64, Vec<(EntityKind, u32)>) {
    let total = (8 + night * 2).min(80);
    let interval_ticks = (240u64.saturating_sub(night as u64 * 12)).max(60); // 8s down to 2s at 30Hz
    // Add heavier mobs as nights escalate. Add new EntityKinds if needed.
    let mut comp = vec![(EntityKind::Goblin, total)];
    // night >= 3: substitute a few Orcs (if/when added)
    (total, interval_ticks, comp)
}
```

### 7. Day cleanup

On Night→Dawn:

- Option A (simple): despawn all remaining mobs. Award any unclaimed loot to
  the nearest player building.
- Option B (nicer): give surviving mobs `Order::Move(off_map_point)` and
  despawn them when they cross the border. No combat during retreat.

Pick A for v1. Ship the dumb thing; revisit.

---

## Unit-side problems to review and suggest fixes for

These are pre-existing issues that will bite TD gameplay specifically. Skim
them, pick the ones that actually manifest, and either fix or document.

### 🔴 Non-deterministic RNG in spawn
`src/blueprints/spawn.rs` around line 139 uses `rand::rng()` for
`MovementSmoothing.speed_variation` and `IdleBehavior.fidget_timer`. Those
are thread-local RNGs and will desync in lockstep. **Fix**: derive from
`entity.to_bits()` or pass `GameRng` into spawn. Verify checksum stays
stable in a 2-peer 5-minute night.

### 🟡 Workers during raids
When a raid hits, player workers near a Mine might keep gathering while
goblins beat on them. Current `UnitStance` defaults to Defensive for workers
(good) — they should auto-aggro the nearest goblin via `auto_aggro_and_attackmove`.
**Verify**: workers actually switch to fighting. If not, check their
`scan_multiplier` via `CombatTuning::scan_multiplier(UnitStance::Defensive) = 1.5`
and the auto-aggro gate (`brain.state == Idle` requirement — workers in
`UnitState::AssignedGathering` might never reach Idle).

**Likely finding**: workers assigned to a processor (`BuildingAssignment`) stay
in worker_fsm states and `resolve_orders` skips them. Mobs will kill them
unmolested. Options:
- (a) Let workers auto-aggro when close enough to a hostile (override
      AssignedGathering via retaliation + damage memory — already partially
      there via `retaliation.rs`).
- (b) Leave workers as "gather-til-dead" and tell the player to garrison.
- (c) Give workers a tiny flee-radius reaction to hostiles, back to base.

Pick (a) — retaliation already fires when hit; verify the worker actually
abandons AssignedGathering on `RecentCombatDamage`.

### 🟡 Ranged mob support
Current `TacticalRole::RangedKiter` retreat logic was just removed (see
`docs/combat-next-agent.md`). If a later night introduces ranged mobs
(e.g. goblin archer), they'll stand at 75% of max range and shoot — which
is fine for basic TD. Don't re-introduce kiting until the baseline is
bulletproof.

### 🟡 NavGrid performance under load
With 50+ mobs all pathing toward the Base at once, `src/world/pathfinding.rs`
will compute a lot of A* paths. **Verify** frame times stay under 10ms for a
wave. If not, consider batched pathfinding (compute once for the first mob
on a spawn edge, cache it, reuse for mobs spawning at the same edge within
N seconds). Don't pre-optimize — measure first.

### 🟡 Mob faction hostile-to-all check
Mobs are `Faction::Neutral`. Player units in `TeamConfig::is_hostile()` need
to be hostile against Neutral. **Verify** in `src/types/teams.rs` (or
wherever `is_hostile` lives). If not, mobs will be ignored by auto-aggro.

### 🟡 Target death cascade
When a mob's target dies (e.g. Sawmill destroyed), `handle_death` releases
all attackers targeting it — they go `Order::Stop, state = Idle`. The next
tick, `auto_aggro_and_attackmove` scans for a new target. **Verify** the
mob re-picks a *building*, not the nearest worker — if the player tactic is
"throw workers at the mob to distract it from the base," the TD feel breaks.
Consider a mob-specific retarget hook that re-runs the same building-prefer
heuristic.

### 🟡 `CombatBudget::max_target_rescans_per_frame`
Already defaults to `usize::MAX`. With 50 mobs rescanning every tick (old
aggro), that would be a lot; with one-shot on-spawn targeting (new design),
it's fine. Leave alone, but make sure the new code doesn't rescan every
tick per mob.

### 🟡 Mob visual density
Goblin LOD system (recent work, per MEMORY.md) uses impostor billboards at
range. With 50 mobs, verify the LOD kicks in before draw-call explosion.
If frame time spikes past 16ms, cap peak concurrent mob count per night
(spawn fewer, make them tougher).

### 🟢 Pack-alert radius (deletable)
Current `mob_aggro` uses `PACK_ALERT_RADIUS=15.0` to wake neighbors when
one mob engages. In the new design, every mob already has a target on
spawn — the pack-alert code is dead weight. Delete when rewriting `mob_aggro`.

---

## Files you'll most likely touch

- `src/simulation/mobs.rs` (main rewrite)
- `src/types/combat.rs` (delete `PatrolState`, `PatrolStateKind`,
  `CampReward`, `CampItemDrops` if you don't re-use them)
- `src/simulation/combat/death.rs` (remove `PatrolState` references in
  `handle_death` if you delete the component)
- Possibly `src/blueprints/spawn.rs` (don't spawn `PatrolState` into mob
  entities anymore)
- `src/ui/event_log_widget.rs` or similar (pre-wave warning banner)

## Files you should NOT touch

- `src/simulation/combat/*.rs` — combat pipeline was just stabilized. See
  `docs/combat-next-agent.md`. Don't reopen it.
- `src/simulation/combat/damage.rs` — THE chokepoint.
- `assets/combat/abilities/*.ron` — tune later if needed.
- `src/infrastructure/save_load.rs` — saves are acceptably broken.

---

## Verification plan

### Single-player first

1. Start a new game. Advance time to Dusk. Confirm a pre-wave banner fires.
2. Night begins. Mobs start appearing at the perimeter, one every ~8s.
3. Watch a mob's path: it should head straight toward a player building
   (not a random worker) and engage combat.
4. Player walls/towers are actually used — mobs swing on them, don't path-
   phase through.
5. Player units intercept mobs in the field — combat resolves cleanly
   (hits land, no oscillation). Verify by watching HP tick down.
6. Night ends. Surviving mobs despawn at Dawn. No stuck goblins.
7. Re-trigger: Night 5 has noticeably more mobs with a shorter interval.
8. Die: if the mobs destroy the Base, loss condition fires (whatever
   `victory.rs` does).

### Lockstep (once single-player works)

- Two native peers, same map, watch a full day/night.
- `DesyncDetected` resource must not fire.
- Usual suspects if it does: `rand::rng()` in spawn path, `HashMap`
  iteration driving sim, `Instant::now()` instead of `SimClock.tick`.

### Stress

- Crank `NightWaveState.base_count` to 100 via dev tweak and play a night.
- Frame time stays under 16ms (60fps native). If not, investigate the
  "NavGrid performance under load" finding above.

---

## Non-goals — do NOT do these now

- Don't add a "build-day / fight-night" hard mode lockout. The player
  should be free to expand during the day at their own pace.
- Don't reintroduce clusters or mob camps for day-time "farm loot" — that
  was RPG-mode, not TD.
- Don't add mob spellcasters, bosses, or elite variants yet. Baseline first.
- Don't change `CombatTuning` defaults unless a playtest proves it.
- Don't add per-player separate spawn pools. One global wave targets all
  players' entities.
- Don't add a minimap warning indicator (nice-to-have, not blocker).
- Don't touch lockstep/checksum/save_load.

---

## Priorities (in order)

1. Mobs spawn 1-by-1 at perimeter over the night.
2. Each mob picks nearest player building/unit and attacks.
3. Mobs don't patrol, don't leash, don't orbit.
4. Difficulty ramps across nights.
5. Pre-wave warning.
6. Dawn cleanup.

Get #1–3 rock-solid first. Polish is cheap once the core loop works.

---

## Rules

- `cargo check` must stay clean at every commit.
- Deterministic RNG (seeded from `MapSeed` + `SimClock.tick` or entity bits).
- `BTreeMap` over `HashMap` for sim-iterating collections.
- Every damage path ends in `apply_damage()`.
- Sim systems in `FixedUpdate` only.
- Test single-player first, lockstep second.

Good luck. The combat layer can handle the pressure now; what's missing is
an AI that produces sustained, directed pressure instead of random orbits.
