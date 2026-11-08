# Сторонние лицензии и зависимости (Third-Party Licenses)

Этот документ содержит перечень сторонних библиотек и компонентов, используемых в проекте **MQ**, а также условия их лицензирования.

## Обзор лицензий зависимостей

| Лицензия | Количество пакетов |
|---|---|
| `MIT OR Apache-2.0` | 267 |
| `MIT` | 126 |
| `Apache-2.0 OR MIT` | 25 |
| `MIT/Apache-2.0` | 22 |
| `Apache-2.0` | 20 |
| `Unicode-3.0` | 18 |
| `Zlib OR Apache-2.0 OR MIT` | 8 |
| `MIT OR Apache-2.0 OR Zlib` | 7 |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | 7 |
| `Unlicense OR MIT` | 6 |
| `MPL-2.0` | 5 |
| `Zlib` | 4 |
| `Apache-2.0/MIT` | 3 |
| `ISC` | 3 |
| `BSD-3-Clause` | 3 |
| `BSL-1.0` | 2 |
| `Apache-2.0 OR ISC OR MIT` | 2 |
| `MIT OR Zlib OR Apache-2.0` | 2 |
| `BSD-3-Clause OR Apache-2.0` | 2 |
| `BSD-3-Clause OR MIT OR Apache-2.0` | 2 |
| `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | 2 |
| `Unlicense/MIT` | 2 |
| `CDLA-Permissive-2.0` | 2 |
| `BSD-2-Clause OR Apache-2.0 OR MIT` | 2 |
| `0BSD OR MIT OR Apache-2.0` | 1 |
| `BSD-2-Clause` | 1 |
| `MIT / Apache-2.0` | 1 |
| `Apache-2.0 AND MIT` | 1 |
| `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0` | 1 |
| `CC0-1.0` | 1 |
| `MIT AND BSD-3-Clause` | 1 |
| `Apache-2.0 AND ISC` | 1 |
| `Apache-2.0 OR BSL-1.0` | 1 |
| `(MIT OR Apache-2.0) AND Unicode-3.0` | 1 |

---

## Лицензии шрифтов (Font Licenses)

### 1. SIL Open Font License 1.1 (OFL-1.1)
Используется для встроенных глифов `egui` / `epaint_default_fonts` (Hack), а также системных шрифтов JetBrains Mono, Inter, Noto Sans, Cantarell, Liberation Sans:

```text
SIL OPEN FONT LICENSE Version 1.1 - 26 February 2007

PREAMBLE
The goals of the Open Font License (OFL) are to stimulate worldwide
development of collaborative font projects, to support the font creation
efforts of academic and linguistic communities, and to provide a free and
open framework in which fonts may be shared and improved in partnership
with others.

