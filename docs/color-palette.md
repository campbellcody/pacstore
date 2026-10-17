# Blue–cyan color palette

Pacstore uses cool blue surfaces and cyan accents: calm backgrounds, clear text,
and a bright accent for the next useful action. The app applies these tokens in both light and dark modes, following the
system theme by default. The Theme selector offers System, Light, and Dark;
changes apply immediately and persist in preferences.

## Theme tokens

| Token | Dark | Light | Use |
| --- | --- | --- | --- |
| `background` | `#081525` | `#F2F8FC` | Main window background |
| `surface` | `#102338` | `#FFFFFF` | Package cards, navigation, dialogs |
| `surface_hover` | `#18344D` | `#E3F1FA` | Hovered rows and secondary buttons |
| `surface_selected` | `#16445C` | `#D2EDF8` | Active navigation and selected packages |
| `border` | `#31516A` | `#BDD1DF` | Decorative card edges and separators |
| `control_border` | `#698DA5` | `#56758C` | Input outlines and checkbox boundaries |
| `text_primary` | `#E7F3FC` | `#102A43` | Headings, package names, body text |
| `text_secondary` | `#A5BED3` | `#486581` | Summaries, metadata, supporting text |
| `accent` | `#38CFF2` | `#006A91` | Primary buttons, links, progress, focus ring |
| `accent_hover` | `#72E2FA` | `#005777` | Hovered primary buttons and links |
| `accent_pressed` | `#16A8D0` | `#00445E` | Pressed primary buttons |
| `on_accent` | `#062033` | `#FFFFFF` | Text and icons on solid accent buttons |

## Status tokens

| Token | Dark | Light | Use |
| --- | --- | --- | --- |
| `success` | `#6EE7B7` | `#146C43` | Completed transactions, installed badges |
| `warning` | `#FCD34D` | `#8A4B00` | Changes requiring attention, conflict summaries |
| `danger` | `#FDA4AF` | `#B4233B` | Errors, removal labels, destructive-action outlines |
| `info` | `#7DD3FC` | `#075985` | Update availability and informational notices |

Use status colors for text, icons, and outlines on `surface`; do not assume they
work as filled button backgrounds. Keep ordinary install and upgrade actions
cyan. Give removal actions an explicit label and the danger outline.

## Interaction and contrast

- Use `text_primary` and `text_secondary` on neutral surfaces. Reserve
  `on_accent` for solid accent buttons in all three interaction states.
- Mark active navigation with `surface_selected`, primary text, and a cyan edge
  or underline. Preserve a visible focus ring independently of selection.
- Use a 2 px `accent` keyboard focus ring with a surface-colored gap. Use
  `control_border` for meaningful control boundaries; `border` is decorative.
- Keep disabled controls on neutral surfaces, with an explicit disabled state.
  Do not reduce the opacity of ordinary supporting text.
- Pair status color with a label or icon. Progress and errors must remain
  understandable without distinguishing hues.
- Avoid gradients behind text. Keep large areas neutral and use cyan sparingly
  for actions, selection markers, and progress.

Target at least 4.5:1 contrast for normal text and 3:1 for meaningful control
outlines and focus indicators against adjacent colors. Check actual rendered
combinations when implementing the theme, including hover and selected states.

## egui implementation guide

[`src/theme.rs`](../src/theme.rs) defines the tokens and installs both variants
through the egui context. Primary buttons use local widget styles for accent
fills and `on_accent` text across resting, hovered, and pressed states. Disabled
primary buttons retain the neutral widget styling. Removal buttons use danger
text and outlines. Selection underlines and keyboard focus rings stay cyan.

## System fonts

At startup, [`src/fonts.rs`](../src/fonts.rs) resolves Fontconfig's `sans-serif`
and `monospace` families using `fc-match` with an outline-font constraint. Those
fonts become the first choices for proportional and monospace text. Bundled egui
fonts remain available for missing glyphs, and become the defaults when
Fontconfig or a font file is unavailable. Font failures are logged as warnings.

Install `fontconfig` and configure its family aliases to change the system fonts
used by Pacstore. Restart after changing fonts. Font selection follows Fontconfig
aliases; it does not read desktop-specific font or point-size preferences.
