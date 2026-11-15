# Palette attribution

Chroma's Rust implementation and original Rustwood/Paper presets are MIT licensed.
Third-party theme names identify their upstream palettes; there is no affiliation
or endorsement. Chroma supplies its own mapping to semantic roles.

- **Gruvbox Dark** — morhetz, MIT/X11. Color values from the
  [upstream color table](https://github.com/morhetz/gruvbox-contrib/blob/master/color.table)
  and [theme source](https://github.com/morhetz/gruvbox/blob/master/colors/gruvbox.vim).
  [Upstream license statement](https://github.com/morhetz/gruvbox#license).
- **Nord** — Copyright (c) 2016-present Sven Greb. MIT.
  [Colors and palettes](https://www.nordtheme.com/docs/colors-and-palettes/),
  [license](https://github.com/nordtheme/nord/blob/develop/license).
- **Catppuccin Mocha** — Copyright (c) 2021 Catppuccin. MIT.
  [Palette](https://catppuccin.com/palette/), ANSI values from the
  [Kitty port](https://github.com/catppuccin/kitty/blob/main/themes/mocha.conf),
  [license](https://github.com/catppuccin/kitty/blob/main/LICENSE).
- **Tokyo Night** — Folke Lemaitre. Color values from the
  [Kitty theme export](https://github.com/folke/tokyonight.nvim/blob/main/extras/kitty/tokyonight_night.conf),
  which identifies its theme export license as MIT. The current upstream plugin's
  [repository license](https://github.com/folke/tokyonight.nvim/blob/main/LICENSE)
  is Apache-2.0. Chroma includes color values and original semantic mappings,
  not plugin code. The export's normal and bright ANSI colors are retained.

The permission and warranty text of the MIT license is reproduced in [LICENSE](LICENSE).
These attributions accompany the distributed presets.

The OKLab conversion uses the public-domain matrices published by
[Björn Ottosson](https://bottosson.github.io/posts/oklab/), with an original Rust
implementation and chroma-reduction gamut mapping.

Rust dependencies retain their own licenses; see each locked crate's metadata.