The OFL allows the licensed fonts to be used, studied, modified and
redistributed freely as long as they are not sold by themselves. The
fonts, including any derivative works, can be bundled, embedded, 
redistributed and/or sold with any software provided that any reserved
names are not used by derivative works. The fonts and derivatives,
however, cannot be released under any other type of license. The
requirement for fonts to remain under this license does not apply
to any document created using the fonts or their derivatives.
```

### 2. Ubuntu Font License 1.0
Используется в составе глифов `epaint_default_fonts`:

```text
UBUNTU FONT LICENCE Version 1.0
This licence allows the licensed fonts to be used, studied, modified and
redistributed freely. The fonts, including any derivative works, can be
bundled, embedded, and redistributed with any software.
```

---

## Внешние компоненты времени выполнения

- **authlib-injector** (https://github.com/yushijinhun/authlib-injector):
  - Лицензия: GNU LGPL v3.0 / GPL v3.0
  - Использование: загружается динамически в изолированном JVM-агенте (`-javaagent`) исключительно при выборе авторизации через независимое сообщество Ely.by.

---

## Полный реестр Rust зависимостей (crates.io)

Всего прямых и транзитивных зависимостей: **552**.

| Крейт | Версия | Лицензия | Репозиторий |
|---|---|---|---|
| `ab_glyph` | `0.2.32` | `Apache-2.0` | [https://github.com/alexheretic/ab-glyph](https://github.com/alexheretic/ab-glyph) |
| `ab_glyph_rasterizer` | `0.1.10` | `Apache-2.0` | [https://github.com/alexheretic/ab-glyph](https://github.com/alexheretic/ab-glyph) |
| `adler2` | `2.0.1` | `0BSD OR MIT OR Apache-2.0` | [https://github.com/oyvindln/adler2](https://github.com/oyvindln/adler2) |
| `ahash` | `0.8.12` | `MIT OR Apache-2.0` | [https://github.com/tkaitchuck/ahash](https://github.com/tkaitchuck/ahash) |
| `aho-corasick` | `1.1.5` | `Unlicense OR MIT` | [https://github.com/BurntSushi/aho-corasick](https://github.com/BurntSushi/aho-corasick) |
| `android-activity` | `0.6.1` | `MIT OR Apache-2.0` | [https://github.com/rust-mobile/android-activity](https://github.com/rust-mobile/android-activity) |
| `android-properties` | `0.2.2` | `MIT` | [https://github.com/miklelappo/android-properties](https://github.com/miklelappo/android-properties) |
| `android_system_properties` | `0.1.6` | `MIT OR Apache-2.0` | [https://github.com/nical/android_system_properties](https://github.com/nical/android_system_properties) |
| `anes` | `0.1.6` | `MIT OR Apache-2.0` | [https://github.com/zrzka/anes-rs](https://github.com/zrzka/anes-rs) |
| `anstream` | `1.0.0` | `MIT OR Apache-2.0` | [https://github.com/rust-cli/anstyle.git](https://github.com/rust-cli/anstyle.git) |
| `anstyle` | `1.0.14` | `MIT OR Apache-2.0` | [https://github.com/rust-cli/anstyle.git](https://github.com/rust-cli/anstyle.git) |
| `anstyle-parse` | `1.0.0` | `MIT OR Apache-2.0` | [https://github.com/rust-cli/anstyle.git](https://github.com/rust-cli/anstyle.git) |
| `anstyle-query` | `1.1.5` | `MIT OR Apache-2.0` | [https://github.com/rust-cli/anstyle.git](https://github.com/rust-cli/anstyle.git) |
| `anstyle-wincon` | `3.0.11` | `MIT OR Apache-2.0` | [https://github.com/rust-cli/anstyle.git](https://github.com/rust-cli/anstyle.git) |
| `anyhow` | `1.0.104` | `MIT OR Apache-2.0` | [https://github.com/dtolnay/anyhow](https://github.com/dtolnay/anyhow) |
| `arbitrary` | `1.5.0` | `MIT OR Apache-2.0` | [https://github.com/rust-fuzz/arbitrary/](https://github.com/rust-fuzz/arbitrary/) |
| `arboard` | `3.7.0` | `MIT OR Apache-2.0` | [https://github.com/1Password/arboard](https://github.com/1Password/arboard) |
| `arrayref` | `0.3.9` | `BSD-2-Clause` | [https://github.com/droundy/arrayref](https://github.com/droundy/arrayref) |
| `arrayvec` | `0.7.8` | `MIT OR Apache-2.0` | [https://github.com/bluss/arrayvec](https://github.com/bluss/arrayvec) |
| `as-raw-xcb-connection` | `1.0.1` | `MIT OR Apache-2.0` | [https://github.com/psychon/as-raw-xcb-connection](https://github.com/psychon/as-raw-xcb-connection) |
| `ash` | `0.38.0+1.3.281` | `MIT OR Apache-2.0` | [https://github.com/ash-rs/ash](https://github.com/ash-rs/ash) |
| `async-compression` | `0.4.52` | `MIT OR Apache-2.0` | [https://github.com/Nullus157/async-compression](https://github.com/Nullus157/async-compression) |
| `atomic-waker` | `1.1.2` | `Apache-2.0 OR MIT` | [https://github.com/smol-rs/atomic-waker](https://github.com/smol-rs/atomic-waker) |
| `autocfg` | `1.5.1` | `Apache-2.0 OR MIT` | [https://github.com/cuviper/autocfg](https://github.com/cuviper/autocfg) |
| `axum` | `0.8.9` | `MIT` | [https://github.com/tokio-rs/axum](https://github.com/tokio-rs/axum) |
| `axum-core` | `0.5.6` | `MIT` | [https://github.com/tokio-rs/axum](https://github.com/tokio-rs/axum) |
| `base64` | `0.21.7` | `MIT OR Apache-2.0` | [https://github.com/marshallpierce/rust-base64](https://github.com/marshallpierce/rust-base64) |
| `base64` | `0.22.1` | `MIT OR Apache-2.0` | [https://github.com/marshallpierce/rust-base64](https://github.com/marshallpierce/rust-base64) |
| `base64` | `0.23.1` | `MIT OR Apache-2.0` | [https://github.com/marshallpierce/rust-base64](https://github.com/marshallpierce/rust-base64) |
| `bit-set` | `0.8.0` | `Apache-2.0 OR MIT` | [https://github.com/contain-rs/bit-set](https://github.com/contain-rs/bit-set) |
| `bit-vec` | `0.8.0` | `Apache-2.0 OR MIT` | [https://github.com/contain-rs/bit-vec](https://github.com/contain-rs/bit-vec) |
| `bitflags` | `1.3.2` | `MIT/Apache-2.0` | [https://github.com/bitflags/bitflags](https://github.com/bitflags/bitflags) |
| `bitflags` | `2.13.2` | `MIT OR Apache-2.0` | [https://github.com/bitflags/bitflags](https://github.com/bitflags/bitflags) |
| `block` | `0.1.6` | `MIT` | [http://github.com/SSheldon/rust-block](http://github.com/SSheldon/rust-block) |
| `block-buffer` | `0.10.4` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/utils](https://github.com/RustCrypto/utils) |
| `block2` | `0.5.1` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `bumpalo` | `3.20.3` | `MIT OR Apache-2.0` | [https://github.com/fitzgen/bumpalo](https://github.com/fitzgen/bumpalo) |
| `bytemuck` | `1.25.2` | `Zlib OR Apache-2.0 OR MIT` | [https://github.com/Lokathor/bytemuck](https://github.com/Lokathor/bytemuck) |
| `bytemuck_derive` | `1.12.1` | `Zlib OR Apache-2.0 OR MIT` | [https://github.com/Lokathor/bytemuck](https://github.com/Lokathor/bytemuck) |
| `byteorder` | `1.5.0` | `Unlicense OR MIT` | [https://github.com/BurntSushi/byteorder](https://github.com/BurntSushi/byteorder) |
| `byteorder-lite` | `0.1.0` | `Unlicense OR MIT` | [https://github.com/image-rs/byteorder-lite](https://github.com/image-rs/byteorder-lite) |
| `bytes` | `1.12.1` | `MIT` | [https://github.com/tokio-rs/bytes](https://github.com/tokio-rs/bytes) |
| `calloop` | `0.13.0` | `MIT` | [https://github.com/Smithay/calloop](https://github.com/Smithay/calloop) |
| `calloop` | `0.14.5` | `MIT` | [https://github.com/Smithay/calloop](https://github.com/Smithay/calloop) |
| `calloop-wayland-source` | `0.3.0` | `MIT` | [https://github.com/smithay/calloop-wayland-source](https://github.com/smithay/calloop-wayland-source) |
| `calloop-wayland-source` | `0.4.1` | `MIT` | [https://github.com/smithay/calloop-wayland-source](https://github.com/smithay/calloop-wayland-source) |
| `cast` | `0.3.0` | `MIT OR Apache-2.0` | [https://github.com/japaric/cast.rs](https://github.com/japaric/cast.rs) |
| `cc` | `1.11.1` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/cc-rs](https://github.com/rust-lang/cc-rs) |
| `cfg-if` | `1.0.5` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/cfg-if](https://github.com/rust-lang/cfg-if) |
| `cfg_aliases` | `0.2.2` | `MIT` | [https://github.com/katharostech/cfg_aliases](https://github.com/katharostech/cfg_aliases) |
| `cgl` | `0.3.2` | `MIT / Apache-2.0` | [https://github.com/servo/cgl-rs](https://github.com/servo/cgl-rs) |
| `chacha20` | `0.10.2` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/stream-ciphers](https://github.com/RustCrypto/stream-ciphers) |
| `chrono` | `0.4.45` | `MIT OR Apache-2.0` | [https://github.com/chronotope/chrono](https://github.com/chronotope/chrono) |
| `ciborium` | `0.2.2` | `Apache-2.0` | [https://github.com/enarx/ciborium](https://github.com/enarx/ciborium) |
| `ciborium-io` | `0.2.2` | `Apache-2.0` | [https://github.com/enarx/ciborium](https://github.com/enarx/ciborium) |
| `ciborium-ll` | `0.2.2` | `Apache-2.0` | [https://github.com/enarx/ciborium](https://github.com/enarx/ciborium) |
| `cipher` | `0.4.4` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/traits](https://github.com/RustCrypto/traits) |
| `clap` | `4.6.7` | `MIT OR Apache-2.0` | [https://github.com/clap-rs/clap](https://github.com/clap-rs/clap) |
| `clap_builder` | `4.6.7` | `MIT OR Apache-2.0` | [https://github.com/clap-rs/clap](https://github.com/clap-rs/clap) |
| `clap_derive` | `4.6.7` | `MIT OR Apache-2.0` | [https://github.com/clap-rs/clap](https://github.com/clap-rs/clap) |
| `clap_lex` | `1.1.1` | `MIT OR Apache-2.0` | [https://github.com/clap-rs/clap](https://github.com/clap-rs/clap) |
| `clipboard-win` | `5.4.3` | `BSL-1.0` | [https://github.com/DoumanAsh/clipboard-win](https://github.com/DoumanAsh/clipboard-win) |
| `codespan-reporting` | `0.11.1` | `Apache-2.0` | [https://github.com/brendanzab/codespan](https://github.com/brendanzab/codespan) |
| `color_quant` | `1.1.0` | `MIT` | [https://github.com/image-rs/color_quant.git](https://github.com/image-rs/color_quant.git) |
| `colorchoice` | `1.0.5` | `MIT OR Apache-2.0` | [https://github.com/rust-cli/anstyle.git](https://github.com/rust-cli/anstyle.git) |
| `combine` | `4.6.9` | `MIT` | [https://github.com/Marwes/combine](https://github.com/Marwes/combine) |
| `compression-codecs` | `0.4.47` | `MIT OR Apache-2.0` | [https://github.com/Nullus157/async-compression](https://github.com/Nullus157/async-compression) |
| `compression-core` | `0.4.34` | `MIT OR Apache-2.0` | [https://github.com/Nullus157/async-compression](https://github.com/Nullus157/async-compression) |
| `concurrent-queue` | `2.5.0` | `Apache-2.0 OR MIT` | [https://github.com/smol-rs/concurrent-queue](https://github.com/smol-rs/concurrent-queue) |
| `core-foundation` | `0.9.4` | `MIT OR Apache-2.0` | [https://github.com/servo/core-foundation-rs](https://github.com/servo/core-foundation-rs) |
| `core-foundation-sys` | `0.8.7` | `MIT OR Apache-2.0` | [https://github.com/servo/core-foundation-rs](https://github.com/servo/core-foundation-rs) |
| `core-graphics` | `0.23.2` | `MIT OR Apache-2.0` | [https://github.com/servo/core-foundation-rs](https://github.com/servo/core-foundation-rs) |
| `core-graphics-types` | `0.1.3` | `MIT OR Apache-2.0` | [https://github.com/servo/core-foundation-rs](https://github.com/servo/core-foundation-rs) |
| `cpufeatures` | `0.2.17` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/utils](https://github.com/RustCrypto/utils) |
| `cpufeatures` | `0.3.1` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/utils](https://github.com/RustCrypto/utils) |
| `crc` | `3.4.0` | `MIT OR Apache-2.0` | [https://github.com/mrhooray/crc-rs.git](https://github.com/mrhooray/crc-rs.git) |
| `crc-catalog` | `2.5.0` | `MIT OR Apache-2.0` | [https://github.com/akhilles/crc-catalog.git](https://github.com/akhilles/crc-catalog.git) |
| `crc32fast` | `1.5.4` | `MIT OR Apache-2.0` | [https://github.com/srijs/rust-crc32fast](https://github.com/srijs/rust-crc32fast) |
| `criterion` | `0.5.1` | `Apache-2.0 OR MIT` | [https://github.com/bheisler/criterion.rs](https://github.com/bheisler/criterion.rs) |
| `criterion-plot` | `0.5.0` | `MIT/Apache-2.0` | [https://github.com/bheisler/criterion.rs](https://github.com/bheisler/criterion.rs) |
| `crossbeam-channel` | `0.5.17` | `MIT OR Apache-2.0` | [https://github.com/crossbeam-rs/crossbeam](https://github.com/crossbeam-rs/crossbeam) |
| `crossbeam-deque` | `0.8.8` | `MIT OR Apache-2.0` | [https://github.com/crossbeam-rs/crossbeam](https://github.com/crossbeam-rs/crossbeam) |
| `crossbeam-epoch` | `0.9.21` | `MIT OR Apache-2.0` | [https://github.com/crossbeam-rs/crossbeam](https://github.com/crossbeam-rs/crossbeam) |
| `crossbeam-utils` | `0.8.23` | `MIT OR Apache-2.0` | [https://github.com/crossbeam-rs/crossbeam](https://github.com/crossbeam-rs/crossbeam) |
| `crunchy` | `0.2.4` | `MIT` | [https://github.com/eira-fransham/crunchy](https://github.com/eira-fransham/crunchy) |
| `crypto-common` | `0.1.7` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/traits](https://github.com/RustCrypto/traits) |
| `ct-codecs` | `1.1.8` | `MIT` | [https://github.com/jedisct1/rust-ct-codecs](https://github.com/jedisct1/rust-ct-codecs) |
| `cursor-icon` | `1.2.0` | `MIT OR Apache-2.0 OR Zlib` | [https://github.com/rust-windowing/cursor-icon](https://github.com/rust-windowing/cursor-icon) |
| `dashmap` | `6.2.1` | `MIT` | [https://github.com/xacrimon/dashmap](https://github.com/xacrimon/dashmap) |
| `data-url` | `0.3.2` | `MIT OR Apache-2.0` | [https://github.com/servo/rust-url](https://github.com/servo/rust-url) |
| `derive_arbitrary` | `1.5.0` | `MIT OR Apache-2.0` | [https://github.com/rust-fuzz/arbitrary](https://github.com/rust-fuzz/arbitrary) |
| `digest` | `0.10.7` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/traits](https://github.com/RustCrypto/traits) |
| `directories` | `6.0.0` | `MIT OR Apache-2.0` | [https://github.com/soc/directories-rs](https://github.com/soc/directories-rs) |
| `dirs-sys` | `0.5.0` | `MIT OR Apache-2.0` | [https://github.com/dirs-dev/dirs-sys-rs](https://github.com/dirs-dev/dirs-sys-rs) |
| `dispatch` | `0.2.0` | `MIT` | [http://github.com/SSheldon/rust-dispatch](http://github.com/SSheldon/rust-dispatch) |
| `dispatch2` | `0.3.1` | `Zlib OR Apache-2.0 OR MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `displaydoc` | `0.2.7` | `MIT OR Apache-2.0` | [https://github.com/yaahc/displaydoc](https://github.com/yaahc/displaydoc) |
| `dlib` | `0.5.3` | `MIT` | [https://github.com/elinorbgr/dlib](https://github.com/elinorbgr/dlib) |
| `document-features` | `0.2.12` | `MIT OR Apache-2.0` | [https://github.com/slint-ui/document-features](https://github.com/slint-ui/document-features) |
| `downcast-rs` | `1.2.1` | `MIT/Apache-2.0` | [https://github.com/marcianx/downcast-rs](https://github.com/marcianx/downcast-rs) |
| `dpi` | `0.1.2` | `Apache-2.0 AND MIT` | [https://github.com/rust-windowing/winit](https://github.com/rust-windowing/winit) |
| `ecolor` | `0.31.1` | `MIT OR Apache-2.0` | [https://github.com/emilk/egui](https://github.com/emilk/egui) |
| `eframe` | `0.31.1` | `MIT OR Apache-2.0` | [https://github.com/emilk/egui/tree/master/crates/eframe](https://github.com/emilk/egui/tree/master/crates/eframe) |
| `egui` | `0.31.1` | `MIT OR Apache-2.0` | [https://github.com/emilk/egui](https://github.com/emilk/egui) |
| `egui-wgpu` | `0.31.1` | `MIT OR Apache-2.0` | [https://github.com/emilk/egui/tree/master/crates/egui-wgpu](https://github.com/emilk/egui/tree/master/crates/egui-wgpu) |
| `egui-winit` | `0.31.1` | `MIT OR Apache-2.0` | [https://github.com/emilk/egui/tree/master/crates/egui-winit](https://github.com/emilk/egui/tree/master/crates/egui-winit) |
| `egui_extras` | `0.31.1` | `MIT OR Apache-2.0` | [https://github.com/emilk/egui](https://github.com/emilk/egui) |
| `egui_glow` | `0.31.1` | `MIT OR Apache-2.0` | [https://github.com/emilk/egui/tree/master/crates/egui_glow](https://github.com/emilk/egui/tree/master/crates/egui_glow) |
| `ehttp` | `0.5.0` | `MIT OR Apache-2.0` | [https://github.com/emilk/ehttp](https://github.com/emilk/ehttp) |
| `either` | `1.19.0` | `MIT OR Apache-2.0` | [https://github.com/rayon-rs/either](https://github.com/rayon-rs/either) |
| `emath` | `0.31.1` | `MIT OR Apache-2.0` | [https://github.com/emilk/egui/tree/master/crates/emath](https://github.com/emilk/egui/tree/master/crates/emath) |
| `enum-map` | `2.7.3` | `MIT OR Apache-2.0` | [https://codeberg.org/xfix/enum-map](https://codeberg.org/xfix/enum-map) |
| `enum-map-derive` | `0.17.0` | `MIT OR Apache-2.0` | [https://codeberg.org/xfix/enum-map](https://codeberg.org/xfix/enum-map) |
| `epaint` | `0.31.1` | `MIT OR Apache-2.0` | [https://github.com/emilk/egui/tree/master/crates/epaint](https://github.com/emilk/egui/tree/master/crates/epaint) |
| `epaint_default_fonts` | `0.31.1` | `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0` | [https://github.com/emilk/egui/tree/master/crates/epaint_default_fonts](https://github.com/emilk/egui/tree/master/crates/epaint_default_fonts) |
| `equivalent` | `1.0.3` | `Apache-2.0 OR MIT` | [https://github.com/indexmap-rs/equivalent](https://github.com/indexmap-rs/equivalent) |
| `errno` | `0.3.14` | `MIT OR Apache-2.0` | [https://github.com/lambda-fairy/rust-errno](https://github.com/lambda-fairy/rust-errno) |
| `error-code` | `3.4.0` | `BSL-1.0` | [https://github.com/DoumanAsh/error-code](https://github.com/DoumanAsh/error-code) |
| `fastrand` | `2.5.0` | `Apache-2.0 OR MIT` | [https://github.com/smol-rs/fastrand](https://github.com/smol-rs/fastrand) |
| `fax` | `0.2.7` | `MIT` | [https://github.com/pdf-rs/fax](https://github.com/pdf-rs/fax) |
| `fdeflate` | `0.3.7` | `MIT OR Apache-2.0` | [https://github.com/image-rs/fdeflate](https://github.com/image-rs/fdeflate) |
| `filetime` | `0.2.29` | `MIT/Apache-2.0` | [https://github.com/alexcrichton/filetime](https://github.com/alexcrichton/filetime) |
| `find-msvc-tools` | `0.1.15` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/cc-rs](https://github.com/rust-lang/cc-rs) |
| `flate2` | `1.1.11` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/flate2-rs](https://github.com/rust-lang/flate2-rs) |
| `float-cmp` | `0.9.0` | `MIT` | [https://github.com/mikedilger/float-cmp](https://github.com/mikedilger/float-cmp) |
| `flume` | `0.11.1` | `Apache-2.0/MIT` | [https://github.com/zesterer/flume](https://github.com/zesterer/flume) |
| `foldhash` | `0.1.5` | `Zlib` | [https://github.com/orlp/foldhash](https://github.com/orlp/foldhash) |
| `foreign-types` | `0.5.0` | `MIT/Apache-2.0` | [https://github.com/sfackler/foreign-types](https://github.com/sfackler/foreign-types) |
| `foreign-types-macros` | `0.2.4` | `MIT/Apache-2.0` | [https://github.com/sfackler/foreign-types](https://github.com/sfackler/foreign-types) |
| `foreign-types-shared` | `0.3.1` | `MIT/Apache-2.0` | [https://github.com/sfackler/foreign-types](https://github.com/sfackler/foreign-types) |
| `form_urlencoded` | `1.2.2` | `MIT OR Apache-2.0` | [https://github.com/servo/rust-url](https://github.com/servo/rust-url) |
| `futures-channel` | `0.3.34` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/futures-rs](https://github.com/rust-lang/futures-rs) |
| `futures-core` | `0.3.34` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/futures-rs](https://github.com/rust-lang/futures-rs) |
| `futures-io` | `0.3.34` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/futures-rs](https://github.com/rust-lang/futures-rs) |
| `futures-macro` | `0.3.34` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/futures-rs](https://github.com/rust-lang/futures-rs) |
| `futures-sink` | `0.3.34` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/futures-rs](https://github.com/rust-lang/futures-rs) |
| `futures-task` | `0.3.34` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/futures-rs](https://github.com/rust-lang/futures-rs) |
| `futures-util` | `0.3.34` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/futures-rs](https://github.com/rust-lang/futures-rs) |
| `generator` | `0.8.12` | `MIT/Apache-2.0` | [https://github.com/Xudong-Huang/generator-rs.git](https://github.com/Xudong-Huang/generator-rs.git) |
| `generic-array` | `0.14.7` | `MIT` | [https://github.com/fizyk20/generic-array.git](https://github.com/fizyk20/generic-array.git) |
| `gethostname` | `1.1.0` | `Apache-2.0` | [https://codeberg.org/swsnr/gethostname.rs.git](https://codeberg.org/swsnr/gethostname.rs.git) |
| `getrandom` | `0.2.17` | `MIT OR Apache-2.0` | [https://github.com/rust-random/getrandom](https://github.com/rust-random/getrandom) |
| `getrandom` | `0.3.4` | `MIT OR Apache-2.0` | [https://github.com/rust-random/getrandom](https://github.com/rust-random/getrandom) |
| `getrandom` | `0.4.3` | `MIT OR Apache-2.0` | [https://github.com/rust-random/getrandom](https://github.com/rust-random/getrandom) |
| `gif` | `0.14.2` | `MIT OR Apache-2.0` | [https://github.com/image-rs/image-gif](https://github.com/image-rs/image-gif) |
| `gl_generator` | `0.14.0` | `Apache-2.0` | [https://github.com/brendanzab/gl-rs/](https://github.com/brendanzab/gl-rs/) |
| `glow` | `0.16.0` | `MIT OR Apache-2.0 OR Zlib` | [https://github.com/grovesNL/glow](https://github.com/grovesNL/glow) |
| `glutin` | `0.32.3` | `Apache-2.0` | [https://github.com/rust-windowing/glutin](https://github.com/rust-windowing/glutin) |
| `glutin-winit` | `0.5.0` | `MIT` | [https://github.com/rust-windowing/glutin](https://github.com/rust-windowing/glutin) |
| `glutin_egl_sys` | `0.7.1` | `Apache-2.0` | [https://github.com/rust-windowing/glutin](https://github.com/rust-windowing/glutin) |
| `glutin_glx_sys` | `0.6.1` | `Apache-2.0` | [https://github.com/rust-windowing/glutin](https://github.com/rust-windowing/glutin) |
| `glutin_wgl_sys` | `0.6.1` | `Apache-2.0` | [https://github.com/rust-windowing/glutin](https://github.com/rust-windowing/glutin) |
| `gpu-alloc` | `0.6.2` | `MIT OR Apache-2.0` | [https://github.com/zakarumych/gpu-alloc](https://github.com/zakarumych/gpu-alloc) |
| `gpu-alloc-types` | `0.3.1` | `MIT OR Apache-2.0` | [https://github.com/zakarumych/gpu-alloc](https://github.com/zakarumych/gpu-alloc) |
| `gpu-allocator` | `0.27.0` | `MIT OR Apache-2.0` | [https://github.com/Traverse-Research/gpu-allocator](https://github.com/Traverse-Research/gpu-allocator) |
| `gpu-descriptor` | `0.3.2` | `MIT OR Apache-2.0` | [https://github.com/zakarumych/gpu-descriptor](https://github.com/zakarumych/gpu-descriptor) |
| `gpu-descriptor-types` | `0.2.0` | `MIT OR Apache-2.0` | [https://github.com/zakarumych/gpu-descriptor](https://github.com/zakarumych/gpu-descriptor) |
| `half` | `2.7.1` | `MIT OR Apache-2.0` | [https://github.com/VoidStarKat/half-rs](https://github.com/VoidStarKat/half-rs) |
| `hashbrown` | `0.14.5` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/hashbrown](https://github.com/rust-lang/hashbrown) |
| `hashbrown` | `0.15.5` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/hashbrown](https://github.com/rust-lang/hashbrown) |
| `hashbrown` | `0.17.1` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/hashbrown](https://github.com/rust-lang/hashbrown) |
| `heck` | `0.5.0` | `MIT OR Apache-2.0` | [https://github.com/withoutboats/heck](https://github.com/withoutboats/heck) |
| `hermit-abi` | `0.5.3` | `MIT OR Apache-2.0` | [https://github.com/hermit-os/hermit-rs](https://github.com/hermit-os/hermit-rs) |
| `hex` | `0.4.3` | `MIT OR Apache-2.0` | [https://github.com/KokaKiwi/rust-hex](https://github.com/KokaKiwi/rust-hex) |
| `hexf-parse` | `0.2.1` | `CC0-1.0` | [https://github.com/lifthrasiir/hexf](https://github.com/lifthrasiir/hexf) |
| `hmac` | `0.12.1` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/MACs](https://github.com/RustCrypto/MACs) |
| `http` | `1.5.0` | `MIT OR Apache-2.0` | [https://github.com/hyperium/http](https://github.com/hyperium/http) |
| `http-body` | `1.1.0` | `MIT` | [https://github.com/hyperium/http-body](https://github.com/hyperium/http-body) |
| `http-body-util` | `0.1.5` | `MIT` | [https://github.com/hyperium/http-body](https://github.com/hyperium/http-body) |
| `httparse` | `1.10.1` | `MIT OR Apache-2.0` | [https://github.com/seanmonstar/httparse](https://github.com/seanmonstar/httparse) |
| `httpdate` | `1.0.3` | `MIT OR Apache-2.0` | [https://github.com/pyfisch/httpdate](https://github.com/pyfisch/httpdate) |
| `hyper` | `1.12.0` | `MIT` | [https://github.com/hyperium/hyper](https://github.com/hyperium/hyper) |
| `hyper-rustls` | `0.27.10` | `Apache-2.0 OR ISC OR MIT` | [https://github.com/rustls/hyper-rustls](https://github.com/rustls/hyper-rustls) |
| `hyper-util` | `0.1.21` | `MIT` | [https://github.com/hyperium/hyper-util](https://github.com/hyperium/hyper-util) |
| `iana-time-zone` | `0.1.65` | `MIT OR Apache-2.0` | [https://github.com/strawlab/iana-time-zone](https://github.com/strawlab/iana-time-zone) |
| `iana-time-zone-haiku` | `0.1.2` | `MIT OR Apache-2.0` | [https://github.com/strawlab/iana-time-zone](https://github.com/strawlab/iana-time-zone) |
| `icu_collections` | `2.4.0` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `icu_locale_core` | `2.4.0` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `icu_normalizer` | `2.4.0` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `icu_normalizer_data` | `2.4.0` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `icu_properties` | `2.4.0` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `icu_properties_data` | `2.4.0` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `icu_provider` | `2.4.0` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `idna` | `1.1.0` | `MIT OR Apache-2.0` | [https://github.com/servo/rust-url/](https://github.com/servo/rust-url/) |
| `idna_adapter` | `1.2.2` | `Apache-2.0 OR MIT` | [https://github.com/hsivonen/idna_adapter](https://github.com/hsivonen/idna_adapter) |
| `image` | `0.25.10` | `MIT OR Apache-2.0` | [https://github.com/image-rs/image](https://github.com/image-rs/image) |
| `image-webp` | `0.2.4` | `MIT OR Apache-2.0` | [https://github.com/image-rs/image-webp](https://github.com/image-rs/image-webp) |
| `imagesize` | `0.12.0` | `MIT` | [https://github.com/Roughsketch/imagesize](https://github.com/Roughsketch/imagesize) |
| `indexmap` | `2.14.2` | `Apache-2.0 OR MIT` | [https://github.com/indexmap-rs/indexmap](https://github.com/indexmap-rs/indexmap) |
| `inout` | `0.1.4` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/utils](https://github.com/RustCrypto/utils) |
| `ipnet` | `2.12.2` | `MIT OR Apache-2.0` | [https://github.com/krisprice/ipnet](https://github.com/krisprice/ipnet) |
| `is-terminal` | `0.4.17` | `MIT` | [https://github.com/sunfishcode/is-terminal](https://github.com/sunfishcode/is-terminal) |
| `is_terminal_polyfill` | `1.70.2` | `MIT OR Apache-2.0` | [https://github.com/polyfill-rs/is_terminal_polyfill](https://github.com/polyfill-rs/is_terminal_polyfill) |
| `itertools` | `0.10.5` | `MIT/Apache-2.0` | [https://github.com/rust-itertools/itertools](https://github.com/rust-itertools/itertools) |
| `itoa` | `1.0.18` | `MIT OR Apache-2.0` | [https://github.com/dtolnay/itoa](https://github.com/dtolnay/itoa) |
| `jni` | `0.22.4` | `MIT OR Apache-2.0` | [https://github.com/jni-rs/jni-rs](https://github.com/jni-rs/jni-rs) |
| `jni-macros` | `0.22.4` | `MIT OR Apache-2.0` | [https://github.com/jni-rs/jni-rs](https://github.com/jni-rs/jni-rs) |
| `jni-sys` | `0.3.1` | `MIT OR Apache-2.0` | [https://github.com/jni-rs/jni-sys](https://github.com/jni-rs/jni-sys) |
| `jni-sys` | `0.4.1` | `MIT OR Apache-2.0` | [https://github.com/jni-rs/jni-sys](https://github.com/jni-rs/jni-sys) |
| `jni-sys-macros` | `0.4.1` | `MIT OR Apache-2.0` | [https://github.com/jni-rs/jni-sys](https://github.com/jni-rs/jni-sys) |
| `jobserver` | `0.1.35` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/jobserver-rs](https://github.com/rust-lang/jobserver-rs) |
| `js-sys` | `0.3.107` | `MIT OR Apache-2.0` | [https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys](https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys) |
| `khronos-egl` | `6.0.0` | `MIT/Apache-2.0` | [https://github.com/timothee-haudebourg/khronos-egl](https://github.com/timothee-haudebourg/khronos-egl) |
| `khronos_api` | `3.1.0` | `Apache-2.0` | [https://github.com/brendanzab/gl-rs/](https://github.com/brendanzab/gl-rs/) |
| `kurbo` | `0.9.5` | `MIT/Apache-2.0` | [https://github.com/linebender/kurbo](https://github.com/linebender/kurbo) |
| `lazy_static` | `1.5.1` | `MIT OR Apache-2.0` | [https://github.com/rust-lang-nursery/lazy-static.rs](https://github.com/rust-lang-nursery/lazy-static.rs) |
| `libc` | `0.2.191` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/libc](https://github.com/rust-lang/libc) |
| `libloading` | `0.8.9` | `ISC` | [https://github.com/nagisa/rust_libloading/](https://github.com/nagisa/rust_libloading/) |
| `libredox` | `0.1.25` | `MIT` | [https://gitlab.redox-os.org/redox-os/libredox.git](https://gitlab.redox-os.org/redox-os/libredox.git) |
| `linux-raw-sys` | `0.4.15` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | [https://github.com/sunfishcode/linux-raw-sys](https://github.com/sunfishcode/linux-raw-sys) |
| `linux-raw-sys` | `0.12.1` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | [https://github.com/sunfishcode/linux-raw-sys](https://github.com/sunfishcode/linux-raw-sys) |
| `litemap` | `0.8.3` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `litrs` | `1.0.1` | `MIT OR Apache-2.0` | [https://github.com/LukasKalbertodt/litrs](https://github.com/LukasKalbertodt/litrs) |
| `lock_api` | `0.4.14` | `MIT OR Apache-2.0` | [https://github.com/Amanieu/parking_lot](https://github.com/Amanieu/parking_lot) |
| `log` | `0.4.35` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/log](https://github.com/rust-lang/log) |
| `loom` | `0.7.2` | `MIT` | [https://github.com/tokio-rs/loom](https://github.com/tokio-rs/loom) |
| `lru-slab` | `0.1.3` | `MIT OR Apache-2.0 OR Zlib` | [https://github.com/Ralith/lru-slab](https://github.com/Ralith/lru-slab) |
| `lzma-rs` | `0.3.0` | `MIT` | [https://github.com/gendx/lzma-rs](https://github.com/gendx/lzma-rs) |
| `malloc_buf` | `0.0.6` | `MIT` | [https://github.com/SSheldon/malloc_buf](https://github.com/SSheldon/malloc_buf) |
| `matchers` | `0.2.0` | `MIT` | [https://github.com/hawkw/matchers](https://github.com/hawkw/matchers) |
| `matchit` | `0.8.4` | `MIT AND BSD-3-Clause` | [https://github.com/ibraheemdev/matchit](https://github.com/ibraheemdev/matchit) |
| `md-5` | `0.10.6` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/hashes](https://github.com/RustCrypto/hashes) |
| `memchr` | `2.8.3` | `Unlicense OR MIT` | [https://github.com/BurntSushi/memchr](https://github.com/BurntSushi/memchr) |
| `memmap2` | `0.9.12` | `MIT OR Apache-2.0` | [https://github.com/RazrFalcon/memmap2-rs](https://github.com/RazrFalcon/memmap2-rs) |
| `memoffset` | `0.9.1` | `MIT` | [https://github.com/Gilnaa/memoffset](https://github.com/Gilnaa/memoffset) |
| `metal` | `0.31.0` | `MIT OR Apache-2.0` | [https://github.com/gfx-rs/metal-rs](https://github.com/gfx-rs/metal-rs) |
| `mime` | `0.3.18` | `MIT OR Apache-2.0` | [https://github.com/hyperium/mime](https://github.com/hyperium/mime) |
| `mime_guess2` | `2.3.1` | `MIT` | [https://github.com/ttys3/mime_guess2](https://github.com/ttys3/mime_guess2) |
| `minisign` | `0.10.0` | `MIT` | [https://github.com/jedisct1/rust-minisign](https://github.com/jedisct1/rust-minisign) |
| `minisign-verify` | `0.3.0` | `MIT` | [https://github.com/jedisct1/rust-minisign-verify](https://github.com/jedisct1/rust-minisign-verify) |
| `miniz_oxide` | `0.8.9` | `MIT OR Zlib OR Apache-2.0` | [https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide](https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide) |
| `miniz_oxide` | `0.9.1` | `MIT OR Zlib OR Apache-2.0` | [https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide](https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide) |
| `mio` | `1.2.4` | `MIT` | [https://github.com/tokio-rs/mio](https://github.com/tokio-rs/mio) |
| `moxcms` | `0.8.1` | `BSD-3-Clause OR Apache-2.0` | [https://github.com/awxkee/moxcms.git](https://github.com/awxkee/moxcms.git) |
| `naga` | `24.0.0` | `MIT OR Apache-2.0` | [https://github.com/gfx-rs/wgpu/tree/trunk/naga](https://github.com/gfx-rs/wgpu/tree/trunk/naga) |
| `nanorand` | `0.7.0` | `Zlib` | [https://github.com/Absolucy/nanorand-rs](https://github.com/Absolucy/nanorand-rs) |
| `ndk` | `0.9.0` | `MIT OR Apache-2.0` | [https://github.com/rust-mobile/ndk](https://github.com/rust-mobile/ndk) |
| `ndk-context` | `0.1.1` | `MIT OR Apache-2.0` | [https://github.com/rust-windowing/android-ndk-rs](https://github.com/rust-windowing/android-ndk-rs) |
| `ndk-sys` | `0.5.0+25.2.9519653` | `MIT OR Apache-2.0` | [https://github.com/rust-mobile/ndk](https://github.com/rust-mobile/ndk) |
| `ndk-sys` | `0.6.0+11769913` | `MIT OR Apache-2.0` | [https://github.com/rust-mobile/ndk](https://github.com/rust-mobile/ndk) |
| `nohash-hasher` | `0.2.0` | `Apache-2.0 OR MIT` | [https://github.com/paritytech/nohash-hasher](https://github.com/paritytech/nohash-hasher) |
| `ntapi` | `0.4.3` | `Apache-2.0 OR MIT` | [https://github.com/MSxDOS/ntapi](https://github.com/MSxDOS/ntapi) |
| `nu-ansi-term` | `0.50.3` | `MIT` | [https://github.com/nushell/nu-ansi-term](https://github.com/nushell/nu-ansi-term) |
| `num-traits` | `0.2.19` | `MIT OR Apache-2.0` | [https://github.com/rust-num/num-traits](https://github.com/rust-num/num-traits) |
| `num_enum` | `0.7.6` | `BSD-3-Clause OR MIT OR Apache-2.0` | [https://github.com/illicitonion/num_enum](https://github.com/illicitonion/num_enum) |
| `num_enum_derive` | `0.7.6` | `BSD-3-Clause OR MIT OR Apache-2.0` | [https://github.com/illicitonion/num_enum](https://github.com/illicitonion/num_enum) |
| `objc` | `0.2.7` | `MIT` | [http://github.com/SSheldon/rust-objc](http://github.com/SSheldon/rust-objc) |
| `objc-sys` | `0.3.5` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2` | `0.5.3` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2` | `0.6.5` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-app-kit` | `0.2.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-app-kit` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-cloud-kit` | `0.2.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-contacts` | `0.2.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-core-data` | `0.2.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-core-foundation` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-core-graphics` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-core-image` | `0.2.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-core-location` | `0.2.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-encode` | `4.1.0` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-foundation` | `0.2.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-foundation` | `0.3.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-io-surface` | `0.3.2` | `Zlib OR Apache-2.0 OR MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-link-presentation` | `0.2.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-metal` | `0.2.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-quartz-core` | `0.2.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-symbols` | `0.2.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-ui-kit` | `0.2.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-uniform-type-identifiers` | `0.2.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `objc2-user-notifications` | `0.2.2` | `MIT` | [https://github.com/madsmtm/objc2](https://github.com/madsmtm/objc2) |
| `once_cell` | `1.21.4` | `MIT OR Apache-2.0` | [https://github.com/matklad/once_cell](https://github.com/matklad/once_cell) |
| `once_cell_polyfill` | `1.70.2` | `MIT OR Apache-2.0` | [https://github.com/polyfill-rs/once_cell_polyfill](https://github.com/polyfill-rs/once_cell_polyfill) |
| `oorandom` | `11.1.5` | `MIT` | [https://hg.sr.ht/~icefox/oorandom](https://hg.sr.ht/~icefox/oorandom) |
| `option-ext` | `0.2.0` | `MPL-2.0` | [https://github.com/soc/option-ext.git](https://github.com/soc/option-ext.git) |
| `orbclient` | `0.3.55` | `MIT` | [https://gitlab.redox-os.org/redox-os/orbclient](https://gitlab.redox-os.org/redox-os/orbclient) |
| `ordered-float` | `4.6.0` | `MIT` | [https://github.com/reem/rust-ordered-float](https://github.com/reem/rust-ordered-float) |
| `owned_ttf_parser` | `0.25.1` | `Apache-2.0` | [https://github.com/alexheretic/owned-ttf-parser](https://github.com/alexheretic/owned-ttf-parser) |
| `parking_lot` | `0.12.5` | `MIT OR Apache-2.0` | [https://github.com/Amanieu/parking_lot](https://github.com/Amanieu/parking_lot) |
| `parking_lot_core` | `0.9.12` | `MIT OR Apache-2.0` | [https://github.com/Amanieu/parking_lot](https://github.com/Amanieu/parking_lot) |
| `paste` | `1.0.15` | `MIT OR Apache-2.0` | [https://github.com/dtolnay/paste](https://github.com/dtolnay/paste) |
| `pbkdf2` | `0.12.2` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/password-hashes/tree/master/pbkdf2](https://github.com/RustCrypto/password-hashes/tree/master/pbkdf2) |
| `percent-encoding` | `2.3.2` | `MIT OR Apache-2.0` | [https://github.com/servo/rust-url/](https://github.com/servo/rust-url/) |
| `phf` | `0.11.3` | `MIT` | [https://github.com/rust-phf/rust-phf](https://github.com/rust-phf/rust-phf) |
| `phf_generator` | `0.11.3` | `MIT` | [https://github.com/rust-phf/rust-phf](https://github.com/rust-phf/rust-phf) |
| `phf_macros` | `0.11.3` | `MIT` | [https://github.com/rust-phf/rust-phf](https://github.com/rust-phf/rust-phf) |
| `phf_shared` | `0.11.3` | `MIT` | [https://github.com/rust-phf/rust-phf](https://github.com/rust-phf/rust-phf) |
| `pico-args` | `0.5.0` | `MIT` | [https://github.com/RazrFalcon/pico-args](https://github.com/RazrFalcon/pico-args) |
| `pin-project` | `1.1.13` | `Apache-2.0 OR MIT` | [https://github.com/taiki-e/pin-project](https://github.com/taiki-e/pin-project) |
| `pin-project-internal` | `1.1.13` | `Apache-2.0 OR MIT` | [https://github.com/taiki-e/pin-project](https://github.com/taiki-e/pin-project) |
| `pin-project-lite` | `0.2.17` | `Apache-2.0 OR MIT` | [https://github.com/taiki-e/pin-project-lite](https://github.com/taiki-e/pin-project-lite) |
| `pkg-config` | `0.3.34` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/pkg-config-rs](https://github.com/rust-lang/pkg-config-rs) |
| `plain` | `0.2.3` | `MIT/Apache-2.0` | [https://github.com/randomites/plain](https://github.com/randomites/plain) |
| `plotters` | `0.3.7` | `MIT` | [https://github.com/plotters-rs/plotters](https://github.com/plotters-rs/plotters) |
| `plotters-backend` | `0.3.7` | `MIT` | [https://github.com/plotters-rs/plotters](https://github.com/plotters-rs/plotters) |
| `plotters-svg` | `0.3.7` | `MIT` | [https://github.com/plotters-rs/plotters.git](https://github.com/plotters-rs/plotters.git) |
| `png` | `0.17.16` | `MIT OR Apache-2.0` | [https://github.com/image-rs/image-png](https://github.com/image-rs/image-png) |
| `png` | `0.18.1` | `MIT OR Apache-2.0` | [https://github.com/image-rs/image-png](https://github.com/image-rs/image-png) |
| `polling` | `3.11.0` | `Apache-2.0 OR MIT` | [https://github.com/smol-rs/polling](https://github.com/smol-rs/polling) |
| `pollster` | `0.4.0` | `Apache-2.0/MIT` | [https://github.com/zesterer/pollster](https://github.com/zesterer/pollster) |
| `potential_utf` | `0.1.6` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `ppv-lite86` | `0.2.21` | `MIT OR Apache-2.0` | [https://github.com/cryptocorrosion/cryptocorrosion](https://github.com/cryptocorrosion/cryptocorrosion) |
| `presser` | `0.3.1` | `MIT OR Apache-2.0` | [https://github.com/EmbarkStudios/presser](https://github.com/EmbarkStudios/presser) |
| `proc-macro-crate` | `3.5.0` | `MIT OR Apache-2.0` | [https://github.com/bkchr/proc-macro-crate](https://github.com/bkchr/proc-macro-crate) |
| `proc-macro2` | `1.0.107` | `MIT OR Apache-2.0` | [https://github.com/dtolnay/proc-macro2](https://github.com/dtolnay/proc-macro2) |
| `profiling` | `1.0.18` | `MIT OR Apache-2.0` | [https://github.com/aclysma/profiling](https://github.com/aclysma/profiling) |
| `pxfm` | `0.1.30` | `BSD-3-Clause OR Apache-2.0` | [https://github.com/awxkee/pxfm](https://github.com/awxkee/pxfm) |
| `quick-error` | `2.0.1` | `MIT/Apache-2.0` | [http://github.com/tailhook/quick-error](http://github.com/tailhook/quick-error) |
| `quick-xml` | `0.41.0` | `MIT` | [https://github.com/tafia/quick-xml](https://github.com/tafia/quick-xml) |
| `quinn` | `0.11.12` | `MIT OR Apache-2.0` | [https://github.com/quinn-rs/quinn](https://github.com/quinn-rs/quinn) |
| `quinn-proto` | `0.11.19` | `MIT OR Apache-2.0` | [https://github.com/quinn-rs/quinn](https://github.com/quinn-rs/quinn) |
| `quinn-udp` | `0.5.16` | `MIT OR Apache-2.0` | [https://github.com/quinn-rs/quinn](https://github.com/quinn-rs/quinn) |
| `quote` | `1.0.47` | `MIT OR Apache-2.0` | [https://github.com/dtolnay/quote](https://github.com/dtolnay/quote) |
| `r-efi` | `5.3.0` | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | [https://github.com/r-efi/r-efi](https://github.com/r-efi/r-efi) |
| `r-efi` | `6.0.0` | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | [https://github.com/r-efi/r-efi](https://github.com/r-efi/r-efi) |
| `rand` | `0.8.8` | `MIT OR Apache-2.0` | [https://github.com/rust-random/rand](https://github.com/rust-random/rand) |
| `rand` | `0.10.3` | `MIT OR Apache-2.0` | [https://github.com/rust-random/rand](https://github.com/rust-random/rand) |
| `rand_chacha` | `0.3.1` | `MIT OR Apache-2.0` | [https://github.com/rust-random/rand](https://github.com/rust-random/rand) |
| `rand_core` | `0.6.4` | `MIT OR Apache-2.0` | [https://github.com/rust-random/rand](https://github.com/rust-random/rand) |
| `rand_core` | `0.10.1` | `MIT OR Apache-2.0` | [https://github.com/rust-random/rand_core](https://github.com/rust-random/rand_core) |
| `rand_pcg` | `0.10.2` | `MIT OR Apache-2.0` | [https://github.com/rust-random/rngs](https://github.com/rust-random/rngs) |
| `range-alloc` | `0.1.5` | `MIT OR Apache-2.0` | [https://github.com/gfx-rs/range-alloc](https://github.com/gfx-rs/range-alloc) |
| `raw-window-handle` | `0.6.2` | `MIT OR Apache-2.0 OR Zlib` | [https://github.com/rust-windowing/raw-window-handle](https://github.com/rust-windowing/raw-window-handle) |
| `rayon` | `1.12.0` | `MIT OR Apache-2.0` | [https://github.com/rayon-rs/rayon](https://github.com/rayon-rs/rayon) |
| `rayon-core` | `1.13.0` | `MIT OR Apache-2.0` | [https://github.com/rayon-rs/rayon](https://github.com/rayon-rs/rayon) |
| `rctree` | `0.5.0` | `MIT` | [https://github.com/RazrFalcon/rctree](https://github.com/RazrFalcon/rctree) |
| `redox_syscall` | `0.4.1` | `MIT` | [https://gitlab.redox-os.org/redox-os/syscall](https://gitlab.redox-os.org/redox-os/syscall) |
| `redox_syscall` | `0.5.18` | `MIT` | [https://gitlab.redox-os.org/redox-os/syscall](https://gitlab.redox-os.org/redox-os/syscall) |
| `redox_syscall` | `0.9.5` | `MIT` | [https://gitlab.redox-os.org/redox-os/kernel](https://gitlab.redox-os.org/redox-os/kernel) |
| `redox_users` | `0.5.3` | `MIT` | [https://gitlab.redox-os.org/redox-os/users](https://gitlab.redox-os.org/redox-os/users) |
| `regex` | `1.13.1` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/regex](https://github.com/rust-lang/regex) |
| `regex-automata` | `0.4.18` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/regex](https://github.com/rust-lang/regex) |
| `regex-syntax` | `0.8.11` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/regex](https://github.com/rust-lang/regex) |
| `renderdoc-sys` | `1.1.0` | `MIT OR Apache-2.0` | [https://github.com/ebkalderon/renderdoc-rs](https://github.com/ebkalderon/renderdoc-rs) |
| `reqwest` | `0.12.28` | `MIT OR Apache-2.0` | [https://github.com/seanmonstar/reqwest](https://github.com/seanmonstar/reqwest) |
| `resvg` | `0.37.0` | `MPL-2.0` | [https://github.com/RazrFalcon/resvg](https://github.com/RazrFalcon/resvg) |
| `rgb` | `0.8.53` | `MIT` | [https://github.com/kornelski/rust-rgb](https://github.com/kornelski/rust-rgb) |
| `ring` | `0.17.14` | `Apache-2.0 AND ISC` | [https://github.com/briansmith/ring](https://github.com/briansmith/ring) |
| `roxmltree` | `0.19.0` | `MIT OR Apache-2.0` | [https://github.com/RazrFalcon/roxmltree](https://github.com/RazrFalcon/roxmltree) |
| `rpassword` | `7.5.4` | `Apache-2.0` | [https://github.com/conradkleinespel/rpassword](https://github.com/conradkleinespel/rpassword) |
| `rtoolbox` | `0.0.6` | `Apache-2.0` | [https://github.com/conradkleinespel/rtoolbox](https://github.com/conradkleinespel/rtoolbox) |
| `rustc-hash` | `1.1.0` | `Apache-2.0/MIT` | [https://github.com/rust-lang-nursery/rustc-hash](https://github.com/rust-lang-nursery/rustc-hash) |
| `rustc-hash` | `2.1.3` | `Apache-2.0 OR MIT` | [https://github.com/rust-lang/rustc-hash](https://github.com/rust-lang/rustc-hash) |
| `rustc_version` | `0.4.1` | `MIT OR Apache-2.0` | [https://github.com/djc/rustc-version-rs](https://github.com/djc/rustc-version-rs) |
| `rustix` | `0.38.44` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | [https://github.com/bytecodealliance/rustix](https://github.com/bytecodealliance/rustix) |
| `rustix` | `1.1.6` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | [https://github.com/bytecodealliance/rustix](https://github.com/bytecodealliance/rustix) |
| `rustls` | `0.23.46` | `Apache-2.0 OR ISC OR MIT` | [https://github.com/rustls/rustls](https://github.com/rustls/rustls) |
| `rustls-pki-types` | `1.15.1` | `MIT OR Apache-2.0` | [https://github.com/rustls/pki-types](https://github.com/rustls/pki-types) |
| `rustls-webpki` | `0.103.16` | `ISC` | [https://github.com/rustls/webpki](https://github.com/rustls/webpki) |
| `rustversion` | `1.0.23` | `MIT OR Apache-2.0` | [https://github.com/dtolnay/rustversion](https://github.com/dtolnay/rustversion) |
| `ryu` | `1.0.23` | `Apache-2.0 OR BSL-1.0` | [https://github.com/dtolnay/ryu](https://github.com/dtolnay/ryu) |
| `salsa20` | `0.10.2` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/stream-ciphers](https://github.com/RustCrypto/stream-ciphers) |
| `same-file` | `1.0.6` | `Unlicense/MIT` | [https://github.com/BurntSushi/same-file](https://github.com/BurntSushi/same-file) |
| `scoped-tls` | `1.0.1` | `MIT/Apache-2.0` | [https://github.com/alexcrichton/scoped-tls](https://github.com/alexcrichton/scoped-tls) |
| `scopeguard` | `1.2.0` | `MIT OR Apache-2.0` | [https://github.com/bluss/scopeguard](https://github.com/bluss/scopeguard) |
| `scrypt` | `0.11.0` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/password-hashes/tree/master/scrypt](https://github.com/RustCrypto/password-hashes/tree/master/scrypt) |
| `semver` | `1.0.28` | `MIT OR Apache-2.0` | [https://github.com/dtolnay/semver](https://github.com/dtolnay/semver) |
| `serde` | `1.0.229` | `MIT OR Apache-2.0` | [https://github.com/serde-rs/serde](https://github.com/serde-rs/serde) |
| `serde_core` | `1.0.229` | `MIT OR Apache-2.0` | [https://github.com/serde-rs/serde](https://github.com/serde-rs/serde) |
| `serde_derive` | `1.0.229` | `MIT OR Apache-2.0` | [https://github.com/serde-rs/serde](https://github.com/serde-rs/serde) |
| `serde_json` | `1.0.154` | `MIT OR Apache-2.0` | [https://github.com/serde-rs/json](https://github.com/serde-rs/json) |
| `serde_path_to_error` | `0.1.20` | `MIT OR Apache-2.0` | [https://github.com/dtolnay/path-to-error](https://github.com/dtolnay/path-to-error) |
| `serde_spanned` | `0.6.9` | `MIT OR Apache-2.0` | [https://github.com/toml-rs/toml](https://github.com/toml-rs/toml) |
| `serde_urlencoded` | `0.7.1` | `MIT/Apache-2.0` | [https://github.com/nox/serde_urlencoded](https://github.com/nox/serde_urlencoded) |
| `sha1` | `0.10.7` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/hashes](https://github.com/RustCrypto/hashes) |
| `sha2` | `0.10.9` | `MIT OR Apache-2.0` | [https://github.com/RustCrypto/hashes](https://github.com/RustCrypto/hashes) |
| `sharded-slab` | `0.1.7` | `MIT` | [https://github.com/hawkw/sharded-slab](https://github.com/hawkw/sharded-slab) |
| `shlex` | `2.0.1` | `MIT OR Apache-2.0` | [https://github.com/comex/rust-shlex](https://github.com/comex/rust-shlex) |
| `signal-hook-registry` | `1.4.8` | `MIT OR Apache-2.0` | [https://github.com/vorner/signal-hook](https://github.com/vorner/signal-hook) |
| `simd-adler32` | `0.3.10` | `MIT` | [https://github.com/mcountryman/simd-adler32](https://github.com/mcountryman/simd-adler32) |
| `simd_cesu8` | `1.2.0` | `Apache-2.0 OR MIT` | [https://github.com/seancroach/simd_cesu8](https://github.com/seancroach/simd_cesu8) |
| `simdutf8` | `0.1.5` | `MIT OR Apache-2.0` | [https://github.com/rusticstuff/simdutf8](https://github.com/rusticstuff/simdutf8) |
| `simplecss` | `0.2.2` | `Apache-2.0 OR MIT` | [https://github.com/linebender/simplecss](https://github.com/linebender/simplecss) |
| `siphasher` | `0.3.11` | `MIT/Apache-2.0` | [https://github.com/jedisct1/rust-siphash](https://github.com/jedisct1/rust-siphash) |
| `siphasher` | `1.0.4` | `MIT OR Apache-2.0` | [https://github.com/jedisct1/rust-siphash](https://github.com/jedisct1/rust-siphash) |
| `slab` | `0.4.12` | `MIT` | [https://github.com/tokio-rs/slab](https://github.com/tokio-rs/slab) |
| `slotmap` | `1.1.1` | `Zlib` | [https://github.com/orlp/slotmap](https://github.com/orlp/slotmap) |
| `smallvec` | `1.16.4` | `MIT OR Apache-2.0` | [https://github.com/servo/rust-smallvec](https://github.com/servo/rust-smallvec) |
| `smithay-client-toolkit` | `0.19.2` | `MIT` | [https://github.com/smithay/client-toolkit](https://github.com/smithay/client-toolkit) |
| `smithay-client-toolkit` | `0.20.0` | `MIT` | [https://github.com/smithay/client-toolkit](https://github.com/smithay/client-toolkit) |
| `smithay-clipboard` | `0.7.3` | `MIT` | [https://github.com/smithay/smithay-clipboard](https://github.com/smithay/smithay-clipboard) |
| `smol_str` | `0.2.2` | `MIT OR Apache-2.0` | [https://github.com/rust-analyzer/smol_str](https://github.com/rust-analyzer/smol_str) |
| `socket2` | `0.6.5` | `MIT OR Apache-2.0` | [https://github.com/rust-lang/socket2](https://github.com/rust-lang/socket2) |
| `spin` | `0.9.9` | `MIT` | [https://github.com/mvdnes/spin-rs.git](https://github.com/mvdnes/spin-rs.git) |
| `spirv` | `0.3.0+sdk-1.3.268.0` | `Apache-2.0` | [https://github.com/gfx-rs/rspirv](https://github.com/gfx-rs/rspirv) |
| `stable_deref_trait` | `1.2.1` | `MIT OR Apache-2.0` | [https://github.com/storyyeller/stable_deref_trait](https://github.com/storyyeller/stable_deref_trait) |
| `static_assertions` | `1.1.0` | `MIT OR Apache-2.0` | [https://github.com/nvzqz/static-assertions-rs](https://github.com/nvzqz/static-assertions-rs) |
| `strict-num` | `0.1.1` | `MIT` | [https://github.com/RazrFalcon/strict-num](https://github.com/RazrFalcon/strict-num) |
| `strsim` | `0.11.1` | `MIT` | [https://github.com/rapidfuzz/strsim-rs](https://github.com/rapidfuzz/strsim-rs) |
| `strum` | `0.26.3` | `MIT` | [https://github.com/Peternator7/strum](https://github.com/Peternator7/strum) |
| `strum_macros` | `0.26.4` | `MIT` | [https://github.com/Peternator7/strum](https://github.com/Peternator7/strum) |
| `subtle` | `2.6.1` | `BSD-3-Clause` | [https://github.com/dalek-cryptography/subtle](https://github.com/dalek-cryptography/subtle) |
| `svgtypes` | `0.13.0` | `MIT/Apache-2.0` | [https://github.com/RazrFalcon/svgtypes](https://github.com/RazrFalcon/svgtypes) |
| `syn` | `2.0.119` | `MIT OR Apache-2.0` | [https://github.com/dtolnay/syn](https://github.com/dtolnay/syn) |
| `syn` | `3.0.9` | `MIT OR Apache-2.0` | [https://github.com/dtolnay/syn](https://github.com/dtolnay/syn) |
| `sync_wrapper` | `1.0.2` | `Apache-2.0` | [https://github.com/Actyx/sync_wrapper](https://github.com/Actyx/sync_wrapper) |
| `synstructure` | `0.14.0` | `MIT` | [https://github.com/mystor/synstructure](https://github.com/mystor/synstructure) |
| `sysinfo` | `0.33.1` | `MIT` | [https://github.com/GuillaumeGomez/sysinfo](https://github.com/GuillaumeGomez/sysinfo) |
| `tar` | `0.4.46` | `MIT OR Apache-2.0` | [https://github.com/composefs/tar-rs](https://github.com/composefs/tar-rs) |
| `tempfile` | `3.27.0` | `MIT OR Apache-2.0` | [https://github.com/Stebalien/tempfile](https://github.com/Stebalien/tempfile) |
| `termcolor` | `1.4.1` | `Unlicense OR MIT` | [https://github.com/BurntSushi/termcolor](https://github.com/BurntSushi/termcolor) |
| `thiserror` | `1.0.69` | `MIT OR Apache-2.0` | [https://github.com/dtolnay/thiserror](https://github.com/dtolnay/thiserror) |
| `thiserror` | `2.0.21` | `MIT OR Apache-2.0` | [https://github.com/dtolnay/thiserror](https://github.com/dtolnay/thiserror) |
| `thiserror-impl` | `1.0.69` | `MIT OR Apache-2.0` | [https://github.com/dtolnay/thiserror](https://github.com/dtolnay/thiserror) |
| `thiserror-impl` | `2.0.21` | `MIT OR Apache-2.0` | [https://github.com/dtolnay/thiserror](https://github.com/dtolnay/thiserror) |
| `thread_local` | `1.1.10` | `MIT OR Apache-2.0` | [https://github.com/Amanieu/thread_local-rs](https://github.com/Amanieu/thread_local-rs) |
| `tiff` | `0.11.3` | `MIT` | [https://github.com/image-rs/image-tiff](https://github.com/image-rs/image-tiff) |
| `tiny-skia` | `0.11.4` | `BSD-3-Clause` | [https://github.com/RazrFalcon/tiny-skia](https://github.com/RazrFalcon/tiny-skia) |
| `tiny-skia-path` | `0.11.4` | `BSD-3-Clause` | [https://github.com/RazrFalcon/tiny-skia/tree/master/path](https://github.com/RazrFalcon/tiny-skia/tree/master/path) |
| `tinystr` | `0.8.5` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `tinytemplate` | `1.2.1` | `Apache-2.0 OR MIT` | [https://github.com/bheisler/TinyTemplate](https://github.com/bheisler/TinyTemplate) |
| `tinyvec` | `1.13.3` | `Zlib OR Apache-2.0 OR MIT` | [https://github.com/Lokathor/tinyvec](https://github.com/Lokathor/tinyvec) |
| `tokio` | `1.54.1` | `MIT` | [https://github.com/tokio-rs/tokio](https://github.com/tokio-rs/tokio) |
| `tokio-macros` | `2.7.2` | `MIT` | [https://github.com/tokio-rs/tokio](https://github.com/tokio-rs/tokio) |
| `tokio-rustls` | `0.26.6` | `MIT OR Apache-2.0` | [https://github.com/rustls/tokio-rustls](https://github.com/rustls/tokio-rustls) |
| `tokio-util` | `0.7.20` | `MIT` | [https://github.com/tokio-rs/tokio](https://github.com/tokio-rs/tokio) |
| `toml` | `0.8.23` | `MIT OR Apache-2.0` | [https://github.com/toml-rs/toml](https://github.com/toml-rs/toml) |
| `toml_datetime` | `0.6.11` | `MIT OR Apache-2.0` | [https://github.com/toml-rs/toml](https://github.com/toml-rs/toml) |
| `toml_datetime` | `1.1.3+spec-1.1.0` | `MIT OR Apache-2.0` | [https://github.com/toml-rs/toml](https://github.com/toml-rs/toml) |
| `toml_edit` | `0.22.27` | `MIT OR Apache-2.0` | [https://github.com/toml-rs/toml](https://github.com/toml-rs/toml) |
| `toml_edit` | `0.25.19+spec-1.1.0` | `MIT OR Apache-2.0` | [https://github.com/toml-rs/toml](https://github.com/toml-rs/toml) |
| `toml_parser` | `1.1.5+spec-1.1.0` | `MIT OR Apache-2.0` | [https://github.com/toml-rs/toml](https://github.com/toml-rs/toml) |
| `toml_write` | `0.1.2` | `MIT OR Apache-2.0` | [https://github.com/toml-rs/toml](https://github.com/toml-rs/toml) |
| `tower` | `0.5.3` | `MIT` | [https://github.com/tower-rs/tower](https://github.com/tower-rs/tower) |
| `tower-http` | `0.6.11` | `MIT` | [https://github.com/tower-rs/tower-http](https://github.com/tower-rs/tower-http) |
| `tower-layer` | `0.3.3` | `MIT` | [https://github.com/tower-rs/tower](https://github.com/tower-rs/tower) |
| `tower-service` | `0.3.3` | `MIT` | [https://github.com/tower-rs/tower](https://github.com/tower-rs/tower) |
| `tracing` | `0.1.44` | `MIT` | [https://github.com/tokio-rs/tracing](https://github.com/tokio-rs/tracing) |
| `tracing-attributes` | `0.1.31` | `MIT` | [https://github.com/tokio-rs/tracing](https://github.com/tokio-rs/tracing) |
| `tracing-core` | `0.1.36` | `MIT` | [https://github.com/tokio-rs/tracing](https://github.com/tokio-rs/tracing) |
| `tracing-log` | `0.2.0` | `MIT` | [https://github.com/tokio-rs/tracing](https://github.com/tokio-rs/tracing) |
| `tracing-subscriber` | `0.3.23` | `MIT` | [https://github.com/tokio-rs/tracing](https://github.com/tokio-rs/tracing) |
| `try-lock` | `0.2.5` | `MIT` | [https://github.com/seanmonstar/try-lock](https://github.com/seanmonstar/try-lock) |
| `ttf-parser` | `0.25.1` | `MIT OR Apache-2.0` | [https://github.com/harfbuzz/ttf-parser](https://github.com/harfbuzz/ttf-parser) |
| `type-map` | `0.5.1` | `MIT/Apache-2.0` | [https://github.com/kardeiz/type-map](https://github.com/kardeiz/type-map) |
| `typenum` | `1.20.1` | `MIT OR Apache-2.0` | [https://github.com/paholg/typenum](https://github.com/paholg/typenum) |
| `unicase` | `2.10.0` | `MIT OR Apache-2.0` | [https://github.com/seanmonstar/unicase](https://github.com/seanmonstar/unicase) |
| `unicode-ident` | `1.0.27` | `(MIT OR Apache-2.0) AND Unicode-3.0` | [https://github.com/dtolnay/unicode-ident](https://github.com/dtolnay/unicode-ident) |
| `unicode-segmentation` | `1.13.3` | `MIT OR Apache-2.0` | [https://github.com/unicode-rs/unicode-segmentation](https://github.com/unicode-rs/unicode-segmentation) |
| `unicode-width` | `0.1.14` | `MIT OR Apache-2.0` | [https://github.com/unicode-rs/unicode-width](https://github.com/unicode-rs/unicode-width) |
| `unicode-xid` | `0.2.6` | `MIT OR Apache-2.0` | [https://github.com/unicode-rs/unicode-xid](https://github.com/unicode-rs/unicode-xid) |
| `untrusted` | `0.9.0` | `ISC` | [https://github.com/briansmith/untrusted](https://github.com/briansmith/untrusted) |
| `ureq` | `2.12.1` | `MIT OR Apache-2.0` | [https://github.com/algesten/ureq](https://github.com/algesten/ureq) |
| `url` | `2.5.8` | `MIT OR Apache-2.0` | [https://github.com/servo/rust-url](https://github.com/servo/rust-url) |
| `usvg` | `0.37.0` | `MPL-2.0` | [https://github.com/RazrFalcon/resvg](https://github.com/RazrFalcon/resvg) |
| `usvg-parser` | `0.37.0` | `MPL-2.0` | [https://github.com/RazrFalcon/resvg](https://github.com/RazrFalcon/resvg) |
| `usvg-tree` | `0.37.0` | `MPL-2.0` | [https://github.com/RazrFalcon/resvg](https://github.com/RazrFalcon/resvg) |
| `utf8_iter` | `1.1.0` | `Apache-2.0 OR MIT` | [https://github.com/hsivonen/utf8_iter](https://github.com/hsivonen/utf8_iter) |
| `utf8parse` | `0.2.2` | `Apache-2.0 OR MIT` | [https://github.com/alacritty/vte](https://github.com/alacritty/vte) |
| `utf_types` | `1.0.0` | `Apache-2.0 OR MIT` | [https://github.com/hsivonen/utf_types](https://github.com/hsivonen/utf_types) |
| `uuid` | `1.29.0` | `Apache-2.0 OR MIT` | [https://github.com/uuid-rs/uuid](https://github.com/uuid-rs/uuid) |
| `valuable` | `0.1.1` | `MIT` | [https://github.com/tokio-rs/valuable](https://github.com/tokio-rs/valuable) |
| `version_check` | `0.9.5` | `MIT/Apache-2.0` | [https://github.com/SergioBenitez/version_check](https://github.com/SergioBenitez/version_check) |
| `walkdir` | `2.5.0` | `Unlicense/MIT` | [https://github.com/BurntSushi/walkdir](https://github.com/BurntSushi/walkdir) |
| `want` | `0.3.2` | `MIT` | [https://github.com/seanmonstar/want](https://github.com/seanmonstar/want) |
| `wasi` | `0.11.1+wasi-snapshot-preview1` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | [https://github.com/bytecodealliance/wasi](https://github.com/bytecodealliance/wasi) |
| `wasip2` | `1.0.4+wasi-0.2.12` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | [https://github.com/bytecodealliance/wasi-rs](https://github.com/bytecodealliance/wasi-rs) |
| `wasm-bindgen` | `0.2.130` | `MIT OR Apache-2.0` | [https://github.com/wasm-bindgen/wasm-bindgen](https://github.com/wasm-bindgen/wasm-bindgen) |
| `wasm-bindgen-futures` | `0.4.80` | `MIT OR Apache-2.0` | [https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/futures](https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/futures) |
| `wasm-bindgen-macro` | `0.2.130` | `MIT OR Apache-2.0` | [https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro](https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro) |
| `wasm-bindgen-macro-support` | `0.2.130` | `MIT OR Apache-2.0` | [https://github.com/wasm-bindgen/wasm-bindgen/tree/main/crates/macro-support](https://github.com/wasm-bindgen/wasm-bindgen/tree/main/crates/macro-support) |
| `wasm-bindgen-shared` | `0.2.130` | `MIT OR Apache-2.0` | [https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared](https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared) |
| `wasm-streams` | `0.4.2` | `MIT OR Apache-2.0` | [https://github.com/MattiasBuelens/wasm-streams/](https://github.com/MattiasBuelens/wasm-streams/) |
| `wayland-backend` | `0.3.17` | `MIT` | [https://github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| `wayland-client` | `0.31.15` | `MIT` | [https://github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| `wayland-csd-frame` | `0.3.0` | `MIT` | [https://github.com/rust-windowing/wayland-csd-frame](https://github.com/rust-windowing/wayland-csd-frame) |
| `wayland-cursor` | `0.31.14` | `MIT` | [https://github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| `wayland-protocols` | `0.32.13` | `MIT` | [https://github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| `wayland-protocols-experimental` | `20250721.0.1` | `MIT` | [https://github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| `wayland-protocols-misc` | `0.3.12` | `MIT` | [https://github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| `wayland-protocols-plasma` | `0.3.12` | `MIT` | [https://github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| `wayland-protocols-wlr` | `0.3.12` | `MIT` | [https://github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| `wayland-scanner` | `0.31.11` | `MIT` | [https://github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| `wayland-sys` | `0.31.11` | `MIT` | [https://github.com/smithay/wayland-rs](https://github.com/smithay/wayland-rs) |
| `web-sys` | `0.3.107` | `MIT OR Apache-2.0` | [https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys](https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys) |
| `web-time` | `1.1.0` | `MIT OR Apache-2.0` | [https://github.com/daxpedda/web-time](https://github.com/daxpedda/web-time) |
| `webbrowser` | `1.2.6` | `MIT OR Apache-2.0` | [https://github.com/amodm/webbrowser-rs](https://github.com/amodm/webbrowser-rs) |
| `webpki-roots` | `0.26.11` | `CDLA-Permissive-2.0` | [https://github.com/rustls/webpki-roots](https://github.com/rustls/webpki-roots) |
| `webpki-roots` | `1.0.11` | `CDLA-Permissive-2.0` | [https://github.com/rustls/webpki-roots](https://github.com/rustls/webpki-roots) |
| `weezl` | `0.1.12` | `MIT OR Apache-2.0` | [https://github.com/image-rs/weezl](https://github.com/image-rs/weezl) |
| `wgpu` | `24.0.5` | `MIT OR Apache-2.0` | [https://github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) |
| `wgpu-core` | `24.0.5` | `MIT OR Apache-2.0` | [https://github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) |
| `wgpu-hal` | `24.0.4` | `MIT OR Apache-2.0` | [https://github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) |
| `wgpu-types` | `24.0.0` | `MIT OR Apache-2.0` | [https://github.com/gfx-rs/wgpu](https://github.com/gfx-rs/wgpu) |
| `winapi` | `0.3.9` | `MIT/Apache-2.0` | [https://github.com/retep998/winapi-rs](https://github.com/retep998/winapi-rs) |
| `winapi-i686-pc-windows-gnu` | `0.4.0` | `MIT/Apache-2.0` | [https://github.com/retep998/winapi-rs](https://github.com/retep998/winapi-rs) |
| `winapi-util` | `0.1.11` | `Unlicense OR MIT` | [https://github.com/BurntSushi/winapi-util](https://github.com/BurntSushi/winapi-util) |
| `winapi-x86_64-pc-windows-gnu` | `0.4.0` | `MIT/Apache-2.0` | [https://github.com/retep998/winapi-rs](https://github.com/retep998/winapi-rs) |
| `windows` | `0.57.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows` | `0.58.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-core` | `0.57.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-core` | `0.58.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-core` | `0.62.2` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-core` | `0.100.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-implement` | `0.57.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-implement` | `0.58.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-implement` | `0.60.2` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-implement` | `0.100.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-interface` | `0.57.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-interface` | `0.58.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-interface` | `0.59.3` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-interface` | `0.100.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-link` | `0.2.1` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-link` | `0.100.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-result` | `0.1.2` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-result` | `0.2.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-result` | `0.4.1` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-result` | `0.100.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-strings` | `0.1.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-strings` | `0.5.1` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-strings` | `0.100.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-sys` | `0.52.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-sys` | `0.59.0` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-sys` | `0.61.2` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows-targets` | `0.52.6` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows_aarch64_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows_aarch64_msvc` | `0.52.6` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows_i686_gnu` | `0.52.6` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows_i686_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows_i686_msvc` | `0.52.6` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows_x86_64_gnu` | `0.52.6` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows_x86_64_gnullvm` | `0.52.6` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `windows_x86_64_msvc` | `0.52.6` | `MIT OR Apache-2.0` | [https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs) |
| `winit` | `0.30.13` | `Apache-2.0` | [https://github.com/rust-windowing/winit](https://github.com/rust-windowing/winit) |
| `winnow` | `0.7.15` | `MIT` | [https://github.com/winnow-rs/winnow](https://github.com/winnow-rs/winnow) |
| `winnow` | `1.0.5` | `MIT` | [https://github.com/winnow-rs/winnow](https://github.com/winnow-rs/winnow) |
| `wit-bindgen` | `0.57.1` | `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | [https://github.com/bytecodealliance/wit-bindgen](https://github.com/bytecodealliance/wit-bindgen) |
| `writeable` | `0.6.4` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `x11-dl` | `2.21.0` | `MIT` | [https://github.com/AltF02/x11-rs.git](https://github.com/AltF02/x11-rs.git) |
| `x11rb` | `0.13.2` | `MIT OR Apache-2.0` | [https://github.com/psychon/x11rb](https://github.com/psychon/x11rb) |
| `x11rb-protocol` | `0.13.2` | `MIT OR Apache-2.0` | [https://github.com/psychon/x11rb](https://github.com/psychon/x11rb) |
| `xattr` | `1.7.0` | `MIT OR Apache-2.0` | [https://github.com/Stebalien/xattr](https://github.com/Stebalien/xattr) |
| `xcursor` | `0.3.11` | `MIT` | [https://github.com/esposm03/xcursor-rs](https://github.com/esposm03/xcursor-rs) |
| `xkbcommon-dl` | `0.4.2` | `MIT` | [https://github.com/rust-windowing/xkbcommon-dl](https://github.com/rust-windowing/xkbcommon-dl) |
| `xkeysym` | `0.2.1` | `MIT OR Apache-2.0 OR Zlib` | [https://github.com/notgull/xkeysym](https://github.com/notgull/xkeysym) |
| `xml-rs` | `0.8.29` | `MIT` | [https://github.com/kornelski/xml-rs](https://github.com/kornelski/xml-rs) |
| `xmlwriter` | `0.1.0` | `MIT` | [https://github.com/RazrFalcon/xmlwriter](https://github.com/RazrFalcon/xmlwriter) |
| `yoke` | `0.8.4` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `yoke-derive` | `0.8.4` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `zerocopy` | `0.8.63` | `BSD-2-Clause OR Apache-2.0 OR MIT` | [https://github.com/google/zerocopy](https://github.com/google/zerocopy) |
| `zerocopy-derive` | `0.8.63` | `BSD-2-Clause OR Apache-2.0 OR MIT` | [https://github.com/google/zerocopy](https://github.com/google/zerocopy) |
| `zerofrom` | `0.1.8` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `zerofrom-derive` | `0.1.8` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `zeroize` | `1.9.1` | `Apache-2.0 OR MIT` | [https://github.com/RustCrypto/utils](https://github.com/RustCrypto/utils) |
| `zerotrie` | `0.2.5` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `zerovec` | `0.11.9` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `zerovec-derive` | `0.11.7` | `Unicode-3.0` | [https://github.com/unicode-org/icu4x](https://github.com/unicode-org/icu4x) |
| `zip` | `2.6.1` | `MIT` | [https://github.com/zip-rs/zip2.git](https://github.com/zip-rs/zip2.git) |
| `zlib-rs` | `0.6.8` | `Zlib` | [https://github.com/trifectatechfoundation/zlib-rs](https://github.com/trifectatechfoundation/zlib-rs) |
| `zmij` | `1.0.23` | `MIT` | [https://github.com/dtolnay/zmij](https://github.com/dtolnay/zmij) |
| `zopfli` | `0.8.4` | `Apache-2.0` | [https://github.com/zopfli-rs/zopfli](https://github.com/zopfli-rs/zopfli) |
| `zune-core` | `0.5.3` | `MIT OR Apache-2.0 OR Zlib` | [https://github.com/etemesi254/zune-image](https://github.com/etemesi254/zune-image) |
| `zune-jpeg` | `0.5.15` | `MIT OR Apache-2.0 OR Zlib` | [https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg](https://github.com/etemesi254/zune-image/tree/dev/crates/zune-jpeg) |
