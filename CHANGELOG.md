# Changelog

All notable changes to this project are documented in this file.

this project follows [Semantic Versioning](https://semver.org/).

## [Unreleased]

Maintained fork of [euclio/spellbound](https://github.com/euclio/spellbound).

### Breaking
- `Checker::new()` now returns `Result<Self, Error>` instead of panicking when
  the spellchecker (or hunspell dictionary) is unavailable
- `SpellingError` now includes UTF-8 byte `start` / `end` offsets into the
  checked text (in addition to `text()`)

### Added
- `Error` / `Error::Unavailable`
- Broader Linux dictionary search paths and `en_US` / `en_GB` fallbacks
- Word tokenization with offsets on Unix (alphabetic / `'` runs)
- GitHub Actions CI (Linux, macOS, Windows; fmt; clippy)
- README documentation for the maintained fork and Linux hunspell setup

### Changed
- Edition 2021; dropped `lazy_static` / `extern crate`
- macOS uses `OnceLock` for the shared `NSSpellChecker`
- Windows COM failures map to `Error` where creating the checker; UTF-16
  indices convert to UTF-8 byte ranges

### Removed
- Travis CI and APPVeyor configs

## [0.1.1] - 2020-02-05

Last release from upstream (`euclio/spellbound`).