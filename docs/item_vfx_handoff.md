# Item VFX Handoff

Purpose: give future agents a clear brief for replacing the current equipped-item prototype with small, readable, tasteful VFX that actually match each item mechanic.

## Current problem

The current equipped-item VFX pass is not good enough.

- It is too generic.
- It uses the same visual language for unrelated mechanics.
- Several effects do not communicate gameplay intent.
- Some items should not have persistent orbiting geometry at all.
- Radius effects and proc effects need different treatment.

Agents working on item VFX should treat the current prototype as disposable.

## Global direction

Target style:

- Small, readable, low-noise.
- Grounded in RTS readability, not hero-shooter spectacle.
- Mostly 1-2 shapes per item effect.
- Stable while idle, stronger only on trigger.
- Color should come from item identity, but shape/motion should explain mechanic.
- VFX should not hide unit silhouettes, health bars, or selection readability.

Avoid:

- Constant orbiting balls unless the item is explicitly about tracking, linking, or sensing.
- Large full-body glows for passive stat items.
- Tall billboard effects on equipped units.
- Pickup-style beam language reused on equipped units.
- Visuals that look like spells when the item is only passive defense.

## Visual grammar by mechanic type

Use this grammar consistently:

- Passive defense: subtle armor sheen, shoulder/torso edge glint, occasional pulse on hit.
- Anti-CC / resistance: short crown or headband pulse near the head, only periodic or on control interaction.
- Movement burst: brief trailing streaks on movement, not permanent aura.
- Detection / scouting: faint scan ripple or intermittent reveal ping.
- Extra slot / inventory utility: almost no world VFX; UI emphasis is better than in-world noise.
- Ally aura / radius buff: thin ground ring with occasional outward pulse.
- Kill reward / energy restore: brief intake spark when a kill happens, not permanent effect.
- Cooldown / first-cast prep: tiny charge mark that appears only when primed or ready.
- Shield / overheal conversion: short shell shimmer only while shield exists.
- Attack proc: weapon flash on proc timing, not permanent orbit.
- Execute / low-HP finisher: target-side accent when valid target is low, not owner-side permanent aura.
- Splash magic attacks: impact-side secondary burst, not caster idle aura.
- Braced / stationary bonus: stance indicator when the condition is active.
- Attack-move slow: shot trail tint or impact tag, not idle effect.

## Item-by-item brief

### Armor

`Padded Vest`
- Mechanic: reduce ranged hit damage.
- Good VFX: soft fabric-toned hit dampening puff or short chest shimmer only when taking ranged damage.
- Idle VFX: none or almost none.

`Bronze Cuirass`
- Mechanic: first melee hit is softened.
- Good VFX: a small bronze plate flash when the protection is available; stronger deflect flash on the consumed hit.
- Idle VFX: tiny readiness glint every few seconds, centered on torso.

`Plate Cuirass`
- Mechanic: prevents one-shot burst threshold.
- Good VFX: a restrained silver emergency barrier flicker when lethal threshold prevention matters.
- Idle VFX: none; maybe a very faint steel edge shine.

### Helmets

`Crusader Helm`
- Mechanic: shorter silence, stun, and slow.
- Good VFX: faint halo band around the head, with a clean snap pulse when CC is reduced.
- Idle VFX: minimal head-level ring, very low alpha.

`Kettle Helm`
- Mechanic: ignores high-ground ranged bonus.
- Good VFX: only on incoming ranged attacks from height advantage, show a short upward-facing deflection glint above the helm.
- Idle VFX: none.

`Viking Helm`
- Mechanic: move burst after a kill.
- Good VFX: brief wind streaks at feet and behind the unit when the kill-triggered move burst is active.
- Idle VFX: none.

### Rings

`Jewel Ring`
- Mechanic: reveals hidden neutral ambushers.
- Good VFX: intermittent scouting pulse around the wearer, like a low thin scan ring or shimmer ping.
- Idle VFX: very sparse, slow scan only.

`Plain Band`
- Mechanic: adds one consumable-only carry slot.
- Good VFX: none in world.
- UI note: if anything, this should be expressed in inventory UI, not gameplay space.

`Wedding Band`
- Mechanic: nearby ally pair gains armor.
- Good VFX: thin ground aura around the owner plus a subtle line or paired pulse to the linked ally when active.
- Idle VFX: only if paired and in range.
- This is the best candidate for a persistent world aura.

