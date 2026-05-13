# 0.5.0 - 2025-02-02

Another breaking release with the highlight this time being the introduction of new subcommands! The existing functionality of calling `inlyne <FILE>` is still preserved as long as it doesn't conflict with an existing subcommand. You can use the new `inlyne view <FILE>` subcommand to **unambiguously** view a file

The first of our new subcommands is `inlyne config open` which makes editing `inlyne`'s config file _much_ simpler for people not accustomed to sniffing out obscure config file locations

## Breaking Changes

- Switch `inlyne`'s CLI with the current functionality under `inlyne view` ([#284])

## Features

- Add history navigation exposed via shortcuts ([#258] [#269])
- Add the new `inlyne config` subcommand for interacting with `inlyne`'s config file ([#285] [#386])
- Modify our custom panic hook to follow a nice workflow for creating GH issues ([#286])
- Allow specifying capital letters in keybinding definition ([#287])
  - In addition to the existing method of `{ key = 'a', mod = ["Shift"] }`
- Allow specifying window position and size through config and CLI ([#290])
- Add a desktop entry file ([#293] [#317])
- Allow double and triple click selection ([#295] [#339])
- Don't show the scrollbar when the content fits on one screen ([#313])
- Add window class properties for wayland ([#343] [#349])

## Fixes

- Update the window title on file navigation ([#274])
- Fix a crash that could happen when clicking some relative file links ([#292])
- Use appropriate relative sizes for headers ([#307])
- Scroll on clicking the scrollbar as well as moving ([#314])
- Top align checkboxes instead of centering with content ([#316])
- Don't panic when the file to view is missing ([#332])

## Documentation

- Track code coverage with codecov.io ([#337] [#341] [#346] [#350] [#378])
- Update some `Cargo.toml` fields ([#369])

## Internal

- Add a dependabot workflow to update CI actions ([#265])
- Dependabot CI action bumps ([#266] [#267] [#340] [#344] [#363] [#374] [#376] [#379] [#384])
- Tweak CI to avoid spawning duplicate jobs for PRs ([#268])
- Ensure that we can always deserialize the default keybindings ([#270])
- Update dependencies ([#272] [#298] [#300] [#301] [#333] [#334] [#336] [#370])
- Improve test coverage ([#273])
- Setup initial metrics infrastructure ([#289])
- Switch our HTTP client from `reqwest` to `ureq` ([#296])
- Placate clippy ([#297] [#326] [#377] [#383])
- Add dev bounding box toggle ([#308] [#312])
- Replace `wiremock` with `tiny_http` for testing ([#320] [#321])
- Update image and remove streamed image decoding ([#325])
- Test our custom user agent ([#328])
- Cleanup test utilities ([#331] [#335])
- Speed up graceful image failure test ([#345])
- Fix test perf regression and new beta toolchain warning ([#348])
- Narrow focus of snapshot tests ([#364])
- Switch from the now deprecated `PanicInfo` to `PanicHookInfo` ([#371])
- Bump CI nightly toolchain version ([#372])

## Contributors

A huge thanks to our contributors

- @CosmicHorrorDev
- @kokoISnoTarget

and everyone else who participates in issues or interacts with the community in other ways :heart:

[#258]: https://github.com/Inlyne-Project/inlyne/pull/258
[#265]: https://github.com/Inlyne-Project/inlyne/pull/265
[#266]: https://github.com/Inlyne-Project/inlyne/pull/266
[#267]: https://github.com/Inlyne-Project/inlyne/pull/267
[#268]: https://github.com/Inlyne-Project/inlyne/pull/268
[#269]: https://github.com/Inlyne-Project/inlyne/pull/269
[#270]: https://github.com/Inlyne-Project/inlyne/pull/270
[#272]: https://github.com/Inlyne-Project/inlyne/pull/272
[#273]: https://github.com/Inlyne-Project/inlyne/pull/273
[#274]: https://github.com/Inlyne-Project/inlyne/pull/274
[#284]: https://github.com/Inlyne-Project/inlyne/pull/284
[#285]: https://github.com/Inlyne-Project/inlyne/pull/285
[#286]: https://github.com/Inlyne-Project/inlyne/pull/286
[#287]: https://github.com/Inlyne-Project/inlyne/pull/287
[#289]: https://github.com/Inlyne-Project/inlyne/pull/289
[#290]: https://github.com/Inlyne-Project/inlyne/pull/290
[#292]: https://github.com/Inlyne-Project/inlyne/pull/292
[#293]: https://github.com/Inlyne-Project/inlyne/pull/293
[#295]: https://github.com/Inlyne-Project/inlyne/pull/295
[#296]: https://github.com/Inlyne-Project/inlyne/pull/296
[#297]: https://github.com/Inlyne-Project/inlyne/pull/297
[#298]: https://github.com/Inlyne-Project/inlyne/pull/298
[#300]: https://github.com/Inlyne-Project/inlyne/pull/300
[#301]: https://github.com/Inlyne-Project/inlyne/pull/301
[#307]: https://github.com/Inlyne-Project/inlyne/pull/307
[#308]: https://github.com/Inlyne-Project/inlyne/pull/308
[#312]: https://github.com/Inlyne-Project/inlyne/pull/312
[#313]: https://github.com/Inlyne-Project/inlyne/pull/313
[#314]: https://github.com/Inlyne-Project/inlyne/pull/314
[#316]: https://github.com/Inlyne-Project/inlyne/pull/316
[#317]: https://github.com/Inlyne-Project/inlyne/pull/317
[#320]: https://github.com/Inlyne-Project/inlyne/pull/320
[#321]: https://github.com/Inlyne-Project/inlyne/pull/321
[#325]: https://github.com/Inlyne-Project/inlyne/pull/325
[#326]: https://github.com/Inlyne-Project/inlyne/pull/326
[#328]: https://github.com/Inlyne-Project/inlyne/pull/328
[#331]: https://github.com/Inlyne-Project/inlyne/pull/331
[#332]: https://github.com/Inlyne-Project/inlyne/pull/332
[#333]: https://github.com/Inlyne-Project/inlyne/pull/333
[#334]: https://github.com/Inlyne-Project/inlyne/pull/334
[#335]: https://github.com/Inlyne-Project/inlyne/pull/335
[#336]: https://github.com/Inlyne-Project/inlyne/pull/336
[#337]: https://github.com/Inlyne-Project/inlyne/pull/337
[#339]: https://github.com/Inlyne-Project/inlyne/pull/339
[#340]: https://github.com/Inlyne-Project/inlyne/pull/340
[#341]: https://github.com/Inlyne-Project/inlyne/pull/341
[#343]: https://github.com/Inlyne-Project/inlyne/pull/343
[#344]: https://github.com/Inlyne-Project/inlyne/pull/344
[#345]: https://github.com/Inlyne-Project/inlyne/pull/345
[#346]: https://github.com/Inlyne-Project/inlyne/pull/346
[#348]: https://github.com/Inlyne-Project/inlyne/pull/348
[#349]: https://github.com/Inlyne-Project/inlyne/pull/349
[#350]: https://github.com/Inlyne-Project/inlyne/pull/350
[#363]: https://github.com/Inlyne-Project/inlyne/pull/363
[#364]: https://github.com/Inlyne-Project/inlyne/pull/364
[#369]: https://github.com/Inlyne-Project/inlyne/pull/369
[#370]: https://github.com/Inlyne-Project/inlyne/pull/370
[#371]: https://github.com/Inlyne-Project/inlyne/pull/371
[#372]: https://github.com/Inlyne-Project/inlyne/pull/372
[#374]: https://github.com/Inlyne-Project/inlyne/pull/374
[#376]: https://github.com/Inlyne-Project/inlyne/pull/376
[#377]: https://github.com/Inlyne-Project/inlyne/pull/377
[#378]: https://github.com/Inlyne-Project/inlyne/pull/378
[#379]: https://github.com/Inlyne-Project/inlyne/pull/379
[#383]: https://github.com/Inlyne-Project/inlyne/pull/383
[#384]: https://github.com/Inlyne-Project/inlyne/pull/384
[#386]: https://github.com/Inlyne-Project/inlyne/pull/386

# 0.4.3 - 2024-08-29

Just a patch release to fix the compilation issue on more recent toolchains with our "older" `time` dependency

## Internal

- Run `$ cargo update -p time`
- Placate clippy
- bump version to v0.4.3
- Update svenstaro/upload-release-action to 2.9.0
  - Whoever at GitHub thought it was a good idea to just _try_ running older actions with a newer incompatible node version with just a warning wasted a good couple of hours of my life :upside_down_face:

# 0.4.2 - 2024-04-06

Just a small bugfix/doc release while new features finish up for the v0.5 series

## Fixes

- Ignore the case when doing header name lookups ([#256])
- Fix a crash when rendering headerless table ([#279])
- Fixes an issue where 1-pixel wide selections would linger ([#288])
- Fixes a crash caused by a mismatch in client/server version support on linux+wayland ([#298])

## Docs

- Exclude outdated repos from the repology badge ([#271])
- Add more instructions for building from source ([#280])

## Internal

<details>
<summary>The usual swarm of non-user-facing changes</summary>

- Fix the exclude for `manual_test_data` for crates.io releases ([#257])
- Don't check for typos on test data

</details>

## Contributors

This release was possible due to the help of the following contributors (in random order) :heart:

- @CosmicHorrorDev
- @0x61nas 
- @kokoISnoTarget

[#256]: https://github.com/Inlyne-Project/inlyne/pull/256
[#279]: https://github.com/Inlyne-Project/inlyne/pull/279
[#288]: https://github.com/Inlyne-Project/inlyne/pull/288
[#298]: https://github.com/Inlyne-Project/inlyne/pull/298
[#271]: https://github.com/Inlyne-Project/inlyne/pull/271
[#280]: https://github.com/Inlyne-Project/inlyne/pull/280
[#257]: https://github.com/Inlyne-Project/inlyne/pull/257

# 0.4.1 - 2024-02-19

## Fixes

- Fix an issue where fonts can fail to be detected on some systems ([#250])

## Docs

- Update the repo link to our newly minted organization ([#251])

[#250]: https://github.com/Inlyne-Project/inlyne/pull/250
[#251]: https://github.com/Inlyne-Project/inlyne/pull/251

# 0.4.0 - 2024-02-17

I'd like to start with a huge thanks to all of our contributors. This release
wouldn't have happened nearly as soon, nor would it have had as many fixes and
features without everyone's help :heart:

## Breaking Changes

- Completions are now generated ahead of time and provided with the release
  assets instead of the old `--gen-completions <SHELL>` flag
- The default light theme `code-highlighter` was changed from the
  `inspired-github` to the new `github` syntax highlighter
- We have a new `wayland` feature that is enabled by default for clipboard
  support. If you don't use wayland and you run into wayland related build
  errors then consider building with the `--no-default-features` with the
  optional `--features x11` if you're using Xorg still
- The default zoom-out keybind is now `<Ctrl+=>` instead of `<Ctrl++>` and
  zoom-reset is now unbound by default instead of `<Ctrl+=>`

## Features

- Font fallback is now supported :tada: (less tofu --> more emojis)
- A **lot** more embedded syntax highlighting themes ([#219])
  - The full list is always in the `inlyne.default.toml` file
- Add clipboard support for wayland ([#243])
- Add support for color-scheme specific `<picture>`s ([#236])
- Underlines are now supported in syntax highlighting ([#221] and [#225])
- `extra` keybindings now override `base` ([#224])
- Use `human-panic` for more user-friendly panic messages ([#172])
- Support table column alignment ([#136])
- Use `taffy` for laying out tables ([#129])

## Fixes

- Inherit alignment for headers ([#241])
- Allow for `px` suffix on pixel length ([#238])
- Mimic GitHub's anchorizer for creating headers' anchor links ([#227])
- Correctly reset table column alignment ([#218])
- Reset scroll on markdown navigation ([#213])
- Debounce file watcher events ([#200])
- More gracefully handle failures in image loading ([#187])
- Switch the TLS library from `openssl` to `rustls` ([#179])
   - Fixes some issues with window's failing some image requests

## Documentation

- Document `fontconfig` dependency ([#220])

## Internal

<details>
<summary>The usual swarm of non-user-facing changes</summary>

- Install `libwayland-dev` and `libxkbcommon-dev`  on ubuntu CI ([#246] and [#247])
- Temporarily disable partial footnotes support ([#244])
- Add tests for more codeblock styles ([#242])
- Set a descriptive user-agent ([#240])
- Reorganize the interpreter's HTML-related code ([#239])
- Make the HTML interpreter more approachable ([#235])
- Make underlines and strikethroughs respect alignment ([#226])
- Reduce the likelihood of a spurious specific windows CI failure ([#222])
- Fix subtract with overflow panic ([#217])
- Refactor CI runs ([#216])
- Fix some typos ([#215])
- Update `glyphon` to v0.3 ([#214])
- Update `taffy` to a non-git version ([#210])
- Migrate from `log` to `tracing` ([#209])
- Keybindings refactor ([#208])
- Refactor watcher changes ([#207])
- Speed up file watcher test happy paths ([#199])
- Late night refactors ([#195])
- Misc cleanup ([#194])
- Pretty up `KeyCombos` representation in user-facing errors ([#193])
- Fix README demo image ([#190] and [#192])
- Switch CI cache to `Swatinem/rust-cache` ([#191])
- Aimless test cleanup ([#189])
- Setup logging for tests ([#183])
- Correctly set the version for windows releases ([#178])
- Add tests for elements nested within a list item ([#176])

</details>

## Contributors

- @AlphaKeks
- @CosmicHorrorDev
- @nicoburns
- @trimental
- @Valentin271

[#129]: https://github.com/Inlyne-Project/inlyne/pull/129
[#136]: https://github.com/Inlyne-Project/inlyne/pull/136
[#172]: https://github.com/Inlyne-Project/inlyne/pull/172
[#176]: https://github.com/Inlyne-Project/inlyne/pull/176
[#178]: https://github.com/Inlyne-Project/inlyne/pull/178
[#179]: https://github.com/Inlyne-Project/inlyne/pull/179
[#183]: https://github.com/Inlyne-Project/inlyne/pull/183
[#187]: https://github.com/Inlyne-Project/inlyne/pull/187
[#189]: https://github.com/Inlyne-Project/inlyne/pull/189
[#190]: https://github.com/Inlyne-Project/inlyne/pull/190
[#191]: https://github.com/Inlyne-Project/inlyne/pull/191
[#192]: https://github.com/Inlyne-Project/inlyne/pull/192
[#193]: https://github.com/Inlyne-Project/inlyne/pull/193
[#194]: https://github.com/Inlyne-Project/inlyne/pull/194
[#195]: https://github.com/Inlyne-Project/inlyne/pull/195
[#199]: https://github.com/Inlyne-Project/inlyne/pull/199
[#200]: https://github.com/Inlyne-Project/inlyne/pull/200
[#207]: https://github.com/Inlyne-Project/inlyne/pull/207
[#208]: https://github.com/Inlyne-Project/inlyne/pull/208
[#209]: https://github.com/Inlyne-Project/inlyne/pull/209
[#210]: https://github.com/Inlyne-Project/inlyne/pull/210
[#213]: https://github.com/Inlyne-Project/inlyne/pull/213
[#214]: https://github.com/Inlyne-Project/inlyne/pull/214
[#215]: https://github.com/Inlyne-Project/inlyne/pull/215
[#216]: https://github.com/Inlyne-Project/inlyne/pull/216
[#217]: https://github.com/Inlyne-Project/inlyne/pull/217
[#218]: https://github.com/Inlyne-Project/inlyne/pull/218
[#219]: https://github.com/Inlyne-Project/inlyne/pull/219
[#220]: https://github.com/Inlyne-Project/inlyne/pull/220
[#221]: https://github.com/Inlyne-Project/inlyne/pull/221
[#222]: https://github.com/Inlyne-Project/inlyne/pull/222
[#224]: https://github.com/Inlyne-Project/inlyne/pull/224
[#225]: https://github.com/Inlyne-Project/inlyne/pull/225
[#226]: https://github.com/Inlyne-Project/inlyne/pull/226
[#227]: https://github.com/Inlyne-Project/inlyne/pull/227
[#235]: https://github.com/Inlyne-Project/inlyne/pull/235
[#236]: https://github.com/Inlyne-Project/inlyne/pull/236
[#238]: https://github.com/Inlyne-Project/inlyne/pull/238
[#239]: https://github.com/Inlyne-Project/inlyne/pull/239
[#240]: https://github.com/Inlyne-Project/inlyne/pull/240
[#241]: https://github.com/Inlyne-Project/inlyne/pull/241
[#242]: https://github.com/Inlyne-Project/inlyne/pull/242
[#243]: https://github.com/Inlyne-Project/inlyne/pull/243
[#244]: https://github.com/Inlyne-Project/inlyne/pull/244
[#246]: https://github.com/Inlyne-Project/inlyne/pull/246
[#247]: https://github.com/Inlyne-Project/inlyne/pull/247

# 0.3.3 - 2023-12-02

Just a small follow-up release to v0.3.2

# Fixed

- Fixes a panic in `wgpu_core` that can occur when resizing windows ([#180])
- Fixes list numbering when an element is nested within a list item ([#181])

# 0.3.2 - 2023-11-23

While waiting for some of the features in the `main` branch to finish baking for
the v0.4 release why not enjoy some bugfixes, doc cleanups, and refactors right
now?

# Fixed

- Fixed a panic that occurred when the viewed file gets removed or renamed ([#145])
- Made live code reloading more robust ([#106] & [#147])
  - Live code reloading should work with more editors (e.g. `neovim`)
  - Live code reloading should more reliable watch the desired file
- Improved syntax highlighting ([#150])
  - We now support highlighting many more formats (e.g. TOML, Dockerfiles, etc.)
  - We now support highlighting code blocks that use a language marker followed
    by a comma like \`\`\`rust,ignore
- Nested numbered lists now display ordering correctly ([#154])
- Fixed a panic that occurred on some specific system configurations ([#169])

# Docs

- Use a repology badge for package manager installation ([#109])
- Make correct location of config file more clear ([#122])

Along with a whole slew of internal refactors and testing improvements

I'd also like to give a big thanks to all of the contributors that helped make
this release possible!

- @AlphaKeks
- @coastalwhite
- @CosmicHorrorDev
- @nicoburns

[#106]: https://github.com/Inlyne-Project/inlyne/pull/106
[#109]: https://github.com/Inlyne-Project/inlyne/pull/109
[#122]: https://github.com/Inlyne-Project/inlyne/pull/122
[#145]: https://github.com/Inlyne-Project/inlyne/pull/145
[#147]: https://github.com/Inlyne-Project/inlyne/pull/147
[#150]: https://github.com/Inlyne-Project/inlyne/pull/150
[#154]: https://github.com/Inlyne-Project/inlyne/pull/154
[#169]: https://github.com/Inlyne-Project/inlyne/pull/169
[#180]: https://github.com/Inlyne-Project/inlyne/pull/180
[#181]: https://github.com/Inlyne-Project/inlyne/pull/181

# 0.3.1 - 2023-05-09

## Deps

- `$ cargo update`

# 0.3.0 - 2023-05-07

Version 0.3 contains many bug fixes and a few improvements such as defaulting to the system color theme, adding a page_width configuration, adding a flag for generating shell completions, and compressing images to save memory.

The list of features and fixes are:
* Fixing the default code color
* Fixing line effects such as underlining
* Defaulting to system theme
* Adding a page_width configuration option
* Adding a flag for generating shell completions
* Storing images in a compressed LZ4 format
* Fixing tables without headers
* More efficient reposition scheduling
* Correct colors on non-srgb textures
* Streaming the decoding of images

# 0.2.1 - 2022-10-21

Version 0.2.1 is a small release with a few important bug fixes such as correct scrollbar positioning, opening of relative markdown files, and watching the open file instead of directory for changes.

A few small added features and changes are:

* Support for details and summary (i.e hidden text)
* Support for horizontal rules (line dividers)
* The preservation of scroll location when resizing
* Opening markdown files in the current window by default, instead of opening a new one

# 0.2.0 - 2022-09-29

Version 0.2.0 comes with **custom keybindings** 🤙 to different actions and control over the scrolling amount per line so you can customise the way that you interact with inlyne for a personalised experience. 

The most exciting feature in my opinion, which has been planned since the start, is **live reloading** 🥳 which monitors the file you're currently looking at for any write changes and automatically refreshes the screen in the background. Hopefully you should find this feature seamless so give it a go!

Support for GitHub style tasklists is now implemented so you can stay on track; along with a few bug fixes.

# 0.1.7 - 2022-08-21

Once again thank you @LovecraftianHorror for another round of great improvements 🎊 

+ **Customise themes with your own colors using the `inlyne.toml` config file**
+ **You can also customise the regular and monospace fonts within `inlyne.toml`** 
+ **Zoom in and out easily with `Ctrl` + `+` and `Ctrl` + `-`  (or `Cmd` + `+` and `Cmd` + `-` on macOS)**
+ Linked markdown files are now opened in another inlyne window when clicked on
+ Clicking on anchor links now sends you to that section
+ Images can now be inlined into rows 
+ The title now shows the path of the opened file, relative to base git or mercurial folders
+ Code block's background color is now determined by the code highlighter
+ A ton of performance optimisations for things such as font loading, image loading, selections and resizing ⚡

# 0.1.6 - 2022-08-15

Thank you @LovecraftianHorror for your contributions to this release. The fixes and features include:

+ **Initial code block syntax highlighting via syntect**
+ **Persistence settings via a configuration file**
+ **Quote block support**
+ Better scrolling on systems and devices that scroll by line height
+ Improved error messages for file paths
+ Lighter dependencies
+ Less blurry image rendering
+ Fixed scrollable area to match rendered scrollbar
+ Fixed code block layout
+ Fixed text line rendering (underline, strikethrough, etc) on small window widths
