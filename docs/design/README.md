# neueOS design specs

The human-readable side of neueOS's look. The code is the source of truth: every value in
these documents lives in `neue-theme` (tokens) or `neue-paint` (drawing), and the documents
say what the values *mean* and when to use them. If a document and the code disagree, the
code wins, and the document is a bug: fix it in the same PR that changes the code.

| Document | Covers | Code |
|---|---|---|
| [tokens.md](tokens.md) | Variants, semantic colours, spacing, control sizes, radii, type scale | `crates/neue-theme/src/{lib,colors,metrics,radii,typography}.rs` |
| [materials.md](materials.md) | Every drawn material: layers, light and dark, translucency, rules for new ones | `crates/neue-theme/src/materials.rs`, `crates/neue-paint/src/materials/` |
| [motion.md](motion.md) | Durations, spring presets, ambient loops, reduced motion, where each is used | `crates/neue-theme/src/motion.rs`, `crates/neue-kit/src/anim.rs` |

## What neueOS is aiming for
A reinterpretation of OS X Mountain Lion's skeuomorphic design with modern technology, not a
copy. Concretely:
- **Physical surfaces:** linen, brushed grey bars, glass, gel. Each is built from vector
  layers (gradients, strokes, blurred shadows) or a procedural texture, never from bitmaps.
  They stay sharp at any scale.
- **Original artwork only.** No Apple icons, textures, fonts, sounds or trademarked names
  in user-facing strings (AGENTS.md, hard rules). Values here were tuned by eye against the
  *feel* of the original, then written down.
- **Light and dark are equals.** Every material has both (not an inversion; the dark look is
  drawn on purpose), plus a graphite accent and an increased-contrast setting.
- **Motion is quick and physical:** controls answer in about a tenth of a second, panels
  settle in a third, whole-screen changes in under half a second, and everything snaps under
  reduced motion.

## Rules for anyone touching the look
1. **No hard-coded colours, metrics or durations** in widgets, shell or compositor code. If
   a designer might tune it, it is a token. If it belongs to an algorithm (noise periods,
   hash seeds, gradient stop positions, proportions of a shape relative to its own size), it
   stays in the material (ADR 0031 §4).
2. **Blue accent, standard contrast, light is the reference look.** Goldens were drawn from
   it; graphite, dark and increased contrast derive from it.
3. **Every material needs light and dark variants**, a golden per state × appearance ×
   scale 1 and 2, and an entry in the widget gallery.
4. **Respect the user's settings:** appearance, accent, contrast and reduced motion arrive
   through `Theme`. Never read them from anywhere else.
5. **Update these documents** when a token or material changes meaning, appears or goes away.

## Related decisions
ADR 0013 and 0038 (UI font), 0031 (token crate), 0036 (springs), 0039 (translucent
materials), 0040 to 0062 (widget families and their materials), 0055 (client-side chrome).
