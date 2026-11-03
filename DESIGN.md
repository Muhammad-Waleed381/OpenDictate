# OpenDictate design

Mode: Operate. Bright, confident and playful, with quiet working surfaces. Replaces the earlier black-and-white neo-brutalist look.

## Colour

Three colours chosen by the owner: Marshmallow White, Galactic Violet, Bubblegum Pink. Tokens live in `src/index.css`, with light and dark values for each.

| Role | Light | Dark | Used for |
|---|---|---|---|
| Ground | `#FFFFFF` | `#111111` | Window and sidebar |
| Surface | `#FFFFFF` | `#1B1B1B` | Cards, separated from the ground by a 1px border |
| Marshmallow (`--muted`) | `#EFECF0` | `#222222` | Selected sidebar item, hover states, chips, tracks |
| Ink | `#1A1240` | `#EFECF0` | Text, default primary button |
| Muted ink | `#5A5370` | `#B3B0BA` | Secondary text |
| Violet (`--brand`) | `#562FF7` | `#A594FF` | Data, selection, links, switches |
| Record (`--record`) | Pink `#F354D0` | Lime `#C6F24E` | The Record action |
| On air (`--record-live`) | Pink `#F354D0` | Marshmallow `#EFECF0` | A live recording: the stage and the header lamp |
| Stage (`--stage`) | `#562FF7` | `#562FF7` | The Home recording panel |

The on-air colour is the lamp: if it is lit, the action or state is about recording. In light mode both are bubblegum pink. In dark mode, chosen by the owner, the Record action is acid lime and a live recording is Marshmallow White, because pink sat too close to the violet stage on a black ground (2.2:1, against 5.2:1 for lime and 5.7:1 for Marshmallow). Text on them is always dark ink (`#2B0524` on pink, `#172000` on lime, `#1A1240` on Marshmallow), because white fails contrast there. The dock pill is always dark and keeps pink in both themes. Avoid small pink marks on violet, which vibrate; the stage turns fully pink while live instead. Errors use `--destructive`, a true red, so they never read as pink.

Light is the default theme, whatever the desktop prefers; Dark and System are choices in Settings.

## Type

One family, Geist by Vercel (1.900, vendored in `src/assets/fonts/geist`), with Geist Mono for code and triggers. Page title 18px semibold, card titles 14px semibold, body 14px, meta 12–13px, figures 24px semibold with tabular numbers. Sentence case everywhere.

## Shape and depth

Radii: 6px chips, 8px controls, 12px cards, 16px stage; Record is a full pill. Cards use a 1px border and no shadow. Only menus, dialogs and toasts cast a shadow.

## Signature

The on-air stage on Home: a full violet panel that turns bubblegum pink while recording, with the hotkey as keycaps, a rolling voice level, live captions and the Record button. It is the one large colour field in the app.

## Delight

Every dictation lands with a small, certain moment, and nothing else celebrates.

- When a dictation lands, the stage title reads "Inserted · 12 words" with a check, the caption shows what arrived, and the waveform of what was just said settles to rest. After 4.5 seconds it returns to "Ready to dictate". Failures never land.
- Last result says how much faster than typing it was, at the same 40 words a minute History uses, only when that is at least 3 seconds.
- Word milestones (the first dictation, then 1,000 up to 1,000,000 words) appear once on Home, in violet soft, never in the on-air colour. Upgrading users start from their current total without a celebration.
- Empty states show the real hotkey, not "the hotkey".

## Rules

No emoji or text glyphs as icons (lucide outline only). No uppercase tracked labels, gradients, hard offset shadows or icon tiles. Tables use a plain grey header, row rules and right-aligned figures. Motion is feedback only and respects reduced motion. The default window is 800×600, so every screen must work there.