`Golden Band`
- Mechanic: neutral kills restore energy.
- Good VFX: a small gold intake spark that flows into the wearer on qualifying kill.
- Idle VFX: none.

`Twin Rings`
- Mechanic: first ability gets shorter cooldown.
- Good VFX: a tiny dual-glyph charge marker that appears only while the bonus is unspent.
- Idle VFX: conditional, not permanent after consumption.

`Linked Rings`
- Mechanic: overheal becomes a small shield.
- Good VFX: thin shell shimmer on the unit only while overheal shield exists.
- Idle VFX: none unless shield is currently present.

### Swords

`Arming Sword`
- Mechanic: every third attack causes bleed.
- Good VFX: small weapon-edge pulse when the next hit is the bleed hit; short red slash accent on proc.
- Idle VFX: only while the third hit is primed.

`Viking Blade`
- Mechanic: bonus execute damage on low HP.
- Good VFX: target-side low-health mark or brief crimson edge on the wielder’s blade when attacking an execute-valid target.
- Idle VFX: none.

### Staffs

`Battle Staff`
- Mechanic: basic attacks splash magic damage.
- Good VFX: impact-side secondary arcane ripple hitting nearby units.
- Idle VFX: maybe a tiny staff-tip ember, but keep it small.

`Mage Crozier`
- Mechanic: first spell after bracing gains range.
- Good VFX: while braced and primed, show a restrained focus sigil near the staff head; consume it on cast.
- Idle VFX: conditional only while braced bonus is ready.

### Bows

`Yew Longbow`
- Mechanic: stationary shot gets bonus range.
- Good VFX: when the unit has remained still long enough, show a faint drawn aim line or leaf-toned focus arc near the bow.
- Idle VFX: conditional on stationary-ready state only.

`War Bow`
- Mechanic: attack-move shots apply a slow.
- Good VFX: only on attack-move arrows, tint trail and add a short impact slow tag.
- Idle VFX: none.

## Implementation rules

- Prefer trigger-driven VFX over permanent idle VFX.
- If an item has no meaningful active state, default to hit/cast/proc feedback instead of constant display.
- Persistent effects should be reserved for:
  - `Wedding Band`
  - `Jewel Ring`
  - `Twin Rings` while primed
  - `Linked Rings` while shield exists
  - `Mage Crozier` while braced and primed
  - `Yew Longbow` while stationary-ready
- `Plain Band`, `Golden Band`, `Viking Helm`, `Kettle Helm`, `War Bow` should not have always-on idle geometry.
- Most items should be implemented with particles, short flashes, or transient decals, not always-on 3D meshes.

## Suggested engineering split

Agent 1: mechanic state audit
- Find which item mechanics are actually implemented versus only described in text.
- Produce a table: item, mechanic state source, trigger event, active window, missing gameplay logic.

Agent 2: visual framework
- Build a small reusable item-VFX framework.
- Support:
  - trigger-on-event flashes
  - conditional persistent state VFX
  - owner-side and target-side effects
  - ground ring helper for radius effects
- Keep it data-driven enough that items do not require bespoke systems where avoidable.

Agent 3: aura/state visuals
- Implement persistent or conditional visuals only for items that need them:
  - `Wedding Band`
  - `Jewel Ring`
  - `Twin Rings`
  - `Linked Rings`
  - `Mage Crozier`
  - `Yew Longbow`

Agent 4: proc/impact visuals
- Implement event-driven visuals for:
  - `Padded Vest`
  - `Bronze Cuirass`
  - `Plate Cuirass`
  - `Crusader Helm`
  - `Kettle Helm`
  - `Viking Helm`
  - `Golden Band`
  - `Arming Sword`
  - `Viking Blade`
  - `Battle Staff`
  - `War Bow`

## Acceptance criteria

- Every item has a visual concept that matches its mechanic.
- At least half of the items have no permanent idle VFX.
- Radius mechanics read as radius mechanics.
- Proc mechanics read on trigger, not at all times.
- Defensive items feel defensive, not magical.
- Utility items do not clutter battles.
- Multiple equipped items on one unit remain readable.
- Visuals do not noticeably worsen large-battle readability.

## Cleanup note

If agents keep the current prototype temporarily, they should still remove:

- generic orbiting shapes from items without active state
- any persistent effect on `Plain Band`
- pickup-like visuals attached to equipped units
- large body-centered rings that do not correspond to actual radius mechanics
